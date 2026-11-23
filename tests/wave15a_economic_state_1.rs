//! Wave 15A integration suite for `ECONOMIC-STATE-1` (`PS-040`):
//! verifies join/exit, 21-epoch unbonding timelock, 100% slash-to-burn across BOTH active and
//! unbonding stake (defeating slash-and-flee), permanent equivocation jailing, integer-floor
//! pro-rata rewards with zero-leak unissued dust, duplicate/malformed evidence refusal,
//! cross-domain proof isolation, and full-rebuild parity.

#![cfg(feature = "protocol-research")]

use bitblock::asset_d1::ValidatorEconPolicyV1;
use bitblock::auth_commit_1::{AuthCommit1Error, DomainIdV1};
use bitblock::economic_state_1::{EconomicDispositionV1, EconomicStateV1, ValidatorStakeRecordV1};
use bitblock::identity_d1::ActorIdV1;

fn genesis_state() -> (EconomicStateV1, [ActorIdV1; 4]) {
    let policy = ValidatorEconPolicyV1::v1_pos_default();
    let vals = [
        ActorIdV1::from_bytes([1; 32]),
        ActorIdV1::from_bytes([2; 32]),
        ActorIdV1::from_bytes([3; 32]),
        ActorIdV1::from_bytes([4; 32]),
    ];
    let s = 100_000_000_000u128;
    let st = EconomicStateV1::from_genesis(
        policy,
        [0x66; 32],
        &[(vals[0], s), (vals[1], s), (vals[2], s), (vals[3], s)],
    )
    .expect("genesis");
    (st, vals)
}

#[test]
fn economic_state_1_slash_and_flee_defeated_jailing_and_floor_rewards() {
    let (mut st, v) = genesis_state();

    // 1. Reward over cap (> 1B) MUST reject fail-closed
    let pre = st.root_commitment().unwrap();
    assert_eq!(
        st.distribute_epoch_rewards(1_000_000_001, 1)
            .unwrap()
            .disposition,
        EconomicDispositionV1::RejectedRewardOverCap
    );
    assert_eq!(st.root_commitment().unwrap(), pre);

    // 2. Integer-floor rewards at epoch 1 (999_999_999 across 4 equal -> 3 units unissued dust)
    let r_rew = st.distribute_epoch_rewards(999_999_999, 1).unwrap();
    assert_eq!(r_rew.disposition, EconomicDispositionV1::Admitted);
    assert_eq!(r_rew.amount_delta, 999_999_996);
    assert_eq!(r_rew.dust_unissued, 3);
    // Duplicate reward distribution for same epoch MUST fail closed
    assert_eq!(
        st.distribute_epoch_rewards(100_000, 1).unwrap().disposition,
        EconomicDispositionV1::RejectedDuplicateEpochOrEvidence
    );

    // 3. V3 starts unbonding 20B at epoch 2 (unlock_epoch = 23) then equivocates at epoch 5!
    assert_eq!(
        st.request_unbond(v[2], 20_000_000_000, 2)
            .unwrap()
            .disposition,
        EconomicDispositionV1::Admitted
    );
    // Early withdrawal at epoch 22 MUST fail closed!
    assert_eq!(
        st.withdraw_matured_unbond(v[2], 22).unwrap().disposition,
        EconomicDispositionV1::RejectedUnbondingNotMatured
    );
    // Reset to epoch 5 on a fresh clone for slash-and-flee check (epoch monotonic!)
    let (mut st2, v2) = genesis_state();
    assert_eq!(
        st2.request_unbond(v2[2], 20_000_000_000, 2)
            .unwrap()
            .disposition,
        EconomicDispositionV1::Admitted
    );
    let r_sl = st2.apply_equivocation_slash(v2[2], [0xA5; 32], 5).unwrap();
    assert_eq!(r_sl.disposition, EconomicDispositionV1::Admitted);
    assert_eq!(r_sl.amount_delta, 50_000_000_000);

    // Permanently jailed V3 CANNOT bond back in and is excluded from eligible_validators!
    assert_eq!(
        st2.bond_stake(v2[2], 100_000_000_000, 6)
            .unwrap()
            .disposition,
        EconomicDispositionV1::RejectedValidatorJailed
    );
    assert!(!st2.eligible_validators().iter().any(|(id, _)| *id == v2[2]));

    // Early withdraw at 22 rejected; matured withdraw at 23 releases ONLY the surviving 10B!
    assert_eq!(
        st2.withdraw_matured_unbond(v2[2], 22).unwrap().disposition,
        EconomicDispositionV1::RejectedUnbondingNotMatured
    );
    let r_wd = st2.withdraw_matured_unbond(v2[2], 23).unwrap();
    assert_eq!(r_wd.disposition, EconomicDispositionV1::Admitted);
    assert_eq!(r_wd.amount_delta, 10_000_000_000);
    st2.verify_rebuild_parity().unwrap();
    st2.verify_conservation().unwrap();
}

#[test]
fn economic_state_1_inactivity_slash_join_evidence_guards_and_domain_isolation() {
    let (mut st, v) = genesis_state();

    // 1. New validator V5 joining below min_self_stake rejected; >= min_self_stake succeeds
    let v5 = ActorIdV1::from_bytes([5; 32]);
    assert_eq!(
        st.bond_stake(v5, 50_000_000_000, 1).unwrap().disposition,
        EconomicDispositionV1::RejectedBelowMinStake
    );
    assert_eq!(
        st.bond_stake(v5, 100_000_000_000, 1).unwrap().disposition,
        EconomicDispositionV1::Admitted
    );
    assert_eq!(st.eligible_validators().len(), 5);

    // 2. Malformed zero slash evidence & duplicate slash evidence reject fail-closed
    assert_eq!(
        st.apply_inactivity_slash(v[1], [0; 32], 2)
            .unwrap()
            .disposition,
        EconomicDispositionV1::RejectedMalformedEvidence
    );
    let r_inact = st.apply_inactivity_slash(v[1], [0xBB; 32], 2).unwrap();
    assert_eq!(r_inact.disposition, EconomicDispositionV1::Admitted);
    // 1% of 100B = 1B burned; V2 NOT permanently jailed (inactivity != equivocation)
    assert_eq!(r_inact.amount_delta, 1_000_000_000);
    assert!(!st.validator_record(&v[1]).unwrap().jailed);
    assert_eq!(
        st.apply_inactivity_slash(v[1], [0xBB; 32], 3)
            .unwrap()
            .disposition,
        EconomicDispositionV1::RejectedDuplicateEpochOrEvidence
    );

    // 3. Epoch rollback refused fail-closed
    assert_eq!(
        st.bond_stake(v[0], 1_000_000, 1).unwrap().disposition,
        EconomicDispositionV1::RejectedInvalidEpoch
    );

    // 4. Proof roundtrip & cross-domain isolation (Domain 6 proof MUST fail under Domain 3 & 5)
    let root = st.root_commitment().unwrap();
    let proof = st.prove_validator(&v5).unwrap();
    let raw = proof
        .verify(
            DomainIdV1::EconomicStake,
            root.economic_trie_root,
            v5.as_bytes(),
        )
        .unwrap()
        .expect("v5 included");
    let rec = ValidatorStakeRecordV1::decode_canonical(&raw).unwrap();
    assert_eq!(rec.bonded_stake, 100_000_000_000);
    assert!(matches!(
        proof.verify(
            DomainIdV1::NativeBalance,
            root.economic_trie_root,
            v5.as_bytes(),
        ),
        Err(AuthCommit1Error::CrossDomainProofRejected { .. })
    ));
    assert!(matches!(
        proof.verify(
            DomainIdV1::GovernanceState,
            root.economic_trie_root,
            v5.as_bytes(),
        ),
        Err(AuthCommit1Error::CrossDomainProofRejected { .. })
    ));
    st.verify_rebuild_parity().unwrap();
    st.verify_conservation().unwrap();
}
