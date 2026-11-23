#![cfg(feature = "protocol-research")]
//! Wave 17C (`UPGRADE-FRAMEWORK-1`, `PS-082`) integration tests: preventive in-place migration
//! lifecycle, immutable-version refusal, height-0 genesis predecessor guard, downgrade &
//! historical-block preservation, crash interruption rollback, all 6 Loop 8–10 established
//! authority-compromise controls, compile-time uninhabited recovery envelope, and terminal Row-1
//! `ProtocolIdentityHashCompromiseV1`.

use bitblock::asset_d1::{AssetPolicyDescriptorV1, ValidatorEconPolicyV1};
use bitblock::bounds_1::ProtocolBoundsProfileV1;
use bitblock::commit_state::DomainRegistryV1;
use bitblock::consensus_gov_d1::GovernancePolicyDescriptorV1;
use bitblock::crypto_d1::{CanonicalEd25519PublicKeyV1, CryptoSuiteDescriptorV1};
use bitblock::governance_1::{GovernanceLifecycleCodeV1, GovernanceStateRootV1};
use bitblock::identity_d1::{CredentialRoleV1, ThresholdPolicyV1};
use bitblock::local_bounds::LocalStorageProfile;
use bitblock::profile_bootstrap::ProfileScopeV1;
use bitblock::profile_d1::{BoundProfileManifestV1, CandidateProfileManifestV1};
use bitblock::protocol_id_1::{ParentCollisionRowV1, TwoRowParentCollisionAuditV1};
use bitblock::resource_1::ResourcePolicyV1;
use bitblock::threat_1::ThreatModelV1;
use bitblock::upgrade_framework_1::{
    CompromiseControlScenarioV1, CurrentEstablishedAuthorityCompromiseDispositionV1,
    PreventiveMigrationRequestV1, RESERVED_RECOVERY_ENVELOPE_WIRE_TAG, UpgradeDispositionV1,
    UpgradeFramework1Error, UpgradeFrameworkAuditReceiptV1, UpgradeFrameworkPolicyV1,
    UpgradeFrameworkVerifierV1, decode_reserved_recovery_envelope,
};

fn build_launch_and_successor(
    genesis_id: [u8; 32],
) -> (BoundProfileManifestV1, BoundProfileManifestV1) {
    let mut keys: Vec<_> = [31u8, 32, 33]
        .into_iter()
        .map(|s| CanonicalEd25519PublicKeyV1::from_secret_seed(&[s; 32]).unwrap())
        .collect();
    keys.sort();
    let gov = GovernancePolicyDescriptorV1 {
        schema_version: 1,
        upgrade_quorum_numerator: 2,
        upgrade_quorum_denominator: 3,
        mandatory_exit_timelock_epochs: ValidatorEconPolicyV1::v1_pos_default()
            .unbonding_delay_epochs,
        max_emergency_pause_epochs: 3,
        automatic_safe_halt_on_proven_fault: true,
        emergency_council_pause_only: true,
        forbid_history_rewrite: true,
        emergency_council: ThresholdPolicyV1 {
            role: CredentialRoleV1::EmergencyPauseSigner,
            threshold: 2,
            keys,
        },
    };
    let preds = "asset-active economic-charging stake-consensus validator-economics"
        .split_ascii_whitespace()
        .map(str::to_string)
        .collect();
    let scope =
        ProfileScopeV1::from_wave1_contracts(&DomainRegistryV1::local_v1(), &gov, preds).unwrap();
    let threat = ThreatModelV1::v1_conservative_consumer(&scope).unwrap();
    let bounds =
        ProtocolBoundsProfileV1::v1_consumer_default(&threat, &LocalStorageProfile::DEFAULT)
            .unwrap();
    let resource =
        ResourcePolicyV1::v1_default(&bounds, &AssetPolicyDescriptorV1::v1_dual_domain_default())
            .unwrap();
    let launch_cand = CandidateProfileManifestV1::canonical_launch_v1(
        genesis_id,
        scope.tag().unwrap().digest(),
        DomainRegistryV1::local_v1().tag().digest(),
        CryptoSuiteDescriptorV1::v1_ed25519_sha256()
            .tag()
            .unwrap()
            .digest(),
        bounds.tag().unwrap().digest(),
        resource.tag().unwrap().digest(),
    )
    .unwrap();
    let launch_bound = BoundProfileManifestV1::seal(launch_cand).unwrap();
    let succ_cand =
        CandidateProfileManifestV1::canonical_successor_program_v1(&launch_bound, [0x5E; 32])
            .unwrap();
    let succ_bound = BoundProfileManifestV1::seal(succ_cand).unwrap();
    (launch_bound, succ_bound)
}

fn active_gov_root(
    profile_id: [u8; 32],
    lifecycle: GovernanceLifecycleCodeV1,
) -> GovernanceStateRootV1 {
    GovernanceStateRootV1 {
        root_version: 1,
        profile_id,
        current_epoch: 30,
        lifecycle,
        pause_until_epoch: 0,
        pause_cooldown_until_epoch: 0,
        halted_epoch: if lifecycle == GovernanceLifecycleCodeV1::SafeHalted {
            30
        } else {
            0
        },
        active_fault_digest: [0u8; 32],
        queued_proposal_count: 1,
        active_param_count: 1,
        governance_trie_root: [0x77; 32],
    }
}

#[test]
fn wave17c_upgrade_framework_1_preventive_lifecycle_historical_and_golden_parity() {
    let genesis_id = [0xAA; 32];
    let (launch, succ) = build_launch_and_successor(genesis_id);
    let prev_pol = UpgradeFrameworkPolicyV1::v1_preventive_default(77, genesis_id).unwrap();
    let imm_pol = UpgradeFrameworkPolicyV1::v1_immutable_only(77, genesis_id).unwrap();
    let verifier = UpgradeFrameworkVerifierV1::new(prev_pol).unwrap();
    let gov_ok = active_gov_root(launch.profile_id, GovernanceLifecycleCodeV1::Normal);

    // 1. Height-0 genesis lineage check: Launch OK, Successor (fabricated old) REJECTED!
    let r_gen_ok = verifier.evaluate_genesis_lineage(&launch, 0, [0x01; 32]);
    assert_eq!(
        r_gen_ok.disposition,
        UpgradeDispositionV1::PreventiveMigrationAdmitted
    );
    let r_gen_fab = verifier.evaluate_genesis_lineage(&succ, 0, [0x02; 32]);
    assert_eq!(
        r_gen_fab.disposition,
        UpgradeDispositionV1::RejectedGenesisFabricatedOldProfile
    );

    // 2. Valid preventive migration (old admitted launch -> distinct CE-43 successor) ADMITTED
    let req_ok = PreventiveMigrationRequestV1 {
        old_admitted_profile: Some(&launch),
        target_candidate_profile: &succ,
        old_version: 1,
        target_version: 2,
        activation_height: 10_000,
        gov_root: &gov_ok,
        elapsed_timelock_epochs: 21,
        all_live_bindings_sound: true,
        unique_last_safe_parent: true,
        collision_audit: None,
        interrupted_before_commit: false,
        evidence_digest: [0x33; 32],
    };
    let r_admit = verifier.evaluate_preventive_migration(&req_ok);
    assert_eq!(
        r_admit.disposition,
        UpgradeDispositionV1::PreventiveMigrationAdmitted
    );

    // 3. Interrupted migration rolls back cleanly to old profile
    let mut req_int = req_ok.clone();
    req_int.interrupted_before_commit = true;
    let r_int = verifier.evaluate_preventive_migration(&req_int);
    assert_eq!(
        r_int.disposition,
        UpgradeDispositionV1::RejectedInterruptedRollbackToOldProfile
    );
    assert!(r_int.rollback_clean);

    // 4. Immutable-version policy refuses any in-place version change
    let imm_verifier = UpgradeFrameworkVerifierV1::new(imm_pol).unwrap();
    let r_imm = imm_verifier.evaluate_preventive_migration(&req_ok);
    assert_eq!(
        r_imm.disposition,
        UpgradeDispositionV1::RejectedImmutableVersionRequiresNewGenesis
    );

    // 5. Historical preservation & post-activation downgrade guard
    verifier
        .verify_historical_or_post_activation_version(10_000, 0, 1, 1, 2)
        .unwrap();
    verifier
        .verify_historical_or_post_activation_version(10_000, 9_999, 1, 1, 2)
        .unwrap();
    verifier
        .verify_historical_or_post_activation_version(10_000, 10_000, 2, 1, 2)
        .unwrap();
    assert!(matches!(
        verifier.verify_historical_or_post_activation_version(10_000, 10_000, 1, 1, 2),
        Err(UpgradeFramework1Error::HistoricalOrDowngradeViolation(_))
    ));
    assert!(matches!(
        verifier.verify_historical_or_post_activation_version(10_000, 5_000, 2, 1, 2),
        Err(UpgradeFramework1Error::HistoricalOrDowngradeViolation(_))
    ));

    // 6. Corroborated golden tags matching tools/upgrade_framework_1_vectors.py
    let p_dig = prev_pol.tag().unwrap().digest();
    assert_eq!(
        prev_pol.tag().unwrap().to_string(),
        "#bb.policy.sha256.udwdlgfcdx3lmezbw6ffttvoom2bqjv2tlyxnbekhmtswgeyy74q"
    );
    assert_eq!(
        imm_pol.tag().unwrap().to_string(),
        "#bb.policy.sha256.kqe4wzngoso465trwfsy2brufqygodn7vytur6af27s7rrqeoxdq"
    );
    let golden_admit = UpgradeFrameworkAuditReceiptV1 {
        receipt_version: 1,
        policy_digest: p_dig,
        old_profile_id: [0x11; 32],
        target_profile_id: [0x22; 32],
        activation_height: 10_000,
        old_version: 1,
        target_version: 2,
        disposition: UpgradeDispositionV1::PreventiveMigrationAdmitted,
        compromise_scenario: CompromiseControlScenarioV1::None,
        historical_verified: true,
        rollback_clean: true,
        evidence_digest: [0x33; 32],
    };
    assert_eq!(
        golden_admit.tag().unwrap().to_string(),
        "#bb.receipt.sha256.efusvpoh6mfavk4gq7x3lg3gv2fe6vzrfnjw6xatkfllaehnmpeq"
    );
    let golden_term = UpgradeFrameworkAuditReceiptV1 {
        disposition: UpgradeDispositionV1::TerminalProtocolIdentityHashCompromise,
        compromise_scenario: CompromiseControlScenarioV1::CollisionOrForgery,
        ..golden_admit
    };
    assert_eq!(
        golden_term.tag().unwrap().to_string(),
        "#bb.receipt.sha256.blfxqa7vir3smboq2b2ecuh3i4mas3hvr7qhohrhjauuuabfy6tq"
    );
}

#[test]
fn wave17c_upgrade_framework_1_twelve_control_compromise_and_terminal_matrix() {
    let genesis_id = [0xAA; 32];
    let (launch, succ) = build_launch_and_successor(genesis_id);
    let prev_pol = UpgradeFrameworkPolicyV1::v1_preventive_default(77, genesis_id).unwrap();
    let verifier = UpgradeFrameworkVerifierV1::new(prev_pol).unwrap();
    let gov_ok = active_gov_root(launch.profile_id, GovernanceLifecycleCodeV1::Normal);
    let gov_halt = active_gov_root(launch.profile_id, GovernanceLifecycleCodeV1::SafeHalted);

    let base_req = PreventiveMigrationRequestV1 {
        old_admitted_profile: Some(&launch),
        target_candidate_profile: &succ,
        old_version: 1,
        target_version: 2,
        activation_height: 10_000,
        gov_root: &gov_ok,
        elapsed_timelock_epochs: 21,
        all_live_bindings_sound: true,
        unique_last_safe_parent: true,
        collision_audit: None,
        interrupted_before_commit: false,
        evidence_digest: [0x33; 32],
    };

    // Missing admitted old profile & self-migration rejected
    let mut no_old = base_req.clone();
    no_old.old_admitted_profile = None;
    assert_eq!(
        verifier.evaluate_preventive_migration(&no_old).disposition,
        UpgradeDispositionV1::RejectedMissingAdmittedOldOrSameTarget
    );
    let mut self_mig = base_req.clone();
    self_mig.target_candidate_profile = &launch;
    assert_eq!(
        verifier
            .evaluate_preventive_migration(&self_mig)
            .disposition,
        UpgradeDispositionV1::RejectedMissingAdmittedOldOrSameTarget
    );

    // Version downgrade rejected
    let mut down = base_req.clone();
    down.target_version = 1;
    assert_eq!(
        verifier.evaluate_preventive_migration(&down).disposition,
        UpgradeDispositionV1::RejectedVersionDowngrade
    );

    // Governance Safe-Halt and short timelock rejected
    let mut halted = base_req.clone();
    halted.gov_root = &gov_halt;
    assert_eq!(
        verifier.evaluate_preventive_migration(&halted).disposition,
        UpgradeDispositionV1::RejectedGovernanceHaltedOrTimelockPending
    );
    let mut short_tl = base_req.clone();
    short_tl.elapsed_timelock_epochs = 20;
    assert_eq!(
        verifier
            .evaluate_preventive_migration(&short_tl)
            .disposition,
        UpgradeDispositionV1::RejectedGovernanceHaltedOrTimelockPending
    );

    // Unsound live binding & ambiguous last-safe parent -> SafeHaltAndSeparatelyTrustedNewGenesis
    let mut unsound = base_req.clone();
    unsound.all_live_bindings_sound = false;
    assert_eq!(
        verifier.evaluate_preventive_migration(&unsound).disposition,
        UpgradeDispositionV1::SafeHaltAndSeparatelyTrustedNewGenesis
    );
    let mut ambig = base_req.clone();
    ambig.unique_last_safe_parent = false;
    assert_eq!(
        verifier.evaluate_preventive_migration(&ambig).disposition,
        UpgradeDispositionV1::SafeHaltAndSeparatelyTrustedNewGenesis
    );

    // All 6 Loop 8-10 established compromise controls -> SafeHaltAndSeparatelyTrustedNewGenesisV1
    for scen in CompromiseControlScenarioV1::ALL_SIX_ESTABLISHED {
        let (disp, rcpt) = verifier.evaluate_established_authority_compromise(
            scen, [0x11; 32], [0x22; 32], 10_000, [0x33; 32],
        );
        assert_eq!(
            disp,
            CurrentEstablishedAuthorityCompromiseDispositionV1::SafeHaltAndSeparatelyTrustedNewGenesisV1
        );
        assert_eq!(
            rcpt.disposition,
            UpgradeDispositionV1::SafeHaltAndSeparatelyTrustedNewGenesis
        );
    }

    // Row 1 (ProtocolIdentityHashCompromise) -> strictly TERMINAL; Row 2 -> SafeHalt new genesis
    let row1 = TwoRowParentCollisionAuditV1 {
        schema_version: 1,
        collision_row: ParentCollisionRowV1::ProtocolIdentityHashCompromise,
        claimed_parent_lookup_id: [0x51; 32],
        core_a_digest: [0x61; 32],
        core_b_digest: [0x62; 32],
        safe_halt_required: true,
    };
    let mut r1_req = base_req.clone();
    r1_req.collision_audit = Some(&row1);
    assert_eq!(
        verifier.evaluate_preventive_migration(&r1_req).disposition,
        UpgradeDispositionV1::TerminalProtocolIdentityHashCompromise
    );

    let row2 = TwoRowParentCollisionAuditV1 {
        collision_row: ParentCollisionRowV1::ProfileOwnedAuthorityBindingCompromise,
        ..row1
    };
    let mut r2_req = base_req;
    r2_req.collision_audit = Some(&row2);
    assert_eq!(
        verifier.evaluate_preventive_migration(&r2_req).disposition,
        UpgradeDispositionV1::SafeHaltAndSeparatelyTrustedNewGenesis
    );

    // Uninhabited PrecommittedIndependentRecoveryEnvelopeV1 rejected at decode & verifier
    assert_eq!(
        decode_reserved_recovery_envelope(&[RESERVED_RECOVERY_ENVELOPE_WIRE_TAG, 1, 2, 3]),
        Err(UpgradeFramework1Error::UninhabitedRecoveryEnvelopeRejected)
    );
    let r_uninh = verifier.evaluate_uninhabited_recovery_claim(
        &[RESERVED_RECOVERY_ENVELOPE_WIRE_TAG],
        [0x11; 32],
        10_000,
        [0x33; 32],
    );
    assert_eq!(
        r_uninh.disposition,
        UpgradeDispositionV1::RejectedUninhabitedRecoveryEnvelope
    );
    assert_eq!(
        UpgradeFrameworkAuditReceiptV1::decode_canonical(&r_uninh.encode_canonical().unwrap())
            .unwrap(),
        r_uninh
    );
}
