//! Visual renderer probe & PNG snapshot exporter for `VM-D1` deterministic spatial program
//! execution over `LANGUAGE-R1` v1 framed 3D programs.
//!
//! Places two framed `LANGUAGE-R1` programs beside permanent orange bedrock `(0,0,0)`:
//! - Program A at `(3, 0, 2)`: `[PushOne, PushOne, Add, Dup, Add, Halt]` (`= 4 = 0b0100`), with a
//!   pre-existing stale voxel at bit 1 `(6, 8, 2)` that gets cleared `Off` while bit 2 `(7, 8, 2)`
//!   turns `On`.
//! - Program B at `(3, 10, 2)`: `[PushOne, PushOne, Add, PushOne, Add, Halt]` (`= 3 = 0b0011`),
//!   emitting bits 0 & 1 (`(5, 18, 2)` and `(6, 18, 2)`) `On`.
//!
//! Renders `801x801` `vm_d1_before.png` and `vm_d1_after.png` via Bitblock's production `Renderer`
//! for direct visual inspection.
//!
//! ```text
//! cargo run --release --features protocol-research --example vm_d1_visual_probe -- --dump-snapshots /tmp/vm_snaps
//! ```

use std::env;
use std::fs;
use std::path::{Path, PathBuf};

use bitblock::camera::{Camera, DEFAULT_VERTICAL_FOV_RADIANS, Vec3};
use bitblock::history::History;
use bitblock::language_r1::{ParsedSpatialProgramV1, SpatialOpcodeV1, encode_program_to_world};
use bitblock::render::Renderer;
use bitblock::vm_d1::{VmInvocationV1, VmOutputRegisterV1, VmSpecD1, schedule_and_evaluate_model};
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

    let anchor_a = Coord::new(3, 0, 2);
    let anchor_b = Coord::new(3, 10, 2);
    let prog_a = ParsedSpatialProgramV1::new(
        3,
        anchor_a,
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
    let prog_b = ParsedSpatialProgramV1::new(
        4,
        anchor_b,
        vec![
            SpatialOpcodeV1::PushOne,
            SpatialOpcodeV1::PushOne,
            SpatialOpcodeV1::Add,
            SpatialOpcodeV1::PushOne,
            SpatialOpcodeV1::Add,
            SpatialOpcodeV1::Halt,
        ],
    )
    .unwrap();

    let mut world = World::new();
    encode_program_to_world(&mut world, &prog_a).unwrap();
    encode_program_to_world(&mut world, &prog_b).unwrap();
    // Stale bit at bit 1 of Program A's output register to visually confirm explicit `Off` clearing!
    world.set_on(Coord::new(6, 8, 2));

    let cam = Camera::from_anchor(
        Vec3::new(14.0, 9.0, 2.0),
        36.0,
        0.16,
        -0.14,
        DEFAULT_VERTICAL_FOV_RADIANS,
    );
    let mut renderer = Renderer::new(VIEWPORT, VIEWPORT);
    renderer.render(&world, &cam);
    if let Some(dir) = &snap_dir {
        write_xrgb_png(
            &dir.join("vm_d1_before.png"),
            VIEWPORT,
            VIEWPORT,
            renderer.pixels(),
        );
    }

    let mut history = History::new(&world);
    let spec = VmSpecD1::v1_default();
    let receipt = schedule_and_evaluate_model(
        &mut world,
        &mut history,
        &spec,
        &[
            VmInvocationV1 {
                anchor: anchor_b,
                output_register: Some(VmOutputRegisterV1 {
                    origin: Coord::new(5, 18, 2),
                    bit_width: 4,
                }),
            },
            VmInvocationV1 {
                anchor: anchor_a,
                output_register: Some(VmOutputRegisterV1 {
                    origin: Coord::new(5, 8, 2),
                    bit_width: 4,
                }),
            },
        ],
        200,
    )
    .unwrap();

    renderer.render(&world, &cam);
    if let Some(dir) = &snap_dir {
        write_xrgb_png(
            &dir.join("vm_d1_after.png"),
            VIEWPORT,
            VIEWPORT,
            renderer.pixels(),
        );
    }

    println!("VM-D1 Visual Probe:");
    println!("  spec_tag    = {}", spec.tag().unwrap());
    println!("  receipt_tag = {}", receipt.tag());
    println!("  prog_a      = {:?}", receipt.outcomes[0].outcome);
    println!("  prog_b      = {:?}", receipt.outcomes[1].outcome);
    println!("  total_gas   = {}", receipt.total_gas_used);
    if let Some(dir) = &snap_dir {
        println!("  snapshots   = {}", dir.display());
    }
}
