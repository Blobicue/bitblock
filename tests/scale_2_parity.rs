//! Comprehensive CI integration tests for `SCALE-2` (`PS-052:SCALE-2@base`):
//!
//! 1. Chunk-relative coordinates (`ChunkRelativePos`, `ChunkRelativeCamera`) across negative
//!    chunk boundaries (`-17, -16, -15, -1, 0, 1, 15, 16, 17`), `±10^17` (beyond `2^53`), and
//!    `i64::MIN` / `i64::MAX` extrema, with a powered negative control for raw `f64` collapse.
//! 2. Hierarchical front-to-back octant/chunk Hi-Z occlusion culling parity against
//!    `Renderer::set_reference_traversal(true)` plus `render_tinted_delta` lazy unpacking when
//!    holes expose previously Hi-Z-culled chunks.
//! 3. Ephemeral sub-pixel hierarchical LOD (`SubpixelLodPolicy::HierarchicalSubpixel`): exact
//!    bit-identical agreement above the one-pixel saturation threshold, O(1) sub-pixel collapse
//!    at far distance, legal palette conformance, and `world_tag` invariance.
//! 4. Bounded ephemeral `SpatialRenderCache`: cache hits on static worlds, exact invalidation on
//!    every `set_on`/`set_off` edit and world switch (even at equal `revision()`), and hard
//!    chunk-count ceiling enforcement.

use bitblock::camera::{Camera, DEFAULT_VERTICAL_FOV_RADIANS, Ray, Vec3};
use bitblock::history::world_tag;
use bitblock::render::{
    ChunkRelativeCamera, ChunkRelativePos, Renderer, SubpixelLodPolicy, bounds_aabb,
    crosshair_overlay_input, raycast, raycast_chunk_relative, raycast_chunk_relative_reference,
    raycast_reference, sweep_point, sweep_point_chunk_relative,
    sweep_point_chunk_relative_reference, sweep_point_reference,
};
use bitblock::world::{ChunkCoord, Coord, World};

fn landmark_cluster(origin: Coord) -> World {
    let mut world = World::new();
    world.defer_octree();
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
    for dy in -2..=5_i64 {
        world.set_on(Coord::new(origin.x, origin.y + dy, origin.z));
    }
    world.flush_octree();
    world
}

fn hollow_shell(side: i64) -> World {
    let mut world = World::new();
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

fn solid_cube(radius: i64) -> World {
    let mut world = World::new();
    world.defer_octree();
    for x in -radius..=radius {
        for y in -radius..=radius {
            for z in -radius..=radius {
                world.set_on(Coord::new(x, y, z));
            }
        }
    }
    world.flush_octree();
    world
}

#[test]
fn chunk_relative_pos_normalization_and_containing_coord_cover_i64_extrema() {
    // Negative & positive chunk boundaries (-17..17)
    for x in [-17_i64, -16, -15, -1, 0, 1, 15, 16, 17] {
        let p =
            ChunkRelativePos::from_coord_offset(Coord::new(x, -x, x), Vec3::new(0.25, -0.25, 0.0));
        assert!(
            p.local.x >= 0.0 && p.local.x < 16.0,
            "local.x out of range: {p:?}"
        );
        assert!(
            p.local.y >= 0.0 && p.local.y < 16.0,
            "local.y out of range: {p:?}"
        );
        assert!(
            p.local.z >= 0.0 && p.local.z < 16.0,
            "local.z out of range: {p:?}"
        );
        assert_eq!(p.containing_coord(), Some(Coord::new(x, -x, x)));
    }

    // Tiny negative sub-ULP offset near 0 must normalize cleanly into [0.0, 16.0), never 16.0.
    let tiny = ChunkRelativePos::new(
        ChunkCoord::new(5, -5, 0),
        Vec3::new(-1.0e-17, -1.0e-17, 16.0),
    );
    assert!(tiny.local.x >= 0.0 && tiny.local.x < 16.0);
    assert!(tiny.local.y >= 0.0 && tiny.local.y < 16.0);
    assert_eq!(tiny.chunk.z, 1);
    assert_eq!(tiny.local.z, 0.0);

    // World-space conversion & exact i128 chunk-offset round-trip.
    let from_vec = ChunkRelativePos::from_world_vec3(Vec3::new(-17.25, 32.5, 15.75));
    assert_eq!(from_vec.chunk, ChunkCoord::new(-2, 2, 0));
    let rel = from_vec.offset_from_chunk(ChunkCoord::new(-1, 1, 0));
    assert!((rel.x - (-1.25)).abs() < 1.0e-12);
    assert!((rel.y - 16.5).abs() < 1.0e-12);
    assert!((rel.z - 15.75).abs() < 1.0e-12);
    let moved = from_vec.translated(Vec3::new(17.25, -32.5, -15.75));
    assert_eq!(moved.chunk, ChunkCoord::new(0, 0, 0));
    assert!(moved.local.length() < 1.0e-12);

    // Extreme signed i64 boundaries
    let max_in = ChunkRelativePos::from_coord_offset(
        Coord::new(i64::MAX, i64::MIN, 100_000_000_000_000_000),
        Vec3::new(0.4, -0.4, 0.25),
    );
    assert_eq!(
        max_in.containing_coord(),
        Some(Coord::new(i64::MAX, i64::MIN, 100_000_000_000_000_000))
    );

    // Stepping 0.51 past i64::MAX or -0.51 past i64::MIN must return None cleanly (never wrap/saturate).
    let past_max =
        ChunkRelativePos::from_coord_offset(Coord::new(i64::MAX, 0, 0), Vec3::new(0.51, 0.0, 0.0));
    assert_eq!(past_max.containing_coord(), None);
    let past_min =
        ChunkRelativePos::from_coord_offset(Coord::new(0, i64::MIN, 0), Vec3::new(0.0, -0.51, 0.0));
    assert_eq!(past_min.containing_coord(), None);
}

#[test]
fn chunk_relative_render_raycast_and_sweep_match_origin_truth_at_1e17_and_i64_extrema() {
    // Powered negative control: prove raw f64 collapses adjacent unit voxels at 10^17.
    let far: i64 = 100_000_000_000_000_000;
    assert_eq!(
        far as f64,
        (far + 1) as f64,
        "negative control: raw f64 must collapse adjacent voxels at 10^17"
    );

    let origin_truth = landmark_cluster(Coord::new(0, 0, 0));
    let anchor_zero =
        ChunkRelativePos::from_coord_offset(Coord::new(0, 0, 0), Vec3::new(0.0, 0.5, 0.0));
    let cam_zero = ChunkRelativeCamera::from_anchor(
        anchor_zero,
        16.5,
        0.55,
        -0.35,
        DEFAULT_VERTICAL_FOV_RADIANS,
    );
    let mut r_zero = Renderer::new(201, 201);
    r_zero.render_chunk_relative(&origin_truth, &cam_zero);
    let zero_hit = raycast_chunk_relative(
        &origin_truth,
        cam_zero.origin_chunk(),
        cam_zero.center_ray_local(),
    )
    .expect("origin cluster must be hit by center ray");
    let zero_sweep = sweep_point_chunk_relative(
        &origin_truth,
        cam_zero.origin_chunk(),
        cam_zero.local_camera().position(),
        cam_zero.local_camera().forward() * 25.0,
    )
    .expect("origin cluster must be hit by forward sweep");

    for far_origin in [
        Coord::new(far, -far, far),
        Coord::new(i64::MAX - 32, i64::MIN + 32, i64::MAX - 32),
        Coord::new(i64::MIN + 32, i64::MAX - 32, -far),
    ] {
        let world = landmark_cluster(far_origin);
        let anchor_far = ChunkRelativePos::from_coord_offset(far_origin, Vec3::new(0.0, 0.5, 0.0));
        let mut cam_far = ChunkRelativeCamera::from_anchor(
            anchor_far,
            16.5,
            0.55,
            -0.35,
            DEFAULT_VERTICAL_FOV_RADIANS,
        );

        let mut opt = Renderer::new(201, 201);
        opt.render_chunk_relative(&world, &cam_far);
        let mut reference = Renderer::new(201, 201);
        reference.set_reference_traversal(true);
        reference.render_chunk_relative(&world, &cam_far);
        assert_eq!(
            opt.pixels(),
            reference.pixels(),
            "chunk-relative optimized vs reference diverged at {far_origin:?}"
        );

        // Exact silhouette agreement with origin-translated ground truth.
        let silhouette_diffs = opt
            .pixels()
            .iter()
            .zip(r_zero.pixels())
            .filter(|&(&a, &b)| (a == 0) != (b == 0))
            .count();
        assert_eq!(
            silhouette_diffs, 0,
            "silhouette diverged from origin truth at {far_origin:?}"
        );

        // Exact raycast parity (optimized vs reference AND translated origin truth).
        let local_ray = cam_far.center_ray_local();
        let hit_opt = raycast_chunk_relative(&world, cam_far.origin_chunk(), local_ray).unwrap();
        let hit_ref =
            raycast_chunk_relative_reference(&world, cam_far.origin_chunk(), local_ray).unwrap();
        assert_eq!(hit_opt, hit_ref);
        assert_eq!(
            hit_opt.coord,
            Coord::new(
                far_origin.x + zero_hit.coord.x,
                far_origin.y + zero_hit.coord.y,
                far_origin.z + zero_hit.coord.z,
            )
        );
        assert!((hit_opt.distance - zero_hit.distance).abs() < 1.0e-11);

        // Exact sweep-point collision parity.
        let local_start = cam_far.local_camera().position();
        let delta = cam_far.local_camera().forward() * 25.0;
        let sw_opt =
            sweep_point_chunk_relative(&world, cam_far.origin_chunk(), local_start, delta).unwrap();
        let sw_ref = sweep_point_chunk_relative_reference(
            &world,
            cam_far.origin_chunk(),
            local_start,
            delta,
        )
        .unwrap();
        assert_eq!(sw_opt, sw_ref);
        assert_eq!(
            sw_opt.coord,
            Coord::new(
                far_origin.x + zero_sweep.coord.x,
                far_origin.y + zero_sweep.coord.y,
                far_origin.z + zero_sweep.coord.z,
            )
        );
        assert!((sw_opt.distance - zero_sweep.distance).abs() < 1.0e-11);

        // Camera translation & rotation across chunk boundary at extreme coordinate keeps local eye in [0, 16)^3.
        let prev_chunk = cam_far.origin_chunk();
        let anchor_before = cam_far.anchor_position();
        assert_eq!(
            anchor_before.containing_coord(),
            Some(Coord::new(far_origin.x, far_origin.y + 1, far_origin.z))
        );
        let tint_fn = |c: Coord| {
            Some(if (c.x + c.y + c.z).rem_euclid(2) == 0 {
                0x002f6fd6
            } else {
                0x00b04020
            })
        };
        let mut tinted_opt = Renderer::new(101, 101);
        tinted_opt.render_tinted_chunk_relative(&world, &cam_far, Some(&tint_fn));
        let mut tinted_ref = Renderer::new(101, 101);
        tinted_ref.set_reference_traversal(true);
        tinted_ref.render_tinted_chunk_relative(&world, &cam_far, Some(&tint_fn));
        assert_eq!(
            tinted_opt.pixels(),
            tinted_ref.pixels(),
            "tinted chunk-relative render diverged at {far_origin:?}"
        );
        cam_far.rotate(0.05, -0.02);
        cam_far.translate(Vec3::new(1.0, 0.0, 2.0));
        cam_far.translate_world(Vec3::new(32.0, -32.0, 16.0));
        assert_ne!(cam_far.origin_chunk(), prev_chunk);
        let eye = cam_far.eye_position();
        assert!(eye.local.x >= 0.0 && eye.local.x < 16.0);
        assert!(eye.local.y >= 0.0 && eye.local.y < 16.0);
        assert!(eye.local.z >= 0.0 && eye.local.z < 16.0);

        // Camera placed inside an occupied bit at extreme coordinate fills the whole viewport.
        let inside_pos = ChunkRelativePos::from_coord_offset(
            Coord::new(far_origin.x, far_origin.y + 4, far_origin.z),
            Vec3::ZERO,
        );
        let inside_cam = ChunkRelativeCamera::from_position(
            inside_pos,
            0.0,
            0.0,
            1.0,
            DEFAULT_VERTICAL_FOV_RADIANS,
        );
        let mut inside_r = Renderer::new(17, 17);
        inside_r.render_chunk_relative(&world, &inside_cam);
        assert_eq!(inside_r.world_pixel_count(), 17 * 17);
    }
}

#[test]
fn negative_chunk_edges_preserve_exact_render_raycast_sweep_and_edit_parity() {
    let mut world = World::new();
    for &x in &[-17_i64, -16, -15, -1, 0, 1, 15, 16, 17] {
        for &y in &[-17_i64, -16, -1, 0, 1, 16, 17] {
            for &z in &[-16_i64, -1, 0, 1, 16] {
                if (x + 2 * y + 3 * z).rem_euclid(5) < 2 {
                    world.set_on(Coord::new(x, y, z));
                }
            }
        }
    }

    let cam = Camera::from_anchor(Vec3::ZERO, 64.0, 0.48, -0.32, DEFAULT_VERTICAL_FOV_RADIANS);
    let cam_rel = ChunkRelativeCamera::from_world_camera(&cam);

    let mut r_opt = Renderer::new(201, 201);
    let mut r_ref = Renderer::new(201, 201);
    let mut r_rel = Renderer::new(201, 201);
    r_ref.set_reference_traversal(true);

    for step in 0..3 {
        if step == 1 {
            world.set_on(Coord::new(-16, -16, -16));
            world.set_off(Coord::new(16, 16, 16));
        } else if step == 2 {
            world.set_off(Coord::new(-1, -1, -1));
            world.set_on(Coord::new(17, -17, 15));
        }

        r_opt.render(&world, &cam);
        r_ref.render(&world, &cam);
        r_rel.render_chunk_relative(&world, &cam_rel);

        assert_eq!(r_opt.pixels(), r_ref.pixels(), "step {step}: opt vs ref");
        assert_eq!(r_rel.pixels(), r_ref.pixels(), "step {step}: rel vs ref");

        for &target in &[
            Coord::new(-17, -16, -1),
            Coord::new(-16, -16, -16),
            Coord::new(0, 0, 0),
            Coord::new(15, 16, 16),
            Coord::new(17, -17, 15),
        ] {
            let origin = Vec3::new(-31.25, 19.5, -42.75);
            let target_v = Vec3::new(target.x as f64, target.y as f64, target.z as f64);
            let delta = target_v - origin;
            let ray = Ray {
                origin,
                direction: delta.normalized(),
            };
            let h_opt = raycast(&world, ray);
            let h_ref = raycast_reference(&world, ray);
            let h_rel = raycast_chunk_relative(&world, ChunkCoord::new(0, 0, 0), ray);
            assert_eq!(h_opt, h_ref, "step {step} raycast opt vs ref");
            assert_eq!(h_rel, h_ref, "step {step} raycast rel vs ref");

            let s_opt = sweep_point(&world, origin, delta * 1.1);
            let s_ref = sweep_point_reference(&world, origin, delta * 1.1);
            let s_rel =
                sweep_point_chunk_relative(&world, ChunkCoord::new(0, 0, 0), origin, delta * 1.1);
            assert_eq!(s_opt, s_ref, "step {step} sweep opt vs ref");
            assert_eq!(s_rel, s_ref, "step {step} sweep rel vs ref");
        }
    }
}

#[test]
fn hierarchical_hi_z_culling_culls_occluded_octants_and_matches_reference_bit_for_bit() {
    let mut world = hollow_shell(36);
    let (min, max) = bounds_aabb(world.bounds().unwrap());
    let anchor = min + (max - min) * 0.5;
    let radius = (max - min).length();
    let frontal = Camera::from_anchor(anchor, radius * 1.5, 0.0, 0.0, DEFAULT_VERTICAL_FOV_RADIANS);
    let angled = Camera::from_anchor(
        anchor,
        radius * 1.5,
        0.65,
        -0.4,
        DEFAULT_VERTICAL_FOV_RADIANS,
    );

    for (label, cam) in [("frontal", &frontal), ("angled", &angled)] {
        let mut opt = Renderer::new(241, 241);
        opt.render(&world, cam);
        let stats = opt.last_traversal_stats();
        assert!(
            stats.octants_hi_z_culled + stats.chunks_hi_z_culled > 0,
            "{label}: expected hierarchical Hi-Z culling to reject occluded subtrees: {stats:?}"
        );
        assert!(
            stats.bits_hi_z_culled > 1_000,
            "{label}: expected >1000 bits Hi-Z culled, got {}",
            stats.bits_hi_z_culled
        );

        let mut reference = Renderer::new(241, 241);
        reference.set_reference_traversal(true);
        reference.render(&world, cam);

        assert_eq!(opt.pixels(), reference.pixels(), "{label}: pixels diverged");
        assert_eq!(
            opt.world_pixel_count(),
            reference.world_pixel_count(),
            "{label}: world_pixel_count diverged"
        );
        assert_eq!(
            opt.observe_final_game_frame().0.tag,
            reference.observe_final_game_frame().0.tag,
            "{label}: canonical frame tag diverged"
        );
    }

    // Exercise render_tinted_delta unpacking Hi-Z occluded back-wall chunks when drilling a front hole!
    let mut delta_r = Renderer::new(241, 241);
    delta_r.render_tinted(&world, &frontal, None);
    assert!(delta_r.last_traversal_stats().bits_hi_z_culled > 2_000);

    let mut changed = Vec::new();
    for x in -3..=3_i64 {
        for y in -3..=3_i64 {
            let front = Coord::new(x, y, -18);
            if world.set_off(front) {
                changed.push((front, true));
            }
        }
    }
    assert!(!changed.is_empty());
    let used_delta = delta_r.render_tinted_delta(&world, &frontal, None, &changed);
    assert!(used_delta, "hole drill should stay on delta path");

    let mut full_r = Renderer::new(241, 241);
    full_r.render_tinted(&world, &frontal, None);
    assert_eq!(
        delta_r.pixels(),
        full_r.pixels(),
        "delta render after uncovering Hi-Z occluded chunks must match full render bit-for-bit"
    );
    assert_eq!(delta_r.world_pixel_count(), full_r.world_pixel_count());
}

#[test]
fn subpixel_hierarchical_lod_agrees_above_threshold_and_collapses_distant_octants() {
    let world = hollow_shell(36);
    let tag_before = world_tag(&world);
    let (min, max) = bounds_aabb(world.bounds().unwrap());
    let anchor = min + (max - min) * 0.5;
    let radius = (max - min).length();

    // 1. Above the 1-pixel saturation threshold: HierarchicalSubpixel must match ExactOnly & reference bit-for-bit.
    let near_cam = Camera::from_anchor(
        anchor,
        radius * 1.4,
        0.5,
        -0.35,
        DEFAULT_VERTICAL_FOV_RADIANS,
    );
    let mut exact = Renderer::new(241, 241);
    exact.render(&world, &near_cam);
    let mut lod = Renderer::new(241, 241);
    lod.set_subpixel_lod_policy(SubpixelLodPolicy::HierarchicalSubpixel);
    assert_eq!(
        lod.subpixel_lod_policy(),
        SubpixelLodPolicy::HierarchicalSubpixel
    );
    lod.render(&world, &near_cam);
    assert_eq!(
        lod.last_traversal_stats().subpixel_lod_nodes,
        0,
        "above-threshold view must not trigger sub-pixel collapse"
    );
    assert_eq!(lod.pixels(), exact.pixels());

    // 2. Far distance where whole world spans >1 px (~3 px) while individual chunks/octants
    //    project <= 1.0 px: HierarchicalSubpixel collapses those subtrees in O(1).
    let far_cam = Camera::from_anchor(anchor, 5_000.0, 0.5, -0.35, DEFAULT_VERTICAL_FOV_RADIANS);
    lod.render(&world, &far_cam);
    let far_stats = lod.last_traversal_stats();
    assert!(
        far_stats.subpixel_lod_nodes > 0 && far_stats.subpixel_lod_bits > 1_000,
        "expected sub-pixel LOD collapse at far distance, got {far_stats:?}"
    );
    assert!(lod.world_pixel_count() > 0);
    lod.draw_crosshair(crosshair_overlay_input());
    let (_, audit) = lod.observe_final_game_frame();
    assert!(
        audit.is_valid(),
        "sub-pixel LOD emitted illegal palette color: {audit:?}"
    );

    // 3. Authenticated world_tag must remain completely unchanged.
    assert_eq!(world_tag(&world), tag_before);
}

#[test]
fn spatial_render_cache_enforces_hard_cap_and_invalidates_immediately_on_edit_or_world_switch() {
    let mut world_a = solid_cube(10); // spans 8 chunks around origin
    let world_b = solid_cube(10); // distinct World instance with IDENTICAL initial revision!
    assert_eq!(world_a.revision(), world_b.revision());

    let cam = Camera::from_anchor(Vec3::ZERO, 40.0, 0.4, -0.25, DEFAULT_VERTICAL_FOV_RADIANS);
    let mut r = Renderer::new(181, 181);

    // Render 1: populates cache
    r.render(&world_a, &cam);
    let s1 = r.spatial_cache_stats();
    assert!(s1.misses > 0, "first render must populate cache: {s1:?}");
    assert_eq!(s1.hits, 0);
    assert!(s1.cached_chunks > 0 && s1.cached_chunks <= s1.max_chunks);

    // Render 2 on unchanged world_a: pure cache hits
    r.render(&world_a, &cam);
    let s2 = r.spatial_cache_stats();
    assert!(
        s2.hits > 0,
        "second render on static world must hit cache: {s2:?}"
    );
    assert_eq!(s2.misses, s1.misses);
    assert_eq!(s2.invalidations, s1.invalidations);

    // Edit world_a -> must invalidate immediately and agree with reference!
    assert!(world_a.set_off(Coord::new(10, 0, 0)));
    r.render(&world_a, &cam);
    let s3 = r.spatial_cache_stats();
    assert_eq!(s3.invalidations, s2.invalidations + 1);
    let mut ref_r = Renderer::new(181, 181);
    ref_r.set_reference_traversal(true);
    ref_r.render(&world_a, &cam);
    assert_eq!(r.pixels(), ref_r.pixels());

    // Switch to distinct world_b (even if we bump world_b to the exact same revision!)
    let mut world_b = world_b;
    assert!(world_b.set_off(Coord::new(-10, 0, 0)));
    assert_eq!(world_a.revision(), world_b.revision());
    r.render(&world_b, &cam);
    let s4 = r.spatial_cache_stats();
    assert_eq!(
        s4.invalidations,
        s3.invalidations + 1,
        "switching worlds at equal revision must invalidate spatial cache"
    );
    ref_r.render(&world_b, &cam);
    assert_eq!(r.pixels(), ref_r.pixels());

    // Enforce hard memory/chunk cap (max_chunks = 2 on an 8-chunk world)
    r.set_spatial_cache_max_chunks(2);
    r.render(&world_b, &cam);
    let s5 = r.spatial_cache_stats();
    assert_eq!(s5.max_chunks, 2);
    assert!(s5.cached_chunks <= 2, "cache exceeded hard cap: {s5:?}");
    assert!(
        s5.cap_skips > 0,
        "expected cap_skips when chunks > max_chunks: {s5:?}"
    );
    assert_eq!(r.pixels(), ref_r.pixels());
}
