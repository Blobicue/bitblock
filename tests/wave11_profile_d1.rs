#![cfg(feature = "protocol-research")]

use bitblock::crypto_d1::CryptoSuiteDescriptorV1;
use bitblock::profile_d1::{
    BoundProfileManifestV1, CandidateProfileManifestV1, ProfileD1Error, ProfileLineageKindV1,
    SubclaimDispositionV1,
};
use bitblock::protocol::ContentTag;

const DEVNET_GENESIS_STATE_TAG: &str =
    "#bb.policy.sha256.jowjx6wyiayipiaypxgmjc2xgynt335d7mrx3qfx2e6pxegyshgq";
const CRYPTO_SUITE_TAG: &str =
    "#bb.policy.sha256.jczjgonfconrys2yy7cgehmsk6mh6757m4r4piyg5ynglq2uanlq";
const PROTOCOL_BOUNDS_TAG: &str =
    "#bb.policy.sha256.yecxf5k5a5gs3y7qm76pzax2f5e6bkidmxytmsthsxhdqozzplaa";
const RESOURCE_POLICY_TAG: &str =
    "#bb.policy.sha256.6yvp73mvdkouoxk2xiolnj33lbxucxta5ikhjcwnvd267tzfrfyq";
const GOLDEN_LAUNCH_CAND_TAG: &str =
    "#bb.policy.sha256.r4ldc6k33p7evc4zg67ksajcpr6dxeiwuazimy6pxhwgaepepaoq";
const GOLDEN_LAUNCH_BOUND_TAG: &str =
    "#bb.receipt.sha256.24biozgdf4dc7n4z5llunavtkegk433r5qfh4plknnjgtvw4naoq";
const GOLDEN_SUCC_CAND_TAG: &str =
    "#bb.policy.sha256.q3sxi64vigpwbcm2yhh4jgtwm7dfxvsva7k4phdjsswbeqeje6ha";
const GOLDEN_SUCC_BOUND_TAG: &str =
    "#bb.receipt.sha256.6sik5vhte62uajm53n43axcz3lylfpiindawhjvried475wxxxka";

fn build_launch_and_successor() -> (BoundProfileManifestV1, BoundProfileManifestV1) {
    let genesis_id = ContentTag::parse_full(DEVNET_GENESIS_STATE_TAG)
        .unwrap()
        .digest();
    let bounds_d = ContentTag::parse_full(PROTOCOL_BOUNDS_TAG)
        .unwrap()
        .digest();
    let crypto_tag = CryptoSuiteDescriptorV1::v1_ed25519_sha256().tag().unwrap();
    assert_eq!(crypto_tag.to_string(), CRYPTO_SUITE_TAG);
    let resource_d = ContentTag::parse_full(RESOURCE_POLICY_TAG)
        .unwrap()
        .digest();

    let launch_cand = CandidateProfileManifestV1::canonical_launch_v1(
        genesis_id,
        [0x21; 32],
        [0x31; 32],
        bounds_d,
        crypto_tag.digest(),
        resource_d,
    )
    .unwrap();
    let launch_bound = BoundProfileManifestV1::seal(launch_cand).unwrap();
    let succ_cand =
        CandidateProfileManifestV1::canonical_successor_program_v1(&launch_bound, [0x71; 32])
            .unwrap();
    let succ_bound = BoundProfileManifestV1::seal(succ_cand).unwrap();
    (launch_bound, succ_bound)
}

#[test]
fn wave11_profile_d1_golden_python_parity_and_ce43_lineage() {
    let (launch, succ) = build_launch_and_successor();
    assert_eq!(
        launch.candidate.tag().unwrap().to_string(),
        GOLDEN_LAUNCH_CAND_TAG
    );
    assert_eq!(launch.tag().unwrap().to_string(), GOLDEN_LAUNCH_BOUND_TAG);
    assert_eq!(
        succ.candidate.tag().unwrap().to_string(),
        GOLDEN_SUCC_CAND_TAG
    );
    assert_eq!(succ.tag().unwrap().to_string(), GOLDEN_SUCC_BOUND_TAG);

    // Exact wire roundtrips and negative truncation checks.
    let raw_l = launch.encode_canonical().unwrap();
    assert_eq!(
        BoundProfileManifestV1::decode_canonical(&raw_l).unwrap(),
        launch
    );
    assert!(BoundProfileManifestV1::decode_canonical(&raw_l[..raw_l.len() - 1]).is_err());

    let raw_s = succ.encode_canonical().unwrap();
    assert_eq!(
        BoundProfileManifestV1::decode_canonical(&raw_s).unwrap(),
        succ
    );
    assert!(BoundProfileManifestV1::decode_canonical(&raw_s[..10]).is_err());

    // CE-43 verification: launch current_scope == genesis_scope; successor chains predecessor_profile_id
    // while keeping immutable genesis_id identical.
    assert_eq!(
        launch.candidate.lineage_kind,
        ProfileLineageKindV1::InitialLaunch
    );
    assert_eq!(launch.candidate.predecessor_profile_id, None);
    assert_eq!(
        launch.candidate.current_scope_digest,
        launch.candidate.genesis_scope_digest
    );
    assert_eq!(
        succ.candidate.lineage_kind,
        ProfileLineageKindV1::SuccessorCandidate
    );
    assert_eq!(
        succ.candidate.predecessor_profile_id,
        Some(launch.profile_id)
    );
    assert_eq!(succ.candidate.genesis_id, launch.candidate.genesis_id);
    assert_ne!(
        succ.candidate.current_scope_digest,
        succ.candidate.genesis_scope_digest
    );
}

#[test]
fn wave11_profile_d1_adversarial_closure_subclaims_and_non_circularity() {
    let (launch, succ) = build_launch_and_successor();

    // 1. Self-reference: profile_id injected into pre-ID digest or predecessor rejects fail-closed.
    let mut self_ref = launch.clone();
    self_ref.candidate.domain_registry_digest = self_ref.profile_id;
    assert_eq!(
        self_ref.verify(),
        Err(ProfileD1Error::SelfReferenceDetected)
    );

    // 2. Tampered profile_id mismatches candidate digest.
    let mut bad_pid = launch.clone();
    bad_pid.profile_id[0] ^= 0xFF;
    assert_eq!(bad_pid.verify(), Err(ProfileD1Error::ProfileIdMismatch));

    // 3. CE-43 violation: InitialLaunch with distinct current_scope or non-None predecessor fails.
    let mut bad_init = launch.candidate.clone();
    bad_init.current_scope_digest = [0x99; 32];
    assert!(bad_init.validate().is_err());

    let mut bad_succ = succ.candidate.clone();
    bad_succ.current_scope_digest = bad_succ.genesis_scope_digest;
    assert!(bad_succ.validate().is_err());

    // 4. Predicate closure violation: dropping entailed `asset-active` while `economic-charging`
    //    is present rejects fail-closed.
    let mut missing_entail = launch.candidate.clone();
    missing_entail
        .activated_predicates
        .retain(|p| p != "asset-active");
    assert!(missing_entail.validate().is_err());

    // 5. Unregistered / reserved `always` token rejects fail-closed.
    let mut with_always = launch.candidate.clone();
    with_always.activated_predicates.push("always".to_string());
    with_always.activated_predicates.sort();
    assert!(with_always.validate().is_err());

    // 6. Mandatory G5.core / G15.resource cannot be downgraded to Excluded or Conditional.
    let mut drop_g5 = launch.candidate.clone();
    for r in &mut drop_g5.subclaims {
        if r.subclaim_id == "G5.core" {
            r.disposition = SubclaimDispositionV1::ExcludedWithReason;
            r.predicate_or_reason = "attempted-bypass".to_string();
        }
    }
    assert!(drop_g5.validate().is_err());

    // 7. Activating G19.program while program-active is inactive on launch rejects fail-closed.
    let mut fake_g19 = launch.candidate.clone();
    for r in &mut fake_g19.subclaims {
        if r.subclaim_id == "G19.program" {
            r.disposition = SubclaimDispositionV1::Required;
            r.predicate_or_reason = "program-active".to_string();
        }
    }
    assert!(fake_g19.validate().is_err());

    // 8. Flipping domain slot 6 active on launch (where program-active is false) rejects fail-closed.
    let mut bad_slot = launch.candidate.clone();
    bad_slot.domain_slots[6].active = true;
    bad_slot.domain_slots[6].commitment_profile_id = 1;
    assert!(bad_slot.validate().is_err());
}
