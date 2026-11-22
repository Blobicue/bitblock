#![cfg(feature = "protocol-research")]
//! End-to-end CI verification suite for Wave 3:
//! - `PROFILE-SCOPE-D0`, `GENESIS-D1`, and `PROTOCOL-ID-1` non-self-referential pre-ID pipeline
//! - `LANGUAGE-R1` strict `+X/+Y/+Z` framed 3D spatial grammar parser and canonical source codec
//!
//! Asserts literal golden `ContentTag` parity with `tools/profile_bootstrap_vectors.py` and
//! `tools/language_r1_vectors.py`, plus exhaustive adversarial negative controls.

use bitblock::commit_state::DomainRegistryV1;
use bitblock::consensus_gov_d1::GovernancePolicyDescriptorV1;
use bitblock::crypto_d1::{
    CanonicalEd25519PublicKeyV1, HashSuiteIdV1, SignatureSuiteIdV1, SigningDomainTagV1,
    SigningPreimageV1, sign_preimage_deterministic,
};
use bitblock::identity_d1::{
    ActorIdV1, CredentialRoleV1, NetworkEditOpV1, ThresholdPolicyV1, TransactionBodyV1,
};
use bitblock::language_r1::{
    LANGUAGE_R1_PARSE_REPORT_SCHEMA, LANGUAGE_R1_PROGRAM_SCHEMA, MAX_SPATIAL_INSTRUCTIONS,
    ParsedSpatialProgramV1, SPATIAL_FRAME_Y, SPATIAL_FRAME_Z, SPATIAL_GRAMMAR_VERSION,
    SpatialOpcodeV1, SpatialParseError, SpatialParseReportV1, chiral_fiducial_offsets,
    encode_program_to_world, parse_program_at, parse_world_programs, spatial_program_voxels,
};
use bitblock::profile_bootstrap::{
    AcceptedParentCollisionDispositionV1, AcceptedProposalCoreV1, BootstrapError,
    ChainGenesisPayloadV1, GENESIS_D1_PAYLOAD_SCHEMA, MAX_SCOPE_PREDICATES,
    PROFILE_SCOPE_D0_SCHEMA, PROTOCOL_ID_1_PROPOSAL_CORE_SCHEMA, ProfileScopeV1,
    ProposedLimitVectorV1, classify_parent_collision, verify_protocol_id_graph_acyclic,
    verify_signed_transaction_envelope,
};
use bitblock::world::{Coord, World};

fn pk(seed: u8) -> CanonicalEd25519PublicKeyV1 {
    CanonicalEd25519PublicKeyV1::from_secret_seed(&[seed; 32]).unwrap()
}

#[test]
fn profile_bootstrap_golden_tags_signed_tx_and_negative_controls() {
    assert_eq!(
        PROFILE_SCOPE_D0_SCHEMA,
        b"bitblock/profile-scope-d0/scope/v1"
    );
    assert_eq!(
        GENESIS_D1_PAYLOAD_SCHEMA,
        b"bitblock/genesis-d1/chain-genesis-payload/v1"
    );
    assert_eq!(
        PROTOCOL_ID_1_PROPOSAL_CORE_SCHEMA,
        b"bitblock/protocol-id-1/accepted-proposal-core/v1"
    );
    assert_eq!(MAX_SCOPE_PREDICATES, 32);

    let mut ckeys = vec![pk(31), pk(32), pk(33)];
    ckeys.sort();
    let gov = GovernancePolicyDescriptorV1 {
        schema_version: 1,
        upgrade_quorum_numerator: 2,
        upgrade_quorum_denominator: 3,
        mandatory_exit_timelock_epochs: 21,
        max_emergency_pause_epochs: 3,
        automatic_safe_halt_on_proven_fault: true,
        emergency_council_pause_only: true,
        forbid_history_rewrite: true,
        emergency_council: ThresholdPolicyV1 {
            role: CredentialRoleV1::EmergencyPauseSigner,
            threshold: 2,
            keys: ckeys,
        },
    };
    let dom = DomainRegistryV1::local_v1();
    let scope = ProfileScopeV1::from_wave1_contracts(
        &dom,
        &gov,
        vec![
            "asset-active".to_string(),
            "economic-charging".to_string(),
            "stake-consensus".to_string(),
            "validator-economics".to_string(),
        ],
    )
    .unwrap();
    assert_eq!(
        scope.tag().unwrap().to_string(),
        "#bb.policy.sha256.yzu3porxqsiiivkwgepymeiuphc7er4wdl226g6go4kfaingn2eq"
    );

    // Reject reserved "always", unknown predicate, missing PE-01..PE-10 entailment, unsorted, zero limits.
    let mut bad_scope = scope.clone();
    bad_scope.activated_predicates = vec!["always".to_string()];
    assert!(matches!(
        bad_scope.validate(),
        Err(BootstrapError::InvalidPredicateToken(_))
    ));
    bad_scope.activated_predicates = vec!["unknown-token".to_string()];
    assert!(matches!(
        bad_scope.validate(),
        Err(BootstrapError::InvalidPredicateToken(_))
    ));
    bad_scope.activated_predicates = vec!["stake-consensus".to_string()];
    assert!(matches!(
        bad_scope.validate(),
        Err(BootstrapError::InvalidPredicateToken(_))
    ));
    let mut zero_lim = ProposedLimitVectorV1::v1_default();
    zero_lim.max_proof_bytes = 0;
    assert!(zero_lim.validate().is_err());

    let genesis = ChainGenesisPayloadV1 {
        genesis_schema_version: 1,
        initial_profile_scope_digest: scope.tag().unwrap().digest(),
        initial_domain_registry_digest: dom.tag().digest(),
        chain_id: 1,
        world_id: 1,
        identity_namespace: "bitblock-mainnet-v1".to_string(),
        initial_world_digest: [0x11; 32],
        initial_actor_set_root: [0x22; 32],
        initial_validator_set_root: [0x33; 32],
        initial_native_supply: 1_000_000_000_000_000,
    };
    assert_eq!(
        genesis.tag().unwrap().to_string(),
        "#bb.policy.sha256.q7pz74ykis4uuqina62xd6immkzczt4c77y6g4elrum5z37xdhcq"
    );
    let genesis_id = genesis.tag().unwrap().digest();

    let core = AcceptedProposalCoreV1 {
        protocol_version: 1,
        chain_id: 1,
        genesis_id,
        profile_id: [0x44; 32],
        height: 1,
        epoch: 0,
        parent_core_id: genesis_id,
        active_validator_root: [0x33; 32],
        execution_state_root: [0x55; 32],
        history_root: [0x66; 32],
        body_digest: [0x77; 32],
    };
    assert_eq!(
        core.tag().to_string(),
        "#bb.receipt.sha256.7a5oxjjdyl5sfkec3va6ziw4q5mxomxmqkwhsfrkwuay6mmfr65q"
    );
    assert_eq!(
        classify_parent_collision(&core, &core),
        AcceptedParentCollisionDispositionV1::DuplicateCredentialNormalized
    );
    let mut forked = core;
    forked.execution_state_root = [0x99; 32];
    assert_eq!(
        classify_parent_collision(&core, &forked),
        AcceptedParentCollisionDispositionV1::DistinctCoreSafeHalt
    );

    // Signed transaction envelope binds chain_id, genesis_id, launch_manifest_id, profile_id.
    let body = TransactionBodyV1 {
        actor_id: ActorIdV1::from_bytes([0x07; 32]),
        nonce: 0,
        valid_after: 1,
        valid_until: 50,
        expected_state_root: None,
        max_fee: 10_000,
        operations: vec![NetworkEditOpV1::On(Coord::new(5, 6, 7))],
    };
    let pre = SigningPreimageV1 {
        protocol_version: 1,
        chain_id: 1,
        genesis_id,
        launch_manifest_id: [0x22; 32],
        profile_id: [0x44; 32],
        domain_tag: SigningDomainTagV1::ActorTransaction,
        signature_suite: SignatureSuiteIdV1::Ed25519StrictRfc8032V1,
        hash_suite: HashSuiteIdV1::Sha256V1,
        payload_digest: body.tag().unwrap().digest(),
    };
    let sig = sign_preimage_deterministic(&[7u8; 32], &pre).unwrap();
    let signer = pk(7);
    verify_signed_transaction_envelope(1, genesis_id, [0x22; 32], [0x44; 32], &body, &signer, &sig)
        .unwrap();
    // Cross-genesis replay fails closed.
    assert!(
        verify_signed_transaction_envelope(
            1, [0xff; 32], [0x22; 32], [0x44; 32], &body, &signer, &sig
        )
        .is_err()
    );

    verify_protocol_id_graph_acyclic(&[
        ("genesis_id", "profile_scope_digest"),
        ("profile_id", "genesis_id"),
        ("proposal_core_id", "profile_id"),
    ])
    .unwrap();
    assert_eq!(
        verify_protocol_id_graph_acyclic(&[("genesis_id", "genesis_id")]),
        Err(BootstrapError::IdentifierCycleDetected)
    );
}

#[test]
fn language_r1_golden_tags_all_opcodes_and_adversarial_parse_corpus() {
    assert_eq!(
        LANGUAGE_R1_PROGRAM_SCHEMA,
        b"bitblock/language-r1/parsed-program/v1"
    );
    assert_eq!(
        LANGUAGE_R1_PARSE_REPORT_SCHEMA,
        b"bitblock/language-r1/parse-report/v1"
    );
    assert_eq!(SPATIAL_GRAMMAR_VERSION, 1);
    assert_eq!(SPATIAL_FRAME_Y, 6);
    assert_eq!(SPATIAL_FRAME_Z, 3);
    assert_eq!(MAX_SPATIAL_INSTRUCTIONS, 12);
    assert_eq!(chiral_fiducial_offsets().len(), 7);

    // 1. Exact golden parity with tools/language_r1_vectors.py
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
    let raw_src = prog.encode_canonical_source().unwrap();
    assert_eq!(
        ParsedSpatialProgramV1::decode_canonical_source(&raw_src, anchor).unwrap(),
        prog
    );
    assert_eq!(
        prog.source_tag().unwrap().to_string(),
        "#bb.policy.sha256.cx3a3qvyiftv5hunzjklk2re2kyloxczp2ehquqmhvht4uq2zshq"
    );
    let mut world = World::new();
    encode_program_to_world(&mut world, &prog).unwrap();
    assert_eq!(spatial_program_voxels(&prog).unwrap().len(), 45);
    let (parsed, report): (Vec<ParsedSpatialProgramV1>, SpatialParseReportV1) =
        parse_world_programs(&world, &[anchor]).unwrap();
    assert_eq!(parsed[0], prog);
    assert_eq!(
        report.tag().to_string(),
        "#bb.receipt.sha256.twavx3jvzbwqkbdydkpjxdkel5so33eqlbc3s7obuygltbvv6dkq"
    );

    // 2. Round-trip every single opcode (0..=7) through 3D voxel encoding and parsing.
    let all_ops: Vec<SpatialOpcodeV1> = (0u8..8)
        .map(|c| {
            let op = SpatialOpcodeV1::from_code(c).unwrap();
            assert_eq!(
                SpatialOpcodeV1::from_glyph_bitmap(op.glyph_bitmap()),
                Some(op)
            );
            op
        })
        .collect();
    let all_prog = ParsedSpatialProgramV1::new(15, Coord::new(-40, 20, -10), all_ops).unwrap();
    let mut w_all = World::new();
    encode_program_to_world(&mut w_all, &all_prog).unwrap();
    assert_eq!(
        parse_program_at(&w_all, Coord::new(-40, 20, -10)).unwrap(),
        all_prog
    );

    // 3. Adversarial corpus: missing delimiter, spacer collision, unknown glyph, overlapping frames.
    let mut missing_delim = world.clone();
    missing_delim.set_off(Coord::new(anchor.x + 26, anchor.y + 5, anchor.z + 2));
    assert!(matches!(
        parse_program_at(&missing_delim, anchor),
        Err(SpatialParseError::MissingDelimiter(_))
    ));

    let mut spacer_hit = world.clone();
    spacer_hit.set_on(Coord::new(anchor.x + 5, anchor.y + 2, anchor.z + 1));
    assert!(matches!(
        parse_program_at(&spacer_hit, anchor),
        Err(SpatialParseError::DelimiterCollision(_))
    ));

    let mut unknown_glyph = world.clone();
    // Flip bottom-left voxel of slot 0 (PushOne -> invalid 4-voxel L-shape)
    unknown_glyph.set_on(Coord::new(anchor.x + 2, anchor.y + 1, anchor.z + 2));
    assert!(matches!(
        parse_program_at(&unknown_glyph, anchor),
        Err(SpatialParseError::UnknownGlyphBitmap { slot: 0, .. })
    ));

    assert_eq!(
        parse_world_programs(&world, &[anchor, anchor]),
        Err(SpatialParseError::OverlappingProgramFrames)
    );
}
