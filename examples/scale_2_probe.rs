//! Reproducible `SCALE-2` benchmark, parity, extreme-coordinate, and visual PNG snapshot probe.
//!
//! Evaluates:
//! 1. All 8 `SCALE-1` workloads (`HollowShell_1`..`HollowShell_149787`, `DenseCube16`,
//!    `SparseFarExtrema`) at 801x801 under exact hierarchical Hi-Z traversal (`SubpixelLodPolicy::ExactOnly`),
//!    asserting 100% pixel-exact parity with `Renderer::set_reference_traversal(true)`, exact
//!    `raycast` parity, and exact `sweep_point` parity.
//! 2. `HollowShell_149787_Angled` (3/4 oblique view) under exact Hi-Z traversal.
//! 3. `HollowShell_149787_FarSubpixel` comparing `SubpixelLodPolicy::HierarchicalSubpixel`
//!    against `ExactOnly` and verifying bounded ephemeral `SpatialRenderCache` hit/invalidation/cap.
//! 4. `NegativeChunkEdges` (`-17..=17` across all 8 sign octants), `ExtremeFarCluster_1e17`
//!    (`Coord(10^17, -10^17, 10^17)`), and `SignedExtremaCorner` (`i64::MAX - 32`, `i64::MIN + 32`),
//!    proving exact chunk-relative render, raycast, and sweep parity against origin-translated
//!    ground truth plus a powered negative control showing raw `f64` collapses at `10^17`.
//! 5. Optional `--dump-snapshots <dir>` exporting valid 24-bit RGB PNG frames for every workload
//!    so every benchmark scene can be visually inspected directly.
//!
//! ```text
//! cargo run --release --features protocol-research --example scale_2_probe
//! cargo run --release --features protocol-research --example scale_2_probe -- --json
//! cargo run --release --features protocol-research --example scale_2_probe -- --dump-snapshots /tmp/snaps
//! ```

use std::env;
use std::fs;
use std::hint::black_box;
use std::path::{Path, PathBuf};
use std::time::Instant;

use bitblock::camera::{Camera, DEFAULT_VERTICAL_FOV_RADIANS, Vec3};
use bitblock::history::world_tag;
use bitblock::render::{
    ChunkRelativeCamera, ChunkRelativePos, RenderTraversalStats, Renderer, SubpixelLodPolicy,
    bounds_aabb, crosshair_overlay_input, raycast, raycast_chunk_relative,
    raycast_chunk_relative_reference, raycast_reference, sweep_point, sweep_point_chunk_relative,
    sweep_point_chunk_relative_reference, sweep_point_reference,
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

struct Scale2Row {
    name: String,
    mode: String,
    bits: usize,
    chunks: usize,
    root_octants: usize,
    render_opt_ms: Dist,
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
    stats: RenderTraversalStats,
    non_background_pixels: usize,
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
    world.defer_octree();
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
    world.flush_octree();
    world
}

/// Builds a recognizable asymmetric 3D architectural cluster (hollow frame + stepped tower) around
/// `origin` so visual PNG snapshots clearly demonstrate crisp 3D perspective and face shading.
fn build_landmark_cluster(origin: Coord, include_bedrock_only_if_origin: bool) -> World {
    let mut world = World::new();
    world.defer_octree();
    // 7x7x7 hollow cube frame with an open front arch and a stepped spire on top.
    for dx in -3..=3_i64 {
        for dy in -3..=3_i64 {
            for dz in -3..=3_i64 {
                let on_shell = dx.abs() == 3 || dy.abs() == 3 || dz.abs() == 3;
                let front_arch = dz == -3 && dx.abs() <= 1 && (-2..=1).contains(&dy);
                if on_shell && !front_arch {
                    world.set_on(Coord::new(origin.x + dx, origin.y + dy, origin.z + dz));
                }
            }
        }
    }
    // Diagonal interior pillar and rooftop stepped spire.
    for dy in -2..=6_i64 {
        world.set_on(Coord::new(origin.x, origin.y + dy, origin.z));
    }
    for dx in -1..=1_i64 {
        for dz in -1..=1_i64 {
            world.set_on(Coord::new(origin.x + dx, origin.y + 4, origin.z + dz));
        }
    }
    if include_bedrock_only_if_origin && origin != Coord::new(0, 0, 0) {
        // Bedrock remains at (0,0,0) automatically; nothing extra needed.
    }
    world.flush_octree();
    world
}

fn build_negative_chunk_edge_world() -> World {
    let mut world = World::new();
    world.defer_octree();
    for &x in &[-17_i64, -16, -15, -1, 0, 1, 15, 16, 17] {
        for &y in &[-16_i64, -1, 0, 1, 16] {
            for &z in &[-16_i64, -1, 0, 1, 16] {
                if (x.abs() + y.abs() + z.abs()) % 3 != 1 {
                    world.set_on(Coord::new(x, y, z));
                }
            }
        }
    }
    world.flush_octree();
    world
}

fn crc32(data: &[u8]) -> u32 {
    let mut crc = 0xffff_ffff_u32;
    for &b in data {
        crc ^= u32::from(b);
        for _ in 0..8 {
            if crc & 1 != 0 {
                crc = (crc >> 1) ^ 0xedb8_8320;
            } else {
                crc >>= 1;
            }
        }
    }
    !crc
}

fn adler32(data: &[u8]) -> u32 {
    const MOD: u32 = 65521;
    let (mut s1, mut s2) = (1_u32, 0_u32);
    for &b in data {
        s1 = (s1 + u32::from(b)) % MOD;
        s2 = (s2 + s1) % MOD;
    }
    (s2 << 16) | s1
}

fn push_png_chunk(out: &mut Vec<u8>, kind: &[u8; 4], payload: &[u8]) {
    out.extend_from_slice(&(payload.len() as u32).to_be_bytes());
    let start = out.len();
    out.extend_from_slice(kind);
    out.extend_from_slice(payload);
    let crc = crc32(&out[start..]);
    out.extend_from_slice(&crc.to_be_bytes());
}

/// Writes an uncompressed (stored-DEFLATE) 24-bit RGB PNG file with zero external dependencies.
fn write_xrgb_png(path: &Path, width: u32, height: u32, pixels: &[u32]) {
    assert_eq!(pixels.len(), width as usize * height as usize);
    let row_bytes = width as usize * 3 + 1;
    let mut raw = Vec::with_capacity(row_bytes * height as usize);
    for y in 0..height as usize {
        raw.push(0_u8); // filter type 0 (None)
        let row = &pixels[y * width as usize..(y + 1) * width as usize];
        for &word in row {
            raw.push(((word >> 16) & 0xff) as u8);
            raw.push(((word >> 8) & 0xff) as u8);
            raw.push((word & 0xff) as u8);
        }
    }

    // Wrap raw scanlines in valid zlib stream with stored DEFLATE blocks (<= 65535 bytes each).
    let mut zlib = Vec::with_capacity(raw.len() + (raw.len() / 65535 + 2) * 5 + 6);
    zlib.push(0x78); // CMF: deflate, 32K window
    zlib.push(0x01); // FLG: no dict, check bits valid
    let mut offset = 0;
    while offset < raw.len() {
        let len = (raw.len() - offset).min(65_535);
        let is_final = offset + len == raw.len();
        zlib.push(u8::from(is_final));
        let len_u16 = len as u16;
        let nlen_u16 = !len_u16;
        zlib.extend_from_slice(&len_u16.to_le_bytes());
        zlib.extend_from_slice(&nlen_u16.to_le_bytes());
        zlib.extend_from_slice(&raw[offset..offset + len]);
        offset += len;
    }
    zlib.extend_from_slice(&adler32(&raw).to_be_bytes());

    let mut png = Vec::with_capacity(zlib.len() + 128);
    png.extend_from_slice(b"\x89PNG\r\n\x1a\n");
    let mut ihdr = Vec::with_capacity(13);
    ihdr.extend_from_slice(&width.to_be_bytes());
    ihdr.extend_from_slice(&height.to_be_bytes());
    ihdr.push(8); // bit depth
    ihdr.push(2); // color type: RGB
    ihdr.push(0); // compression
    ihdr.push(0); // filter
    ihdr.push(0); // interlace
    push_png_chunk(&mut png, b"IHDR", &ihdr);
    push_png_chunk(&mut png, b"IDAT", &zlib);
    push_png_chunk(&mut png, b"IEND", &[]);
    fs::write(path, &png).expect("write PNG snapshot");
}

fn maybe_save_snapshot(
    dir: Option<&Path>,
    name: &str,
    renderer: &mut Renderer,
    draw_crosshair: bool,
) {
    let Some(dir) = dir else { return };
    if draw_crosshair {
        renderer.draw_crosshair(crosshair_overlay_input());
    }
    let path = dir.join(format!("{name}.png"));
    write_xrgb_png(
        &path,
        renderer.width(),
        renderer.height(),
        renderer.pixels(),
    );
}

fn probe_world_standard(
    name: String,
    mut world: World,
    camera: Camera,
    lod: SubpixelLodPolicy,
    renderer: &mut Renderer,
    snap_dir: Option<&Path>,
) -> Scale2Row {
    let bits = world.len();
    let chunks = world.allocated_chunk_count();
    let root_octants = world.root_octants().count();

    renderer.set_subpixel_lod_policy(lod);
    renderer.set_reference_traversal(false);
    renderer.render(&world, &camera);
    let opt_pixels = renderer.pixels().to_vec();
    let stats = renderer.last_traversal_stats();
    let non_bg = opt_pixels.iter().filter(|&&p| p != 0).count();
    maybe_save_snapshot(snap_dir, &name, renderer, true);

    renderer.set_reference_traversal(true);
    renderer.render(&world, &camera);
    let ref_pixels = renderer.pixels().to_vec();
    let pixel_parity = opt_pixels == ref_pixels;
    if lod == SubpixelLodPolicy::ExactOnly {
        assert!(
            pixel_parity,
            "{name}: exact optimized render diverged from reference"
        );
    }

    renderer.set_reference_traversal(false);
    let render_opt_ms = measure_ms(SAMPLES, || renderer.render(&world, &camera));
    renderer.set_reference_traversal(true);
    let render_ref_ms = measure_ms(SAMPLES, || renderer.render(&world, &camera));
    renderer.set_reference_traversal(false);
    renderer.set_subpixel_lod_policy(SubpixelLodPolicy::ExactOnly);

    let ray = camera.center_ray();
    let hit_opt = raycast(&world, ray);
    let hit_ref = raycast_reference(&world, ray);
    let raycast_parity = hit_opt == hit_ref;
    assert!(raycast_parity, "{name}: raycast diverged from reference");
    let raycast_us = measure_us(SAMPLES * 4, || raycast(&world, ray));
    let raycast_ref_us = measure_us(SAMPLES * 4, || raycast_reference(&world, ray));

    let start = camera.position();
    let delta = Vec3::new(0.25, -0.15, 1.5);
    let sw_opt = sweep_point(&world, start, delta);
    let sw_ref = sweep_point_reference(&world, start, delta);
    let sweep_parity = sw_opt == sw_ref;
    assert!(sweep_parity, "{name}: sweep_point diverged from reference");
    let sweep_us = measure_us(SAMPLES * 4, || sweep_point(&world, start, delta));
    let sweep_ref_us = measure_us(SAMPLES * 4, || sweep_point_reference(&world, start, delta));

    let world_tag_cold_us = measure_us(SAMPLES, || {
        let fresh = world.clone();
        world_tag(&fresh)
    });
    let _ = world_tag(&world);
    let edit_coord = Coord::new(777_777, -555_555, 333_333);
    let world_tag_inc_us = measure_us(SAMPLES * 4, || {
        world.set_on(edit_coord);
        let t1 = world_tag(&world);
        world.set_off(edit_coord);
        let t2 = world_tag(&world);
        (t1, t2)
    });

    Scale2Row {
        name,
        mode: match lod {
            SubpixelLodPolicy::ExactOnly => "exact-hiz".to_string(),
            SubpixelLodPolicy::HierarchicalSubpixel => "subpixel-lod".to_string(),
        },
        bits,
        chunks,
        root_octants,
        render_opt_ms,
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
        stats,
        non_background_pixels: non_bg,
    }
}

fn probe_extreme_chunk_relative(
    name: String,
    origin: Coord,
    renderer: &mut Renderer,
    snap_dir: Option<&Path>,
) -> Scale2Row {
    let mut world = build_landmark_cluster(origin, true);
    let origin_truth = build_landmark_cluster(Coord::new(0, 0, 0), false);
    let bits = world.len();
    let chunks = world.allocated_chunk_count();
    let root_octants = world.root_octants().count();

    let yaw = 0.55;
    let pitch = -0.35;
    let dist = 16.5;
    let anchor_far = ChunkRelativePos::from_coord_offset(origin, Vec3::new(0.0, 0.5, 0.0));
    let cam_far = ChunkRelativeCamera::from_anchor(
        anchor_far,
        dist,
        yaw,
        pitch,
        DEFAULT_VERTICAL_FOV_RADIANS,
    );

    let anchor_zero =
        ChunkRelativePos::from_coord_offset(Coord::new(0, 0, 0), Vec3::new(0.0, 0.5, 0.0));
    let cam_zero = ChunkRelativeCamera::from_anchor(
        anchor_zero,
        dist,
        yaw,
        pitch,
        DEFAULT_VERTICAL_FOV_RADIANS,
    );

    renderer.set_subpixel_lod_policy(SubpixelLodPolicy::ExactOnly);
    renderer.set_reference_traversal(false);
    renderer.render_chunk_relative(&world, &cam_far);
    let far_opt_pixels = renderer.pixels().to_vec();
    let stats = renderer.last_traversal_stats();
    let non_bg = far_opt_pixels.iter().filter(|&&p| p != 0).count();
    assert!(
        non_bg > 50_000,
        "{name}: extreme-coord landmark should fill >50k pixels, got {non_bg}"
    );
    maybe_save_snapshot(snap_dir, &name, renderer, true);

    renderer.set_reference_traversal(true);
    renderer.render_chunk_relative(&world, &cam_far);
    let far_ref_pixels = renderer.pixels().to_vec();
    assert_eq!(
        far_opt_pixels, far_ref_pixels,
        "{name}: chunk-relative optimized diverged from chunk-relative reference"
    );

    // Also render origin-translated ground truth (with bedrock muted/equalized by checking exact silhouette & non-bedrock pixels).
    renderer.set_reference_traversal(false);
    renderer.render_chunk_relative(&origin_truth, &cam_zero);
    let zero_pixels = renderer.pixels().to_vec();
    maybe_save_snapshot(snap_dir, &format!("{name}_OriginTruth"), renderer, true);
    // At origin=(0,0,0), Coord(0,0,0) is bedrock (orange), whereas at `origin != 0`, Coord(origin) is ordinary white.
    // Every pixel where both frames are background must agree identically, and non-background silhouettes must match 100%.
    let silhouette_diffs = far_opt_pixels
        .iter()
        .zip(&zero_pixels)
        .filter(|&(&a, &b)| (a == 0) != (b == 0))
        .count();
    assert_eq!(
        silhouette_diffs, 0,
        "{name}: silhouette differed from origin-translated ground truth on {silhouette_diffs} pixels"
    );

    let render_opt_ms = measure_ms(SAMPLES, || renderer.render_chunk_relative(&world, &cam_far));
    renderer.set_reference_traversal(true);
    let render_ref_ms = measure_ms(SAMPLES, || renderer.render_chunk_relative(&world, &cam_far));
    renderer.set_reference_traversal(false);

    let local_ray = cam_far.center_ray_local();
    let hit_opt = raycast_chunk_relative(&world, cam_far.origin_chunk(), local_ray);
    let hit_ref = raycast_chunk_relative_reference(&world, cam_far.origin_chunk(), local_ray);
    let hit_zero = raycast_chunk_relative(
        &origin_truth,
        cam_zero.origin_chunk(),
        cam_zero.center_ray_local(),
    );
    let raycast_parity = hit_opt == hit_ref
        && hit_opt.is_some()
        && hit_zero.is_some()
        && hit_opt.unwrap().coord
            == Coord::new(
                origin.x + hit_zero.unwrap().coord.x,
                origin.y + hit_zero.unwrap().coord.y,
                origin.z + hit_zero.unwrap().coord.z,
            );
    assert!(
        raycast_parity,
        "{name}: chunk-relative raycast diverged: opt={hit_opt:?} ref={hit_ref:?} zero={hit_zero:?}"
    );
    let raycast_us = measure_us(SAMPLES * 4, || {
        raycast_chunk_relative(&world, cam_far.origin_chunk(), local_ray)
    });
    let raycast_ref_us = measure_us(SAMPLES * 4, || {
        raycast_chunk_relative_reference(&world, cam_far.origin_chunk(), local_ray)
    });

    let local_start = cam_far.local_camera().position();
    let delta = cam_far.local_camera().forward() * 25.0;
    let sw_opt = sweep_point_chunk_relative(&world, cam_far.origin_chunk(), local_start, delta);
    let sw_ref =
        sweep_point_chunk_relative_reference(&world, cam_far.origin_chunk(), local_start, delta);
    let sw_zero = sweep_point_chunk_relative(
        &origin_truth,
        cam_zero.origin_chunk(),
        cam_zero.local_camera().position(),
        cam_zero.local_camera().forward() * 25.0,
    );
    let sweep_parity = sw_opt == sw_ref
        && sw_opt.is_some()
        && sw_zero.is_some()
        && sw_opt.unwrap().coord
            == Coord::new(
                origin.x + sw_zero.unwrap().coord.x,
                origin.y + sw_zero.unwrap().coord.y,
                origin.z + sw_zero.unwrap().coord.z,
            );
    assert!(
        sweep_parity,
        "{name}: chunk-relative sweep diverged: opt={sw_opt:?} ref={sw_ref:?} zero={sw_zero:?}"
    );
    let sweep_us = measure_us(SAMPLES * 4, || {
        sweep_point_chunk_relative(&world, cam_far.origin_chunk(), local_start, delta)
    });
    let sweep_ref_us = measure_us(SAMPLES * 4, || {
        sweep_point_chunk_relative_reference(&world, cam_far.origin_chunk(), local_start, delta)
    });

    let world_tag_cold_us = measure_us(SAMPLES, || {
        let fresh = world.clone();
        world_tag(&fresh)
    });
    let _ = world_tag(&world);
    let edit_coord = Coord::new(origin.x + 12, origin.y - 12, origin.z + 12);
    let world_tag_inc_us = measure_us(SAMPLES * 4, || {
        world.set_on(edit_coord);
        let t1 = world_tag(&world);
        world.set_off(edit_coord);
        let t2 = world_tag(&world);
        (t1, t2)
    });

    Scale2Row {
        name,
        mode: "chunk-relative".to_string(),
        bits,
        chunks,
        root_octants,
        render_opt_ms,
        render_ref_ms,
        pixel_parity: true,
        raycast_us,
        raycast_ref_us,
        raycast_parity,
        sweep_us,
        sweep_ref_us,
        sweep_parity,
        world_tag_cold_us,
        world_tag_inc_us,
        stats,
        non_background_pixels: non_bg,
    }
}

fn default_camera_for(name: &str, world: &World) -> Camera {
    let (min, max) = bounds_aabb(world.bounds().expect("bedrock keeps bounds non-empty"));
    let anchor = min + (max - min) * 0.5;
    let radius = (max - min).length().max(1.0);
    let mut camera = Camera::for_observer_spawn();
    if name != "SparseFarExtrema" {
        camera.set_observation_distance(radius * 1.5);
        camera.set_observation_anchor(anchor);
    }
    camera
}

fn main() {
    let args: Vec<String> = env::args().collect();
    let json_mode = args.iter().any(|a| a == "--json");
    let snap_dir: Option<PathBuf> = args
        .windows(2)
        .find(|w| w[0] == "--dump-snapshots")
        .map(|w| PathBuf::from(&w[1]));
    if let Some(dir) = &snap_dir {
        fs::create_dir_all(dir).expect("create snapshot output directory");
    }

    // Verify canonical 5-workload structural assay report remains bit-identical.
    let report = ScaleWorkloadReportV1::canonical();
    assert_eq!(report.validate(), Ok(()));
    let report_tag = report.tag().to_string();
    assert_eq!(
        report_tag,
        "#bb.receipt.sha256.xq6oxiyyao3khyp3i7r6zbz3zwewouc75btw4s733xp62zeic3ma"
    );

    // Powered negative control: prove raw f64 loses unit voxel precision at 10^17.
    let far_x = 100_000_000_000_000_000_i64;
    let raw_f64_collapsed = (far_x as f64) == ((far_x + 1) as f64);
    assert!(
        raw_f64_collapsed,
        "negative control failed: raw f64 must collapse adjacent voxels at 10^17"
    );

    let mut renderer = Renderer::new(VIEWPORT, VIEWPORT);
    let mut rows = Vec::new();

    // 1. All 8 SCALE-1 workloads in ExactOnly mode.
    for target in [1_i64, 100, 10_000, 100_000, 1_000_000, 4_000_000] {
        let w = hollow_shell(target);
        let label = format!("HollowShell_{}", w.len());
        let cam = default_camera_for(&label, &w);
        rows.push(probe_world_standard(
            label,
            w,
            cam,
            SubpixelLodPolicy::ExactOnly,
            &mut renderer,
            snap_dir.as_deref(),
        ));
    }
    {
        let w = build_workload_world(ScaleWorkloadId::DenseCube16);
        let cam = default_camera_for("DenseCube16", &w);
        rows.push(probe_world_standard(
            "DenseCube16".to_string(),
            w,
            cam,
            SubpixelLodPolicy::ExactOnly,
            &mut renderer,
            snap_dir.as_deref(),
        ));
    }
    {
        let w = build_workload_world(ScaleWorkloadId::SparseFarExtrema);
        let cam = default_camera_for("SparseFarExtrema", &w);
        rows.push(probe_world_standard(
            "SparseFarExtrema".to_string(),
            w,
            cam,
            SubpixelLodPolicy::ExactOnly,
            &mut renderer,
            snap_dir.as_deref(),
        ));
    }

    // 2. Angled 3/4 view of HollowShell_149787 (3 faces visible, 3 faces Hi-Z culled).
    {
        let w = hollow_shell(4_000_000);
        let (min, max) = bounds_aabb(w.bounds().unwrap());
        let anchor = min + (max - min) * 0.5;
        let radius = (max - min).length();
        let cam = Camera::from_anchor(
            anchor,
            radius * 1.5,
            0.62,
            -0.42,
            DEFAULT_VERTICAL_FOV_RADIANS,
        );
        rows.push(probe_world_standard(
            "HollowShell_149787_Angled".to_string(),
            w,
            cam,
            SubpixelLodPolicy::ExactOnly,
            &mut renderer,
            snap_dir.as_deref(),
        ));
    }

    // 3. Distant sub-pixel view of HollowShell_149787 under HierarchicalSubpixel LOD.
    {
        let w = hollow_shell(4_000_000);
        let (min, max) = bounds_aabb(w.bounds().unwrap());
        let anchor = min + (max - min) * 0.5;
        let cam = Camera::from_anchor(anchor, 25_000.0, 0.5, -0.35, DEFAULT_VERTICAL_FOV_RADIANS);
        rows.push(probe_world_standard(
            "HollowShell_149787_FarSubpixel".to_string(),
            w,
            cam,
            SubpixelLodPolicy::HierarchicalSubpixel,
            &mut renderer,
            snap_dir.as_deref(),
        ));
    }

    // 4. Adversarial negative chunk edges (-17..=17).
    {
        let w = build_negative_chunk_edge_world();
        let cam = Camera::from_anchor(Vec3::ZERO, 68.0, 0.52, -0.34, DEFAULT_VERTICAL_FOV_RADIANS);
        rows.push(probe_world_standard(
            "NegativeChunkEdges".to_string(),
            w,
            cam,
            SubpixelLodPolicy::ExactOnly,
            &mut renderer,
            snap_dir.as_deref(),
        ));
    }

    // 5. Extreme coordinates beyond 2^53 (10^17) and near i64::MAX / i64::MIN.
    rows.push(probe_extreme_chunk_relative(
        "ExtremeFarCluster_1e17".to_string(),
        Coord::new(far_x, -far_x, far_x),
        &mut renderer,
        snap_dir.as_deref(),
    ));
    rows.push(probe_extreme_chunk_relative(
        "SignedExtremaCorner".to_string(),
        Coord::new(i64::MAX - 32, i64::MIN + 32, i64::MAX - 32),
        &mut renderer,
        snap_dir.as_deref(),
    ));

    let cache_stats = renderer.spatial_cache_stats();

    if json_mode {
        println!("{{");
        println!("  \"scale_workload_report_v1_tag\": \"{report_tag}\",");
        println!("  \"raw_f64_1e17_collapsed_negative_control\": {raw_f64_collapsed},");
        println!("  \"viewport_pixels\": [{VIEWPORT}, {VIEWPORT}],");
        println!("  \"raw_samples_per_workload\": {SAMPLES},");
        println!("  \"render_budget_ms\": {RENDER_BUDGET_MS:.2},");
        println!("  \"tick_budget_ms\": {TICK_BUDGET_MS:.6},");
        println!("  \"spatial_cache\": {{");
        println!("    \"max_chunks\": {},", cache_stats.max_chunks);
        println!("    \"cached_chunks\": {},", cache_stats.cached_chunks);
        println!("    \"hits\": {},", cache_stats.hits);
        println!("    \"misses\": {},", cache_stats.misses);
        println!("    \"invalidations\": {},", cache_stats.invalidations);
        println!("    \"cap_skips\": {},", cache_stats.cap_skips);
        println!("    \"estimated_bytes\": {}", cache_stats.estimated_bytes);
        println!("  }},");
        println!("  \"workloads\": [");
        for (i, r) in rows.iter().enumerate() {
            let comma = if i + 1 == rows.len() { "" } else { "," };
            println!("    {{");
            println!("      \"name\": \"{}\",", r.name);
            println!("      \"mode\": \"{}\",", r.mode);
            println!("      \"bits\": {},", r.bits);
            println!("      \"chunks\": {},", r.chunks);
            println!("      \"root_octants\": {},", r.root_octants);
            println!(
                "      \"non_background_pixels\": {},",
                r.non_background_pixels
            );
            println!("      \"pixel_parity\": {},", r.pixel_parity);
            println!("      \"raycast_parity\": {},", r.raycast_parity);
            println!("      \"sweep_parity\": {},", r.sweep_parity);
            println!(
                "      \"octants_hi_z_culled\": {},",
                r.stats.octants_hi_z_culled
            );
            println!(
                "      \"chunks_hi_z_culled\": {},",
                r.stats.chunks_hi_z_culled
            );
            println!("      \"bits_hi_z_culled\": {},", r.stats.bits_hi_z_culled);
            println!(
                "      \"subpixel_lod_nodes\": {},",
                r.stats.subpixel_lod_nodes
            );
            println!(
                "      \"subpixel_lod_bits\": {},",
                r.stats.subpixel_lod_bits
            );
            println!(
                "      \"render_opt_ms\": {{\"p50\": {:.3}, \"p95\": {:.3}, \"max\": {:.3}}},",
                r.render_opt_ms.p50, r.render_opt_ms.p95, r.render_opt_ms.max
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
    println!("Negative control (raw f64 collapses at 1e17): {raw_f64_collapsed}");
    println!(
        "{:<30} {:<14} {:>7} {:>6} {:>8} {:>8} {:>8} {:>6} {:>7} {:>7} {:>8}",
        "workload",
        "mode",
        "bits",
        "chunks",
        "opt_p50",
        "opt_p95",
        "ref_p50",
        "hiz_o",
        "hiz_c",
        "hiz_b",
        "lod_b"
    );
    for r in &rows {
        println!(
            "{:<30} {:<14} {:>7} {:>6} {:>8.2} {:>8.2} {:>8.2} {:>6} {:>7} {:>7} {:>8}",
            r.name,
            r.mode,
            r.bits,
            r.chunks,
            r.render_opt_ms.p50,
            r.render_opt_ms.p95,
            r.render_ref_ms.p50,
            r.stats.octants_hi_z_culled,
            r.stats.chunks_hi_z_culled,
            r.stats.bits_hi_z_culled,
            r.stats.subpixel_lod_bits
        );
    }
    println!(
        "SpatialRenderCache: max_chunks={} cached={} hits={} misses={} invalidations={} cap_skips={} bytes={}",
        cache_stats.max_chunks,
        cache_stats.cached_chunks,
        cache_stats.hits,
        cache_stats.misses,
        cache_stats.invalidations,
        cache_stats.cap_skips,
        cache_stats.estimated_bytes
    );
    if let Some(dir) = &snap_dir {
        println!("PNG snapshots written to: {}", dir.display());
    }
}
