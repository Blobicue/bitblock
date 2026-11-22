//! Visual benchmark & multi-tick PNG snapshot exporter for `RULE-1` (synchronous 3D Moore Life
//! `B5/S45` over Bitblock's 1-bit sparse `World`).
//!
//! Renders an 801x801 scene containing permanent bedrock `(0,0,0)` surrounded by a 6-cell 3D
//! octahedron still life, a pulsing 10-cell period-4 3D oscillator, and Carter Bays' 10-cell
//! period-4 3D glider translating diagonally `(0, +1, +1)` every 4 ticks across ticks
//! `t = 0, 1, 2, 4, 8, 12, 16, 20` using Bitblock's production `Renderer`, while verifying exact
//! `History` replay parity and measuring step/render latency.
//!
//! ```text
//! cargo run --release --features protocol-research --example rule_1_visual_probe
//! cargo run --release --features protocol-research --example rule_1_visual_probe -- --json
//! cargo run --release --features protocol-research --example rule_1_visual_probe -- --dump-snapshots /tmp/rule1_snaps
//! ```

use std::env;
use std::fs;
use std::hint::black_box;
use std::path::{Path, PathBuf};
use std::time::Instant;

use bitblock::camera::{Camera, DEFAULT_VERTICAL_FOV_RADIANS, Vec3};
use bitblock::history::{History, world_tag};
use bitblock::render::Renderer;
use bitblock::rule_1::{
    Rule1DescriptorV1, bays_4555_glider_coords, octahedron_still_life_coords,
    period_4_oscillator_coords, step_rule_1, step_rule_1_with_history,
};
use bitblock::world::{BEDROCK_COORD, Coord, World};

const VIEWPORT: u32 = 801;

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

fn write_xrgb_png(path: &Path, width: u32, height: u32, pixels: &[u32]) {
    assert_eq!(pixels.len(), width as usize * height as usize);
    let mut raw = Vec::with_capacity((width as usize * 3 + 1) * height as usize);
    for y in 0..height as usize {
        raw.push(0_u8);
        let row = &pixels[y * width as usize..(y + 1) * width as usize];
        for &word in row {
            raw.push(((word >> 16) & 0xff) as u8);
            raw.push(((word >> 8) & 0xff) as u8);
            raw.push((word & 0xff) as u8);
        }
    }
    let mut zlib = Vec::with_capacity(raw.len() + (raw.len() / 65535 + 2) * 5 + 6);
    zlib.push(0x78);
    zlib.push(0x01);
    let mut offset = 0;
    while offset < raw.len() {
        let len = (raw.len() - offset).min(65_535);
        let is_final = offset + len == raw.len();
        zlib.push(u8::from(is_final));
        let len_u16 = len as u16;
        zlib.extend_from_slice(&len_u16.to_le_bytes());
        zlib.extend_from_slice(&(!len_u16).to_le_bytes());
        zlib.extend_from_slice(&raw[offset..offset + len]);
        offset += len;
    }
    zlib.extend_from_slice(&adler32(&raw).to_be_bytes());

    let mut png = Vec::with_capacity(zlib.len() + 128);
    png.extend_from_slice(b"\x89PNG\r\n\x1a\n");
    let mut ihdr = Vec::with_capacity(13);
    ihdr.extend_from_slice(&width.to_be_bytes());
    ihdr.extend_from_slice(&height.to_be_bytes());
    ihdr.extend_from_slice(&[8, 2, 0, 0, 0]);
    push_png_chunk(&mut png, b"IHDR", &ihdr);
    push_png_chunk(&mut png, b"IDAT", &zlib);
    push_png_chunk(&mut png, b"IEND", &[]);
    fs::write(path, &png).expect("write PNG snapshot");
}

fn build_showcase_world() -> World {
    let mut world = World::new();
    // 1. 6-cell 3D octahedron still life placed cleanly beside permanent bedrock (0, 0, 0).
    for c in octahedron_still_life_coords(Coord::new(0, 0, 6)) {
        world.set_on(c);
    }
    // 2. Period-4 3D oscillator placed to the left (-8, 0, 0).
    for c in period_4_oscillator_coords(Coord::new(-8, 0, 0)) {
        world.set_on(c);
    }
    // 3. Carter Bays' 10-cell period-4 3D glider launching from (5, -2, -2),
    //    gliding cleanly along (0, +1, +1) every 4 ticks across 20 ticks!
    for c in bays_4555_glider_coords(Coord::new(5, -2, -2)) {
        world.set_on(c);
    }
    world
}

fn showcase_camera() -> Camera {
    // Crisp 3/4 elevated view framing bedrock+octahedron (center), oscillator (left),
    // and the Bays glider trajectory (right -> upper back-right) across ticks 0..=20.
    Camera::from_anchor(
        Vec3::new(0.0, 1.5, 2.0),
        24.0,
        0.16,
        -0.28,
        DEFAULT_VERTICAL_FOV_RADIANS,
    )
}

fn main() {
    let args: Vec<String> = env::args().collect();
    let json_mode = args.iter().any(|a| a == "--json");
    let snap_dir: Option<PathBuf> = args
        .windows(2)
        .find(|w| w[0] == "--dump-snapshots")
        .map(|w| PathBuf::from(&w[1]));
    if let Some(dir) = &snap_dir {
        fs::create_dir_all(dir).expect("create snapshot output dir");
    }

    let desc = Rule1DescriptorV1::bays_4555_default();
    let initial = build_showcase_world();
    let mut world = initial.clone();
    let mut hist_world = initial.clone();
    let mut history = History::new(&initial);

    let cam = showcase_camera();
    let mut renderer = Renderer::new(VIEWPORT, VIEWPORT);

    let capture_ticks: &[u64] = &[0, 1, 2, 4, 8, 12, 16, 20];
    let mut step_us_samples = Vec::new();
    let mut render_ms_samples = Vec::new();
    let mut tick_rows = Vec::new();

    // Render tick 0 initial state.
    let t_r0 = Instant::now();
    renderer.render(&world, &cam);
    render_ms_samples.push(t_r0.elapsed().as_nanos() as f64 / 1_000_000.0);
    if let Some(dir) = &snap_dir {
        write_xrgb_png(
            &dir.join("rule1_t00.png"),
            VIEWPORT,
            VIEWPORT,
            renderer.pixels(),
        );
    }
    tick_rows.push((
        0u64,
        world.len(),
        0u32,
        0u32,
        0u32,
        world_tag(&world).to_string(),
    ));

    for tick in 1..=20_u64 {
        let t0 = Instant::now();
        let receipt = step_rule_1(&mut world, &desc, tick).unwrap();
        step_us_samples.push(t0.elapsed().as_nanos() as f64 / 1_000.0);

        let hist_receipt =
            step_rule_1_with_history(&mut hist_world, &mut history, &desc, tick).unwrap();
        assert_eq!(
            receipt, hist_receipt,
            "direct vs History RULE-1 parity mismatch at tick {tick}"
        );
        assert_eq!(world_tag(&world), world_tag(&hist_world));

        if capture_ticks.contains(&tick) {
            let tr = Instant::now();
            renderer.render(&world, &cam);
            black_box(renderer.pixels().first().copied());
            render_ms_samples.push(tr.elapsed().as_nanos() as f64 / 1_000_000.0);
            if let Some(dir) = &snap_dir {
                write_xrgb_png(
                    &dir.join(format!("rule1_t{tick:02}.png")),
                    VIEWPORT,
                    VIEWPORT,
                    renderer.pixels(),
                );
            }
            tick_rows.push((
                tick,
                world.len(),
                receipt.candidates_evaluated,
                receipt.births,
                receipt.deaths,
                world_tag(&world).to_string(),
            ));
        }
    }

    // Verify glider displaced by exact (0, +5, +5) after 20 ticks (5 full period-4 cycles),
    // bedrock + octahedron still intact, and period-4 oscillator returned to exact phase 0!
    assert!(world.is_on(BEDROCK_COORD));
    for c in octahedron_still_life_coords(Coord::new(0, 0, 6)) {
        assert!(world.is_on(c), "octahedron cell lost: {c:?}");
    }
    for c in period_4_oscillator_coords(Coord::new(-8, 0, 0)) {
        assert!(
            world.is_on(c),
            "oscillator phase-0 cell missing at t=20: {c:?}"
        );
    }
    for c in bays_4555_glider_coords(Coord::new(5, 3, 3)) {
        assert!(
            world.is_on(c),
            "glider shifted (0,+5,+5) cell missing at t=20: {c:?}"
        );
    }
    assert_eq!(world.len(), 27); // 1 bedrock + 6 octahedron + 10 oscillator + 10 glider

    step_us_samples.sort_by(f64::total_cmp);
    render_ms_samples.sort_by(f64::total_cmp);
    let step_p50 = step_us_samples[step_us_samples.len() / 2];
    let step_max = *step_us_samples.last().unwrap();
    let render_p50 = render_ms_samples[render_ms_samples.len() / 2];
    let render_max = *render_ms_samples.last().unwrap();

    if json_mode {
        println!("{{");
        println!("  \"rule_descriptor_tag\": \"{}\",", desc.tag().unwrap());
        println!("  \"showcase_ticks\": 20,");
        println!("  \"final_live_cells\": {},", world.len());
        println!("  \"history_parity\": true,");
        println!("  \"glider_shift_20_ticks\": [0, 5, 5],");
        println!("  \"step_p50_us\": {step_p50:.2},");
        println!("  \"step_max_us\": {step_max:.2},");
        println!("  \"render_801x801_p50_ms\": {render_p50:.3},");
        println!("  \"render_801x801_max_ms\": {render_max:.3},");
        println!("  \"snapshots\": [");
        for (idx, (tick, bits, cand, births, deaths, wtag)) in tick_rows.iter().enumerate() {
            let comma = if idx + 1 == tick_rows.len() { "" } else { "," };
            println!(
                "    {{\"tick\": {tick}, \"bits\": {bits}, \"candidates\": {cand}, \"births\": {births}, \"deaths\": {deaths}, \"world_tag\": \"{wtag}\"}}{comma}"
            );
        }
        println!("  ]");
        println!("}}");
    } else {
        println!("RULE-1 B5/S45 Visual Showcase Probe (ticks=0..=20, 801x801):");
        println!("  descriptor tag: {}", desc.tag().unwrap());
        println!("  step latency:   p50={step_p50:.2} us, max={step_max:.2} us");
        println!("  render latency: p50={render_p50:.3} ms, max={render_max:.3} ms");
        println!(
            "  final bits:     {} (bedrock+octahedron=7, oscillator=10, glider@+0,+5,+5=10)",
            world.len()
        );
        if let Some(dir) = &snap_dir {
            println!("  PNG snapshots:  {}", dir.display());
        }
    }
}
