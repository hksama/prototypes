#![forbid(unsafe_op_in_unsafe_fn)]
//  Forces the compiler to treat any unsafe operation inside an unsafe fn as a compile-time error unless it is explicitly wrapped in an unsafe {} block.

use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::{
    atomic::{AtomicU64, Ordering},
};
use crate::allocators::bump::BumpAllocator;
use thiserror::Error;
use libc::{MAP_ANONYMOUS, MAP_FAILED, MAP_SHARED, PROT_READ, PROT_WRITE, c_void, mmap};
pub mod allocators;

// Define size classes for small allocations. These are approximately logarithmically spaced.
const SIZE_CLASSES: [usize; 16] = [
    8, 16, 24, 32, 48, 64, 96, 128, 192, 256, 384, 512, 768, 1024, 1536, 2048,
];
const NUM_CLASSES: usize = SIZE_CLASSES.len();

/// Allocator Strategy Interface. Each strategy (Bump, Arena, FreeList) implements this.
//Todo: Implement for Send and Sync to allow multi-threaded access. This may require internal synchronization in some strategies.
pub trait AllocatorStrategy {
    /// Allocate memory according to layout.
    /// Returns null on failure.
    unsafe fn alloc(&self, layout: Layout) -> *mut u8;

    /// Deallocate memory.
    /// SAFETY: ptr must be from a prior alloc() call with the same layout.
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout);
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


/// Method to call mmap and get a large block of memory for bootstrapping the allocator; used by all allocators to get their initial memory region. 
unsafe fn bootstrap_memory(alloc_size: usize) -> Result<*mut c_void, HallocErrors> {
    let null_ptr: *mut c_void = std::ptr::null_mut();
    let prot_bits = PROT_WRITE | PROT_READ;
    let flag_bits = MAP_SHARED|MAP_ANONYMOUS;
    unsafe {
        let mem_ptr = mmap(null_ptr, alloc_size, prot_bits, flag_bits, -1, 0);
        
        match mem_ptr {
            MAP_FAILED =>{
                Err(HallocErrors::MapFailed)
            } 
            _=>{
                Ok(mem_ptr)
            }
        }
            // println!("Error: {}", Error::last_os_error());
        
    }
}


/// Errors enum for the halloc crate.
#[derive(Error,Debug)]
enum HallocErrors {
    #[error("Failed to perform mmap.")]
    MapFailed
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
        // println!("Allocating {} bytes", size);
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
                self.counters.large_deallocs.fetch_add(1, Ordering::Relaxed);
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
        let mut layout_list: Vec<Layout> = Vec::new();

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
        // let stats = allocator.stats();
        // println!("{:#?}", stats);
        // assert_eq!(stats.allocs[1], 1); // 16 bytes falls into class index 1
        // assert_eq!(stats.deallocs[1], 1);
        // assert_eq!(stats.large_allocs, 0);
        // assert_eq!(stats.large_deallocs, 0);
        // assert_eq!(stats.bytes_allocated, 16);
        // assert_eq!(stats.bytes_deallocated, 16);
    }

    #[test]
    fn understand_ptr_arithmetic() {
        let allocator = BumpAllocator::new(1024).unwrap();
        let layout1 = Layout::from_size_align(16, 8).unwrap();
        let layout2 = Layout::from_size_align(32, 8).unwrap();

        unsafe {
            let ptr1 = allocator.alloc(layout1);
            let ptr2 = allocator.alloc(layout2);

            // println!("ptr1: {:?} and usize is {}", ptr1, ptr1 as usize);
            // println!("ptr2: {:?} and usize is {}", ptr2, ptr2 as usize);

            // Check that ptr2 is exactly 16 bytes ahead of ptr1
            assert_eq!((ptr1 as usize) + 16, ptr2 as usize);
        }
    }

    #[test]
    fn test_bootstrap_memory()->Result<(),HallocErrors>{
        unsafe {   
            let initial_ptr= bootstrap_memory(1000000 as usize)?;
            *(initial_ptr as *mut u64)= 123;
            assert_eq!(*(initial_ptr as *mut u64), 123);
            println!("{:?} is the initial pointer",initial_ptr);
            Ok(())
        }
    }

    /// Common Test suite for all allocators. Each test should be run against each strategy. 
    #[test]
    fn alloc1byte() {
    }

    #[test]
    fn alloc8bytes() {
    }

    #[test]
    fn alloc16bytes() {
    }

    #[test]
    fn alloc1mb() {
    }

    //todo: Break down into more granular tests for fragmentation, edge cases, etc.
    #[test]
    fn alignmentchecks() {
    }

    #[test]
    fn doublefree() {
    }

    #[test]
    fn stresstest() {
    }

    #[test]
    fn randomalloc_free() {
    }
    //todo: add more tests for edge cases, fragmentation, multi-threaded access, etc.


}


