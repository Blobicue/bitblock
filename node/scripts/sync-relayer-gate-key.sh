#!/usr/bin/env bash
# FALLBACK-ONLY operator script for syncing Gameladder GATE_PASSWORD into ~/bitblock/node/.runtime/relayer/relayer.env.
#
# PRIMARY PATH (recommended while rngcoin-node is hot fallback):
#   Owen runs ~/rngcoin-node/scripts/sync-relayer-gate-key.sh ONCE.
#   `bb-relayer` mounts ~/rngcoin-node/.runtime/relayer read-only at `/etc/rng-relayer:ro`
#   (`GATE_KEY_ENV_FILE=/etc/rng-relayer/relayer.env`) and picks up the fresh key live on its
#   next poll tick — zero secret is ever copied into ~/bitblock.
#
# Use THIS fallback script only after Phase 3 retires ~/rngcoin-node or on a standalone host.
# Never run by agents; Owen's hands only.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
NODE_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
ENV_FILE="${NODE_DIR}/.runtime/relayer/relayer.env"
GAMELADDER_URL="${GAMELADDER_URL:-https://gameladder.io}"

RAW_KEY=""
if [[ "${1:-}" == "--stdin" ]]; then
  IFS= read -r -d '' RAW_KEY || true
else
  read -rsp "Enter current Gameladder GATE_PASSWORD (hidden): " RAW_KEY
  echo ""
fi

CLEAN_KEY="$(printf '%s' "$RAW_KEY" | node -e '
  const fs = require("fs");
  const raw = fs.readFileSync(0, "utf8");
  const clean = raw.replace(/^\uFEFF/, "").replace(/[\r\n]+$/, "");
  process.stdout.write(clean);
')"

if [[ -z "$CLEAN_KEY" ]]; then
  echo "ERROR: empty GATE_PASSWORD provided — aborting without touching ${ENV_FILE}" >&2
  exit 2
fi

SHA_PREFIX="$(printf '%s' "$CLEAN_KEY" | sha256sum | cut -c1-12)"
echo "[bb-relayer-sync] Verifying key (sha256:${SHA_PREFIX}..., len=${#CLEAN_KEY}) against ${GAMELADDER_URL}/api/market/proof..."

HTTP_CODE="$(curl -s -o /dev/null -w '%{http_code}' --max-time 10 \
  -H "x-gate-key: ${CLEAN_KEY}" \
  "${GAMELADDER_URL}/api/market/proof")"

if [[ "$HTTP_CODE" != "200" ]]; then
  echo "ERROR: ${GAMELADDER_URL}/api/market/proof returned HTTP ${HTTP_CODE} — aborting without touching ${ENV_FILE}" >&2
  exit 1
fi

mkdir -p "$(dirname "$ENV_FILE")"
TMP_FILE="${ENV_FILE}.tmp.$$"
umask 077
printf 'GAMELADDER_GATE_KEY=%s\n' "$CLEAN_KEY" > "$TMP_FILE"
chmod 600 "$TMP_FILE"
mv -f "$TMP_FILE" "$ENV_FILE"
echo "[bb-relayer-sync] Updated ${ENV_FILE} (mode 0600, sha256:${SHA_PREFIX}...)."

echo "[bb-relayer-sync] Recreating bb-relayer..."
BB_UID="$(id -u)" BB_GID="$(id -g)" docker compose \
  -p bitblock-shadow \
  -f "${NODE_DIR}/deploy/docker-compose.testnet.yml" \
  --profile relayer \
  up -d --no-deps --force-recreate --no-build relayer

sleep 5
node "${SCRIPT_DIR}/check-bits-testnet-health.mjs" --strict-relayer
echo "[bb-relayer-sync] SUCCESS — bb-relayer is active!"
