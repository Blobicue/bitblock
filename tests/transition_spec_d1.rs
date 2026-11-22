//! Integration tests for `TRANSITION-SPEC-D1` (`crates/research/src/transition_spec.rs`).
//!
//! Exercises the complete public contract, extension registry, batch codec, receipt codec,
//! and atomic executor surface from an external integration boundary.

#![cfg(feature = "protocol-research")]

use bitblock::coordinate_episode::OccupancyEpisodeLedger;
use bitblock::history::History;
use bitblock::program::{Edit, Operation};
use bitblock::transition_spec::{
    BASE_RESERVED_DISPOSITION_MAX, BASE_RESERVED_OP_TAG_MAX, BASE_STATE_DOMAINS_MASK,
    BASE_TRANSITION_SPEC_VERSION, BaseDispositionV1, BaseTransitionBatchV1,
    BaseTransitionReceiptV1, BaseTransitionSpecD1, ConflictPolicy, DOMAIN_BIT_ACTIVE_EPISODE,
    DOMAIN_BIT_EPISODE_HISTORY, DOMAIN_BIT_WORLD_OCCUPANCY, EXTENSION_SLOT_WIRE_LEN,
    ExtensionInterfaceRegistryV1, ExtensionSlotDescriptorV1, MAX_BASE_BATCH_OPS,
    TRANSITION_SPEC_D1_BASE_RECEIPT_SCHEMA, TRANSITION_SPEC_D1_CONTRACT_SCHEMA,
    TRANSITION_SPEC_D1_EXTENSION_SLOT_SCHEMA, TransitionSpecError, execute_base_transition_bytes,
};
use bitblock::world::{Coord, World};

#[test]
fn transition_spec_d1_public_contract_and_negative_controls() {
    assert_eq!(
        BASE_STATE_DOMAINS_MASK,
        DOMAIN_BIT_WORLD_OCCUPANCY | DOMAIN_BIT_ACTIVE_EPISODE | DOMAIN_BIT_EPISODE_HISTORY
    );
    assert_eq!(BASE_RESERVED_OP_TAG_MAX, 15);
    assert_eq!(BASE_RESERVED_DISPOSITION_MAX, 15);
    assert_eq!(EXTENSION_SLOT_WIRE_LEN, 32);
    assert!(!TRANSITION_SPEC_D1_CONTRACT_SCHEMA.is_empty());
    assert!(!TRANSITION_SPEC_D1_EXTENSION_SLOT_SCHEMA.is_empty());
    assert!(!TRANSITION_SPEC_D1_BASE_RECEIPT_SCHEMA.is_empty());

    assert_eq!(
        ConflictPolicy::from_code(0),
        Some(ConflictPolicy::RejectConcurrentWrite)
    );
    assert_eq!(
        ConflictPolicy::from_code(1),
        Some(ConflictPolicy::DeterministicOrder)
    );
    assert_eq!(ConflictPolicy::from_code(2), None);
    assert_eq!(ConflictPolicy::RejectConcurrentWrite.code(), 0);

    for code in 0..=6u8 {
        let disp = BaseDispositionV1::from_code(code).expect("valid disposition code");
        assert_eq!(disp.code(), code);
    }
    assert_eq!(BaseDispositionV1::from_code(7), None);

    let slot1 = ExtensionSlotDescriptorV1 {
        slot_id: 1,
        op_tag_min: 16,
        op_tag_max: 23,
        disposition_tag_min: 16,
        disposition_tag_max: 23,
        owned_domain_bit: 3,
        read_domains_mask: BASE_STATE_DOMAINS_MASK | (1 << 3),
        write_domains_mask: 1 << 3,
        base_work_units: 10,
        per_item_work_units: 2,
        conflict_policy: ConflictPolicy::RejectConcurrentWrite,
    };
    let slot2 = ExtensionSlotDescriptorV1 {
        slot_id: 2,
        op_tag_min: 24,
        op_tag_max: 31,
        disposition_tag_min: 24,
        disposition_tag_max: 31,
        owned_domain_bit: 4,
        read_domains_mask: BASE_STATE_DOMAINS_MASK,
        write_domains_mask: 1 << 4,
        base_work_units: 12,
        per_item_work_units: 3,
        conflict_policy: ConflictPolicy::DeterministicOrder,
    };
    assert!(slot1.tag().is_ok());
    let reg = ExtensionInterfaceRegistryV1::new(vec![slot1, slot2]).expect("valid registry");
    assert_eq!(reg.slots().len(), 2);

    let spec = BaseTransitionSpecD1::with_extensions(reg);
    let spec_bytes = spec.encode_canonical().expect("encode spec");
    assert_eq!(
        BaseTransitionSpecD1::decode_canonical(&spec_bytes),
        Ok(spec.clone())
    );
    assert_eq!(spec.work_cost(5), Ok(5));

    // Exhaustive truncation on contract bytes
    for cut in 0..spec_bytes.len() {
        assert!(BaseTransitionSpecD1::decode_canonical(&spec_bytes[..cut]).is_err());
    }

    // Over-limit batch rejected before allocation
    let mut over_limit = Vec::new();
    over_limit.extend_from_slice(&BASE_TRANSITION_SPEC_VERSION.to_be_bytes());
    over_limit.extend_from_slice(&1u64.to_be_bytes());
    over_limit.extend_from_slice(&((MAX_BASE_BATCH_OPS as u32) + 1).to_be_bytes());
    assert_eq!(
        BaseTransitionBatchV1::decode_canonical(&over_limit, MAX_BASE_BATCH_OPS),
        Err(TransitionSpecError::BatchOverLimit {
            count: MAX_BASE_BATCH_OPS + 1,
            limit: MAX_BASE_BATCH_OPS
        })
    );

    // Execute empty idle batch + non-empty edit batch and verify receipt roundtrip
    let initial = World::new();
    let mut world = initial.clone();
    let mut history = History::new(&initial);
    let mut episodes = OccupancyEpisodeLedger::new(history.genesis());

    let idle_batch = BaseTransitionBatchV1 {
        spec_version: BASE_TRANSITION_SPEC_VERSION,
        tick: 0,
        edits: Vec::new(),
    };
    let idle_receipt = execute_base_transition_bytes(
        &spec,
        &initial,
        &mut world,
        &mut history,
        &mut episodes,
        &idle_batch.encode_canonical().expect("idle batch"),
    )
    .expect("idle transition");
    assert!(idle_receipt.dispositions.is_empty());
    assert_eq!(idle_receipt.work_units, 0);
    assert!(idle_receipt.tag().is_ok());

    let active_batch = BaseTransitionBatchV1 {
        spec_version: BASE_TRANSITION_SPEC_VERSION,
        tick: 1,
        edits: vec![
            Edit::new(Operation::On, Coord::new(-16, 0, 15)),
            Edit::new(Operation::Off, Coord::new(-16, 0, 15)),
        ],
    };
    let active_receipt = execute_base_transition_bytes(
        &spec,
        &initial,
        &mut world,
        &mut history,
        &mut episodes,
        &active_batch.encode_canonical().expect("active batch"),
    )
    .expect("active transition");
    assert_eq!(
        active_receipt.dispositions,
        vec![BaseDispositionV1::Mint, BaseDispositionV1::Burn]
    );
    let r_bytes = active_receipt.encode_canonical().expect("encode receipt");
    assert_eq!(
        BaseTransitionReceiptV1::decode_canonical(&r_bytes),
        Ok(active_receipt)
    );
    for cut in 0..r_bytes.len() {
        assert!(BaseTransitionReceiptV1::decode_canonical(&r_bytes[..cut]).is_err());
    }
}
