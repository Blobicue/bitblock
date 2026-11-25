#!/usr/bin/env bash
# Zero-DNS, zero-Cloudflare local traffic switch & instant rollback for Phase 2 cutover on spark-7117.
#
# Modes:
#   --mode shadow    Root `/` stays on legacy rng-node-0..3; `/bits-shadow/*` proxies to bb-rpc-lb:8645.
#   --mode bitblock  Root `/` AND `/bits-shadow/*` route to bb-rpc-lb:8645; pauses rng-testnet-relayer
#                    (rng-node-0..3 stay hot as 7-day fallback) and starts bb-relayer.
#   --mode rollback  Reverts `/` back to legacy rng-node-0..3 (<50ms caddy reload), stops bb-relayer,
#                    and restarts rng-testnet-relayer.
#
# Guarantees:
#   - NEVER touches Cloudflare API, DNS, tunnel credentials, or ~/rngcoin-node git files.
#   - Uses directory bind-mount node/.runtime/cutover:/etc/caddy:ro + container-local `admin localhost:2019`
#     so every switch after initial arming is a zero-drop `<50ms` `caddy reload` with zero tunnel restart.
#   - Supports `--dry-run` (validated in CI without Docker).
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
NODE_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
DEPLOY_DIR="$NODE_DIR/deploy"
RUNTIME_CUTOVER_DIR="$NODE_DIR/.runtime/cutover"
RNG_REPO_DIR="${RNG_REPO_DIR:-$HOME/rngcoin-node}"
PUBLIC_RPC_URL="${PUBLIC_RPC_URL:-https://testnet-rpc.gameladder.io}"
LOCAL_BB_RPC="${LOCAL_BB_RPC:-http://127.0.0.1:8640}"

MODE=""
DRY_RUN=0
SKIP_HEALTH=0

while [[ $# -gt 0 ]]; do
  case "$1" in
    --mode)
      MODE="${2:-}"
      shift 2
      ;;
    --dry-run)
      DRY_RUN=1
      shift
      ;;
    --skip-health)
      SKIP_HEALTH=1
      shift
      ;;
    -h|--help)
      sed -n '2,18p' "$0"
      exit 0
      ;;
    *)
      echo "ERROR: unknown argument: $1" >&2
      exit 2
      ;;
  esac
done

case "$MODE" in
  shadow|bitblock|rollback) ;;
  *)
    echo "ERROR: --mode must be one of: shadow | bitblock | rollback" >&2
    exit 2
    ;;
esac

if [[ "$MODE" == "bitblock" ]]; then
  SRC_CADDYFILE="$DEPLOY_DIR/Caddyfile.cutover-bitblock"
  EXPECTED_PROTOCOL="bitblock-l1-v1"
else
  SRC_CADDYFILE="$DEPLOY_DIR/Caddyfile.cutover-rng"
  EXPECTED_PROTOCOL="rngcoin-legacy"
fi

if [[ ! -f "$SRC_CADDYFILE" ]]; then
  echo "ERROR: missing target Caddyfile: $SRC_CADDYFILE" >&2
  exit 1
fi

# Static sanity checks on checked-in Caddyfiles (runs in both --dry-run and live mode).
grep -q "admin localhost:2019" "$SRC_CADDYFILE" || { echo "ERROR: Caddyfile missing admin localhost:2019" >&2; exit 1; }
grep -q "max_size 64KB" "$SRC_CADDYFILE" || { echo "ERROR: Caddyfile missing 64KB request body limit" >&2; exit 1; }
grep -q "bb-rpc-lb:8645" "$SRC_CADDYFILE" || { echo "ERROR: Caddyfile missing bb-rpc-lb:8645 upstream" >&2; exit 1; }

if [[ "$DRY_RUN" -eq 1 ]]; then
  echo "[cutover-switch][DRY-RUN] mode=$MODE"
  echo "[cutover-switch][DRY-RUN] target_caddyfile=$SRC_CADDYFILE (validated)"
  echo "[cutover-switch][DRY-RUN] runtime_mount=$RUNTIME_CUTOVER_DIR -> /etc/caddy-cutover:ro (zero edits to $RNG_REPO_DIR)"
  echo "[cutover-switch][DRY-RUN] cloudflare_dns_changes=0 cloudflared_restarts=0"
  if [[ "$MODE" == "bitblock" ]]; then
    echo "[cutover-switch][DRY-RUN] relayer_action=start bb-relayer, stop rng-testnet-relayer (rng-node-0..3 stay hot)"
  else
    echo "[cutover-switch][DRY-RUN] relayer_action=stop bb-relayer (if running), ensure rng-testnet-relayer running"
  fi
  echo "CUTOVER_SWITCH_DRY_RUN=PASS mode=$MODE"
  exit 0
fi

# 1. Fail-closed pre-flight health check on Bitblock shadow cluster before routing any traffic to it.
if [[ "$MODE" != "rollback" && "$SKIP_HEALTH" -eq 0 ]]; then
  echo "[cutover-switch] Running pre-flight health check against $LOCAL_BB_RPC..."
  node "$SCRIPT_DIR/check-bits-testnet-health.mjs" --rpc "$LOCAL_BB_RPC" --skip-relayer
fi

# 2. Ensure bb-rpc-lb is attached to rngcoin-node_rngnet with alias bb-rpc-lb.
if ! docker network inspect rngcoin-node_rngnet --format '{{range .Containers}}{{.Name}} {{end}}' | grep -qw "bb-rpc-lb"; then
  echo "[cutover-switch] Connecting bb-rpc-lb to rngcoin-node_rngnet (alias bb-rpc-lb)..."
  docker network connect --alias bb-rpc-lb rngcoin-node_rngnet bb-rpc-lb
fi

# 3. Atomically stage target Caddyfile inside gitignored runtime directory.
mkdir -p "$RUNTIME_CUTOVER_DIR"
cp "$SRC_CADDYFILE" "$RUNTIME_CUTOVER_DIR/Caddyfile.tmp.$$"
mv -f "$RUNTIME_CUTOVER_DIR/Caddyfile.tmp.$$" "$RUNTIME_CUTOVER_DIR/Caddyfile"

# 4. Ensure rng-rpc-lb mounts RUNTIME_CUTOVER_DIR at /etc/caddy-cutover:ro and runs Caddy against
#    /etc/caddy-cutover/Caddyfile (avoids Docker Compose appending onto the base /etc/caddy/Caddyfile
#    single-file mount; one-time ~0.5s recreate, then zero-drop <50ms `caddy reload` forever).
CURRENT_MOUNT="$(docker inspect rng-rpc-lb --format '{{range .Mounts}}{{.Source}}:{{.Destination}} {{end}}' 2>/dev/null || true)"
if [[ "$CURRENT_MOUNT" != *"${RUNTIME_CUTOVER_DIR}:/etc/caddy-cutover"* ]]; then
  echo "[cutover-switch] Arming rng-rpc-lb with directory mount ${RUNTIME_CUTOVER_DIR}:/etc/caddy-cutover:ro (one-time recreate, zero cloudflared restart)..."
  TMP_OVERRIDE="$RUNTIME_CUTOVER_DIR/rng-rpc-lb.override.yml"
  cat > "$TMP_OVERRIDE" <<EOF
services:
  rpc-lb:
    command: ["caddy", "run", "--config", "/etc/caddy-cutover/Caddyfile", "--adapter", "caddyfile"]
    volumes:
      - ${RUNTIME_CUTOVER_DIR}:/etc/caddy-cutover:ro
EOF
  RNG_UID="$(id -u)" RNG_GID="$(id -g)" docker compose \
    -p rngcoin-node \
    -f "${RNG_REPO_DIR}/docker-compose.testnet.yml" \
    -f "$TMP_OVERRIDE" \
    --project-directory "$RNG_REPO_DIR" \
    up -d --no-deps --force-recreate rpc-lb
  sleep 2
else
  echo "[cutover-switch] Hot-reloading rng-rpc-lb Caddyfile (<50ms, zero tunnel restart)..."
  docker exec rng-rpc-lb caddy reload --config /etc/caddy-cutover/Caddyfile --adapter caddyfile
fi

# 5. Single-active relayer orchestration (rng-node-0..3 stay running hot in ALL modes).
if [[ "$MODE" == "bitblock" ]]; then
  echo "[cutover-switch] Activating bb-relayer and pausing rng-testnet-relayer (rng-node-0..3 remain hot)..."
  docker stop rng-testnet-relayer >/dev/null 2>&1 || true
  BB_UID="$(id -u)" BB_GID="$(id -g)" docker compose \
    -p bitblock-shadow \
    -f "$DEPLOY_DIR/docker-compose.testnet.yml" \
    -f "$DEPLOY_DIR/docker-compose.cutover.yml" \
    --profile relayer \
    up -d --no-deps relayer
else
  echo "[cutover-switch] Ensuring single active rng-testnet-relayer (stopping bb-relayer if running)..."
  docker stop bb-relayer >/dev/null 2>&1 || true
  docker start rng-testnet-relayer >/dev/null 2>&1 || true
fi

# 6. Immediate post-switch verification against live local LB + public tunnel URL.
sleep 1
echo "[cutover-switch] Verifying local LB http://127.0.0.1:8540/proof..."
LOCAL_PROOF="$(curl -fsS --max-time 8 http://127.0.0.1:8540/proof)"
if [[ "$EXPECTED_PROTOCOL" == "bitblock-l1-v1" ]]; then
  echo "$LOCAL_PROOF" | grep -q '"protocol":"bitblock-l1-v1"' || {
    echo "ERROR: post-switch http://127.0.0.1:8540/proof did not return bitblock-l1-v1!" >&2
    exit 1
  }
else
  echo "$LOCAL_PROOF" | grep -q '"ok":true' || {
    echo "ERROR: post-switch http://127.0.0.1:8540/proof failed!" >&2
    exit 1
  }
fi

echo "[cutover-switch] Verifying /bits-shadow/proof via http://127.0.0.1:8540/bits-shadow/proof..."
curl -fsS --max-time 8 http://127.0.0.1:8540/bits-shadow/proof | grep -q '"protocol":"bitblock-l1-v1"'

echo "[cutover-switch] SUCCESS — mode=$MODE active on rng-rpc-lb (public URL: $PUBLIC_RPC_URL, 0 DNS/tunnel changes)."
