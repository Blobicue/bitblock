# Phase 3 Decommission Runbook: Retiring Legacy `rngcoin-node` After the 7-Day Soak

> **Scope & Hard Boundaries:** Strictly testnet/devnet (`lab: true`, `DEVNET-TEST-KEYS-NOT-PRODUCTION-CUSTODY`, `SIMULATED-TEST-ONLY-NO-REAL-MONEY`).
> No mainnet, no genesis with value, no `L1-PRODUCTION`, no `VALUE-1`, no `PROD-OPS-1`, no DNS edits, no new Cloudflare Tunnel, and no tunnel credential changes.
> Gameladder stays behind its soft-launch gate until Owen decides otherwise. Original IP everywhere.
>
> **Timing Gate:** Do **not** execute any step in Section 3 before **`2026-12-02T04:37:21Z`** (end of the 7-day continuous soak) and explicit Owen/driver sign-off recorded in `~/platform/PLATFORM.md`.

---

## 1. Dual-Gate Readiness Check (Read-Only)

Run the read-only soak scorecard outside `node/`:

```bash
node tools/soak_report.mjs --markdown
node tools/soak_report.mjs --json
```

Phase 3 requires **both** gates to report `true`:

| Gate | Exact Exit Criteria |
|---|---|
| **Gate 1 — `chainSoakReady`** | Continuous window `>= 168h` (`>= 600` probes at 15-minute cadence), probe pass rate `>= 99.5%`, `0` Ed25519 HotStuff QC verification failures, `0` closed supply conservation failures, `0` HTTP `413` body-cap or `<= 100` `/proof/blocks` cap failures, `0` validator availability shortfalls (`< 3/4` online), `0` `rollbackRecommended: true` events, strictly non-decreasing finalized height, and hot fallback `rng-node-0..3` online (`4/4`) across `>= 99.5%` of probes. |
| **Gate 2 — `fullPhase3Ready`** | `chainSoakReady === true`, **AND** `bb-relayer` (`127.0.0.1:8690`) verified `active` (`relayerActiveSamples >= 1` and `>= 24h` active window after Owen's single `~/rngcoin-node/scripts/sync-relayer-gate-key.sh` run) with end-to-end gated Gameladder flows passing (`ActorIdV1` bind, `1,000`-Bit signup grant from `TestnetGrantPool`, atomic `80/20` `PurchaseSplit`, `SimulatedPayout`, and Chrome `/l1.html` green). |

If `chainSoakReady === true` at `2026-12-02T04:37:21Z` while `fullPhase3Ready === false` solely because Owen's gate-key sync is still pending, keep `rng-node-0..3` hot and wait for the 24-hour active relayer confirmation before proceeding.

---

## 2. Pre-Retirement Read-Only Archival Snapshot

Before stopping any legacy `rngcoin-node` container, record an immutable final snapshot on `spark-7117` (never copying secrets or private keys into any git repository):

```bash
ARCHIVE_DIR="/home/juzz/backups/rngcoin-testnet-phase3-$(date -u +%Y%m%dT%H%M%SZ)"
mkdir -p "$ARCHIVE_DIR"
node tools/soak_report.mjs --json > "$ARCHIVE_DIR/soak-scorecard-final.json"
curl -sS http://127.0.0.1:8545/health > "$ARCHIVE_DIR/rng-node-0-final-health.json"
curl -sS https://testnet-rpc.gameladder.io/health > "$ARCHIVE_DIR/bb-public-final-health.json"
docker ps --format 'table {{.Names}}\t{{.Image}}\t{{.Status}}' > "$ARCHIVE_DIR/containers-pre-retire.txt"
```

Verify `$ARCHIVE_DIR/soak-scorecard-final.json` shows `"fullPhase3Ready": true` before continuing.

---

## 3. Reversible Stop-Before-Prune Retirement Sequence

All steps below are **local container/config operations on `spark-7117` only** — zero Cloudflare Dashboard changes, zero DNS edits, and zero tunnel credential modifications.

### Step 3.1 — Confirm `bb-relayer` Gate-Key Ownership Independence
During Phase 2, `bb-relayer` mounts `~/rngcoin-node/.runtime/relayer` read-only at `/etc/rng-relayer:ro` so Owen's single `sync-relayer-gate-key.sh` run activated both relayers.
- **Do not delete `~/rngcoin-node/.runtime/relayer/relayer.env`** until Owen optionally migrates the gate-key file to `~/bitblock/node/.runtime/relayer/relayer.env` via `node/scripts/sync-relayer-gate-key.sh`. Leaving the read-only mount intact keeps `bb-relayer` uninterrupted even after every `rngcoin-node` container is stopped.

### Step 3.2 — Stop (Do Not Delete) Legacy `rng-testnet-relayer` & `rng-node-0..3`
Because `/` on `rng-rpc-lb` has already served `bb-rpc-lb:8645` cleanly for 7 days, stopping `rng-node-0..3` causes **zero traffic disruption**:

```bash
docker stop rng-testnet-relayer rng-node-0 rng-node-1 rng-node-2 rng-node-3
node node/scripts/check-bits-testnet-health.mjs --rpc https://testnet-rpc.gameladder.io --strict-relayer
```

Hold here for 15 minutes and verify `check-bits-testnet-health.mjs --strict-relayer` passes with zero errors and advancing height on `https://testnet-rpc.gameladder.io`. (If anything unexpected appears, `docker start rng-node-0 rng-node-1 rng-node-2 rng-node-3` restores the hot fallback immediately.)

### Step 3.3 — Optional Local Tunnel Target Simplification (`rng-rpc-lb` → `bb-rpc-lb`)
The existing `rng-testnet-named-tunnel` (`cloudflared`) remains untouched with its existing credentials and DNS record (`testnet-rpc.gameladder.io`):
- **Option A (Zero-touch steady state):** Keep `rng-rpc-lb` (Caddy 2 in `bitblock` mode) as the thin ingress proxy in front of `bb-rpc-lb:8645`, and remove only the `/bits-shadow/*` temporary route once Gameladder retires shadow references.
- **Option B (Direct local bridge handoff):** Update the local `cloudflared` ingress config on `spark-7117` from `http://rpc-lb:8545` to `http://bb-rpc-lb:8645` on the shared Docker bridge, reload/restart `cloudflared` locally, verify `https://testnet-rpc.gameladder.io/health` reports `bitblock-l1-v1` (`chainId: 1112101971`), and then `docker stop rng-rpc-lb`.

### Step 3.4 — Final Verification & Archive Freeze
1. Run `node node/scripts/check-bits-testnet-health.mjs --rpc https://testnet-rpc.gameladder.io --strict-relayer` → confirm `ok: true`, `4/4` validators, `qcVerified: true`, `conservationOk: true`, `bodyCapStatus: 413`, `blocksPageCount: 100`, `relayerMode: "active"`.
2. Verify live Chrome `/l1.html` against `https://testnet-rpc.gameladder.io` → green Ed25519 HotStuff QC, Domain-3 Patricia proof, `0` console errors.
3. Disable the Phase 2 soak timer only after recording the final scorecard:
   ```bash
   systemctl --user disable --now bitblock-phase2-soak.timer
   ```
4. **Only after 24 hours of clean post-stop operation** and explicit Owen approval may stopped `rng-node-0..3` containers be removed (preserving `~/rngcoin-node` repo history and the read-only archival backup).

---

## 4. Gameladder Session (`1db6dc30`) Phase 3 Cleanup Handoff

Once Section 3 is complete and recorded in `~/platform/PLATFORM.md`, Gameladder may ship its own CI-gated cleanup PR behind the soft-launch gate:
- Lock `BITS_ADAPTER_MODE` to `bitblock-merged-testnet` (or retain `auto` with `bitblock-l1-v1` primary),
- Retire legacy `/l1-light.wasm` (`653 KB` BLS12-381 verifier) and legacy `rngcoin-v1` code paths, keeping `/bitblock-light.wasm` (`149,181 B` gzipped, SHA-256 `a4f8a6da8b8ec7f9ed720ca9e1d3cd823136aa8232b724b2af5125406be7649c`) as the single browser light verifier,
- Remove temporary `/bits-shadow` UI presets from `web/l1.html` while keeping the soft-launch gate intact until Owen decides to lift it.
