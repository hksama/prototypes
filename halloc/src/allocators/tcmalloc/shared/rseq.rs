//! Restartable Sequences (rseq) — a Linux per-thread critical-section ABI.
//!
//! `SizedTcMallocAllocator::allocate_fresh` bumps a shared cursor with a CAS retry
//! loop. Under contention that loop spins. **rseq** lets a thread run a tiny
//! read-modify-write on *its current CPU* without atomics; if the kernel
//! preempts or migrates the thread mid-sequence, userspace restarts.
//!
//! This module is a minimal, educational x86_64 implementation with annotated
//! inline assembly. Production tcmalloc shards state per CPU; the stress tests
//! here follow that model.
//!
//! ## glibc interaction
//!
//! glibc ≥ 2.35 registers its own `struct rseq` for each pthread. The kernel
//! allows **one** registration per thread, so a second `rseq(2)` from this crate
//! fails with `EINVAL` unless glibc's registration is disabled at process start:
//!
//! ```text
//! GLIBC_TUNABLES=glibc.pthread.rseq=0
//! ```
//!
//! Tests set this via `.cargo/config.toml`. For binaries embedding this allocator,
//! export the same variable before launch (or use `LD_SHOW_AUXV` / `ld.so` tunables
//! in the wrapper script).

use std::cell::UnsafeCell;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

// ---------------------------------------------------------------------------
// Kernel ABI (linux/include/uapi/linux/rseq.h)
// ---------------------------------------------------------------------------

/// Magic passed to the `rseq(2)` syscall on registration.
const RSEQ_SIG: u32 = 0x5305_3053;

/// ELF aux-vector keys published by the kernel (`linux/auxvec.h`).
#[cfg(target_os = "linux")]
const AT_RSEQ_FEATURE_SIZE: libc::c_ulong = 27;
#[cfg(target_os = "linux")]
const AT_RSEQ_ALIGN: libc::c_ulong = 28;

/// `cpu_id_start` / `cpu_id` value before the kernel publishes a CPU index.
const RSEQ_CPU_ID_UNINITIALIZED: u32 = u32::MAX;

/// Why `rseq(2)` registration failed on this host.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RseqError {
    /// Not a Linux target, or this architecture has no `SYS_rseq` constant.
    Unsupported,
    /// Kernel did not publish rseq aux-vector entries (older kernel or disabled).
    NotAdvertisedByKernel,
    /// Thread-local `struct rseq` is not aligned as required by `AT_RSEQ_ALIGN`.
    MisalignedThreadStorage { addr: usize, required_align: usize },
    /// Syscall returned an error; see `errno` (e.g. 22 = EINVAL, 38 = ENOSYS).
    ///
    /// `EINVAL` on Linux+glibc often means glibc already registered rseq — set
    /// `GLIBC_TUNABLES=glibc.pthread.rseq=0` before process start.
    SyscallFailed { errno: i32 },
}

/// 32-byte critical-section descriptor registered with the kernel.
#[repr(C, align(32))]
struct RseqCs {
    version: u32,
    flags: u32,
    /// Virtual address of the first instruction *after* the 4-byte signature.
    start_ip: u64,
    /// Byte offset from `start_ip` to the first instruction past the commit point.
    post_commit_offset: u64,
    /// Virtual address of the abort handler (executed after a restartable abort).
    abort_ip: u64,
}

/// Per-thread block registered via `rseq(2)`.
///
/// Layout matches `linux/rseq.h` (including `mm_cid` as 32-bit).
#[repr(C, align(32))]
struct RseqAbi {
    cpu_id_start: u32,
    cpu_id: u32,
    rseq_cs: u64,
    flags: u32,
    node_id: u32,
    mm_cid: u32,
}

const _: () = assert!(std::mem::size_of::<RseqAbi>() == 32);
const _: () = assert!(std::mem::align_of::<RseqAbi>() == 32);

/// Wrapper so the thread-local slot inherits 32-byte alignment explicitly.
#[repr(C, align(32))]
struct RseqTlsSlot(UnsafeCell<RseqAbi>);

impl RseqAbi {
    const fn new() -> Self {
        Self {
            cpu_id_start: RSEQ_CPU_ID_UNINITIALIZED,
            cpu_id: RSEQ_CPU_ID_UNINITIALIZED,
            rseq_cs: 0,
            flags: 0,
            node_id: 0,
            mm_cid: 0,
        }
    }
}

thread_local! {
    static THREAD_RSEQ: RseqTlsSlot = const {
        RseqTlsSlot(UnsafeCell::new(RseqAbi::new()))
    };
    static THREAD_REGISTERED: AtomicBool = AtomicBool::new(false);
}

/// Upper bound on Linux CPU indices we shard test state across.
pub const MAX_CPUS: usize = 1024;

// ---------------------------------------------------------------------------
// Registration
// ---------------------------------------------------------------------------

/// Linux syscall number for `rseq(2)` on this architecture, if known and available on current architecture.
///
/// Uses `libc::SYS_rseq` from the platform headers
#[cfg(target_os = "linux")]
pub fn rseq_syscall_number() -> Option<libc::c_long> {
    #[cfg(any(
        target_arch = "x86_64",
        target_arch = "x86",
        target_arch = "aarch64",
        target_arch = "arm",
        target_arch = "riscv64",
        target_arch = "powerpc64",
        target_arch = "s390x"
    ))]
    {
        return Some(libc::SYS_rseq);
    }
    #[cfg(not(any(
        target_arch = "x86_64",
        target_arch = "x86",
        target_arch = "aarch64",
        target_arch = "arm",
        target_arch = "riscv64",
        target_arch = "powerpc64",
        target_arch = "s390x"
    )))]
    {
        None
    }
}

#[cfg(not(target_os = "linux"))]
pub fn rseq_syscall_number() -> Option<libc::c_long> {
    None
}

/// Kernel-published rseq registration parameters from the ELF aux vector.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RseqAuxv {
    pub feature_size: usize,
    pub align: usize,
}

/// Reads `AT_RSEQ_FEATURE_SIZE` and `AT_RSEQ_ALIGN` via `getauxval(3)`.
#[cfg(target_os = "linux")]
pub fn rseq_auxv() -> Option<RseqAuxv> {
    let feature_size = unsafe { libc::getauxval(AT_RSEQ_FEATURE_SIZE) };
    let align = unsafe { libc::getauxval(AT_RSEQ_ALIGN) };
    if feature_size == 0 || align == 0 {
        return None;
    }
    Some(RseqAuxv {
        feature_size: feature_size as usize,
        align: align as usize,
    })
}

#[cfg(not(target_os = "linux"))]
pub fn rseq_auxv() -> Option<RseqAuxv> {
    None
}

#[cfg(target_os = "linux")]
fn rseq_registration_len(auxv: RseqAuxv) -> usize {
    // Kernel requires `rseq_len >= offsetof(struct rseq, end)` (28 on current ABI).
    // Aux "feature size" can lag; never register with less than our struct size.
    std::cmp::max(std::mem::size_of::<RseqAbi>(), auxv.feature_size)
}

/// Returns `true` when `rseq(2)` registration succeeds on this host.
pub fn rseq_available() -> bool {
    register_current_thread().is_ok()
}

/// Detailed registration error for diagnostics (errno, alignment, auxv, etc.).
pub fn rseq_registration_error() -> Option<RseqError> {
    register_current_thread().err()
}

fn register_current_thread() -> Result<(), RseqError> {
    if THREAD_REGISTERED.with(|r| r.load(Ordering::Acquire)) {
        return Ok(());
    }

    #[cfg(not(target_os = "linux"))]
    {
        return Err(RseqError::Unsupported);
    }

    #[cfg(target_os = "linux")]
    {
        let Some(syscall_nr) = rseq_syscall_number() else {
            return Err(RseqError::Unsupported);
        };
        let Some(auxv) = rseq_auxv() else {
            return Err(RseqError::NotAdvertisedByKernel);
        };
        let reg_len = rseq_registration_len(auxv);

        THREAD_RSEQ.with(|tls| {
            let abi_ptr = tls.0.get().cast::<RseqAbi>();
            let addr = abi_ptr as usize;
            if addr % auxv.align != 0 {
                return Err(RseqError::MisalignedThreadStorage {
                    addr,
                    required_align: auxv.align,
                });
            }

            unsafe {
                *libc::__errno_location() = 0;
                let rc = libc::syscall(
                    syscall_nr,
                    abi_ptr,
                    reg_len,
                    0,
                    RSEQ_SIG as libc::c_long,
                );
                if rc == 0 {
                    Ok(())
                } else {
                    Err(RseqError::SyscallFailed {
                        errno: *libc::__errno_location(),
                    })
                }
            }
        })?;

        THREAD_REGISTERED.with(|r| r.store(true, Ordering::Release));
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Restartable fetch-add (per-CPU shard)
// ---------------------------------------------------------------------------

/// Adds `delta` to `shards[cpu]` inside an rseq critical section.
///
/// Each CPU index has its own shard, so the critical section never shares a
/// cache line with another CPU's hot path — the same pattern tcmalloc uses
/// before flushing to a central heap.
///
/// Returns the shard value **before** the add, or `None` if rseq is unavailable
/// or the current CPU index is out of range.
pub fn restartable_fetch_add(shards: &[AtomicUsize], delta: usize) -> Option<usize> {
    register_current_thread().ok()?;

    THREAD_RSEQ.with(|tls| unsafe {
        let abi = &mut *tls.0.get();
        let cpu = abi.cpu_id;
        if cpu as usize >= shards.len() {
            return None;
        }

        let shard_ptr = shards[cpu as usize].as_ptr();

        // Descriptor lives on the stack so its addresses are paired with the asm
        // block below (llvm gives each `asm!` expansion its own local labels).
        let mut cs = RseqCs {
            version: 0,
            flags: 0,
            start_ip: 0,
            post_commit_offset: 0,
            abort_ip: 0,
        };

        let mut old_value = 0usize;
        let mut aborted = 0u32;

        loop {
            abi.cpu_id_start = cpu;
            abi.rseq_cs = &raw mut cs as u64;
            std::sync::atomic::fence(Ordering::SeqCst);

            aborted = 0;
            old_value = 0;

            // Annotated restartable sequence (x86_64).
            //
            // Layout:
            //   [patch RseqCs from local labels]
            //   [.long signature]
            //   START: CPU check + body
            //   COMMIT:
            //   ABORT:
            std::arch::asm!(
                // ---- patch RseqCs for this asm expansion ----------------
                "mov {cs}, %r8",
                "lea 20f(%rip), %rax",
                "mov %rax, 8(%r8)",           // start_ip
                "lea 30f(%rip), %rax",
                "lea 20f(%rip), %rcx",
                "sub %rcx, %rax",
                "mov %rax, 16(%r8)",          // post_commit_offset
                "lea 50f(%rip), %rax",
                "mov %rax, 24(%r8)",          // abort_ip
                // Fall-through would execute the signature bytes as code (0x53
                // is `push %rbx`); jump over them into the critical section.
                "jmp 20f",

                // ---- rseq signature (must sit immediately before start_ip) -
                ".byte 0x53, 0x05, 0x30, 0x53",
                "20:", // FETCH_ADD_START

                // ---- CPU identity check -----------------------------------
                // Load the per-thread rseq ABI block. If `cpu_id != cpu_id_start`
                // the thread was migrated; jump to the abort path. The kernel
                // may also force this jump after preemption.
                "mov {abi}, %rax",
                "mov 4(%rax), %ecx",          // abi->cpu_id      (kernel-maintained)
                "mov (%rax), %edx",           // abi->cpu_id_start (we set above)
                "cmp %ecx, %edx",
                "jne 50f",                    // -> FETCH_ADD_ABORT

                // ---- critical section body (no atomics) -------------------
                // This CPU owns `shards[cpu]`; plain load/add/store is safe
                // until we leave the critical section.
                "mov {shard}, %rax",          // rax = &shards[cpu]
                "mov (%rax), %rcx",           // rcx = old value
                "mov %rcx, {old}",            // export old value to Rust
                "add {delta}, %rcx",          // rcx = new value
                "mov %rcx, (%rax)",           // store (per-CPU shard)

                "30:", // FETCH_ADD_COMMIT
                // First instruction past the commit point. A completed sequence
                // must reach here without aborting.
                "jmp 60f",

                // Signature before abort_ip (same layout rule as start_ip).
                ".byte 0x53, 0x05, 0x30, 0x53",
                "50:", // FETCH_ADD_ABORT (abort_ip)
                "movl $1, {aborted:e}",
                "60:",

                abi = in(reg) abi,
                shard = in(reg) shard_ptr,
                delta = in(reg) delta,
                cs = in(reg) &raw mut cs,
                old = out(reg) old_value,
                aborted = out(reg) aborted,
                out("rax") _,
                out("rcx") _,
                out("rdx") _,
                options(nostack, att_syntax),
            );

            if aborted == 0 {
                return Some(old_value);
            }
            abi.cpu_id_start = abi.cpu_id;
        }
    })
}

// ---------------------------------------------------------------------------
// Restartable bump reserve (per-CPU cursor, mirrors allocate_fresh)
// ---------------------------------------------------------------------------

/// Rounds `address` up to `alignment` (power of two). Same helper logic as
/// `SizedTcMallocAllocator::align_up`.
#[inline(always)]
fn align_up(address: usize, alignment: usize) -> Option<usize> {
    debug_assert!(alignment.is_power_of_two());
    address
        .checked_add(alignment - 1)
        .map(|value| value & !(alignment - 1))
}

/// Per-CPU bump reserve inside an rseq critical section.
///
/// `cursors[cpu]` is advanced by `size` bytes after aligning, matching the
/// CAS loop in `allocate_fresh` but without atomic retries on the fast path.
///
/// Returns the aligned start offset on success.
pub fn restartable_bump_reserve(
    cursors: &[AtomicUsize],
    size: usize,
    align: usize,
    limit: usize,
) -> Option<usize> {
    if size == 0 || !align.is_power_of_two() {
        return None;
    }
    register_current_thread().ok()?;

    THREAD_RSEQ.with(|tls| unsafe {
        let abi = &mut *tls.0.get();
        let cpu = abi.cpu_id;
        if cpu as usize >= cursors.len() {
            return None;
        }

        let cursor_ptr = cursors[cpu as usize].as_ptr();
        let align_m1 = align - 1;
        let align_mask = !(align - 1);

        let mut cs = RseqCs {
            version: 0,
            flags: 0,
            start_ip: 0,
            post_commit_offset: 0,
            abort_ip: 0,
        };

        let mut result = 0usize;
        let mut aborted = 0u32;

        loop {
            abi.cpu_id_start = cpu;
            abi.rseq_cs = &raw mut cs as u64;
            std::sync::atomic::fence(Ordering::SeqCst);

            aborted = 0;
            result = 0;

            std::arch::asm!(
                "mov {cs}, %r8",
                "lea 20f(%rip), %rax",
                "mov %rax, 8(%r8)",
                "lea 30f(%rip), %rax",
                "lea 20f(%rip), %rcx",
                "sub %rcx, %rax",
                "mov %rax, 16(%r8)",
                "lea 50f(%rip), %rax",
                "mov %rax, 24(%r8)",
                "jmp 20f",

                ".byte 0x53, 0x05, 0x30, 0x53",
                "20:", // BUMP_START

                "mov {abi}, %rax",
                "mov 4(%rax), %ecx",
                "mov (%rax), %edx",
                "cmp %ecx, %edx",
                "jne 50f",

                // Mirrors allocate_fresh: load cursor, align, bounds-check, store.
                "mov {cursor}, %rax",
                "mov (%rax), %rdi",           // rdi = current
                "add {align_m1}, %rdi",
                "and {align_mask}, %rdi",     // rdi = aligned start
                "mov %rdi, {result}",
                "mov %rdi, %rcx",
                "add {size}, %rcx",           // rcx = next
                "cmp {limit}, %rcx",
                "ja 45f",
                "mov %rcx, (%rax)",           // publish next cursor
                "jmp 30f",
                "45:",
                "xor %rdi, %rdi",
                "mov %rdi, {result}",         // out-of-space (not an abort)
                "30:", // BUMP_COMMIT
                "jmp 60f",

                ".byte 0x53, 0x05, 0x30, 0x53",
                "50:", // BUMP_ABORT (abort_ip)
                "movl $1, {aborted:e}",
                "60:",

                abi = in(reg) abi,
                cursor = in(reg) cursor_ptr,
                size = in(reg) size,
                align_m1 = in(reg) align_m1,
                align_mask = in(reg) align_mask,
                limit = in(reg) limit,
                cs = in(reg) &raw mut cs,
                result = out(reg) result,
                aborted = out(reg) aborted,
                out("rax") _,
                out("rcx") _,
                out("rdi") _,
                out("rdx") _,
                options(nostack, att_syntax),
            );

            if aborted != 0 {
                continue;
            }
            return if result == 0 { None } else { Some(result) };
        }
    })
}

// ---------------------------------------------------------------------------
// Test helpers
// ---------------------------------------------------------------------------

/// Builds a per-CPU shard table for stress tests.
pub fn new_cpu_shards() -> Vec<AtomicUsize> {
    (0..MAX_CPUS).map(|_| AtomicUsize::new(0)).collect()
}

/// Sum of all per-CPU shards (for assertions).
pub fn sum_shards(shards: &[AtomicUsize]) -> usize {
    shards.iter().map(|s| s.load(Ordering::Relaxed)).sum()
}

// ---------------------------------------------------------------------------
// Multithreaded stress tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod rseq_tests {
    use super::*;
    use std::sync::Arc;
    use std::thread;
    use std::time::Duration;

    fn require_rseq() {
        if rseq_available() {
            return;
        }
        let mut msg = String::from("rseq(2) registration failed.\n");
        msg.push_str(&format!("  syscall nr: {:?}\n", rseq_syscall_number()));
        msg.push_str(&format!("  auxv: {:?}\n", rseq_auxv()));
        THREAD_RSEQ.with(|tls| {
            let addr = tls.0.get() as usize;
            msg.push_str(&format!(
                "  thread-local struct rseq @ 0x{addr:x} (mod {} = {})\n",
                addr % 32,
                addr % 32
            ));
        });
        if let Some(err) = rseq_registration_error() {
            msg.push_str(&format!("  error: {err:?}\n"));
        }
        msg.push_str(
            "If errno is 22 (EINVAL) on glibc Linux, glibc may already own rseq.\n\
             Set before process start: GLIBC_TUNABLES=glibc.pthread.rseq=0\n\
             (configured for `cargo test` via .cargo/config.toml).",
        );
        panic!("{msg}");
    }

    /// Prints everything needed to debug registration on your machine.
    #[test]
    fn diagnose_rseq_environment() {
        println!("rseq_syscall_number() = {:?}", rseq_syscall_number());
        println!("rseq_auxv() = {:?}", rseq_auxv());
        println!("sizeof(RseqAbi) = {}", std::mem::size_of::<RseqAbi>());
        println!("alignof(RseqAbi) = {}", std::mem::align_of::<RseqAbi>());
        THREAD_RSEQ.with(|tls| {
            let addr = tls.0.get() as usize;
            println!("thread-local RseqAbi @ 0x{addr:x}");
            println!("  addr % 32 = {}", addr % 32);
            println!("  addr % 28 = {}", addr % 28);
        });
        match rseq_registration_error() {
            None if rseq_available() => {
                THREAD_RSEQ.with(|tls| unsafe {
                    let abi = &*tls.0.get();
                    println!("registration OK, cpu_id = {}", abi.cpu_id);
                });
                let shards = new_cpu_shards();
                match restartable_fetch_add(&shards, 1) {
                    Some(v) => println!("restartable_fetch_add OK, old = {v}"),
                    None => println!("restartable_fetch_add returned None"),
                }
            }
            Some(err) => println!("registration error: {err:?}"),
            None => println!("registration error: unknown"),
        }
    }

    #[test]
    fn rseq_registers_on_current_thread() {
        require_rseq();
        THREAD_RSEQ.with(|tls| unsafe {
            let abi = &*tls.0.get();
            assert_ne!(abi.cpu_id, RSEQ_CPU_ID_UNINITIALIZED);
        });
    }

    #[test]
    fn restartable_fetch_add_single_thread() {
        require_rseq();
        let shards = new_cpu_shards();
        let cpu = THREAD_RSEQ.with(|tls| unsafe { (*tls.0.get()).cpu_id as usize });
        assert!(cpu < MAX_CPUS);

        let old = restartable_fetch_add(&shards, 10).expect("add should succeed");
        assert_eq!(old, 0);
        assert_eq!(shards[cpu].load(Ordering::Relaxed), 10);
    }

    #[test]
    fn restartable_fetch_add_multithreaded_no_lost_updates() {
        require_rseq();
        let shards = Arc::new(new_cpu_shards());
        const THREADS: usize = 16;
        const ITERS: usize = 50_000;

        thread::scope(|scope| {
            for _ in 0..THREADS {
                let shards = Arc::clone(&shards);
                scope.spawn(move || {
                    for _ in 0..ITERS {
                        restartable_fetch_add(&shards, 1).expect("each add should succeed");
                    }
                });
            }
        });

        assert_eq!(sum_shards(&shards), THREADS * ITERS);
    }

    #[test]
    fn restartable_fetch_add_stress_with_preemption_pressure() {
        require_rseq();
        let shards = Arc::new(new_cpu_shards());
        const THREADS: usize = 32;
        const ITERS: usize = 20_000;

        thread::scope(|scope| {
            for tid in 0..THREADS {
                let shards = Arc::clone(&shards);
                scope.spawn(move || {
                    for i in 0..ITERS {
                        if tid % 4 == 0 && i % 128 == 0 {
                            thread::yield_now();
                        }
                        if i % 512 == 0 {
                            thread::sleep(Duration::from_nanos(1));
                        }
                        restartable_fetch_add(&shards, 1).expect("add should succeed");
                    }
                });
            }
        });

        assert_eq!(sum_shards(&shards), THREADS * ITERS);
    }

    #[test]
    fn restartable_bump_reserve_single_thread() {
        require_rseq();
        let cursors = new_cpu_shards();
        let cpu = THREAD_RSEQ.with(|tls| unsafe { (*tls.0.get()).cpu_id as usize });
        let limit = 4096;

        let a = restartable_bump_reserve(&cursors, 64, 64, limit).expect("first reserve");
        let b = restartable_bump_reserve(&cursors, 64, 64, limit).expect("second reserve");
        assert_eq!(a, 0);
        assert_eq!(b, 64);
        assert_eq!(cursors[cpu].load(Ordering::Relaxed), 128);
    }

    #[test]
    fn restartable_bump_reserve_multithreaded_disjoint() {
        require_rseq();
        let cursors = Arc::new(new_cpu_shards());
        const THREADS: usize = 16;
        const ITERS: usize = 2_000;
        const SLOT: usize = 64;
        const LIMIT: usize = SLOT * ITERS + 1024;

        // Offsets are unique per CPU shard; tag each claim with the CPU index.
        let claimed = Arc::new(std::sync::Mutex::new(Vec::<(u32, usize)>::new()));

        thread::scope(|scope| {
            for _ in 0..THREADS {
                let cursors = Arc::clone(&cursors);
                let claimed = Arc::clone(&claimed);
                scope.spawn(move || {
                    for _ in 0..ITERS {
                        let offset = restartable_bump_reserve(&cursors, SLOT, SLOT, LIMIT)
                            .expect("reserve should succeed");
                        assert_eq!(offset % SLOT, 0);
                        let cpu = THREAD_RSEQ.with(|tls| unsafe { (*tls.0.get()).cpu_id });
                        {
                            let mut slots = claimed.lock().unwrap();
                            assert!(
                                !slots.contains(&(cpu, offset)),
                                "duplicate (cpu={cpu}, offset={offset}) — rseq bump broke exclusivity"
                            );
                            slots.push((cpu, offset));
                        }
                    }
                });
            }
        });

        assert_eq!(claimed.lock().unwrap().len(), THREADS * ITERS);
    }

    #[test]
    fn restartable_bump_reserve_stress_with_yield() {
        require_rseq();
        let cursors = Arc::new(new_cpu_shards());
        const THREADS: usize = 24;
        const ITERS: usize = 5_000;
        const SLOT: usize = 32;
        const LIMIT: usize = SLOT * ITERS + 4096;

        let total_reserved = Arc::new(AtomicUsize::new(0));

        thread::scope(|scope| {
            for t in 0..THREADS {
                let cursors = Arc::clone(&cursors);
                let total_reserved = Arc::clone(&total_reserved);
                scope.spawn(move || {
                    for i in 0..ITERS {
                        if t % 3 == 0 && i % 64 == 0 {
                            thread::yield_now();
                        }
                        let offset = restartable_bump_reserve(&cursors, SLOT, SLOT, LIMIT)
                            .expect("reserve should succeed");
                        assert_eq!(offset % SLOT, 0);
                        total_reserved.fetch_add(SLOT, Ordering::Relaxed);
                    }
                });
            }
        });

        assert_eq!(total_reserved.load(Ordering::Relaxed), THREADS * ITERS * SLOT);
        assert!(sum_shards(&cursors) <= THREADS * LIMIT);
    }

    #[test]
    fn align_up_matches_sized_tcmalloc() {
        assert_eq!(align_up(0, 64), Some(0));
        assert_eq!(align_up(1, 64), Some(64));
        assert_eq!(align_up(64, 64), Some(64));
        assert_eq!(align_up(65, 64), Some(128));
    }
}
