//! Reproducible `SCALE-1` workload and latency-distribution probe.
//!
//! Evaluates:
//! 1. All 5 deterministic `ScaleWorkloadReportV1` worlds (asserting the pinned canonical
//!    receipt tag `#bb.receipt.sha256.xq6oxiyyao3khyp3i7r6zbz3zwewouc75btw4s733xp62zeic3ma`).
//! 2. Raw multi-sample distributions (`p50`, `p95`, `max` across 15 independent samples) at
//!    801x801 viewport across progressive hollow shells (`1`..`149,787` bits) plus
//!    `SparseFarExtrema` (±1,048,576) for:
//!    - Octree-pruned `Renderer::render` vs reference full-scan render (asserting exact pixel parity)
//!    - `raycast` vs `raycast_reference` (asserting exact hit parity)
//!    - `sweep_point` vs `sweep_point_reference` (asserting exact hit parity)
//!    - `world_tag` v2 cold rebuild vs single-bit incremental update
//!
//! ```text
//! cargo run --release --features protocol-research --example scale_1_probe
//! cargo run --release --features protocol-research --example scale_1_probe -- --json
//! ```

use std::env;
use std::hint::black_box;
use std::time::Instant;

use bitblock::camera::{Camera, Vec3};
use bitblock::history::world_tag;
use bitblock::render::{
    Renderer, bounds_aabb, raycast, raycast_reference, sweep_point, sweep_point_reference,
};
use bitblock::scale_assay::{ScaleWorkloadId, ScaleWorkloadReportV1, build_workload_world};
use bitblock::world::{Coord, World};

const VIEWPORT: u32 = 801;
const SAMPLES: usize = 15;
const RENDER_BUDGET_MS: f64 = 8.33;
const TICK_BUDGET_MS: f64 = 1000.0 / 60.0;

struct Dist {
    p50: f64,
    p95: f64,
    max: f64,
}

struct ScaleRow {
    name: String,
    bits: usize,
    chunks: usize,
    root_octants: usize,
    render_octree_ms: Dist,
    render_ref_ms: Dist,
    pixel_parity: bool,
    raycast_us: Dist,
    raycast_ref_us: Dist,
    raycast_parity: bool,
    sweep_us: Dist,
    sweep_ref_us: Dist,
    sweep_parity: bool,
    world_tag_cold_us: Dist,
    world_tag_inc_us: Dist,
}

fn summarize(mut samples: Vec<f64>) -> Dist {
    samples.sort_by(f64::total_cmp);
    let n = samples.len();
    let idx = |pct: usize| ((n * pct).div_ceil(100)).saturating_sub(1).min(n - 1);
    Dist {
        p50: samples[idx(50)],
        p95: samples[idx(95)],
        max: samples[n - 1],
    }
}

fn measure_ms<R>(count: usize, mut f: impl FnMut() -> R) -> Dist {
    black_box(f());
    let mut out = Vec::with_capacity(count);
    for _ in 0..count {
        let t0 = Instant::now();
        black_box(f());
        out.push(t0.elapsed().as_nanos() as f64 / 1_000_000.0);
    }
    summarize(out)
}

fn measure_us<R>(count: usize, mut f: impl FnMut() -> R) -> Dist {
    black_box(f());
    let mut out = Vec::with_capacity(count);
    for _ in 0..count {
        let t0 = Instant::now();
        black_box(f());
        out.push(t0.elapsed().as_nanos() as f64 / 1_000.0);
    }
    summarize(out)
}

fn hollow_shell(target: i64) -> World {
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

fn probe_world(name: String, mut world: World, renderer: &mut Renderer) -> ScaleRow {
    let bits = world.len();
    let chunks = world.allocated_chunk_count();
    let root_octants = world.root_octants().count();

    let (min, max) = bounds_aabb(world.bounds().expect("bedrock keeps bounds non-empty"));
    let anchor = min + (max - min) * 0.5;
    let radius = (max - min).length().max(1.0);
    let mut camera = Camera::for_observer_spawn();
    if name != "SparseFarExtrema" {
        camera.set_observation_distance(radius * 1.5);
        camera.set_observation_anchor(anchor);
    }

    // 1. Pixel-exact parity between octree-pruned renderer and reference traversal
    renderer.set_reference_traversal(false);
    renderer.render(&world, &camera);
    let octree_pixels = renderer.pixels().to_vec();
    renderer.set_reference_traversal(true);
    renderer.render(&world, &camera);
    let pixel_parity = renderer.pixels() == octree_pixels.as_slice();
    assert!(
        pixel_parity,
        "{name}: octree render diverged from reference"
    );

    renderer.set_reference_traversal(false);
    let render_octree_ms = measure_ms(SAMPLES, || renderer.render(&world, &camera));
    renderer.set_reference_traversal(true);
    let render_ref_ms = measure_ms(SAMPLES, || renderer.render(&world, &camera));
    renderer.set_reference_traversal(false);

    // 2. Raycast parity and distributions
    let ray = camera.center_ray();
    let hit_opt = raycast(&world, ray);
    let hit_ref = raycast_reference(&world, ray);
    let raycast_parity = hit_opt == hit_ref;
    assert!(raycast_parity, "{name}: raycast diverged from reference");
    let raycast_us = measure_us(SAMPLES * 4, || raycast(&world, ray));
    let raycast_ref_us = measure_us(SAMPLES * 4, || raycast_reference(&world, ray));

    // 3. Sweep-point collision parity and distributions
    let start = camera.position();
    let delta = Vec3::new(0.25, -0.15, 1.5);
    let sw_opt = sweep_point(&world, start, delta);
    let sw_ref = sweep_point_reference(&world, start, delta);
    let sweep_parity = sw_opt == sw_ref;
    assert!(sweep_parity, "{name}: sweep_point diverged from reference");
    let sweep_us = measure_us(SAMPLES * 4, || sweep_point(&world, start, delta));
    let sweep_ref_us = measure_us(SAMPLES * 4, || sweep_point_reference(&world, start, delta));

    // 4. world_tag v2 cold rebuild (fresh clone with empty chunk digest cache) vs single-edit incremental
    let world_tag_cold_us = measure_us(SAMPLES, || {
        let fresh = world.clone();
        world_tag(&fresh)
    });
    let _ = world_tag(&world); // warm cache
    let edit_coord = Coord::new(777_777, -555_555, 333_333);
    let world_tag_inc_us = measure_us(SAMPLES * 4, || {
        world.set_on(edit_coord);
        let t1 = world_tag(&world);
        world.set_off(edit_coord);
        let t2 = world_tag(&world);
        (t1, t2)
    });

    ScaleRow {
        name,
        bits,
        chunks,
        root_octants,
        render_octree_ms,
        render_ref_ms,
        pixel_parity,
        raycast_us,
        raycast_ref_us,
        raycast_parity,
        sweep_us,
        sweep_ref_us,
        sweep_parity,
        world_tag_cold_us,
        world_tag_inc_us,
    }
}

fn main() {
    let json_mode = env::args().any(|a| a == "--json");

    // Verify canonical 5-workload structural assay report first
    let report = ScaleWorkloadReportV1::canonical();
    assert_eq!(report.validate(), Ok(()));
    let report_tag = report.tag().to_string();
    assert_eq!(
        report_tag,
        "#bb.receipt.sha256.xq6oxiyyao3khyp3i7r6zbz3zwewouc75btw4s733xp62zeic3ma"
    );

    let mut renderer = Renderer::new(VIEWPORT, VIEWPORT);
    let mut rows = Vec::new();
    for target in [1_i64, 100, 10_000, 100_000, 1_000_000, 4_000_000] {
        let w = hollow_shell(target);
        let label = format!("HollowShell_{}", w.len());
        rows.push(probe_world(label, w, &mut renderer));
    }
    rows.push(probe_world(
        "DenseCube16".to_string(),
        build_workload_world(ScaleWorkloadId::DenseCube16),
        &mut renderer,
    ));
    rows.push(probe_world(
        "SparseFarExtrema".to_string(),
        build_workload_world(ScaleWorkloadId::SparseFarExtrema),
        &mut renderer,
    ));

    if json_mode {
        println!("{{");
        println!("  \"scale_workload_report_v1_tag\": \"{report_tag}\",");
        println!("  \"viewport_pixels\": [{VIEWPORT}, {VIEWPORT}],");
        println!("  \"raw_samples_per_workload\": {SAMPLES},");
        println!("  \"render_budget_ms\": {RENDER_BUDGET_MS:.2},");
        println!("  \"tick_budget_ms\": {TICK_BUDGET_MS:.6},");
        println!("  \"workloads\": [");
        for (i, r) in rows.iter().enumerate() {
            let comma = if i + 1 == rows.len() { "" } else { "," };
            println!("    {{");
            println!("      \"name\": \"{}\",", r.name);
            println!("      \"bits\": {},", r.bits);
            println!("      \"chunks\": {},", r.chunks);
            println!("      \"root_octants\": {},", r.root_octants);
            println!("      \"pixel_parity\": {},", r.pixel_parity);
            println!("      \"raycast_parity\": {},", r.raycast_parity);
            println!("      \"sweep_parity\": {},", r.sweep_parity);
            println!(
                "      \"render_octree_ms\": {{\"p50\": {:.3}, \"p95\": {:.3}, \"max\": {:.3}}},",
                r.render_octree_ms.p50, r.render_octree_ms.p95, r.render_octree_ms.max
            );
            println!(
                "      \"render_ref_ms\": {{\"p50\": {:.3}, \"p95\": {:.3}, \"max\": {:.3}}},",
                r.render_ref_ms.p50, r.render_ref_ms.p95, r.render_ref_ms.max
            );
            println!(
                "      \"raycast_us\": {{\"p50\": {:.2}, \"p95\": {:.2}, \"max\": {:.2}}},",
                r.raycast_us.p50, r.raycast_us.p95, r.raycast_us.max
            );
            println!(
                "      \"raycast_ref_us\": {{\"p50\": {:.2}, \"p95\": {:.2}, \"max\": {:.2}}},",
                r.raycast_ref_us.p50, r.raycast_ref_us.p95, r.raycast_ref_us.max
            );
            println!(
                "      \"sweep_us\": {{\"p50\": {:.2}, \"p95\": {:.2}, \"max\": {:.2}}},",
                r.sweep_us.p50, r.sweep_us.p95, r.sweep_us.max
            );
            println!(
                "      \"sweep_ref_us\": {{\"p50\": {:.2}, \"p95\": {:.2}, \"max\": {:.2}}},",
                r.sweep_ref_us.p50, r.sweep_ref_us.p95, r.sweep_ref_us.max
            );
            println!(
                "      \"world_tag_cold_us\": {{\"p50\": {:.2}, \"p95\": {:.2}, \"max\": {:.2}}},",
                r.world_tag_cold_us.p50, r.world_tag_cold_us.p95, r.world_tag_cold_us.max
            );
            println!(
                "      \"world_tag_inc_us\": {{\"p50\": {:.2}, \"p95\": {:.2}, \"max\": {:.2}}}",
                r.world_tag_inc_us.p50, r.world_tag_inc_us.p95, r.world_tag_inc_us.max
            );
            println!("    }}{comma}");
        }
        println!("  ]");
        println!("}}");
        return;
    }

    println!("ScaleWorkloadReportV1 tag: {report_tag}");
    println!(
        "{:<22} {:>7} {:>6} {:>9} {:>9} {:>9} {:>8} {:>8} {:>8} {:>8}",
        "workload",
        "bits",
        "chunks",
        "oct_p50ms",
        "oct_p95ms",
        "ref_p50ms",
        "ray_us",
        "swp_us",
        "tag_cold",
        "tag_inc"
    );
    for r in &rows {
        println!(
            "{:<22} {:>7} {:>6} {:>9.2} {:>9.2} {:>9.2} {:>8.1} {:>8.1} {:>8.1} {:>8.1}",
            r.name,
            r.bits,
            r.chunks,
            r.render_octree_ms.p50,
            r.render_octree_ms.p95,
            r.render_ref_ms.p50,
            r.raycast_us.p50,
            r.sweep_us.p50,
            r.world_tag_cold_us.p50,
            r.world_tag_inc_us.p50
        );
    }
}
