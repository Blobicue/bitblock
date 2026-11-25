# Bitblock + `rngcoin-node` Single-Chain Merge Design (Phase 0 Contract)

- **Status:** Phase 0 Design Contract — **Testnet / Devnet Only (`lab: true`)**
- **Owner:** Bitblock session (`~/bitblock`, `Blobicue/bitblock`)
- **Counterparties for sign-off before Phase 1 code:** Gameladder session (`~/gameladder-1`, `gameladder.io`) and Bitblock Studio session (`~/bitblock-frontier`)
- **Baselines pinned:**
- `~/bitblock` `origin/main` commit `6aef32e7527eacebdf4e288476e3a9c1de7e64ef` (`100` consecutive 10/10-CI-green PRs `#4`–`#103`, `200/200` independently vectored protocol schemas, `262` Lean 4 `--trust=0` refinement/safety/launch/activation theorems across `11` modules, `100%` `bbmutate` kill rate across `75` modules, `TESTNET-1@base=BOUNDED`, `PROD-OPS-1@base=READY`)
- `~/rngcoin-node` `origin/main` commit `f31e326` (4-validator Docker Compose + Caddy LB + `cloudflared` named tunnel + Node 22 Gameladder `relayer` sidecar + `gameladder-1/web/l1.js` verifier)

---

## 1. Purpose & Hard Boundaries

Per Owen's 2026-11-24 platform architecture (`~/platform/PLATFORM.md`), **Gameladder** is the browser game platform, **Bitblock Studio** is the browser creation tool and engine, and **the merged Bitblock chain** is the single authoritative currency ledger (**Bits**, `1 Bit = 1 base unit`) that creators earn and players spend.

**Bitblock L1** and **`rngcoin-node`** converge into **ONE chain**:
- **Bitblock L1** provides the canonical protocol, state machine, cryptography, authentication, consensus, upgrade/launch closure, Python vector verification, and Lean 4 formal proofs.
- **`rngcoin-node`** contributes its proven operational runtime and deployment shell (`libp2p` WAN transport, `redb` ACID persistence, `axum` CORS HTTP proof doors, Docker Compose + Caddy + `cloudflared` stack, and the Node 22 relayer sidecar), after which `rngcoin-node` is retired as the deprecated testnet predecessor.

### Mandatory Non-Negotiable Boundaries (Testnet / Devnet Only)

1. **No real value:** Every genesis, faucet/grant pool, purchase split, and simulated payout receipt specified in this design is strictly `lab: true` testnet/devnet only, explicitly labeled **`DEVNET-TEST-KEYS-NOT-PRODUCTION-CUSTODY`**.
2. **Excluded capabilities:** This merge design does **not** promote or authorize `L1-PRODUCTION`, `VALUE-1`, mainnet genesis, real-money purchases, real-money creator payouts, payment providers, or exchange listings. Those remain locked in [`CAPABILITY_TREE.md`](CAPABILITY_TREE.md) and parked exclusively for Owen.
3. **Single source of truth:** Once the merged testnet is live, the chain ledger is the sole source of truth for Bits balances. Gameladder's Durable Object economy and Bitblock Studio's local JSON ledger (`tools/economy.mjs`) operate as read/write-through caches and transaction clients — never independent minting ledgers. **Gameladder never mints.**
4. **Secrets & governance:** All credentials, API keys, Cloudflare tunnel tokens, billing, DNS, and repository rulesets remain exclusively in Owen's hands. Zero private keys ever reside in Cloudflare Workers.
5. **Original IP only:** Every schema, UI label, identifier, and doc strictly uses original Bitblock / Gameladder / Bits terminology.
6. **Standing engineering discipline:** Every implementation PR lands only after **10/10 GitHub Actions CI green**, preserves zero diffs on frozen Bitblock crates/proofs, maintains `100%` independently vectored protocol schemas and `100%` `bbmutate` kill coverage, and visually verifies every browser/chart artifact.

---

## 2. System Architecture & Component Contribution Matrix

```
+------------------------------------+       +------------------------------------------+
| Bitblock Studio                    |       | Gameladder (gameladder.io)               |
| ~/bitblock-frontier                |       | ~/gameladder-1                           |
| - Browser creation tool & engine   | ----> | - Accounts, discovery, play hosting      |
| - tools/economy.mjs (cache/client) |       | - Durable Object economy (cache/client)  |
+------------------------------------+       | - K1 Ed25519 key registry (/api/market/) |
                                             | - /l1.html + bitblock-light-wasm         |
                                             +---------------------+--------------------+
                                                                   |
                          Poll /api/market/proof* & /keys/*        | GET /proof* & local Ed25519
                          (zero keys in Cloudflare Workers)        | QC + Patricia verification
                                                                   v
+------------------------------------+       +------------------------------------------+
| Relayer Sidecar (~/bitblock/node/) | ----> | Merged Bitblock Testnet Node             |
| - Isolated local Ed25519 key 0600  | POST  | ~/bitblock/node/ (isolated sub-workspace)|
| - Bounded K1 cache & rate budget   | /tx   | - axum CORS proof doors + /tx + /faucet  |
| - Submits canonical Bits txs       |       | - libp2p QUIC/TCP + Gossipsub + Kademlia |
+------------------------------------+       | - redb ACID store + SIGNER-OPS-1 WAL     |
                                             | - Docker Compose + Caddy LB + cloudflared|
                                             +---------------------+--------------------+
                                                                   | path deps
                                                                   v
                                             +------------------------------------------+
                                             | Verified Bitblock L1 Protocol Core       |
                                             | ~/bitblock/crates/{core,research}        |
                                             | - bits_economy_1 (80/20, GrantPool, etc.)|
                                             | - ASSET-1, IDENTITY-1, AUTH-COMMIT-1     |
                                             | - CONSENSUS-1, SIGNER-OPS-1, CLIENT-1    |
                                             | - CHAIN-LAUNCH-1, UPGRADE-1, TESTNET-1   |
                                             | - 200+ Python vectors + 262 Lean proofs  |
                                             +------------------------------------------+
```

### Exact Contribution & Retirement Matrix

| Subsystem | Source | Disposition in Merged Chain |
|---|---|---|
| Canonical big-endian codecs, `ContentTag`, `bbmutate`, clean-room Python vectors | `~/bitblock` (`crates/core`, `crates/research`, `tools/*_vectors.py`) | **KEPT (authoritative)** — single canonical wire format across node, relayer, and browser WASM |
| Cryptography (`CRYPTO-D1`, `CRYPTO-1`) | `~/bitblock` (`crypto_d1.rs`, `crypto_1.rs`) | **KEPT (authoritative)** — strict RFC 8032 Ed25519 + SHA-256, 9 role-separated signing domains |
| Identity, addressing & delegation (`IDENTITY-D1`, `IDENTITY-1`) | `~/bitblock` (`identity_d1.rs`, `identity_1.rs`) | **KEPT (authoritative)** — canonical `ActorIdV1`, sequential nonces, depth-1 scoped delegation, 2-step timelocked recovery |
| Authenticated state tries (`AUTH-COMMIT-1`, `ASSET-1`, `EXECUTION-ROOT-1`) | `~/bitblock` (`auth_commit_1.rs`, `asset_1.rs`, `execution_root_1.rs`) | **KEPT (authoritative)** — 256-bit domain-separated binary Patricia tries (`DomainIdV1`), closed supply conservation, voxel occupancy deposits |
| PoS validator economics & schedule (`ECONOMIC-STATE-1`, `VALIDATOR-ECON-1`, `VALIDATOR-SCHEDULE-1`) | `~/bitblock` | **KEPT (authoritative)** — bonded stake, 21-epoch unbonding, pro-rata epoch rewards, permanent equivocation jail & slash |
| BFT consensus & anti-double-sign signer (`CONSENSUS-1`, `SIGNER-OPS-1`, `CLIENT-1`) | `~/bitblock` (`consensus_1.rs`, `signer_ops_1.rs`, `client_1.rs`) | **KEPT (authoritative)** — 2-phase Ed25519 HotStuff BFT, kernel-locked signer watermarks (`flock` / `share_mode(0)`), 6-state finality, Lean 4 `ConsensusSafety.lean` |
| Launch closure, upgrade & successor activation (`PROFILE-1`, `RELEASE-1`, `CHAIN-LAUNCH-1`, `UPGRADE-1`, `SUCCESSOR-ACTIVATION-1`) | `~/bitblock` | **KEPT (authoritative)** — Lean-verified launch & `v1 -> v2` successor activation (`CE-44`) |
| Bits creator economy (`bits_economy_1`) | **NEW** additive module in `~/bitblock/crates/research/` | **ADDED** — `80/20` `PurchaseSplit`, genesis-funded `TestnetGrantPool`, simulated `PayoutReceipt`, `IdentityBind`, optional `L2Anchor` (`wasm32`-clean, zero new deps) |
| WAN P2P networking (`libp2p` QUIC + TCP/Noise/Yamux + Gossipsub + Kademlia + Identify) | `~/rngcoin-node` (`crates/rng-net`) | **ADOPTED** into isolated `~/bitblock/node/` sub-workspace |
| Crash-safe ACID persistence (`redb` atomic block + state + tx index + WAL) | `~/rngcoin-node` (`crates/rng-store`) | **ADOPTED** into `~/bitblock/node/` paired with Bitblock's `SIGNER-OPS-1` kernel-locked watermark |
| CORS HTTP proof doors (`/health`, `/metrics`, `/proof*`, `POST /tx`, `POST /faucet`) + backpressure/rate-limit shedding | `~/rngcoin-node` (`crates/rng-node`) | **ADOPTED** into `~/bitblock/node/` serving versioned Bitblock payloads |
| Deployment stack (Docker Compose 4-validator cluster, Caddy LB, `cloudflared` tunnel, systemd runbook) | `~/rngcoin-node` (`docker-compose.testnet.yml`, `deploy/`, `scripts/`) | **ADOPTED** into `~/bitblock/node/deploy/` |
| Gameladder relayer sidecar (`relayer/index.mjs`, `relayer/lib.mjs`) | `~/rngcoin-node` (`relayer/`) | **ADOPTED & UPDATED** to encode/submit canonical Bitblock Bits transactions while preserving zero-key Worker isolation and bounded K1 lookup cache |
| Browser light-client WASM loader (`gameladder-1/web/l1-wasm.js` / `l1-light.wasm`) | `~/rngcoin-node` (`crates/rng-light-wasm`) | **REPLACED** with compact `bitblock-light-wasm` verifying Ed25519 `ConsensusCommitCertificateV1` + `DomainPatriciaProofV1` + Bitblock canonical tx encoding |
| BIP-39 non-custodial wallet CLI | `~/rngcoin-node` (`crates/rng-cli`) | **ADOPTED (optional Phase 1/2)** inside `~/bitblock/node/` emitting `IDENTITY-1` Ed25519 keys |
| `rng-codec` LEB128-TLV & `rng-types` 14 RNGcoin opcodes | `~/rngcoin-node` | **RETIRED** after Phase 2 cutover |
| `bls12_381` aggregate QCs & Feldman DKG threshold beacon (`DkgDeal`, `DkgComplaint`) | `~/rngcoin-node` (`crates/rng-crypto`, `crates/rng-bft`) | **RETIRED** — replaced 100% by Bitblock's Lean-verified strict Ed25519 HotStuff BFT |
| `RngWager` (Law L9 casino), `GamePoolCreate/Buy/Sell` (50/50 AMM), `PlayStake`, `SeatBond`, `allowanceFold` daily minting, 1% transfer tax, keyless reserve | `~/rngcoin-node` (`crates/rng-state`) | **RETIRED** — incompatible with clean `80/20` Bits creator economy and "gameladder never mints" |

---

## 3. Canonical Bits Creator-Economy Protocol Specification (`bits_economy_1`)

All Bits protocol types live in a new additive, `wasm32`-clean, zero-new-dependency module `crates/research/src/bits_economy_1.rs` (strictly behind `--features protocol-research`), accompanied by clean-room Python vectors (`tools/bits_economy_1_vectors.py`), integration tests (`tests/wave34_bits_economy_1.rs`), and `100%` `bbmutate` kill coverage.

### 3.1 Unit, Conservation Law & Genesis Pools

- **Base unit:** `1 Bit = 1` integer base unit (`u128` checked arithmetic everywhere; zero floats).
- **Zero transfer tax:** Direct `BitsTransferV1` transfers exact `amount` from sender to recipient with zero hidden spread or percentage tax.
- **Closed `ASSET-1` supply conservation (zero runtime minting):**
  Every block enforces exact closed conservation across `DomainIdV1::NativeBalance` (`3`) and the fixed protocol system accounts:
  ```
  sum(liquid_user_balances)
    + testnet_grant_pool_balance
    + platform_treasury_balance
    + simulated_payout_escrow_balance
    + total_locked_voxel_deposits
    + total_burned_fees
    == genesis_supply
  ```
- **Why Gameladder never mints:** Signup bonuses (`1,000` Bits default in `bitblock-frontier/tools/economy.mjs`) and `/faucet` test drips are **transfers out of the fixed genesis-funded `TestnetGrantPool`**, gated strictly by `BitsGenesisConfigV1.lab == true`, per-account caps, and rate windows. No runtime transaction or relayer message can increase `genesis_supply`.

### 3.2 Canonical Transaction Kinds (`BitsTxPayloadV1`)

Every transaction is signed under Bitblock's non-circular `IdentityContextV1` (`protocol_version`, `chain_id`, `genesis_id`, `launch_manifest_id`, `profile_id`, `world_tag`, `SigningDomainTagV1`, `Ed25519StrictRfc8032V1`, `Sha256V1`) with sequential `IDENTITY-1` nonces and height validity windows:

| Opcode | Schema String | Payload Fields (Big-Endian Canonical) | Invariants & Authority |
|---|---|---|---|
| `0x01` | `bitblock/bits-1/transfer/v1` | `from: ActorIdV1`, `to: ActorIdV1`, `amount: u128`, `memo_digest: [u8; 32]` | `amount > 0`, `from != to`, signed by `from` (or valid depth-1 delegate with `Transfer` scope). Exact `amount` moved; zero tax. |
| `0x02` | `bitblock/bits-1/purchase-split/v1` | `buyer: ActorIdV1`, `creator: ActorIdV1`, `price_bits: u128`, `world_id_hash: [u8; 32]`, `item_slug_hash: [u8; 32]`, `order_id: [u8; 32]` | `price_bits >= 10` (matches Studio minimum), `buyer != creator`. Atomic integer split: `creator_share = (price_bits * 80) / 100`, `treasury_share = price_bits - creator_share` (`creator_share + treasury_share == price_bits` always). Signed by `buyer` or bounded depth-1 relayer delegate with `Purchase` scope and per-tx/per-epoch spend cap. Rejects duplicate `order_id`. |
| `0x03` | `bitblock/bits-1/testnet-grant/v1` | `recipient: ActorIdV1`, `amount: u128`, `grant_kind: u8` (`1=Signup`, `2=Faucet`, `3=CarryOver`), `grant_id: [u8; 32]` | Requires `lab == true`. Transfers strictly from `TestnetGrantPool` -> `recipient`. Enforces `amount <= max_single_grant` (`1,000` Bits signup / `50` Bits faucet), deduplicates `grant_id`, and rejects fail-closed if `TestnetGrantPool < amount`. Zero minting. |
| `0x04` | `bitblock/bits-1/simulated-payout/v1` | `creator: ActorIdV1`, `amount: u128`, `payout_request_id: [u8; 32]` | Requires `lab == true`. `amount >= 100` Bits. Transfers `creator -> simulated_payout_escrow` and emits canonical `BitsSimulatedPayoutReceiptV1` stamped with `SIMULATED-TEST-ONLY-NO-REAL-MONEY`. Never interacts with any payment processor. |
| `0x05` | `bitblock/bits-1/identity-bind/v1` | `actor_id: ActorIdV1`, `ed25519_pk: CanonicalEd25519PublicKeyV1`, `provider: u8` (`1=GameladderPlayerHash`, `2=StudioHandle`, `3=Discord`), `external_id_hash: [u8; 32]` | Signed by the `ActorAdmin` key for `actor_id` AND proof-of-possession signed by `ed25519_pk` under domain `bb.bits1.identity_bind.v1`. Registers the reverse lookup `ed25519_pk -> actor_id` and `(provider, external_id_hash) -> actor_id`. |
| `0x06` | `bitblock/bits-1/l2-anchor/v1` | `app_id_hash: [u8; 32]`, `epoch_id: u64`, `l2_root: [u8; 32]`, `event_count: u32` | Strictly audit-only epoch commitment (monotone `epoch_id` per `app_id_hash`, duplicate rejected). Carries **zero minting power** and **zero balance claims** — all balance changes occur explicitly through opcodes `0x01`–`0x04`. |

### 3.3 Account Addressing & Optional Scoped Relayer Delegation

1. **Canonical Address (`ActorIdV1`):** Derived deterministically via `IDENTITY-D1` from `(chain_id, "bits.gameladder.v1", initial_authorization_descriptor)`. Because `ActorIdV1` is decoupled from any single raw public key, a player or creator can rotate a compromised device key or invoke 2-step timelocked recovery (`IDENTITY-1`) without losing their handle mapping or Bits balance.
2. **Raw Ed25519 Lookup Alias:** Existing Gameladder K1 keys are 44-byte SPKI DER -> raw 32-byte Ed25519 public keys (`extractEd25519FromSpkiB64`). Every HTTP proof door (`/proof/wallet/{addr}`) accepts **either** a 64-char hex `ActorIdV1` **or** a 64-char hex raw Ed25519 K1 pubkey (resolving via the on-chain `BitsIdentityBindV1` index and returning both `actorId` and `ed25519Hex`).
3. **Non-Custodial Client Signing + Optional Depth-1 Scoped Delegation:**
   - **Direct self-custody (default):** Browser client signs `BitsTransferV1`, `BitsPurchaseSplitV1`, `BitsSimulatedPayoutV1`, or `BitsIdentityBindV1` locally via WebCrypto Ed25519 + `bitblock-light-wasm` and submits via Gameladder or directly to `POST /tx`.
   - **Smooth in-game checkout (optional):** A K1-keyed player may sign one `IDENTITY-1` **non-transitive depth-1 `DelegationGrantV1`** granting the local relayer's Ed25519 public key permission to submit `BitsPurchaseSplitV1` up to an explicit `max_bits_per_tx`, `max_bits_total`, and `expiry_height`. The relayer never holds the player's private key, cannot transfer out of scope, cannot sub-delegate (`A -> B -> C` rejected fail-closed), and loses authority automatically at `expiry_height` or upon one-click revocation.

---

## 4. Consensus, Cryptography & Browser `wasm32` Light-Client Contract

### 4.1 Consensus & Signer Safety

- **Protocol:** Bitblock `CONSENSUS-1` 2-phase HotStuff BFT (`PrepareQC` + `CommitQC`) over strict RFC 8032 Ed25519 + SHA-256 (`CRYPTO-D1`), backed by Lean 4 `ConsensusSafety.lean` (`--trust=0`).
- **Retired crypto:** Zero `bls12_381` pairings, zero Feldman DKG dealings/complaints, zero threshold randomness beacon.
- **Anti-double-sign durability:** Every validator daemon pairs `redb` ACID block commits with Bitblock's `SIGNER-OPS-1` kernel-locked monotonic watermark file (`flock` on Linux, `share_mode(0)` on Windows) so a restarted or duplicated validator process refuses equivocating votes fail-closed.

### 4.2 Browser `wasm32` Light-Client ABI Swap (`bitblock-light-wasm`)

`gameladder-1/web/l1-wasm.js` currently loads `/l1-light.wasm` compiled from `rngcoin-node/crates/rng-light-wasm`. Phase 1 ships a compact zero-bindgen `bitblock-light-wasm` binary (compiled from `crates/core` + `crates/research` under `wasm32-unknown-unknown`, `panic=abort`, `opt-level="s"`) preserving the exact memory allocation & JSON C-ABI bridge (`alloc`, `dealloc`, packed `(ptr << 32) | len` return values) so `gameladder-1/web/l1-wasm.js` needs only a targeted swap:

| Export Name | Previous (`rng-light-wasm`) | Merged (`bitblock-light-wasm`) |
|---|---|---|
| `alloc` / `dealloc` / `memory` | Linear-memory JSON buffer bridge | **Unchanged** |
| `verify_wallet_proof_json` | Verifies `rng-crypto` `WalletTree` Merkle proof | Verifies Bitblock `AUTH-COMMIT-1` `DomainPatriciaProofV1` (`DomainIdV1::NativeBalance = 3`) against header `native_balance_root` |
| `verify_header_qc_json` | Verifies BLS12-381 aggregate QC | Verifies strict RFC 8032 Ed25519 `ConsensusCommitCertificateV1` (`>= 2/3` active validator weight, distinct sorted signers, domain-bound preimage) |
| `verify_epoch_handoff_json` | Verifies BLS group PK handoff | Verifies `VALIDATOR-SCHEDULE-1` delayed epoch committee handoff committed in `validator_schedule_root` |
| `encode_tx_signing_bytes_json` | Encodes `rng-l1-tx/v1` LEB128-TLV signing digest | Encodes Bitblock `IdentityContextV1` + `BitsTxPayloadV1` canonical big-endian signing preimage |
| `encode_tx_tlv_json` | Encodes signed LEB128-TLV + Hashcash PoW | Encodes canonical signed Bitblock wire bytes (`tx_hex` + `tlv_hex` alias, `pow_nonce = 0`) |
| `verify_beacon_json` | Verifies BLS12-381 threshold beacon | **Removed** (retired with `RngWager`) |
| `verify_l9_settlement` | Checks Law L9 wager seal delay | **Removed** (retired with `RngWager`) |

---

## 5. HTTP Proof-Door & Relayer Interface Contract

To minimize churn in `gameladder-1` and keep monitoring/LB healthchecks seamless, `~/bitblock/node/` preserves the exact HTTP route paths from `rngcoin-node`'s `axum` server with versioned Bitblock JSON payloads:

| Route | Method | Response / Request Contract |
|---|---|---|
| `/health` | `GET` | `{ ok, protocol: "bitblock-l1-v1", nodeId, network, lab, height, round, peers, mempoolLen, genesisTag }` |
| `/metrics` | `GET` | Prometheus text exposition (`bitblock_chain_height`, `bitblock_bft_round`, `bitblock_peers_connected`, `bitblock_mempool_len`, `bitblock_grant_pool_remaining`, `bitblock_treasury_balance`, `bitblock_signer_wal_writes_total`, `bitblock_http_rate_limited_total`, `bitblock_http_shed_total`) |
| `/proof` | `GET` | `{ ok: true, protocol: "bitblock-l1-v1", genesis, genesisTag, launchManifestId, profileId, network, lab: true, custodyLabel: "DEVNET-TEST-KEYS-NOT-PRODUCTION-CUSTODY", unit: { name: "Bits", symbol: "BITS", baseUnit: 1, creatorSplitBps: 8000, treasurySplitBps: 2000 }, head: { height, hash, root, walletRoot, walletCount, grantPoolRemaining, treasuryBalance, simulatedPayoutEscrow, genesisSupply, committeeEpoch, l1: { height, head }, qc, header } }` |
| `/proof/blocks?from=&limit=` | `GET` | `{ ok: true, height, head, blocks: [{ k, hash, parent, txCount, l2Count, timestampOrHeight, root }] }` (served from hot in-RAM ring buffer + `redb` fallback) |
| `/proof/block/{k}` | `GET` | `{ ok: true, block, qc }` (canonical Bitblock block + Ed25519 `ConsensusCommitCertificateV1`) |
| `/proof/tx/{hash}` | `GET` | `{ ok: true, height, txHash, tx, receipt }` |
| `/proof/wallet/{addr}` | `GET` | Accepts 64-char hex `ActorIdV1` **or** raw 32-byte Ed25519 K1 pubkey hex. Returns `{ ok: true, actorId, ed25519Hex, balance, seq, proof, root, domainId: 3 }` (404 with current `root` + non-inclusion Patricia proof when absent) |
| `/proof/validators` | `GET` | `{ ok: true, committeeEpoch, committee: [{ validatorId, votePubkeyHex, proposalPubkeyHex, power, bonded, blocksSigned, slashed }], allValidators }` |
| `/tx` | `POST` | Accepts `{ tx_hex }` (canonical Bitblock signed bytes; accepts `{ tlv_hex }` as a transitional alias during cutover). Returns `202 Accepted` `{ ok: true, txHash }` or `400` `{ ok: false, reason }` |
| `/faucet` | `POST` | Strictly gated on `lab == true` (returns `403` when `lab == false`). Accepts `{ addr_hex, amount }` (or legacy `{ pubkey_hex, amount }`), enforces per-key cooldown & global sliding window, submits `BitsTestnetGrantV1(Faucet)` drawing from `TestnetGrantPool` (zero minting) |

### What the Gameladder Relayer Needs (`relayer/lib.mjs` Delta)

The Gameladder session keeps the relayer working through the switch by adapting four bounded surfaces in `relayer/lib.mjs` (zero change to Cloudflare Worker secrets or `/api/market/*` auth):
1. **Single-source tx encoding:** Replace the manual `rng-l1-tx/v1` LEB128-TLV builder (`buildSignedL2AnchorTx`) with Bitblock's canonical big-endian encoder (either calling `bitblock-light-wasm` directly in Node 22 via `loadL1Wasm` — zero codec drift — or a mirrored pure-JS canonical encoder tested against `tools/bits_economy_1_vectors.py`).
2. **Explicit Bits transaction dispatch:** Instead of piggybacking balance mint claims inside `L2Anchor`, the relayer reads Gameladder's event queue / economy deltas and dispatches:
   - `BitsIdentityBindV1` when a player registers a K1 Ed25519 key (signed client-side and relayed verbatim),
   - `BitsTestnetGrantV1(Signup)` for eligible new testnet accounts (deduplicated by `grant_id`, bounded by `TestnetGrantPool`),
   - `BitsPurchaseSplitV1` for pass/item/marketplace purchases (either relaying the player's K1-signed tx or signing via the player's active depth-1 `DelegationGrantV1`),
   - `BitsSimulatedPayoutV1` for creator testnet payout requests (relayed player-signed tx),
   - `BitsL2AnchorV1` for optional periodic Gameladder archive root checkpoints (zero minting).
3. **Nonce & sequence tracking:** Query `GET /proof/wallet/{actorId_or_pubkey}` for `seq`, keeping the existing self-healing expected-nonce retry and atomic state persistence (`relayer-state.json` `0600`).
4. **Drop Hashcash PoW:** Remove `solvePowBits` / `leadingZeroBits` mining loop; anti-spam is enforced by `IDENTITY-1` sequential nonces, bounded tx sizes, and `axum` concurrency/rate limits.

### Gameladder & Studio Economy Cache Interface (`PLATFORM.md` Contract)

Both Gameladder's Durable Object economy and Bitblock Studio's `tools/economy.mjs` converge on one tiny client interface so switching from local/DO state to the merged testnet is a backend swap, not a UI rewrite:
- `getBalance(actorOrPubkey)` -> `GET /proof/wallet/{addr}` (cached locally with `height` + `root` watermark)
- `submitTransfer(signedTxHex)` / `submitPurchaseSplit(signedTxHex)` / `submitSimulatedPayout(signedTxHex)` -> `POST /tx`
- `requestTestnetGrant(actorOrPubkey, amount)` -> `POST /faucet` (testnet `lab: true` only)
- `getTxStatus(txHash)` -> `GET /proof/tx/{hash}` (mapped to Bitblock `CLIENT-1` 6-state finality)

---

## 6. Workspace Isolation, Dependency Hygiene & CI Protection

To keep Bitblock's verified protocol core ultra-fast, minimal-dependency, and `wasm32`-clean while adopting `rngcoin-node`'s heavy native runtime stack:

```
~/bitblock/
├── Cargo.toml                  # Root workspace (bitblock-core, bitblock-research, bitblock-wasm, bitblock) — UNBLOATED
├── Cargo.lock                  # Root lockfile stays minimal (zero tokio/axum/libp2p/redb)
├── crates/
│   ├── core/                   # FROZEN (0 diffs)
│   ├── research/               # Additive pure-protocol bits_economy_1.rs (wasm32-clean, zero new deps)
│   └── wasm/                   # Existing ABI-17 canvas client (untouched) + optional bitblock-light-wasm crate
├── docs/
│   └── CHAIN_MERGE_DESIGN.md   # THIS CONTRACT (Phase 0, PR #104)
├── proof/                      # Existing 11 Lean 4 modules FROZEN (0 diffs)
└── node/                       # ISOLATED sub-workspace (separate Cargo.toml & Cargo.lock; NOT in root members)
    ├── Cargo.toml              # tokio, axum, libp2p, redb, clap, tracing, serde_json -> path deps to ../crates/{core,research}
    ├── Cargo.lock              # Isolated native daemon lockfile
    ├── crates/
    │   ├── bitblock-net/       # libp2p QUIC/TCP/Noise/Yamux/Gossipsub/Kademlia (adapted from rng-net)
    │   ├── bitblock-store/     # redb ACID store + SIGNER-OPS-1 kernel-locked watermark (adapted from rng-store)
    │   ├── bitblock-node/      # Full/validator daemon + axum CORS proof doors (adapted from rng-node)
    │   └── bitblock-light-wasm/# Compact wasm32 browser/Node light verifier (replaces rng-light-wasm)
    ├── relayer/                # Updated zero-dep Node 22 Gameladder relayer sidecar
    └── deploy/                 # docker-compose.testnet.yml, Caddyfile.lb, cloudflared template, runbook
```

- **Why an isolated `node/` sub-workspace wins:**
  1. Root `cargo test --workspace`, `wasm32` builds, `bbindex`, `bbverify`, `bbmutate`, and Lean 4 CI stay 100% deterministic, fast, and unaffected by `libp2p`/`tokio`/`redb` transitive dependencies.
  2. `node/` gets its own dedicated CI job (added in Phase 1 PR `#107`) alongside the 5 existing required CI checks, keeping every PR **10/10+ CI green**.
  3. All frozen Bitblock surfaces (`crates/core/**`, `src/main.rs`, `src/app/**`, 14 frozen research protocol modules, all 11 prior Lean 4 proof modules) remain strictly untouched.

---

## 7. Phased Migration & Cutover Plan (Zero-Downtime Testnet Switch)

### Phase 0 — Written Merge Contract & Cross-Session Sign-Off (**Current PR `#104` — Zero Code**)
- **Owner:** Bitblock session (`a31a4bf9`).
- **Deliverables:**
  1. Land `docs/CHAIN_MERGE_DESIGN.md` + regenerated `index/` in `Blobicue/bitblock` via PR `#104` (`10/10` CI green).
  2. Append a dated cross-session proposal under `## Proposals` in `~/platform/PLATFORM.md`.
- **Exit gate (Hard Stop):** Both the Gameladder session (`1db6dc30`) and Bitblock Studio session (`0a466d62`) review and confirm the HTTP proof-door JSON shapes, `bitblock-light-wasm` C-ABI, and relayer contract before Phase 1 code starts.

### Phase 1 — Parallel Implementation & Shadow Testnet (PRs `#105`–`#108`, Gated on Phase 0 Sign-Off)
1. **PR `#105` — Pure Bits Protocol (`bits_economy_1`):**
   - Add `crates/research/src/bits_economy_1.rs`, `tools/bits_economy_1_vectors.py`, and `tests/wave34_bits_economy_1.rs` (`206/206` vectored schemas, `100%` `bbmutate` kill, `wasm32` clippy clean, `10/10` CI green).
2. **PR `#106` — Compact Browser Light Verifier (`bitblock-light-wasm`):**
   - Build `bitblock-light-wasm` verifying Ed25519 `ConsensusCommitCertificateV1` + `DomainPatriciaProofV1` + canonical Bits tx encoding, with automated Node 22 V8 parity harness and live Chrome visual verification screenshot.
3. **PR `#107` — Isolated Operational Shell (`~/bitblock/node/`):**
   - Port `libp2p` (`bitblock-net`), `redb` + `SIGNER-OPS-1` WAL (`bitblock-store`), `axum` CORS proof doors (`bitblock-node`), and Docker Compose / Caddy / `cloudflared` stack; verify 4-node P2P consensus, crash/restart recovery, equivocation refusal, and proof-door HTTP contract in CI.
4. **PR `#108` — Shadow Relayer & Chrome `/l1.html` Verification:**
   - Stand up the merged 4-validator Bitblock shadow testnet alongside the running `rngcoin-node` testnet on a separate local/staging port (`127.0.0.1:8640` / shadow tunnel).
   - Run the updated relayer in shadow mode against Gameladder's read-only proof doors, verifying `BitsIdentityBindV1`, `BitsTestnetGrantV1`, `BitsPurchaseSplitV1` (`80/20`), and `BitsSimulatedPayoutV1`.
   - Visually verify Gameladder `/l1.html?rpc=http://127.0.0.1:8640` in Chrome showing green Ed25519 QC verification, Patricia wallet proof verification, and Bits economy telemetry.

### Phase 2 — Testnet Cutover & `rngcoin-node` Retirement (Coordinated with Gameladder Session)
1. **Clean-slate Bits testnet genesis (`lab: true`):**
   - Initialize the fresh 4-validator merged Bitblock testnet genesis with a fixed genesis-funded `TestnetGrantPool` (`DEVNET-TEST-KEYS-NOT-PRODUCTION-CUSTODY`).
   - *Optional carry-over:* Before flipping traffic, snapshot K1-linked positive testnet wallet balances from `rngcoin-node` `GET /proof/wallet/*` into deterministic `BitsTestnetGrantV1(CarryOver)` allocations (dropping all legacy casino/AMM/BLS state).
2. **Switch Gameladder & Relayer:**
   - Gameladder session deploys the swapped `/l1-light.wasm` + `web/l1-wasm.js` / `web/l1.js` updates to `gameladder.io` and switches the relayer's `L1_RPC_URL` to the merged Bitblock testnet (`testnet-rpc.gameladder.io` once Owen repoints the tunnel).
3. **Instant rollback window:**
   - Keep `rngcoin-node` containers warm in standby for a 48-hour soak window; any critical anomaly rolls back immediately by repointing `L1_RPC_URL` and reverting `/l1-light.wasm`.
4. **Retire `rngcoin-node`:**
   - After clean soak verification (4-validator finality, zero conservation violations, zero signer WAL errors, verified `80/20` purchase splits & `/l1.html` Chrome verification), stop `rngcoin-node` containers and mark `~/rngcoin-node` archived/deprecated in favor of `~/bitblock`.

---

## 8. Non-Claims & Open Items Parked for Owen / Counterparty Sessions

- **Non-claims:** Passing this merge design and Phase 1–2 testnet cutover establishes a verified, formally grounded, multi-process Bits testnet wired into Gameladder and Bitblock Studio — it does **not** establish production security (`L1-PRODUCTION`), economic/legal fitness for real value (`VALUE-1`), physical HSM key custody, third-party security audit clearance, or production operations sign-off (`PROD-OPS-1` / `G18`).
- **Counterparty review items requested in `~/platform/PLATFORM.md`:**
  1. Gameladder session (`1db6dc30`): confirm `/proof`, `/proof/wallet/{addr}`, `POST /tx`, `POST /faucet`, and `bitblock-light-wasm` C-ABI signatures match what `web/l1.js`, `web/l1-wasm.js`, and the relayer sidecar need.
  2. Bitblock Studio session (`0a466d62`): confirm `BitsPurchaseSplitV1` (`price_bits >= 10`, exact `80/20` split), `BitsTestnetGrantV1` (`1,000` Bits signup), and `BitsSimulatedPayoutV1` (`>= 100` Bits) match `tools/economy.mjs` constants and API expectations.
