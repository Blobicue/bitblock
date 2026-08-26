//! Prints deterministic CPU-frame observations for named Bitblock fixtures.

use bitblock::camera::Camera;
use bitblock::frame::FrameRef;
use bitblock::render::{Renderer, bounds_aabb};
use bitblock::world::{Coord, World};

const SIZE: u32 = 801;

fn main() {
    let initial = World::new();
    observe(
        "initial-one-pixel",
        &initial,
        &Camera::for_single_pixel_voxel(SIZE),
    );
    observe("spawn-cube", &initial, &Camera::for_observer_spawn());

    let mut aggregate = World::new();
    for coord in [
        Coord::new(-7, 3, 2),
        Coord::new(1, 0, 0),
        Coord::new(11, -4, 9),
    ] {
        aggregate.set_on(coord);
    }
    let mut camera = Camera::for_observer_spawn();
    let (min, max) = bounds_aabb(aggregate.bounds().expect("bedrock keeps bounds nonempty"));
    camera.fit_bounds(min, max, SIZE, SIZE);
    observe("aggregate-four-bit", &aggregate, &camera);
}

fn observe(label: &str, world: &World, camera: &Camera) {
    let mut renderer = Renderer::new(SIZE, SIZE);
    renderer.render(world, camera);
    renderer.draw_crosshair(bitblock::render::crosshair_overlay_input());
    let frame = FrameRef::new(renderer.width(), renderer.height(), renderer.pixels())
        .expect("renderer shape");
    let observation = frame.observe();
    println!("{label}\t{}\t{:?}", observation.tag, observation.facts);
}
