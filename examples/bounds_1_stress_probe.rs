//! Release-mode worst-case block admission stress benchmark for `BOUNDS-1`.
//!
//! Measures real host wall-clock timings for every worst-case block admission ceiling frozen in
//! `ProtocolBoundsProfileV1`:
//! 1. Streaming pre-allocation preflight (`10,000` calls).
//! 2. `2,048` strict RFC 8032 Ed25519 signature verifications (`verify_signed_preimage`).
//! 3. `4,096` `COMMIT-1` `WorldRootV1` Patricia state writes & root recomputation.
//! 4. `64` context-bound `TypedPatriciaProofV1` lookup verifications.
//!
//! Emits structured JSON when `--json` is passed so benchmark charts and `BOUNDS-1` receipts bind
//! directly to exact release-mode measurements against the `400 ms` verification ceiling.
//!
//! ```text
//! cargo run --release --features protocol-research --example bounds_1_stress_probe -- --json
//! ```

use std::env;
use std::time::Instant;

use bitblock::bounds_1::{
    BlockEnvelopePreflightHeaderV1, DEFAULT_MAX_BLOCK_BYTES, DEFAULT_MAX_BLOCK_OPS,
    DEFAULT_MAX_BLOCK_SIGNATURES, DEFAULT_MAX_BLOCK_TXS, DEFAULT_MAX_PROOF_BYTES,
    DEFAULT_MAX_PROOF_DEPTH, DEFAULT_MAX_STATE_WRITES_PER_BLOCK, DEFAULT_MAX_TX_BYTES,
    DEFAULT_MAX_VERIFY_BUDGET_MS, LocalVmBoundsV1, ProtocolBoundsProfileV1,
};
use bitblock::commit_r1::CoordinateKeyV1;
use bitblock::commit_state::{
    CommitmentOperationalLimitsDescriptorV1, CommitmentProfileDescriptorV1, DomainRegistryV1,
    ExpectedProofContextV1, LookupResult, ON_V1_VALUE_BYTES, ProfileRegistryV1, TypedPatriciaTrie,
    verify_lookup,
};
use bitblock::consensus_gov_d1::GovernancePolicyDescriptorV1;
use bitblock::crypto_d1::{
    CanonicalEd25519PublicKeyV1, HashSuiteIdV1, SignatureSuiteIdV1, SigningDomainTagV1,
    SigningPreimageV1, sign_preimage_deterministic, verify_preimage_strict,
};
use bitblock::identity_d1::{CredentialRoleV1, ThresholdPolicyV1};
use bitblock::local_bounds::LocalStorageProfile;
use bitblock::profile_bootstrap::ProfileScopeV1;
use bitblock::threat_1::ThreatModelV1;
use bitblock::vm_d1::VmSpecD1;
use bitblock::world::Coord;

fn median(mut samples: Vec<f64>) -> f64 {
    samples.sort_by(|a, b| a.total_cmp(b));
    samples[samples.len() / 2]
}

fn max_val(samples: &[f64]) -> f64 {
    samples.iter().copied().fold(0.0_f64, f64::max)
}

fn main() {
    let json_mode = env::args().any(|a| a == "--json");

    let mut keys = [31u8, 32, 33]
        .into_iter()
        .map(|s| CanonicalEd25519PublicKeyV1::from_secret_seed(&[s; 32]).unwrap())
        .collect::<Vec<_>>();
    keys.sort();
    let gov = GovernancePolicyDescriptorV1 {
        emergency_council: ThresholdPolicyV1 {
            role: CredentialRoleV1::EmergencyPauseSigner,
            threshold: 2,
            keys,
        },
        schema_version: 1,
        upgrade_quorum_numerator: 2,
        upgrade_quorum_denominator: 3,
        mandatory_exit_timelock_epochs: 21,
        max_emergency_pause_epochs: 3,
        automatic_safe_halt_on_proven_fault: true,
        emergency_council_pause_only: true,
        forbid_history_rewrite: true,
    };
    let scope = ProfileScopeV1::from_wave1_contracts(
        &DomainRegistryV1::local_v1(),
        &gov,
        vec![
            "asset-active".to_string(),
            "economic-charging".to_string(),
            "stake-consensus".to_string(),
            "validator-economics".to_string(),
        ],
    )
    .unwrap();
    let threat = ThreatModelV1::v1_conservative_consumer(&scope).unwrap();
    let profile =
        ProtocolBoundsProfileV1::v1_consumer_default(&threat, &LocalStorageProfile::DEFAULT)
            .unwrap();
    let vm_bounds = LocalVmBoundsV1::v1_default(&VmSpecD1::v1_default()).unwrap();

    // 1. Streaming preflight latency (10,000 worst-case valid headers)
    let worst_valid = BlockEnvelopePreflightHeaderV1 {
        block_bytes: u64::from(DEFAULT_MAX_BLOCK_BYTES),
        tx_count: u64::from(DEFAULT_MAX_BLOCK_TXS),
        max_tx_bytes_observed: u64::from(DEFAULT_MAX_TX_BYTES),
        signature_count: u64::from(DEFAULT_MAX_BLOCK_SIGNATURES),
        op_count: u64::from(DEFAULT_MAX_BLOCK_OPS),
        state_writes: u64::from(DEFAULT_MAX_STATE_WRITES_PER_BLOCK),
        max_proof_bytes_observed: u64::from(DEFAULT_MAX_PROOF_BYTES),
        max_proof_depth_observed: u64::from(DEFAULT_MAX_PROOF_DEPTH),
        estimated_hashes: 1_000_000,
        estimated_alloc_bytes: 16 * 1024 * 1024,
    };
    let t_pre = Instant::now();
    for _ in 0..10_000 {
        profile.preflight_block_envelope(&worst_valid).unwrap();
    }
    let preflight_10k_ms = t_pre.elapsed().as_secs_f64() * 1000.0;

    // 2. 2,048 strict RFC 8032 Ed25519 verifications (7 samples)
    let seed = [7u8; 32];
    let pk = CanonicalEd25519PublicKeyV1::from_secret_seed(&seed).unwrap();
    let mut preimages = Vec::with_capacity(DEFAULT_MAX_BLOCK_SIGNATURES as usize);
    let mut sigs = Vec::with_capacity(DEFAULT_MAX_BLOCK_SIGNATURES as usize);
    for i in 0..DEFAULT_MAX_BLOCK_SIGNATURES {
        let mut digest = [0u8; 32];
        digest[0..4].copy_from_slice(&i.to_be_bytes());
        let pre = SigningPreimageV1 {
            protocol_version: 1,
            chain_id: 1,
            genesis_id: [0x11; 32],
            launch_manifest_id: [0x22; 32],
            profile_id: [0x33; 32],
            domain_tag: SigningDomainTagV1::ActorTransaction,
            signature_suite: SignatureSuiteIdV1::Ed25519StrictRfc8032V1,
            hash_suite: HashSuiteIdV1::Sha256V1,
            payload_digest: digest,
        };
        let sig = sign_preimage_deterministic(&seed, &pre).unwrap();
        preimages.push(pre);
        sigs.push(sig);
    }
    let mut ed_samples = Vec::with_capacity(7);
    for _ in 0..7 {
        let t0 = Instant::now();
        for (pre, sig) in preimages.iter().zip(sigs.iter()) {
            verify_preimage_strict(&pk, pre, sig).unwrap();
        }
        ed_samples.push(t0.elapsed().as_secs_f64() * 1000.0);
    }

    // 3. 4,096 COMMIT-1 Patricia WorldRootV1 state writes (7 samples)
    let prof_w = CommitmentProfileDescriptorV1::world_v1();
    let op_limits = CommitmentOperationalLimitsDescriptorV1::LOCAL_DEFAULT;
    let profiles = ProfileRegistryV1::local_v1();
    let entries: Vec<(Vec<u8>, Vec<u8>)> = (0..DEFAULT_MAX_STATE_WRITES_PER_BLOCK as i64)
        .map(|i| {
            let c = Coord::new((i * 17) - 32_000, (i * 31) % 2048, (i * 13) % 2048);
            (
                CoordinateKeyV1::from_coord(c).as_bytes().to_vec(),
                ON_V1_VALUE_BYTES.to_vec(),
            )
        })
        .collect();
    let mut pat_samples = Vec::with_capacity(7);
    let mut last_trie = None;
    for _ in 0..7 {
        let t0 = Instant::now();
        let trie = TypedPatriciaTrie::from_entries(prof_w, &op_limits, entries.clone()).unwrap();
        let _root = trie.root();
        pat_samples.push(t0.elapsed().as_secs_f64() * 1000.0);
        last_trie = Some(trie);
    }
    let last_trie = last_trie.unwrap();

    // 4. 64 context-bound TypedPatriciaProofV1 verifications (7 samples)
    let ctx = ExpectedProofContextV1::for_profile(&prof_w, last_trie.root());
    let proofs: Vec<(Vec<u8>, Vec<u8>)> = entries[..64]
        .iter()
        .map(|(k, _)| (k.clone(), last_trie.prove(k).unwrap().to_canonical_bytes()))
        .collect();
    let mut proof_samples = Vec::with_capacity(7);
    for _ in 0..7 {
        let t0 = Instant::now();
        for (key, proof_bytes) in &proofs {
            assert_eq!(
                verify_lookup(&profiles, &ctx, key, proof_bytes, None).unwrap(),
                LookupResult::Present(ON_V1_VALUE_BYTES.to_vec())
            );
        }
        proof_samples.push(t0.elapsed().as_secs_f64() * 1000.0);
    }

    let ed_p50 = median(ed_samples.clone());
    let ed_max = max_val(&ed_samples);
    let pat_p50 = median(pat_samples.clone());
    let pat_max = max_val(&pat_samples);
    let prf_p50 = median(proof_samples.clone());
    let prf_max = max_val(&proof_samples);
    let total_p50 = ed_p50 + pat_p50 + prf_p50 + (preflight_10k_ms / 10_000.0);
    let total_max = ed_max + pat_max + prf_max + (preflight_10k_ms / 10_000.0);
    // Conservative ~1 KiB total persisted node/journal overhead per sustained state write
    let annual_bytes = profile
        .projected_worst_case_annual_state_bytes(threat.target_block_time_ms, 1024)
        .unwrap();
    let annual_gib = annual_bytes as f64 / (1024.0 * 1024.0 * 1024.0);

    if json_mode {
        println!(
            "{{\n  \"protocol_bounds_tag\": \"{}\",\n  \"local_vm_bounds_tag\": \"{}\",\n  \"budget_ms\": {},\n  \"preflight_10k_ms\": {:.4},\n  \"ed25519_2048_p50_ms\": {:.3},\n  \"ed25519_2048_max_ms\": {:.3},\n  \"patricia_4096_p50_ms\": {:.3},\n  \"patricia_4096_max_ms\": {:.3},\n  \"proof_64_p50_ms\": {:.3},\n  \"proof_64_max_ms\": {:.3},\n  \"total_worst_case_p50_ms\": {:.3},\n  \"total_worst_case_max_ms\": {:.3},\n  \"within_400ms_budget\": {},\n  \"projected_annual_gib_at_1kib_per_write\": {:.2}\n}}",
            profile.tag().unwrap(),
            vm_bounds.tag().unwrap(),
            DEFAULT_MAX_VERIFY_BUDGET_MS,
            preflight_10k_ms,
            ed_p50,
            ed_max,
            pat_p50,
            pat_max,
            prf_p50,
            prf_max,
            total_p50,
            total_max,
            total_p50 <= f64::from(DEFAULT_MAX_VERIFY_BUDGET_MS),
            annual_gib,
        );
    } else {
        println!("BOUNDS-1 Worst-Case Stress Probe:");
        println!("  protocol_bounds_tag = {}", profile.tag().unwrap());
        println!("  local_vm_bounds_tag = {}", vm_bounds.tag().unwrap());
        println!("  2048x Ed25519 p50   = {ed_p50:.3} ms (max {ed_max:.3} ms)");
        println!("  4096x Patricia p50  = {pat_p50:.3} ms (max {pat_max:.3} ms)");
        println!("  64x Proof verify    = {prf_p50:.3} ms (max {prf_max:.3} ms)");
        println!(
            "  Total worst-case    = {total_p50:.3} ms / {DEFAULT_MAX_VERIFY_BUDGET_MS} ms budget"
        );
        println!("  Annual disk @1KiB/w = {annual_gib:.2} GiB/yr");
    }
}
