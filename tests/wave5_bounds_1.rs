#![cfg(feature = "protocol-research")]
//! End-to-end CI verification suite for Wave 5 (`BOUNDS-1`):
//! - Canonical `ProtocolBoundsProfileV1`, `LocalVmBoundsV1` (`CE-05`), and `SafeHaltEvidenceV1`
//! - Literal golden `ContentTag` parity with `tools/bounds_1_vectors.py`
//! - Exhaustive 1-over pre-allocation preflight, arithmetic overflow, widening refusal, per-peer
//!   DoS caps, consensus safe-halt evidence, and `VM-D1` batch preflight checks.

use bitblock::bounds_1::{
    BOUNDS_1_LOCAL_VM_PROFILE_SCHEMA, BOUNDS_1_PROTOCOL_PROFILE_SCHEMA,
    BOUNDS_1_SAFE_HALT_EVIDENCE_SCHEMA, BlockEnvelopePreflightHeaderV1, Bounds1Error,
    DEFAULT_MAX_ALLOC_BYTES_PER_BLOCK, DEFAULT_MAX_BLOCK_BYTES, DEFAULT_MAX_BLOCK_OPS,
    DEFAULT_MAX_BLOCK_SIGNATURES, DEFAULT_MAX_BLOCK_TXS, DEFAULT_MAX_HASHES_PER_BLOCK,
    DEFAULT_MAX_PEER_INFLIGHT_BYTES, DEFAULT_MAX_PEER_REQUESTS_PER_SEC, DEFAULT_MAX_PROOF_BYTES,
    DEFAULT_MAX_PROOF_DEPTH, DEFAULT_MAX_STATE_WRITES_PER_BLOCK, DEFAULT_MAX_TX_BYTES,
    DEFAULT_MAX_VERIFY_BUDGET_MS, LocalVmBoundsV1, ProtocolBoundsProfileV1, VIOLATION_ALLOC_BUDGET,
    VIOLATION_ARITHMETIC_OVERFLOW, VIOLATION_BLOCK_BYTES, VIOLATION_BLOCK_OPS,
    VIOLATION_BLOCK_SIGNATURES, VIOLATION_BLOCK_TXS, VIOLATION_HASH_BUDGET, VIOLATION_LOCAL_VM,
    VIOLATION_PEER_DOS, VIOLATION_PROOF_BYTES, VIOLATION_PROOF_DEPTH, VIOLATION_STATE_WRITES,
    VIOLATION_TX_BYTES,
};
use bitblock::commit_state::DomainRegistryV1;
use bitblock::consensus_gov_d1::GovernancePolicyDescriptorV1;
use bitblock::crypto_d1::CanonicalEd25519PublicKeyV1;
use bitblock::identity_d1::{CredentialRoleV1, ThresholdPolicyV1};
use bitblock::language_r1::{ParsedSpatialProgramV1, SpatialOpcodeV1};
use bitblock::local_bounds::LocalStorageProfile;
use bitblock::profile_bootstrap::ProfileScopeV1;
use bitblock::threat_1::ThreatModelV1;
use bitblock::vm_d1::VmSpecD1;
use bitblock::world::Coord;

fn sample_threat() -> ThreatModelV1 {
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
    let scope = ProfileScopeV1::from_wave1_contracts(
        &DomainRegistryV1::local_v1(),
        &gov,
        vec![
            "asset-active".to_string(),
            "economic-charging".to_string(),
            "stake-consensus".to_string(),
            "validator-economics".to_string(),
        ],
    )
    .unwrap();
    ThreatModelV1::v1_conservative_consumer(&scope).unwrap()
}

#[test]
fn bounds_1_golden_tags_preflight_safe_halt_and_local_vm_ce05() {
    assert_eq!(
        BOUNDS_1_PROTOCOL_PROFILE_SCHEMA,
        b"bitblock/bounds-1/protocol-profile/v1"
    );
    assert_eq!(
        BOUNDS_1_LOCAL_VM_PROFILE_SCHEMA,
        b"bitblock/bounds-1/local-vm-bounds/v1"
    );
    assert_eq!(
        BOUNDS_1_SAFE_HALT_EVIDENCE_SCHEMA,
        b"bitblock/bounds-1/safe-halt-evidence/v1"
    );
    assert_eq!(DEFAULT_MAX_VERIFY_BUDGET_MS, 400);

    let threat = sample_threat();
    let profile =
        ProtocolBoundsProfileV1::v1_consumer_default(&threat, &LocalStorageProfile::DEFAULT)
            .unwrap();
    assert_eq!(
        profile.tag().unwrap().to_string(),
        "#bb.policy.sha256.yecxf5k5a5gs3y7qm76pzax2f5e6bkidmxytmsthsxhdqozzplaa"
    );

    let vm_spec = VmSpecD1::v1_default();
    let vm_bounds = LocalVmBoundsV1::v1_default(&vm_spec).unwrap();
    assert_eq!(
        vm_bounds.tag().unwrap().to_string(),
        "#bb.policy.sha256.o2xw5sig3l5n43lfwyxh5cpsa5kaydtgtrqerw4liahezlrn7j6a"
    );

    // Worst-shaped valid block header passes pre-allocation preflight cleanly.
    let valid = BlockEnvelopePreflightHeaderV1 {
        block_bytes: u64::from(DEFAULT_MAX_BLOCK_BYTES),
        tx_count: u64::from(DEFAULT_MAX_BLOCK_TXS),
        max_tx_bytes_observed: u64::from(DEFAULT_MAX_TX_BYTES),
        signature_count: u64::from(DEFAULT_MAX_BLOCK_SIGNATURES),
        op_count: u64::from(DEFAULT_MAX_BLOCK_OPS),
        state_writes: u64::from(DEFAULT_MAX_STATE_WRITES_PER_BLOCK),
        max_proof_bytes_observed: u64::from(DEFAULT_MAX_PROOF_BYTES),
        max_proof_depth_observed: u64::from(DEFAULT_MAX_PROOF_DEPTH),
        estimated_hashes: u64::from(DEFAULT_MAX_HASHES_PER_BLOCK),
        estimated_alloc_bytes: u64::from(DEFAULT_MAX_ALLOC_BYTES_PER_BLOCK)
            - u64::from(DEFAULT_MAX_BLOCK_BYTES),
    };
    profile.preflight_block_envelope(&valid).unwrap();

    // Exact 1-over rejections across every bounded dimension + checked u64 overflow.
    let cases: &[(BlockEnvelopePreflightHeaderV1, u8)] = &[
        (
            BlockEnvelopePreflightHeaderV1 {
                block_bytes: valid.block_bytes + 1,
                estimated_alloc_bytes: 0,
                ..valid
            },
            VIOLATION_BLOCK_BYTES,
        ),
        (
            BlockEnvelopePreflightHeaderV1 {
                max_tx_bytes_observed: valid.max_tx_bytes_observed + 1,
                ..valid
            },
            VIOLATION_TX_BYTES,
        ),
        (
            BlockEnvelopePreflightHeaderV1 {
                tx_count: valid.tx_count + 1,
                ..valid
            },
            VIOLATION_BLOCK_TXS,
        ),
        (
            BlockEnvelopePreflightHeaderV1 {
                signature_count: valid.signature_count + 1,
                ..valid
            },
            VIOLATION_BLOCK_SIGNATURES,
        ),
        (
            BlockEnvelopePreflightHeaderV1 {
                op_count: valid.op_count + 1,
                ..valid
            },
            VIOLATION_BLOCK_OPS,
        ),
        (
            BlockEnvelopePreflightHeaderV1 {
                state_writes: valid.state_writes + 1,
                ..valid
            },
            VIOLATION_STATE_WRITES,
        ),
        (
            BlockEnvelopePreflightHeaderV1 {
                max_proof_bytes_observed: valid.max_proof_bytes_observed + 1,
                ..valid
            },
            VIOLATION_PROOF_BYTES,
        ),
        (
            BlockEnvelopePreflightHeaderV1 {
                max_proof_depth_observed: valid.max_proof_depth_observed + 1,
                ..valid
            },
            VIOLATION_PROOF_DEPTH,
        ),
        (
            BlockEnvelopePreflightHeaderV1 {
                estimated_hashes: valid.estimated_hashes + 1,
                ..valid
            },
            VIOLATION_HASH_BUDGET,
        ),
        (
            BlockEnvelopePreflightHeaderV1 {
                estimated_alloc_bytes: valid.estimated_alloc_bytes + 1,
                ..valid
            },
            VIOLATION_ALLOC_BUDGET,
        ),
        (
            BlockEnvelopePreflightHeaderV1 {
                estimated_alloc_bytes: u64::MAX,
                ..valid
            },
            VIOLATION_ARITHMETIC_OVERFLOW,
        ),
    ];
    for (hdr, expected_code) in cases {
        let err = profile.preflight_block_envelope(hdr).unwrap_err();
        assert_eq!(err.code, *expected_code);
    }

    // Over-bound consensus-accepted artifact emits exact golden SafeHaltEvidenceV1!
    let halt = profile
        .enforce_consensus_accepted_bounds([0xAB; 32], &cases[0].0)
        .unwrap_err();
    assert_eq!(
        halt.tag().to_string(),
        "#bb.receipt.sha256.blh4qbetfmqq2yu7pq32phgsyu66mqakmf7ffjfmkorzvxkbfvta"
    );

    // tighten_only accepts stricter profile and rejects any 1-unit widening fail-closed.
    let mut stricter = profile;
    stricter.max_state_writes_per_block = 2_048;
    assert_eq!(profile.tighten_only(stricter).unwrap(), stricter);
    let mut widened = profile;
    widened.max_state_writes_per_block += 1;
    assert!(matches!(
        profile.tighten_only(widened),
        Err(Bounds1Error::WideningForbidden(_))
    ));

    // Per-peer DoS budget checks
    profile
        .check_peer_budget(
            u64::from(DEFAULT_MAX_PEER_INFLIGHT_BYTES),
            u64::from(DEFAULT_MAX_PEER_REQUESTS_PER_SEC),
        )
        .unwrap();
    assert_eq!(
        profile
            .check_peer_budget(u64::from(DEFAULT_MAX_PEER_INFLIGHT_BYTES) + 1, 1)
            .unwrap_err()
            .code,
        VIOLATION_PEER_DOS
    );
    assert_eq!(
        profile
            .check_peer_budget(1, u64::from(DEFAULT_MAX_PEER_REQUESTS_PER_SEC) + 1)
            .unwrap_err()
            .code,
        VIOLATION_PEER_DOS
    );

    // CE-05 LocalVmBoundsV1 preflight
    let prog = ParsedSpatialProgramV1::new(
        3,
        Coord::new(3, 0, 2),
        vec![
            SpatialOpcodeV1::PushOne,
            SpatialOpcodeV1::PushOne,
            SpatialOpcodeV1::Add,
            SpatialOpcodeV1::Halt,
        ],
    )
    .unwrap();
    assert_eq!(
        vm_bounds
            .preflight_vm_batch(&vm_spec, std::slice::from_ref(&prog), &[4])
            .unwrap(),
        48
    );
    let too_many: Vec<_> = (0..65).map(|_| prog.clone()).collect();
    let widths = vec![4u8; 65];
    assert_eq!(
        vm_bounds
            .preflight_vm_batch(&vm_spec, &too_many, &widths)
            .unwrap_err()
            .code,
        VIOLATION_LOCAL_VM
    );
}
