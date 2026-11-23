#![cfg(feature = "protocol-research")]

use bitblock::bounds_1::LocalVmBoundsV1;
use bitblock::camera::{Camera, DEFAULT_VERTICAL_FOV_RADIANS, Vec3};
use bitblock::history::{History, world_tag};
use bitblock::language_r1::{ParsedSpatialProgramV1, SpatialOpcodeV1, encode_program_to_world};
use bitblock::render::Renderer;
use bitblock::vm_1::{
    LocalSpatialVmV1, Vm1Error, VmProgramRequestV1, VmStepTraceV1, evaluate_program_traced,
    step_opcode,
};
use bitblock::vm_d1::{
    VmInvocationV1, VmOutputRegisterV1, VmSpecD1, VmTrapV1, schedule_and_evaluate_model,
};
use bitblock::world::{BEDROCK_COORD, Coord, World};

const GOLDEN_TRACE_A_TAG: &str =
    "#bb.receipt.sha256.zr6heiln6uepluy6k7h6af3idgwgupnezvrk4ovajdf3t4hqegdq";
const GOLDEN_TICK1_RECEIPT_TAG: &str =
    "#bb.receipt.sha256.vka355inmn2tfu6ebpz73rxtu33c3rrkmfyvh2mvnm7olmeilxkq";

fn build_golden_world() -> (World, History) {
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
    world.set_on(Coord::new(5, 18, 2)); // sentinel bit 0 in B output -> preserved on rollback
    let history = History::new(&world);
    (world, history)
}

#[test]
fn wave9a_vm_1_golden_python_parity_and_differential_fuzz() {
    let vm = LocalSpatialVmV1::v1_default().unwrap();
    let (mut world, mut history) = build_golden_world();

    // Unordered submission must sort deterministically by ascending anchor.
    let (receipt, traces) = vm
        .execute_tick(
            &mut world,
            &mut history,
            &[
                VmProgramRequestV1 {
                    anchor: Coord::new(3, 20, 2),
                    output_register: Some(VmOutputRegisterV1 {
                        origin: Coord::new(5, 28, 2),
                        bit_width: 4,
                    }),
                    gas_limit_override: None,
                },
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
            ],
        )
        .unwrap();

    assert_eq!(traces[0].tag().to_string(), GOLDEN_TRACE_A_TAG);
    assert_eq!(receipt.tag().to_string(), GOLDEN_TICK1_RECEIPT_TAG);
    assert_eq!(receipt.entries[0].outcome, Ok(5));
    assert_eq!(receipt.entries[1].outcome, Err(VmTrapV1::GasExhausted));
    assert_eq!(receipt.entries[2].outcome, Ok(3));
    assert!(
        !world.is_on(Coord::new(6, 8, 2)),
        "stale A bit 1 cleared Off"
    );
    assert!(
        world.is_on(Coord::new(5, 18, 2)),
        "B rollback must preserve untouched sentinel voxel"
    );
    assert_eq!(
        history.receipts().last().unwrap().post_world().digest(),
        receipt.post_world_digest
    );

    // Wire round-trip and negative truncation check for VmStepTraceV1.
    let encoded_trace = traces[0].encode_canonical();
    assert_eq!(
        VmStepTraceV1::decode_canonical(&encoded_trace).unwrap(),
        traces[0]
    );
    assert!(VmStepTraceV1::decode_canonical(&encoded_trace[..encoded_trace.len() - 1]).is_err());

    // 120 seeded differential programs comparing LocalSpatialVmV1 vs vm_d1::schedule_and_evaluate_model.
    let all_ops = [
        SpatialOpcodeV1::Nop,
        SpatialOpcodeV1::PushOne,
        SpatialOpcodeV1::PushZero,
        SpatialOpcodeV1::Add,
        SpatialOpcodeV1::Sub,
        SpatialOpcodeV1::Dup,
        SpatialOpcodeV1::Swap,
        SpatialOpcodeV1::Halt,
    ];
    let spec = VmSpecD1::v1_default();
    let mut seed: u64 = 0x564D_315F_4655_5A5Au64;
    let mut next_u64 = || {
        seed = seed
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        seed
    };

    for case in 0..120u32 {
        let len = (next_u64() % 12) as usize + 1;
        let mut ops = Vec::with_capacity(len);
        for i in 0..len {
            if i + 1 == len && case % 4 != 0 {
                ops.push(SpatialOpcodeV1::Halt);
            } else {
                ops.push(all_ops[(next_u64() as usize) % all_ops.len()]);
            }
        }
        let anchor = Coord::new(4, 0, 2);
        let prog = ParsedSpatialProgramV1::new((case % 15) as u8 + 1, anchor, ops).unwrap();
        let out_reg = if case % 2 == 0 {
            Some(VmOutputRegisterV1 {
                origin: Coord::new(6, 10, 2),
                bit_width: ((case % 8) as u8) + 1,
            })
        } else {
            None
        };

        let mut w_vm1 = World::new();
        encode_program_to_world(&mut w_vm1, &prog).unwrap();
        let mut h_vm1 = History::new(&w_vm1);
        let mut w_d1 = w_vm1.clone();
        let mut h_d1 = History::new(&w_d1);

        let (r_vm1, t_vm1) = vm
            .execute_tick(
                &mut w_vm1,
                &mut h_vm1,
                &[VmProgramRequestV1 {
                    anchor,
                    output_register: out_reg,
                    gas_limit_override: None,
                }],
            )
            .unwrap();
        let r_d1 = schedule_and_evaluate_model(
            &mut w_d1,
            &mut h_d1,
            &spec,
            &[VmInvocationV1 {
                anchor,
                output_register: out_reg,
            }],
            vm.bounds.max_gas_per_program,
        )
        .unwrap();

        assert_eq!(
            r_vm1.entries[0].outcome, r_d1.outcomes[0].outcome,
            "case {case}"
        );
        assert_eq!(
            r_vm1.entries[0].gas_used, r_d1.outcomes[0].gas_used,
            "case {case}"
        );
        assert_eq!(
            r_vm1.entries[0].emitted_writes, r_d1.outcomes[0].emitted_writes,
            "case {case}"
        );
        assert_eq!(
            r_vm1.post_world_digest, r_d1.post_world_digest,
            "case {case}"
        );
        assert_eq!(
            t_vm1[0].final_gas_used, r_vm1.entries[0].gas_used,
            "case {case}"
        );
        assert_eq!(
            t_vm1[0].final_status_code,
            r_vm1.entries[0].status_code(),
            "case {case}"
        );
        // Gas monotonicity and bounded step count invariant.
        assert!(t_vm1[0].steps.len() <= prog.instructions.len());
        let mut prev_gas = 0u64;
        for step in &t_vm1[0].steps {
            assert!(step.gas_after >= prev_gas);
            assert!(step.stack_depth_after <= vm.bounds.max_stack_depth);
            prev_gas = step.gas_after;
        }
    }
}

#[test]
fn wave9a_vm_1_all_seven_traps_bounds_preflight_and_camera_invariance() {
    let spec = VmSpecD1::v1_default();
    let vm = LocalSpatialVmV1::v1_default().unwrap();

    // 1. Direct step_opcode checked i64 ArithmeticOverflow (both Add and Sub) + stack untouched on trap!
    let mut s_add = vec![i64::MAX, 1];
    assert_eq!(
        step_opcode(&mut s_add, SpatialOpcodeV1::Add, 64),
        Err(VmTrapV1::ArithmeticOverflow)
    );
    assert_eq!(s_add, vec![i64::MAX, 1], "trap must leave stack untouched");

    let mut s_sub = vec![i64::MIN, 1];
    assert_eq!(
        step_opcode(&mut s_sub, SpatialOpcodeV1::Sub, 64),
        Err(VmTrapV1::ArithmeticOverflow)
    );
    assert_eq!(s_sub, vec![i64::MIN, 1]);

    // 2. StackOverflow via tightened LocalVmBoundsV1 (max_stack_depth = 2).
    let mut tight_bounds = LocalVmBoundsV1::v1_default(&spec).unwrap();
    tight_bounds.max_stack_depth = 2;
    let prog_ovf = ParsedSpatialProgramV1::new(
        1,
        Coord::new(3, 0, 2),
        vec![
            SpatialOpcodeV1::PushOne,
            SpatialOpcodeV1::PushOne,
            SpatialOpcodeV1::PushOne,
            SpatialOpcodeV1::Halt,
        ],
    )
    .unwrap();
    let (res_ovf, tr_ovf) = evaluate_program_traced(&spec, &tight_bounds, &prog_ovf, None).unwrap();
    assert_eq!(res_ovf, Err(VmTrapV1::StackOverflow));
    assert_eq!(
        tr_ovf.final_status_code,
        VmTrapV1::StackOverflow.status_code()
    );

    // 3. StackUnderflow (empty stack on output register) + MissingHalt + ForbiddenOutputTarget
    //    (bedrock and program frame) + OutputWriteConflict + output write GasExhausted.
    let p_empty =
        ParsedSpatialProgramV1::new(1, Coord::new(3, 0, 2), vec![SpatialOpcodeV1::Halt]).unwrap();
    let p_no_halt =
        ParsedSpatialProgramV1::new(2, Coord::new(3, 10, 2), vec![SpatialOpcodeV1::PushOne])
            .unwrap();
    let p_bedrock = ParsedSpatialProgramV1::new(
        3,
        Coord::new(3, 20, 2),
        vec![SpatialOpcodeV1::PushOne, SpatialOpcodeV1::Halt],
    )
    .unwrap();
    let p_frame = ParsedSpatialProgramV1::new(
        4,
        Coord::new(3, 30, 2),
        vec![SpatialOpcodeV1::PushOne, SpatialOpcodeV1::Halt],
    )
    .unwrap();
    let p_win = ParsedSpatialProgramV1::new(
        5,
        Coord::new(3, 40, 2),
        vec![SpatialOpcodeV1::PushOne, SpatialOpcodeV1::Halt],
    )
    .unwrap();
    let p_conflict = ParsedSpatialProgramV1::new(
        6,
        Coord::new(3, 50, 2),
        vec![SpatialOpcodeV1::PushZero, SpatialOpcodeV1::Halt],
    )
    .unwrap();
    let p_write_gas = ParsedSpatialProgramV1::new(
        7,
        Coord::new(3, 60, 2),
        vec![SpatialOpcodeV1::PushOne, SpatialOpcodeV1::Halt],
    )
    .unwrap();

    let mut world = World::new();
    for p in [
        &p_empty,
        &p_no_halt,
        &p_bedrock,
        &p_frame,
        &p_win,
        &p_conflict,
        &p_write_gas,
    ] {
        encode_program_to_world(&mut world, p).unwrap();
    }
    let mut history = History::new(&world);
    let (r_traps, _) = vm
        .execute_tick(
            &mut world,
            &mut history,
            &[
                VmProgramRequestV1 {
                    anchor: p_empty.anchor,
                    output_register: Some(VmOutputRegisterV1 {
                        origin: Coord::new(40, 0, 2),
                        bit_width: 1,
                    }),
                    gas_limit_override: None,
                },
                VmProgramRequestV1 {
                    anchor: p_no_halt.anchor,
                    output_register: None,
                    gas_limit_override: None,
                },
                VmProgramRequestV1 {
                    anchor: p_bedrock.anchor,
                    output_register: Some(VmOutputRegisterV1 {
                        origin: BEDROCK_COORD,
                        bit_width: 1,
                    }),
                    gas_limit_override: None,
                },
                VmProgramRequestV1 {
                    anchor: p_frame.anchor,
                    output_register: Some(VmOutputRegisterV1 {
                        origin: p_frame.anchor,
                        bit_width: 1,
                    }),
                    gas_limit_override: None,
                },
                VmProgramRequestV1 {
                    anchor: p_win.anchor,
                    output_register: Some(VmOutputRegisterV1 {
                        origin: Coord::new(40, 40, 2),
                        bit_width: 2,
                    }),
                    gas_limit_override: None,
                },
                VmProgramRequestV1 {
                    anchor: p_conflict.anchor,
                    output_register: Some(VmOutputRegisterV1 {
                        origin: Coord::new(41, 40, 2),
                        bit_width: 2,
                    }),
                    gas_limit_override: None,
                },
                VmProgramRequestV1 {
                    anchor: p_write_gas.anchor,
                    output_register: Some(VmOutputRegisterV1 {
                        origin: Coord::new(40, 60, 2),
                        bit_width: 2,
                    }),
                    gas_limit_override: Some(5), // 2+1=3 for ops, +20 write_gas > 5
                },
            ],
        )
        .unwrap();

    assert_eq!(r_traps.entries[0].outcome, Err(VmTrapV1::StackUnderflow));
    assert_eq!(r_traps.entries[1].outcome, Err(VmTrapV1::MissingHalt));
    assert_eq!(
        r_traps.entries[2].outcome,
        Err(VmTrapV1::ForbiddenOutputTarget(BEDROCK_COORD))
    );
    assert_eq!(
        r_traps.entries[3].outcome,
        Err(VmTrapV1::ForbiddenOutputTarget(p_frame.anchor))
    );
    assert_eq!(r_traps.entries[4].outcome, Ok(1));
    assert_eq!(
        r_traps.entries[5].outcome,
        Err(VmTrapV1::OutputWriteConflict(Coord::new(41, 40, 2)))
    );
    assert_eq!(r_traps.entries[6].outcome, Err(VmTrapV1::GasExhausted));
    assert_eq!(r_traps.tick_emitted_writes, 2);
    assert!(world.is_on(Coord::new(40, 40, 2)));
    assert!(!world.is_on(Coord::new(41, 40, 2)));
    assert!(
        !world.is_on(Coord::new(42, 40, 2)),
        "conflicting C bit 1 rolled back"
    );

    // 4. Fail-closed BOUNDS-1 (CE-05) preflight 1-over refusal before execution.
    let mut cap_bounds = LocalVmBoundsV1::v1_default(&spec).unwrap();
    cap_bounds.max_programs_per_tick = 2;
    let capped_vm = LocalSpatialVmV1::new(spec, cap_bounds).unwrap();
    let (mut w_cap, mut h_cap) = build_golden_world();
    let pre_tag = world_tag(&w_cap);
    let err = capped_vm
        .execute_tick(
            &mut w_cap,
            &mut h_cap,
            &[
                VmProgramRequestV1 {
                    anchor: Coord::new(3, 0, 2),
                    output_register: None,
                    gas_limit_override: None,
                },
                VmProgramRequestV1 {
                    anchor: Coord::new(3, 10, 2),
                    output_register: None,
                    gas_limit_override: None,
                },
                VmProgramRequestV1 {
                    anchor: Coord::new(3, 20, 2),
                    output_register: None,
                    gas_limit_override: None,
                },
            ],
        )
        .unwrap_err();
    assert!(matches!(err, Vm1Error::BoundsPreflight(_)));
    assert_eq!(
        world_tag(&w_cap),
        pre_tag,
        "preflight refusal leaves world untouched"
    );
    assert!(
        h_cap.receipts().is_empty(),
        "preflight refusal commits zero history"
    );

    // 5. Camera-pose invariance: rendering from different cameras cannot perturb execution tags.
    let (mut w1, mut h1) = build_golden_world();
    let (mut w2, mut h2) = build_golden_world();
    let mut renderer = Renderer::new(65, 65);
    let cam1 = Camera::from_anchor(
        Vec3::new(0.0, 0.0, 0.0),
        10.0,
        0.1,
        0.2,
        DEFAULT_VERTICAL_FOV_RADIANS,
    );
    let cam2 = Camera::from_anchor(
        Vec3::new(99.0, -40.0, 15.0),
        80.0,
        1.4,
        -0.7,
        DEFAULT_VERTICAL_FOV_RADIANS,
    );
    renderer.render(&w1, &cam1);
    renderer.render(&w2, &cam2);
    let reqs = [VmProgramRequestV1 {
        anchor: Coord::new(3, 0, 2),
        output_register: Some(VmOutputRegisterV1 {
            origin: Coord::new(5, 8, 2),
            bit_width: 4,
        }),
        gas_limit_override: Some(u64::MAX), // clamped to LocalVmBoundsV1::max_gas_per_program
    }];
    let (ra, ta) = vm.execute_tick(&mut w1, &mut h1, &reqs).unwrap();
    let (rb, tb) = vm.execute_tick(&mut w2, &mut h2, &reqs).unwrap();
    assert_eq!(ra.tag(), rb.tag());
    assert_eq!(ta[0].tag(), tb[0].tag());
    assert_eq!(ta[0].gas_limit, vm.bounds.max_gas_per_program);
}
