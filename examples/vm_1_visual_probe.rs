//! Visual 3D execution & atomic-rollback probe + release throughput benchmark for `VM-1` (`PS-047`).

use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Instant;

use bitblock::camera::{Camera, DEFAULT_VERTICAL_FOV_RADIANS, Vec3};
use bitblock::history::History;
use bitblock::language_r1::{ParsedSpatialProgramV1, SpatialOpcodeV1, encode_program_to_world};
use bitblock::render::Renderer;
use bitblock::vm_1::{LocalSpatialVmV1, VmProgramRequestV1};
use bitblock::vm_d1::VmOutputRegisterV1;
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

fn push_chunk(out: &mut Vec<u8>, kind: &[u8; 4], payload: &[u8]) {
    out.extend_from_slice(&(payload.len() as u32).to_be_bytes());
    let start = out.len();
    out.extend_from_slice(kind);
    out.extend_from_slice(payload);
    out.extend_from_slice(&crc32(&out[start..]).to_be_bytes());
}

fn write_png(path: &Path, width: u32, height: u32, pixels: &[u32]) {
    let mut raw = Vec::with_capacity((width as usize * 3 + 1) * height as usize);
    for y in 0..height as usize {
        raw.push(0_u8);
        for &w in &pixels[y * width as usize..(y + 1) * width as usize] {
            raw.push(((w >> 16) & 0xff) as u8);
            raw.push(((w >> 8) & 0xff) as u8);
            raw.push((w & 0xff) as u8);
        }
    }
    let mut zlib = Vec::with_capacity(raw.len() + (raw.len() / 65535 + 2) * 5 + 6);
    zlib.extend_from_slice(&[0x78, 0x01]);
    let mut off = 0;
    while off < raw.len() {
        let len = (raw.len() - off).min(65_535);
        zlib.push(u8::from(off + len == raw.len()));
        let lu = len as u16;
        zlib.extend_from_slice(&lu.to_le_bytes());
        zlib.extend_from_slice(&(!lu).to_le_bytes());
        zlib.extend_from_slice(&raw[off..off + len]);
        off += len;
    }
    zlib.extend_from_slice(&adler32(&raw).to_be_bytes());
    let mut png = Vec::with_capacity(zlib.len() + 128);
    png.extend_from_slice(b"\x89PNG\r\n\x1a\n");
    let mut ihdr = Vec::with_capacity(13);
    ihdr.extend_from_slice(&width.to_be_bytes());
    ihdr.extend_from_slice(&height.to_be_bytes());
    ihdr.extend_from_slice(&[8, 2, 0, 0, 0]);
    push_chunk(&mut png, b"IHDR", &ihdr);
    push_chunk(&mut png, b"IDAT", &zlib);
    push_chunk(&mut png, b"IEND", &[]);
    fs::write(path, &png).expect("write PNG");
}

fn main() {
    let args: Vec<String> = env::args().collect();
    let snap_dir: Option<PathBuf> = args
        .windows(2)
        .find(|w| w[0] == "--dump-snapshots")
        .map(|w| PathBuf::from(&w[1]));
    if let Some(dir) = &snap_dir {
        fs::create_dir_all(dir).expect("mkdir snap_dir");
    }

    let vm = LocalSpatialVmV1::v1_default().unwrap();
    let prog_a = ParsedSpatialProgramV1::new(
        1,
        Coord::new(3, 0, 2),
        vec![
            SpatialOpcodeV1::PushOne,
            SpatialOpcodeV1::PushOne,
            SpatialOpcodeV1::Add,
            SpatialOpcodeV1::Dup,
            SpatialOpcodeV1::Add,
            SpatialOpcodeV1::PushOne,
            SpatialOpcodeV1::Add,
            SpatialOpcodeV1::Halt,
        ],
    )
    .unwrap();
    let prog_b = ParsedSpatialProgramV1::new(
        2,
        Coord::new(3, 10, 2),
        vec![
            SpatialOpcodeV1::PushOne,
            SpatialOpcodeV1::PushOne,
            SpatialOpcodeV1::Add,
            SpatialOpcodeV1::Halt,
        ],
    )
    .unwrap();
    let prog_c = ParsedSpatialProgramV1::new(
        3,
        Coord::new(3, 20, 2),
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
    encode_program_to_world(&mut world, &prog_c).unwrap();
    world.set_on(Coord::new(6, 8, 2)); // stale bit 1 in A output -> cleared Off
    world.set_on(Coord::new(5, 18, 2)); // sentinel bit 0 in B output -> preserved on GasExhausted rollback
    let mut history = History::new(&world);

    let cam = Camera::from_anchor(
        Vec3::new(16.0, 14.0, 2.0),
        48.0,
        0.14,
        -0.12,
        DEFAULT_VERTICAL_FOV_RADIANS,
    );
    let mut renderer = Renderer::new(VIEWPORT, VIEWPORT);

    // Frame 0: Initial world (t = 0)
    renderer.render(&world, &cam);
    if let Some(dir) = &snap_dir {
        write_png(
            &dir.join("vm1_t0_init.png"),
            VIEWPORT,
            VIEWPORT,
            renderer.pixels(),
        );
    }

    // Tick 1: A=5 commits (0b0101), B traps GasExhausted (sentinel preserved), C=3 commits (0b0011)
    let (r1, tr1) = vm
        .execute_tick(
            &mut world,
            &mut history,
            &[
                VmProgramRequestV1 {
                    anchor: Coord::new(3, 0, 2),
                    output_register: Some(VmOutputRegisterV1 {
                        origin: Coord::new(5, 8, 2),
                        bit_width: 4,
                    }),
                    gas_limit_override: None,
                },
                VmProgramRequestV1 {
                    anchor: Coord::new(3, 10, 2),
                    output_register: Some(VmOutputRegisterV1 {
                        origin: Coord::new(5, 18, 2),
                        bit_width: 4,
                    }),
                    gas_limit_override: Some(4),
                },
                VmProgramRequestV1 {
                    anchor: Coord::new(3, 20, 2),
                    output_register: Some(VmOutputRegisterV1 {
                        origin: Coord::new(5, 28, 2),
                        bit_width: 4,
                    }),
                    gas_limit_override: None,
                },
            ],
        )
        .unwrap();
    renderer.render(&world, &cam);
    if let Some(dir) = &snap_dir {
        write_png(
            &dir.join("vm1_t1_exec.png"),
            VIEWPORT,
            VIEWPORT,
            renderer.pixels(),
        );
    }

    // Tick 2: B runs with full budget -> writes 2 (0b0010, clearing bit 0 & setting bit 1),
    // while C deliberately targets B's output register -> OutputWriteConflict + atomic rollback!
    let (r2, _) = vm
        .execute_tick(
            &mut world,
            &mut history,
            &[
                VmProgramRequestV1 {
                    anchor: Coord::new(3, 10, 2),
                    output_register: Some(VmOutputRegisterV1 {
                        origin: Coord::new(5, 18, 2),
                        bit_width: 4,
                    }),
                    gas_limit_override: None,
                },
                VmProgramRequestV1 {
                    anchor: Coord::new(3, 20, 2),
                    output_register: Some(VmOutputRegisterV1 {
                        origin: Coord::new(5, 18, 2),
                        bit_width: 4,
                    }),
                    gas_limit_override: None,
                },
            ],
        )
        .unwrap();
    renderer.render(&world, &cam);
    if let Some(dir) = &snap_dir {
        write_png(
            &dir.join("vm1_t2_conflict.png"),
            VIEWPORT,
            VIEWPORT,
            renderer.pixels(),
        );
    }

    // Release latency benchmark: 200 iterations of Tick 1 over fresh worlds.
    let mut samples_us = Vec::with_capacity(200);
    for _ in 0..200 {
        let mut bw = World::new();
        encode_program_to_world(&mut bw, &prog_a).unwrap();
        encode_program_to_world(&mut bw, &prog_b).unwrap();
        encode_program_to_world(&mut bw, &prog_c).unwrap();
        let mut bh = History::new(&bw);
        let t0 = Instant::now();
        let _ = vm
            .execute_tick(
                &mut bw,
                &mut bh,
                &[
                    VmProgramRequestV1 {
                        anchor: Coord::new(3, 0, 2),
                        output_register: Some(VmOutputRegisterV1 {
                            origin: Coord::new(5, 8, 2),
                            bit_width: 4,
                        }),
                        gas_limit_override: None,
                    },
                    VmProgramRequestV1 {
                        anchor: Coord::new(3, 10, 2),
                        output_register: Some(VmOutputRegisterV1 {
                            origin: Coord::new(5, 18, 2),
                            bit_width: 4,
                        }),
                        gas_limit_override: Some(4),
                    },
                    VmProgramRequestV1 {
                        anchor: Coord::new(3, 20, 2),
                        output_register: Some(VmOutputRegisterV1 {
                            origin: Coord::new(5, 28, 2),
                            bit_width: 4,
                        }),
                        gas_limit_override: None,
                    },
                ],
            )
            .unwrap();
        samples_us.push(t0.elapsed().as_secs_f64() * 1e6);
    }
    samples_us.sort_by(|a, b| a.total_cmp(b));
    let p50_us = samples_us[samples_us.len() / 2];
    let p95_us = samples_us[(samples_us.len() * 95) / 100];
    let max_us = *samples_us.last().unwrap();

    println!("{{");
    println!("  \"trace_a_tag\": \"{}\",", tr1[0].tag());
    println!("  \"tick1_receipt_tag\": \"{}\",", r1.tag());
    println!("  \"tick2_receipt_tag\": \"{}\",", r2.tag());
    println!("  \"tick1_gas_used\": {},", r1.tick_gas_used);
    println!("  \"tick1_emitted_writes\": {},", r1.tick_emitted_writes);
    println!("  \"tick2_gas_used\": {},", r2.tick_gas_used);
    println!("  \"tick2_emitted_writes\": {},", r2.tick_emitted_writes);
    println!(
        "  \"history_parity\": {},",
        history.receipts().last().unwrap().post_world().digest() == r2.post_world_digest
    );
    println!("  \"bench_3prog_tick_p50_us\": {:.2},", p50_us);
    println!("  \"bench_3prog_tick_p95_us\": {:.2},", p95_us);
    println!("  \"bench_3prog_tick_max_us\": {:.2}", max_us);
    println!("}}");
}
