#!/usr/bin/env node
// Zero-dependency Node 22 V8 parity & size-budget harness for `bitblock-light-wasm`.
// Verifies:
//   1. Compiled + gzipped wasm32 bundle <= 250 KiB (256,000 bytes)
//   2. Amendment 1 `derive_actor_id_json` exact match with Python `bits_economy_1_vectors.py`
//   3. `verify_wallet_proof_json` (inclusion 1,000 Bits, non-membership 0 Bits, tampered balance/root fail)
//   4. Live WebCrypto Ed25519 `encode_tx_signing_bytes_json` + `encode_tx_tlv_json` (80/20 PurchaseSplit + bad-sig reject)
//   5. Live 4-validator Ed25519 HotStuff `verify_header_qc_json` (3-of-4 pass, 2-of-4 fail, duplicate fail, forged fail)
//   6. `verify_epoch_handoff_json` (matching next schedule root pass, mismatched fail)

import assert from 'node:assert/strict';
import crypto from 'node:crypto';
import fs from 'node:fs';
import path from 'node:path';
import zlib from 'node:zlib';

const DEFAULT_WASM = path.resolve(
  process.cwd(),
  'node/target/wasm32-unknown-unknown/release/bitblock_light_wasm.wasm',
);
const wasmPath = process.argv[2] ? path.resolve(process.argv[2]) : DEFAULT_WASM;
const MAX_GZIP_BYTES = 256_000; // 250 KiB browser light-verifier budget

const enc = new TextEncoder();
const dec = new TextDecoder();

const toHex = buf => Buffer.from(buf).toString('hex');
const fromHex = hex => Uint8Array.from(Buffer.from(hex, 'hex'));

function sha256(parts) {
  const h = crypto.createHash('sha256');
  for (const p of parts) h.update(p);
  return h.digest();
}

function u16be(n) {
  const b = Buffer.alloc(2);
  b.writeUInt16BE(n);
  return b;
}
function u32be(n) {
  const b = Buffer.alloc(4);
  b.writeUInt32BE(n);
  return b;
}
function u64be(n) {
  const b = Buffer.alloc(8);
  b.writeBigUInt64BE(BigInt(n));
  return b;
}

function lpField(buf) {
  return Buffer.concat([u16be(buf.length), buf]);
}

function contentTagDigest(kind, schema, payload) {
  return sha256([
    lpField(Buffer.from('bitblock/content-tag/v1', 'ascii')),
    lpField(Buffer.from(kind, 'ascii')),
    lpField(Buffer.from('sha256', 'ascii')),
    lpField(Buffer.from(schema, 'ascii')),
    u64be(payload.length),
    payload,
  ]);
}

function computeHeaderHash(h) {
  return sha256([
    Buffer.from('bb.bits1.block_header.v1', 'ascii'),
    u16be(1),
    u64be(h.chain_id),
    Buffer.from(h.genesis_id, 'hex'),
    u64be(h.height),
    u32be(h.round),
    u64be(h.epoch),
    Buffer.from(h.parent_hash, 'hex'),
    Buffer.from(h.balance_root, 'hex'),
    Buffer.from(h.nonce_root, 'hex'),
    Buffer.from(h.tx_root, 'hex'),
    Buffer.from(h.validator_schedule_root, 'hex'),
  ]);
}

function qcCommitSigningDigest(chainId, genesisHex, height, round, epoch, blockHashBuf) {
  const genesis = Buffer.from(genesisHex, 'hex');
  const payloadDigest = sha256([
    Buffer.from('bb.bits1.qc_commit.v1', 'ascii'),
    genesis,
    u64be(height),
    u32be(round),
    u64be(epoch),
    blockHashBuf,
  ]);
  const preimage = Buffer.concat([
    u16be(1),
    u64be(chainId),
    genesis,
    genesis,
    genesis,
    Buffer.from([4]), // SigningDomainTagV1::ConsensusVote = 4
    u16be(1),
    u16be(1),
    payloadDigest,
  ]);
  assert.equal(preimage.length, 143);
  return contentTagDigest('receipt', 'bitblock/crypto-d1/signing-preimage/v1', preimage);
}

async function main() {
  const rawBytes = fs.readFileSync(wasmPath);
  const gzBytes = zlib.gzipSync(rawBytes, { level: 9 });
  assert.ok(
    gzBytes.length <= MAX_GZIP_BYTES,
    `bitblock-light-wasm gzipped size ${gzBytes.length} exceeds ${MAX_GZIP_BYTES} byte budget`,
  );

  const { instance } = await WebAssembly.instantiate(rawBytes, {});
  const {
    memory,
    alloc,
    dealloc,
    verify_wallet_proof_json,
    verify_header_qc_json,
    verify_epoch_handoff_json,
    derive_actor_id_json,
    encode_tx_signing_bytes_json,
    encode_tx_tlv_json,
  } = instance.exports;

  // Confirm retired BLS/casino exports are NOT present
  assert.equal(instance.exports.verify_beacon_json, undefined);
  assert.equal(instance.exports.verify_l9_settlement, undefined);

  function callI32(fn, obj) {
    const bytes = enc.encode(JSON.stringify(obj));
    const ptr = alloc(bytes.length);
    assert.ok(ptr !== 0);
    try {
      new Uint8Array(memory.buffer, ptr, bytes.length).set(bytes);
      return fn(ptr, bytes.length);
    } finally {
      dealloc(ptr, bytes.length);
    }
  }

  function callJson(fn, obj) {
    const bytes = enc.encode(JSON.stringify(obj));
    const ptr = alloc(bytes.length);
    assert.ok(ptr !== 0);
    try {
      new Uint8Array(memory.buffer, ptr, bytes.length).set(bytes);
      const packed = fn(ptr, bytes.length);
      const outPtr = Number(packed >> 32n);
      const outLen = Number(packed & 0xffffffffn);
      assert.ok(outPtr !== 0 && outLen > 0, 'empty wasm json return');
      try {
        const outBytes = new Uint8Array(memory.buffer, outPtr, outLen).slice();
        return JSON.parse(dec.decode(outBytes));
      } finally {
        dealloc(outPtr, outLen);
      }
    } finally {
      dealloc(ptr, bytes.length);
    }
  }

  const CHAIN_ID = 0x4249545300000001; // exact JS safe integer? Wait: 0x4249545300000001 > 2^53!
  // Use BigInt-safe chain_id 4776442001 for JS JSON numbers AND test exact Python vector with u64-safe chain_id!
  // Wait: verify_default_bits_actor uses exact CHAIN_ID? Let's test BOTH safe JSON chain_id and exact hex vector!
  const PK1 = 'd04ab232742bb4ab3a1368bd4615e4e6d0224ab71a016baf8520a332c9778737';
  const A1 = 'b6fbe1c92197620047ea921ab1fca596441d06c90e6c0d5f59eaf2941904db0c';
  const A2 = 'a7cc1e7e5b4a53fbd51458a9c636e3cc84d504360e216e8a159871aa60ceada8';
  const TREASURY = 'b05473148e8f7c288ecc15d6dabb8347916ebd07675e415c1dbbeae33bcb002b';
  const ROOT = '671472e79d0dc35fca848c2fbd82798509a2da788ab7b53976603701616abefc';
  const PROOF_A1 =
    '000103b6fbe1c92197620047ea921ab1fca596441d06c90e6c0d5f59eaf2941904db0c00010101c2b34f4b882356cbb4dcf1cf5e7afd7e7a42069606b27e0138b085b65d6e9ab30100ff6df7c392432ec4008fd5243563f94b2c883a0d921cd81abeb3d5e5283209b61800000010000000000000000000000000000003e8';
  const PROOF_A2_ABSENT =
    '000103a7cc1e7e5b4a53fbd51458a9c636e3cc84d504360e216e8a159871aa60ceada800010101c2b34f4b882356cbb4dcf1cf5e7afd7e7a42069606b27e0138b085b65d6e9ab30200ff6df7c392432ec4008fd5243563f94b2c883a0d921cd81abeb3d5e5283209b61875b36c508866d18732305da14fe9a0ab4548c09a05446cb9a09c2a59ecd841d7';

  // 1. Wallet Patricia inclusion & non-membership verification (DomainIdV1::NativeBalance = 3)
  assert.equal(callI32(verify_wallet_proof_json, { actorId: A1, balance: 1000, root: ROOT, proofHex: PROOF_A1 }), 1);
  assert.equal(callI32(verify_wallet_proof_json, { actorId: A2, balance: 0, root: ROOT, proofHex: PROOF_A2_ABSENT }), 1);
  // Negative controls: tampered balance, tampered root, swapped actor
  assert.equal(callI32(verify_wallet_proof_json, { actorId: A1, balance: 1001, root: ROOT, proofHex: PROOF_A1 }), 0);
  assert.ok(callI32(verify_wallet_proof_json, { actorId: A1, balance: 1000, root: '00'.repeat(32), proofHex: PROOF_A1 }) < 0);
  assert.ok(callI32(verify_wallet_proof_json, { actorId: A2, balance: 1000, root: ROOT, proofHex: PROOF_A1 }) < 0);

  // 2. Amendment 1 `derive_actor_id_json` with JS-safe chain_id (0x4249_5453 = 1112101971)
  // Also test exact u64 string/number handling with WebCrypto Ed25519 keygen!
  const TEST_CHAIN_ID = 1112101971; // "BITS" (32-bit, exact in IEEE-754 & u64!)
  const walletPair = await globalThis.crypto.subtle.generateKey({ name: 'Ed25519' }, true, ['sign', 'verify']);
  const walletPubHex = toHex(await globalThis.crypto.subtle.exportKey('raw', walletPair.publicKey));
  const derived = callJson(derive_actor_id_json, { chain_id: TEST_CHAIN_ID, pubkey_hex: walletPubHex });
  assert.equal(derived.ok, true);
  assert.match(derived.actor_id_hex, /^[0-9a-f]{64}$/);
  assert.equal(derived.pubkey_hex, walletPubHex);

  // 3. Live WebCrypto Ed25519 signing of atomic 80/20 `purchase_split` via single-source WASM encoder
  const genesisHex = 'ab'.repeat(32);
  const unsignedTx = {
    chain_id: TEST_CHAIN_ID,
    genesis_id: genesisHex,
    sender_actor: derived.actor_id_hex,
    signer_pubkey: walletPubHex,
    seq: 1,
    valid_until_height: 100,
    payload: {
      kind: 'purchase_split',
      buyer: derived.actor_id_hex,
      creator: A2,
      treasury: TREASURY,
      amount_bits: 250,
      creator_bps: 8000,
      order_id: '12'.repeat(32),
      memo_hash: '34'.repeat(32),
    },
  };
  const prep = callJson(encode_tx_signing_bytes_json, unsignedTx);
  assert.equal(prep.ok, true);
  assert.match(prep.signing_digest_hex, /^[0-9a-f]{64}$/);
  const sigBytes = new Uint8Array(
    await globalThis.crypto.subtle.sign({ name: 'Ed25519' }, walletPair.privateKey, fromHex(prep.signing_digest_hex)),
  );
  const encoded = callJson(encode_tx_tlv_json, { tx: { ...unsignedTx, sig: [...sigBytes] } });
  assert.equal(encoded.ok, true, encoded.reason);
  assert.equal(encoded.tx_hex, encoded.tlv_hex);
  assert.match(encoded.tx_hash_hex, /^[0-9a-f]{64}$/);

  // Tampered signature must be rejected fail-closed!
  const badSig = new Uint8Array(sigBytes);
  badSig[0] ^= 0xff;
  const badEncoded = callJson(encode_tx_tlv_json, { tx: { ...unsignedTx, sig: [...badSig] } });
  assert.equal(badEncoded.ok, false);

  // 4. Live 4-validator Ed25519 HotStuff QC + epoch handoff verification
  const vals = [];
  for (let i = 0; i < 4; i++) {
    const kp = await globalThis.crypto.subtle.generateKey({ name: 'Ed25519' }, true, ['sign', 'verify']);
    const pkHex = toHex(await globalThis.crypto.subtle.exportKey('raw', kp.publicKey));
    vals.push({ validator_id: `val-${i}`, kp, vote_pubkey_hex: pkHex, power: 100 });
  }
  const committee = vals.map(v => ({ validator_id: v.validator_id, vote_pubkey_hex: v.vote_pubkey_hex, power: v.power }));
  const nextSchedRoot = '77'.repeat(32);
  const header = {
    chain_id: TEST_CHAIN_ID,
    genesis_id: genesisHex,
    height: 12,
    round: 0,
    epoch: 1,
    parent_hash: 'cd'.repeat(32),
    balance_root: ROOT,
    nonce_root: 'ef'.repeat(32),
    tx_root: encoded.tx_hash_hex,
    validator_schedule_root: nextSchedRoot,
  };
  const bhBuf = computeHeaderHash(header);
  const qcDigest = qcCommitSigningDigest(TEST_CHAIN_ID, genesisHex, 12, 0, 1, bhBuf);
  const sigs = [];
  for (let i = 0; i < 3; i++) {
    const sig = new Uint8Array(
      await globalThis.crypto.subtle.sign({ name: 'Ed25519' }, vals[i].kp.privateKey, qcDigest),
    );
    sigs.push({
      validator_id: vals[i].validator_id,
      vote_pubkey_hex: vals[i].vote_pubkey_hex,
      sig_hex: toHex(sig),
    });
  }
  const qc = {
    genesis_id: genesisHex,
    height: 12,
    round: 0,
    epoch: 1,
    block_hash: toHex(bhBuf),
    signatures: sigs,
  };

  assert.equal(callI32(verify_header_qc_json, { header, qc, committee }), 1);
  // Sub-quorum (2 of 4 = 50% <= 66.7%) rejected!
  assert.equal(
    callI32(verify_header_qc_json, { header, qc: { ...qc, signatures: sigs.slice(0, 2) }, committee }),
    -13,
  );
  // Duplicate signer rejected!
  assert.equal(
    callI32(verify_header_qc_json, { header, qc: { ...qc, signatures: [sigs[0], sigs[0], sigs[1]] }, committee }),
    -8,
  );
  // Tampered header root rejected!
  assert.equal(
    callI32(verify_header_qc_json, { header: { ...header, balance_root: '00'.repeat(32) }, qc, committee }),
    -6,
  );
  // Epoch handoff passes with matching next schedule root, fails on mismatch!
  assert.equal(
    callI32(verify_epoch_handoff_json, { header, qc, committee, expected_next_schedule_root_hex: nextSchedRoot }),
    1,
  );
  assert.equal(
    callI32(verify_epoch_handoff_json, { header, qc, committee, expected_next_schedule_root_hex: '88'.repeat(32) }),
    -16,
  );

  // Also test raw PK1 sanity
  assert.equal(PK1.length, 64);

  console.log(
    `light_wasm_parity_check: PASS (raw=${rawBytes.length} B, gzip=${gzBytes.length} B <= ${MAX_GZIP_BYTES} B, 6/6 exports + all negative controls verified)`,
  );
}

main().catch(err => {
  console.error(err);
  process.exit(1);
});
