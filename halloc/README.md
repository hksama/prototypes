# halloc

A Rust memory allocator library built to study allocation strategies suited to **Redis-like workloads** — frequent small, short-lived allocations with mixed lifetimes and reuse patterns.

> **Status:** This repository is under **active development**. APIs, internals, and test coverage are evolving.

## Motivation

General-purpose allocators optimize for broad workloads. In-memory data stores such as Redis allocate heavily in predictable size bands (keys, values, internal structures) and benefit from strategies tuned to those access patterns. `halloc` is a learning project that implements and compares allocator designs in that context.

## Current Progress

### Core infrastructure

- **`AllocatorStrategy` trait** — common interface for pluggable allocation backends (`alloc` / `dealloc`).
- **`Halloc` + `GlobalAlloc`** — a global allocator wrapper with **size-class statistics** (16 logarithmically spaced classes from 8 B to 2 KiB, plus a large-allocation bucket). Allocation currently delegates to the system allocator while instrumentation is exercised.
- **`bootstrap_memory`** — reserves backing storage via `mmap` for custom allocator regions.
- **Safety posture** — `#![forbid(unsafe_op_in_unsafe_fn)]` to keep `unsafe` boundaries explicit.

### Implemented strategies

| Strategy | Description | Status |
|---|---|---|
| **Bump** | Sequential bump pointer over an `mmap`-backed region; lock-free cursor via atomics. Supports top-of-stack deallocation only. | Working prototype |
| **Free-list** | Explicit free-list reuse with **Best-Fit**, **Worst-Fit**, and **First-Fit** policies; alignment-aware splitting of free blocks. | Working prototype with unit tests |

### Testing & benchmarking

- Unit tests for size-class indexing, bump pointer arithmetic, `mmap` bootstrap, and free-list allocations (1 B through 1 MiB).
- A shared allocator test suite is scaffolded (alignment, fragmentation, double-free, stress, random alloc/free) — stubs in place, not yet filled in.
- Criterion benchmark harness started for free-list vs. system `malloc` comparison.

## Planned Features

- **Additional allocator backends** — arena, sized-class, slab, and thread-cache (`tc`) strategies (module stubs declared, not yet implemented).
- **End-to-end `GlobalAlloc` integration** — route `Halloc` allocations through the custom strategies instead of the system allocator.
- **`Send` / `Sync` support** — enable safe multi-threaded use with per-strategy synchronization where needed.
- **Hardening** — correct best/worst-fit selection, memory region growth on capacity exhaustion, coalescing of adjacent free blocks.
- **Expanded validation** — fragmentation and edge-case tests, concurrent stress tests, and allocator-vs-`malloc` benchmarks.

## Project Layout

```
src/
├── lib.rs              # GlobalAlloc wrapper, size classes, stats, mmap bootstrap
└── allocators/
    ├── mod.rs          # Strategy module registry
    ├── bump.rs         # Bump allocator
    └── free_list.rs    # Free-list allocator
benches/
└── free_list.rs        # Performance comparison (in progress)
```

## Building

```bash
cargo build
cargo test
cargo bench   # requires Criterion (dev-dependency)
```

## License

MIT
