//! Wave 36 (`PROGRAM-STATE-1@base`, `PS-048`) integration test suite:
//! Launch-profile inactive wall, `CE-43` successor activation, canonical code/availability checks,
//! separately typed Code vs Instance Merkle roots & cross-domain rejection, atomic trap rollback,
//! and cold restart parity.

#![cfg(feature = "protocol-research")]

use bitblock::bounds_1::LocalVmBoundsV1;
use bitblock::language_r1::SpatialOpcodeV1;
use bitblock::program_state_1::{
    ProgramCodeRecordV1, ProgramExecDispositionV1, ProgramState1Error, ProgramStateEngineV1,
    ProgramTreeDomainV1, canonical_program_state_profile_specs, profile_permits_program_state,
};
use bitblock::vm_d1::{VmSpecD1, VmTrapV1};

#[test]
fn program_state_1_launch_wall_successor_exec_trap_rollback_and_restart_parity() {
    let vm_spec = VmSpecD1::v1_default();
    let vm_bounds = LocalVmBoundsV1::v1_default(&vm_spec).expect("bounds");
    let (launch_spec, succ_spec) = canonical_program_state_profile_specs().expect("specs");

    assert!(!profile_permits_program_state(&launch_spec));
    assert!(profile_permits_program_state(&succ_spec));

    let code_ok = ProgramCodeRecordV1::from_opcodes(&[
        SpatialOpcodeV1::PushOne,
        SpatialOpcodeV1::Dup,
        SpatialOpcodeV1::Add,
        SpatialOpcodeV1::Dup,
        SpatialOpcodeV1::Add,
        SpatialOpcodeV1::Halt,
    ])
    .expect("valid code");
    let code_trap = ProgramCodeRecordV1::from_opcodes(&[
        SpatialOpcodeV1::PushOne,
        SpatialOpcodeV1::Add, // StackUnderflow trap!
        SpatialOpcodeV1::Halt,
    ])
    .expect("trap code");

    let mut eng = ProgramStateEngineV1::empty(vm_spec, vm_bounds).expect("empty");
    let empty_roots = eng.roots();

    // Launch profile wall: all operations refused with zero state mutation.
    assert_eq!(
        eng.register_code(&launch_spec, code_ok.clone()),
        Err(ProgramState1Error::ProfileProgramInactive)
    );
    let launch_rec = eng
        .execute_instance(&launch_spec, [0x11u8; 32], [0x33u8; 32], 100)
        .expect("launch exec receipt");
    assert_eq!(
        launch_rec.disposition,
        ProgramExecDispositionV1::RejectedProfileInactive
    );
    assert_eq!(launch_rec.post_state_root, empty_roots.program_state_root);
    assert_eq!(eng.roots(), empty_roots);

    // CE-43 Successor profile: register both programs, instantiate, execute, and verify trap rollback.
    let cid_ok = eng
        .register_code(&succ_spec, code_ok.clone())
        .expect("reg ok");
    let cid_trap = eng
        .register_code(&succ_spec, code_trap.clone())
        .expect("reg trap");

    let inst_ok = [0x11u8; 32];
    let inst_trap = [0x12u8; 32];
    let owner = [0x22u8; 32];
    let slot_a = [0x33u8; 32];
    let slot_b = [0x44u8; 32];

    eng.instantiate_program(&succ_spec, inst_ok, cid_ok, owner)
        .expect("inst ok");
    eng.instantiate_program(&succ_spec, inst_trap, cid_trap, owner)
        .expect("inst trap");

    // 1. Clean execution commits value=4, gas_used=33, and updates roots.
    let r_ok = eng
        .execute_instance(&succ_spec, inst_ok, slot_a, 200)
        .expect("exec ok");
    assert_eq!(r_ok.disposition, ProgramExecDispositionV1::Admitted);
    assert_eq!(r_ok.written_value, 4);
    assert_eq!(r_ok.gas_used, 33);
    assert_ne!(r_ok.pre_state_root, r_ok.post_state_root);
    assert_eq!(
        eng.instance(&inst_ok).expect("inst").storage.get(&slot_a),
        Some(&4)
    );

    // 2. Trap execution rolls back atomically with zero storage/root mutation.
    let roots_before_trap = eng.roots();
    let r_trap = eng
        .execute_instance(&succ_spec, inst_trap, slot_b, 200)
        .expect("exec trap");
    assert_eq!(
        r_trap.disposition,
        ProgramExecDispositionV1::TrappedAtomicRollback
    );
    assert_eq!(
        r_trap.trap_status_code,
        VmTrapV1::StackUnderflow.status_code()
    );
    assert_eq!(r_trap.post_state_root, roots_before_trap.program_state_root);
    assert_eq!(eng.roots(), roots_before_trap);
    assert!(
        eng.instance(&inst_trap)
            .expect("inst trap")
            .storage
            .is_empty()
    );

    // 3. Uncharged storage-write work (gas_limit=15 covers 13 opcode gas but NOT +20 storage write)
    //    traps GasExhausted and rolls back atomically!
    let r_uncharged = eng
        .execute_instance(&succ_spec, inst_ok, slot_b, 15)
        .expect("exec uncharged");
    assert_eq!(
        r_uncharged.disposition,
        ProgramExecDispositionV1::TrappedAtomicRollback
    );
    assert_eq!(
        r_uncharged.trap_status_code,
        VmTrapV1::GasExhausted.status_code()
    );
    assert_eq!(
        r_uncharged.post_state_root,
        roots_before_trap.program_state_root
    );
    assert_eq!(
        eng.instance(&inst_ok).expect("inst").storage.get(&slot_b),
        None
    );

    // 4. Cold restart replay produces byte-identical roots (`verify_rebuild_parity`).
    let mut replay = ProgramStateEngineV1::empty(vm_spec, vm_bounds).expect("replay");
    replay.register_code(&succ_spec, code_ok).expect("reg1");
    replay.register_code(&succ_spec, code_trap).expect("reg2");
    replay
        .instantiate_program(&succ_spec, inst_ok, cid_ok, owner)
        .expect("i1");
    replay
        .instantiate_program(&succ_spec, inst_trap, cid_trap, owner)
        .expect("i2");
    let _ = replay
        .execute_instance(&succ_spec, inst_ok, slot_a, 200)
        .expect("e1");
    let _ = replay
        .execute_instance(&succ_spec, inst_trap, slot_b, 200)
        .expect("e2");
    assert_eq!(replay.roots(), eng.roots());
    assert_eq!(replay.verify_rebuild_parity().expect("parity"), eng.roots());
}

#[test]
fn program_state_1_adversarial_malformed_code_stale_version_and_cross_domain_proofs() {
    let vm_spec = VmSpecD1::v1_default();
    let vm_bounds = LocalVmBoundsV1::v1_default(&vm_spec).expect("bounds");
    let (_, succ_spec) = canonical_program_state_profile_specs().expect("specs");

    let mut code =
        ProgramCodeRecordV1::from_opcodes(&[SpatialOpcodeV1::PushOne, SpatialOpcodeV1::Halt])
            .expect("valid");

    // 1. Tampered availability digest (unavailable bytes) rejected fail-closed.
    let mut bad_avail = code.clone();
    bad_avail.availability_digest[0] ^= 0xFF;
    assert!(matches!(
        bad_avail.validate(),
        Err(ProgramState1Error::InvalidCode(_))
    ));

    // 2. Stale grammar version rejected fail-closed.
    let mut stale_ver = code.clone();
    stale_ver.grammar_version = 99;
    assert!(matches!(
        stale_ver.validate(),
        Err(ProgramState1Error::InvalidCode(_))
    ));

    // 3. Malformed opcode byte (> 7) rejected fail-closed.
    code.opcode_bytes.push(8);
    assert!(matches!(
        code.validate(),
        Err(ProgramState1Error::InvalidCode(_))
    ));

    // 4. Zero gas limit and over-bound gas limit rejected fail-closed.
    let valid_code =
        ProgramCodeRecordV1::from_opcodes(&[SpatialOpcodeV1::PushOne, SpatialOpcodeV1::Halt])
            .expect("valid");
    let mut eng = ProgramStateEngineV1::empty(vm_spec, vm_bounds).expect("empty");
    let cid = eng.register_code(&succ_spec, valid_code).expect("reg");
    let inst = [0xAAu8; 32];
    eng.instantiate_program(&succ_spec, inst, cid, [0xBBu8; 32])
        .expect("inst");
    assert!(matches!(
        eng.execute_instance(&succ_spec, inst, [0xCCu8; 32], 0),
        Err(ProgramState1Error::UnchargedOrOverBoundWork(_))
    ));
    assert!(matches!(
        eng.execute_instance(
            &succ_spec,
            inst,
            [0xCCu8; 32],
            vm_bounds.max_gas_per_program + 1
        ),
        Err(ProgramState1Error::UnchargedOrOverBoundWork(_))
    ));

    // 5. Cross-domain proof rejection: Code proof verified against Instance root MUST fail, and vice versa.
    let roots = eng.roots();
    let code_proof = eng.prove_code(cid).expect("prove code");
    let inst_proof = eng.prove_instance(inst).expect("prove inst");
    code_proof
        .verify(ProgramTreeDomainV1::Code, roots.code_root)
        .expect("code proof ok");
    inst_proof
        .verify(ProgramTreeDomainV1::Instance, roots.instance_root)
        .expect("inst proof ok");
    assert!(matches!(
        code_proof.verify(ProgramTreeDomainV1::Instance, roots.instance_root),
        Err(ProgramState1Error::ProofVerificationFailed(_))
    ));
    assert!(matches!(
        inst_proof.verify(ProgramTreeDomainV1::Code, roots.code_root),
        Err(ProgramState1Error::ProofVerificationFailed(_))
    ));
}
