//! Release-mode 4-metric emergence assay probe & 3D PNG snapshot exporter for `EMERGENCE-R1`.
//!
//! Runs the frozen `EmergenceCorpusSpecV1::v1_default()` corpus (engineered `B5/S45` structures +
//! matched-density `8x8x8` deterministic soups across `B5/S45`, `B6/S45` subcritical, and `B4/S45`
//! supercritical control rules), prints structured JSON (`--json`), and exports `801x801` 3D PNG
//! snapshots across `t = 0, 4, 12, 20` for side-by-side visual contact-sheet verification.
//!
//! ```text
//! cargo run --release --features protocol-research --example emergence_r1_visual_probe -- --json --dump-snapshots /tmp/em_snaps
//! ```

use std::env;
use std::fs;
use std::path::{Path, PathBuf};

use bitblock::camera::{Camera, DEFAULT_VERTICAL_FOV_RADIANS, Vec3};
use bitblock::emergence_r1::{
    DEFAULT_EMERGENCE_SOUP_SEEDS, EmergenceCorpusSpecV1, RULE_CODE_B4S45_SUPERCRITICAL,
    RULE_CODE_B5S45, RULE_CODE_B6S45_SUBCRITICAL, generate_deterministic_soup, run_emergence_assay,
    step_moore_rule_bounded,
};
use bitblock::render::Renderer;

const VIEWPORT: u32 = 801;

fn crc32(data: &[u8]) -> u32 {
    let mut crc = 0xffff_ffff_u32;
    for &b in data {
        crc ^= u32::from(b);
        for _ in 0..8 {
            crc = if crc & 1 != 0 {
                (crc >> 1) ^ 0xedb8_8320
            } else {
                crc >> 1
            };
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
    out.extend_from_slice(&crc32(&out[start..]).to_be_bytes());
}

fn write_xrgb_png(path: &Path, width: u32, height: u32, pixels: &[u32]) {
    let mut raw = Vec::with_capacity((width as usize * 3 + 1) * height as usize);
    for y in 0..height as usize {
        raw.push(0_u8);
        for &word in &pixels[y * width as usize..(y + 1) * width as usize] {
            raw.push(((word >> 16) & 0xff) as u8);
            raw.push(((word >> 8) & 0xff) as u8);
            raw.push((word & 0xff) as u8);
        }
    }
    let mut zlib = Vec::with_capacity(raw.len() + (raw.len() / 65535 + 2) * 5 + 6);
    zlib.extend_from_slice(&[0x78, 0x01]);
    let mut offset = 0;
    while offset < raw.len() {
        let len = (raw.len() - offset).min(65_535);
        zlib.push(u8::from(offset + len == raw.len()));
        let lu = len as u16;
        zlib.extend_from_slice(&lu.to_le_bytes());
        zlib.extend_from_slice(&(!lu).to_le_bytes());
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

fn main() {
    let args: Vec<String> = env::args().collect();
    let json_mode = args.iter().any(|a| a == "--json");
    let snap_dir: Option<PathBuf> = args
        .windows(2)
        .find(|w| w[0] == "--dump-snapshots")
        .map(|w| PathBuf::from(&w[1]));

    let spec = EmergenceCorpusSpecV1::v1_default();
    let report = run_emergence_assay(&spec).unwrap();

    if let Some(dir) = &snap_dir {
        fs::create_dir_all(dir).expect("create snapshot dir");
        let mut renderer = Renderer::new(VIEWPORT, VIEWPORT);
        let cam = Camera::from_anchor(
            Vec3::new(16.0, 16.0, 16.0),
            26.0,
            0.55,
            -0.42,
            DEFAULT_VERTICAL_FOV_RADIANS,
        );
        let seed = DEFAULT_EMERGENCE_SOUP_SEEDS[0];
        for &(rule_code, tag_name) in &[
            (RULE_CODE_B5S45, "b5s45"),
            (RULE_CODE_B6S45_SUBCRITICAL, "b6s45"),
            (RULE_CODE_B4S45_SUPERCRITICAL, "b4s45"),
        ] {
            let mut w =
                generate_deterministic_soup(seed, spec.soup_box_edge, spec.soup_density_bps);
            for tick in 0..=spec.ticks {
                if matches!(tick, 0 | 4 | 12 | 20) {
                    renderer.render(&w, &cam);
                    write_xrgb_png(
                        &dir.join(format!("soup_{tag_name}_t{tick:02}.png")),
                        VIEWPORT,
                        VIEWPORT,
                        renderer.pixels(),
                    );
                }
                if tick < spec.ticks
                    && step_moore_rule_bounded(
                        &mut w,
                        rule_code,
                        spec.max_candidate_cells,
                        spec.max_mutations_per_tick,
                    )
                    .is_none()
                {
                    // Render final capped state for remaining requested frames so blow-up is visible
                    for rem in (tick + 1)..=spec.ticks {
                        if matches!(rem, 4 | 12 | 20) {
                            renderer.render(&w, &cam);
                            write_xrgb_png(
                                &dir.join(format!("soup_{tag_name}_t{rem:02}.png")),
                                VIEWPORT,
                                VIEWPORT,
                                renderer.pixels(),
                            );
                        }
                    }
                    break;
                }
            }
        }
    }

    if json_mode {
        println!("{{");
        println!("  \"corpus_spec_tag\": \"{}\",", spec.tag().unwrap());
        println!("  \"assay_report_tag\": \"{}\",", report.tag());
        println!("  \"trajectories\": [");
        for (i, t) in report.trajectories.iter().enumerate() {
            let comma = if i + 1 == report.trajectories.len() {
                ""
            } else {
                ","
            };
            println!(
                "    {{\"rule_code\": {}, \"subject_kind\": {}, \"seed\": {}, \"initial\": {}, \"final\": {}, \"peak\": {}, \"survival_ticks\": {}, \"active_ticks\": {}, \"cap_hit\": {}, \"morton_bps\": {}, \"hamming\": {}, \"centroid_l1_x100\": {}, \"period\": {}}}{}",
                t.rule_code,
                t.subject_kind,
                t.seed,
                t.initial_mutable_cells,
                t.final_mutable_cells,
                t.peak_mutable_cells,
                t.survival_ticks,
                t.active_mutation_ticks,
                t.cap_hit,
                t.morton_compress_bps,
                t.perturbation_hamming_t_end,
                t.centroid_l1_shift_x100,
                t.detected_period,
                comma
            );
        }
        println!("  ]");
        println!("}}");
    } else {
        println!("EMERGENCE-R1 Assay:");
        println!("  corpus_spec_tag  = {}", spec.tag().unwrap());
        println!("  assay_report_tag = {}", report.tag());
        for t in &report.trajectories {
            println!(
                "  rule={} subj={} seed={:#x} init={} fin={} peak={} surv={} act={} cap={} morton_bps={} ham={} shift_x100={} per={}",
                t.rule_code,
                t.subject_kind,
                t.seed,
                t.initial_mutable_cells,
                t.final_mutable_cells,
                t.peak_mutable_cells,
                t.survival_ticks,
                t.active_mutation_ticks,
                t.cap_hit,
                t.morton_compress_bps,
                t.perturbation_hamming_t_end,
                t.centroid_l1_shift_x100,
                t.detected_period
            );
        }
    }
}
