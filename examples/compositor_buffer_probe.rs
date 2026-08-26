use std::alloc::{GlobalAlloc, Layout, System};
use std::hint::black_box;
use std::sync::atomic::{AtomicUsize, Ordering};

use bitblock::sidebar::FrameCompositor;
#[cfg(feature = "protocol-research")]
use bitblock::sidebar::TwoBufferReferenceCompositor;

const OUTPUT_WIDTH: u32 = 1_201;
const HEIGHT: u32 = 801;
const SIMULATION_WIDTH: u32 = 801;

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

fn pixel_bytes(width: u32, height: u32) -> usize {
    width as usize * height as usize * size_of::<u32>()
}

fn measure<T>(construct: impl FnOnce() -> T) -> (usize, usize) {
    ALLOCATIONS.store(0, Ordering::SeqCst);
    ALLOCATED_BYTES.store(0, Ordering::SeqCst);
    let value = black_box(construct());
    black_box(&value);
    (
        ALLOCATIONS.load(Ordering::SeqCst),
        ALLOCATED_BYTES.load(Ordering::SeqCst),
    )
}

#[cfg(feature = "protocol-research")]
fn measure_reference() -> (usize, usize) {
    measure(|| TwoBufferReferenceCompositor::new(OUTPUT_WIDTH, HEIGHT, SIMULATION_WIDTH))
}

#[cfg(not(feature = "protocol-research"))]
fn measure_reference() -> (usize, usize) {
    panic!("the reference arm requires --features protocol-research")
}

fn main() {
    let arm = std::env::args()
        .nth(1)
        .expect("usage: compositor_buffer_probe <reference|candidate>");

    let output_bytes = pixel_bytes(OUTPUT_WIDTH, HEIGHT);
    let sidebar_bytes = pixel_bytes(OUTPUT_WIDTH - SIMULATION_WIDTH, HEIGHT);
    let ((allocations, allocated_bytes), expected) = match arm.as_str() {
        "reference" => (measure_reference(), (2, output_bytes + sidebar_bytes)),
        "candidate" => (
            measure(|| FrameCompositor::new(OUTPUT_WIDTH, HEIGHT, SIMULATION_WIDTH)),
            (1, output_bytes),
        ),
        _ => panic!("unknown arm '{arm}'"),
    };

    println!(
        "arm={arm}\tallocations={allocations}\tallocated_bytes={allocated_bytes}\toutput_bytes={output_bytes}\tsidebar_bytes={sidebar_bytes}"
    );
    assert_eq!(allocations, expected.0, "unexpected allocation count");
    assert_eq!(
        allocated_bytes, expected.1,
        "unexpected allocated-byte count"
    );
}
