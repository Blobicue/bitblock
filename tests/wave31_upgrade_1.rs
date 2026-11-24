#![cfg(feature = "protocol-research")]
//! Wave 31 (`UPGRADE-1@base`, `PS-083`) end-to-end integration tests:
//! 1. Deterministic preventive `v1 -> v2` migration, 10-domain logical state conservation, and
//!    live `SuccessorCandidate` admission through frozen `PROFILE-1` (`profile_1.rs`);
//! 2. 10 fail-closed adversarial controls spanning `CE-30`/`CE-31` polarity reversal, supply/bedrock
//!    drift, unauthorized/unelapsed timelock, mid-migration atomic rollback & restart parity,
//!    post-activation downgrade refusal, archival historical verification, established compromise
//!    refusal, occurrence handoff mismatch, and un-admitted `program-active` successor rejection.

use bitblock::profile_1::{
    MigrationEvidenceBindingV1, PredicateEnvironmentV1, ProfileAdmissionDispositionV1,
    ProfileAdmissionRequestV1, admit_candidate_profile, canonical_admission_policy_id,
    canonical_devnet_launch_evidence_entries, canonical_launch_role_graph_edges,
};
use bitblock::profile_d1::{BoundProfileManifestV1, ProfileLineageKindV1};
use bitblock::transition_spec_1::ResolvedProfileTransitionSpecV1;
use bitblock::upgrade_1::{
    MigrationFaultHookV1, Upgrade1Error, admit_canonical_devnet_successor_profile,
    build_canonical_devnet_upgrade_1, enforce_post_activation_non_downgrade,
    execute_bounded_profile_migration, verify_archival_historical_block,
    verify_upgrade_1_evidence_manifest,
};

#[test]
fn wave31_deterministic_preventive_migration_and_successor_profile_admission() {
    let out = build_canonical_devnet_upgrade_1().expect("canonical UPGRADE-1 outcome");
    assert_ne!(
        out.migration_spec.predecessor_admitted_profile_id,
        out.migration_spec.successor_candidate_profile_id
    );
    assert_eq!(out.old_snapshot.world_root, out.new_snapshot.world_root);
    assert_eq!(
        out.old_snapshot.native_balance_root,
        out.new_snapshot.native_balance_root
    );
    assert_eq!(
        out.old_snapshot.economic_stake_root,
        out.new_snapshot.economic_stake_root
    );
    assert_ne!(
        out.old_snapshot.governance_root, out.new_snapshot.governance_root,
        "governance parameter upgrade must advance governance_root"
    );
    assert!(!out.new_snapshot.program_state_active);
    assert_eq!(
        out.replay_receipt.conserved_total_native_supply,
        200_000_000_000
    );
    assert_eq!(out.replay_receipt.conserved_bonded_stake, 400_000_000_000);

    // Feed real UPGRADE-1 evidence into frozen PROFILE-1 (`admit_candidate_profile`)!
    let (closure, adm_manifest) = admit_canonical_devnet_successor_profile(&out, [0x31u8; 20])
        .expect("frozen PROFILE-1 admits preventive v1->v2 successor");
    assert_eq!(
        closure.lineage_kind,
        ProfileLineageKindV1::SuccessorCandidate
    );
    assert!(!closure.initial_launch_upgrade_framework_scoped);
    assert_eq!(
        adm_manifest.admitted_profile_id,
        out.successor_candidate.successor_bound_manifest.profile_id
    );
    assert_eq!(
        adm_manifest.admission_disposition,
        ProfileAdmissionDispositionV1::AdmittedBoundedV1
    );
}

#[test]
fn wave31_ten_adversarial_migration_rollback_downgrade_and_compromise_controls() {
    let out = build_canonical_devnet_upgrade_1().expect("base UPGRADE-1");
    let fx = &out.launch_fixture;
    let succ = &out.successor_candidate;
    let total_supply = out.replay_receipt.conserved_total_native_supply;
    let total_stake = out.replay_receipt.conserved_bonded_stake;

    // 1. CE-30/CE-31 polarity reversal / self-reference (old == new or swapped).
    {
        let mut bad_spec = out.migration_spec;
        bad_spec.successor_candidate_profile_id = bad_spec.predecessor_admitted_profile_id;
        assert!(matches!(
            bad_spec.validate(),
            Err(Upgrade1Error::PolarityOrLineageViolation(_))
        ));
        let mut swapped = out.migration_spec;
        swapped.predecessor_admitted_profile_id = out.migration_spec.successor_candidate_profile_id;
        swapped.successor_candidate_profile_id = out.migration_spec.predecessor_admitted_profile_id;
        let mut snap = out.old_snapshot;
        assert!(
            execute_bounded_profile_migration(
                &swapped,
                &out.occurrence_handoff,
                &fx.bound_manifest,
                &succ.successor_bound_manifest,
                &fx.exec_registry,
                &fx.bundle.initial_gov_params,
                out.launch_parity_record.initial_execution_root_ref,
                &mut snap,
                total_supply,
                total_stake,
                21,
                true,
                MigrationFaultHookV1::None,
            )
            .is_err()
        );
    }

    // 2. Governance root drift / tampered pre-migration logical state rejected before commit.
    {
        let mut bad_snap = out.old_snapshot;
        bad_snap.governance_root[0] ^= 0xFF;
        let snapshot_before = bad_snap;
        assert!(matches!(
            execute_bounded_profile_migration(
                &out.migration_spec,
                &out.occurrence_handoff,
                &fx.bound_manifest,
                &succ.successor_bound_manifest,
                &fx.exec_registry,
                &fx.bundle.initial_gov_params,
                out.launch_parity_record.initial_execution_root_ref,
                &mut bad_snap,
                total_supply,
                total_stake,
                21,
                true,
                MigrationFaultHookV1::None,
            ),
            Err(Upgrade1Error::ConservationViolation(_))
        ));
        assert_eq!(bad_snap, snapshot_before);
    }

    // 3. Silent ProgramState activation attempt rejected fail-closed.
    {
        let mut bad_snap = out.old_snapshot;
        bad_snap.program_state_active = true;
        assert!(matches!(
            execute_bounded_profile_migration(
                &out.migration_spec,
                &out.occurrence_handoff,
                &fx.bound_manifest,
                &succ.successor_bound_manifest,
                &fx.exec_registry,
                &fx.bundle.initial_gov_params,
                out.launch_parity_record.initial_execution_root_ref,
                &mut bad_snap,
                total_supply,
                total_stake,
                21,
                true,
                MigrationFaultHookV1::None,
            ),
            Err(Upgrade1Error::ConservationViolation(_))
        ));
    }

    // 4. Unauthorized activation / unelapsed governance exit-timelock (< 21 epochs).
    {
        let mut snap = out.old_snapshot;
        assert!(matches!(
            execute_bounded_profile_migration(
                &out.migration_spec,
                &out.occurrence_handoff,
                &fx.bound_manifest,
                &succ.successor_bound_manifest,
                &fx.exec_registry,
                &fx.bundle.initial_gov_params,
                out.launch_parity_record.initial_execution_root_ref,
                &mut snap,
                total_supply,
                total_stake,
                5, // < 21 epochs
                true,
                MigrationFaultHookV1::None,
            ),
            Err(Upgrade1Error::UnauthorizedOrCompromised(_))
        ));
        assert_eq!(snap, out.old_snapshot);
    }

    // 5. Mid-migration interruption atomic rollback + deterministic restart bit-exact parity.
    {
        let mut snap = out.old_snapshot;
        assert_eq!(
            execute_bounded_profile_migration(
                &out.migration_spec,
                &out.occurrence_handoff,
                &fx.bound_manifest,
                &succ.successor_bound_manifest,
                &fx.exec_registry,
                &fx.bundle.initial_gov_params,
                out.launch_parity_record.initial_execution_root_ref,
                &mut snap,
                total_supply,
                total_stake,
                21,
                true,
                MigrationFaultHookV1::InterruptMidApply,
            ),
            Err(Upgrade1Error::InterruptedAtomicRollback)
        );
        assert_eq!(
            snap, out.old_snapshot,
            "atomic rollback leaves zero residue"
        );
        let (comp2, rcpt2) = execute_bounded_profile_migration(
            &out.migration_spec,
            &out.occurrence_handoff,
            &fx.bound_manifest,
            &succ.successor_bound_manifest,
            &fx.exec_registry,
            &fx.bundle.initial_gov_params,
            out.launch_parity_record.initial_execution_root_ref,
            &mut snap,
            total_supply,
            total_stake,
            21,
            true,
            MigrationFaultHookV1::None,
        )
        .expect("clean restart after rollback succeeds");
        assert_eq!(comp2, out.new_composition);
        assert_eq!(rcpt2, out.replay_receipt);
    }

    // 6. Post-activation old-profile downgrade rejected fail-closed.
    assert!(matches!(
        enforce_post_activation_non_downgrade(
            &out.migration_spec,
            out.migration_spec.activation_height,
            out.migration_spec.predecessor_admitted_profile_id,
        ),
        Err(Upgrade1Error::DowngradeOrHistoricalRejected(_))
    ));
    assert!(
        enforce_post_activation_non_downgrade(
            &out.migration_spec,
            out.migration_spec.activation_height,
            out.migration_spec.successor_candidate_profile_id,
        )
        .is_ok()
    );

    // 7. Archival pre-activation historical verification passes under old profile, rejects new/post.
    assert!(
        verify_archival_historical_block(
            &out.migration_spec,
            &out.occurrence_handoff,
            out.migration_spec.activation_height - 1,
            out.migration_spec.predecessor_admitted_profile_id,
        )
        .is_ok()
    );
    assert!(
        verify_archival_historical_block(
            &out.migration_spec,
            &out.occurrence_handoff,
            out.migration_spec.activation_height - 1,
            out.migration_spec.successor_candidate_profile_id,
        )
        .is_err()
    );

    // 8. Established authority-binding compromise (`all_live_bindings_sound = false`) refused.
    {
        let mut snap = out.old_snapshot;
        assert!(matches!(
            execute_bounded_profile_migration(
                &out.migration_spec,
                &out.occurrence_handoff,
                &fx.bound_manifest,
                &succ.successor_bound_manifest,
                &fx.exec_registry,
                &fx.bundle.initial_gov_params,
                out.launch_parity_record.initial_execution_root_ref,
                &mut snap,
                total_supply,
                total_stake,
                21,
                false,
                MigrationFaultHookV1::None,
            ),
            Err(Upgrade1Error::UnauthorizedOrCompromised(_))
        ));
    }

    // 9. Tampered OccurrenceUniverseHandoffV1 receipt bridge or history-start mismatch rejected.
    {
        let mut bad_h = out.occurrence_handoff;
        bad_h.receipt_bridge_equivalence_digest[0] ^= 0x01;
        assert!(
            verify_upgrade_1_evidence_manifest(
                &out.migration_spec,
                &bad_h,
                &out.replay_receipt,
                &out.evidence_manifest,
            )
            .is_err()
        );
    }

    // 10. SuccessorCandidate activating `program-active` without `PROGRAM-CONSENSUS` rejected by frozen PROFILE-1.
    {
        let prog_bound = BoundProfileManifestV1::seal(
            bitblock::profile_d1::CandidateProfileManifestV1::canonical_successor_program_v1(
                &fx.bound_manifest,
                out.migration_spec.successor_scope_digest,
            )
            .expect("program successor cand"),
        )
        .expect("bind prog successor");
        let prog_spec = ResolvedProfileTransitionSpecV1::resolve_from_profile(
            &prog_bound,
            &fx.bundle.bounds_profile,
            &fx.bundle.resource_policy,
        )
        .expect("prog spec");
        let bad_req = ProfileAdmissionRequestV1 {
            genesis_package: &fx.package,
            bound_manifest: &prog_bound,
            transition_spec: &prog_spec,
            parity_record: &out.launch_parity_record,
            pred_env: PredicateEnvironmentV1 {
                launch_predicates: fx.package.state.launch_predicates.clone(),
                target_predicates: prog_bound.candidate.activated_predicates.clone(),
            },
            evidence_entries: canonical_devnet_launch_evidence_entries(),
            role_graph_edges: canonical_launch_role_graph_edges(),
            migration_binding: Some(MigrationEvidenceBindingV1 {
                old_admitted_profile_id: fx.bound_manifest.profile_id,
                new_candidate_profile_id: prog_bound.profile_id,
                upgrade_1_artifact_digest: out.evidence_manifest.tag().unwrap().digest(),
            }),
            verkle_binding: None,
            admission_policy_id: canonical_admission_policy_id(
                "bitblock-profile-1-launch-policy-v1",
            ),
        };
        assert!(admit_candidate_profile(&bad_req).is_err());
    }
}
