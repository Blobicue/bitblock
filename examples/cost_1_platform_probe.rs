//! Release measurement probe for `COST-1@base` (`PS-062`) covering the browser-game-platform
//! Bits creator economy (`BitsTestnetGrantV1`, `BitsPurchaseSplitV1`, `prove_wallet`,
//! `verify_bits_wallet_proof`) at 64 and 256 active accounts, plus all 6 powered negative controls.

#[cfg(not(feature = "protocol-research"))]
fn main() {
    eprintln!("Run with --features protocol-research");
    std::process::exit(1);
}

#[cfg(feature = "protocol-research")]
fn main() {
    use std::time::Instant;

    use bitblock::bits_economy_1::{
        BITS_DEFAULT_CREATOR_SPLIT_BPS, BitsEconomyStateV1, BitsGenesisConfigV1, BitsGrantKindV1,
        BitsPurchaseSplitV1, BitsSignedTxV1, BitsTestnetGrantV1, BitsTxPayloadV1,
        compute_purchase_memo_hash, derive_default_bits_actor_id, verify_bits_wallet_proof,
    };
    use bitblock::cost_1::{
        COST_1_ALL_CONTROLS_MASK, Cost1DispositionV1, Cost1PlatformBudgetProfileV1,
        Cost1PlatformObservationV1, evaluate_cost_1_platform_envelope,
    };
    use bitblock::crypto_d1::CanonicalEd25519PublicKeyV1;

    fn seed_for(tag: u8, idx: u16) -> [u8; 32] {
        let mut out = [tag; 32];
        out[0..2].copy_from_slice(&idx.to_be_bytes());
        out
    }

    fn run_workload(accounts: usize) -> serde_like::WorkloadResult {
        let cfg = BitsGenesisConfigV1::canonical_testnet_default();
        let genesis_id = cfg.genesis_id().expect("genesis_id");
        let relayer_seed = [0xAAu8; 32];
        let relayer_pk = CanonicalEd25519PublicKeyV1::from_secret_seed(&relayer_seed).expect("pk");
        let mut st = BitsEconomyStateV1::from_genesis(cfg, &[], &[relayer_pk]).expect("genesis");

        let mut seeds = Vec::with_capacity(accounts);
        let mut actors = Vec::with_capacity(accounts);
        let mut grant_us = Vec::with_capacity(accounts);

        for i in 0..accounts {
            let s = seed_for(0x42, i as u16);
            let pk = CanonicalEd25519PublicKeyV1::from_secret_seed(&s).expect("pk");
            let actor = derive_default_bits_actor_id(cfg.chain_id, &pk).expect("actor");
            let mut gid = [0u8; 32];
            gid[0..2].copy_from_slice(&(i as u16).to_be_bytes());
            let tx = BitsSignedTxV1::sign_with_seed(
                cfg.chain_id,
                genesis_id,
                cfg.grant_pool_actor,
                &relayer_seed,
                (i as u64) + 1,
                10_000,
                BitsTxPayloadV1::TestnetGrant(BitsTestnetGrantV1 {
                    recipient: actor,
                    recipient_pubkey: pk,
                    amount_bits: 1_000,
                    grant_kind: BitsGrantKindV1::Signup,
                    grant_id: gid,
                }),
            )
            .expect("sign grant");
            let t0 = Instant::now();
            st.apply_signed_tx(&tx, 1).expect("admit grant");
            grant_us.push(t0.elapsed().as_micros().max(1) as u64);
            seeds.push(s);
            actors.push(actor);
        }

        let pairs = accounts / 2;
        let mut purchase_us = Vec::with_capacity(pairs);
        let mut tx_bytes = 0u32;
        let mut rec_bytes = 0u32;

        for i in 0..pairs {
            let b = i * 2;
            let c = i * 2 + 1;
            let mut oid = [0x99u8; 32];
            oid[0..2].copy_from_slice(&(i as u16).to_be_bytes());
            let tx = BitsSignedTxV1::sign_with_seed(
                cfg.chain_id,
                genesis_id,
                actors[b],
                &seeds[b],
                1,
                10_000,
                BitsTxPayloadV1::PurchaseSplit(BitsPurchaseSplitV1 {
                    buyer: actors[b],
                    creator: actors[c],
                    treasury: cfg.platform_treasury_actor,
                    amount_bits: 250,
                    creator_bps: BITS_DEFAULT_CREATOR_SPLIT_BPS,
                    order_id: oid,
                    memo_hash: compute_purchase_memo_hash(&oid, "obby-v1", "vip", &actors[b]),
                }),
            )
            .expect("sign purchase");
            tx_bytes = tx.encode_canonical().expect("tx").len() as u32;
            let t0 = Instant::now();
            let r = st.apply_signed_tx(&tx, 2).expect("admit");
            purchase_us.push(t0.elapsed().as_micros().max(1) as u64);
            rec_bytes = r.encode_canonical().expect("rec").len() as u32;
        }
        st.verify_conservation().expect("conserved");

        let mut inc_max = 0u32;
        let mut verify_us = Vec::with_capacity(accounts);
        for actor in &actors {
            let p = st.prove_wallet(*actor.as_bytes()).expect("prove");
            inc_max = inc_max.max(
                p.patricia_proof
                    .encode_canonical()
                    .expect("proof wire")
                    .len() as u32,
            );
            let t0 = Instant::now();
            let _ = verify_bits_wallet_proof(st.balance_root(), &p).expect("verify");
            verify_us.push(t0.elapsed().as_micros().max(1) as u64);
        }

        let unfunded_pk = CanonicalEd25519PublicKeyV1::from_secret_seed(&[0xEEu8; 32]).expect("pk");
        let abs_proof = st.prove_wallet(*unfunded_pk.as_bytes()).expect("prove abs");
        let abs_bytes = abs_proof
            .patricia_proof
            .encode_canonical()
            .expect("proof wire")
            .len() as u32;

        grant_us.sort_unstable();
        purchase_us.sort_unstable();
        verify_us.sort_unstable();

        let profile = Cost1PlatformBudgetProfileV1::canonical_platform_default();
        let obs = Cost1PlatformObservationV1 {
            light_wasm_gzip_bytes: 149_181,
            purchase_tx_wire_bytes: tx_bytes,
            receipt_wire_bytes: rec_bytes,
            wallet_inclusion_proof_bytes: inc_max,
            wallet_absence_proof_bytes: abs_bytes,
            phone_session_door_bytes: 30_720,
            active_accounts_sampled: accounts as u32,
            apply_purchase_split_p50_us: purchase_us[purchase_us.len() / 2],
            apply_purchase_split_max_us: *purchase_us.last().unwrap(),
            verify_wallet_proof_p50_us: verify_us[verify_us.len() / 2],
            verify_wallet_proof_max_us: *verify_us.last().unwrap(),
            apply_grant_p50_us: grant_us[grant_us.len() / 2],
            powered_controls_mask: COST_1_ALL_CONTROLS_MASK,
        };
        let receipt = evaluate_cost_1_platform_envelope(&profile, &obs).expect("eval");
        assert_eq!(receipt.disposition, Cost1DispositionV1::Met);
        serde_like::WorkloadResult {
            accounts,
            tx_bytes,
            rec_bytes,
            inc_max,
            abs_bytes,
            annual_mib_10tps: receipt.projected_annual_mib_at_10tps,
            grant_p50_us: obs.apply_grant_p50_us,
            purchase_p50_us: obs.apply_purchase_split_p50_us,
            purchase_max_us: obs.apply_purchase_split_max_us,
            verify_p50_us: obs.verify_wallet_proof_p50_us,
            verify_max_us: obs.verify_wallet_proof_max_us,
            profile_tag: profile.tag().expect("ptag").to_string(),
            receipt_tag: receipt.tag().expect("rtag").to_string(),
        }
    }

    mod serde_like {
        pub struct WorkloadResult {
            pub accounts: usize,
            pub tx_bytes: u32,
            pub rec_bytes: u32,
            pub inc_max: u32,
            pub abs_bytes: u32,
            pub annual_mib_10tps: u32,
            pub grant_p50_us: u64,
            pub purchase_p50_us: u64,
            pub purchase_max_us: u64,
            pub verify_p50_us: u64,
            pub verify_max_us: u64,
            pub profile_tag: String,
            pub receipt_tag: String,
        }
    }

    let w64 = run_workload(64);
    let w256 = run_workload(256);
    println!(
        "{{\n  \"profileTag\": \"{}\",\n  \"workloads\": [\n    {{\"accounts\":{},\"txBytes\":{},\"receiptBytes\":{},\"maxInclusionProofBytes\":{},\"absenceProofBytes\":{},\"annualMibAt10Tps\":{},\"grantP50Us\":{},\"purchaseP50Us\":{},\"purchaseMaxUs\":{},\"verifyP50Us\":{},\"verifyMaxUs\":{},\"receiptTag\":\"{}\"}},\n    {{\"accounts\":{},\"txBytes\":{},\"receiptBytes\":{},\"maxInclusionProofBytes\":{},\"absenceProofBytes\":{},\"annualMibAt10Tps\":{},\"grantP50Us\":{},\"purchaseP50Us\":{},\"purchaseMaxUs\":{},\"verifyP50Us\":{},\"verifyMaxUs\":{},\"receiptTag\":\"{}\"}}\n  ]\n}}",
        w64.profile_tag,
        w64.accounts,
        w64.tx_bytes,
        w64.rec_bytes,
        w64.inc_max,
        w64.abs_bytes,
        w64.annual_mib_10tps,
        w64.grant_p50_us,
        w64.purchase_p50_us,
        w64.purchase_max_us,
        w64.verify_p50_us,
        w64.verify_max_us,
        w64.receipt_tag,
        w256.accounts,
        w256.tx_bytes,
        w256.rec_bytes,
        w256.inc_max,
        w256.abs_bytes,
        w256.annual_mib_10tps,
        w256.grant_p50_us,
        w256.purchase_p50_us,
        w256.purchase_max_us,
        w256.verify_p50_us,
        w256.verify_max_us,
        w256.receipt_tag,
    );
}
