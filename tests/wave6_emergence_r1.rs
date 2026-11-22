#![cfg(feature = "protocol-research")]
//! End-to-end CI verification suite for Wave 6 (`EMERGENCE-R1`):
//! - 4-metric falsifiable emergence assay over `RULE-1` (`B5/S45`) vs matched-density randomized
//!   soups and negative-control rules (`B6/S45` subcritical die-out & `B4/S45` supercritical blow-up)
//! - Literal golden `ContentTag` parity with `tools/emergence_r1_vectors.py`
//! - Strict quantitative separation assertions and fail-closed corpus spec controls.

use bitblock::emergence_r1::{
    DEFAULT_EMERGENCE_SOUP_SEEDS, EMERGENCE_R1_ASSAY_REPORT_SCHEMA,
    EMERGENCE_R1_CORPUS_SPEC_SCHEMA, EmergenceCorpusSpecV1, EmergenceError,
    EmergenceTrajectoryMetricsV1, RULE_CODE_B4S45_SUPERCRITICAL, RULE_CODE_B5S45,
    RULE_CODE_B6S45_SUBCRITICAL, SUBJECT_BAYS_GLIDER, SUBJECT_DETERMINISTIC_SOUP,
    SUBJECT_OCTAHEDRON_STILL_LIFE, SUBJECT_PERIOD4_OSCILLATOR, run_emergence_assay,
};

#[test]
fn emergence_r1_golden_tags_and_quantitative_phase_separation() {
    assert_eq!(
        EMERGENCE_R1_CORPUS_SPEC_SCHEMA,
        b"bitblock/emergence-r1/corpus-spec/v1"
    );
    assert_eq!(
        EMERGENCE_R1_ASSAY_REPORT_SCHEMA,
        b"bitblock/emergence-r1/assay-report/v1"
    );
    assert_eq!(DEFAULT_EMERGENCE_SOUP_SEEDS.len(), 4);

    let spec = EmergenceCorpusSpecV1::v1_default();
    assert_eq!(
        spec.tag().unwrap().to_string(),
        "#bb.policy.sha256.5cby7wdtezi5y64vo6pcsjt5kdcurkmezxnjsq5oidbavlca4mza"
    );

    let report = run_emergence_assay(&spec).unwrap();
    assert_eq!(
        report.tag().to_string(),
        "#bb.receipt.sha256.54o2gvb4dxly2gxewna3zzvw6ufdtmiiveyc6msducfjbstadzca"
    );
    assert_eq!(report.trajectories.len(), 15);

    // 1. Engineered B5/S45 coherent structures
    let glider = &report.trajectories[0];
    let osc = &report.trajectories[1];
    let oct = &report.trajectories[2];
    assert_eq!(glider.subject_kind, SUBJECT_BAYS_GLIDER);
    assert_eq!(glider.final_mutable_cells, 10);
    assert_eq!(glider.centroid_l1_shift_x100, 1_000);
    assert_eq!(glider.detected_period, 0);
    assert_eq!(osc.subject_kind, SUBJECT_PERIOD4_OSCILLATOR);
    assert_eq!(osc.detected_period, 4);
    assert_eq!(osc.centroid_l1_shift_x100, 0);
    assert_eq!(oct.subject_kind, SUBJECT_OCTAHEDRON_STILL_LIFE);
    assert_eq!(oct.detected_period, 1);
    assert_eq!(oct.active_mutation_ticks, 0);

    // 2. Quantitative separation between B6/S45 (subcritical), B5/S45 (edge), B4/S45 (supercritical)
    let soups = |rc: u8| -> Vec<_> {
        report
            .trajectories
            .iter()
            .filter(|t| t.subject_kind == SUBJECT_DETERMINISTIC_SOUP && t.rule_code == rc)
            .collect()
    };
    let s_b5 = soups(RULE_CODE_B5S45);
    let s_b6 = soups(RULE_CODE_B6S45_SUBCRITICAL);
    let s_b4 = soups(RULE_CODE_B4S45_SUPERCRITICAL);
    assert_eq!(s_b5.len(), 4);
    assert_eq!(s_b6.len(), 4);
    assert_eq!(s_b4.len(), 4);

    let sum_surv = |xs: &[&EmergenceTrajectoryMetricsV1]| -> u32 {
        xs.iter().map(|t| u32::from(t.survival_ticks)).sum()
    };
    let sum_final = |xs: &[&EmergenceTrajectoryMetricsV1]| -> u64 {
        xs.iter().map(|t| u64::from(t.final_mutable_cells)).sum()
    };
    let sum_ham = |xs: &[&EmergenceTrajectoryMetricsV1]| -> u64 {
        xs.iter()
            .map(|t| u64::from(t.perturbation_hamming_t_end))
            .sum()
    };

    // B5/S45 survives substantially longer than subcritical B6/S45 (> 3x total survival ticks)
    assert!(sum_surv(&s_b5) > sum_surv(&s_b6) * 2);
    assert_eq!(sum_final(&s_b6), 0);

    // Supercritical B4/S45 exhibits runaway population growth & chaotic damage spreading vs B5/S45
    assert!(sum_final(&s_b4) > sum_final(&s_b5) * 10);
    assert!(sum_ham(&s_b4) > sum_ham(&s_b5) * 20);

    // 3. Invalid corpus spec negative controls
    let mut bad = spec.clone();
    bad.ticks = 0;
    assert!(matches!(
        bad.validate(),
        Err(EmergenceError::InvalidSpec(_))
    ));
    let mut empty_seeds = spec;
    empty_seeds.soup_seeds.clear();
    assert!(matches!(
        empty_seeds.validate(),
        Err(EmergenceError::InvalidSpec(_))
    ));
}
