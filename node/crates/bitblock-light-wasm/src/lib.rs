//! Zero-bindgen `wasm32-unknown-unknown` light-client verifier & single-source canonical Bits
//! transaction encoder for Gameladder `/l1.html`, Bitblock Studio, and the Node 22 relayer.
//!
//! Exports a compact linear-memory JSON C-ABI (`<= 250 KiB` gzipped; zero `bls12_381` deps):
//! - `alloc` / `dealloc`
//! - `verify_wallet_proof_json`: verifies `AUTH-COMMIT-1` `DomainPatriciaProofV1` (`Domain 3`)
//! - `verify_header_qc_json`: verifies strict RFC 8032 Ed25519 2-phase HotStuff `QuorumCert` (`> 2/3`)
//! - `verify_epoch_handoff_json`: verifies Ed25519 QC + `VALIDATOR-SCHEDULE-1` next schedule root
//! - `derive_actor_id_json`: Amendment 1 deterministic `ActorIdV1` derivation from Ed25519 K1 pubkey
//! - `encode_tx_signing_bytes_json`: single-source `BitsSignedTxV1` signing digest for WebCrypto Ed25519
//! - `encode_tx_tlv_json`: verifies strict Ed25519 signature & returns canonical big-endian wire hex

#![allow(clippy::not_unsafe_ptr_arg_deref)]

use std::alloc::{Layout, alloc as std_alloc, dealloc as std_dealloc};
use std::collections::{BTreeMap, BTreeSet};
use std::slice;

use bitblock_research::auth_commit_1::{DomainIdV1, DomainPatriciaProofV1};
use bitblock_research::bits_economy_1::{
    BITS_1_VERSION, BitsGrantKindV1, BitsIdentityBindV1, BitsIdentityProviderV1, BitsL2AnchorV1,
    BitsPurchaseSplitV1, BitsSignedTxV1, BitsSimulatedPayoutV1, BitsTestnetGrantV1, BitsTransferV1,
    BitsTxPayloadV1, derive_default_bits_actor_id,
};
use bitblock_research::crypto_d1::{
    CanonicalEd25519PublicKeyV1, CanonicalEd25519SignatureV1, HashSuiteIdV1, SignatureSuiteIdV1,
    SigningDomainTagV1, SigningPreimageV1, verify_preimage_strict,
};
use bitblock_research::identity_d1::ActorIdV1;
use serde::{Deserialize, Serialize};
use serde_json::json;
use sha2::{Digest, Sha256};

const BLOCK_HEADER_HASH_PREFIX: &[u8] = b"bb.bits1.block_header.v1";
const QC_COMMIT_PAYLOAD_PREFIX: &[u8] = b"bb.bits1.qc_commit.v1";

/// Canonical merged Bits testnet block header verified by browsers and light clients.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct BitsBlockHeaderJson {
    /// Chain identifier (`0x4249_5453_0000_0001` for testnet v1).
    pub chain_id: u64,
    /// 64-char hex 32-byte genesis digest.
    pub genesis_id: String,
    /// Block height (`>= 1`).
    pub height: u64,
    /// Consensus view/round (`>= 0`).
    pub round: u32,
    /// Active validator schedule epoch (`>= 0`).
    pub epoch: u64,
    /// 64-char hex parent block hash (`genesis_id` for height 1).
    pub parent_hash: String,
    /// 64-char hex `DomainIdV1::NativeBalance` (`3`) Patricia root.
    pub balance_root: String,
    /// 64-char hex `DomainIdV1::Nonce` (`2`) Patricia root.
    pub nonce_root: String,
    /// 64-char hex Merkle root of committed transactions in this block.
    pub tx_root: String,
    /// 64-char hex `DomainIdV1::ValidatorSchedule` (`7`) root.
    pub validator_schedule_root: String,
}

/// One validator vote signature inside an Ed25519 HotStuff `QuorumCert`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct BitsQcSignatureJson {
    /// Validator identifier (64-char hex `ActorIdV1` or pubkey hex).
    pub validator_id: String,
    /// 64-char hex Ed25519 consensus vote public key.
    pub vote_pubkey_hex: String,
    /// 128-char hex strict RFC 8032 Ed25519 signature over the QC commit preimage.
    pub sig_hex: String,
}

/// Canonical Ed25519 HotStuff `QuorumCert` committing a finalized [`BitsBlockHeaderJson`].
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct BitsQuorumCertJson {
    /// 64-char hex genesis digest.
    pub genesis_id: String,
    /// Finalized block height.
    pub height: u64,
    /// Finalized consensus round.
    pub round: u32,
    /// Active committee epoch.
    pub epoch: u64,
    /// 64-char hex canonical header hash.
    pub block_hash: String,
    /// Distinct validator vote signatures exceeding `2/3` of active committee weight.
    pub signatures: Vec<BitsQcSignatureJson>,
}

/// One active committee member entry supplied to [`verify_header_qc_json`].
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct BitsCommitteeMemberJson {
    /// Validator identifier matching [`BitsQcSignatureJson::validator_id`].
    pub validator_id: String,
    /// 64-char hex Ed25519 consensus vote public key.
    #[serde(alias = "ed25519_pk_hex")]
    pub vote_pubkey_hex: String,
    /// Positive voting weight.
    pub power: u64,
}

#[derive(Deserialize)]
struct HeaderQcInput {
    header: BitsBlockHeaderJson,
    qc: BitsQuorumCertJson,
    committee: Vec<BitsCommitteeMemberJson>,
}

#[derive(Deserialize)]
struct EpochHandoffInput {
    header: BitsBlockHeaderJson,
    qc: BitsQuorumCertJson,
    committee: Vec<BitsCommitteeMemberJson>,
    #[serde(alias = "expected_next_group_pk_hex")]
    expected_next_schedule_root_hex: String,
}

#[derive(Deserialize)]
struct WalletProofInput {
    #[serde(alias = "actorId", alias = "actor_id_hex")]
    actor_id: String,
    #[serde(alias = "balanceBits", alias = "balance_bits")]
    balance: u64,
    #[serde(alias = "balanceRoot", alias = "balance_root")]
    root: String,
    #[serde(alias = "proofHex", alias = "patricia_proof_hex")]
    proof_hex: String,
}

#[derive(Deserialize)]
struct DeriveActorInput {
    chain_id: u64,
    #[serde(alias = "pubkeyHex", alias = "ed25519Hex")]
    pubkey_hex: String,
}

#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum TxPayloadInput {
    Transfer {
        from: String,
        to: String,
        amount_bits: u64,
        #[serde(default)]
        memo_hash: String,
    },
    PurchaseSplit {
        buyer: String,
        creator: String,
        treasury: String,
        amount_bits: u64,
        creator_bps: u16,
        order_id: String,
        memo_hash: String,
    },
    TestnetGrant {
        recipient: String,
        recipient_pubkey: String,
        amount_bits: u64,
        grant_kind: u8,
        grant_id: String,
    },
    SimulatedPayout {
        creator: String,
        amount_bits: u64,
        payout_request_id: String,
    },
    IdentityBind {
        actor_id: String,
        ed25519_pk: String,
        provider: u8,
        external_id_hash: String,
        pop_signature: String,
    },
    L2Anchor {
        app_id_hash: String,
        epoch_id: u64,
        l2_root: String,
        event_count: u32,
    },
}

#[derive(Deserialize)]
struct TxInput {
    chain_id: u64,
    genesis_id: String,
    sender_actor: String,
    signer_pubkey: String,
    seq: u64,
    #[serde(default)]
    valid_until_height: u64,
    payload: TxPayloadInput,
    #[serde(default)]
    sig_hex: String,
    #[serde(default)]
    sig: Vec<u8>,
}

#[derive(Deserialize)]
struct EncodeSignedTxInput {
    tx: TxInput,
}

/// Allocates `len` bytes inside WebAssembly linear memory.
#[unsafe(no_mangle)]
pub extern "C" fn alloc(len: usize) -> *mut u8 {
    if len == 0 {
        return std::ptr::null_mut();
    }
    let Ok(layout) = Layout::from_size_align(len, 1) else {
        return std::ptr::null_mut();
    };
    unsafe { std_alloc(layout) }
}

/// Frees a buffer previously returned by [`alloc`].
#[unsafe(no_mangle)]
pub extern "C" fn dealloc(ptr: *mut u8, len: usize) {
    if ptr.is_null() || len == 0 {
        return;
    }
    if let Ok(layout) = Layout::from_size_align(len, 1) {
        unsafe { std_dealloc(ptr, layout) };
    }
}

fn read_input(ptr: *const u8, len: usize) -> Option<&'static [u8]> {
    if ptr.is_null() || len == 0 {
        return None;
    }
    Some(unsafe { slice::from_raw_parts(ptr, len) })
}

fn pack_output_bytes(bytes: &[u8]) -> u64 {
    let len = bytes.len();
    if len == 0 {
        return 0;
    }
    let ptr = alloc(len);
    if ptr.is_null() {
        return 0;
    }
    unsafe {
        std::ptr::copy_nonoverlapping(bytes.as_ptr(), ptr, len);
    }
    ((ptr as usize as u64) << 32) | (len as u64)
}

fn decode_hex_32(s: &str) -> Result<[u8; 32], &'static str> {
    let raw = hex::decode(s.trim()).map_err(|_| "invalid 32-byte hex")?;
    raw.try_into().map_err(|_| "expected exactly 32 bytes")
}

fn decode_hex_32_or_zero(s: &str) -> Result<[u8; 32], &'static str> {
    if s.trim().is_empty() {
        return Ok([0u8; 32]);
    }
    decode_hex_32(s)
}

fn decode_hex_64(s: &str) -> Result<[u8; 64], &'static str> {
    let raw = hex::decode(s.trim()).map_err(|_| "invalid 64-byte hex")?;
    raw.try_into().map_err(|_| "expected exactly 64 bytes")
}

/// Computes the canonical 32-byte header hash of a [`BitsBlockHeaderJson`].
pub fn compute_bits_header_hash(header: &BitsBlockHeaderJson) -> Result<[u8; 32], &'static str> {
    let genesis_id = decode_hex_32(&header.genesis_id)?;
    let parent_hash = decode_hex_32(&header.parent_hash)?;
    let balance_root = decode_hex_32(&header.balance_root)?;
    let nonce_root = decode_hex_32(&header.nonce_root)?;
    let tx_root = decode_hex_32(&header.tx_root)?;
    let schedule_root = decode_hex_32(&header.validator_schedule_root)?;

    let mut h = Sha256::new();
    h.update(BLOCK_HEADER_HASH_PREFIX);
    h.update(BITS_1_VERSION.to_be_bytes());
    h.update(header.chain_id.to_be_bytes());
    h.update(genesis_id);
    h.update(header.height.to_be_bytes());
    h.update(header.round.to_be_bytes());
    h.update(header.epoch.to_be_bytes());
    h.update(parent_hash);
    h.update(balance_root);
    h.update(nonce_root);
    h.update(tx_root);
    h.update(schedule_root);
    Ok(h.finalize().into())
}

/// Builds the canonical `CRYPTO-D1` [`SigningPreimageV1`] signed by validators inside a QC commit.
pub fn build_qc_commit_preimage(
    chain_id: u64,
    genesis_id: [u8; 32],
    height: u64,
    round: u32,
    epoch: u64,
    block_hash: [u8; 32],
) -> SigningPreimageV1 {
    let mut h = Sha256::new();
    h.update(QC_COMMIT_PAYLOAD_PREFIX);
    h.update(genesis_id);
    h.update(height.to_be_bytes());
    h.update(round.to_be_bytes());
    h.update(epoch.to_be_bytes());
    h.update(block_hash);
    let payload_digest: [u8; 32] = h.finalize().into();
    SigningPreimageV1 {
        protocol_version: BITS_1_VERSION,
        chain_id,
        genesis_id,
        launch_manifest_id: genesis_id,
        profile_id: genesis_id,
        domain_tag: SigningDomainTagV1::ConsensusVote,
        signature_suite: SignatureSuiteIdV1::Ed25519StrictRfc8032V1,
        hash_suite: HashSuiteIdV1::Sha256V1,
        payload_digest,
    }
}

fn verify_qc_parts(
    header: &BitsBlockHeaderJson,
    qc: &BitsQuorumCertJson,
    committee: &[BitsCommitteeMemberJson],
) -> i32 {
    if header.height == 0 || committee.is_empty() {
        return -2;
    }
    let Ok(header_hash) = compute_bits_header_hash(header) else {
        return -3;
    };
    let Ok(qc_block_hash) = decode_hex_32(&qc.block_hash) else {
        return -4;
    };
    let Ok(genesis_id) = decode_hex_32(&header.genesis_id) else {
        return -5;
    };
    let Ok(qc_genesis) = decode_hex_32(&qc.genesis_id) else {
        return -5;
    };
    if header_hash != qc_block_hash
        || genesis_id != qc_genesis
        || header.height != qc.height
        || header.round != qc.round
        || header.epoch != qc.epoch
    {
        return -6;
    }

    let mut total_power = 0u128;
    let mut members: BTreeMap<String, (CanonicalEd25519PublicKeyV1, u64)> = BTreeMap::new();
    for m in committee {
        if m.power == 0 {
            return -7;
        }
        let Ok(pk_bytes) = decode_hex_32(&m.vote_pubkey_hex) else {
            return -7;
        };
        let Ok(pk) = CanonicalEd25519PublicKeyV1::from_bytes(pk_bytes) else {
            return -7;
        };
        let key = m.validator_id.trim().to_lowercase();
        if members.insert(key, (pk, m.power)).is_some() {
            return -7;
        }
        total_power = total_power.saturating_add(u128::from(m.power));
    }
    if total_power == 0 {
        return -7;
    }

    let preimage = build_qc_commit_preimage(
        header.chain_id,
        genesis_id,
        qc.height,
        qc.round,
        qc.epoch,
        qc_block_hash,
    );
    let mut seen_signers: BTreeSet<String> = BTreeSet::new();
    let mut signed_power = 0u128;
    for s in &qc.signatures {
        let vid = s.validator_id.trim().to_lowercase();
        if !seen_signers.insert(vid.clone()) {
            return -8;
        }
        let Some(&(expected_pk, power)) = members.get(&vid) else {
            return -9;
        };
        let Ok(pk_bytes) = decode_hex_32(&s.vote_pubkey_hex) else {
            return -10;
        };
        if pk_bytes != *expected_pk.as_bytes() {
            return -10;
        }
        let Ok(sig_bytes) = decode_hex_64(&s.sig_hex) else {
            return -11;
        };
        let Ok(sig) = CanonicalEd25519SignatureV1::from_bytes(sig_bytes) else {
            return -11;
        };
        if verify_preimage_strict(&expected_pk, &preimage, &sig).is_err() {
            return -12;
        }
        signed_power = signed_power.saturating_add(u128::from(power));
    }
    if signed_power.saturating_mul(3) <= total_power.saturating_mul(2) {
        return -13;
    }
    1
}

/// Pure helper verifying a JSON `DomainPatriciaProofV1` balance proof (`1` = valid, `<= 0` = fail).
pub fn check_wallet_proof_slice(bytes: &[u8]) -> i32 {
    let Ok(input) = serde_json::from_slice::<WalletProofInput>(bytes) else {
        return -1;
    };
    let Ok(actor_bytes) = decode_hex_32(&input.actor_id) else {
        return -2;
    };
    let Ok(root_bytes) = decode_hex_32(&input.root) else {
        return -3;
    };
    let Ok(proof_bytes) = hex::decode(input.proof_hex.trim()) else {
        return -4;
    };
    let Ok(proof) = DomainPatriciaProofV1::decode_canonical(&proof_bytes) else {
        return -5;
    };
    let Ok(val_opt) = proof.verify(DomainIdV1::NativeBalance, root_bytes, &actor_bytes) else {
        return -6;
    };
    let proven_balance = match val_opt {
        None => 0u128,
        Some(v) => {
            let Ok(arr) = <[u8; 16]>::try_from(v.as_slice()) else {
                return -7;
            };
            u128::from_be_bytes(arr)
        }
    };
    if proven_balance == u128::from(input.balance) {
        1
    } else {
        0
    }
}

/// Pure helper verifying an Ed25519 HotStuff header + QuorumCert (`1` = valid, `< 0` = fail).
pub fn check_header_qc_slice(bytes: &[u8]) -> i32 {
    let Ok(input) = serde_json::from_slice::<HeaderQcInput>(bytes) else {
        return -1;
    };
    verify_qc_parts(&input.header, &input.qc, &input.committee)
}

/// Pure helper verifying an Ed25519 QC + `VALIDATOR-SCHEDULE-1` epoch handoff (`1` = valid).
pub fn check_epoch_handoff_slice(bytes: &[u8]) -> i32 {
    let Ok(input) = serde_json::from_slice::<EpochHandoffInput>(bytes) else {
        return -1;
    };
    let code = verify_qc_parts(&input.header, &input.qc, &input.committee);
    if code != 1 {
        return code;
    }
    let Ok(expected_root) = decode_hex_32(&input.expected_next_schedule_root_hex) else {
        return -14;
    };
    let Ok(header_root) = decode_hex_32(&input.header.validator_schedule_root) else {
        return -15;
    };
    if header_root == expected_root { 1 } else { -16 }
}

/// Pure helper deriving canonical `ActorIdV1` from `(chain_id, pubkey_hex)` (Amendment 1).
pub fn derive_actor_id_slice(bytes: &[u8]) -> Vec<u8> {
    let input: DeriveActorInput = match serde_json::from_slice(bytes) {
        Ok(v) => v,
        Err(e) => {
            return serde_json::to_vec(&json!({ "ok": false, "reason": format!("bad json: {e}") }))
                .unwrap_or_default();
        }
    };
    let pk_bytes = match decode_hex_32(&input.pubkey_hex) {
        Ok(b) => b,
        Err(e) => {
            return serde_json::to_vec(&json!({ "ok": false, "reason": e })).unwrap_or_default();
        }
    };
    let pk = match CanonicalEd25519PublicKeyV1::from_bytes(pk_bytes) {
        Ok(p) => p,
        Err(e) => {
            return serde_json::to_vec(&json!({ "ok": false, "reason": e.to_string() }))
                .unwrap_or_default();
        }
    };
    match derive_default_bits_actor_id(input.chain_id, &pk) {
        Ok(actor) => serde_json::to_vec(&json!({
            "ok": true,
            "actor_id_hex": hex::encode(actor.as_bytes()),
            "pubkey_hex": hex::encode(pk.as_bytes()),
        }))
        .unwrap_or_default(),
        Err(e) => {
            serde_json::to_vec(&json!({ "ok": false, "reason": e.to_string() })).unwrap_or_default()
        }
    }
}

fn build_tx_from_input(input: &TxInput, require_sig: bool) -> Result<BitsSignedTxV1, String> {
    let genesis_id = decode_hex_32(&input.genesis_id).map_err(|e| e.to_string())?;
    let sender_actor =
        ActorIdV1::from_bytes(decode_hex_32(&input.sender_actor).map_err(|e| e.to_string())?);
    let signer_pk_bytes = decode_hex_32(&input.signer_pubkey).map_err(|e| e.to_string())?;
    let signer_pubkey =
        CanonicalEd25519PublicKeyV1::from_bytes(signer_pk_bytes).map_err(|e| e.to_string())?;

    let payload = match &input.payload {
        TxPayloadInput::Transfer {
            from,
            to,
            amount_bits,
            memo_hash,
        } => BitsTxPayloadV1::Transfer(BitsTransferV1 {
            from: ActorIdV1::from_bytes(decode_hex_32(from).map_err(|e| e.to_string())?),
            to: ActorIdV1::from_bytes(decode_hex_32(to).map_err(|e| e.to_string())?),
            amount_bits: u128::from(*amount_bits),
            memo_hash: decode_hex_32_or_zero(memo_hash).map_err(|e| e.to_string())?,
        }),
        TxPayloadInput::PurchaseSplit {
            buyer,
            creator,
            treasury,
            amount_bits,
            creator_bps,
            order_id,
            memo_hash,
        } => BitsTxPayloadV1::PurchaseSplit(BitsPurchaseSplitV1 {
            buyer: ActorIdV1::from_bytes(decode_hex_32(buyer).map_err(|e| e.to_string())?),
            creator: ActorIdV1::from_bytes(decode_hex_32(creator).map_err(|e| e.to_string())?),
            treasury: ActorIdV1::from_bytes(decode_hex_32(treasury).map_err(|e| e.to_string())?),
            amount_bits: u128::from(*amount_bits),
            creator_bps: *creator_bps,
            order_id: decode_hex_32(order_id).map_err(|e| e.to_string())?,
            memo_hash: decode_hex_32(memo_hash).map_err(|e| e.to_string())?,
        }),
        TxPayloadInput::TestnetGrant {
            recipient,
            recipient_pubkey,
            amount_bits,
            grant_kind,
            grant_id,
        } => BitsTxPayloadV1::TestnetGrant(BitsTestnetGrantV1 {
            recipient: ActorIdV1::from_bytes(decode_hex_32(recipient).map_err(|e| e.to_string())?),
            recipient_pubkey: CanonicalEd25519PublicKeyV1::from_bytes(
                decode_hex_32(recipient_pubkey).map_err(|e| e.to_string())?,
            )
            .map_err(|e| e.to_string())?,
            amount_bits: u128::from(*amount_bits),
            grant_kind: BitsGrantKindV1::from_code(*grant_kind).map_err(|e| e.to_string())?,
            grant_id: decode_hex_32(grant_id).map_err(|e| e.to_string())?,
        }),
        TxPayloadInput::SimulatedPayout {
            creator,
            amount_bits,
            payout_request_id,
        } => BitsTxPayloadV1::SimulatedPayout(BitsSimulatedPayoutV1 {
            creator: ActorIdV1::from_bytes(decode_hex_32(creator).map_err(|e| e.to_string())?),
            amount_bits: u128::from(*amount_bits),
            payout_request_id: decode_hex_32(payout_request_id).map_err(|e| e.to_string())?,
        }),
        TxPayloadInput::IdentityBind {
            actor_id,
            ed25519_pk,
            provider,
            external_id_hash,
            pop_signature,
        } => BitsTxPayloadV1::IdentityBind(BitsIdentityBindV1 {
            actor_id: ActorIdV1::from_bytes(decode_hex_32(actor_id).map_err(|e| e.to_string())?),
            ed25519_pk: CanonicalEd25519PublicKeyV1::from_bytes(
                decode_hex_32(ed25519_pk).map_err(|e| e.to_string())?,
            )
            .map_err(|e| e.to_string())?,
            provider: BitsIdentityProviderV1::from_code(*provider).map_err(|e| e.to_string())?,
            external_id_hash: decode_hex_32(external_id_hash).map_err(|e| e.to_string())?,
            pop_signature: CanonicalEd25519SignatureV1::from_bytes(
                decode_hex_64(pop_signature).map_err(|e| e.to_string())?,
            )
            .map_err(|e| e.to_string())?,
        }),
        TxPayloadInput::L2Anchor {
            app_id_hash,
            epoch_id,
            l2_root,
            event_count,
        } => BitsTxPayloadV1::L2Anchor(BitsL2AnchorV1 {
            app_id_hash: decode_hex_32(app_id_hash).map_err(|e| e.to_string())?,
            epoch_id: *epoch_id,
            l2_root: decode_hex_32(l2_root).map_err(|e| e.to_string())?,
            event_count: *event_count,
        }),
    };

    let signature = if require_sig {
        let sig_bytes = if !input.sig_hex.trim().is_empty() {
            decode_hex_64(&input.sig_hex).map_err(|e| e.to_string())?
        } else {
            <[u8; 64]>::try_from(input.sig.as_slice())
                .map_err(|_| "signature must be 64 bytes".to_string())?
        };
        CanonicalEd25519SignatureV1::from_bytes(sig_bytes).map_err(|e| e.to_string())?
    } else {
        // Placeholder canonical signature ignored by encode_unsigned_canonical / signing_digest.
        let dummy_seed = [0x01u8; 32];
        let dummy = BitsSignedTxV1::sign_with_seed(
            input.chain_id,
            genesis_id,
            sender_actor,
            &dummy_seed,
            input.seq,
            input.valid_until_height,
            payload,
        )
        .map_err(|e| e.to_string())?;
        dummy.signature
    };

    Ok(BitsSignedTxV1 {
        version: BITS_1_VERSION,
        chain_id: input.chain_id,
        genesis_id,
        sender_actor,
        signer_pubkey,
        seq: input.seq,
        valid_until_height: input.valid_until_height,
        payload,
        signature,
    })
}

/// Pure helper producing the canonical signing digest & unsigned wire hex for WebCrypto Ed25519.
pub fn encode_tx_signing_bytes_slice(bytes: &[u8]) -> Vec<u8> {
    let input: TxInput = match serde_json::from_slice(bytes) {
        Ok(v) => v,
        Err(e) => {
            return serde_json::to_vec(
                &json!({ "ok": false, "reason": format!("bad tx json: {e}") }),
            )
            .unwrap_or_default();
        }
    };
    let tx = match build_tx_from_input(&input, false) {
        Ok(t) => t,
        Err(e) => {
            return serde_json::to_vec(&json!({ "ok": false, "reason": e })).unwrap_or_default();
        }
    };
    let unsigned = match tx.encode_unsigned_canonical() {
        Ok(b) => b,
        Err(e) => {
            return serde_json::to_vec(&json!({ "ok": false, "reason": e.to_string() }))
                .unwrap_or_default();
        }
    };
    let digest = match tx.signing_digest() {
        Ok(d) => d,
        Err(e) => {
            return serde_json::to_vec(&json!({ "ok": false, "reason": e.to_string() }))
                .unwrap_or_default();
        }
    };
    let unsigned_hex = hex::encode(&unsigned);
    serde_json::to_vec(&json!({
        "ok": true,
        "signing_digest_hex": hex::encode(digest),
        "unsigned_tx_hex": unsigned_hex,
        "unsigned_tlv_hex": unsigned_hex,
    }))
    .unwrap_or_default()
}

/// Pure helper verifying strict Ed25519 signature & encoding canonical signed Bits wire hex.
pub fn encode_tx_tlv_slice(bytes: &[u8]) -> Vec<u8> {
    let input: EncodeSignedTxInput = match serde_json::from_slice(bytes) {
        Ok(v) => v,
        Err(e) => {
            return serde_json::to_vec(
                &json!({ "ok": false, "reason": format!("bad input json: {e}") }),
            )
            .unwrap_or_default();
        }
    };
    let tx = match build_tx_from_input(&input.tx, true) {
        Ok(t) => t,
        Err(e) => {
            return serde_json::to_vec(&json!({ "ok": false, "reason": e })).unwrap_or_default();
        }
    };
    if let Err(e) = tx.verify_signature() {
        return serde_json::to_vec(
            &json!({ "ok": false, "reason": format!("invalid tx signature: {e}") }),
        )
        .unwrap_or_default();
    }
    let wire = match tx.encode_canonical() {
        Ok(b) => b,
        Err(e) => {
            return serde_json::to_vec(&json!({ "ok": false, "reason": e.to_string() }))
                .unwrap_or_default();
        }
    };
    let tx_hash = match tx.tx_hash() {
        Ok(h) => h,
        Err(e) => {
            return serde_json::to_vec(&json!({ "ok": false, "reason": e.to_string() }))
                .unwrap_or_default();
        }
    };
    let wire_hex = hex::encode(&wire);
    serde_json::to_vec(&json!({
        "ok": true,
        "tx_hex": wire_hex,
        "tlv_hex": wire_hex,
        "tx_hash_hex": hex::encode(tx_hash),
        "pow_nonce": 0,
        "pow_leading_zeros": 0,
    }))
    .unwrap_or_default()
}

/// C-ABI export verifying a JSON `DomainPatriciaProofV1` balance proof (`1` = ok).
#[unsafe(no_mangle)]
pub extern "C" fn verify_wallet_proof_json(ptr: *const u8, len: usize) -> i32 {
    let Some(bytes) = read_input(ptr, len) else {
        return -1;
    };
    check_wallet_proof_slice(bytes)
}

/// C-ABI export verifying an Ed25519 HotStuff header + QuorumCert (`1` = ok).
#[unsafe(no_mangle)]
pub extern "C" fn verify_header_qc_json(ptr: *const u8, len: usize) -> i32 {
    let Some(bytes) = read_input(ptr, len) else {
        return -1;
    };
    check_header_qc_slice(bytes)
}

/// C-ABI export verifying an Ed25519 QC + `VALIDATOR-SCHEDULE-1` epoch handoff (`1` = ok).
#[unsafe(no_mangle)]
pub extern "C" fn verify_epoch_handoff_json(ptr: *const u8, len: usize) -> i32 {
    let Some(bytes) = read_input(ptr, len) else {
        return -1;
    };
    check_epoch_handoff_slice(bytes)
}

/// C-ABI export deriving canonical `ActorIdV1` from `(chain_id, pubkey_hex)` (Amendment 1).
#[unsafe(no_mangle)]
pub extern "C" fn derive_actor_id_json(ptr: *const u8, len: usize) -> u64 {
    let Some(bytes) = read_input(ptr, len) else {
        return 0;
    };
    pack_output_bytes(&derive_actor_id_slice(bytes))
}

/// C-ABI export returning canonical signing digest + unsigned wire hex for WebCrypto Ed25519.
#[unsafe(no_mangle)]
pub extern "C" fn encode_tx_signing_bytes_json(ptr: *const u8, len: usize) -> u64 {
    let Some(bytes) = read_input(ptr, len) else {
        return 0;
    };
    pack_output_bytes(&encode_tx_signing_bytes_slice(bytes))
}

/// C-ABI export verifying strict Ed25519 signature & encoding canonical signed Bits wire hex.
#[unsafe(no_mangle)]
pub extern "C" fn encode_tx_tlv_json(ptr: *const u8, len: usize) -> u64 {
    let Some(bytes) = read_input(ptr, len) else {
        return 0;
    };
    pack_output_bytes(&encode_tx_tlv_slice(bytes))
}

#[cfg(test)]
mod tests {
    use super::*;
    use bitblock_research::bits_economy_1::{
        BitsEconomyStateV1, BitsGenesisConfigV1, BitsGrantKindV1,
    };
    use bitblock_research::crypto_d1::sign_preimage_deterministic;

    #[test]
    fn light_wasm_verifies_patricia_wallet_qc_handoff_and_bits_tx() {
        let cfg = BitsGenesisConfigV1::canonical_testnet_default();
        let s1 = [0x11u8; 32];
        let s_rel = [0x99u8; 32];
        let pk1 = CanonicalEd25519PublicKeyV1::from_secret_seed(&s1).unwrap();
        let pk_rel = CanonicalEd25519PublicKeyV1::from_secret_seed(&s_rel).unwrap();
        let a1 = derive_default_bits_actor_id(cfg.chain_id, &pk1).unwrap();

        // 1. Amendment 1 derive_actor_id_slice
        let d_out: serde_json::Value = serde_json::from_slice(&derive_actor_id_slice(
            &serde_json::to_vec(&json!({
                "chain_id": cfg.chain_id,
                "pubkey_hex": hex::encode(pk1.as_bytes()),
            }))
            .unwrap(),
        ))
        .unwrap();
        assert_eq!(d_out["ok"], true);
        assert_eq!(d_out["actor_id_hex"], hex::encode(a1.as_bytes()));

        // 2. Real DomainPatriciaProofV1 wallet proof check
        let mut st = BitsEconomyStateV1::from_genesis(cfg, &[], &[pk_rel]).unwrap();
        let gid = st.genesis_id();
        let gtx = BitsSignedTxV1::sign_with_seed(
            cfg.chain_id,
            gid,
            cfg.grant_pool_actor,
            &s_rel,
            1,
            10,
            BitsTxPayloadV1::TestnetGrant(BitsTestnetGrantV1 {
                recipient: a1,
                recipient_pubkey: pk1,
                amount_bits: 1_000,
                grant_kind: BitsGrantKindV1::Signup,
                grant_id: [0xA1; 32],
            }),
        )
        .unwrap();
        st.apply_signed_tx(&gtx, 1).unwrap();
        let wp = st.prove_wallet(*pk1.as_bytes()).unwrap();
        let proof_hex = hex::encode(wp.patricia_proof.encode_canonical().unwrap());
        let ok_wp = serde_json::to_vec(&json!({
            "actorId": hex::encode(wp.actor_id.as_bytes()),
            "balance": wp.balance_bits,
            "root": hex::encode(wp.balance_root),
            "proofHex": proof_hex,
        }))
        .unwrap();
        assert_eq!(check_wallet_proof_slice(&ok_wp), 1);

        // Tampered balance must fail!
        let bad_wp = serde_json::to_vec(&json!({
            "actorId": hex::encode(wp.actor_id.as_bytes()),
            "balance": 1_001u128,
            "root": hex::encode(wp.balance_root),
            "proofHex": proof_hex,
        }))
        .unwrap();
        assert_eq!(check_wallet_proof_slice(&bad_wp), 0);

        // 3. Real 4-validator Ed25519 HotStuff QC + epoch handoff
        let v_seeds = [[0x41u8; 32], [0x42u8; 32], [0x43u8; 32], [0x44u8; 32]];
        let v_pks: Vec<_> = v_seeds
            .iter()
            .map(|s| CanonicalEd25519PublicKeyV1::from_secret_seed(s).unwrap())
            .collect();
        let committee: Vec<_> = v_pks
            .iter()
            .enumerate()
            .map(|(i, pk)| BitsCommitteeMemberJson {
                validator_id: format!("val-{i}"),
                vote_pubkey_hex: hex::encode(pk.as_bytes()),
                power: 100,
            })
            .collect();
        let sched_root = [0x55u8; 32];
        let header = BitsBlockHeaderJson {
            chain_id: cfg.chain_id,
            genesis_id: hex::encode(gid),
            height: 1,
            round: 0,
            epoch: 0,
            parent_hash: hex::encode(gid),
            balance_root: hex::encode(st.balance_root()),
            nonce_root: hex::encode(st.nonce_root()),
            tx_root: hex::encode([0x12u8; 32]),
            validator_schedule_root: hex::encode(sched_root),
        };
        let bh = compute_bits_header_hash(&header).unwrap();
        let pre = build_qc_commit_preimage(cfg.chain_id, gid, 1, 0, 0, bh);
        let sigs: Vec<_> = (0..3)
            .map(|i| BitsQcSignatureJson {
                validator_id: format!("val-{i}"),
                vote_pubkey_hex: hex::encode(v_pks[i].as_bytes()),
                sig_hex: hex::encode(
                    sign_preimage_deterministic(&v_seeds[i], &pre)
                        .unwrap()
                        .as_bytes(),
                ),
            })
            .collect();
        let qc = BitsQuorumCertJson {
            genesis_id: hex::encode(gid),
            height: 1,
            round: 0,
            epoch: 0,
            block_hash: hex::encode(bh),
            signatures: sigs,
        };
        let qc_json = serde_json::to_vec(&json!({
            "header": header,
            "qc": qc,
            "committee": committee,
        }))
        .unwrap();
        assert_eq!(check_header_qc_slice(&qc_json), 1);

        // Sub-quorum (2 of 4 = 50% <= 66.7%) rejected!
        let mut sub_qc = qc.clone();
        sub_qc.signatures.pop();
        let sub_json = serde_json::to_vec(&json!({
            "header": header,
            "qc": sub_qc,
            "committee": committee,
        }))
        .unwrap();
        assert_eq!(check_header_qc_slice(&sub_json), -13);

        // Epoch handoff passes with matching schedule root, fails with wrong root
        let handoff_ok = serde_json::to_vec(&json!({
            "header": header,
            "qc": qc,
            "committee": committee,
            "expected_next_schedule_root_hex": hex::encode(sched_root),
        }))
        .unwrap();
        assert_eq!(check_epoch_handoff_slice(&handoff_ok), 1);
        let handoff_bad = serde_json::to_vec(&json!({
            "header": header,
            "qc": qc,
            "committee": committee,
            "expected_next_schedule_root_hex": hex::encode([0x56u8; 32]),
        }))
        .unwrap();
        assert_eq!(check_epoch_handoff_slice(&handoff_bad), -16);
    }
}
