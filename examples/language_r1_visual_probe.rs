//! Visual renderer probe & PNG snapshot exporter for `LANGUAGE-R1` strict `+X/+Y/+Z` framed 3D
//! spatial programs.
//!
//! Encodes the canonical 6-instruction voxel program `[PushOne, PushOne, Add, Dup, Add, Halt]`
//! (`| | + || + #`) into a `World` beside permanent bedrock `(0,0,0)`, parses it back from raw
//! `{0, 1}` voxels, and renders crisp `801x801` front-elevation and 3/4-perspective PNG snapshots
//! via Bitblock's production `Renderer` for direct visual verification.
//!
//! ```text
//! cargo run --release --features protocol-research --example language_r1_visual_probe -- --dump-snapshots /tmp/lang_snaps
//! ```

use std::env;
use std::fs;
use std::path::{Path, PathBuf};

use bitblock::camera::{Camera, DEFAULT_VERTICAL_FOV_RADIANS, Vec3};
use bitblock::language_r1::{
    ParsedSpatialProgramV1, SpatialOpcodeV1, encode_program_to_world, parse_world_programs,
};
use bitblock::render::Renderer;
use bitblock::world::{Coord, World};

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
    let snap_dir: Option<PathBuf> = args
        .windows(2)
        .find(|w| w[0] == "--dump-snapshots")
        .map(|w| PathBuf::from(&w[1]));
    if let Some(dir) = &snap_dir {
        fs::create_dir_all(dir).expect("create snapshot dir");
    }

    let anchor = Coord::new(3, 0, 2);
    let program = ParsedSpatialProgramV1::new(
        3,
        anchor,
        vec![
            SpatialOpcodeV1::PushOne,
            SpatialOpcodeV1::PushOne,
            SpatialOpcodeV1::Add,
            SpatialOpcodeV1::Dup,
            SpatialOpcodeV1::Add,
            SpatialOpcodeV1::Halt,
        ],
    )
    .unwrap();
    let mut world = World::new();
    encode_program_to_world(&mut world, &program).unwrap();
    let (parsed, report) = parse_world_programs(&world, &[anchor]).unwrap();
    assert_eq!(parsed[0], program);

    let mut renderer = Renderer::new(VIEWPORT, VIEWPORT);
    // 1. Crisp front elevation looking directly at the 6 voxel glyphs (| | + || + #), L-fiducial, & bedrock
    let cam_front = Camera::from_anchor(
        Vec3::new(14.0, 2.5, 3.0),
        34.0,
        0.0,
        -0.06,
        DEFAULT_VERTICAL_FOV_RADIANS,
    );
    renderer.render(&world, &cam_front);
    if let Some(dir) = &snap_dir {
        write_xrgb_png(
            &dir.join("language_r1_front.png"),
            VIEWPORT,
            VIEWPORT,
            renderer.pixels(),
        );
    }

    // 2. Elevated 3/4 oblique view showing depth separation between z=0 L-fiducial/header & z=2 glyphs
    let cam_iso = Camera::from_anchor(
        Vec3::new(13.0, 2.5, 3.0),
        30.0,
        0.42,
        -0.34,
        DEFAULT_VERTICAL_FOV_RADIANS,
    );
    renderer.render(&world, &cam_iso);
    if let Some(dir) = &snap_dir {
        write_xrgb_png(
            &dir.join("language_r1_oblique.png"),
            VIEWPORT,
            VIEWPORT,
            renderer.pixels(),
        );
    }

    println!("LANGUAGE-R1 Visual Probe:");
    println!("  source_tag = {}", program.source_tag().unwrap());
    println!("  report_tag = {}", report.tag());
    println!("  voxels     = {} (including bedrock)", world.len());
    if let Some(dir) = &snap_dir {
        println!("  snapshots  = {}", dir.display());
    }
}
