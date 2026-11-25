# Platform Economy & Game-State v2 Design Contract (`BITS-ITEMS-1`, `BITS-PAYOUTS-1`, `GAME-STATE-1`)

- **Status:** Phase A Design Contract — **Testnet / Devnet Only (`lab: true`)**
- **Owner:** Bitblock Chain session (`a31a4bf9`, `~/bitblock`, `Blobicue/bitblock`)
- **Counterparties for Design Hard Stop sign-off before Phase B code (`PRs #120`–`#125`):**
  - Gameladder session (`1db6dc30`, `~/gameladder-1`, `gameladder.io`)
  - Bitblock Studio session (`0a466d62`, `~/bitblock-frontier`)
- **Pinned `origin/main` baseline:** commit `19c595508d6ca8b9d21b16c2fda01ad6878f5d34` (tree `c359f3fb400c41bd60c847f2c70ad1596626b432`, `115` consecutive 10/10-CI-green PRs `#4`–`#118`, `87` modules, `213/213` independently vectored schemas, `3,852/3,852` documented public items, `12` Lean 4 `--trust=0` modules, `COST-1@base=BOUNDED`, `PROGRAM-STATE-1@base=BOUNDED`).
- **Parent specs:** `~/platform/PLATFORM.md`, `~/platform/STUDIO_PLATFORM_SPEC.md`, `~/platform/PLATFORM_CHAIN_MIGRATION.md`, [`CHAIN_MERGE_DESIGN.md`](CHAIN_MERGE_DESIGN.md).

---

## 1. Purpose & Hard Boundaries

`BITS-ECONOMY-1` (`crates/research/src/bits_economy_1.rs`, `BitsEconomyRefinement.lean`) already settles closed-conservation `1 Bit = 1 base unit` payments and atomic `80/20` creator splits on the live Phase 2 merged testnet (`https://testnet-rpc.gameladder.io`, `bitblock-l1-v1`, `chainId: 1112101971`).

To compete with Roblox on the browser (Gameladder = platform, Bitblock Studio = creation tool, Bitblock L1 = authoritative currency & ownership ledger), three platform-facing testnet layers are needed next:

1. **`BITS-ITEMS-1`:** On-chain passes, repeatable consumables, and opt-in tradable limited collectibles with browser-verifiable `EntitlementRootV1` proofs and automatic secondary-sale creator royalties.
2. **`BITS-PAYOUTS-1` + `WorldPublishAttestationV1`:** Deterministic multi-creator / `remixedFrom` revenue splits (exact `8,000 bps` creator pool), epoch-batched simulated testnet payouts with holdback windows, and canonical RFC 8032 Ed25519 publish/runtime manifest signatures.
3. **`GAME-STATE-1`:** Compact on-chain world checkpoints (`WorldStateAnchorV1`) anchoring Studio's bounded badges, `playerdata`, and leaderboards with owner-or-delegated signing authority and browser-verifiable `PlayerStateProofV1`.

### Non-Negotiable Boundaries

1. **Testnet / devnet only (`lab: true`):** Every payout batch and settlement receipt is strictly simulated and carries the mandatory canonical label **`SIMULATED-TEST-ONLY-NO-REAL-MONEY`**. No real money, no real payouts, no payment processors, and no exchange listings.
2. **Excluded capabilities remain locked/parked:** `PROGRAM-CONSENSUS`, `PROD-OPS-1`, `L1-CANDIDATE`, `L1-PRODUCTION`, and `VALUE-1` remain untouched and excluded.
3. **Zero edits to `node/` during the 7-day soak:** All Phase A and Phase B PRs stay strictly outside `node/` (`git diff 19c5955..HEAD -- node/` = `0` bytes) with zero disturbance to the running Phase 2 testnet soak cluster (`bb-node-0..3`, `bb-rpc-lb`, `bb-relayer`, `bitblock-phase2-soak.timer`) or hot `rng-node-0..3` fallback. Wiring into `node/` and `bitblock-light-wasm` happens **only after** Phase 3 decommission completes.
4. **Purely additive sibling modules (zero v1 tag drift):** `crates/research/src/bits_economy_1.rs`, `crates/research/src/auth_commit_1.rs`, root `Cargo.lock`, and existing Lean 4 proof modules stay untouched. All v2 objects live in dedicated sibling modules (`bits_items_1.rs`, `bits_payouts_1.rs`, `game_state_1.rs`).
5. **Single source of truth & zero custody:** The chain ledger remains the sole source of truth for Bits and entitlements (**Gameladder never mints**); user spending and publish keys remain strict in-browser Ed25519 self-custody keys mapped via Cross-Session Amendment 1 (`ActorIdV1` canonical ID + raw 32-byte Ed25519 pubkey alias).
6. **Original IP only:** Every schema, identifier, and doc uses original Bitblock / Gameladder / Bits terminology.

---

## 2. Additive Opcode, Schema & Domain-Prefix Allocation

v1 (`bits_economy_1.rs`) occupies opcodes `0x01..0x06` and `AUTH-COMMIT-1` occupies `DomainIdV1` `1..=7`. Platform Economy & Game-State v2 reserves strictly disjoint opcodes **`0x10..0x2F`** and dedicated SHA-256 domain-separation prefixes so cross-domain proof confusion with v1 roots is cryptographically impossible.

| Opcode | Module | Canonical Schema String | Summary |
|---:|---|---|---|
| `0x10` | `bits_items_1` | `bitblock/bits-items-1/item-class/v1` | Register creator-defined pass, consumable, or limited collectible |
| `0x11` | `bits_items_1` | `bitblock/bits-items-1/purchase-and-grant/v1` | Atomic `80/20` Bits payment + on-chain entitlement grant |
| `0x12` | `bits_items_1` | `bitblock/bits-items-1/consume-item/v1` | Replay-guarded consumable burn decrementing owned quantity |
| `0x13` | `bits_items_1` | `bitblock/bits-items-1/trade-item/v1` | Dual-signed opt-in P2P secondary trade with creator royalty + platform fee |
| — | `bits_items_1` | `bitblock/bits-items-1/entitlement-root/v1` | Separately typed `EntitlementRootV1` & `EntitlementProofV1` commitment |
| `0x20` | `bits_payouts_1` | `bitblock/bits-payouts-1/revenue-split/v1` | Multi-creator + `remixedFrom` revenue split policy (`sum == 8,000 bps`) |
| `0x21` | `bits_payouts_1` | `bitblock/bits-payouts-1/payout-batch/v1` | Epoch-batched simulated testnet creator payout claim (`lab == true`) |
| `0x22` | `bits_payouts_1` | `bitblock/bits-payouts-1/settlement-receipt/v1` | Canonical simulated settlement receipt (`SIMULATED-TEST-ONLY-NO-REAL-MONEY`) |
| `0x23` | `bits_payouts_1` | `bitblock/bits-payouts-1/publish-attestation/v1` | Creator Ed25519 world publish & runtime manifest attestations |
| `0x28` | `game_state_1` | `bitblock/game-state-1/world-anchor/v1` | Compact per-world badges, `playerdata`, and leaderboard Merkle checkpoint |
| `0x29` | `game_state_1` | `bitblock/game-state-1/anchor-delegate/v1` | Owner-signed depth-1 expiring delegate grant / revoke for world anchoring |
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
    NonFungiblePass = 1,       // Unique per (actor, world_id, item_id); quantity in {0, 1}
    RepeatableConsumable = 2,  // Fungible per-actor u64 balance; repeatable purchases + burns
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
    pub tradable: bool,             // Opt-in per ItemClassV1 (must be false for RepeatableConsumable)
    pub royalty_bps: u16,           // 0..=1_000 (0%..=10%); MUST be 0 when tradable == false
    pub metadata_sha256: [u8; 32],  // SHA-256 of canonical off-chain Studio item metadata JSON
}
```

- **Bounds aligned with `STUDIO_PLATFORM_SPEC.md` §4.3:** `<= 32` passes, `<= 64` consumables, and `<= 64` limited collectibles per `world_id`.
- **Immutability:** Once registered for `(world_id, item_id)`, `kind`, `max_supply`, `tradable`, and `royalty_bps` are immutable so buyers have permanent cryptographic guarantees.

### 3.2 Primary Purchase & Grant (`PurchaseAndGrantV1`, Opcode `0x11`)

- Buyer signs one canonical `PurchaseAndGrantV1 { order_id: [u8; 32], world_id, item_id, quantity: u32, expected_price_bits: u128, memo_hash: [u8; 32] }`.
- **Atomic execution:**
  1. Verifies `order_id` has never been executed (replay guard) and `total_price = price_bits * quantity` matches `expected_price_bits`.
  2. For `NonFungiblePass`: requires `quantity == 1` and buyer does **not** already hold `(world_id, item_id)`.
  3. For `LimitedCollectible`: requires `quantity == 1` and `minted_count + 1 <= max_supply`; assigns `serial_no = minted_count + 1`.
  4. Executes exact `80/20` Bits split (or delegates to the world's active `RevenueSplitPolicyV1` from Track #2 when registered):
     - `creator_pool = (total_price * 8_000) / 10_000`
     - `platform_bits = total_price - creator_pool` (`creator_pool + platform_bits == total_price`)
  5. Updates `EntitlementRootV1` atomically. Any check failure rolls back **both** Bits balances and entitlements with zero partial state mutation.

### 3.3 Consumable Burn (`ConsumeItemV1`, Opcode `0x12`)

- Owner signs `ConsumeItemV1 { consume_id: [u8; 32], world_id, item_id, quantity: u32, context_hash: [u8; 32] }`.
- Valid only for `ItemKindV1::RepeatableConsumable`; requires `quantity >= 1`, `consume_id` unused, and `owned_quantity >= quantity`. Decrements `owned_quantity -= quantity` atomically.

### 3.4 Opt-In Secondary Trade (`TradeItemV1`, Opcode `0x13`)

- Valid **only** when `ItemClassV1.tradable == true` (`NonFungiblePass` or `LimitedCollectible`).
- Requires **dual RFC 8032 Ed25519 signatures** (seller + buyer, `seller_actor != buyer_actor`) over `sha256("bb.bits_items1.trade_offer.v1" || trade_id || world_id || item_id || serial_no || seller_actor || buyer_actor || price_bits || expires_at_height)`.
- **Exact secondary-sale split (`price_bits >= 20`, integer floor, remainder to seller):**
  - `creator_royalty = (price_bits * royalty_bps as u128) / 10_000` (`0%..=10%`)
  - `platform_fee = (price_bits * 500) / 10_000` (`5%` fixed)
  - `seller_proceeds = price_bits - creator_royalty - platform_fee` (`85%..=95%`)
  - **Exact conservation:** `creator_royalty + platform_fee + seller_proceeds == price_bits` (zero Bits created or burned).
- Atomically transfers `(world_id, item_id, serial_no)` ownership from `seller_actor` to `buyer_actor` and updates `EntitlementRootV1`.

### 3.5 Lean 4 `--trust=0` Target (`proof/BitblockProof/BitsItemsRefinement.lean`)

Proves (standard Lean axioms `[propext, Quot.sound]` only, `0` warnings):
1. `nonFungiblePass_unique_owner_count`: a `NonFungiblePass` entitlement count per `(actor, world, item)` never exceeds `1`.
2. `limitedCollectible_supply_bounded`: `minted_count <= max_supply` across all primary grants, burns, and trades.
3. `tradeItem_exact_conservation`: `creator_royalty + platform_fee + seller_proceeds = price_bits` for all `price_bits >= 20` and `royalty_bps <= 1_000`, and total liquid + treasury Bits supply is strictly invariant.
4. `consumeItem_no_underflow` & `failed_op_atomic_rollback`: any rejected item tx leaves balances, supplies, and entitlements bit-identical.

---

## 4. Track #2 — `BITS-PAYOUTS-1` & `WorldPublishAttestationV1`

Directly supports Owen's *"developers make way more money"* goal for studio teams and remixed worlds while closing the cryptographic signature contract already reserved in `STUDIO_PLATFORM_SPEC.md` §4.3.

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
  - `dust = creator_pool - sum(base_i)` (always `>= 0` since `sum(floor(x_i)) <= floor(sum(x_i))`!)
  - `primary_creator` receives `base_primary + dust`.
  - **Invariant:** `sum(credited_recipients) + platform_bits == price_bits` — **0 Bits ever lost to integer rounding**.
- **Remix protection:** If `remixed_from_world.is_some()`, the upstream ancestor's `(recipient_actor, share_bps)` is locked immutably at initial policy registration and cannot be lowered or removed in later policy updates.

### 4.2 Epoch Creator Payout Batching & Simulated Settlement (`Opcodes 0x21..0x22`)

- **Strict testnet boundary:** Structurally refuses with `TestnetOnlyDisabled` whenever `genesis.lab == false`.
- **`CreatorPayoutBatchV1` (opcode `0x21`):**
  - Aggregates accrued creator earnings across orders/worlds for `creator_actor` up to `claim_through_epoch`.
  - Enforces **minimum threshold** (`total_amount_bits >= 100`, matching `BITS_DEFAULT_MIN_PAYOUT`), **holdback window** (`current_epoch >= entry.earning_epoch + config.holdback_epochs`, default `holdback_epochs = 2`, configurable `>= 1`), and **per-order non-double-claim** (`order_id` inserted into `claimed_orders`).
  - Transfers `total_amount_bits` from `creator_actor` into `simulated_payout_escrow` (preserving exact closed `ASSET-1` conservation).
- **`SimulatedSettlementReceiptV1` (opcode `0x22`):**
  - Canonical receipt binding `batch_id`, `creator_actor`, `order_count`, `orders_merkle_root`, `total_amount_bits`, `settled_epoch`, and the mandatory fixed ASCII disclaimer **`SIMULATED-TEST-ONLY-NO-REAL-MONEY`**.

### 4.3 `WorldPublishAttestationV1` & `RuntimeManifestAttestationV1` (Opcode `0x23`)

Locks the exact canonical signing preimages reserved by `STUDIO_PLATFORM_SPEC.md` §4.3 & §5.1 so Bitblock Studio, Gameladder `CzHub`, and the chain verify identical signatures:

- **`WorldPublishAttestationV1` (`creator_sig_ed25519`):**
  - Canonical preimage: `sha256("gameladder-studio-publish/v1\0" || version_u16_be || len_prefixed(world_id) || release_version_u32_be || len_prefixed(runtime_version) || world_sha256 || thumb_sha256 || economy_manifest_sha256 || owner_actor_id)`.
  - Signed strictly by the active Ed25519 key of the world's `owner` `ActorIdV1` (survives key rotation via Amendment 1).
- **`RuntimeManifestAttestationV1` (`runtime-manifest.json` `sig_ed25519`):**
  - Canonical preimage: `sha256("gameladder-studio-runtime/v1\0" || len_prefixed(semver_sha8) || format_version_u32_be || files_merkle_root || manifest_sha256)`.
  - Aligns cleanly with `RELEASE-1`'s multi-signer release provenance model.

### 4.4 Lean 4 `--trust=0` Target (`proof/BitblockProof/BitsPayoutsRefinement.lean`)

Proves (`[propext, Quot.sound]` only, `0` warnings):
1. `revenueSplit_dust_nonneg_and_exact`: `sum(base_i) <= creator_pool` and `sum(final_shares) + platform_bits = price_bits` for any valid `1..=8` split summing to `8,000 bps`.
2. `remixShare_immutable_under_update`: any valid policy update preserves the locked `remixedFrom` ancestor share.
3. `payoutBatch_holdback_and_no_double_claim`: no order can be claimed before `earning_epoch + holdback_epochs` or claimed twice, and `sum(claimed_payouts) <= accrued_earnings` with exact closed escrow conservation.

---

## 5. Track #3 — `GAME-STATE-1` (Compact World Checkpoints, Badges & PlayerData Roots)

Full gameplay stays off-chain (Browser WebGPU + Cloudflare Room DOs). `GAME-STATE-1` gives Studio worlds compact, bounded, cryptographically verifiable L1 checkpoints without activating `PROGRAM-CONSENSUS`.

### 5.1 Owner & Depth-1 Delegated Anchor Authority (`AnchorDelegateGrantV1`, Opcode `0x29`)

- A world's `owner` `ActorIdV1` may submit `WorldStateAnchorV1` directly **or** sign a depth-1 non-transitive `AnchorDelegateGrantV1`:
  - `{ version: 1, world_id, owner_actor, delegate_pubkey: CanonicalEd25519PublicKeyV1, delegation_id: [u8; 32], expires_at_epoch: u64 }`.
- Scoped strictly to **one `world_id`** and an **expiring epoch** (zero wallet/transfer/publish authority; non-transitive: delegates cannot sub-delegate).
- Immediately revocable via owner-signed `AnchorDelegateRevokeV1 { world_id, delegation_id }`.

### 5.2 `WorldStateAnchorV1` & `PlayerStateProofV1` (Opcode `0x28`)

```rust
pub struct WorldStateAnchorV1 {
    pub version: u16,               // 1
    pub world_id: String,           // 1..=64 bytes
    pub release_version: u32,       // Must match an attested release (>= 1)
    pub epoch: u64,                 // Strictly monotonic per world_id
    pub world_sha256: [u8; 32],     // Immutable R2 world.bitblock.json digest
    pub badges_root: [u8; 32],      // Sorted Merkle root of (actor_id -> u16 badge_mask, <= 16 badges)
    pub playerdata_root: [u8; 32],  // Sorted Merkle root of (actor_id, key -> sha256(value))
    pub leaderboard_root: [u8; 32], // Sorted Merkle root of (board_id, rank -> (actor_id, score_i64))
    pub player_count: u32,          // Informational bounded snapshot count
    pub signer_actor: ActorIdV1,    // World owner ActorIdV1
    pub delegation_id: Option<[u8; 32]>, // None = owner-signed; Some(id) = valid active AnchorDelegateGrantV1
}
```

- **Studio Tier-1 structural bounds enforced verbatim (`STUDIO_PLATFORM_SPEC.md` §5.3):**
  - `<= 64` PlayerData keys per `(world_id, actor_id)`
  - `<= 64` UTF-8 bytes per PlayerData key
  - `<= 2,048` UTF-8 bytes per PlayerData value
  - `<= 16` badges per `world_id` (compact `u16` bitmask + canonical badge-ID table)
  - `<= 12 KiB` proof byte ceiling (`BOUNDS-1`) for `PlayerStateProofV1`.

---

## 6. Reserved Post-Soak HTTP Proof Doors & `bitblock-light-wasm` Budget

*(No `node/` or `bitblock-light-wasm` files change during the 7-day soak; these routes and exports lock the post-Phase-3 integration contract so Gameladder and Studio can plan ahead cleanly.)*

### 6.1 Reserved Read/Verify Proof Doors

| Route | Returns (JSON, `camelCase`) | Verification Anchor |
|---|---|---|
| `GET /proof/item/{worldId}/{itemId}` | `{ ok, itemClass, mintedCount, itemClassRoot, proof }` | `ItemClassRootV1` |
| `GET /proof/entitlement/{actorOrPk}/{worldId}/{itemId}` | `{ ok, actorId, ed25519Hex, worldId, itemId, kind, quantity, serialNos[], entitlementRoot, proof }` | `EntitlementRootV1` (Amendment-1 dual lookup!) |
| `GET /proof/payout/{batchId}` | `{ ok, receipt, settlementTag, simulated: true, testOnly: true, disclaimer: "SIMULATED-TEST-ONLY-NO-REAL-MONEY" }` | `SimulatedSettlementReceiptV1` |
| `GET /proof/world-state/{worldId}` | `{ ok, anchor, badgesRoot, playerdataRoot, leaderboardRoot, epoch }` | `WorldStateAnchorV1` |
| `GET /proof/world-state/{worldId}/player/{actorOrPk}` | `{ ok, actorId, badgeMask, playerdataProofs[], leaderboardProof?, anchorEpoch }` | `PlayerStateProofV1` |

### 6.2 Browser `bitblock-light-wasm` Budget Guard (`<= 250 KiB` Gzip)

- Current `bitblock-light-wasm` gzipped size: **`149,181 B` (`145.7 KiB`)** vs **`256,000 B` (`250 KiB`)** budget (`106,819 B` headroom).
- Planned post-soak additive exports (`verify_entitlement_proof_json`, `verify_publish_attestation_json`, `verify_world_state_proof_json`) reuse the exact same pure-Rust SHA-256 + Ed25519 primitives already compiled into `bitblock-light-wasm`, keeping projected total size **`< 175 KiB` gzipped** (well inside `250 KiB`).

---

## 7. Cross-Session Alignment Questions (For Design Hard Stop Sign-Off)

Before Phase B (`PRs #120`–`#125`) starts, Gameladder (`1db6dc30`) and Bitblock Studio (`0a466d62`) are asked to confirm or propose amendments on these five concrete points:

1. **Secondary-trade fee & royalty schedule (`TradeItemV1`):** Confirm opt-in `tradable: bool` per `ItemClassV1`, creator royalty `0..=1,000 bps` (`0–10%`), fixed `500 bps` (`5%`) platform fee, remainder (`85–95%`) to seller, and `price_bits >= 20`.
2. **Co-creator & remix split contract (`RevenueSplitPolicyV1`):** Confirm `1..=8` recipients summing strictly to `8,000 bps` (fixed `2,000 bps` platform), immutable `remixedFrom` share (`0..=2,000 bps`) locked at first publish, and remainder-to-primary-creator integer rounding.
3. **Publish & runtime signature preimages (`WorldPublishAttestationV1`):** Confirm the exact `gameladder-studio-publish/v1\0` and `gameladder-studio-runtime/v1\0` canonical preimages for `STUDIO_PLATFORM_SPEC.md` §4.3 `creator_sig_ed25519` and §5.1 `sig_ed25519`.
4. **World checkpoint delegation (`AnchorDelegateGrantV1`):** Confirm how Gameladder's `CzHub`/relayer signer will request/store a creator's signed `AnchorDelegateGrantV1` (and renewal before `expires_at_epoch`) so `WorldStateAnchorV1` checkpoints publish cleanly without requiring the creator's browser online.
5. **JSON field-name boundary:** Confirm `camelCase` on HTTP proof-door JSON responses (matching existing `/proof/wallet/{addr}` and `/api/bits/*`) paired with `snake_case` inside canonical Studio `economy_manifest` payloads, with Amendment-1 dual lookup (`actorId` or raw `ed25519Hex`) on all player/creator read doors.

---

## 8. Execution & Verification Roadmap

| PR | Scope | Hard Gates |
|---|---|---|
| **PR `#119` (Phase A)** | `docs/PLATFORM_ECONOMY_V2_DESIGN.md` + `index/` + `~/platform/PLATFORM.md` proposal | `check_planning_docs --self-test`, `bbgraph gates && selftest`, `bbverify full`, `0` Rust/`node/` diffs, **10/10 CI green** → **DESIGN HARD STOP** |
| **PR `#120` (Phase B)** | `bits_items_1.rs` + `tools/bits_items_1_vectors.py` + `tests/wave37_bits_items_1.rs` | `wasm32` clippy, `bbmutate` 100%, `bbverify full + quick`, `0` `node/` diffs, **10/10 CI green** |
| **PR `#121` (Phase B)** | `proof/BitblockProof/BitsItemsRefinement.lean` + Rust correspondence | `lake build && --trust=0` (`[propext, Quot.sound]` only, `0` warnings), **10/10 CI green** |
| **PR `#122` (Phase B)** | `bits_payouts_1.rs` (`RevenueSplitPolicyV1`, payouts, `WorldPublishAttestationV1`) + vectors + `wave38` | `wasm32` clippy, `bbmutate` 100%, `bbverify full + quick`, `0` `node/` diffs, **10/10 CI green** |
| **PR `#123` (Phase B)** | `proof/BitblockProof/BitsPayoutsRefinement.lean` + Rust correspondence | `lake build && --trust=0` (`[propext, Quot.sound]` only, `0` warnings), **10/10 CI green** |
| **PR `#124` (Phase B)** | `game_state_1.rs` + vectors + `wave39` + `platform_economy_v2_probe.rs` | `wasm32` clippy, `bbmutate` 100%, `bbverify full + quick`, `0` `node/` diffs, **10/10 CI green** |
| **PR `#125` (Phase B)** | Merged-head `docs/PLATFORM_ECONOMY_V2_RECEIPT.md` + `docs/platform-economy-v2-v1.json` + visually verified chart PNG | `bbgraph gates && selftest`, `bbverify full`, `0` `node/` diffs, **10/10 CI green** |
