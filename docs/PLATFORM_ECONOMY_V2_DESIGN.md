# Platform Economy & Game-State v2 Design Contract (`BITS-ITEMS-1`, `BITS-PAYOUTS-1`, `GAME-STATE-1`)

- **Status:** **Signed Off & Frozen (Phase A Complete; Phase B Parked)** — **Testnet / Devnet Only (`lab: true`)**
- **Owner:** Bitblock Chain session (`a31a4bf9`, `~/bitblock`, `Blobicue/bitblock`)
- **Cross-session sign-offs recorded (`~/platform/PLATFORM.md`, 2026-11-25):**
  - Bitblock Studio (`0a466d62`, `~/bitblock-frontier`, Studio V13 sign-off): **CONFIRMED 5/5**
  - Gameladder (`1db6dc30`, `~/gameladder-1`, live @ PR `#13` `f99cefb6`): **APPROVED WITH AMENDMENTS A–E** (all 5 wire amendments folded verbatim into this revision)
- **Execution status per Owen's 2026-11-25 Priority (`~/platform/PLATFORM.md` `## CURRENT PRIORITY`):**
  - The platform is now prioritizing the product itself (Hosted Studio on gated `gameladder.io`, a fun replayable multiplayer game, real devices, and flagship worlds live).
  - **Phase B (`BITS-ITEMS-1`, `BITS-PAYOUTS-1`, `GAME-STATE-1` implementation PRs) is intentionally NOT started** — this signed-off contract stays ready on `main` for when creators need Economy v2, while the chain remains in **soak and maintenance mode only**.
- **Pinned `origin/main` baseline:** commit `f19c24ba5ba0c4e41f4b08657032df6813bde500` (`116` consecutive 10/10-CI-green PRs `#4`–`#119`, `87` modules, `213/213` independently vectored schemas, `3,852/3,852` documented public items, `12` Lean 4 `--trust=0` modules, `COST-1@base=BOUNDED`, `PROGRAM-STATE-1@base=BOUNDED`).
- **Parent specs:** `~/platform/PLATFORM.md`, `~/platform/STUDIO_PLATFORM_SPEC.md`, `~/platform/PLATFORM_CHAIN_MIGRATION.md`, [`CHAIN_MERGE_DESIGN.md`](CHAIN_MERGE_DESIGN.md).

---

## 1. Purpose & Hard Boundaries

`BITS-ECONOMY-1` (`crates/research/src/bits_economy_1.rs`, `BitsEconomyRefinement.lean`) already settles closed-conservation `1 Bit = 1 base unit` payments and atomic `80/20` creator splits on the live Phase 2 merged testnet (`https://testnet-rpc.gameladder.io`, `bitblock-l1-v1`, `chainId: 1112101971`).

When creator demand justifies activating Economy v2 on testnet, three additive layers build cleanly on top of v1 without touching frozen v1 bytes:

1. **`BITS-ITEMS-1`:** On-chain passes, repeatable consumables, and opt-in tradable limited collectibles with browser-verifiable `EntitlementRootV1` proofs and automatic secondary-sale creator royalties (routed through `RevenueSplitPolicyV1` when registered).
2. **`BITS-PAYOUTS-1` + `WorldPublishAttestationV1`:** Deterministic multi-creator / `remixedFrom` revenue splits (exact `8,000 bps` creator pool), epoch-batched simulated testnet payouts with holdback windows, and canonical RFC 8032 Ed25519 publish/runtime manifest signatures.
3. **`GAME-STATE-1`:** Compact on-chain world checkpoints (`WorldStateAnchorV1`) anchoring Studio's bounded badges, `playerdata`, and leaderboards for wallet-bound `ActorIdV1` players, signed by the world owner or the off-edge `bb-relayer` anchor delegate, with browser-verifiable `PlayerStateProofV1`.

### Non-Negotiable Boundaries

1. **Testnet / devnet only (`lab: true`):** Every payout batch and settlement receipt is strictly simulated and carries the mandatory canonical label **`SIMULATED-TEST-ONLY-NO-REAL-MONEY`**. No real money, no real payouts, no payment processors, and no exchange listings.
2. **Excluded capabilities remain locked/parked:** `PROGRAM-CONSENSUS`, `PROD-OPS-1`, `L1-CANDIDATE`, `L1-PRODUCTION`, and `VALUE-1` remain untouched and excluded.
3. **Zero edits to `node/` during the 7-day soak:** The live Phase 2 testnet soak cluster (`bb-node-0..3`, `bb-rpc-lb`, `bb-relayer`, `bitblock-phase2-soak.timer`) and hot `rng-node-0..3` fallback stay completely undisturbed. Any `node/` maintenance (including the post-soak `cluster_e2e` timing check) or future Economy v2 wiring waits until after the soak clears and the driver requests it.
4. **Purely additive sibling modules (zero v1 tag drift):** `crates/research/src/bits_economy_1.rs`, `crates/research/src/auth_commit_1.rs`, root `Cargo.lock`, and existing Lean 4 proof modules stay untouched. All future v2 objects live in dedicated sibling modules (`bits_items_1.rs`, `bits_payouts_1.rs`, `game_state_1.rs`).
5. **Single source of truth & zero custody:** The chain ledger remains the sole source of truth for Bits and entitlements (**Gameladder never mints**); Cloudflare Workers (`CzHub` / `ROOM` DOs) hold **zero** Ed25519 signing keys; user spending and publish keys remain strict in-browser Ed25519 self-custody keys mapped via Cross-Session Amendment 1 (`ActorIdV1` canonical ID + raw 32-byte Ed25519 pubkey alias).
6. **Original IP only:** Every schema, identifier, and doc uses original Bitblock / Gameladder / Bits terminology.

---

## 2. Additive Opcode, Schema & Domain-Prefix Allocation

v1 (`bits_economy_1.rs`) occupies opcodes `0x01..0x06` and `AUTH-COMMIT-1` occupies `DomainIdV1` `1..=7`. Platform Economy & Game-State v2 reserves strictly disjoint opcodes **`0x10..0x2F`** and dedicated SHA-256 domain-separation prefixes so cross-domain proof confusion with v1 roots is cryptographically impossible.

| Opcode | Module | Canonical Schema String | Summary |
|---:|---|---|---|
| `0x10` | `bits_items_1` | `bitblock/bits-items-1/item-class/v1` | Register immutable creator-defined pass, consumable, or limited collectible |
| `0x11` | `bits_items_1` | `bitblock/bits-items-1/purchase-and-grant/v1` | Atomic `80/20` Bits payment + on-chain entitlement grant (coexists with v1 `0x02`) |
| `0x12` | `bits_items_1` | `bitblock/bits-items-1/consume-item/v1` | Replay-guarded consumable burn decrementing owned quantity |
| `0x13` | `bits_items_1` | `bitblock/bits-items-1/trade-item/v1` | Dual-signed opt-in P2P secondary trade with creator royalty + platform fee |
| — | `bits_items_1` | `bitblock/bits-items-1/entitlement-root/v1` | Separately typed `EntitlementRootV1` & `EntitlementProofV1` commitment |
| `0x20` | `bits_payouts_1` | `bitblock/bits-payouts-1/revenue-split/v1` | Multi-creator + `remixedFrom` revenue split policy (`sum == 8,000 bps`) |
| `0x21` | `bits_payouts_1` | `bitblock/bits-payouts-1/payout-batch/v1` | Epoch-batched simulated testnet creator payout claim (`lab == true`) |
| `0x22` | `bits_payouts_1` | `bitblock/bits-payouts-1/settlement-receipt/v1` | Canonical simulated settlement receipt (`SIMULATED-TEST-ONLY-NO-REAL-MONEY`) |
| `0x23` | `bits_payouts_1` | `bitblock/bits-payouts-1/publish-attestation/v1` | Creator Ed25519 world publish & runtime manifest attestations |
| `0x28` | `game_state_1` | `bitblock/game-state-1/world-anchor/v1` | Compact per-world badges, `playerdata`, and leaderboard Merkle checkpoint |
| `0x29` | `game_state_1` | `bitblock/game-state-1/anchor-delegate/v1` | Owner-signed depth-1 expiring delegate grant / revoke for `bb-relayer` anchoring |
| — | `game_state_1` | `bitblock/game-state-1/player-state-proof/v1` | Browser-verifiable inclusion proof against a finalized `WorldStateAnchorV1` |

### SHA-256 Tree & Signature Domain Prefixes (All Strictly Disjoint)

- `bits_items_1`: `bb.bits_items1.class.empty.v1`, `bb.bits_items1.class.leaf.v1`, `bb.bits_items1.class.node.v1`, `bb.bits_items1.ent.empty.v1`, `bb.bits_items1.ent.leaf.v1`, `bb.bits_items1.ent.node.v1`, `bb.bits_items1.trade_offer.v1`.
- `bits_payouts_1`: `bb.bits_payouts1.split.v1`, `bb.bits_payouts1.batch.v1`, `gameladder-studio-publish/v1\0` (exact match with `STUDIO_PLATFORM_SPEC.md` §4.3), `gameladder-studio-runtime/v1\0`.
- `game_state_1`: `bb.game_state1.badges.v1`, `bb.game_state1.playerdata.v1`, `bb.game_state1.leaderboard.v1`, `bb.game_state1.anchor_delegate.v1`.

---

## 3. Track #1 — `BITS-ITEMS-1` (On-Chain Passes, Consumables & Portable Items)

Replaces Gameladder's unbacked `CzHub` DO entitlement counter (`ent:<worldId>:<itemId>:<sub>`) with a cryptographically provable, portable on-chain entitlement tree while preserving exact `ASSET-1` / `BITS-ECONOMY-1` closed supply conservation.

### 3.1 Item Kinds & `ItemClassV1` (Opcode `0x10`)

```rust
pub enum ItemKindV1 {
    NonFungiblePass = 1,       // Unique per (actor, world_id, item_id); quantity in {0, 1}; serial_no == 0
    RepeatableConsumable = 2,  // Fungible per-actor u64 balance; repeatable purchases + burns; non-tradable
    LimitedCollectible = 3,    // Capped supply (max_supply > 0), unique monotonic serial_no 1..=max_supply
}

pub struct ItemClassV1 {
    pub version: u16,               // 1
    pub world_id: String,           // 1..=64 bytes, ^[a-z0-9][a-z0-9_-]{0,63}$
    pub item_id: String,            // 1..=64 bytes, ^[a-z0-9][a-z0-9_-]{0,63}$
    pub creator_actor: ActorIdV1,   // Canonical Amendment-1 creator ActorIdV1
    pub kind: ItemKindV1,
    pub price_bits: u128,           // Integer [10, 100_000] matching STUDIO_PLATFORM_SPEC §4.3
    pub max_supply: u64,            // 0 = uncapped (Pass/Consumable only); > 0 mandatory for LimitedCollectible
    pub tradable: bool,             // Opt-in per ItemClassV1 (default false; MUST be false for RepeatableConsumable)
    pub royalty_bps: u16,           // 0..=1_000 (0%..=10%); MUST be 0 when tradable == false
    pub metadata_sha256: [u8; 32],  // SHA-256 of canonical off-chain Studio item metadata JSON
}
```

- **Bounds aligned with `STUDIO_PLATFORM_SPEC.md` §4.3:** `<= 32` passes, `<= 64` consumables, and `<= 64` limited collectibles per `world_id`.
- **Full v2.0 immutability (Amendment B3):** Once registered for `(world_id, item_id)`, **every field** of `ItemClassV1` — including `price_bits` and `metadata_sha256`, as well as `kind`, `max_supply`, `tradable`, and `royalty_bps` — is strictly immutable in v2.0. Re-submitting opcode `0x10` for an existing `(world_id, item_id)` is refused fail-closed; a new price tier or revised item metadata registers a distinct `item_id`.

### 3.2 Primary Purchase & Grant (`PurchaseAndGrantV1`, Opcode `0x11`)

- **Zero-downtime v1/v2 coexistence (Amendment E1):** v1 `BitsPurchaseSplitV1` (`0x02`) stays accepted alongside v2 `PurchaseAndGrantV1` (`0x11`) indefinitely so Gameladder's `POST /api/bits/purchase/quote` can auto-select `0x11` whenever `GET /proof/item/{worldId}/{itemId}` is registered on-chain and `0x02` otherwise.
- Buyer signs one canonical `PurchaseAndGrantV1 { order_id: [u8; 32], world_id, item_id, quantity: u32, expected_price_bits: u128, memo_hash: [u8; 32] }`.
- **Atomic execution:**
  1. Verifies `order_id` has never been executed (replay guard) and `total_price = price_bits * quantity` matches `expected_price_bits`.
  2. For `NonFungiblePass`: requires `quantity == 1`, assigns canonical `serial_no = 0`, and rejects if buyer already holds `(world_id, item_id)`.
  3. For `LimitedCollectible`: requires `quantity == 1` and `minted_count + 1 <= max_supply`; assigns `serial_no = minted_count + 1`.
  4. Executes exact `80/20` Bits split (routing the `8,000 bps` `creator_pool` through the world's active `RevenueSplitPolicyV1` from §4.1 when registered, or to `ItemClassV1.creator_actor` otherwise):
     - `creator_pool = (total_price * 8_000) / 10_000`
     - `platform_bits = total_price - creator_pool` (`creator_pool + platform_bits == total_price`)
  5. Updates `EntitlementRootV1` atomically. Any check failure rolls back **both** Bits balances and entitlements with zero partial state mutation.

### 3.3 Consumable Burn (`ConsumeItemV1`, Opcode `0x12`)

- Owner signs `ConsumeItemV1 { consume_id: [u8; 32], world_id, item_id, quantity: u32, context_hash: [u8; 32] }`.
- Valid only for `ItemKindV1::RepeatableConsumable`; requires `quantity >= 1`, `consume_id` unused, and `owned_quantity >= quantity`. Decrements `owned_quantity -= quantity` atomically.

### 3.4 Opt-In Secondary Trade (`TradeItemV1`, Opcode `0x13`)

- Valid **only** when `ItemClassV1.tradable == true` (`NonFungiblePass` or `LimitedCollectible`). Gameladder may host an off-chain trade-offer relay in `CzHub` while seller and buyer dual-sign strictly in-browser via `/bitblock-light.wasm` (zero custody).
- **`NonFungiblePass` trade invariants (Amendment B1):**
  - `serial_no` MUST equal **`0`** for `NonFungiblePass` in both `TradeItemV1` and `EntitlementProofV1` (and MUST be `1..=max_supply` for `LimitedCollectible`).
  - For `NonFungiblePass`, execution strictly refuses if `buyer_actor` **already holds** `(world_id, item_id)`, preserving `nonFungiblePass_unique_owner_count` across both primary grants and secondary trades.
- Requires **dual RFC 8032 Ed25519 signatures** (seller + buyer, `seller_actor != buyer_actor`) over the 32-byte digest `sha256("bb.bits_items1.trade_offer.v1" || trade_id[32] || u32_be(len(world_id)) || utf8(world_id) || u32_be(len(item_id)) || utf8(item_id) || u64_be(serial_no) || seller_actor[32] || buyer_actor[32] || u128_be(price_bits) || u64_be(expires_at_height))`.
- **Exact secondary-sale split (`price_bits >= 20`, integer floor, remainder to seller):**
  - `creator_royalty = (price_bits * royalty_bps as u128) / 10_000` (`0%..=10%`)
  - `platform_fee = (price_bits * 500) / 10_000` (`5%` fixed)
  - `seller_proceeds = price_bits - creator_royalty - platform_fee` (`85%..=95%`)
  - **Exact conservation:** `creator_royalty + platform_fee + seller_proceeds == price_bits` (zero Bits created or burned).
- **Secondary royalty routing with co-creators & remixes (Amendment B2):**
  - When a world has a registered `RevenueSplitPolicyV1` (§4.1), `creator_royalty` is distributed proportionally across its `8,000 bps` creator pool:
    - For each recipient `i`: `royalty_base_i = (creator_royalty * share_bps_i as u128) / 8_000`
    - `royalty_dust = creator_royalty - sum(royalty_base_i)` (`>= 0`) credited to `primary_creator`.
    - Exact conservation: `sum(royalty_credited_i) == creator_royalty`.
  - When no `RevenueSplitPolicyV1` is registered for `world_id`, `creator_royalty` credits `ItemClassV1.creator_actor` directly.
- Atomically transfers `(world_id, item_id, serial_no)` ownership from `seller_actor` to `buyer_actor` and updates `EntitlementRootV1`.

### 3.5 Lean 4 `--trust=0` Target (`proof/BitblockProof/BitsItemsRefinement.lean`)

Proves (standard Lean axioms `[propext, Quot.sound]` only, `0` warnings):
1. `nonFungiblePass_unique_owner_count`: a `NonFungiblePass` entitlement count per `(actor, world, item)` never exceeds `1` across both primary grants and `TradeItemV1` secondary trades (Amendment B1).
2. `limitedCollectible_supply_bounded`: `minted_count <= max_supply` across all primary grants, burns, and trades.
3. `tradeItem_exact_conservation`: `sum(royalty_credited_i) + platform_fee + seller_proceeds = price_bits` for all `price_bits >= 20`, `royalty_bps <= 1_000`, and any valid `RevenueSplitPolicyV1` (Amendment B2), keeping total Bits supply strictly invariant.
4. `consumeItem_no_underflow` & `failed_op_atomic_rollback`: any rejected item tx leaves balances, supplies, and entitlements bit-identical.

---

## 4. Track #2 — `BITS-PAYOUTS-1` & `WorldPublishAttestationV1`

Directly supports Owen's *"developers make way more money"* goal for studio teams and remixed worlds while locking the byte-exact cryptographic signature contract reserved in `STUDIO_PLATFORM_SPEC.md` §4.3 & §5.1.

### 4.1 Multi-Creator & Remix Revenue Splits (`RevenueSplitPolicyV1`, Opcode `0x20`)

```rust
pub struct RevenueShareV1 {
    pub recipient_actor: ActorIdV1, // Canonical Amendment-1 ActorIdV1
    pub share_bps: u16,             // > 0
    pub is_remix_ancestor: bool,    // True for at most one upstream remixedFrom creator
}

pub struct RevenueSplitPolicyV1 {
    pub version: u16,                   // 1
    pub world_id: String,               // 1..=64 bytes
    pub primary_creator: ActorIdV1,     // Must appear in shares[]
    pub remixed_from_world: Option<String>,
    pub remix_bps_locked: u16,          // 0..=2_000 (0%..=20% of total price; immutable after first publish)
    pub shares: Vec<RevenueShareV1>,    // 1..=8 distinct recipient_actors, sum(share_bps) == 8_000
}
```

- **Fixed platform boundary:** Platform treasury remains fixed at `2,000 bps` (`20%`); `sum(shares[i].share_bps) == 8_000` (`80%`) strictly.
- **Exact zero-dust integer distribution for any `price_bits >= 10`:**
  - `creator_pool = (price_bits * 8_000) / 10_000`
  - `platform_bits = price_bits - creator_pool`
  - For each recipient `i`: `base_i = (price_bits * share_bps_i) / 10_000`
  - `dust = creator_pool - sum(base_i)` (always `>= 0` since `sum(floor(x_i)) <= floor(sum(x_i))`)
  - `primary_creator` receives `base_primary + dust`.
  - **Invariant:** `sum(credited_recipients) + platform_bits == price_bits` — **0 Bits ever lost to integer rounding**.
- **Remix protection:** If `remixed_from_world.is_some()`, the upstream ancestor's `(recipient_actor, share_bps)` is locked immutably at initial policy registration and cannot be lowered or removed in later policy updates. On Gameladder's side, Tier-1 publish validation verifies `economy_manifest.revenue_split` against `CzHub`'s recorded `world.remixedFrom`, and `POST /api/bits/purchase/quote` refuses to quote if on-chain `RevenueSplitPolicyV1` diverges from `world.remixedFrom`.

### 4.2 Epoch Creator Payout Batching & Simulated Settlement (`Opcodes 0x21..0x22`)

- **Strict testnet boundary:** Structurally refuses with `TestnetOnlyDisabled` whenever `genesis.lab == false`.
- **`CreatorPayoutBatchV1` (opcode `0x21`):**
  - Aggregates accrued creator earnings across orders/worlds for `creator_actor` up to `claim_through_epoch`.
  - Enforces **minimum threshold** (`total_amount_bits >= 100`, matching `BITS_DEFAULT_MIN_PAYOUT`), **holdback window** (`current_epoch >= entry.earning_epoch + config.holdback_epochs`, default `holdback_epochs = 2`, configurable `>= 1`), **per-order non-double-claim** (`order_id` inserted into `claimed_orders`), and **on-chain balance ceiling (Amendment E2)**: strictly requires **`creator_actor.balance >= total_amount_bits`** at escrow transfer so `/creator.html` displays `claimable = min(matured_unclaimed_earnings, on_chain_balance)`.
  - Transfers `total_amount_bits` from `creator_actor` into `simulated_payout_escrow` (preserving exact closed `ASSET-1` conservation).
- **`SimulatedSettlementReceiptV1` (opcode `0x22`):**
  - Canonical receipt binding `batch_id`, `creator_actor`, `order_count`, `orders_merkle_root`, `total_amount_bits`, `settled_epoch`, and the mandatory fixed ASCII disclaimer **`SIMULATED-TEST-ONLY-NO-REAL-MONEY`**.

### 4.3 `WorldPublishAttestationV1` & `RuntimeManifestAttestationV1` (Opcode `0x23`, Amendment A)

Locks the exact cross-language byte preimages for `STUDIO_PLATFORM_SPEC.md` §4.3 (`creator_sig_ed25519`) and §5.1 (`runtime-manifest.json` `sig_ed25519`):

- **Exact length prefix & digest signing rule (Amendment A1):**
  - Every variable-length UTF-8 string uses a **`u32_be`** byte-length prefix (`u32_be(len) || utf8_bytes`).
  - RFC 8032 Ed25519 signs the **32-byte `sha256(...)` digest** (not the raw unhashed preimage bytes), matching WebCrypto (`crypto.subtle.digest("SHA-256", preimage)` → `sign("Ed25519", key, digest)`) and Rust `verify_raw_message_strict(&pk, &digest, &sig)`.
- **`WorldPublishAttestationV1` (`creator_sig_ed25519`):**
  - **Preimage digest (32 bytes):**
    ```text
    publish_digest = sha256(
        b"gameladder-studio-publish/v1\0"
        || u16_be(1)
        || u32_be(len(world_id))        || utf8(world_id)
        || u32_be(release_version)
        || u32_be(len(runtime_version)) || utf8(runtime_version)
        || world_sha256[32]
        || thumb_sha256[32]
        || economy_manifest_sha256[32]
        || owner_actor_id[32]
    )
    ```
  - **Optimistic CAS on `release_version` (Amendment A2):** Signed publish envelopes carry `expected_version == release_version`; `CzHub` enforces optimistic CAS (`409 VERSION_CONFLICT` if stale/raced) so signatures bind exact release versions and cannot replay across versions.
  - **Absent optional payloads (Amendments A3 & A4):**
    - `thumb_sha256 = [0u8; 32]` when no thumbnail PNG is uploaded.
    - `world_sha256 = sha256(exact_transmitted_utf8_world_json_bytes)` — hashes the exact UTF-8 wire bytes rather than re-serialized JSON, preventing JS `1.0 -> "1"` vs Rust `serde_json` `1.0 -> "1.0"` float drift.
    - `economy_manifest_sha256 = sha256(canonical_json_utf8(economy_manifest))` (ASCII-key-sorted compact UTF-8 JSON matching `tools/upload-studio-runtime.mjs:canonicalJson`; integers/booleans/strings only — zero floats; `[0u8; 32]` when absent).
  - Signed strictly by the active Ed25519 key of the world's `owner` `ActorIdV1` (survives key rotation via Amendment 1).
- **`RuntimeManifestAttestationV1` (`runtime-manifest.json` `sig_ed25519`, Amendment A5):**
  - Replaces non-existent `files_merkle_root` with the shipped 32-byte raw **`files_sha256`** (`canonical_json_sha256({ entry, files, format_version, schema, semver })`) and avoids self-reference by computing `manifest_sha256 = sha256(canonical_json_utf8(manifest_with_sig_ed25519_null))`:
    ```text
    runtime_digest = sha256(
        b"gameladder-studio-runtime/v1\0"
        || u32_be(len(semver_sha8)) || utf8(semver_sha8)
        || u32_be(format_version)
        || files_sha256[32]
        || manifest_sha256[32]
    )
    ```
  - Signed over the 32-byte `runtime_digest`, aligning with `RELEASE-1`'s multi-signer release provenance model.

### 4.4 Lean 4 `--trust=0` Target (`proof/BitblockProof/BitsPayoutsRefinement.lean`)

Proves (`[propext, Quot.sound]` only, `0` warnings):
1. `revenueSplit_dust_nonneg_and_exact`: `sum(base_i) <= creator_pool` and `sum(final_shares) + platform_bits = price_bits` for any valid `1..=8` split summing to `8,000 bps`.
2. `remixShare_immutable_under_update`: any valid policy update preserves the locked `remixedFrom` ancestor share.
3. `payoutBatch_holdback_balance_and_no_double_claim`: no order can be claimed before `earning_epoch + holdback_epochs` or claimed twice, `total_amount_bits <= creator_actor.balance`, and `sum(claimed_payouts) <= accrued_earnings` with exact closed escrow conservation.

---

## 5. Track #3 — `GAME-STATE-1` (Compact World Checkpoints, Badges & PlayerData Roots)

Full gameplay stays off-chain (Browser WebGPU + Cloudflare Room DOs). `GAME-STATE-1` gives Studio worlds compact, bounded, cryptographically verifiable L1 checkpoints without activating `PROGRAM-CONSENSUS`.

### 5.1 Owner & Off-Edge `bb-relayer` Delegated Anchor Authority (`AnchorDelegateGrantV1`, Opcode `0x29`, Amendment C1)

- **Zero hot keys in Cloudflare Workers:** Gameladder Workers (`CzHub` / `ROOM` DOs) hold **zero** Ed25519 signing keys. When a creator enables automated checkpoints, `AnchorDelegateGrantV1.delegate_pubkey` MUST be the off-edge **`bb-relayer` sidecar's dedicated anchor Ed25519 pubkey** (published read-only at `GET /api/studio/anchor/delegate-info`).
- **End-to-end zero-custody delegation flow:**
  1. Creator clicks **Enable On-Chain Checkpoints** in `/creator.html` (or Studio), signs `AnchorDelegateGrantV1` in-browser (`gl_l1_wallet_v1`), and broadcasts to `POST /tx`.
  2. Creator registers `{ worldId, delegationId, expiresAtEpoch, txHash }` in `CzHub`; `CzHub` verifies finalized L1 tx inclusion, displays renewal banners in `/creator.html` `<= 24` epochs before expiry, and serves `GET /api/studio/relayer/pending-anchors` to `bb-relayer`.
  3. `bb-relayer` signs and submits `WorldStateAnchorV1` checkpoints within the active delegation window.
- **Canonical structure & epoch bounds:**
  - `{ version: 1, world_id, owner_actor, delegate_pubkey: CanonicalEd25519PublicKeyV1, delegation_id: [u8; 32], expires_at_epoch: u64 }`.
  - `expires_at_epoch` is measured strictly in chain **`committeeEpoch`** units with **`default_ttl = 720` epochs** and **hard ceiling `<= 8,760` epochs** (`expires_at_epoch <= current_epoch + 8_760`).
  - Scoped strictly to **one `world_id`** (zero wallet/transfer/publish authority; non-transitive: delegates cannot sub-delegate) and immediately revocable via owner-signed `AnchorDelegateRevokeV1 { world_id, delegation_id }`.

### 5.2 `WorldStateAnchorV1` & `PlayerStateProofV1` (Opcode `0x28`, Amendment C2)

```rust
pub struct WorldStateAnchorV1 {
    pub version: u16,                    // 1
    pub world_id: String,                // 1..=64 bytes
    pub release_version: u32,            // >= 1; MUST match an attested on-chain WorldPublishAttestationV1
    pub epoch: u64,                      // Strictly monotonic per world_id (committeeEpoch)
    pub world_sha256: [u8; 32],          // Must match the attested WorldPublishAttestationV1.world_sha256
    pub badges_root: [u8; 32],           // Sorted Merkle root of (wallet-bound actor_id -> u16 badge_mask, <= 16 badges)
    pub playerdata_root: [u8; 32],       // Sorted Merkle root of (wallet-bound actor_id, key -> sha256(value))
    pub leaderboard_root: [u8; 32],      // Sorted Merkle root of (board_id, rank -> (actor_id, score_i64))
    pub player_count: u32,               // Count of wallet-bound ActorIdV1 players in snapshot
    pub signer_actor: ActorIdV1,         // World owner ActorIdV1
    pub delegation_id: Option<[u8; 32]>, // None = owner-signed; Some(id) = valid active AnchorDelegateGrantV1
}
```

- **Guest-play (`dec:no_auth_wall`) & attestation boundary (Amendment C2):**
  - `WorldStateAnchorV1` anchors **wallet-bound `ActorIdV1` players only**. Guest (`g:<hash16>`) and unbound Discord (`d:<snowflake>`) progress stays off-chain in `CzHub` + R2 with zero auth wall and becomes anchor-eligible once the player binds an Ed25519 wallet.
  - `WorldStateAnchorV1` requires an on-chain `WorldPublishAttestationV1` for `(world_id, release_version)` whose `world_sha256` matches verbatim (clean prerequisite since enabling checkpoints already uses the creator's browser Ed25519 wallet).
  - `leaderboard_root` keys lock `board_id` (default `"default"`, `1..=32` UTF-8 bytes, `^[a-z0-9][a-z0-9_-]{0,31}$`) and exact signed integer **`score_i64: i64`**.
- **Studio Tier-1 structural bounds enforced verbatim (`STUDIO_PLATFORM_SPEC.md` §5.3):**
  - `<= 64` PlayerData keys per `(world_id, actor_id)`
  - `<= 64` UTF-8 bytes per PlayerData key
  - `<= 2,048` UTF-8 bytes per PlayerData value
  - `<= 16` badges per `world_id` (compact `u16` bitmask + canonical badge-ID table)
  - `<= 12 KiB` proof byte ceiling (`BOUNDS-1`) for `PlayerStateProofV1`.

---

## 6. Reserved Post-Soak HTTP Proof Doors & `bitblock-light-wasm` Budget (Amendment D)

*(No `node/` or `bitblock-light-wasm` files change during the 7-day soak; these routes and exports lock the complete post-soak integration contract.)*

### 6.1 Reserved Read/Verify Proof Doors (All `camelCase` JSON, Amendment-1 Dual Lookup)

| Route | Returns (JSON, `camelCase`) | Verification Anchor |
|---|---|---|
| `GET /proof/item/{worldId}/{itemId}` | `{ ok, exists, itemClass, mintedCount, itemClassRoot, proof }` | `ItemClassRootV1` |
| `GET /proof/entitlement/{actorOrPk}/{worldId}/{itemId}` | **Present:** `{ ok: true, exists: true, actorId, ed25519Hex, worldId, itemId, kind, quantity, serialNos[], entitlementRoot, proof }` · **Absent (Amendment D2):** `{ ok: true, exists: false, actorId, ed25519Hex, worldId, itemId, quantity: 0, serialNos: [], entitlementRoot, proof }` (`serialNos = [0]` when `NonFungiblePass` held) | `EntitlementRootV1` |
| `GET /proof/revenue-split/{worldId}` *(Amendment D1)* | `{ ok, exists, policy, splitRoot, proof }` | `RevenueSplitPolicyV1` |
| `GET /proof/publish/{worldId}/{releaseVersion}` *(Amendment D1)* | `{ ok, exists, attestation, publishDigestHex, publishRoot, proof }` | `WorldPublishAttestationV1` |
| `GET /proof/payout/{batchId}` | `{ ok, receipt, settlementTag, simulated: true, testOnly: true, disclaimer: "SIMULATED-TEST-ONLY-NO-REAL-MONEY" }` | `SimulatedSettlementReceiptV1` |
| `GET /proof/world-state/{worldId}` | `{ ok, exists, anchor, badgesRoot, playerdataRoot, leaderboardRoot, epoch }` | `WorldStateAnchorV1` |
| `GET /proof/world-state/{worldId}/player/{actorOrPk}` | `{ ok, actorId, ed25519Hex, badgeMask, playerdataProofs[], leaderboardProof?, anchorEpoch }` | `PlayerStateProofV1` |

- **Finalized transaction receipt attribution (`GET /proof/tx/{hash}`, Amendment D2):**
  - For opcode `0x11` (`PurchaseAndGrantV1`) and opcode `0x13` (`TradeItemV1`), `GET /proof/tx/{hash}` returns `{ ok, height, txHash, kind, worldId, itemId, itemKind, serialNo?, totalBits, platformBits, recipientPayouts: [{ actorId, bits, isRemixAncestor }] }` (with **zero buyer PII**), enabling Gameladder's creator-private `GET /api/bits/earnings` to attribute co-creator, remix, and secondary-royalty earnings directly from finalized chain receipts.

### 6.2 Browser `bitblock-light-wasm` Budget Guard (`<= 250 KiB` Gzip)

- Current `bitblock-light-wasm` gzipped size: **`149,181 B` (`145.7 KiB`)** vs **`256,000 B` (`250 KiB`)** budget (`106,819 B` headroom).
- Planned future additive exports (`verify_entitlement_proof_json`, `verify_publish_attestation_json`, `verify_world_state_proof_json`) reuse the exact same pure-Rust SHA-256 + Ed25519 primitives already compiled into `bitblock-light-wasm`, keeping projected total size **`< 175 KiB` gzipped** (well inside `250 KiB`).

---

## 7. Cross-Session Sign-Off & Amendment Disposition Log

| # | Alignment Topic | Studio V13 (`0a466d62`) | Gameladder (`1db6dc30`) | Disposition in This Contract |
|---|---|---|---|---|
| 1 | Secondary trades & royalties (`TradeItemV1`) | Confirmed | Confirmed + **Amendments B1/B2/B3** | **Folded in §3.1, §3.4, §3.5:** `serial_no = 0` & buyer-non-ownership guard for `NonFungiblePass`; secondary royalty routed via `RevenueSplitPolicyV1`; `ItemClassV1` fully immutable in v2.0. |
| 2 | Co-creator/remix splits & simulated payouts | Confirmed | Confirmed + **Amendments E1/E2** | **Folded in §3.2, §4.2, §4.4:** v1 `0x02` coexists with v2 `0x11`; `CreatorPayoutBatchV1` enforces `creator_actor.balance >= total_amount_bits`. |
| 3 | Publish & runtime signature preimages | Confirmed (`u32_be`) | **Amendment A (A1–A5)** | **Folded in §4.3:** `u32_be` lengths, Ed25519 signs 32-byte `sha256` digest, `expected_version == release_version` CAS, `[0u8; 32]` optional thumb/manifest, raw UTF-8 `world_json` bytes + canonical JSON `economy_manifest`, non-circular `files_sha256` + `sig_ed25519: null` runtime attestation. |
| 4 | World checkpoint delegation & guest-play boundary | Confirmed | Confirmed + **Amendments C1/C2** | **Folded in §5.1, §5.2:** zero hot keys in Workers (`bb-relayer` is delegate), `committeeEpoch` TTL (`default = 720`, cap `<= 8,760`), wallet-bound `ActorIdV1` players only, `board_id` (`"default"`, `1..=32 B`) + `score_i64`, requires on-chain `WorldPublishAttestationV1`. |
| 5 | JSON casing & proof-door completeness | Confirmed | Confirmed + **Amendments D1/D2** | **Folded in §6.1:** added `GET /proof/revenue-split/{worldId}` and `GET /proof/publish/{worldId}/{releaseVersion}`, explicit `exists: false` non-membership proof shape, and `GET /proof/tx/{hash}` `recipientPayouts[]` attribution (zero buyer PII). |

---

## 8. Parked Phase B Roadmap (Not Started — Awaiting Creator Demand & Driver Go-Ahead)

Per Owen's 2026-11-25 product priority (`~/platform/PLATFORM.md`), **Phase B stays parked** until Hosted Studio, fun multiplayer games, real-device smoothness, and live flagship worlds create creator demand:

- **Track #1 (`BITS-ITEMS-1`):** `crates/research/src/bits_items_1.rs` + `tools/bits_items_1_vectors.py` + `tests/wave37_bits_items_1.rs` + `proof/BitblockProof/BitsItemsRefinement.lean` (`--trust=0`).
- **Track #2 (`BITS-PAYOUTS-1` + `WorldPublishAttestationV1`):** `crates/research/src/bits_payouts_1.rs` + `tools/bits_payouts_1_vectors.py` + `tests/wave38_bits_payouts_1.rs` + `proof/BitblockProof/BitsPayoutsRefinement.lean` (`--trust=0`).
- **Track #3 (`GAME-STATE-1` + Merged-Head Receipt):** `crates/research/src/game_state_1.rs` + `tools/game_state_1_vectors.py` + `tests/wave39_game_state_1.rs` + `examples/platform_economy_v2_probe.rs` + `docs/PLATFORM_ECONOMY_V2_RECEIPT.md`.
