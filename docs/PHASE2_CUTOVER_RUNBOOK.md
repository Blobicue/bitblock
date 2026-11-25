# Phase 2 Cutover Runbook: Reversible Testnet Transition (`rngcoin-node` → Merged Bitblock L1)

> **Scope & Hard Boundaries:** Strictly testnet/devnet (`lab: true`, `DEVNET-TEST-KEYS-NOT-PRODUCTION-CUSTODY`, `SIMULATED-TEST-ONLY-NO-REAL-MONEY`).
> No mainnet, no genesis with value, no `L1-PRODUCTION`, no `VALUE-1`, no real-money checkout or payout, no DNS edits, and no new Cloudflare Tunnel or tunnel credentials.
> Gameladder stays behind its soft-launch gate throughout Phase 2. Original IP everywhere.

---

## 1. Zero-Cloudflare / Zero-DNS Switching Architecture

On `spark-7117`, the existing `rng-testnet-named-tunnel` (`cloudflare/cloudflared:latest`) reads a locally managed ingress file mapping `https://testnet-rpc.gameladder.io -> http://rpc-lb:8545` (`rng-rpc-lb`, Caddy 2 on Docker bridge `rngcoin-node_rngnet`).

Because ingress routing terminates at local Caddy on `spark-7117`, **zero Cloudflare Dashboard changes, zero DNS edits, zero new tunnels, and zero tunnel credential changes are required**. Switching and rollback happen purely via local Caddy config:

| Component | Host Port (Localhost Only) | Role During Phase 2 |
|---|---|---|
| `rng-node-0..3` | `127.0.0.1:8545..8548` (P2P `19000..19003`) | **Hot fallback** — stays running and finalizing continuously for the full 7-day soak |
| `rng-rpc-lb` (Caddy 2) | `127.0.0.1:8540` (`rpc-lb:8545` on `rngnet`) | Public tunnel target; hot-reloads between `Caddyfile.cutover-rng` and `Caddyfile.cutover-bitblock` in `< 50 ms` |
| `rng-testnet-relayer` | `127.0.0.1:8590` | Active in `shadow` / `rollback` modes; paused in `bitblock` mode |
| `bb-node-0..3` | `127.0.0.1:8645..8648` (P2P `19100..19103`) | Merged Bitblock L1 Bits testnet validators (`SIGNER-OPS-1` `flock` + `redb` ACID) |
| `bb-rpc-lb` (Caddy 2) | `127.0.0.1:8640` (`bb-rpc-lb:8645` on `rngnet`) | HA primary-with-failover LB fronting `bb-node-0..3` |
| `bb-relayer` (Node 22) | `127.0.0.1:8690` | Single-source `bitblock-light-wasm` Bits relayer; mounts `~/rngcoin-node/.runtime/relayer` **read-only** at `/etc/rng-relayer:ro` |

### Shared Read-Only Gate-Key Architecture (One Sync by Owen)

`bb-relayer` never copies `GAMELADDER_GATE_KEY` into `~/bitblock`. Via `node/deploy/docker-compose.cutover.yml`, `bb-relayer` mounts `~/rngcoin-node/.runtime/relayer` read-only at `/etc/rng-relayer:ro` (`GATE_KEY_ENV_FILE=/etc/rng-relayer/relayer.env`) and re-reads `GAMELADDER_GATE_KEY` dynamically on each poll cycle without ever logging key material.

When Owen runs `~/rngcoin-node/scripts/sync-relayer-gate-key.sh` **once**, both `rng-testnet-relayer` and `bb-relayer` receive the active gate key. `node/scripts/sync-relayer-gate-key.sh` is retained strictly as a post-Phase-3 / standalone fallback.

---

## 2. Gameladder Session (`1db6dc30`) Handoff Contract

Gameladder owns its repo (`~/gameladder-1`) and production CD behind the soft-launch gate via its own CI-gated PRs. To guarantee rollback remains **100% local config on `spark-7117` with zero emergency Worker redeploy**, Gameladder deploys a **dual-protocol** client and adapter **before** `/` flips:

1. **Browser `wasm32` Light Verifier (`/bitblock-light.wasm` alongside legacy `/l1-light.wasm`):**
   - Build command in `~/bitblock`:
     ```bash
     cargo build --manifest-path node/Cargo.toml --release --target wasm32-unknown-unknown -p bitblock-light-wasm
     ```
   - Artifact path: `node/target/wasm32-unknown-unknown/release/bitblock_light_wasm.wasm`
   - Verified release size: **`384,201 B` raw / `149,181 B` (`145.7 KiB`) gzipped** (well under the `256,000 B` / `250 KiB` budget; zero `bls12_381`, zero DKG, zero Law L9).
   - Reference SHA-256: `a4f8a6da8b8ec7f9ed720ca9e1d3cd823136aa8232b724b2af5125406be7649c`.
   - Exported C-ABI (`alloc`, `dealloc`, `verify_wallet_proof_json`, `verify_header_qc_json`, `verify_epoch_handoff_json`, `derive_actor_id_json`, `encode_tx_signing_bytes_json`, `encode_tx_tlv_json`).
2. **Protocol Auto-Detection (`GET /proof`):**
   - Merged Bitblock L1 returns `"protocol": "bitblock-l1-v1"`, `"chainId": 1112101971`, `"lab": true`, `"unit": { "name": "Bits", "symbol": "BITS", "baseUnit": 1, "creatorSplitBps": 8000, "treasurySplitBps": 2000 }`, `"systemActors": { "platformTreasury", "grantPool", "payoutEscrow" }`, and Ed25519 HotStuff `head.header` + `head.qc`.
   - Legacy `rngcoin-node` omits `"protocol": "bitblock-l1-v1"`.
   - Gameladder's `server/bits-adapter.js` and `web/l1-wasm.js` auto-select Bitblock mode when `proof.protocol === "bitblock-l1-v1"` and seamlessly fall back to legacy `rngcoin-v1` if `/` is reverted.
3. **Cross-Session Amendment 1 (`ActorIdV1` Binding):**
   - `GET /proof/wallet/{addr}` accepts **either** canonical 64-hex `ActorIdV1` **or** raw 64-hex Ed25519 `pubkeyHex`, and always returns `{ ok: true, actorId, ed25519Hex, pubkeyHex, balance, seq, root, proofHex, domainId: 3, height }` (including non-membership Patricia proofs for unfunded accounts).
   - Gameladder stores `actorId` alongside `pubkeyHex` in `Identity` DO and resolves balances by `actorId` on the merged chain so key rotations preserve balance and receipt history.
4. **Pre-Cutover Live Probe Route (`/bits-shadow/*`):**
   - Once `cutover-switch.sh --mode shadow` is armed, `https://testnet-rpc.gameladder.io/bits-shadow/*` proxies cleanly (prefix-stripped) to `bb-rpc-lb:8645` while `/` stays on `rngcoin-node`, allowing Gameladder to run gated live verification prior to the `/` cutover.

---

## 3. Step-by-Step Cutover & Rollback Procedures

### Step 3.1 — Boot Persistent Localhost Shadow Cluster (`bb-node-0..3` + `bb-rpc-lb` + `bb-relayer`)

```bash
./node/deploy/init-docker-testnet.sh
BB_UID=$(id -u) BB_GID=$(id -g) docker compose \
  -p bitblock-shadow \
  -f node/deploy/docker-compose.testnet.yml \
  -f node/deploy/docker-compose.cutover.yml \
  --profile relayer up -d --build
node node/scripts/check-bits-testnet-health.mjs --rpc http://127.0.0.1:8640
```

At this stage, `https://testnet-rpc.gameladder.io/` remains **100% untouched** on legacy `rngcoin-node`.

### Step 3.2 — Arm `/bits-shadow/*` Pre-Cutover Probe (Root `/` Still on `rngcoin-node`)

After Gameladder's dual-protocol PR is ready to test against `/bits-shadow/*`:

```bash
bash node/scripts/cutover-switch.sh --mode shadow
```

- First-time arming recreates `rng-rpc-lb` once (~0.5s, zero `cloudflared` restart, zero git diffs inside `~/rngcoin-node`) with directory mount `node/.runtime/cutover:/etc/caddy:ro` and container-local `admin localhost:2019`.
- Verify `https://testnet-rpc.gameladder.io/proof` still serves `rngcoin-node` and `https://testnet-rpc.gameladder.io/bits-shadow/proof` serves `protocol: "bitblock-l1-v1"`.

### Step 3.3 — Owner Gate-Key Sync (Single Action by Owen)

Owen runs locally on `spark-7117`:

```bash
~/rngcoin-node/scripts/sync-relayer-gate-key.sh
```

Both `rng-testnet-relayer` and `bb-relayer` pick up the active gate key automatically (zero secret copied into `~/bitblock`).

### Step 3.4 — Flip `/` to Merged Bitblock L1 (Driver Go-Ahead)

Once Gameladder's dual-protocol PR is live behind the soft-launch gate and both relayers can go active:

```bash
bash node/scripts/cutover-switch.sh --mode bitblock
node node/scripts/check-bits-testnet-health.mjs --rpc https://testnet-rpc.gameladder.io --strict-relayer
```

- Hot-reloads `rng-rpc-lb` in `< 50 ms` (zero tunnel restart).
- Pauses `rng-testnet-relayer` and activates `bb-relayer` as the single active relayer while `rng-node-0..3` continue running hot on `127.0.0.1:8545..8548`.

### Step 3.5 — Instant Config-Only Rollback (`< 50 ms`, Zero Worker Redeploy)

If any rollback trigger fires during the 7-day soak:

```bash
bash node/scripts/cutover-switch.sh --mode rollback
```

- Hot-reloads `Caddyfile.cutover-rng` onto `rng-rpc-lb` (`/` back to hot `rng-node-0..3`), stops `bb-relayer`, and restarts `rng-testnet-relayer`.
- Because Gameladder is dual-protocol, no emergency Cloudflare Worker deploy is needed.

---

## 4. 7-Day Continuous Soak Criteria & Rollback Triggers

### Mandatory Immediate Rollback Triggers
Execute `bash node/scripts/cutover-switch.sh --mode rollback` immediately if any of the following occur:
1. Fewer than `3/4` Bitblock validators finalizing blocks for `> 60` seconds.
2. Any `bitblock-light-wasm` Ed25519 HotStuff QC or `AUTH-COMMIT-1` Domain 3 Patricia proof verification failure.
3. Any closed supply conservation mismatch (`sum != genesis_supply`) or unexpected `lab !== true` flag.
4. Any `SIGNER-OPS-1` lock contention, corrupt watermark, or equivocation alert.
5. Persistent `5xx` responses on `testnet-rpc.gameladder.io` or `bb-relayer` stuck in `degraded` state.

### Phase 3 Unlock Criteria (Retiring `rngcoin-node`)
Legacy `rngcoin-node` containers and legacy `/l1-light.wasm` may ONLY be retired in Phase 3 after:
1. **7 continuous days** of green `node node/scripts/check-bits-testnet-health.mjs --rpc https://testnet-rpc.gameladder.io --strict-relayer` checks (`rollbackRecommended: false`, `0` unplanned rollbacks).
2. Verified end-to-end gated `gameladder.io` flows on the live merged testnet: Amendment 1 `ActorIdV1` bind, `1,000`-Bit signup grant from `TestnetGrantPool` (zero runtime minting), atomic `80/20` `BitsPurchaseSplitV1`, `BitsSimulatedPayoutV1`, and live Chrome `/l1.html` verification with zero console errors.
3. Explicit Owner/driver sign-off recorded in `~/platform/PLATFORM.md`.
