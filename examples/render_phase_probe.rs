//! Where inside one render the time goes: clearing, projecting, or rasterising.
//!
//! `render_time_probe` says a frame costs 0.90ms at one bit and 8.40ms at
//! 12,699, with the same 109,561 pixels drawn either way. That says the cost is
//! not the pixel count, but not which part of the per-bit path it is.
//!
//! The decomposition uses only public API and one property of the renderer: a
//! bit whose vertices all fail to project is rejected before any triangle is
//! set up. Pointing the camera so the world is behind it therefore costs the
//! buffer clear plus eight projections per bit and nothing else.
//!
//!   clear    = the away-facing render of a single bit
//!   project  = away-facing render of N bits, minus that
//!   raster   = the facing render, minus the away-facing one
//!
//! Two things this had to get right, both found by the numbers being absurd
//! rather than by care. `Renderer::new` allocates 7.7MB, so building one inside
//! the timed closure measures the allocator -- it reported a "clear" larger
//! than the whole render it was a component of. And `World::empty()` is not
//! empty: origin bedrock is immutable, so it holds one bit, and rendering it
//! from the spawn camera draws that bit across most of the screen. A
//! decomposition whose parts exceed the total is wrong on its face, which is
//! the only reason either got caught.
//!
//! ```text
//! cargo run --release --example render_phase_probe
//! ```
//!
//! Local wall-clock samples on one machine in one build profile. Not a
//! benchmark, not a cross-machine claim.

use std::time::Instant;

use bitblock::camera::Camera;
use bitblock::render::{Renderer, bounds_aabb};
use bitblock::world::{Coord, World};

const SIZE: u32 = 801;
const BUDGET_MS: f64 = 1000.0 / 60.0;
const REPEATS: u32 = 12;

struct Sample {
    bits: usize,
    facing: f64,
    away: f64,
    drawn: u64,
}

fn main() {
    let mut renderer = Renderer::new(SIZE, SIZE);
    let mut samples = Vec::new();

    for target in [1_i64, 100, 10_000, 100_000] {
        let world = shell(target);
        let bits = world.iter_on().count();
        let (min, max) = bounds_aabb(world.bounds().expect("bedrock keeps bounds nonempty"));
        let anchor = min + (max - min) * 0.5;
        let radius = (max - min).length().max(1.0);

        let mut facing = Camera::for_observer_spawn();
        facing.set_observation_distance(radius * 1.5);
        facing.set_observation_anchor(anchor);

        // The same camera with the world put far behind it, so every projection
        // fails and the rasteriser never runs.
        let mut away = facing.clone();
        let back = facing.position() - anchor;
        away.set_observation_anchor(facing.position() + back * 8.0);

        renderer.render(&world, &facing);
        let drawn =
            bitblock::frame::FrameRef::new(renderer.width(), renderer.height(), renderer.pixels())
                .expect("renderer shape is a valid frame")
                .observe()
                .facts
                .non_background;

        samples.push(Sample {
            bits,
            facing: time(REPEATS, || renderer.render(&world, &facing)),
            away: time(REPEATS, || renderer.render(&world, &away)),
            drawn,
        });
    }

    let clear = samples[0].away;
    println!(
        "{:>8}  {:>9}  {:>9}  {:>9}  {:>9}  {:>9}",
        "bits", "drawn px", "clear", "project", "raster", "total"
    );
    for sample in &samples {
        let project = (sample.away - clear).max(0.0);
        let raster = (sample.facing - sample.away).max(0.0);
        println!(
            "{:>8}  {:>9}  {clear:>8.2}ms  {project:>8.2}ms  {raster:>8.2}ms  {:>8.2}ms",
            sample.bits, sample.drawn, sample.facing
        );
    }

    let worst = samples.last().expect("at least one sample");
    println!();
    println!(
        "Budget {BUDGET_MS:.2}ms at 60Hz; the largest world here uses {:.0}% of it.",
        100.0 * worst.facing / BUDGET_MS
    );
    println!(
        "Clear is {} pixels of colour and depth -- {} bytes -- every frame whatever is drawn.",
        SIZE * SIZE,
        SIZE * SIZE * 12
    );
    println!(
        "Raster is what survives the hidden-bit test; ordering the bits lands in project, \n\
         because the away-facing control orders them too."
    );
}

fn time(repeats: u32, mut body: impl FnMut()) -> f64 {
    body();
    let mut best = u128::MAX;
    for _ in 0..repeats {
        let mark = Instant::now();
        body();
        best = best.min(mark.elapsed().as_nanos());
    }
    best as f64 / 1e6
}

/// A hollow cube shell of roughly `target` bits, centred on the origin.
fn shell(target: i64) -> World {
    let mut world = World::new();
    if target <= 1 {
        return world;
    }
    let side = (target as f64).cbrt().ceil() as i64;
    for x in 0..side {
        for y in 0..side {
            for z in 0..side {
                let interior =
                    x > 0 && y > 0 && z > 0 && x < side - 1 && y < side - 1 && z < side - 1;
                if !interior {
                    world.set_on(Coord::new(x - side / 2, y - side / 2, z - side / 2));
                }
            }
        }
    }
    world
}
