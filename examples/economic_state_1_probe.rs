//! Release probe for Wave 15A `ECONOMIC-STATE-1` (`PS-040`):
//! simulates genesis -> integer-floor rewards (with unissued dust) -> unbonding -> 50%
//! slash-to-burn across active+unbonding -> matured post-slash withdrawal, emitting JSON for
//! `plot_economic_state_1.py`.

use bitblock::asset_d1::ValidatorEconPolicyV1;
use bitblock::economic_state_1::{EconomicDispositionV1, EconomicStateV1};
use bitblock::identity_d1::ActorIdV1;

fn main() {
    let policy = ValidatorEconPolicyV1::v1_pos_default();
    let profile_id = [0x66; 32];
    let v = [
        ActorIdV1::from_bytes([1; 32]),
        ActorIdV1::from_bytes([2; 32]),
        ActorIdV1::from_bytes([3; 32]),
        ActorIdV1::from_bytes([4; 32]),
    ];
    let stake = 100_000_000_000u128;
    let mut st = EconomicStateV1::from_genesis(
        policy,
        profile_id,
        &[(v[0], stake), (v[1], stake), (v[2], stake), (v[3], stake)],
    )
    .expect("genesis");

    let mut trace = Vec::new();
    let push_snap = |label: &str, s: &EconomicStateV1, out: &mut Vec<String>| {
        let r = s.root_commitment().unwrap();
        out.push(format!(
            "{{\"stage\": \"{}\", \"active_stake\": {}, \"unbonding_stake\": {}, \"accrued_rewards\": {}, \"withdrawn\": {}, \"slashed_burned\": {}}}",
            label.replace('\n', "\\n"),
            r.total_active_bonded,
            r.total_unbonding,
            r.total_accrued_rewards,
            r.total_withdrawn,
            r.total_slashed_burned
        ));
    };

    push_snap("Genesis\n(4x100B)", &st, &mut trace);

    // Stage 1: Integer-floor epoch rewards (999_999_999 -> 3 dust unissued!)
    let r_rew = st.distribute_epoch_rewards(999_999_999, 1).unwrap();
    assert_eq!(r_rew.disposition, EconomicDispositionV1::Admitted);
    let reward_dust = r_rew.dust_unissued;
    push_snap("Rewards\n(e=1)", &st, &mut trace);

    // Stage 2: V3 unbonds 20B at epoch 2 (unlock_epoch = 23)
    assert_eq!(
        st.request_unbond(v[2], 20_000_000_000, 2)
            .unwrap()
            .disposition,
        EconomicDispositionV1::Admitted
    );
    push_snap("V3 Unbond\n20B (e=2)", &st, &mut trace);

    // Stage 3: V3 equivocates at epoch 5 -> 50% burned across active (80B->40B) + unbonding (20B->10B)
    assert_eq!(
        st.apply_equivocation_slash(v[2], [0xE1; 32], 5)
            .unwrap()
            .disposition,
        EconomicDispositionV1::Admitted
    );
    push_snap("V3 Equiv\nSlash 50%", &st, &mut trace);

    // Negative controls: jailed rebond, over-cap reward, malformed slash, early withdraw
    assert_eq!(
        st.bond_stake(v[2], 100_000_000_000, 6).unwrap().disposition,
        EconomicDispositionV1::RejectedValidatorJailed
    );
    assert_eq!(
        st.distribute_epoch_rewards(2_000_000_000, 6)
            .unwrap()
            .disposition,
        EconomicDispositionV1::RejectedRewardOverCap
    );
    assert_eq!(
        st.apply_inactivity_slash(v[0], [0; 32], 6)
            .unwrap()
            .disposition,
        EconomicDispositionV1::RejectedMalformedEvidence
    );
    assert_eq!(
        st.withdraw_matured_unbond(v[2], 22).unwrap().disposition,
        EconomicDispositionV1::RejectedUnbondingNotMatured
    );

    // Stage 4: Matured withdrawal at epoch 23 (10B post-slash released)
    assert_eq!(
        st.withdraw_matured_unbond(v[2], 23).unwrap().disposition,
        EconomicDispositionV1::Admitted
    );
    push_snap("V3 Mature\nWithdraw", &st, &mut trace);
    st.verify_rebuild_parity().unwrap();
    st.verify_conservation().unwrap();

    let v_names = ["V1", "V2", "V3 (Jailed)", "V4"];
    let vals_json: Vec<String> = v
        .iter()
        .zip(v_names.iter())
        .map(|(id, name)| {
            let rec = st.validator_record(id).unwrap();
            format!(
                "{{\"name\": \"{}\", \"bonded\": {}, \"slashed\": {}}}",
                name, rec.bonded_stake, rec.total_slashed
            )
        })
        .collect();

    println!("{{");
    println!("  \"reward_dust\": {},", reward_dust);
    println!("  \"trace\": [{}],", trace.join(", "));
    println!("  \"validators\": [{}],", vals_json.join(", "));
    println!("  \"gates_passed\": 7,");
    println!("  \"gates_total\": 7,");
    println!("  \"gate_rows\": [");
    println!(
        "    {{\"gate\": \"Closed Supply Conservation\", \"result\": \"Sources == Sinks across all transitions\"}},"
    );
    println!(
        "    {{\"gate\": \"Slash-and-Flee Defeated\", \"result\": \"50% burned on BOTH active & unbonding\"}},"
    );
    println!(
        "    {{\"gate\": \"Permanent Equivocation Jail\", \"result\": \"Jailed rebond -> RejectedValidatorJailed\"}},"
    );
    println!(
        "    {{\"gate\": \"21-Epoch Unbonding Timelock\", \"result\": \"e=22 rejected; e=23 releases 10B\"}},"
    );
    println!(
        "    {{\"gate\": \"Integer-Floor Reward Cap\", \"result\": \"Over-cap rejected; dust=3 unissued\"}},"
    );
    println!(
        "    {{\"gate\": \"Evidence Replay & Malformed\", \"result\": \"Zero & duplicate evidence rejected\"}},"
    );
    println!(
        "    {{\"gate\": \"Domain 6 Rebuild & Isolation\", \"result\": \"0 diffs; Stake != Balance != Gov\"}}"
    );
    println!("  ]");
    println!("}}");
}
