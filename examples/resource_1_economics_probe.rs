//! Release economics simulation probe for `RESOURCE-1` (`PS-039`), exporting deterministic
//! EIP-1559 burst/recovery trajectories, `BOUNDS-1` gas-share breakdown, conservation checks, and
//! attacker capital lockup curves as JSON.

use std::collections::BTreeMap;

use bitblock::asset_d1::AssetPolicyDescriptorV1;
use bitblock::bounds_1::ProtocolBoundsProfileV1;
use bitblock::identity_d1::ActorIdV1;
use bitblock::local_bounds::LocalStorageProfile;
use bitblock::profile_bootstrap::ProfileScopeV1;
use bitblock::resource_1::{
    ResourceAccountingLedgerV1, ResourcePolicyV1, TxResourceRequestV1, TxResourceWorkV1,
};
use bitblock::threat_1::ThreatModelV1;
use bitblock::world::Coord;

fn canonical_policy() -> ResourcePolicyV1 {
    let dom = bitblock::commit_state::DomainRegistryV1::local_v1();
    let mut ckeys = [31u8, 32, 33]
        .into_iter()
        .map(|s| {
            bitblock::crypto_d1::CanonicalEd25519PublicKeyV1::from_secret_seed(&[s; 32]).unwrap()
        })
        .collect::<Vec<_>>();
    ckeys.sort();
    let council = bitblock::identity_d1::ThresholdPolicyV1 {
        role: bitblock::identity_d1::CredentialRoleV1::EmergencyPauseSigner,
        threshold: 2,
        keys: ckeys,
    };
    let gov = bitblock::consensus_gov_d1::GovernancePolicyDescriptorV1 {
        schema_version: 1,
        upgrade_quorum_numerator: 2,
        upgrade_quorum_denominator: 3,
        mandatory_exit_timelock_epochs: 21,
        max_emergency_pause_epochs: 3,
        automatic_safe_halt_on_proven_fault: true,
        emergency_council_pause_only: true,
        forbid_history_rewrite: true,
        emergency_council: council,
    };
    let scope = ProfileScopeV1::from_wave1_contracts(
        &dom,
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
    let bounds =
        ProtocolBoundsProfileV1::v1_consumer_default(&threat, &LocalStorageProfile::DEFAULT)
            .unwrap();
    let asset = AssetPolicyDescriptorV1::v1_dual_domain_default();
    ResourcePolicyV1::v1_default(&bounds, &asset).unwrap()
}

fn main() {
    let policy = canonical_policy();
    let pol_tag = policy.tag().unwrap();

    // 1. 64-block EIP-1559 simulation: 24 blocks 100% spam burst, 8 blocks 50% target, 32 blocks 10% quiet.
    let mut bf = 100u128;
    let mut eip_series = Vec::with_capacity(65);
    eip_series.push((0u64, 0u64, bf));
    for h in 1..=64u64 {
        let gas = if h <= 24 {
            100_000_000
        } else if h <= 32 {
            50_000_000
        } else {
            10_000_000
        };
        bf = policy.next_base_fee(bf, gas).unwrap();
        eip_series.push((h, gas, bf));
    }

    // 2. Worst-case component gas shares vs BOUNDS-1 ceilings.
    let sig_gas = 2_048u64 * u64::from(policy.gas_per_ed25519_sig);
    let write_gas = 4_096u64 * u64::from(policy.gas_per_state_write);
    let proof_gas = 64u64 * 12_288u64 * u64::from(policy.gas_per_proof_byte);
    let ops_gas = 16_384u64 * u64::from(policy.gas_per_occupancy_op);
    let bytes_gas = (2u64 * 1024 * 1024) * u64::from(policy.gas_per_tx_byte);

    // 3. 16-block storage-spam attacker capital lockup + burn trajectory (100% write-heavy blocks).
    let attacker = ActorIdV1::from_bytes([0xAA; 32]);
    let proposer = ActorIdV1::from_bytes([0x99; 32]);
    let initial_supply = 1_000_000_000_000_000u128;
    let mut ledger = ResourceAccountingLedgerV1::new(
        BTreeMap::from([(attacker, initial_supply), (proposer, 0u128)]),
        100,
    )
    .unwrap();
    let mut spam_rows = Vec::new();
    let mut next_x = 1i64;
    for h in 1..=16u64 {
        // Pack 40 txs x 100 writes = 4,000 writes (~54M gas + sigs/bytes -> ~98M gas)
        let mut reqs = Vec::new();
        for _ in 0..40 {
            let mut coords = Vec::with_capacity(100);
            for _ in 0..100 {
                coords.push(Coord::new(next_x, 1, 0));
                next_x += 1;
            }
            reqs.push(TxResourceRequestV1 {
                payer: attacker,
                max_fee_per_gas: 100_000_000,
                max_priority_tip_per_gas: 5,
                work: TxResourceWorkV1 {
                    tx_bytes: 2_048,
                    ed25519_signatures: 1,
                    state_writes: 100,
                    proof_bytes: 512,
                    occupancy_ops: 100,
                },
                execution_succeeded: true,
                minted_coords: coords,
                burned_coords: vec![],
            });
        }
        let s = ledger.settle_block(&policy, h, proposer, &reqs).unwrap();
        let locked_total: u128 = ledger.locked_deposits.values().map(|&(_, v)| v).sum();
        let prop_bal = *ledger.balances.get(&proposer).unwrap_or(&0);
        spam_rows.push((
            h,
            s.base_fee_per_gas,
            s.block_gas_used,
            ledger.total_burned_fees,
            locked_total,
            prop_bal,
        ));
    }

    println!("{{");
    println!("  \"resource_policy_tag\": \"{pol_tag}\",");
    println!("  \"target_block_gas\": {},", policy.target_block_gas);
    println!("  \"max_block_gas\": {},", policy.max_block_gas);
    println!(
        "  \"component_ceilings_gas\": {{\"ed25519_2048\": {sig_gas}, \"state_writes_4096\": {write_gas}, \"proof_bytes_768k\": {proof_gas}, \"occupancy_ops_16384\": {ops_gas}, \"block_bytes_2m\": {bytes_gas}}},"
    );
    println!("  \"eip1559_trajectory\": [");
    for (i, (h, g, b)) in eip_series.iter().enumerate() {
        let comma = if i + 1 == eip_series.len() { "" } else { "," };
        println!("    {{\"block\": {h}, \"gas_used\": {g}, \"base_fee\": {b}}}{comma}");
    }
    println!("  ],");
    println!("  \"spam_attack_trajectory\": [");
    for (i, (h, bf_h, g, burned, locked, tips)) in spam_rows.iter().enumerate() {
        let comma = if i + 1 == spam_rows.len() { "" } else { "," };
        println!(
            "    {{\"block\": {h}, \"base_fee\": {bf_h}, \"gas_used\": {g}, \"cum_burned\": {burned}, \"cum_locked_deposits\": {locked}, \"cum_proposer_tips\": {tips}}}{comma}"
        );
    }
    println!("  ]");
    println!("}}");
}
