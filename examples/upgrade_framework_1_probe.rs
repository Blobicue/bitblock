//! Release lifecycle & 12-control compromise benchmark probe for Wave 17C
//! (`UPGRADE-FRAMEWORK-1`, `PS-082`).

use std::time::Instant;

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
use bitblock::resource_1::ResourcePolicyV1;
use bitblock::threat_1::ThreatModelV1;
use bitblock::upgrade_framework_1::{
    CompromiseControlScenarioV1, PreventiveMigrationRequestV1, RESERVED_RECOVERY_ENVELOPE_WIRE_TAG,
    UpgradeDispositionV1, UpgradeFrameworkAuditReceiptV1, UpgradeFrameworkPolicyV1,
    UpgradeFrameworkVerifierV1,
};

fn main() {
    let genesis_id = [0xAA; 32];
    let prev_pol = UpgradeFrameworkPolicyV1::v1_preventive_default(77, genesis_id).unwrap();
    let imm_pol = UpgradeFrameworkPolicyV1::v1_immutable_only(77, genesis_id).unwrap();
    let verifier = UpgradeFrameworkVerifierV1::new(prev_pol).unwrap();
    let p_dig = prev_pol.tag().unwrap().digest();

    // Canonical golden receipts matching tools/upgrade_framework_1_vectors.py
    let admit_rcpt = UpgradeFrameworkAuditReceiptV1 {
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
    let (_, comp_rcpt) = verifier.evaluate_established_authority_compromise(
        CompromiseControlScenarioV1::CollisionOrForgery,
        [0x11; 32],
        [0x22; 32],
        10_000,
        [0x33; 32],
    );
    let term_rcpt = UpgradeFrameworkAuditReceiptV1 {
        disposition: UpgradeDispositionV1::TerminalProtocolIdentityHashCompromise,
        compromise_scenario: CompromiseControlScenarioV1::CollisionOrForgery,
        ..admit_rcpt
    };
    let uninh_rcpt = verifier.evaluate_uninhabited_recovery_claim(
        &[RESERVED_RECOVERY_ENVELOPE_WIRE_TAG],
        [0x11; 32],
        10_000,
        [0x33; 32],
    );

    // Real manifest benchmark
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
    let launch = BoundProfileManifestV1::seal(
        CandidateProfileManifestV1::canonical_launch_v1(
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
        .unwrap(),
    )
    .unwrap();
    let succ = BoundProfileManifestV1::seal(
        CandidateProfileManifestV1::canonical_successor_program_v1(&launch, [0x5E; 32]).unwrap(),
    )
    .unwrap();
    let gov_ok = GovernanceStateRootV1 {
        root_version: 1,
        profile_id: launch.profile_id,
        current_epoch: 30,
        lifecycle: GovernanceLifecycleCodeV1::Normal,
        pause_until_epoch: 0,
        pause_cooldown_until_epoch: 0,
        halted_epoch: 0,
        active_fault_digest: [0u8; 32],
        queued_proposal_count: 1,
        active_param_count: 1,
        governance_trie_root: [0x77; 32],
    };
    let req = PreventiveMigrationRequestV1 {
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

    let mut t_prev = Vec::with_capacity(200);
    let mut t_comp = Vec::with_capacity(200);
    let mut t_hist = Vec::with_capacity(200);
    for _ in 0..200 {
        let a = Instant::now();
        let _ = verifier.evaluate_preventive_migration(&req);
        t_prev.push(a.elapsed().as_secs_f64() * 1e6);

        let b = Instant::now();
        for scen in CompromiseControlScenarioV1::ALL_SIX_ESTABLISHED {
            let _ = verifier.evaluate_established_authority_compromise(
                scen, [0x11; 32], [0x22; 32], 10_000, [0x33; 32],
            );
        }
        t_comp.push(b.elapsed().as_secs_f64() * 1e6);

        let c = Instant::now();
        for h in 9_995..10_005 {
            let v = if h < 10_000 { 1 } else { 2 };
            verifier
                .verify_historical_or_post_activation_version(10_000, h, v, 1, 2)
                .unwrap();
        }
        t_hist.push(c.elapsed().as_secs_f64() * 1e6);
    }
    t_prev.sort_by(f64::total_cmp);
    t_comp.sort_by(f64::total_cmp);
    t_hist.sort_by(f64::total_cmp);

    println!("PREV_POL_TAG={}", prev_pol.tag().unwrap());
    println!("IMM_POL_TAG={}", imm_pol.tag().unwrap());
    println!("ADMIT_TAG={}", admit_rcpt.tag().unwrap());
    println!("COMP_TAG={}", comp_rcpt.tag().unwrap());
    println!("TERM_TAG={}", term_rcpt.tag().unwrap());
    println!("UNINH_TAG={}", uninh_rcpt.tag().unwrap());
    println!("PREV_P50_US={:.2}", t_prev[100]);
    println!("COMP_P50_US={:.2}", t_comp[100]);
    println!("HIST_P50_US={:.2}", t_hist[100]);
}
