use std::alloc::{GlobalAlloc, Layout, System};
use std::hint::black_box;
use std::sync::atomic::{AtomicUsize, Ordering};

use bitblock::camera::Camera;
use bitblock::history::{History, world_tag};
use bitblock::sidebar::{DisplayStatus, SidebarSnapshot};
use bitblock::world::World;

struct CountingAllocator;

static ALLOCATIONS: AtomicUsize = AtomicUsize::new(0);
static ALLOCATED_BYTES: AtomicUsize = AtomicUsize::new(0);

unsafe impl GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        ALLOCATIONS.fetch_add(1, Ordering::Relaxed);
        ALLOCATED_BYTES.fetch_add(layout.size(), Ordering::Relaxed);
        unsafe { System.alloc(layout) }
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        ALLOCATIONS.fetch_add(1, Ordering::Relaxed);
        ALLOCATED_BYTES.fetch_add(layout.size(), Ordering::Relaxed);
        unsafe { System.alloc_zeroed(layout) }
    }

    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        unsafe { System.dealloc(pointer, layout) }
    }

    unsafe fn realloc(&self, pointer: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        ALLOCATIONS.fetch_add(1, Ordering::Relaxed);
        ALLOCATED_BYTES.fetch_add(new_size, Ordering::Relaxed);
        unsafe { System.realloc(pointer, layout, new_size) }
    }
}

#[global_allocator]
static GLOBAL: CountingAllocator = CountingAllocator;

fn main() {
    let world = World::new();
    let history = History::new(&world);
    let camera = Camera::for_observer_spawn();
    let snapshot = SidebarSnapshot {
        frame_source_tick: 0,
        tick_index: 1,
        occupied_bits: world.len() as u64,
        mutable_bits: world.len().saturating_sub(1) as u64,
        bounds: world.bounds(),
        observer_position: camera.position(),
        observer_cell: None,
        yaw_radians: camera.yaw_radians(),
        pitch_radians: camera.pitch_radians(),
        observation_distance: camera.observation_distance(),
        origin_distance: camera.position().length(),
        simulation_width: 801,
        simulation_height: 801,
        output_width: 1201,
        output_height: 801,
        sidebar_width: 400,
        world_pixels: 1,
        top_saturated: false,
        world: world_tag(&world),
        history_head: history.head(),
        history_len: 0,
        prior_frame_head: None,
        prior_frame_len: 0,
        prior_display_head: None,
        prior_display_len: 0,
        display_status: DisplayStatus::AuditDisabled,
        visible: true,
        previous_raster: None,
    };

    black_box(snapshot.tag());
    ALLOCATIONS.store(0, Ordering::SeqCst);
    ALLOCATED_BYTES.store(0, Ordering::SeqCst);
    let tag = black_box(snapshot.tag());
    let allocations = ALLOCATIONS.load(Ordering::SeqCst);
    let bytes = ALLOCATED_BYTES.load(Ordering::SeqCst);
    println!("{tag}\tallocations={allocations}\tallocated_bytes={bytes}");
    assert_eq!(allocations, 0, "SidebarSnapshot::tag allocated");
    assert_eq!(bytes, 0, "SidebarSnapshot::tag allocated bytes");
}
