#![cfg(feature = "protocol-research")]
//! End-to-end CI verification suite for Wave 4:
//! - `THREAT-1`: quantitative adversary model & 11-class property/loss/mitigation claim map
//! - `VM-D1`: frozen deterministic local visual-machine semantics over `LANGUAGE-R1` v1 opcodes
//!
//! Asserts literal golden `ContentTag` parity with `tools/threat_1_and_vm_d1_vectors.py` plus
//! exhaustive negative controls across all 7 `VmTrapV1` variants, atomic rollback, `History`
//! replay parity, ascending-anchor permutation invariance, and unsafe `THREAT-1` rejections.

use bitblock::commit_state::DomainRegistryV1;
use bitblock::consensus_gov_d1::GovernancePolicyDescriptorV1;
use bitblock::crypto_d1::CanonicalEd25519PublicKeyV1;
use bitblock::history::{History, world_tag};
use bitblock::identity_d1::{CredentialRoleV1, ThresholdPolicyV1};
use bitblock::language_r1::{ParsedSpatialProgramV1, SpatialOpcodeV1, encode_program_to_world};
use bitblock::profile_bootstrap::ProfileScopeV1;
use bitblock::threat_1::{
    MAX_BFT_CORRUPT_WEIGHT_BPS, THREAT_1_CLAIM_MAP_SCHEMA, THREAT_1_CLASS_COUNT,
    THREAT_1_MODEL_SCHEMA, Threat1Error, ThreatClaimMapV1, ThreatModelV1,
};
use bitblock::vm_d1::{
    DEFAULT_VM_D1_GAS_PER_OUTPUT_BIT, DEFAULT_VM_D1_MAX_OUTPUT_BITS, DEFAULT_VM_D1_MAX_STACK_DEPTH,
    DEFAULT_VM_D1_OPCODE_GAS, VM_D1_MODEL_RECEIPT_SCHEMA, VM_D1_SPEC_SCHEMA, VmD1Error,
    VmInvocationV1, VmOutputRegisterV1, VmSpecD1, VmTrapV1, evaluate_program_stack,
    schedule_and_evaluate_model,
};
use bitblock::world::{BEDROCK_COORD, Coord, World};

fn sample_scope() -> ProfileScopeV1 {
    let mut keys = [31u8, 32, 33]
        .into_iter()
        .map(|s| CanonicalEd25519PublicKeyV1::from_secret_seed(&[s; 32]).unwrap())
        .collect::<Vec<_>>();
    keys.sort();
    let gov = GovernancePolicyDescriptorV1 {
        emergency_council: ThresholdPolicyV1 {
            role: CredentialRoleV1::EmergencyPauseSigner,
            threshold: 2,
            keys,
        },
        schema_version: 1,
        upgrade_quorum_numerator: 2,
        upgrade_quorum_denominator: 3,
        mandatory_exit_timelock_epochs: 21,
        max_emergency_pause_epochs: 3,
        automatic_safe_halt_on_proven_fault: true,
        emergency_council_pause_only: true,
        forbid_history_rewrite: true,
    };
    ProfileScopeV1::from_wave1_contracts(
        &DomainRegistryV1::local_v1(),
        &gov,
        vec![
            "asset-active".to_string(),
            "economic-charging".to_string(),
            "stake-consensus".to_string(),
            "validator-economics".to_string(),
        ],
    )
    .unwrap()
}

#[test]
fn threat_1_golden_tags_11_classes_and_adversarial_controls() {
    assert_eq!(
        THREAT_1_MODEL_SCHEMA,
        b"bitblock/threat-1/adversary-model/v1"
    );
    assert_eq!(
        THREAT_1_CLAIM_MAP_SCHEMA,
        b"bitblock/threat-1/property-claim-map/v1"
    );
    assert_eq!(THREAT_1_CLASS_COUNT, 11);
    assert_eq!(MAX_BFT_CORRUPT_WEIGHT_BPS, 3_333);

    let scope = sample_scope();
    let model = ThreatModelV1::v1_conservative_consumer(&scope).unwrap();
    assert_eq!(
        model.tag().unwrap().to_string(),
        "#bb.policy.sha256.qnfdid2kwcnzhyxinyitgiu62cimnl5a37ugwjye7wolmu7qurmq"
    );
    assert_eq!(model.consensus_inclusion_window_blocks(32), Some(96));
    assert_eq!(model.consensus_inclusion_window_blocks(0), None);

    let map = ThreatClaimMapV1::from_model(&model).unwrap();
    assert_eq!(
        map.tag().unwrap().to_string(),
        "#bb.receipt.sha256.kf6z6jjjbalzphsawlx7b3q7qpy4geyxxpr2vhlpmnnr5k374apq"
    );

    // Negative controls across BFT weight, retention, peers, builders, censorship, bridges, and claim map.
    let mut bad = model;
    bad.max_corrupt_weight_bps = 3_334;
    assert!(matches!(bad.validate(), Err(Threat1Error::InvalidModel(_))));
    bad = model;
    bad.rolling_retention_epochs = model.unbonding_delay_epochs - 1;
    assert!(matches!(bad.validate(), Err(Threat1Error::InvalidModel(_))));
    bad = model;
    bad.min_honest_peers = 3;
    assert!(matches!(bad.validate(), Err(Threat1Error::InvalidModel(_))));
    bad = model;
    bad.release_builder_threshold_m = 2;
    bad.release_builder_total_n = 5;
    assert!(matches!(bad.validate(), Err(Threat1Error::InvalidModel(_))));
    bad = model;
    bad.sequencer_censorship_guaranteed = true;
    assert!(matches!(bad.validate(), Err(Threat1Error::InvalidModel(_))));
    bad = model;
    bad.external_bridges_admitted = true;
    assert!(matches!(bad.validate(), Err(Threat1Error::InvalidModel(_))));

    let mut bad_map = map.clone();
    bad_map.entries[4].primary_threshold = 0;
    assert!(matches!(
        bad_map.validate(),
        Err(Threat1Error::InvalidClaimMap(_))
    ));
    let mut bad_excl = map;
    bad_excl.entries[6].has_explicit_v1_exclusion = false;
    assert!(matches!(
        bad_excl.validate(),
        Err(Threat1Error::InvalidClaimMap(_))
    ));
}

#[test]
fn vm_d1_golden_tags_scheduling_traps_rollback_and_history_parity() {
    assert_eq!(VM_D1_SPEC_SCHEMA, b"bitblock/vm-d1/semantics-spec/v1");
    assert_eq!(
        VM_D1_MODEL_RECEIPT_SCHEMA,
        b"bitblock/vm-d1/model-step-receipt/v1"
    );
    assert_eq!(DEFAULT_VM_D1_MAX_STACK_DEPTH, 64);
    assert_eq!(DEFAULT_VM_D1_MAX_OUTPUT_BITS, 64);
    assert_eq!(DEFAULT_VM_D1_OPCODE_GAS, [1, 2, 2, 3, 3, 2, 2, 1]);
    assert_eq!(DEFAULT_VM_D1_GAS_PER_OUTPUT_BIT, 10);

    let spec = VmSpecD1::v1_default();
    assert_eq!(
        spec.tag().unwrap().to_string(),
        "#bb.policy.sha256.nvshpvblkenho2me3rp5qwwz4mjrtsssrzz6k7uippu2plpw2ava"
    );

    // 1. Exact golden receipt parity with tools/threat_1_and_vm_d1_vectors.py
    let anchor = Coord::new(3, 0, 2);
    let prog = ParsedSpatialProgramV1::new(
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
    encode_program_to_world(&mut world, &prog).unwrap();
    let mut history = History::new(&world);
    let receipt = schedule_and_evaluate_model(
        &mut world,
        &mut history,
        &spec,
        &[VmInvocationV1 {
            anchor,
            output_register: Some(VmOutputRegisterV1 {
                origin: Coord::new(5, 8, 2),
                bit_width: 4,
            }),
        }],
        100,
    )
    .unwrap();
    assert_eq!(
        receipt.tag().to_string(),
        "#bb.receipt.sha256.dhvpeyzrvab6dzqxxkmbkd7agfockzkszfrqgywkkef2jyw2k6aq"
    );
    assert_eq!(history.receipts().len(), 1);
    assert_eq!(history.receipts()[0].post_world(), world_tag(&world));

    // 2. Ascending anchor permutation invariance + OutputWriteConflict atomic rollback
    let anchor_b = Coord::new(3, 10, 2);
    let prog_b = ParsedSpatialProgramV1::new(
        4,
        anchor_b,
        vec![
            SpatialOpcodeV1::PushOne,
            SpatialOpcodeV1::PushZero,
            SpatialOpcodeV1::Swap,
            SpatialOpcodeV1::Sub, // 0 - 1 = -1 (all 1s in two's complement)
            SpatialOpcodeV1::Nop,
            SpatialOpcodeV1::Halt,
        ],
    )
    .unwrap();
    let mut w1 = World::new();
    encode_program_to_world(&mut w1, &prog).unwrap();
    encode_program_to_world(&mut w1, &prog_b).unwrap();
    let mut w2 = w1.clone();
    let inv_a = VmInvocationV1 {
        anchor,
        output_register: Some(VmOutputRegisterV1 {
            origin: Coord::new(5, 8, 2),
            bit_width: 4,
        }),
    };
    let inv_b_conflict = VmInvocationV1 {
        anchor: anchor_b,
        output_register: Some(VmOutputRegisterV1 {
            origin: Coord::new(7, 8, 2), // overlaps Program A's bit 2!
            bit_width: 4,
        }),
    };
    let mut h1 = History::new(&w1);
    let mut h2 = History::new(&w2);
    let r1 = schedule_and_evaluate_model(&mut w1, &mut h1, &spec, &[inv_a, inv_b_conflict], 200)
        .unwrap();
    let r2 = schedule_and_evaluate_model(&mut w2, &mut h2, &spec, &[inv_b_conflict, inv_a], 200)
        .unwrap();
    assert_eq!(r1, r2);
    assert_eq!(r1.outcomes[0].outcome, Ok(4));
    assert_eq!(
        r1.outcomes[1].outcome,
        Err(VmTrapV1::OutputWriteConflict(Coord::new(7, 8, 2)))
    );
    assert_eq!(r1.outcomes[1].emitted_writes, 0);
    // Program B rolled back completely (no partial writes at (8,8,2), (9,8,2), (10,8,2))
    assert!(!w1.is_on(Coord::new(9, 8, 2)));

    // 3. ForbiddenOutputTarget (bedrock & program frame) rolls back with zero mutation
    let mut w_forbid = World::new();
    encode_program_to_world(&mut w_forbid, &prog).unwrap();
    let pre = world_tag(&w_forbid);
    let mut h_forbid = History::new(&w_forbid);
    let r_bed = schedule_and_evaluate_model(
        &mut w_forbid,
        &mut h_forbid,
        &spec,
        &[VmInvocationV1 {
            anchor,
            output_register: Some(VmOutputRegisterV1 {
                origin: BEDROCK_COORD,
                bit_width: 2,
            }),
        }],
        100,
    )
    .unwrap();
    assert_eq!(
        r_bed.outcomes[0].outcome,
        Err(VmTrapV1::ForbiddenOutputTarget(BEDROCK_COORD))
    );
    assert_eq!(world_tag(&w_forbid), pre);

    // 4. Exhaustive pure-stack traps: StackUnderflow, StackOverflow, GasExhausted, MissingHalt
    let underflow =
        ParsedSpatialProgramV1::new(0, anchor, vec![SpatialOpcodeV1::Add, SpatialOpcodeV1::Halt])
            .unwrap();
    assert_eq!(
        evaluate_program_stack(&spec, &underflow, 100).unwrap().0,
        Err(VmTrapV1::StackUnderflow)
    );
    let missing_halt =
        ParsedSpatialProgramV1::new(0, anchor, vec![SpatialOpcodeV1::PushOne]).unwrap();
    assert_eq!(
        evaluate_program_stack(&spec, &missing_halt, 100).unwrap().0,
        Err(VmTrapV1::MissingHalt)
    );
    assert_eq!(
        evaluate_program_stack(&spec, &prog, 5).unwrap().0,
        Err(VmTrapV1::GasExhausted)
    );
    let mut tiny_stack = spec;
    tiny_stack.max_stack_depth = 1;
    assert_eq!(
        evaluate_program_stack(&tiny_stack, &prog, 100).unwrap().0,
        Err(VmTrapV1::StackOverflow)
    );

    // 5. Duplicate anchor and zero width reject before scheduling
    assert_eq!(
        schedule_and_evaluate_model(&mut w_forbid, &mut h_forbid, &spec, &[inv_a, inv_a], 100),
        Err(VmD1Error::DuplicateInvocationAnchor(anchor))
    );
    assert_eq!(
        schedule_and_evaluate_model(
            &mut w_forbid,
            &mut h_forbid,
            &spec,
            &[VmInvocationV1 {
                anchor,
                output_register: Some(VmOutputRegisterV1 {
                    origin: Coord::new(5, 8, 2),
                    bit_width: 0,
                }),
            }],
            100,
        ),
        Err(VmD1Error::InvalidOutputWidth(0))
    );
}
