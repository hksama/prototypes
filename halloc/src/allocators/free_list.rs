use std::alloc::Layout;
use std::sync::{atomic::{AtomicUsize, Ordering}, Arc, Mutex};

use crate::{bootstrap_memory, AllocatorStrategy};

/// Defines how a suitable freed block is selected.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FreeListPolicy {
    /// - `BestFit` selects the smallest suitable block.
    BestFit,
    /// - `WorstFit` selects the largest suitable block.
    WorstFit,
    /// - `FirstFit` selects the first suitable block in free-list order.
    FirstFit,
}

#[derive(Clone, Copy, Debug)]
struct FreeBlock {
    // UNSAFE and TODO: Check this cuz wont this overflow easily?
    ptr: *mut u8,
    size: usize,
}

pub struct FreeListAllocator {
    /// Start of the memory region reserved for this allocator.
    start: Arc<usize>,
    /// Next unallocated byte in that region.
    curr: Arc<AtomicUsize>,
    /// Total usable capacity of the region.
    capacity: usize,
    policy: FreeListPolicy,
    // A mutex keeps block selection and removal atomic.
    free_list: Arc<Mutex<Vec<FreeBlock>>>,
}

pub const MEM_EXTEND_SIZE: usize = 1024 * 1024; // 1 MiB

impl FreeListAllocator {
    /// Creates a best-fit allocator with `capacity` usable bytes.
    pub fn new(capacity: usize) -> Option<Self> {
        Self::with_policy(capacity, FreeListPolicy::BestFit)
    }

    /// Creates an allocator using the supplied free-list selection policy.
    fn with_policy(capacity: usize, policy: FreeListPolicy) -> Option<Self> {
        // Map at least one MiB so a zero-capacity allocator can still be
        // constructed, while `capacity` remains the enforced usable limit.
        let mapped_size = capacity.max(MEM_EXTEND_SIZE);
        let bootstrap_ptr = unsafe { bootstrap_memory(mapped_size).ok()? } as usize;

        Some(Self {
            start: Arc::new(bootstrap_ptr),
            curr: Arc::new(AtomicUsize::new(bootstrap_ptr)),
            capacity,
            policy,
            free_list: Arc::new(Mutex::new(Vec::new())),
        })
    }

    #[inline(always)]
    fn align_up(address: usize, alignment: usize) -> Option<usize> {
        debug_assert!(alignment.is_power_of_two());
        // TODO: Check for how this is handled and write tests
        // Whats the point of this?
        address.checked_add(alignment - 1).map(|value| value & !(alignment - 1))
    }

    /// Finds, removes, and splits a suitable free block according to `policy`.
    fn search_free_list(&self, layout: Layout) -> Option<*mut u8> {
        let mut free_list = self.free_list.lock().unwrap();
        let size = layout.size();
        let alignment = layout.align();

        // TODO: Match policy to prevent branches down the line. This is a hot path.

        let mut selected: Option<(usize, usize, usize)> = None;
        for (index, block) in free_list.iter().enumerate() {
            let aligned = Self::align_up(block.ptr as usize, alignment)?;
            let padding = aligned.checked_sub(block.ptr as usize)?;
            let required = padding.checked_add(size)?;
            if required > block.size {
                continue;
            }

            selected = match selected {
                // TODO: Move this outside and remove match as the None happens only once.
                None => Some((index, aligned, padding)),
                Some((selected_index, selected_aligned, selected_padding)) => {
                    let selected_size = free_list[selected_index].size;
                    let choose_current = match self.policy {
                        // TODO: This is incorrect; Best-Fit should check with global minimum, which is not defined.
                        FreeListPolicy::BestFit => block.size < selected_size,
                        // TODO: This is incorrect; Worst-Fit should check with global maximum, which is not defined.
                        FreeListPolicy::WorstFit => block.size > selected_size,
                        FreeListPolicy::FirstFit => block.size < selected_size, // This is a no-op; First-Fit will break after the first match.
                    };
                    if choose_current {
                        Some((index, aligned, padding))
                    } else {
                        Some((selected_index, selected_aligned, selected_padding))
                    }
                }
            };

            if matches!(self.policy, FreeListPolicy::FirstFit) {
                break;
            }
        }

        let (index, aligned, padding) = selected?;
        let block = free_list.swap_remove(index);

        // Keep unallocated portions available for future requests.
        if padding != 0 {
            free_list.push(FreeBlock {
                ptr: block.ptr,
                size: padding,
            });
        }
        let consumed = padding + size;
        let remaining = block.size - consumed;
        if remaining != 0 {
            free_list.push(FreeBlock {
                ptr: (aligned + size) as *mut u8,
                size: remaining,
            });
        }

        Some(aligned as *mut u8)
    }

    /// Reserves fresh space, respecting layout alignment and allocator capacity.
    #[inline(always)]
    fn allocate_fresh(&self, layout: Layout) -> *mut u8 {
        let size = layout.size();
        let region_end = match (*self.start).checked_add(self.capacity) {
            Some(end) => end,
            None => return std::ptr::null_mut(),
        };

        loop {
            let current = self.curr.load(Ordering::Relaxed);
            let aligned = match Self::align_up(current, layout.align()) {
                Some(value) => value,
                None => return std::ptr::null_mut(),
            };
            let next = match aligned.checked_add(size) {
                Some(value) if value <= region_end => value,
                _ => return std::ptr::null_mut(),
            };

            if self.curr.compare_exchange_weak(
                current,
                next,
                Ordering::Relaxed,
                Ordering::Relaxed,
            ).is_ok() {
                return aligned as *mut u8;
            }
        }
    }
}

impl AllocatorStrategy for FreeListAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        if layout.size() == 0 {
            return std::ptr::null_mut();
        }

        self.search_free_list(layout)
        // What if it crashes here? 
            .unwrap_or_else(|| self.allocate_fresh(layout))
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        if !ptr.is_null() && layout.size() != 0 {
            self.free_list.lock().unwrap().push(FreeBlock {
                ptr,
                size: layout.size(),
            });
        }
    }
}

#[cfg(test)]
mod freelist_allocator_tests {
    use super::*;

    fn allocation_succeeds(size: usize) {
        let allocator = FreeListAllocator::new(size).expect("allocator should initialize");
        let layout = Layout::from_size_align(size, 8).unwrap();

        unsafe {
            let ptr = allocator.alloc(layout);
            assert!(!ptr.is_null());
            assert_eq!((ptr as usize) % layout.align(), 0);
            allocator.dealloc(ptr, layout);
        }
    }

    #[test]
    fn alloc_1_byte() {
        allocation_succeeds(1);
    }

    #[test]
    fn alloc_8_bytes() {
        allocation_succeeds(8);
    }

    #[test]
    fn alloc_16_bytes() {
        allocation_succeeds(16);
    }

    #[test]
    fn alloc_1_mib() {
        allocation_succeeds(MEM_EXTEND_SIZE);
    }
}
