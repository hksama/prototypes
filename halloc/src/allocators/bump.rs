use std::sync::{Arc, atomic::{AtomicUsize, Ordering}};
use std::alloc::Layout;
use crate::{AllocatorWrapper, request_memory_by_mmap};
pub struct BumpAllocator {
    /// Start of the allocated region
    start: Arc<usize>,
    /// Current position, Arc for thread-safe updates, AtomicUsize for lock-free increments
    curr: Arc<AtomicUsize>,
    /// Total capacity of the allocated region
    capacity: usize,
}
pub const MEM_EXTEND_SIZE: usize = 1024 * 1024; // 1 MiB

impl BumpAllocator {
    /// Create a new bump allocator by allocating `capacity` bytes from the system.
    pub fn new(capacity: usize) -> Option<Self> {
        // For now, use System allocator to get the initial block
        // let layout = Layout::from_size_align(capacity, 4).ok()?;
        // println!("{:?}",layout);
        // let start = unsafe { std::alloc::alloc(layout) as usize };
        // println!("start:{:?} and as usize {}",start, start as usize);
        // if start.is_null() {
        //     return None;
        // }
        let bootstrap_ptr = unsafe{request_memory_by_mmap(MEM_EXTEND_SIZE).ok()?} as usize;

        Some(Self {
            start: Arc::new(bootstrap_ptr),
            curr: Arc::new(AtomicUsize::new(bootstrap_ptr)),
            capacity,
        })
    }
}

impl AllocatorWrapper for BumpAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let size = layout.size();
        let alignment = layout.align();
        // check if self is initialized or not.
        // let heap_start = *self.start as usize;

        if size == 0 {
            return std::ptr::null_mut();
        }
        if size > self.capacity {
            panic!("Requested size {} exceeds bump allocator capacity {}", size, self.capacity);
            // println!("Requested size {} exceeds bump allocator capacity {}", size, self.capacity);
            // unsafe { request_memory_by_mmap(MEM_EXTEND_SIZE).unwrap() };
        }
        // Calculate the next aligned address
        let updated = self.curr.fetch_add(size, Ordering::Relaxed);
        // (heap_start+updated_next) as *mut u8
        updated as *mut u8
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        // Bump allocator does not deallocate individual blocks.
        // Special case is when deallocating the block with same address as where pointer is currently pointing at.
        // let heap_start = self.start as usize;
        // match ptr as usize{

        // }
        let size = layout.size();
        let current_addr = self.curr.load(Ordering::Relaxed);
        if ptr as usize == current_addr {
            self.curr.fetch_sub(size, Ordering::Relaxed);
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
