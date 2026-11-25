// Zero-dependency Node 22 Bits relayer core for the merged Bitblock L1 shadow testnet.
// Uses the compiled `bitblock_light_wasm.wasm` directly as its single-source canonical
// transaction encoder & verifier (zero duplicated codec logic, zero Hashcash PoW, zero
// runtime minting — all testnet grants debit strictly from TestnetGrantPool).

import crypto from 'node:crypto';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const enc = new TextEncoder();
const dec = new TextDecoder();

// Exact 12-byte ASN.1 DER SubjectPublicKeyInfo prefix for raw 32-byte Ed25519 keys.
const ED25519_SPKI_PREFIX = Buffer.from('302a300506032b6570032100', 'hex');
// Exact 16-byte ASN.1 DER PKCS#8 prefix for raw 32-byte Ed25519 secret seeds.
const ED25519_PKCS8_PREFIX = Buffer.from('302e020100300506032b657004220420', 'hex');

export const DEFAULT_WASM_PATH = path.resolve(
  __dirname,
  '../target/wasm32-unknown-unknown/release/bitblock_light_wasm.wasm',
);
export const DEFAULT_MAX_KEY_LOOKUPS_PER_CYCLE = 40;
export const DEFAULT_MAX_KEY_LOOKUPS_PER_HOUR = 100;
export const DEFAULT_K1_POS_TTL_MS = 60 * 60_000;
export const DEFAULT_K1_NEG_TTL_MS = 5 * 60_000;

export function sha256Hex(input) {
  return crypto.createHash('sha256').update(input).digest('hex');
}

export function extractEd25519FromSpkiB64(spkiB64) {
  if (typeof spkiB64 !== 'string' || !spkiB64) return null;
  let der;
  try {
    der = Buffer.from(spkiB64, 'base64');
  } catch {
    return null;
  }
  if (der.length !== 44) return null;
  if (!der.subarray(0, 12).equals(ED25519_SPKI_PREFIX)) return null;
  return der.subarray(12, 44).toString('hex');
}

/** Generate an isolated Ed25519 relayer keypair (stored at 0600 locally). */
export function generateRelayerKey() {
  const { publicKey, privateKey } = crypto.generateKeyPairSync('ed25519');
  const spki = publicKey.export({ type: 'spki', format: 'der' });
  const pkcs8 = privateKey.export({ type: 'pkcs8', format: 'der' });
  return {
    pubkeyHex: spki.subarray(12, 44).toString('hex'),
    pkcs8Hex: pkcs8.toString('hex'),
  };
}

/** Build a relayer key object from a raw 32-byte hex Ed25519 seed (useful for tests/init). */
export function relayerKeyFromSeedHex(seedHex) {
  const seed = Buffer.from(seedHex.trim(), 'hex');
  if (seed.length !== 32) throw new TypeError('seedHex must be 32 bytes');
  const pkcs8 = Buffer.concat([ED25519_PKCS8_PREFIX, seed]);
  const priv = crypto.createPrivateKey({ key: pkcs8, type: 'pkcs8', format: 'der' });
  const pub = crypto.createPublicKey(priv);
  const spki = pub.export({ type: 'spki', format: 'der' });
  return {
    pubkeyHex: spki.subarray(12, 44).toString('hex'),
    pkcs8Hex: pkcs8.toString('hex'),
  };
}

export function loadPrivateKeyObject(pkcs8Hex) {
  return crypto.createPrivateKey({
    key: Buffer.from(pkcs8Hex, 'hex'),
    type: 'pkcs8',
    format: 'der',
  });
}

export function signDigestEd25519(digestHex, pkcs8Hex) {
  const priv = loadPrivateKeyObject(pkcs8Hex);
  const sig = crypto.sign(null, Buffer.from(digestHex, 'hex'), priv);
  return sig.toString('hex');
}

/** Instantiate the compiled `bitblock_light_wasm.wasm` single-source codec & verifier. */
export async function loadLightWasm(wasmPath = DEFAULT_WASM_PATH) {
  const rawBytes = fs.readFileSync(wasmPath);
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

  function callI32(fn, obj) {
    const bytes = enc.encode(JSON.stringify(obj));
    const ptr = alloc(bytes.length);
    if (!ptr) throw new Error('wasm alloc failed');
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
    if (!ptr) throw new Error('wasm alloc failed');
    try {
      new Uint8Array(memory.buffer, ptr, bytes.length).set(bytes);
      const packed = fn(ptr, bytes.length);
      const outPtr = Number(packed >> 32n);
      const outLen = Number(packed & 0xffffffffn);
      if (!outPtr || !outLen) return { ok: false, reason: 'empty wasm return' };
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

  return {
    deriveActorId(chainId, pubkeyHex) {
      return callJson(derive_actor_id_json, { chain_id: chainId, pubkey_hex: pubkeyHex });
    },
    verifyWalletProof(proofObj) {
      return callI32(verify_wallet_proof_json, proofObj);
    },
    verifyHeaderQc(payloadObj) {
      return callI32(verify_header_qc_json, payloadObj);
    },
    verifyEpochHandoff(payloadObj) {
      return callI32(verify_epoch_handoff_json, payloadObj);
    },
    signAndEncodeTx(txWithoutSig, pkcs8Hex) {
      const prep = callJson(encode_tx_signing_bytes_json, txWithoutSig);
      if (!prep.ok) {
        throw new Error(`encode_tx_signing_bytes_json failed: ${prep.reason}`);
      }
      const sigHex = signDigestEd25519(prep.signing_digest_hex, pkcs8Hex);
      const encoded = callJson(encode_tx_tlv_json, {
        tx: { ...txWithoutSig, sig_hex: sigHex },
      });
      if (!encoded.ok) {
        throw new Error(`encode_tx_tlv_json failed: ${encoded.reason}`);
      }
      return {
        txHex: encoded.tx_hex,
        txHashHex: encoded.tx_hash_hex,
      };
    },
  };
}

/**
 * Execute one deterministic Bitblock L1 Bits relayer cycle:
 * 1. Read live L1 `/proof` + Amendment 1 `/proof/wallet/{pubkey}` & `/proof/wallet/{grantPool}`
 * 2. Poll Gameladder `/api/bits/relayer/pending-intents` for signup grants and pre-signed intents
 * 3. Poll Gameladder `/api/market/proof` for audit-only `BitsL2AnchorV1` epoch commitments
 * 4. Submit canonical transactions via `POST /tx` (zero Hashcash PoW, zero runtime minting)
 */
export async function runBitsRelayerCycle({
  gameladderUrl,
  gateKey = '',
  l1RpcUrl,
  relayerKey,
  wasm,
  state,
  appId = 'gameladder',
  fetchImpl = globalThis.fetch,
  nowMs = Date.now(),
}) {
  const glBase = String(gameladderUrl).replace(/\/+$/, '');
  const l1Base = String(l1RpcUrl).replace(/\/+$/, '');
  const glHeaders = gateKey ? { 'x-gate-key': gateKey } : {};

  // 1. Read L1 chain head & system actors.
  const l1Res = await fetchImpl(`${l1Base}/proof`);
  if (!l1Res.ok) {
    return { ok: false, reason: `L1 /proof failed: HTTP ${l1Res.status}` };
  }
  const l1Head = await l1Res.json();
  if (!l1Head.ok || !l1Head.genesis || !l1Head.systemActors) {
    return { ok: false, reason: 'L1 /proof malformed' };
  }

  const chainId = Number(l1Head.chainId);
  const genesisHex = l1Head.genesis;
  const grantPoolActor = l1Head.systemActors.grantPool;
  const l1Height = Number(l1Head.head?.height || 0);

  // Resolve relayer's own canonical ActorIdV1 (Amendment 1) and current sequence numbers.
  const relWalletRes = await fetchImpl(`${l1Base}/proof/wallet/${relayerKey.pubkeyHex}`);
  if (!relWalletRes.ok) {
    return { ok: false, reason: `L1 /proof/wallet/relayer failed: HTTP ${relWalletRes.status}` };
  }
  const relWallet = await relWalletRes.json();
  const relayerActor = relWallet.actorId;
  let relayerSeq = Number(relWallet.seq || 0);

  const gpWalletRes = await fetchImpl(`${l1Base}/proof/wallet/${grantPoolActor}`);
  if (!gpWalletRes.ok) {
    return { ok: false, reason: `L1 /proof/wallet/grantPool failed: HTTP ${gpWalletRes.status}` };
  }
  const gpWallet = await gpWalletRes.json();
  let grantPoolSeq = Number(gpWallet.seq || 0);

  const processedIds = new Set(Array.isArray(state.processedIntentIds) ? state.processedIntentIds : []);
  const submittedTxs = [];
  let signupGrantsCount = 0;
  let signedIntentsCount = 0;
  let lastTxHash = state.lastTxHash || null;

  // 2. Poll Gameladder pending Bits intents (if endpoint exists).
  const intentsRes = await fetchImpl(`${glBase}/api/bits/relayer/pending-intents`, {
    headers: glHeaders,
  });
  if (intentsRes.status === 401 || intentsRes.status === 403) {
    return { ok: false, standbyGate: true, reason: `Gameladder gate active (HTTP ${intentsRes.status})` };
  }
  if (intentsRes.ok) {
    const intentsData = await intentsRes.json();
    const intents = Array.isArray(intentsData.intents) ? intentsData.intents : [];
    for (const intent of intents) {
      const id = String(intent.intentId || intent.grantId || '');
      if (!id || processedIds.has(id)) continue;

      if (intent.kind === 'signup_grant') {
        if (!l1Head.lab) continue;
        const recipientPubkey = String(intent.recipientPubkeyHex || '').toLowerCase();
        if (!/^[0-9a-f]{64}$/.test(recipientPubkey)) continue;

        // Amendment 1: resolve canonical ActorIdV1 via L1 proof door.
        const rwRes = await fetchImpl(`${l1Base}/proof/wallet/${recipientPubkey}`);
        if (!rwRes.ok) continue;
        const rw = await rwRes.json();
        const recipientActor = rw.actorId;
        const grantIdHex = /^[0-9a-f]{64}$/i.test(String(intent.grantId || ''))
          ? String(intent.grantId).toLowerCase()
          : sha256Hex(`bits-signup-grant:${id}`);
        const amountBits = Math.max(1, Math.min(1000, Math.floor(Number(intent.amountBits || 1000))));

        grantPoolSeq += 1;
        const encoded = wasm.signAndEncodeTx(
          {
            chain_id: chainId,
            genesis_id: genesisHex,
            sender_actor: grantPoolActor,
            signer_pubkey: relayerKey.pubkeyHex,
            seq: grantPoolSeq,
            valid_until_height: l1Height + 200,
            payload: {
              kind: 'testnet_grant',
              recipient: recipientActor,
              recipient_pubkey: recipientPubkey,
              amount_bits: amountBits,
              grant_kind: 1, // Signup
              grant_id: grantIdHex,
            },
          },
          relayerKey.pkcs8Hex,
        );

        const postRes = await fetchImpl(`${l1Base}/tx`, {
          method: 'POST',
          headers: { 'content-type': 'application/json' },
          body: JSON.stringify({ tx_hex: encoded.txHex }),
        });
        const postJson = await postRes.json().catch(() => ({ ok: false }));
        if (postRes.status === 202 && postJson.ok) {
          processedIds.add(id);
          signupGrantsCount += 1;
          lastTxHash = postJson.txHash || encoded.txHashHex;
          submittedTxs.push({ intentId: id, kind: 'signup_grant', txHash: lastTxHash });
        } else {
          grantPoolSeq -= 1;
        }
      } else if (intent.kind === 'signed_tx' && typeof intent.txHex === 'string') {
        const postRes = await fetchImpl(`${l1Base}/tx`, {
          method: 'POST',
          headers: { 'content-type': 'application/json' },
          body: JSON.stringify({ tx_hex: intent.txHex }),
        });
        const postJson = await postRes.json().catch(() => ({ ok: false }));
        if (postRes.status === 202 && postJson.ok) {
          processedIds.add(id);
          signedIntentsCount += 1;
          lastTxHash = postJson.txHash;
          submittedTxs.push({ intentId: id, kind: 'signed_tx', txHash: lastTxHash });
        }
      }
    }
  }

  // 3. Poll Gameladder `/api/market/proof` for audit-only `BitsL2AnchorV1` commitment (zero minting!).
  let anchored = false;
  let lastAnchoredBlock = Number(state.lastAnchoredBlock || 0);
  let lastAnchoredEpoch = Number(state.lastAnchoredEpoch || 0);
  let lastAnchoredRoot = state.lastAnchoredRoot || null;
  let anchoredCount = Number(state.anchoredCount || 0);

  const glProofRes = await fetchImpl(`${glBase}/api/market/proof`, { headers: glHeaders });
  if (glProofRes.status === 401 || glProofRes.status === 403) {
    return { ok: false, standbyGate: true, reason: `Gameladder gate active (HTTP ${glProofRes.status})` };
  }
  if (glProofRes.ok) {
    const glProof = await glProofRes.json();
    const glHeight = Number(glProof.head?.l1?.height ?? glProof.head?.count ?? 0);
    const glRoot = glProof.head?.l1?.head || glProof.head?.root || '';
    const eventCount = Math.max(0, Math.floor(Number(glProof.head?.eventCount ?? glProof.head?.walletCount ?? 0)));

    if (glHeight > lastAnchoredBlock && /^[0-9a-f]{64}$/i.test(glRoot)) {
      const nextEpoch = lastAnchoredEpoch + 1;
      relayerSeq += 1;
      const encoded = wasm.signAndEncodeTx(
        {
          chain_id: chainId,
          genesis_id: genesisHex,
          sender_actor: relayerActor,
          signer_pubkey: relayerKey.pubkeyHex,
          seq: relayerSeq,
          valid_until_height: l1Height + 200,
          payload: {
            kind: 'l2_anchor',
            app_id_hash: sha256Hex(appId),
            epoch_id: nextEpoch,
            l2_root: glRoot.toLowerCase(),
            event_count: eventCount,
          },
        },
        relayerKey.pkcs8Hex,
      );
      const postRes = await fetchImpl(`${l1Base}/tx`, {
        method: 'POST',
        headers: { 'content-type': 'application/json' },
        body: JSON.stringify({ tx_hex: encoded.txHex }),
      });
      const postJson = await postRes.json().catch(() => ({ ok: false }));
      if (postRes.status === 202 && postJson.ok) {
        anchored = true;
        lastAnchoredBlock = glHeight;
        lastAnchoredEpoch = nextEpoch;
        lastAnchoredRoot = glRoot.toLowerCase();
        anchoredCount += 1;
        lastTxHash = postJson.txHash || encoded.txHashHex;
        submittedTxs.push({ intentId: `anchor-${nextEpoch}`, kind: 'l2_anchor', txHash: lastTxHash });
      }
    }
  }

  const nextState = {
    ...state,
    lastAnchoredBlock,
    lastAnchoredEpoch,
    lastAnchoredRoot,
    anchoredCount,
    signupGrantsRelayed: Number(state.signupGrantsRelayed || 0) + signupGrantsCount,
    intentsRelayed: Number(state.intentsRelayed || 0) + signedIntentsCount,
    lastTxHash,
    lastCycleAtMs: nowMs,
    processedIntentIds: [...processedIds].slice(-4096),
  };

  return {
    ok: true,
    anchored,
    signupGrantsCount,
    signedIntentsCount,
    submittedTxs,
    lastTxHash,
    nextState,
  };
}
