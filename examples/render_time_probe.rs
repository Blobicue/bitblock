//! Wall-clock cost of one rendered frame, against the fixed-tick budget.
//!
//! This is a local wall-time sample on one machine, in one build profile, with
//! whatever else that machine was doing. It is not a benchmark, not a
//! cross-machine claim, and not evidence about any world but the ones below.
//! What it is for: the observer runs a fixed 60 Hz tick, so a frame has about
//! 16.67 ms, and until now nothing in this repository measured how much of that
//! a render actually spends.
//!
//! Occupancy is varied because the renderer traverses occupied bits: a cost
//! that is flat in `N` and a cost that is linear in `N` want different fixes,
//! and the shape of the curve says which one this is.
//!
//! Run it in release. A debug figure measures the absence of optimisation:
//!
//! ```text
//! cargo run --release --example render_time_probe
//! ```

use std::time::Instant;

use bitblock::camera::Camera;
use bitblock::frame::FrameRef;
use bitblock::render::{Renderer, bounds_aabb};
use bitblock::world::{Coord, World};

const SIZE: u32 = 801;
const BUDGET_MS: f64 = 1000.0 / 60.0;
const REPEATS: u32 = 12;

fn main() {
    // Reporting the drawn-pixel count is not decoration. render() has two fast
    // paths -- a world that projects to one pixel, and a camera inside a bit --
    // and a probe that silently lands on either measures nothing about the
    // per-bit loop it was meant to time.
    println!(
        "{:>10}  {:>10}  {:>9}  {:>9}  {:>9}  {:>8}",
        "bits", "drawn px", "render", "observe", "total", "of budget"
    );

    for (_label, world) in [
        ("bedrock", filled(0)),
        ("1e2", filled(100)),
        ("1e4", filled(10_000)),
        ("1e5", filled(100_000)),
    ] {
        // NOT fit_bounds. `fit` here means saturation: it solves for the
        // distance at which every corner lands within half a pixel of centre,
        // so a fitted world is one pixel and render() returns down its fast
        // path. That is the intended meaning of fit in this observer, and it is
        // why a probe that used it measured nothing. The distance below is a
        // few world radii, which puts the occupied set on screen at a size the
        // per-bit loop actually has to draw.
        let mut camera = Camera::for_observer_spawn();
        if let Some(bounds) = world.bounds() {
            let (min, max) = bounds_aabb(bounds);
            let anchor = min + (max - min) * 0.5;
            let radius = (max - min).length().max(1.0);
            camera.set_observation_distance(radius * 1.5);
            camera.set_observation_anchor(anchor);
        }

        let bits = world.iter_on().count();
        let mut renderer = Renderer::new(SIZE, SIZE);
        // One untimed pass so the first sample is not paying for cold pages.
        renderer.render(&world, &camera);

        let mut render_ns = u128::MAX;
        let mut observe_ns = u128::MAX;
        for _ in 0..REPEATS {
            let mark = Instant::now();
            renderer.render(&world, &camera);
            renderer.draw_crosshair(bitblock::render::crosshair_overlay_input());
            render_ns = render_ns.min(mark.elapsed().as_nanos());

            let frame = FrameRef::new(renderer.width(), renderer.height(), renderer.pixels())
                .expect("renderer shape is a valid frame");
            let mark = Instant::now();
            let _ = frame.observe();
            observe_ns = observe_ns.min(mark.elapsed().as_nanos());
        }

        let drawn = FrameRef::new(renderer.width(), renderer.height(), renderer.pixels())
            .expect("renderer shape is a valid frame")
            .observe()
            .facts
            .non_background;
        let render_ms = render_ns as f64 / 1e6;
        let observe_ms = observe_ns as f64 / 1e6;
        let total_ms = render_ms + observe_ms;
        println!(
            "{bits:>10}  {drawn:>10}  {render_ms:>7.2}ms  {observe_ms:>7.2}ms  {total_ms:>7.2}ms  {:>7.1}%",
            100.0 * total_ms / BUDGET_MS
        );
    }

    println!();
    println!(
        "{} x {} = {} pixels, budget {BUDGET_MS:.2}ms at 60Hz.",
        SIZE,
        SIZE,
        SIZE * SIZE
    );
    println!("Fastest of {REPEATS} samples each, which reports the machine at its least busy");
    println!("rather than a distribution. Nothing here is evidence about another host.");
    println!();
    println!("Note the camera is placed a few world radii away, NOT with fit_bounds.");
    println!("`fit` in this observer means saturation -- it solves for the distance at which");
    println!("the whole world lands within half a pixel of centre -- so a fitted world takes");
    println!("render()'s one-pixel fast path and a probe using it measures nothing.");
}

/// A deterministic world with roughly `target` non-bedrock bits, laid out as a
/// cube shell so the occupied set has surface area rather than being one solid
/// block the renderer can reject wholesale.
fn filled(target: i64) -> World {
    let mut world = World::new();
    if target == 0 {
        return world;
    }
    let side = (target as f64).cbrt().ceil() as i64;
    let mut placed = 0;
    for x in 0..side {
        for y in 0..side {
            for z in 0..side {
                if placed >= target {
                    return world;
                }
                // Skip the interior: a hollow shell is the shape a voxel
                // renderer actually has to draw.
                let interior =
                    x > 0 && y > 0 && z > 0 && x < side - 1 && y < side - 1 && z < side - 1;
                if interior {
                    continue;
                }
                world.set_on(Coord::new(x - side / 2, y - side / 2, z - side / 2));
                placed += 1;
            }
        }
    }
    world
}
