use std::sync::{Arc, atomic::{AtomicUsize, Ordering}};
use std::alloc::{Layout, GlobalAlloc, System};
use crate::AllocatorStrategy;
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
        let layout = Layout::from_size_align(capacity, 4).ok()?;
        // println!("{:?}",layout);
        let start = unsafe { std::alloc::alloc(layout) as *mut u8 };
        // println!("start:{:?} and as usize {}",start, start as usize);
        if start.is_null() {
            return None;
        }

        Some(Self {
            start,
            next: Arc::new(AtomicUsize::new(0)),
        })
    }
}

impl AllocatorStrategy for BumpAllocator {
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
        if ptr as usize == current_addr {
            self.next.fetch_sub(size, Ordering::Relaxed);
        }
    }
}


impl Drop for BumpAllocator {
    fn drop(&mut self) {
        // let layout =
        //     Layout::from_size_align(self.next.load(Ordering::Relaxed) - self.start as usize, 8)
        //         .unwrap();
        // unsafe {
        //     std::alloc::dealloc(self.start, layout);
        // }
    }
}
