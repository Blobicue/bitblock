//! Observes allocation requests made by canonical world-root construction.
//!
//! This is a local allocator probe, not a wall-time, hardware-memory-traffic,
//! or energy measurement. Run it in release mode when comparing implementations.

use std::alloc::{GlobalAlloc, Layout, System};
use std::hint::black_box;
use std::sync::atomic::{AtomicUsize, Ordering};

use bitblock::history::world_tag;
use bitblock::world::{Coord, World};

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

fn measure(world: &World) -> (usize, usize, bitblock::protocol::ContentTag) {
    ALLOCATIONS.store(0, Ordering::SeqCst);
    ALLOCATED_BYTES.store(0, Ordering::SeqCst);
    let tag = black_box(world_tag(black_box(world)));
    (
        ALLOCATIONS.load(Ordering::SeqCst),
        ALLOCATED_BYTES.load(Ordering::SeqCst),
        tag,
    )
}

fn spread_world() -> World {
    let mut world = World::new();
    for index in 0_i64..4_096 {
        let x = (index % 64) * 17 - 512;
        let y = ((index / 64) % 64) * 19 - 608;
        let z = (index / (64 * 64)) * 23 - 23;
        world.set_on(Coord::new(x, y, z));
    }
    world.set_on(Coord::new(i64::MIN, 0, i64::MAX));
    world
}

fn main() {
    let origin = World::new();
    let spread = spread_world();

    for (label, world) in [("origin", &origin), ("spread", &spread)] {
        let (allocations, allocated_bytes, tag) = measure(world);
        println!(
            "{label}\toccupied={}\tallocations={allocations}\tallocated_bytes={allocated_bytes}\tworld={tag}",
            world.len()
        );
    }
}
