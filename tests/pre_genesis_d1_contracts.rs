#![cfg(feature = "protocol-research")]
//! End-to-end integration and golden-tag parity suite for the Wave 1 pre-genesis design contracts:
//! `CRYPTO-D1`, `IDENTITY-D1`, `ASSET-D1`, `VALIDATOR-ECON-D1`, `CONSENSUS-D1`, and `GOVERNANCE-D1`.
//!
//! Asserts literal canonical `ContentTag` parity with the independent Python vector suites
//! (`tools/crypto_d1_vectors.py` and `tools/pre_genesis_d1_vectors.py`), exercises every gate
//! obligation and adversarial negative control, and pins every canonical big-endian encoder site.

use bitblock::asset_d1::{
    ASSET_D1_POLICY_SCHEMA, AssetConservationLedgerV1, AssetEconError, AssetPolicyDescriptorV1,
    BPS_DENOMINATOR, OccupancyTokenRecordV1, VALIDATOR_ECON_D1_POLICY_SCHEMA,
    ValidatorEconPolicyV1, ValidatorStakeBookV1,
};
use bitblock::consensus_gov_d1::{
    CONSENSUS_D1_POLICY_SCHEMA, ConsensusGovError, ConsensusPolicyDescriptorV1,
    GOVERNANCE_D1_POLICY_SCHEMA, GovernanceActionV1, GovernancePolicyDescriptorV1,
    InitialValidatorDescriptorV1, InitialValidatorSetV1, MAX_ACTIVE_VALIDATORS_V1, VoteSignDocV1,
    evaluate_governance_action, verify_quorum_certificate,
};
use bitblock::crypto_d1::{
    CRYPTO_D1_SIGNING_PREIMAGE_SCHEMA, CRYPTO_D1_SUITE_DESCRIPTOR_SCHEMA,
    CanonicalEd25519PublicKeyV1, CanonicalEd25519SignatureV1, CryptoD1Error,
    CryptoSuiteDescriptorV1, ED25519_PUBLIC_KEY_LEN, ED25519_SIGNATURE_LEN, HashSuiteIdV1,
    SignatureSuiteIdV1, SigningDomainTagV1, SigningPreimageV1, sign_preimage_deterministic,
    verify_batch_with_strict_fallback, verify_preimage_strict, verify_raw_message_strict,
};
use bitblock::identity_d1::{
    ActorIdV1, ActorNonceV1, AuthorizationDescriptorV1, CredentialRoleV1, DelegationGrantV1,
    IDENTITY_D1_AUTH_DESCRIPTOR_SCHEMA, IDENTITY_D1_TX_BODY_SCHEMA, IdentityD1Error,
    MAX_DELEGATIONS_PER_ACTOR, MAX_THRESHOLD_KEYS, MAX_TX_BATCH_OPS, NetworkEditOpV1,
    ThresholdPolicyV1, TransactionBodyV1, validate_actor_descriptor_set,
};
use bitblock::world::{BEDROCK_COORD, Coord};

fn pk(seed: u8) -> CanonicalEd25519PublicKeyV1 {
    CanonicalEd25519PublicKeyV1::from_secret_seed(&[seed; 32]).unwrap()
}

fn hex_encode(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2);
    for &b in bytes {
        use std::fmt::Write as _;
        let _ = write!(&mut s, "{b:02x}");
    }
    s
}

#[test]
fn golden_canonical_tags_match_independent_python_vectors() {
    assert_eq!(
        CRYPTO_D1_SUITE_DESCRIPTOR_SCHEMA,
        b"bitblock/crypto-d1/suite-descriptor/v1"
    );
    assert_eq!(
        CRYPTO_D1_SIGNING_PREIMAGE_SCHEMA,
        b"bitblock/crypto-d1/signing-preimage/v1"
    );
    assert_eq!(
        IDENTITY_D1_AUTH_DESCRIPTOR_SCHEMA,
        b"bitblock/identity-d1/authorization-descriptor/v1"
    );
    assert_eq!(
        IDENTITY_D1_TX_BODY_SCHEMA,
        b"bitblock/identity-d1/transaction-body/v1"
    );
    assert_eq!(ASSET_D1_POLICY_SCHEMA, b"bitblock/asset-d1/policy/v1");
    assert_eq!(
        VALIDATOR_ECON_D1_POLICY_SCHEMA,
        b"bitblock/validator-econ-d1/policy/v1"
    );
    assert_eq!(
        CONSENSUS_D1_POLICY_SCHEMA,
        b"bitblock/consensus-d1/policy/v1"
    );
    assert_eq!(
        GOVERNANCE_D1_POLICY_SCHEMA,
        b"bitblock/governance-d1/policy/v1"
    );
    assert_eq!(ED25519_PUBLIC_KEY_LEN, 32);
    assert_eq!(ED25519_SIGNATURE_LEN, 64);
    assert_eq!(MAX_THRESHOLD_KEYS, 32);
    assert_eq!(MAX_DELEGATIONS_PER_ACTOR, 16);
    assert_eq!(MAX_TX_BATCH_OPS, 256);
    assert_eq!(BPS_DENOMINATOR, 10_000);
    assert_eq!(MAX_ACTIVE_VALIDATORS_V1, 128);

    // 1. CRYPTO-D1 suite descriptor & signing preimage
    let suite = CryptoSuiteDescriptorV1::v1_ed25519_sha256();
    assert_eq!(
        suite.tag().unwrap().to_string(),
        "#bb.policy.sha256.jczjgonfconrys2yy7cgehmsk6mh6757m4r4piyg5ynglq2uanlq"
    );
    let pre = SigningPreimageV1 {
        protocol_version: 1,
        chain_id: 77,
        genesis_id: [0x11; 32],
        launch_manifest_id: [0x22; 32],
        profile_id: [0x33; 32],
        domain_tag: SigningDomainTagV1::ActorTransaction,
        signature_suite: SignatureSuiteIdV1::Ed25519StrictRfc8032V1,
        hash_suite: HashSuiteIdV1::Sha256V1,
        payload_digest: [0x44; 32],
    };
    assert_eq!(
        pre.tag().to_string(),
        "#bb.receipt.sha256.blgh6lg27ohjxiv2royj2wbkii7zny2innes6coaeqpy2ckcwoeq"
    );
    let k = pk(1);
    assert_eq!(
        CanonicalEd25519PublicKeyV1::from_slice(k.as_bytes()).unwrap(),
        k
    );
    assert_eq!(
        hex_encode(&k.key_id()),
        "2c9bab9f88a372cb0f49d5fc2b9c7acf52376ec95e0d5cbd7bdfa865ff39bb95"
    );
    let sig = sign_preimage_deterministic(&[1u8; 32], &pre).unwrap();
    assert_eq!(
        CanonicalEd25519SignatureV1::from_slice(sig.as_bytes()).unwrap(),
        sig
    );
    verify_preimage_strict(&k, &pre, &sig).unwrap();
    verify_raw_message_strict(&k, &pre.signing_digest(), &sig).unwrap();
    verify_batch_with_strict_fallback(&[(k, pre, sig)]).unwrap();

    // 2. IDENTITY-D1 authorization descriptor, pre-genesis ActorIdV1, and TransactionBodyV1
    let mut action_keys = vec![pk(1), pk(2)];
    action_keys.sort();
    let desc = AuthorizationDescriptorV1 {
        descriptor_version: 0,
        action_policy: ThresholdPolicyV1 {
            role: CredentialRoleV1::ActorAction,
            threshold: 2,
            keys: action_keys,
        },
        admin_policy: ThresholdPolicyV1 {
            role: CredentialRoleV1::ActorAdmin,
            threshold: 1,
            keys: vec![pk(3)],
        },
        delegations: vec![],
        revoked: false,
    };
    assert_eq!(
        desc.tag().unwrap().to_string(),
        "#bb.policy.sha256.ewnhqdwxbhpuvwzeghndqdsssfsaz7u26xgwbxnporebo5poqd3q"
    );
    let actor_id = ActorIdV1::derive(1, "bitblock-mainnet-v1", &desc).unwrap();
    assert_eq!(
        hex_encode(actor_id.as_bytes()),
        "c830de027d46f85cbf460a219bd3098e2452476e30b2e598a25743c9a94b30de"
    );

    let tx = TransactionBodyV1 {
        actor_id: ActorIdV1::from_bytes([0x07; 32]),
        nonce: 0,
        valid_after: 10,
        valid_until: 20,
        expected_state_root: Some([0x55; 32]),
        max_fee: 1_000,
        operations: vec![
            NetworkEditOpV1::On(Coord::new(1, 2, 3)),
            NetworkEditOpV1::Off(Coord::new(-4, 5, -6)),
        ],
    };
    assert_eq!(
        tx.tag().unwrap().to_string(),
        "#bb.intent.sha256.foomjcxk5bnt4xzqhmt4zhwxa7hkdq37ykbkijliwasy4gwymela"
    );

    // 3. ASSET-D1 & VALIDATOR-ECON-D1
    let asset_pol = AssetPolicyDescriptorV1::v1_dual_domain_default();
    assert_eq!(
        asset_pol.tag().unwrap().to_string(),
        "#bb.policy.sha256.nxy3syo3wcq5ch7ayryyut25sbyustu4snj5vphox52fsnnlo4ca"
    );
    let econ_pol = ValidatorEconPolicyV1::v1_pos_default();
    assert_eq!(
        econ_pol.tag().unwrap().to_string(),
        "#bb.policy.sha256.uzhiakc4bqij66hyg6h75kxsrlxqfaunxd7yzx36kdlxvii2gpaa"
    );

    // 4. CONSENSUS-D1 & GOVERNANCE-D1
    let cons_pol = ConsensusPolicyDescriptorV1::v1_bft_pos_default();
    assert_eq!(
        cons_pol.tag().unwrap().to_string(),
        "#bb.policy.sha256.7g333rrr5tpxptksywvawnlgbqvyppsfpxkefmtzanv6rfsz5tla"
    );
    let mut council_keys = vec![pk(31), pk(32), pk(33)];
    council_keys.sort();
    let gov_pol = GovernancePolicyDescriptorV1 {
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
            keys: council_keys,
        },
    };
    assert_eq!(
        gov_pol.tag().unwrap().to_string(),
        "#bb.policy.sha256.advjjkb4urtxkycounxi54i7x4x7uvefeqcatwnrzatpys7n2lwa"
    );
}

#[test]
fn adversarial_negative_controls_across_all_wave1_contracts() {
    // CRYPTO-D1 error surfaces & suite rejections
    assert_eq!(
        SignatureSuiteIdV1::from_code(99),
        Err(CryptoD1Error::UnsupportedSignatureSuite(99))
    );
    assert_eq!(
        HashSuiteIdV1::from_code(99),
        Err(CryptoD1Error::UnsupportedHashSuite(99))
    );
    assert_eq!(
        SigningDomainTagV1::from_code(99),
        Err(CryptoD1Error::UnknownSigningDomain(99))
    );
    let mut bad_suite = CryptoSuiteDescriptorV1::v1_ed25519_sha256();
    bad_suite.reject_small_order_points = false;
    assert!(bad_suite.encode_canonical().is_err());

    // IDENTITY-D1: role separation, self-delegation, revocation, and nonce monotonicity
    assert_eq!(
        CredentialRoleV1::from_code(99),
        Err(IdentityD1Error::UnknownCredentialRole(99))
    );
    let mut wrong_role = ThresholdPolicyV1 {
        role: CredentialRoleV1::ValidatorVote,
        threshold: 1,
        keys: vec![pk(1)],
    };
    assert!(wrong_role.validate(CredentialRoleV1::ActorAction).is_err());
    wrong_role.role = CredentialRoleV1::ActorAction;
    let desc = AuthorizationDescriptorV1 {
        descriptor_version: 0,
        action_policy: wrong_role,
        admin_policy: ThresholdPolicyV1 {
            role: CredentialRoleV1::ActorAdmin,
            threshold: 1,
            keys: vec![pk(2)],
        },
        delegations: vec![],
        revoked: false,
    };
    let aid = ActorIdV1::derive(1, "ns", &desc).unwrap();
    let mut self_del = desc.clone();
    self_del.delegations.push(DelegationGrantV1 {
        delegate_actor_id: aid,
        allow_on: true,
        allow_off: false,
        expires_at_height: 50,
    });
    assert_eq!(
        validate_actor_descriptor_set(&[(aid, self_del)]),
        Err(IdentityD1Error::DelegationCycleDetected)
    );
    let mut nonce = ActorNonceV1::default();
    nonce.consume(0).unwrap();
    assert!(nonce.consume(2).is_err());

    // ASSET-D1 & VALIDATOR-ECON-D1 conservation & concentration bounds
    let asset_pol = AssetPolicyDescriptorV1::v1_dual_domain_default();
    let mut ledger =
        AssetConservationLedgerV1::from_genesis(asset_pol, &[(aid, 5_000_000)]).unwrap();
    assert_eq!(
        ledger.apply_mint(aid, BEDROCK_COORD, [1; 32]),
        Err(AssetEconError::BedrockHasNoEconomicToken)
    );
    let coord = Coord::new(10, -2, 7);
    ledger.apply_mint(aid, coord, [0x5a; 32]).unwrap();
    let rec: OccupancyTokenRecordV1 = ledger.episode_at(coord).unwrap();
    assert_eq!(rec.locked_deposit, asset_pol.deposit_per_occupied_block);
    assert_eq!(ledger.total_burned_fees(), asset_pol.base_fee_burn);
    ledger.apply_burn(aid, coord).unwrap();
    assert_eq!(ledger.active_episode_count(), 0);
    assert_eq!(
        ledger.balance_of(&aid),
        5_000_000 - 2 * asset_pol.base_fee_burn
    );

    let econ_pol = ValidatorEconPolicyV1::v1_pos_default();
    // 50% stake concentration in one genesis validator must fail (> 25% max_single_validator_bps).
    let concentrated = vec![
        (ActorIdV1::from_bytes([1; 32]), 500_000_000_000),
        (ActorIdV1::from_bytes([2; 32]), 100_000_000_000),
        (ActorIdV1::from_bytes([3; 32]), 100_000_000_000),
        (ActorIdV1::from_bytes([4; 32]), 100_000_000_000),
    ];
    assert!(ValidatorStakeBookV1::from_genesis(econ_pol, &concentrated).is_err());

    // CONSENSUS-D1 & GOVERNANCE-D1: pause-only council verification, role checks, & root pinning
    let vals: Vec<InitialValidatorDescriptorV1> = (1u8..=4)
        .map(|i| InitialValidatorDescriptorV1 {
            validator_id: ActorIdV1::from_bytes([i; 32]),
            vote_key: pk(40 + i),
            proposal_key: pk(50 + i),
            weight: 100,
        })
        .collect();
    let vset = InitialValidatorSetV1::new(vals.clone()).unwrap();
    assert_eq!(vset.total_weight(), 400);
    assert_eq!(
        hex_encode(&vset.active_validator_root()),
        "aea25c84571e7148551fe19253af28955045e03ed99352edb2b5ff3ed74a31f9"
    );
    let canonical_vote = VoteSignDocV1 {
        epoch: 0,
        active_validator_root: vset.active_validator_root(),
        height: 1,
        round: 0,
        vote_phase: 1,
        block_id: [0x77; 32],
    };
    assert_eq!(
        hex_encode(&canonical_vote.payload_digest()),
        "1c03d0a247b7dcaca4752f85fa31aaa20bdb520951fe8e3fb62b5049cc5ce48d"
    );
    // Reusing vote_key as proposal_key must reject.
    let mut dup_role_vals = vals.clone();
    dup_role_vals[0].proposal_key = dup_role_vals[0].vote_key;
    assert!(matches!(
        InitialValidatorSetV1::new(dup_role_vals),
        Err(ConsensusGovError::InvalidValidatorSet(_))
    ));

    let vote = VoteSignDocV1 {
        epoch: 1,
        active_validator_root: [0xff; 32], // wrong validator root
        height: 5,
        round: 0,
        vote_phase: 1,
        block_id: [0x22; 32],
    };
    assert!(matches!(
        verify_quorum_certificate(&vset, 1, [1; 32], [2; 32], [3; 32], &vote, &[]),
        Err(ConsensusGovError::InvalidQuorumCertificate(_))
    ));

    // Valid 2-of-3 emergency pause signed by EmergencyPauseSigner council passes; 4-epoch pause fails.
    let mut council_keys = vec![pk(31), pk(32), pk(33)];
    council_keys.sort();
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
            keys: council_keys,
        },
    };
    let ev_digest = [0xee; 32];
    let pause_pre = SigningPreimageV1 {
        protocol_version: 1,
        chain_id: 1,
        genesis_id: [1; 32],
        launch_manifest_id: [2; 32],
        profile_id: [3; 32],
        domain_tag: SigningDomainTagV1::EmergencyPause,
        signature_suite: SignatureSuiteIdV1::Ed25519StrictRfc8032V1,
        hash_suite: HashSuiteIdV1::Sha256V1,
        payload_digest: ev_digest,
    };
    let cs1 = sign_preimage_deterministic(&[31u8; 32], &pause_pre).unwrap();
    let cs2 = sign_preimage_deterministic(&[32u8; 32], &pause_pre).unwrap();
    evaluate_governance_action(
        &gov,
        &vset,
        &GovernanceActionV1::EmergencyPause {
            duration_epochs: 2,
            evidence_digest: ev_digest,
        },
        Some(pause_pre),
        &[(pk(31), cs1), (pk(32), cs2)],
    )
    .unwrap();
    assert!(
        evaluate_governance_action(
            &gov,
            &vset,
            &GovernanceActionV1::EmergencyPause {
                duration_epochs: 4,
                evidence_digest: ev_digest,
            },
            Some(pause_pre),
            &[(pk(31), cs1), (pk(32), cs2)],
        )
        .is_err()
    );
}
