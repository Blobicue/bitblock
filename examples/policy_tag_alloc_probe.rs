use std::alloc::{GlobalAlloc, Layout, System};
use std::hint::black_box;
use std::sync::atomic::{AtomicUsize, Ordering};

use bitblock::sidebar::composite_policy_tag;

struct CountingAllocator;

static ALLOCATIONS: AtomicUsize = AtomicUsize::new(0);
static ALLOCATED_BYTES: AtomicUsize = AtomicUsize::new(0);

unsafe impl GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        ALLOCATIONS.fetch_add(1, Ordering::SeqCst);
        ALLOCATED_BYTES.fetch_add(layout.size(), Ordering::SeqCst);
        unsafe { System.alloc(layout) }
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        ALLOCATIONS.fetch_add(1, Ordering::SeqCst);
        ALLOCATED_BYTES.fetch_add(layout.size(), Ordering::SeqCst);
        unsafe { System.alloc_zeroed(layout) }
    }

    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        unsafe { System.dealloc(pointer, layout) }
    }

    unsafe fn realloc(&self, pointer: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        ALLOCATIONS.fetch_add(1, Ordering::SeqCst);
        ALLOCATED_BYTES.fetch_add(new_size, Ordering::SeqCst);
        unsafe { System.realloc(pointer, layout, new_size) }
    }
}

#[global_allocator]
static GLOBAL: CountingAllocator = CountingAllocator;

fn measure(call: impl FnOnce()) -> (usize, usize) {
    ALLOCATIONS.store(0, Ordering::SeqCst);
    ALLOCATED_BYTES.store(0, Ordering::SeqCst);
    call();
    (
        ALLOCATIONS.load(Ordering::SeqCst),
        ALLOCATED_BYTES.load(Ordering::SeqCst),
    )
}

fn main() {
    let cold = measure(|| {
        black_box(composite_policy_tag());
    });
    let tag = composite_policy_tag();
    let warm = measure(|| {
        black_box(composite_policy_tag());
    });

    assert_eq!(cold, (0, 0));
    assert_eq!(warm, (0, 0));
    println!(
        "cold_allocations={}\tcold_allocated_bytes={}\twarm_allocations={}\twarm_allocated_bytes={}\tcomposite_policy={tag}",
        cold.0, cold.1, warm.0, warm.1
    );
}
