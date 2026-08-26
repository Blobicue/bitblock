//! Render one `.bb` program to a PNG without a window: the gallery camera.
//!
//! The interaction-model judgement needs artifacts, not opinions. This example
//! exercises the same public path the app uses — `Program::parse`, `World`,
//! `Camera`, `Renderer` — writes the 801x801 game frame out as a truecolor PNG,
//! and prints the camera it chose plus the on-bit count so every gallery image
//! in `docs/gallery/` is reproducible from its `.bb` source alone.
//!
//! Usage: `cargo run --release --example gallery_frame -- <program.bb> <out.png> [yaw-deg] [pitch-deg]`

use std::env;
use std::fs;
use std::io::Write;
use std::path::Path;

use bitblock::camera::{Camera, Vec3, DEFAULT_VERTICAL_FOV_RADIANS};
use bitblock::program::Program;
use bitblock::render::{bounds_aabb, Renderer};
use bitblock::world::World;

const SIZE: u32 = 801;

fn main() {
    let mut args = env::args().skip(1);
    let program_path =
        args.next().expect("usage: gallery_frame <program.bb> <out.png> [yaw-deg] [pitch-deg]");
    let out_path = args.next().expect("usage: gallery_frame <program.bb> <out.png> [yaw-deg] [pitch-deg]");
    let yaw_deg: f64 = args.next().map(|s| s.parse().expect("yaw degrees")).unwrap_or(35.0);
    let pitch_deg: f64 = args.next().map(|s| s.parse().expect("pitch degrees")).unwrap_or(-22.0);

    let source = fs::read_to_string(&program_path).expect("read program");
    let program = Program::parse(&source).expect("parse program");
    let mut world = World::new_with_origin();
    let applied = program.apply(&mut world);

    // Frame the world's own bounds, then pull back for a quiet margin.
    let (min, max) = bounds_aabb(world.bounds().expect("program left the world empty"));
    let center = Vec3::new(
        (min.x + max.x) / 2.0,
        (min.y + max.y) / 2.0,
        (min.z + max.z) / 2.0,
    );
    let radius =
        ((max.x - min.x).powi(2) + (max.y - min.y).powi(2) + (max.z - min.z).powi(2)).sqrt() / 2.0;
    let distance = (radius / (DEFAULT_VERTICAL_FOV_RADIANS / 2.0).tan()) * 1.28;
    let camera = Camera::from_anchor(
        center,
        distance,
        yaw_deg.to_radians(),
        pitch_deg.to_radians(),
        DEFAULT_VERTICAL_FOV_RADIANS,
    );

    let mut renderer = Renderer::new(SIZE, SIZE);
    renderer.render(&world, &camera);
    write_png(Path::new(&out_path), renderer.pixels(), SIZE, SIZE).expect("write png");

    println!(
        "program={}\tapplied={}\ton_bits={}\tcamera=({:.2},{:.2},{:.2})+{:.2}@({:.0}deg,{:.0}deg)\tout={}",
        program_path,
        applied,
        world.iter_on().count(),
        camera.position().x,
        camera.position().y,
        camera.position().z,
        distance,
        yaw_deg,
        pitch_deg,
        out_path,
    );
}

/// Minimal truecolor PNG writer: filter 0, deflate stored blocks, no dependencies.
/// A gallery image does not need compression; it needs to be exact and dependency-free.
fn write_png(path: &Path, pixels: &[u32], width: u32, height: u32) -> std::io::Result<()> {
    let mut raw = Vec::with_capacity((width as usize + 1) * height as usize * 3);
    for y in 0..height as usize {
        raw.push(0u8); // filter type 0 per scanline
        for x in 0..width as usize {
            let word = pixels[y * width as usize + x];
            raw.push(((word >> 16) & 0xff) as u8);
            raw.push(((word >> 8) & 0xff) as u8);
            raw.push((word & 0xff) as u8);
        }
    }

    // zlib stream: header, stored deflate blocks, adler32 of the raw bytes.
    let mut z = vec![0x78, 0x01];
    let mut a: u32 = 1;
    let mut b: u32 = 0;
    for &byte in &raw {
        a = (a + byte as u32) % 65521;
        b = (b + a) % 65521;
    }
    let adler32 = (b << 16) | a;
    let total = raw.len();
    for (offset, chunk) in raw.chunks(65_535).enumerate() {
        let last = offset * 65_535 + chunk.len() == total;
        z.push(u8::from(last));
        let len = chunk.len() as u16;
        z.extend_from_slice(&len.to_le_bytes());
        z.extend_from_slice(&(!len).to_le_bytes());
        z.extend_from_slice(chunk);
    }
    z.extend_from_slice(&adler32.to_be_bytes());

    let mut png = Vec::new();
    png.extend_from_slice(b"\x89PNG\r\n\x1a\n");
    let mut ihdr = Vec::new();
    ihdr.extend_from_slice(&width.to_be_bytes());
    ihdr.extend_from_slice(&height.to_be_bytes());
    ihdr.extend_from_slice(&[8, 2, 0, 0, 0]); // 8-bit truecolor
    push_chunk(&mut png, b"IHDR", &ihdr);
    push_chunk(&mut png, b"IDAT", &z);
    push_chunk(&mut png, b"IEND", &[]);

    let mut f = fs::File::create(path)?;
    f.write_all(&png)?;
    f.flush()
}

fn push_chunk(out: &mut Vec<u8>, kind: &[u8; 4], data: &[u8]) {
    out.extend_from_slice(&(data.len() as u32).to_be_bytes());
    out.extend_from_slice(kind);
    out.extend_from_slice(data);
    let mut crc = 0xffff_ffffu32;
    for &byte in kind.iter().chain(data.iter()) {
        crc ^= byte as u32;
        for _ in 0..8 {
            let mask = (crc & 1).wrapping_neg();
            crc = (crc >> 1) ^ (0xedb8_8320 & mask);
        }
    }
    out.extend_from_slice(&(!crc).to_be_bytes());
}
