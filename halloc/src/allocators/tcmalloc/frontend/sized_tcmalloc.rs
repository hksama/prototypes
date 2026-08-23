use std::alloc::Layout;
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc, Mutex,
};

use crate::{bootstrap_memory, AllocatorStrategy};

/// Fixed small-object size classes (in bytes), each a power of two up to 1 KiB.
pub const SIZE_CLASSES: [usize; 11] = [1, 2, 4, 8, 16, 32, 64, 128, 256, 512, 1024];

const NUM_CLASSES: usize = SIZE_CLASSES.len();

pub const MEM_EXTEND_SIZE: usize = 1024 * 1024 * 2; // 2 MiB

/// Thread-safe allocator that routes requests into per-class free lists.
///
/// Each **size class** holds objects of one fixed size. A request is rounded up
/// to the smallest class that satisfies both `layout.size()` and `layout.align()`.
pub struct SizedTcMallocAllocator {
    /// Start of the memory region reserved for this allocator.
    start: Arc<usize>,
    /// Next unallocated byte in that region.
    curr: Arc<AtomicUsize>,
    /// Total usable capacity of the region.
    capacity: usize,
    /// One LIFO free list per size class; reused blocks are pushed here on `dealloc`.
    free_lists: [Arc<Mutex<Vec<*mut u8>>>; NUM_CLASSES],
}

impl SizedTcMallocAllocator {
    /// Creates a sized-class allocator with `capacity` usable bytes.
    pub fn new(capacity: usize) -> Option<Self> {
        let mapped_size = capacity.max(MEM_EXTEND_SIZE);
        let bootstrap_ptr = unsafe { bootstrap_memory(mapped_size).ok()? } as usize;

        Some(Self {
            start: Arc::new(bootstrap_ptr),
            curr: Arc::new(AtomicUsize::new(bootstrap_ptr)),
            capacity,
            free_lists: std::array::from_fn(|_| Arc::new(Mutex::new(Vec::new()))),
        })
    }

    /// Rounds `address` up to the next multiple of `alignment` (power of two).
    #[inline(always)]
    fn align_up(address: usize, alignment: usize) -> Option<usize> {
        debug_assert!(alignment.is_power_of_two());
        address
            .checked_add(alignment - 1)
            .map(|value| value & !(alignment - 1))
    }

    /// Maps a `Layout` to the smallest size class that fits size and alignment.
    ///
    /// Returns `(class_index, class_size)` or `None` when the request exceeds
    /// the largest supported class (1024 bytes).
    fn class_for_layout(layout: Layout) -> Option<(usize, usize)> {
        let size = layout.size();
        let align = layout.align();

        SIZE_CLASSES
            .iter()
            .enumerate()
            .find(|&(_, &class_size)| class_size >= size && class_size >= align)
            .map(|(index, &class_size)| (index, class_size))
    }
    /// Pops a reused block from the free list for `class_index`, if any.
    fn pop_free(&self, class_index: usize) -> Option<*mut u8> {
        self.free_lists[class_index].lock().unwrap().pop()
    }

    /// Reserves a fresh object of `class_size` bytes from the bump region.
    #[inline(always)]
    fn allocate_fresh(&self, class_size: usize, align: usize) -> *mut u8 {
        let alignment = class_size.max(align);
        let region_end = match (*self.start).checked_add(self.capacity) {
            Some(end) => end,
            None => return std::ptr::null_mut(),
        };

        loop {
            let current = self.curr.load(Ordering::Relaxed);
            let aligned = match Self::align_up(current, alignment) {
                Some(value) => value,
                None => return std::ptr::null_mut(),
            };
            let next = match aligned.checked_add(class_size) {
                Some(value) if value <= region_end => value,
                _ => return std::ptr::null_mut(),
            };

            if self
                .curr
                .compare_exchange_weak(current, next, Ordering::Relaxed, Ordering::Relaxed)
                .is_ok()
            {
                return aligned as *mut u8;
            }
        }
    }

    /// Pushes `ptr` onto the free list for the class derived from `layout`.
    fn push_free(&self, ptr: *mut u8, layout: Layout) {
        if let Some((class_index, _)) = Self::class_for_layout(layout) {
            self.free_lists[class_index].lock().unwrap().push(ptr);
        }
    }
}

impl AllocatorStrategy for SizedTcMallocAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        if layout.size() == 0 {
            return std::ptr::null_mut();
        }

        let Some((class_index, class_size)) = Self::class_for_layout(layout) else {
            return std::ptr::null_mut();
        };

        self.pop_free(class_index)
            .unwrap_or_else(|| self.allocate_fresh(class_size, layout.align()))
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        if !ptr.is_null() && layout.size() != 0 {
            self.push_free(ptr, layout);
        }
    }
}

#[cfg(test)]
mod sized_tcmalloc_allocator_tests {
    use super::*;

    fn allocation_succeeds(size: usize) {
        let allocator = SizedTcMallocAllocator::new(size * 4).expect("allocator should initialize");
        let layout = Layout::from_size_align(size, size.min(8).max(1)).unwrap();

        unsafe {
            let ptr = allocator.alloc(layout);
            assert!(!ptr.is_null(), "allocation of {size} bytes should succeed");
            assert_eq!((ptr as usize) % layout.align(), 0);
            allocator.dealloc(ptr, layout);
        }
    }

    #[test]
    fn alloc_1_byte() {
        allocation_succeeds(1);
    }

    #[test]
    fn alloc_2_bytes() {
        allocation_succeeds(2);
    }

    #[test]
    fn alloc_4_bytes() {
        allocation_succeeds(4);
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
    fn alloc_32_bytes() {
        allocation_succeeds(32);
    }

    #[test]
    fn alloc_64_bytes() {
        allocation_succeeds(64);
    }

    #[test]
    fn alloc_128_bytes() {
        allocation_succeeds(128);
    }

    #[test]
    fn alloc_256_bytes() {
        allocation_succeeds(256);
    }

    #[test]
    fn alloc_512_bytes() {
        allocation_succeeds(512);
    }

    #[test]
    fn alloc_1024_bytes() {
        allocation_succeeds(1024);
    }

    #[test]
    fn alloc_rounds_up_to_next_class() {
        let allocator = SizedTcMallocAllocator::new(4096).expect("allocator should initialize");
        // 3 bytes rounds up to the 4-byte class.
        let layout = Layout::from_size_align(3, 4).unwrap();

        unsafe {
            let ptr = allocator.alloc(layout);
            assert!(!ptr.is_null());
            assert_eq!((ptr as usize) % 4, 0);
            allocator.dealloc(ptr, layout);
        }
    }

    #[test]
    fn dealloc_returns_block_to_free_list() {
        let allocator = SizedTcMallocAllocator::new(4096).expect("allocator should initialize");
        let layout = Layout::from_size_align(16, 8).unwrap();

        unsafe {
            let first = allocator.alloc(layout);
            assert!(!first.is_null());
            allocator.dealloc(first, layout);

            let second = allocator.alloc(layout);
            assert_eq!(first, second, "freed block should be reused from its size class");
        }
    }

    #[test]
    fn alloc_larger_than_max_class_fails() {
        let allocator = SizedTcMallocAllocator::new(MEM_EXTEND_SIZE).expect("allocator should initialize");
        let layout = Layout::from_size_align(1025, 8).unwrap();

        unsafe {
            let ptr = allocator.alloc(layout);
            assert!(ptr.is_null(), "requests above 1024 bytes are not handled by sized classes");
        }
    }

    #[test]
    fn class_for_layout_maps_correctly() {
        assert_eq!(SizedTcMallocAllocator::class_for_layout(Layout::from_size_align(1, 1).unwrap()), Some((0, 1)));
        assert_eq!(SizedTcMallocAllocator::class_for_layout(Layout::from_size_align(3, 4).unwrap()), Some((2, 4)));
        assert_eq!(SizedTcMallocAllocator::class_for_layout(Layout::from_size_align(100, 64).unwrap()), Some((7, 128)));
        assert_eq!(SizedTcMallocAllocator::class_for_layout(Layout::from_size_align(1025, 8).unwrap()), None);
    }
}
