#![cfg(feature = "protocol-research")]
//! End-to-end CI verification suite for `RULE-1` (synchronous 3D Moore-neighborhood Life `B5/S45`).
//!
//! Asserts byte-exact golden `ContentTag` parity with the independent Python reference stepper
//! (`tools/rule_1_vectors.py`), 48-way octahedral symmetry invariance on Carter Bays' 10-cell
//! period-4 3D glider, still-life quiescence, period-4 oscillator return, permanent bedrock
//! survival, `i64::MIN`/`i64::MAX` boundary safety, fail-closed resource limits, and deterministic
//! `(initial_world, accepted_actions, rule_version)` replay parity with `History`.

use std::collections::BTreeSet;

use bitblock::history::{History, world_tag};
use bitblock::rule_1::{
    BAYS_4555_BIRTH_MASK, BAYS_4555_SURVIVE_MASK, DEFAULT_RULE_1_MAX_CANDIDATE_CELLS,
    DEFAULT_RULE_1_MAX_MUTATIONS_PER_TICK, RULE_1_DESCRIPTOR_SCHEMA, RULE_1_STEP_RECEIPT_SCHEMA,
    Rule1DescriptorV1, Rule1Error, Rule1StepPlan, Rule1StepReceiptV1, bays_4555_glider_coords,
    compute_rule_1_plan, octahedral_transform, octahedron_still_life_coords,
    period_4_oscillator_coords, step_rule_1, step_rule_1_with_history,
};
use bitblock::world::{BEDROCK_COORD, Coord, World};

#[test]
fn rule_1_golden_tags_match_independent_python_stepper() {
    assert_eq!(RULE_1_DESCRIPTOR_SCHEMA, b"bitblock/rule-1/descriptor/v1");
    assert_eq!(
        RULE_1_STEP_RECEIPT_SCHEMA,
        b"bitblock/rule-1/step-receipt/v1"
    );
    assert_eq!(BAYS_4555_BIRTH_MASK, 1 << 5);
    assert_eq!(BAYS_4555_SURVIVE_MASK, (1 << 4) | (1 << 5));
    assert_eq!(DEFAULT_RULE_1_MAX_CANDIDATE_CELLS, 1_000_000);
    assert_eq!(DEFAULT_RULE_1_MAX_MUTATIONS_PER_TICK, 65_536);

    let desc = Rule1DescriptorV1::bays_4555_default();
    assert_eq!(
        desc.tag().unwrap().to_string(),
        "#bb.policy.sha256.jckp2uhbjj2fa4evhth5sqyzde45yy7bnpl4htsbuezxw55itdqq"
    );

    // 1. Bays 10-cell glider at (20, 20, 20) + bedrock for 4 ticks -> matches Python golden tags.
    let mut world = World::new();
    for c in bays_4555_glider_coords(Coord::new(20, 20, 20)) {
        world.set_on(c);
    }
    let mut hist_world = world.clone();
    let mut history = History::new(&hist_world);

    let r1 = step_rule_1(&mut world, &desc, 1).unwrap();
    let hr1 = step_rule_1_with_history(&mut hist_world, &mut history, &desc, 1).unwrap();
    assert_eq!(r1, hr1);
    assert_eq!(
        r1.tag().to_string(),
        "#bb.receipt.sha256.iixge4c2kypetpd2futpdd3cdiyr4e5pm2kpuf3zcx5oknbii57a"
    );

    let mut r4 = r1;
    for tick in 2..=4 {
        r4 = step_rule_1(&mut world, &desc, tick).unwrap();
        let hr = step_rule_1_with_history(&mut hist_world, &mut history, &desc, tick).unwrap();
        assert_eq!(r4, hr);
    }
    assert_eq!(
        r4.tag().to_string(),
        "#bb.receipt.sha256.onankhpta54nmiufuyfkxpgqohp3nxjghr3dvvhlewqpyc64jfyq"
    );
    assert_eq!(world_tag(&world), world_tag(&hist_world));

    // 2. 6-cell octahedron still life at (0, 0, 6) -> quiescent tick 1 matches Python golden tag.
    let mut still = World::new();
    for c in octahedron_still_life_coords(Coord::new(0, 0, 6)) {
        still.set_on(c);
    }
    let rs = step_rule_1(&mut still, &desc, 1).unwrap();
    assert!(rs.quiescent);
    assert_eq!(
        rs.tag().to_string(),
        "#bb.receipt.sha256.pp2w2naumy5a7v3dwwr72lmjd2qtkklbrwhbamb52xbabsjr74pa"
    );
    let raw_rs = rs.encode_canonical();
    assert_eq!(Rule1StepReceiptV1::decode_canonical(&raw_rs).unwrap(), rs);
}

#[test]
fn rule_1_48_octahedral_symmetries_oscillator_extrema_and_fail_closed_limits() {
    let desc = Rule1DescriptorV1::bays_4555_default();

    // 1. All 48 octahedral symmetries of Bays' 10-cell glider translate by transformed (0, +1, +1)
    //    in 4 ticks with exact 10-cell preservation.
    let anchor = Coord::new(100, -100, 100);
    let base = bays_4555_glider_coords(Coord::new(0, 0, 0));
    for sym in 0u8..48 {
        let mut w = World::new();
        let mut expected_after_4 = BTreeSet::new();
        let shift = octahedral_transform(Coord::new(0, 1, 1), sym);
        for c in base {
            let t = octahedral_transform(c, sym);
            let placed = Coord::new(anchor.x + t.x, anchor.y + t.y, anchor.z + t.z);
            w.set_on(placed);
            expected_after_4.insert(Coord::new(
                placed.x + shift.x,
                placed.y + shift.y,
                placed.z + shift.z,
            ));
        }
        for tick in 1..=4 {
            step_rule_1(&mut w, &desc, tick).unwrap();
        }
        let actual: BTreeSet<Coord> = w.iter_on().filter(|&c| c != BEDROCK_COORD).collect();
        assert_eq!(
            actual, expected_after_4,
            "octahedral symmetry {sym} failed exact glider translation"
        );
    }

    // 2. Period-4 3D oscillator returns to exact initial world_tag every 4 ticks.
    let mut osc = World::new();
    for c in period_4_oscillator_coords(Coord::new(-20, 5, 10)) {
        osc.set_on(c);
    }
    let osc_start = world_tag(&osc);
    let r1 = step_rule_1(&mut osc, &desc, 1).unwrap();
    assert!(!r1.quiescent);
    assert_ne!(world_tag(&osc), osc_start);
    for tick in 2..=4 {
        step_rule_1(&mut osc, &desc, tick).unwrap();
    }
    assert_eq!(world_tag(&osc), osc_start);

    // 3. i64::MAX / i64::MIN boundary safety: no overflow panic; isolated corner cell dies cleanly.
    let mut corner = World::new();
    corner.set_on(Coord::new(i64::MAX, i64::MIN, i64::MAX));
    let plan: Rule1StepPlan = compute_rule_1_plan(&corner, &desc).unwrap();
    assert_eq!(plan.deaths, 1);
    assert_eq!(plan.births, 0);
    step_rule_1(&mut corner, &desc, 1).unwrap();
    assert!(!corner.is_on(Coord::new(i64::MAX, i64::MIN, i64::MAX)));
    assert!(corner.is_on(BEDROCK_COORD));

    // 4. Fail-closed resource limits leave World byte-identical.
    let mut bounded = World::new();
    for c in bays_4555_glider_coords(Coord::new(20, 20, 20)) {
        bounded.set_on(c);
    }
    let before = world_tag(&bounded);
    let tight_cand = Rule1DescriptorV1::bays_4555_with_limits(10, 100);
    assert!(matches!(
        step_rule_1(&mut bounded, &tight_cand, 1),
        Err(Rule1Error::CandidateLimitExceeded { .. })
    ));
    assert_eq!(world_tag(&bounded), before);

    let tight_mut = Rule1DescriptorV1::bays_4555_with_limits(10_000, 1);
    assert!(matches!(
        step_rule_1(&mut bounded, &tight_mut, 1),
        Err(Rule1Error::MutationLimitExceeded { .. })
    ));
    assert_eq!(world_tag(&bounded), before);

    // 5. Invalid descriptor (B0 or bedrock_clamped_on = false) rejected fail-closed.
    let mut bad_desc = desc;
    bad_desc.bedrock_clamped_on = false;
    assert!(bad_desc.validate().is_err());
    bad_desc.bedrock_clamped_on = true;
    bad_desc.birth_mask = 1;
    assert!(bad_desc.validate().is_err());
}
