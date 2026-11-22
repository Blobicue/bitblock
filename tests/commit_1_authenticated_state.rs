#![cfg(feature = "protocol-research")]
//! `COMMIT-1` integration verification suite: authenticated local state across `WorldRootV1`,
//! `ActiveTokenRootV1`, and `EpisodeRootV1`, context-bound proofs, logical pruning, pin-epoch
//! write-barrier GC, and real-filesystem crash recovery across every publication boundary.

use std::collections::HashSet;
use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use bitblock::commit_r1::CoordinateKeyV1;
use bitblock::commit_state::{
    ActiveTokenRootV1, COMMIT_1_ACTIVE_TOKEN_ROOT_SCHEMA, COMMIT_1_COMMITMENT_SET_SCHEMA,
    COMMIT_1_DOMAIN_REGISTRY_SCHEMA, COMMIT_1_EPISODE_ROOT_SCHEMA, COMMIT_1_NODE_CODEC_VERSION,
    COMMIT_1_OP_LIMITS_SCHEMA, COMMIT_1_POINT_PROOF_SCHEMA, COMMIT_1_PROFILE_SCHEMA,
    COMMIT_1_PROOF_POLICY_SCHEMA, COMMIT_1_ROOT_CODEC_VERSION, COMMIT_1_SCHEMA_VERSION,
    COMMIT_1_WORLD_ROOT_SCHEMA, CommitStateError, CommitmentOperationalLimitsDescriptorV1,
    CommitmentProfileDescriptorV1, CommitmentSetPredecessorV1, CommitmentSetV1,
    CommitmentSubjectV1, CrashPoint, DomainRegistryV1, DomainTagV1, DurableAuthenticatedLedger,
    DurableCommitmentStore, EpisodeRecordV1, EpisodeRootV1, EventIdV1, ExpectedProofContextV1,
    GcSweepOutcome, LocalAuthenticatedSidecar, LookupResult, MemoryCommitmentStore, MintIdV1,
    NodeStoreKey, ON_V1_VALUE_BYTES, ProfileRegistryV1, ProofPolicyDescriptorV1, TypedPatriciaTrie,
    WorldRootV1, typed_empty_root, verify_claim, verify_independent_projection, verify_lookup,
};
use bitblock::coordinate_episode::OccupancyEpisodeLedger;
use bitblock::history::{History, world_tag};
use bitblock::history_store::HistoryReplayLimits;
use bitblock::journal::AnchorPolicy;
use bitblock::local_bounds::LocalStorageProfile;
use bitblock::program::{Edit, Operation};
use bitblock::transition_spec::BaseTransitionSpecD1;
use bitblock::world::{BEDROCK_COORD, Coord, World};

fn unique_temp_dir(label: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock after epoch")
        .as_nanos();
    let dir = std::env::temp_dir().join(format!(
        "bitblock_commit1_{label}_{}_{nanos}",
        std::process::id()
    ));
    fs::create_dir_all(&dir).expect("create temp test dir");
    dir
}

fn scope_digest() -> [u8; 32] {
    BaseTransitionSpecD1::canonical_base_only()
        .tag()
        .expect("base transition spec tag")
        .digest()
}

#[test]
fn insertion_order_independence_and_depth_192_256_zero_suffix_boundaries() {
    let limits = CommitmentOperationalLimitsDescriptorV1::LOCAL_DEFAULT;
    let profiles = ProfileRegistryV1::local_v1();
    assert_eq!(
        *profiles.resolve_op_limits(&limits.id()).unwrap(),
        CommitmentOperationalLimitsDescriptorV1::LOCAL_DEFAULT
    );
    assert_eq!(COMMIT_1_SCHEMA_VERSION, 1);
    assert_eq!(COMMIT_1_NODE_CODEC_VERSION, 1);
    assert_eq!(COMMIT_1_ROOT_CODEC_VERSION, 1);
    assert!(!COMMIT_1_PROOF_POLICY_SCHEMA.is_empty());
    assert!(!COMMIT_1_OP_LIMITS_SCHEMA.is_empty());
    assert!(!COMMIT_1_PROFILE_SCHEMA.is_empty());
    assert!(!COMMIT_1_DOMAIN_REGISTRY_SCHEMA.is_empty());
    assert!(!COMMIT_1_WORLD_ROOT_SCHEMA.is_empty());
    assert!(!COMMIT_1_ACTIVE_TOKEN_ROOT_SCHEMA.is_empty());
    assert!(!COMMIT_1_EPISODE_ROOT_SCHEMA.is_empty());
    assert!(!COMMIT_1_POINT_PROOF_SCHEMA.is_empty());
    assert!(!COMMIT_1_COMMITMENT_SET_SCHEMA.is_empty());

    // 192-bit twin keys differing ONLY at bit 191 (z = 0 vs z = 1 -> depth-192 zero-bit-suffix leaves!)
    let prof_w = CommitmentProfileDescriptorV1::world_v1();
    let coords = [
        BEDROCK_COORD,
        Coord::new(0, 0, 1),
        Coord::new(i64::MIN, i64::MIN, i64::MIN),
        Coord::new(i64::MAX, i64::MAX, i64::MAX),
        Coord::new(-16, 15, -1),
        Coord::new(16, -15, 1),
    ];
    let mut forward = TypedPatriciaTrie::empty(prof_w).unwrap();
    for c in coords {
        assert!(
            forward
                .upsert(
                    CoordinateKeyV1::from_coord(c).as_bytes().to_vec(),
                    ON_V1_VALUE_BYTES.to_vec()
                )
                .unwrap()
        );
    }
    let mut reverse = TypedPatriciaTrie::empty(prof_w).unwrap();
    for c in coords.into_iter().rev() {
        assert!(
            reverse
                .upsert(
                    CoordinateKeyV1::from_coord(c).as_bytes().to_vec(),
                    ON_V1_VALUE_BYTES.to_vec()
                )
                .unwrap()
        );
    }
    let rebuilt = TypedPatriciaTrie::from_entries(
        prof_w,
        &limits,
        coords
            .iter()
            .map(|&c| {
                (
                    CoordinateKeyV1::from_coord(c).as_bytes().to_vec(),
                    ON_V1_VALUE_BYTES.to_vec(),
                )
            })
            .collect(),
    )
    .unwrap();
    assert_eq!(forward.root(), reverse.root());
    assert_eq!(forward.root(), rebuilt.root());

    // Verify presence and absence claims for depth-192 twin leaves and non-members.
    let ctx_w = ExpectedProofContextV1::for_profile(&prof_w, forward.root());
    for c in coords {
        let key = CoordinateKeyV1::from_coord(c);
        let proof = forward.prove(key.as_bytes()).unwrap();
        verify_claim(
            &profiles,
            &ctx_w,
            key.as_bytes(),
            &LookupResult::Present(ON_V1_VALUE_BYTES.to_vec()),
            &proof.to_canonical_bytes(),
            None,
        )
        .unwrap();
    }
    let absent_key = CoordinateKeyV1::from_coord(Coord::new(0, 0, 2));
    let absent_proof = forward.prove(absent_key.as_bytes()).unwrap();
    verify_claim(
        &profiles,
        &ctx_w,
        absent_key.as_bytes(),
        &LookupResult::Absent,
        &absent_proof.to_canonical_bytes(),
        None,
    )
    .unwrap();

    // Deleting one twin collapses the depth-191 branch cleanly and matches cold rebuild.
    assert!(
        forward
            .delete(CoordinateKeyV1::from_coord(Coord::new(0, 0, 1)).as_bytes())
            .unwrap()
    );
    let rebuilt_after = TypedPatriciaTrie::from_entries(
        prof_w,
        &limits,
        coords
            .iter()
            .filter(|&&c| c != Coord::new(0, 0, 1))
            .map(|&c| {
                (
                    CoordinateKeyV1::from_coord(c).as_bytes().to_vec(),
                    ON_V1_VALUE_BYTES.to_vec(),
                )
            })
            .collect(),
    )
    .unwrap();
    assert_eq!(forward.root(), rebuilt_after.root());

    // 256-bit EpisodeRootV1 twin keys differing ONLY at bit 255 (depth-256 zero-bit-suffix leaves!)
    let prof_e = CommitmentProfileDescriptorV1::episode_v1();
    let mut k0 = [0x5Au8; 32];
    k0[31] = 0xFE;
    let mut k1 = [0x5Au8; 32];
    k1[31] = 0xFF;
    let v0 = EpisodeRecordV1 {
        coord: Coord::new(1, 0, 0),
        mint: MintIdV1::from_bytes(k0),
        consuming_burn: None,
    }
    .encode_canonical();
    let v1 = EpisodeRecordV1 {
        coord: Coord::new(2, 0, 0),
        mint: MintIdV1::from_bytes(k1),
        consuming_burn: Some(EventIdV1::from_bytes([0xCC; 32])),
    }
    .encode_canonical();
    let mut ep_trie = TypedPatriciaTrie::empty(prof_e).unwrap();
    ep_trie.upsert(k0.to_vec(), v0.clone()).unwrap();
    ep_trie.upsert(k1.to_vec(), v1.clone()).unwrap();
    let ctx_e = ExpectedProofContextV1::for_profile(&prof_e, ep_trie.root());
    for (k, v) in [(&k0, &v0), (&k1, &v1)] {
        let bytes = ep_trie.prove(k).unwrap().to_canonical_bytes();
        verify_claim(
            &profiles,
            &ctx_e,
            k,
            &LookupResult::Present(v.clone()),
            &bytes,
            None,
        )
        .unwrap();
    }
    let miss_k = [0x5Au8; 32]; // byte 31 = 0x5A != 0xFE/0xFF -> divergent extension
    let miss_bytes = ep_trie.prove(&miss_k).unwrap().to_canonical_bytes();
    verify_claim(
        &profiles,
        &ctx_e,
        &miss_k,
        &LookupResult::Absent,
        &miss_bytes,
        None,
    )
    .unwrap();
}

#[test]
fn adversarial_proof_domain_codec_and_policy_controls_reject_fail_closed() {
    let profiles = ProfileRegistryV1::local_v1();
    let prof_w = CommitmentProfileDescriptorV1::world_v1();
    let prof_a = CommitmentProfileDescriptorV1::active_token_v1();
    let prof_e = CommitmentProfileDescriptorV1::episode_v1();

    let mut wt = TypedPatriciaTrie::empty(prof_w).unwrap();
    let c1 = Coord::new(3, 4, 5);
    let c2 = Coord::new(-3, 4, 5);
    let k1 = CoordinateKeyV1::from_coord(c1);
    let k2 = CoordinateKeyV1::from_coord(c2);
    wt.upsert(k1.as_bytes().to_vec(), ON_V1_VALUE_BYTES.to_vec())
        .unwrap();
    wt.upsert(k2.as_bytes().to_vec(), ON_V1_VALUE_BYTES.to_vec())
        .unwrap();

    let ctx_w = ExpectedProofContextV1::for_profile(&prof_w, wt.root());
    let valid_bytes = wt.prove(k1.as_bytes()).unwrap().to_canonical_bytes();

    // 1. Cross-domain / cross-profile substitution rejects
    let ctx_a_wrong = ExpectedProofContextV1::for_profile(&prof_a, wt.root());
    assert!(verify_lookup(&profiles, &ctx_a_wrong, k1.as_bytes(), &valid_bytes, None).is_err());

    // 2. Internally inconsistent ExpectedProofContextV1 rejects before proof work
    let mut bad_ctx = ctx_w;
    bad_ctx.domain_tag = DomainTagV1::ActiveToken;
    assert!(matches!(
        verify_lookup(&profiles, &bad_ctx, k1.as_bytes(), &valid_bytes, None),
        Err(CommitStateError::RegistryMismatch { .. })
    ));

    // 3. Stale version rejects
    let mut bad_ver = valid_bytes.clone();
    bad_ver[1] = 2;
    assert!(matches!(
        verify_lookup(&profiles, &ctx_w, k1.as_bytes(), &bad_ver, None),
        Err(CommitStateError::WrongVersion { .. })
    ));

    // 4. Trailing byte rejects
    let mut trailing = valid_bytes.clone();
    trailing.push(0x00);
    assert!(matches!(
        verify_lookup(&profiles, &ctx_w, k1.as_bytes(), &trailing, None),
        Err(CommitStateError::MalformedEncoding { .. })
    ));

    // 5. ClaimMismatch when caller expects Absent on a valid presence proof
    assert!(matches!(
        verify_claim(
            &profiles,
            &ctx_w,
            k1.as_bytes(),
            &LookupResult::Absent,
            &valid_bytes,
            None,
        ),
        Err(CommitStateError::ClaimMismatch)
    ));

    // 6. Widening local cap rejects; tightening below actual work rejects over-bound proof
    let mut widened = ProofPolicyDescriptorV1::WORLD_DEFAULT;
    widened.max_hash_invocations += 1;
    assert!(matches!(
        verify_lookup(
            &profiles,
            &ctx_w,
            k1.as_bytes(),
            &valid_bytes,
            Some(widened)
        ),
        Err(CommitStateError::LocalCapWidensPolicy { .. })
    ));
    let mut too_small_bytes = ProofPolicyDescriptorV1::WORLD_DEFAULT;
    too_small_bytes.max_proof_bytes = (valid_bytes.len() - 1) as u32;
    assert!(matches!(
        verify_lookup(
            &profiles,
            &ctx_w,
            k1.as_bytes(),
            &valid_bytes,
            Some(too_small_bytes)
        ),
        Err(CommitStateError::LimitExceeded {
            field: "max_proof_bytes"
        })
    ));
    let mut too_few_hashes = ProofPolicyDescriptorV1::WORLD_DEFAULT;
    too_few_hashes.max_hash_invocations = 1;
    assert!(matches!(
        verify_lookup(
            &profiles,
            &ctx_w,
            k1.as_bytes(),
            &valid_bytes,
            Some(too_few_hashes)
        ),
        Err(CommitStateError::LimitExceeded {
            field: "max_hash_invocations"
        })
    ));

    // 7. Bedrock forbidden in ActiveTokenRootV1 and EpisodeRecordV1; burn==mint forbidden
    let mut at = TypedPatriciaTrie::empty(prof_a).unwrap();
    assert!(matches!(
        at.upsert(
            CoordinateKeyV1::from_coord(BEDROCK_COORD)
                .as_bytes()
                .to_vec(),
            vec![0x11; 32]
        ),
        Err(CommitStateError::InvalidKeyOrValue { .. })
    ));
    let mut et = TypedPatriciaTrie::empty(prof_e).unwrap();
    let bad_bedrock_ep = EpisodeRecordV1 {
        coord: BEDROCK_COORD,
        mint: MintIdV1::from_bytes([0x22; 32]),
        consuming_burn: None,
    }
    .encode_canonical();
    assert!(matches!(
        et.upsert(vec![0x22; 32], bad_bedrock_ep),
        Err(CommitStateError::InvalidKeyOrValue { .. })
    ));
    let bad_self_burn = EpisodeRecordV1 {
        coord: Coord::new(1, 2, 3),
        mint: MintIdV1::from_bytes([0x33; 32]),
        consuming_burn: Some(EventIdV1::from_bytes([0x33; 32])),
    }
    .encode_canonical();
    assert!(matches!(
        et.upsert(vec![0x33; 32], bad_self_burn),
        Err(CommitStateError::InvalidKeyOrValue { .. })
    ));

    // 8. Non-empty steps on EmptyRoot terminal rejects
    let empty_ctx = ExpectedProofContextV1::for_profile(&prof_w, typed_empty_root(&prof_w));
    let mut forged_empty = valid_bytes.clone();
    // Truncate right after first step and append TERM_EMPTY (0x00), setting step_count = 1
    let step0_end = 4 + 1 + 2 + 1 + 32;
    forged_empty.truncate(step0_end);
    forged_empty[2] = 0;
    forged_empty[3] = 1;
    forged_empty.push(0);
    assert!(verify_lookup(&profiles, &empty_ctx, k1.as_bytes(), &forged_empty, None).is_err());

    // 9. Typed root tags stay strictly distinct from legacy world_tag
    let mut w = World::new();
    w.set_on(c1);
    let legacy = world_tag(&w);
    let typed = WorldRootV1 {
        root: wt.root(),
        profile_id: prof_w.id(),
    }
    .tag();
    let ar_tag = ActiveTokenRootV1 {
        root: at.root(),
        profile_id: prof_a.id(),
    }
    .tag();
    let er_tag = EpisodeRootV1 {
        root: et.root(),
        profile_id: prof_e.id(),
    }
    .tag();
    assert_ne!(legacy, typed);
    assert_ne!(typed, ar_tag);
    assert_ne!(ar_tag, er_tag);
}

#[test]
fn logical_episode_pruning_changes_only_episode_root_and_matches_independent_projection() {
    let initial_world = World::new();
    let mut world = initial_world.clone();
    let mut history = History::new(&initial_world);
    let mut episodes = OccupancyEpisodeLedger::from_history(&initial_world, &history).unwrap();
    let storage_profile = LocalStorageProfile::DEFAULT;

    let mut sidecar = LocalAuthenticatedSidecar::from_genesis(
        &initial_world,
        &history,
        &episodes,
        &storage_profile,
        scope_digest(),
    )
    .unwrap();

    let c1 = Coord::new(10, 20, 30);
    let mut tx1 = history.begin_tick(1);
    tx1.apply_edit(&history, &mut world, Edit::new(Operation::On, c1));
    let r1 = history.commit_tick(tx1, &world).unwrap().clone();
    episodes.apply_receipt(&r1).unwrap();
    let _ = sidecar
        .advance_for_receipt(&initial_world, &world, &history, &episodes, &r1)
        .unwrap();
    let mint1 = episodes.get_active(c1).unwrap().mint();

    let mut tx2 = history.begin_tick(2);
    tx2.apply_edit(&history, &mut world, Edit::new(Operation::Off, c1));
    let r2 = history.commit_tick(tx2, &world).unwrap().clone();
    episodes.apply_receipt(&r2).unwrap();
    let _ = sidecar
        .advance_for_receipt(&initial_world, &world, &history, &episodes, &r2)
        .unwrap();

    let w_before = sidecar.world_root();
    let a_before = sidecar.active_token_root();
    let e_before = sidecar.episode_root();

    // Logically prune closed mint1 from retained episodes.
    assert_eq!(episodes.prune_closed_provenance(1, &HashSet::new()), 1);
    let pruned_set = sidecar
        .advance_for_logical_episode_prune(&initial_world, &world, &history, &episodes, &[mint1])
        .unwrap();

    assert_eq!(sidecar.world_root(), w_before);
    assert_eq!(sidecar.active_token_root(), a_before);
    assert_ne!(sidecar.episode_root(), e_before);

    // Independent projection without knowing mint1 was pruned must disagree; with pruned set it passes!
    assert!(
        verify_independent_projection(
            &initial_world,
            &history,
            &HashSet::new(),
            &pruned_set,
            &ProfileRegistryV1::local_v1(),
            &DomainRegistryV1::local_v1(),
            HistoryReplayLimits::DEFAULT,
        )
        .is_err()
    );
    let mut pruned = HashSet::new();
    pruned.insert(mint1);
    let projected = verify_independent_projection(
        &initial_world,
        &history,
        &pruned,
        &pruned_set,
        &ProfileRegistryV1::local_v1(),
        &DomainRegistryV1::local_v1(),
        HistoryReplayLimits::DEFAULT,
    )
    .unwrap();
    assert_eq!(projected.episode_root, sidecar.episode_root());
}

#[test]
fn memory_store_gc_epoch_barrier_failed_cas_and_corruption_controls() {
    let initial_world = World::new();
    let mut world = initial_world.clone();
    let mut history = History::new(&initial_world);
    let mut episodes = OccupancyEpisodeLedger::from_history(&initial_world, &history).unwrap();
    let profile = LocalStorageProfile::DEFAULT;
    let mut sidecar = LocalAuthenticatedSidecar::from_genesis(
        &initial_world,
        &history,
        &episodes,
        &profile,
        scope_digest(),
    )
    .unwrap();

    let mut store = MemoryCommitmentStore::new();
    let s0_nodes = sidecar.collect_all_node_records();
    let s0_id = store
        .publish_with_crash_point(None, sidecar.current_set(), &s0_nodes, CrashPoint::None)
        .unwrap();

    // Advance tick 1 and tick 2.
    let mut tx1 = history.begin_tick(1);
    tx1.apply_edit(
        &history,
        &mut world,
        Edit::new(Operation::On, Coord::new(1, 0, 0)),
    );
    let r1 = history.commit_tick(tx1, &world).unwrap().clone();
    episodes.apply_receipt(&r1).unwrap();
    let s1 = sidecar
        .advance_for_receipt(&initial_world, &world, &history, &episodes, &r1)
        .unwrap();
    let s1_nodes = sidecar.collect_all_node_records();
    let s1_id = store
        .publish_with_crash_point(Some(s0_id), &s1, &s1_nodes, CrashPoint::None)
        .unwrap();

    // Failed head CAS using stale expected prior leaves head unchanged at s1_id.
    let mut tx2 = history.begin_tick(2);
    tx2.apply_edit(
        &history,
        &mut world,
        Edit::new(Operation::On, Coord::new(2, 0, 0)),
    );
    let r2 = history.commit_tick(tx2, &world).unwrap().clone();
    episodes.apply_receipt(&r2).unwrap();
    let s2 = sidecar
        .advance_for_receipt(&initial_world, &world, &history, &episodes, &r2)
        .unwrap();
    let s2_nodes = sidecar.collect_all_node_records();
    let stale_s2 = CommitmentSetV1 {
        predecessor: CommitmentSetPredecessorV1::OrdinaryCommitmentSuccessorV1 {
            prior_commitment_set_id: s0_id,
        },
        subject: CommitmentSubjectV1::LocalTransitionV1 {
            local_storage_profile_id: profile.tag().digest(),
            scope_digest: scope_digest(),
            domain_registry_digest: DomainRegistryV1::local_v1().digest(),
            ledger_sequence: 1,
        },
        ..s2.clone()
    };
    assert!(matches!(
        store.publish_with_crash_point(Some(s0_id), &stale_s2, &s2_nodes, CrashPoint::None),
        Err(CommitStateError::HeadCasConflict { .. })
    ));
    assert_eq!(store.head_set_id().unwrap(), Some(s1_id));
    let rec = store.recover().unwrap();
    assert_eq!(rec.recovered_head, Some(s1_id));
    assert_eq!(rec.released_provisional_pins, 1);

    // Concurrent GC write-barrier race: mark at epoch E, then publish s2 (bumping epoch to E+1) -> sweep MUST abort!
    let mark_before = store.begin_gc_mark().unwrap();
    let s2_id = store
        .publish_with_crash_point(Some(s1_id), &s2, &s2_nodes, CrashPoint::None)
        .unwrap();
    assert!(matches!(
        store.finish_gc_sweep(&mark_before),
        GcSweepOutcome::AbortedEpochChanged { .. }
    ));

    // Unpin intermediate s1_id while pinning genesis s0_id; fresh mark+sweep reclaims s1-only orphans safely.
    store.pin_permanent(s0_id).unwrap();
    assert!(store.unpin_permanent(&s1_id).unwrap());
    let before_count = store.node_count();
    let clean_mark = store.begin_gc_mark().unwrap();
    match store.finish_gc_sweep(&clean_mark) {
        GcSweepOutcome::Completed {
            reclaimed_nodes,
            remaining_nodes,
        } => {
            assert!(reclaimed_nodes > 0);
            assert_eq!(remaining_nodes + reclaimed_nodes, before_count);
        }
        other => panic!("expected completed sweep, got {other:?}"),
    }
    // Both pinned genesis s0 and active head s2 still pass full closure verification!
    assert_eq!(store.recover().unwrap().recovered_head, Some(s2_id));

    // Corrupting a live reachable node must fail closed during recovery (never silently cache-miss).
    let live_key: NodeStoreKey = *s2_nodes.keys().next().unwrap();
    assert!(store.corrupt_node_for_test(&live_key));
    assert!(matches!(
        store.recover(),
        Err(CommitStateError::CorruptOrMissingNode { .. })
    ));
}

#[test]
fn durable_filesystem_crash_recovery_gauntlet_across_all_five_crash_points() {
    let crash_points = [
        CrashPoint::AfterNodeStaging,
        CrashPoint::AfterProvisionalPin,
        CrashPoint::AfterManifestWrite,
        CrashPoint::AfterHeadCas,
        CrashPoint::AfterPinPromotion,
    ];
    let profile = LocalStorageProfile::DEFAULT;
    let initial_world = World::new();

    for (idx, &cp) in crash_points.iter().enumerate() {
        let dir = unique_temp_dir(&format!("crash_{idx}"));
        let journal_path = dir.join("ledger.bbj");
        let store_dir = dir.join("commit_store");

        let (mut dal, _create, genesis_id) = DurableAuthenticatedLedger::create_new(
            &journal_path,
            &store_dir,
            &profile,
            scope_digest(),
            &initial_world,
        )
        .unwrap();
        assert_eq!(dal.store().head_set_id().unwrap(), Some(genesis_id));
        assert_eq!(*dal.storage_profile(), profile);

        // Exclusive lock conflict check while store is open.
        assert!(DurableCommitmentStore::open_and_recover(&store_dir).is_err());

        // Inject deterministic crash during tick 1 publication.
        let res = dal.append_tick_with_crash_point(
            1,
            &[
                Edit::new(Operation::On, Coord::new(idx as i64 + 1, -2, 5)),
                Edit::new(Operation::On, Coord::new(idx as i64 + 2, 7, -9)),
            ],
            cp,
        );
        assert!(matches!(res, Err(CommitStateError::SimulatedCrash(p)) if p == cp));
        let expected_set = dal.sidecar().current_set().clone();
        drop(dal);

        // Reopen and recover: must recover complete consistent tuple matching verified ledger replay.
        let (mut recovered, _scan, _report) = DurableAuthenticatedLedger::open_and_recover(
            &journal_path,
            &store_dir,
            &profile,
            scope_digest(),
            AnchorPolicy::UnanchoredLocal,
            &initial_world,
        )
        .unwrap();
        assert_eq!(recovered.sidecar().current_set(), &expected_set);
        assert_eq!(
            recovered.store().head_set_id().unwrap(),
            Some(expected_set.id())
        );

        // Subsequent tick 2 commits cleanly after recovery.
        let (_, set2_id) = recovered
            .append_tick(
                2,
                &[Edit::new(Operation::Off, Coord::new(idx as i64 + 1, -2, 5))],
            )
            .unwrap()
            .expect("tick 2 committed");
        assert_eq!(recovered.store().head_set_id().unwrap(), Some(set2_id));
        assert!(recovered.store().persisted_node_count().unwrap() > 0);
        let sample_key = *recovered
            .sidecar()
            .collect_all_node_records()
            .keys()
            .next()
            .unwrap();
        assert!(
            !recovered
                .store()
                .read_verified_node(&sample_key)
                .unwrap()
                .is_empty()
        );

        // Test on-disk node corruption fails closed on reopen!
        let corrupt_path = recovered.store().node_file_path(&sample_key);
        drop(recovered);
        let mut raw = fs::read(&corrupt_path).unwrap();
        raw[0] ^= 0xFF;
        fs::write(&corrupt_path, raw).unwrap();
        assert!(matches!(
            DurableCommitmentStore::open_and_recover(&store_dir),
            Err(CommitStateError::CorruptOrMissingNode { .. })
        ));

        let _ = fs::remove_dir_all(&dir);
    }
}

#[test]
fn durable_pack_compaction_gc_and_torn_tail_recovery() {
    let dir = unique_temp_dir("pack_gc_tail");
    let journal_path = dir.join("ledger.bbj");
    let store_dir = dir.join("commit_store");
    let profile = LocalStorageProfile::DEFAULT;
    let initial_world = World::new();

    let (mut dal, _create, genesis_id) = DurableAuthenticatedLedger::create_new(
        &journal_path,
        &store_dir,
        &profile,
        scope_digest(),
        &initial_world,
    )
    .unwrap();

    let mut intermediate_ids = Vec::new();
    for tick in 1..=8_u64 {
        let (_, set_id) = dal
            .append_tick(
                tick,
                &[Edit::new(Operation::On, Coord::new(tick as i64, 0, 0))],
            )
            .unwrap()
            .expect("tick committed");
        if tick < 8 {
            intermediate_ids.push(set_id);
        }
    }
    let head_id = dal.store().head_set_id().unwrap().unwrap();
    let pack_path = store_dir.join("nodes").join("nodes.pack");
    assert!(pack_path.exists(), "batched nodes.pack must exist");
    let size_before_gc = fs::metadata(&pack_path).unwrap().len();
    let count_before_gc = dal.store().persisted_node_count().unwrap();
    drop(dal);

    // 1. Simulate a torn mid-append tail in `nodes.pack` after a committed head -> recovery must
    //    verify the committed head closure intact and cleanly truncate the unreferenced torn tail.
    {
        use std::io::Write;
        let mut f = fs::OpenOptions::new()
            .append(true)
            .open(&pack_path)
            .unwrap();
        f.write_all(b"BBP1\xde\xad\xbe\xef_torn_tail").unwrap();
        f.sync_all().unwrap();
    }
    let (mut store, report) = DurableCommitmentStore::open_and_recover(&store_dir).unwrap();
    assert_eq!(report.recovered_head, Some(head_id));
    assert_eq!(fs::metadata(&pack_path).unwrap().len(), size_before_gc);
    assert_eq!(store.persisted_node_count().unwrap(), count_before_gc);

    // 2. Unpin intermediate sets, mark+sweep compacting `nodes.pack`, and verify both genesis and
    //    head remain intact on reopen while pack file shrinks.
    for id in &intermediate_ids {
        assert!(store.unpin_permanent(id).unwrap());
    }
    let mark = store.begin_gc_mark().unwrap();
    match store.finish_gc_sweep(&mark).unwrap() {
        GcSweepOutcome::Completed {
            reclaimed_nodes,
            remaining_nodes,
        } => {
            assert!(reclaimed_nodes > 0, "expected orphan pack nodes reclaimed");
            assert_eq!(remaining_nodes + reclaimed_nodes, count_before_gc);
        }
        other => panic!("expected compacting sweep completion, got {other:?}"),
    }
    let size_after_gc = fs::metadata(&pack_path).unwrap().len();
    assert!(
        size_after_gc < size_before_gc,
        "compacted pack ({size_after_gc} B) must be smaller than pre-GC pack ({size_before_gc} B)"
    );
    assert_eq!(store.read_manifest(&genesis_id).unwrap().id(), genesis_id);
    drop(store);

    let (reopened, _scan, rec_report) = DurableAuthenticatedLedger::open_and_recover(
        &journal_path,
        &store_dir,
        &profile,
        scope_digest(),
        AnchorPolicy::UnanchoredLocal,
        &initial_world,
    )
    .unwrap();
    assert_eq!(rec_report.recovered_head, Some(head_id));
    assert_eq!(reopened.ledger().history().receipts().len(), 8);

    let _ = fs::remove_dir_all(&dir);
}
