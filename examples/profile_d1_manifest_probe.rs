//! Manifest & CE-43 lineage probe for `PROFILE-D1` (`PS-079`).

use bitblock::crypto_d1::CryptoSuiteDescriptorV1;
use bitblock::profile_d1::{
    BoundProfileManifestV1, CandidateProfileManifestV1, SubclaimDispositionV1,
    SubclaimEvidenceStatusV1,
};
use bitblock::protocol::ContentTag;

const DEVNET_GENESIS_STATE_TAG: &str =
    "#bb.policy.sha256.jowjx6wyiayipiaypxgmjc2xgynt335d7mrx3qfx2e6pxegyshgq";
const PROTOCOL_BOUNDS_TAG: &str =
    "#bb.policy.sha256.yecxf5k5a5gs3y7qm76pzax2f5e6bkidmxytmsthsxhdqozzplaa";
const RESOURCE_POLICY_TAG: &str =
    "#bb.policy.sha256.6yvp73mvdkouoxk2xiolnj33lbxucxta5ikhjcwnvd267tzfrfyq";

fn hex_short(bytes: &[u8; 32]) -> String {
    bytes[..6].iter().map(|b| format!("{b:02x}")).collect()
}

fn disp_str(d: SubclaimDispositionV1, pr: &str, ev: SubclaimEvidenceStatusV1) -> String {
    let ev_s = match ev {
        SubclaimEvidenceStatusV1::Satisfied => "Satisfied",
        SubclaimEvidenceStatusV1::EvidencePending => "Pending",
    };
    match d {
        SubclaimDispositionV1::Required => format!("Required ({ev_s})"),
        SubclaimDispositionV1::ConditionalWithPredicate => format!("Cond[{pr}] ({ev_s})"),
        SubclaimDispositionV1::ExcludedWithReason => format!("Excluded ({pr})"),
    }
}

fn main() {
    let genesis_id = ContentTag::parse_full(DEVNET_GENESIS_STATE_TAG)
        .unwrap()
        .digest();
    let bounds_d = ContentTag::parse_full(PROTOCOL_BOUNDS_TAG)
        .unwrap()
        .digest();
    let crypto_d = CryptoSuiteDescriptorV1::v1_ed25519_sha256()
        .tag()
        .unwrap()
        .digest();
    let resource_d = ContentTag::parse_full(RESOURCE_POLICY_TAG)
        .unwrap()
        .digest();

    let launch_cand = CandidateProfileManifestV1::canonical_launch_v1(
        genesis_id, [0x21; 32], [0x31; 32], bounds_d, crypto_d, resource_d,
    )
    .unwrap();
    let launch_bound = BoundProfileManifestV1::seal(launch_cand).unwrap();

    let succ_cand =
        CandidateProfileManifestV1::canonical_successor_program_v1(&launch_bound, [0x71; 32])
            .unwrap();
    let succ_bound = BoundProfileManifestV1::seal(succ_cand).unwrap();

    let l_active = launch_bound
        .candidate
        .domain_slots
        .iter()
        .filter(|s| s.active)
        .count();
    let l_inactive = launch_bound.candidate.domain_slots.len() - l_active;
    let s_active = succ_bound
        .candidate
        .domain_slots
        .iter()
        .filter(|s| s.active)
        .count();
    let s_inactive = succ_bound.candidate.domain_slots.len() - s_active;

    println!("{{");
    println!(
        "  \"launch_cand_tag\": \"{}\",",
        launch_bound.candidate.tag().unwrap()
    );
    println!(
        "  \"launch_bound_tag\": \"{}\",",
        launch_bound.tag().unwrap()
    );
    println!(
        "  \"succ_cand_tag\": \"{}\",",
        succ_bound.candidate.tag().unwrap()
    );
    println!("  \"succ_bound_tag\": \"{}\",", succ_bound.tag().unwrap());
    println!("  \"genesis_id_short\": \"{}\",", hex_short(&genesis_id));
    println!(
        "  \"launch_profile_id_short\": \"{}\",",
        hex_short(&launch_bound.profile_id)
    );
    println!(
        "  \"successor_profile_id_short\": \"{}\",",
        hex_short(&succ_bound.profile_id)
    );
    println!(
        "  \"launch_predicates\": {},",
        launch_bound.candidate.activated_predicates.len()
    );
    println!(
        "  \"successor_predicates\": {},",
        succ_bound.candidate.activated_predicates.len()
    );
    println!("  \"launch_active_domains\": {},", l_active);
    println!("  \"launch_inactive_domains\": {},", l_inactive);
    println!("  \"successor_active_domains\": {},", s_active);
    println!("  \"successor_inactive_domains\": {},", s_inactive);
    println!("  \"subclaim_rows\": [");
    let scopes = [
        ("G15.economic", "economic-charging (fee/deposit invariants)"),
        ("G15.resource", "always mandatory (BOUNDS-1 / DoS caps)"),
        ("G19.program", "program-active (on-chain VM extension)"),
        ("G19.upgrade", "in-place-upgrade (migration lifecycle)"),
        ("G19.verkle", "verkle-selected (separate R&D lane)"),
        ("G5.asset", "asset-active (transfer/supply conservation)"),
        ("G5.core", "always mandatory (occupancy/episode atomicity)"),
    ];
    for (idx, (sid, desc)) in scopes.iter().enumerate() {
        let lr = launch_bound
            .candidate
            .subclaims
            .iter()
            .find(|r| r.subclaim_id == *sid)
            .unwrap();
        let sr = succ_bound
            .candidate
            .subclaims
            .iter()
            .find(|r| r.subclaim_id == *sid)
            .unwrap();
        let comma = if idx + 1 < scopes.len() { "," } else { "" };
        println!(
            "    {{\"id\": \"{}\", \"scope\": \"{}\", \"launch\": \"{}\", \"successor\": \"{}\"}}{}",
            sid,
            desc,
            disp_str(lr.disposition, &lr.predicate_or_reason, lr.evidence_status),
            disp_str(sr.disposition, &sr.predicate_or_reason, sr.evidence_status),
            comma
        );
    }
    println!("  ]");
    println!("}}");
}
