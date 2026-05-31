#![forbid(unsafe_op_in_unsafe_fn)]

use std::alloc::{GlobalAlloc, Layout, System};
use std::ops::Mul;
use std::sync::{Arc,atomic::{AtomicU64, Ordering, AtomicUsize}};

// Define size classes for small allocations. These are approximately logarithmically spaced.
const SIZE_CLASSES: [usize; 16] = [
    8, 16, 24, 32, 48, 64, 96, 128, 192, 256, 384, 512, 768, 1024, 1536, 2048,
];
const NUM_CLASSES: usize = SIZE_CLASSES.len();

/// Core allocator interface. Each strategy (Bump, Arena, FreeList) implements this.
pub trait CoreAllocator {
    /// Allocate memory according to layout.
    /// Returns null on failure.
    unsafe fn alloc(&self, layout: Layout) -> *mut u8;

    /// Deallocate memory.
    /// SAFETY: ptr must be from a prior alloc() call with the same layout.
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout);
}

pub struct BumpAllocator {
    /// Start of the allocated region
    start: *mut u8,
    /// Current position, Arc for thread-safe updates, AtomicUsize for lock-free increments
    next: Arc<AtomicUsize>,
}

impl BumpAllocator {
    /// Create a new bump allocator by allocating `capacity` bytes from the system.
    pub fn new(capacity: usize) -> Option<Self> {
        // For now, use System allocator to get the initial block
        let layout = Layout::from_size_align(capacity, 8).ok()?;
        let start = unsafe {
            std::alloc::alloc(layout) as *mut u8
        };
        if start.is_null() {
            return None;
        }

        Some(Self {
            start,
            next: Arc::new(AtomicUsize::new(0)),
        })
    }
} 


impl CoreAllocator for BumpAllocator{
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let size = layout.size();
        // let alignment = layout.align();
        let heap_start = self.start as usize;

        // Calculate the next aligned address
        let updated_next = self.next.fetch_add(size, Ordering::Relaxed);
        // (heap_start+updated_next) as *mut u8
        updated_next as *mut u8
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        // Bump allocator does not deallocate individual blocks.
        // Special case is when deallocating the block with same address as where pointer is currently pointing at.
        // let heap_start = self.start as usize; 
        // match ptr as usize{
            
        // }
        let size = layout.size();
        let current_addr = self.next.load(Ordering::Relaxed);
        if ptr as usize == current_addr{
            self.next.fetch_sub(size, Ordering::Relaxed);
        }

    }
}
impl Drop for BumpAllocator {
    fn drop(&mut self) {
        let layout = Layout::from_size_align(self.next.load(Ordering::Relaxed) - self.start as usize, 8).unwrap();
        unsafe {
            std::alloc::dealloc(self.start, layout);
        }
    }
}

#[derive(Debug)]
pub struct StatsSnapshot {
    pub allocs: [u64; NUM_CLASSES],
    pub deallocs: [u64; NUM_CLASSES],
    pub large_allocs: u64,
    pub large_deallocs: u64,
    pub bytes_allocated: u64,
    pub bytes_deallocated: u64,
}

struct Counters {
    allocs: [AtomicU64; NUM_CLASSES],
    deallocs: [AtomicU64; NUM_CLASSES],
    large_allocs: AtomicU64,
    large_deallocs: AtomicU64,
    bytes_allocated: AtomicU64,
    bytes_deallocated: AtomicU64,
}

impl Counters {
    pub const fn new() -> Self {
        Self {
            allocs: [
                AtomicU64::new(0),
                AtomicU64::new(0),
                AtomicU64::new(0),
                AtomicU64::new(0),
                AtomicU64::new(0),
                AtomicU64::new(0),
                AtomicU64::new(0),
                AtomicU64::new(0),
                AtomicU64::new(0),
                AtomicU64::new(0),
                AtomicU64::new(0),
                AtomicU64::new(0),
                AtomicU64::new(0),
                AtomicU64::new(0),
                AtomicU64::new(0),
                AtomicU64::new(0),
            ],
            deallocs: [
                AtomicU64::new(0),
                AtomicU64::new(0),
                AtomicU64::new(0),
                AtomicU64::new(0),
                AtomicU64::new(0),
                AtomicU64::new(0),
                AtomicU64::new(0),
                AtomicU64::new(0),
                AtomicU64::new(0),
                AtomicU64::new(0),
                AtomicU64::new(0),
                AtomicU64::new(0),
                AtomicU64::new(0),
                AtomicU64::new(0),
                AtomicU64::new(0),
                AtomicU64::new(0),
            ],
            large_allocs: AtomicU64::new(0),
            large_deallocs: AtomicU64::new(0),
            bytes_allocated: AtomicU64::new(0),
            bytes_deallocated: AtomicU64::new(0),
        }
    }

    fn snapshot(&self) -> StatsSnapshot {
    let allocs = std::array::from_fn(|i| self.allocs[i].load(Ordering::Relaxed));
    let deallocs = std::array::from_fn(|i| self.deallocs[i].load(Ordering::Relaxed));

    StatsSnapshot {
        allocs,
        deallocs,
        large_allocs: self.large_allocs.load(Ordering::Relaxed),
        large_deallocs: self.large_deallocs.load(Ordering::Relaxed),
        bytes_allocated: self.bytes_allocated.load(Ordering::Relaxed),
        bytes_deallocated: self.bytes_deallocated.load(Ordering::Relaxed),
    }
}
}

pub struct Halloc {
    system: System,
    counters: Counters,
}

impl Halloc {
    pub const fn new() -> Self {
        Self {
            system: System,
            counters: Counters::new(),
        }
    }

    pub fn stats(&self) -> StatsSnapshot {
        self.counters.snapshot()
    }
}

unsafe impl GlobalAlloc for Halloc {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let size = layout.size();
        self.counters
            .bytes_allocated
            .fetch_add(size as u64, Ordering::Relaxed);

        match class_index(size) {
            Some(index) => {
                self.counters.allocs[index].fetch_add(1, Ordering::Relaxed);
            }
            None => {
                self.counters.large_allocs.fetch_add(1, Ordering::Relaxed);
            }
        }

        unsafe { self.system.alloc(layout) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        let size = layout.size();
        self.counters
            .bytes_deallocated
            .fetch_add(size as u64, Ordering::Relaxed);

        match class_index(size) {
            Some(index) => {
                self.counters.deallocs[index].fetch_add(1, Ordering::Relaxed);
            }
            None => {
                self.counters
                    .large_deallocs
                    .fetch_add(1, Ordering::Relaxed);
            }
        }

        unsafe { self.system.dealloc(ptr, layout) }
    }
}



//returns the class index for a given size, or None if the size is larger than the largest class
fn class_index(size: usize) -> Option<usize> {
    if size == 0 {
        return Some(0);
    }

    SIZE_CLASSES
        .iter()
        .position(|class_size| size <= *class_size)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn class_index_matches_bounds() {
        assert_eq!(class_index(1), Some(0));
        assert_eq!(class_index(8), Some(0));
        assert_eq!(class_index(9), Some(1));
        assert_eq!(class_index(64), Some(5));
        assert_eq!(class_index(2048), Some(NUM_CLASSES - 1));
        assert_eq!(class_index(2049), None);
        // println!("{}",ha)
    }

    #[test]
    fn stats_tracking() {
        let allocator = Halloc::new();

        // Allocate and deallocate some memory
        let mut layout_list:Vec<Layout> = Vec::new();
        
        // layout_list.push(Layout::from_size_align(16, 8).unwrap());
        layout_list.push(Layout::from_size_align(48, 8).unwrap());
        layout_list.push(Layout::from_size_align(160, 16).unwrap());   
        for layout in layout_list {
        unsafe {
            let ptr = allocator.alloc(layout);
            assert!(!ptr.is_null());
            allocator.dealloc(ptr, layout);
        }
    }
        let stats = allocator.stats();
        println!("{:#?}", stats);
        // assert_eq!(stats.allocs[1], 1); // 16 bytes falls into class index 1
        // assert_eq!(stats.deallocs[1], 1);
        // assert_eq!(stats.large_allocs, 0);
        // assert_eq!(stats.large_deallocs, 0);
        // assert_eq!(stats.bytes_allocated, 16);
        // assert_eq!(stats.bytes_deallocated, 16);
    }
}
