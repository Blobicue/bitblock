//! Release measurement probe for `PROGRAM-STATE-1@base` (`PS-048`): benchmarks canonical code
//! registration, instantiation, deterministic `VM-1` execution, atomic trap rollback, domain-separated
//! Merkle proof verification, and cold-rebuild parity under `--features protocol-research`.

#[cfg(not(feature = "protocol-research"))]
fn main() {
    eprintln!("Run with --features protocol-research");
    std::process::exit(1);
}

#[cfg(feature = "protocol-research")]
fn main() {
    use std::time::Instant;

    use bitblock::bounds_1::LocalVmBoundsV1;
    use bitblock::language_r1::SpatialOpcodeV1;
    use bitblock::program_state_1::{
        ProgramCodeRecordV1, ProgramExecDispositionV1, ProgramStateEngineV1, ProgramTreeDomainV1,
        canonical_program_state_profile_specs,
    };
    use bitblock::vm_d1::VmSpecD1;

    let vm_spec = VmSpecD1::v1_default();
    let vm_bounds = LocalVmBoundsV1::v1_default(&vm_spec).expect("bounds");
    let (_, succ_spec) = canonical_program_state_profile_specs().expect("specs");

    let code_ok = ProgramCodeRecordV1::from_opcodes(&[
        SpatialOpcodeV1::PushOne,
        SpatialOpcodeV1::Dup,
        SpatialOpcodeV1::Add,
        SpatialOpcodeV1::Dup,
        SpatialOpcodeV1::Add,
        SpatialOpcodeV1::Halt,
    ])
    .expect("code ok");
    let code_trap = ProgramCodeRecordV1::from_opcodes(&[
        SpatialOpcodeV1::PushOne,
        SpatialOpcodeV1::Add,
        SpatialOpcodeV1::Halt,
    ])
    .expect("code trap");

    let mut reg_ns = Vec::with_capacity(64);
    let mut exec_ns = Vec::with_capacity(64);
    let mut trap_ns = Vec::with_capacity(64);
    let mut prove_ns = Vec::with_capacity(64);
    let mut verify_ns = Vec::with_capacity(64);
    let mut parity_ns = Vec::with_capacity(64);

    let mut last_code_tag = String::new();
    let mut last_roots_tag = String::new();
    let mut last_exec_tag = String::new();

    for run in 0u8..64 {
        let mut eng = ProgramStateEngineV1::empty(vm_spec, vm_bounds).expect("empty");
        let t0 = Instant::now();
        let cid_ok = eng.register_code(&succ_spec, code_ok.clone()).expect("reg");
        reg_ns.push(t0.elapsed().as_nanos() as u64);
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

        let t1 = Instant::now();
        let r_ok = eng
            .execute_instance(&succ_spec, inst_ok, slot_a, 200)
            .expect("exec ok");
        exec_ns.push(t1.elapsed().as_nanos() as u64);
        assert_eq!(r_ok.disposition, ProgramExecDispositionV1::Admitted);

        let t2 = Instant::now();
        let r_trap = eng
            .execute_instance(&succ_spec, inst_trap, slot_b, 200)
            .expect("exec trap");
        trap_ns.push(t2.elapsed().as_nanos() as u64);
        assert_eq!(
            r_trap.disposition,
            ProgramExecDispositionV1::TrappedAtomicRollback
        );

        let t3 = Instant::now();
        let proof = eng.prove_instance(inst_ok).expect("prove");
        prove_ns.push(t3.elapsed().as_nanos() as u64);

        let roots = eng.roots();
        let t4 = Instant::now();
        proof
            .verify(ProgramTreeDomainV1::Instance, roots.instance_root)
            .expect("verify");
        verify_ns.push(t4.elapsed().as_nanos() as u64);

        let t5 = Instant::now();
        let _ = eng.verify_rebuild_parity().expect("parity");
        parity_ns.push(t5.elapsed().as_nanos() as u64);

        if run == 63 {
            last_code_tag = code_ok.tag().expect("ctag").to_string();
            last_roots_tag = roots.tag().expect("rtag").to_string();
            last_exec_tag = r_ok.tag().expect("etag").to_string();
        }
    }

    for v in [
        &mut reg_ns,
        &mut exec_ns,
        &mut trap_ns,
        &mut prove_ns,
        &mut verify_ns,
        &mut parity_ns,
    ] {
        v.sort_unstable();
    }
    let p50_us = |v: &[u64]| (v[v.len() / 2] as f64) / 1_000.0;
    println!(
        "{{\n  \"runs\": 64,\n  \"codeTag\": \"{}\",\n  \"rootsTag\": \"{}\",\n  \"execReceiptTag\": \"{}\",\n  \"registerCodeP50Us\": {:.2},\n  \"executeAdmittedP50Us\": {:.2},\n  \"trapRollbackP50Us\": {:.2},\n  \"proveInstanceP50Us\": {:.2},\n  \"verifyProofP50Us\": {:.2},\n  \"rebuildParityP50Us\": {:.2}\n}}",
        last_code_tag,
        last_roots_tag,
        last_exec_tag,
        p50_us(&reg_ns),
        p50_us(&exec_ns),
        p50_us(&trap_ns),
        p50_us(&prove_ns),
        p50_us(&verify_ns),
        p50_us(&parity_ns),
    );
}
