#!/usr/bin/env python3
"""Clean-room pure-Python independent executor & vector suite for EXECUTOR-1 (PS-065).

Imports NO Rust or Bitblock code. Implements an independent launch-profile state machine
across all 10 active launch domains (COMMIT-1 World/ActiveToken/Episode + Domains 1..=7
Authorization/Nonce/NativeBalance/EpisodeOwnership/Governance/EconomicStake/ValidatorSchedule)
plus the fail-closed INACTIVE Program/VM band (80..=95) and unassigned tags (96..=255).

Reconstructs both Wave 16 canonical schemas:
  * bitblock/executor-1/block-batch/v1
  * bitblock/executor-1/execution-receipt/v1
"""

from __future__ import annotations

import copy
import hashlib
import struct
import sys

from asset_1_vectors import encode_episode_value, episode_trie_key
from auth_commit_1_vectors import (
    DOM_AUTH,
    DOM_ECONOMIC_STAKE,
    DOM_EPISODE_OWNERSHIP,
    DOM_GOVERNANCE,
    DOM_NATIVE_BALANCE,
    DOM_NONCE,
    DOM_VALIDATOR_SCHEDULE,
    DomainPatriciaPy,
)
from commit_state_vectors import (
    COMMIT_1_ACTIVE_TOKEN_ROOT_SCHEMA,
    COMMIT_1_EPISODE_ROOT_SCHEMA,
    COMMIT_1_OP_LIMITS_SCHEMA,
    COMMIT_1_PROFILE_SCHEMA,
    COMMIT_1_PROOF_POLICY_SCHEMA,
    COMMIT_1_WORLD_ROOT_SCHEMA,
    DOMAIN_ACTIVE_TOKEN,
    DOMAIN_EPISODE,
    DOMAIN_WORLD,
    KEY_COORD_V1,
    KEY_EVENT_V1,
    ON_V1,
    ROOT_ACTIVE_TOKEN,
    ROOT_EPISODE,
    ROOT_WORLD,
    VAL_EPISODE_V1,
    VAL_MINT_V1,
    VAL_ON_V1,
    build_patricia,
    coord_key,
    encode_op_limits,
    encode_profile,
    encode_proof_policy,
    episode_record,
)
from crypto_d1_vectors import (
    content_tag,
    ed_public_from_seed,
    ed_sign,
    ed_verify_strict,
    encode_signing_preimage,
)
from economic_state_1_vectors import encode_econ_root, encode_val_record, u128_be
from governance_1_vectors import encode_gov_root, param_trie_key
from identity_1_vectors import (
    derive_actor_id,
    encode_actor_auth_state,
    encode_auth_descriptor,
    encode_threshold_policy,
    encode_tx_body,
    pk,
)
from validator_schedule_1_vectors import encode_epoch_schedule, epoch_trie_key

BATCH_SCHEMA = b"bitblock/executor-1/block-batch/v1"
RECEIPT_SCHEMA = b"bitblock/executor-1/execution-receipt/v1"

OP_BASE_PLACE_VOXEL = 1
OP_BASE_DELETE_VOXEL = 2
OP_AUTH_COMMIT_TX = 16
OP_ASSET_TRANSFER = 32
OP_ASSET_MINT_DEPOSIT = 33
OP_ASSET_BURN_REFUND = 34
OP_ECON_BOND_STAKE = 48
OP_ECON_SLASH_EQUIVOCATION = 49
OP_SCHEDULE_EPOCH = 50
OP_GOV_ADVANCE_EPOCH = 64
OP_GOV_SAFE_HALT = 65

DISP_ADMITTED = 0
DISP_INACTIVE_BAND = 1
DISP_UNKNOWN_TAG = 2
DISP_MALFORMED = 3
DISP_BEDROCK = 4
DISP_NOOP = 5
DISP_AUTH_FAILED = 6
DISP_ASSET_FAILED = 7
DISP_VAL_SCHED_FAILED = 8
DISP_GOV_FAILED = 9
DISP_GAS_LIMIT = 10

WORLD_SCHEMA = b"bitblock/world/chunk-merkle-sha256/v2"
WORLD_CHUNK_SCHEMA = b"bitblock/world/chunk-bits-i64-be/v2"
HIST_GENESIS_SCHEMA = b"bitblock/history/genesis/v1"
HIST_EVENT_SCHEMA = b"bitblock/history/event/v1"
HIST_RECEIPT_SCHEMA = b"bitblock/history/receipt/v1"
BOUNDS_SAFE_HALT_SCHEMA = b"bitblock/bounds-1/safe-halt-evidence/v1"

# Default RESOURCE-1 & TRANSITION-SPEC-1 launch parameters
BASE_OP_GAS = 2_000
GAS_PER_TX_BYTE = 16
BAND_SPECS = [
    # (slot_id, op_min, op_max, active, write_mask, base_gas, per_byte_gas)
    (1, 16, 31, True, (1 << 3) | (1 << 4), 21_000, 16),
    (2, 32, 47, True, (1 << 5) | (1 << 6), 25_000, 16),
    (3, 48, 63, True, (1 << 7) | (1 << 8), 35_000, 16),
    (4, 64, 79, True, 1 << 9, 40_000, 16),
    (5, 80, 95, False, 1 << 10, 50_000, 128),
]
BASE_DOMAINS_MASK = 0b111
MAX_BATCH_GAS = 100_000_000


def world_tag_py(coords: set[tuple[int, int, int]]) -> bytes:
    chunks: dict[tuple[int, int, int], list[int]] = {}
    for x, y, z in coords:
        cx, rx = divmod(x, 16)
        cy, ry = divmod(y, 16)
        cz, rz = divmod(z, 16)
        words = chunks.setdefault((cx, cy, cz), [0] * 64)
        idx = rx + 16 * (ry + 16 * rz)
        words[idx // 64] |= 1 << (idx % 64)
    leaves = []
    for (cx, cy, cz) in sorted(chunks):
        words = chunks[(cx, cy, cz)]
        chunk_bytes = struct.pack(">qqq", cx, cy, cz) + b"".join(
            struct.pack(">Q", w) for w in words
        )
        _, c_dig = content_tag("world", WORLD_CHUNK_SCHEMA, chunk_bytes)
        leaves.append((cx, cy, cz, c_dig))
    root_bytes = struct.pack(">Q", len(leaves)) + b"".join(
        struct.pack(">qqq", cx, cy, cz) + dig for cx, cy, cz, dig in leaves
    )
    _, w_dig = content_tag("world", WORLD_SCHEMA, root_bytes)
    return w_dig


def encode_fork_context(
    chain_id: int, world_id: int, genesis_id: bytes, profile_id: bytes, spec_digest: bytes
) -> bytes:
    return struct.pack(">QQ", chain_id, world_id) + genesis_id + profile_id + spec_digest


def encode_snapshot(
    batch_height: int, epoch: int, roots10: list[bytes], program_active: bool = False
) -> bytes:
    assert len(roots10) == 10
    out = (
        struct.pack(">HQQ", 1, batch_height, epoch)
        + b"".join(roots10)
        + bytes([1 if program_active else 0])
    )
    assert len(out) == 339
    return out


def encode_batch(
    batch_height: int,
    epoch: int,
    fork_ctx: bytes,
    base_fee: int,
    ops: list[tuple[int, bytes]],
) -> bytes:
    out = (
        struct.pack(">HQQ", 1, batch_height, epoch)
        + fork_ctx
        + struct.pack(">QI", base_fee, len(ops))
    )
    for tag, payload in ops:
        out += struct.pack(">BH", tag, len(payload)) + payload
    return out


def decode_batch(raw: bytes) -> tuple[int, int, bytes, int, list[tuple[int, bytes]]]:
    assert len(raw) >= 142
    ver, height, epoch = struct.unpack(">HQQ", raw[:18])
    assert ver == 1
    fork_ctx = raw[18:130]
    base_fee, count = struct.unpack(">QI", raw[130:142])
    cur = 142
    ops: list[tuple[int, bytes]] = []
    for _ in range(count):
        tag, plen = struct.unpack(">BH", raw[cur : cur + 3])
        cur += 3
        ops.append((tag, raw[cur : cur + plen]))
        cur += plen
    assert cur == len(raw)
    return height, epoch, fork_ctx, base_fee, ops


class IndependentPythonExecutorV1:
    """Clean-room independent launch-profile executor sharing zero authority-path code with Rust."""

    def __init__(
        self,
        fork_ctx: bytes,
        chain_id: int,
        genesis_id: bytes,
        launch_manifest_id: bytes,
        profile_id: bytes,
        bounds_profile_digest: bytes,
        actors: dict[bytes, dict],
        balances: dict[bytes, int],
        stakes: dict[bytes, int],
        val_keys: dict[bytes, tuple[bytes, bytes]],
        params: dict[int, int],
    ) -> None:
        self.fork_ctx = fork_ctx
        self.chain_id = chain_id
        self.genesis_id = genesis_id
        self.launch_manifest_id = launch_manifest_id
        self.profile_id = profile_id
        self.bounds_profile_digest = bounds_profile_digest
        self.batch_height = 0
        self.epoch = 0

        # Base substrate (COMMIT-1)
        self.world: set[tuple[int, int, int]] = {(0, 0, 0)}
        self.genesis_world_digest = world_tag_py(self.world)
        _, self.hist_head = content_tag("receipt", HIST_GENESIS_SCHEMA, self.genesis_world_digest)
        self.active_mints: dict[tuple[int, int, int], bytes] = {}
        self.episodes: dict[bytes, tuple[tuple[int, int, int], bytes, bytes | None]] = {}

        # Domains 1..=2 (AUTH-COMMIT-1)
        self.actors = copy.deepcopy(actors)
        # Domains 3..=4 (ASSET-1)
        self.balances = dict(balances)
        self.asset_episodes: dict[tuple[int, int, int], tuple[bytes, bytes, int]] = {}
        self.deposit_per_voxel = 1_000_000
        self.asset_base_fee = 1_000
        # Domain 5 (GOVERNANCE-1)
        self.gov_epoch = 0
        self.gov_lifecycle = 0
        self.gov_halted_epoch = 0
        self.gov_fault = bytes(32)
        self.gov_params = dict(params)
        # Domain 6 (ECONOMIC-STATE-1)
        self.econ_epoch = 0
        self.min_self_stake = 100_000_000_000
        self.slash_equiv_bps = 5_000
        self.val_records: dict[bytes, dict] = {
            vid: {
                "bonded": amt,
                "unbonding": [],
                "rewards": 0,
                "jailed": False,
                "slashes": 0,
            }
            for vid, amt in stakes.items()
        }
        self.econ_total_slashed = 0
        self.econ_total_minted = 0
        self.econ_total_withdrawn = 0
        self.econ_genesis_pool = sum(stakes.values())
        self.econ_added_bonds = 0
        # Domain 7 (VALIDATOR-SCHEDULE-1)
        self.val_keys = dict(val_keys)
        self.schedules: dict[int, bytes] = {}
        self._init_genesis_schedules()

    def _gov_root_sha(self) -> bytes:
        t = DomainPatriciaPy(
            DOM_GOVERNANCE,
            {param_trie_key(k): struct.pack(">Q", v) for k, v in self.gov_params.items()},
        )
        r = encode_gov_root(
            self.profile_id,
            self.gov_epoch,
            self.gov_lifecycle,
            0,
            self.gov_halted_epoch,
            0,
            self.gov_fault,
            0,
            len(self.gov_params),
            t.root(),
        )
        return hashlib.sha256(r).digest()

    def _econ_root_sha(self) -> bytes:
        entries = {}
        active_cnt = 0
        bonded = 0
        unb = 0
        for vid, rec in self.val_records.items():
            u_sum = sum(a for _, a in rec["unbonding"])
            bonded += rec["bonded"]
            unb += u_sum
            if not rec["jailed"] and rec["bonded"] >= self.min_self_stake:
                active_cnt += 1
            entries[vid] = encode_val_record(
                rec["bonded"], u_sum, rec["rewards"], rec["jailed"], rec["slashes"], rec["unbonding"]
            )
        t = DomainPatriciaPy(DOM_ECONOMIC_STAKE, entries)
        r = encode_econ_root(
            self.profile_id,
            self.econ_epoch,
            len(self.val_records),
            active_cnt,
            bonded,
            unb,
            0,
            self.econ_total_withdrawn,
            self.econ_total_slashed,
            self.econ_genesis_pool,
            self.econ_added_bonds,
            self.econ_total_minted,
            t.root(),
        )
        return hashlib.sha256(r).digest()

    def _compute_schedule(self, eff_epoch: int, src_epoch: int) -> bytes:
        eligible = sorted(
            (vid, rec["bonded"])
            for vid, rec in self.val_records.items()
            if not rec["jailed"] and rec["bonded"] >= self.min_self_stake
        )
        assert 4 <= len(eligible) <= 128
        raw_total = sum(s for _, s in eligible)
        weights = {vid: s for vid, s in eligible}
        if len(eligible) >= 5:
            cap = (raw_total * 2_500) // 10_000
            for vid in list(weights):
                if weights[vid] > cap:
                    weights[vid] = cap
        eff_total = sum(weights.values())
        max_bps = max((w * 10_000) // eff_total for w in weights.values())
        assert max_bps <= 3_333
        vals = [
            (vid, self.val_keys[vid][0], self.val_keys[vid][1], weights[vid])
            for vid, _ in eligible
        ]
        return encode_epoch_schedule(
            eff_epoch, src_epoch, self._econ_root_sha(), self._gov_root_sha(), max_bps, vals
        )

    def _init_genesis_schedules(self) -> None:
        self.schedules[0] = self._compute_schedule(0, 0)
        self.schedules[1] = self._compute_schedule(1, 0)

    def snapshot_roots(self) -> list[bytes]:
        # 1..=3: COMMIT-1
        _, pp_w = content_tag("policy", COMMIT_1_PROOF_POLICY_SCHEMA, encode_proof_policy(8192, 193, 192, 193, 32768, 128, 16384))
        _, pp_a = content_tag("policy", COMMIT_1_PROOF_POLICY_SCHEMA, encode_proof_policy(8192, 193, 192, 193, 32768, 160, 16384))
        _, pp_e = content_tag("policy", COMMIT_1_PROOF_POLICY_SCHEMA, encode_proof_policy(12288, 257, 256, 257, 49152, 256, 24576))
        _, op_id = content_tag("policy", COMMIT_1_OP_LIMITS_SCHEMA, encode_op_limits())
        _, prof_w_id = content_tag("policy", COMMIT_1_PROFILE_SCHEMA, encode_profile(ROOT_WORLD, DOMAIN_WORLD, COMMIT_1_WORLD_ROOT_SCHEMA, KEY_COORD_V1, 192, VAL_ON_V1, pp_w, op_id))
        _, prof_a_id = content_tag("policy", COMMIT_1_PROFILE_SCHEMA, encode_profile(ROOT_ACTIVE_TOKEN, DOMAIN_ACTIVE_TOKEN, COMMIT_1_ACTIVE_TOKEN_ROOT_SCHEMA, KEY_COORD_V1, 192, VAL_MINT_V1, pp_a, op_id))
        _, prof_e_id = content_tag("policy", COMMIT_1_PROFILE_SCHEMA, encode_profile(ROOT_EPISODE, DOMAIN_EPISODE, COMMIT_1_EPISODE_ROOT_SCHEMA, KEY_EVENT_V1, 256, VAL_EPISODE_V1, pp_e, op_id))

        w_entries = [(coord_key(*c), ON_V1) for c in self.world]
        w_root = build_patricia(prof_w_id, COMMIT_1_WORLD_ROOT_SCHEMA, 192, w_entries).hash
        at_entries = [(coord_key(*c), m) for c, m in self.active_mints.items()]
        at_root = build_patricia(prof_a_id, COMMIT_1_ACTIVE_TOKEN_ROOT_SCHEMA, 192, at_entries).hash
        ep_entries = [
            (mid, episode_record(c, mid, b))
            for mid, (c, _, b) in self.episodes.items()
        ]
        ep_root = build_patricia(prof_e_id, COMMIT_1_EPISODE_ROOT_SCHEMA, 256, ep_entries).hash
        # 4..=5: AUTH-COMMIT-1
        auth_t = DomainPatriciaPy(
            DOM_AUTH,
            {
                aid: encode_actor_auth_state(
                    aid, a["desc_bytes"], a["nonce"], a["guardians_bytes"], a["delay"], None
                )
                for aid, a in self.actors.items()
            },
        )
        nonce_t = DomainPatriciaPy(
            DOM_NONCE,
            {aid: struct.pack(">Q", a["nonce"]) for aid, a in self.actors.items()},
        )
        # 6..=7: ASSET-1
        bal_t = DomainPatriciaPy(
            DOM_NATIVE_BALANCE,
            {aid: u128_be(b) for aid, b in self.balances.items() if b > 0},
        )
        aep_t = DomainPatriciaPy(
            DOM_EPISODE_OWNERSHIP,
            {
                episode_trie_key(*c): encode_episode_value(owner, mid, dep)
                for c, (owner, mid, dep) in self.asset_episodes.items()
            },
        )
        # 8: GOVERNANCE-1
        gov_t = DomainPatriciaPy(
            DOM_GOVERNANCE,
            {param_trie_key(k): struct.pack(">Q", v) for k, v in self.gov_params.items()},
        )
        # 9: ECONOMIC-STATE-1
        econ_t = DomainPatriciaPy(
            DOM_ECONOMIC_STAKE,
            {
                vid: encode_val_record(
                    r["bonded"],
                    sum(a for _, a in r["unbonding"]),
                    r["rewards"],
                    r["jailed"],
                    r["slashes"],
                    r["unbonding"],
                )
                for vid, r in self.val_records.items()
            },
        )
        # 10: VALIDATOR-SCHEDULE-1
        sched_t = DomainPatriciaPy(
            DOM_VALIDATOR_SCHEDULE,
            {epoch_trie_key(ep): data for ep, data in self.schedules.items()},
        )
        return [
            w_root,
            at_root,
            ep_root,
            auth_t.root(),
            nonce_t.root(),
            bal_t.root(),
            aep_t.root(),
            gov_t.root(),
            econ_t.root(),
            sched_t.root(),
        ]

    def snapshot_bytes(self) -> bytes:
        return encode_snapshot(self.batch_height, self.epoch, self.snapshot_roots(), False)

    def _classify(self, op_tag: int, plen: int) -> tuple[int, bool, int, int]:
        if op_tag <= 15:
            return 0, True, BASE_DOMAINS_MASK, BASE_OP_GAS + plen * GAS_PER_TX_BYTE
        for slot_id, lo, hi, active, wmask, bgas, pbgas in BAND_SPECS:
            if lo <= op_tag <= hi:
                return slot_id, active, wmask, bgas + plen * pbgas
        return 0xFFFF, False, 0, 0

    def _dispatch(self, op_tag: int, payload: bytes, op_index: int, height: int, epoch: int) -> int:
        if op_tag in (OP_BASE_PLACE_VOXEL, OP_BASE_DELETE_VOXEL):
            if len(payload) != 24:
                return DISP_MALFORMED
            c = struct.unpack(">qqq", payload)
            if c == (0, 0, 0):
                return DISP_BEDROCK
            is_on = c in self.world
            if op_tag == OP_BASE_PLACE_VOXEL:
                if is_on:
                    return DISP_NOOP
                op_code = 0
                self.world.add(c)
            else:
                if not is_on:
                    return DISP_NOOP
                op_code = 1
                self.world.remove(c)
            tick = height * 1024 + op_index
            edit_bytes = bytes([op_code]) + payload
            ev_payload = self.hist_head + struct.pack(">Q", tick) + edit_bytes
            _, ev_dig = content_tag("event", HIST_EVENT_SCHEMA, ev_payload)
            post_w_dig = world_tag_py(self.world)
            rcpt_payload = (
                self.hist_head
                + struct.pack(">QQ", tick, 1)
                + edit_bytes
                + post_w_dig
                + ev_dig
                + b"\x00\x00"
            )
            _, self.hist_head = content_tag("receipt", HIST_RECEIPT_SCHEMA, rcpt_payload)
            if op_tag == OP_BASE_PLACE_VOXEL:
                self.active_mints[c] = ev_dig
                self.episodes[ev_dig] = (c, ev_dig, None)
            else:
                consumed = self.active_mints.pop(c)
                c0, m0, _ = self.episodes[consumed]
                self.episodes[consumed] = (c0, m0, ev_dig)
            return DISP_ADMITTED

        if op_tag == OP_AUTH_COMMIT_TX:
            if len(payload) != 224:
                return DISP_MALFORMED
            actor_id = payload[0:32]
            signer_id = payload[32:64]
            nonce, va, vu = struct.unpack(">QQQ", payload[64:88])
            max_fee = int.from_bytes(payload[88:104], "big")
            x, y, z = struct.unpack(">qqq", payload[104:128])
            pub = payload[128:160]
            sig = payload[160:224]
            st = self.actors.get(actor_id)
            if st is None or signer_id != actor_id or nonce != st["nonce"]:
                return DISP_AUTH_FAILED
            if not (va <= height <= vu) or pub not in st["action_keys"]:
                return DISP_AUTH_FAILED
            tx_bytes = encode_tx_body(
                actor_id, nonce, va, vu, self.genesis_world_digest, max_fee, [(0, x, y, z)]
            )
            _, tx_dig = content_tag("intent", b"bitblock/identity-d1/transaction-body/v1", tx_bytes)
            wb = hashlib.sha256(b"bb.id1.world_bind.v1" + self.genesis_world_digest + tx_dig).digest()
            pre = encode_signing_preimage(
                1,
                self.chain_id,
                self.genesis_id,
                self.launch_manifest_id,
                self.profile_id,
                1,
                1,
                1,
                wb,
            )
            _, sign_dig = content_tag("receipt", b"bitblock/crypto-d1/signing-preimage/v1", pre)
            if not ed_verify_strict(pub, sign_dig, sig):
                return DISP_AUTH_FAILED
            st["nonce"] += 1
            return DISP_ADMITTED

        if op_tag == OP_ASSET_TRANSFER:
            if len(payload) != 80:
                return DISP_MALFORMED
            frm = payload[0:32]
            to = payload[32:64]
            amt = int.from_bytes(payload[64:80], "big")
            need = amt + self.asset_base_fee
            if amt == 0 or self.balances.get(frm, 0) < need:
                return DISP_ASSET_FAILED
            self.balances[frm] -= need
            self.balances[to] = self.balances.get(to, 0) + amt
            return DISP_ADMITTED

        if op_tag == OP_ASSET_MINT_DEPOSIT:
            if len(payload) != 88:
                return DISP_MALFORMED
            actor = payload[0:32]
            c = struct.unpack(">qqq", payload[32:56])
            mid = payload[56:88]
            need = self.asset_base_fee + self.deposit_per_voxel
            if c == (0, 0, 0) or c in self.asset_episodes or self.balances.get(actor, 0) < need:
                return DISP_ASSET_FAILED
            self.balances[actor] -= need
            self.asset_episodes[c] = (actor, mid, self.deposit_per_voxel)
            return DISP_ADMITTED

        if op_tag == OP_ASSET_BURN_REFUND:
            if len(payload) != 56:
                return DISP_MALFORMED
            actor = payload[0:32]
            c = struct.unpack(">qqq", payload[32:56])
            ep = self.asset_episodes.get(c)
            if ep is None or ep[0] != actor or self.balances.get(actor, 0) < self.asset_base_fee:
                return DISP_ASSET_FAILED
            self.balances[actor] = self.balances[actor] - self.asset_base_fee + ep[2]
            del self.asset_episodes[c]
            return DISP_ADMITTED

        if op_tag == OP_ECON_BOND_STAKE:
            if len(payload) != 48:
                return DISP_MALFORMED
            vid = payload[0:32]
            amt = int.from_bytes(payload[32:48], "big")
            if epoch < self.econ_epoch or amt == 0:
                return DISP_VAL_SCHED_FAILED
            rec = self.val_records.get(vid)
            if rec is None:
                if amt < self.min_self_stake:
                    return DISP_VAL_SCHED_FAILED
                self.val_records[vid] = {
                    "bonded": amt,
                    "unbonding": [],
                    "rewards": 0,
                    "jailed": False,
                    "slashes": 0,
                }
            else:
                if rec["jailed"]:
                    return DISP_VAL_SCHED_FAILED
                rec["bonded"] += amt
            self.econ_epoch = epoch
            self.econ_added_bonds += amt
            return DISP_ADMITTED

        if op_tag == OP_ECON_SLASH_EQUIVOCATION:
            if len(payload) != 64:
                return DISP_MALFORMED
            vid = payload[0:32]
            rec = self.val_records.get(vid)
            if epoch < self.econ_epoch or rec is None:
                return DISP_VAL_SCHED_FAILED
            s_act = (rec["bonded"] * self.slash_equiv_bps) // 10_000
            rec["bonded"] -= s_act
            tot = s_act
            new_u = []
            for mat, uamt in rec["unbonding"]:
                su = (uamt * self.slash_equiv_bps) // 10_000
                tot += su
                if uamt - su > 0:
                    new_u.append((mat, uamt - su))
            rec["unbonding"] = new_u
            rec["jailed"] = True
            rec["slashes"] += 1
            self.econ_total_slashed += tot
            self.econ_epoch = epoch
            return DISP_ADMITTED

        if op_tag == OP_SCHEDULE_EPOCH:
            if len(payload) != 8 or self.gov_lifecycle == 2:
                return DISP_VAL_SCHED_FAILED
            (src_epoch,) = struct.unpack(">Q", payload)
            eff = src_epoch + 2
            if eff in self.schedules or src_epoch != self.econ_epoch:
                return DISP_VAL_SCHED_FAILED
            self.schedules[eff] = self._compute_schedule(eff, src_epoch)
            return DISP_ADMITTED

        if op_tag == OP_GOV_ADVANCE_EPOCH:
            if len(payload) != 8:
                return DISP_MALFORMED
            (next_ep,) = struct.unpack(">Q", payload)
            if next_ep < self.gov_epoch:
                return DISP_GOV_FAILED
            self.gov_epoch = next_ep
            return DISP_ADMITTED

        if op_tag == OP_GOV_SAFE_HALT:
            if len(payload) != 49:
                return DISP_MALFORMED
            art = payload[0:32]
            vcode = payload[32]
            obs, lim = struct.unpack(">QQ", payload[33:49])
            if epoch < self.gov_epoch or obs <= lim:
                return DISP_GOV_FAILED
            ev_bytes = (
                struct.pack(">H", 1)
                + self.bounds_profile_digest
                + art
                + bytes([vcode])
                + struct.pack(">QQ", obs, lim)
            )
            _, fdig = content_tag("receipt", BOUNDS_SAFE_HALT_SCHEMA, ev_bytes)
            self.gov_epoch = epoch
            self.gov_lifecycle = 2
            self.gov_halted_epoch = epoch
            self.gov_fault = fdig
            return DISP_ADMITTED

        return DISP_UNKNOWN_TAG

    def execute_batch_bytes(self, raw: bytes) -> tuple[bytes, str]:
        height, epoch, fork_ctx, base_fee, ops = decode_batch(raw)
        assert fork_ctx == self.fork_ctx
        assert height == self.batch_height + 1 and epoch >= self.epoch
        _, batch_digest = content_tag("receipt", BATCH_SCHEMA, raw)
        pre_snap = self.snapshot_bytes()
        self.batch_height = height
        self.epoch = epoch

        admitted = 0
        rejected = 0
        total_gas = 0
        total_fee = 0
        cum_mask = 0
        outcomes_bytes = []
        effect_digests = []

        for idx, (op_tag, payload) in enumerate(ops):
            slot_id, is_active, wmask, sgas = self._classify(op_tag, len(payload))
            if slot_id == 0xFFFF:
                disp, g, f, mm = DISP_UNKNOWN_TAG, 0, 0, 0
            elif not is_active:
                disp, g, f, mm = DISP_INACTIVE_BAND, 0, 0, 0
            elif total_gas + sgas > MAX_BATCH_GAS:
                disp, g, f, mm = DISP_GAS_LIMIT, 0, 0, 0
            else:
                snap_copy = copy.deepcopy(self.__dict__)
                disp = self._dispatch(op_tag, payload, idx, height, epoch)
                g = sgas
                f = sgas * base_fee
                if disp == DISP_ADMITTED:
                    mm = wmask
                else:
                    self.__dict__ = snap_copy
                    mm = 0
            if disp == DISP_ADMITTED:
                admitted += 1
                cum_mask |= mm
            else:
                rejected += 1
            total_gas += g
            total_fee += f
            step_snap_sha = hashlib.sha256(self.snapshot_bytes()).digest()
            eff = hashlib.sha256(
                b"bb.exec1.step.v1"
                + struct.pack(">IBHBQ", idx, op_tag, slot_id, disp, g)
                + u128_be(f)
                + struct.pack(">I", mm)
                + step_snap_sha
            ).digest()
            effect_digests.append(eff)
            outcomes_bytes.append(
                struct.pack(">IBHBQ", idx, op_tag, slot_id, disp, g)
                + u128_be(f)
                + struct.pack(">I", mm)
                + eff
            )

        post_snap = self.snapshot_bytes()
        out_dig = hashlib.sha256(
            b"bb.exec1.outcomes.v1" + struct.pack(">I", len(effect_digests)) + b"".join(effect_digests)
        ).digest()
        rcpt_bytes = (
            struct.pack(">H", 1)
            + batch_digest
            + self.fork_ctx
            + pre_snap
            + post_snap
            + struct.pack(">IIQ", admitted, rejected, total_gas)
            + u128_be(total_fee)
            + struct.pack(">I", cum_mask)
            + out_dig
            + struct.pack(">I", len(outcomes_bytes))
            + b"".join(outcomes_bytes)
        )
        tag, _ = content_tag("receipt", RECEIPT_SCHEMA, rcpt_bytes)
        return rcpt_bytes, tag


def build_canonical_fixture() -> tuple[IndependentPythonExecutorV1, bytes, bytes, list[bytes]]:
    desc_a = encode_auth_descriptor(
        0, encode_threshold_policy(0, 1, [pk(11)]), encode_threshold_policy(1, 1, [pk(12)]), [], False
    )
    desc_b = encode_auth_descriptor(
        0, encode_threshold_policy(0, 1, [pk(21)]), encode_threshold_policy(1, 1, [pk(22)]), [], False
    )
    actor_a = derive_actor_id(77, "exec-a", desc_a)
    actor_b = derive_actor_id(77, "exec-b", desc_b)
    vals = [bytes([i]) * 32 for i in (1, 2, 3, 4)]
    genesis_id = bytes([0xAA]) * 32
    profile_id = bytes([0xBB]) * 32
    spec_digest = bytes.fromhex(
        "6b7ac56789805f58dcbf383389ee34c196f46a7048ec9faee8bd51a082acc748"
    )
    bounds_digest = bytes.fromhex(
        "d60743788fc9c1a4edefbf92ccc4dd3d25c6aaf14230c29b1863b3b29c0b9a95"
    )
    launch_manifest_id = bytes([0x22]) * 32
    fork_ctx = encode_fork_context(77, 1, genesis_id, profile_id, spec_digest)
    actors = {
        actor_a: {
            "desc_bytes": desc_a,
            "nonce": 0,
            "action_keys": [pk(11)],
            "guardians_bytes": encode_threshold_policy(1, 1, [pk(13)]),
            "delay": 100,
        },
        actor_b: {
            "desc_bytes": desc_b,
            "nonce": 0,
            "action_keys": [pk(21)],
            "guardians_bytes": encode_threshold_policy(1, 1, [pk(23)]),
            "delay": 100,
        },
    }
    ex = IndependentPythonExecutorV1(
        fork_ctx=fork_ctx,
        chain_id=77,
        genesis_id=genesis_id,
        launch_manifest_id=launch_manifest_id,
        profile_id=profile_id,
        bounds_profile_digest=bounds_digest,
        actors=actors,
        balances={actor_a: 5_000_000, actor_b: 5_000_000},
        stakes={v: 100_000_000_000 for v in vals},
        val_keys={vals[i]: (pk(31 + i), pk(41 + i)) for i in range(4)},
        params={1: 50_000_000},
    )
    return ex, actor_a, actor_b, vals


def main() -> int:
    ex, actor_a, actor_b, vals = build_canonical_fixture()
    w0 = ex.genesis_world_digest
    tx_bytes = encode_tx_body(actor_a, 0, 1, 50, w0, 1_000, [(0, 2, 0, 0)])
    _, tx_dig = content_tag("intent", b"bitblock/identity-d1/transaction-body/v1", tx_bytes)
    wb = hashlib.sha256(b"bb.id1.world_bind.v1" + w0 + tx_dig).digest()
    pre = encode_signing_preimage(
        1, 77, ex.genesis_id, ex.launch_manifest_id, ex.profile_id, 1, 1, 1, wb
    )
    _, sign_dig = content_tag("receipt", b"bitblock/crypto-d1/signing-preimage/v1", pre)
    sig = ed_sign(bytes([11]) * 32, sign_dig)
    assert ed_verify_strict(ed_public_from_seed(bytes([11]) * 32), sign_dig, sig)

    ops = [
        (OP_BASE_PLACE_VOXEL, struct.pack(">qqq", 2, 0, 0)),
        (
            OP_AUTH_COMMIT_TX,
            actor_a
            + actor_a
            + struct.pack(">QQQ", 0, 1, 50)
            + u128_be(1_000)
            + struct.pack(">qqq", 2, 0, 0)
            + pk(11)
            + sig,
        ),
        (OP_ASSET_TRANSFER, actor_a + actor_b + u128_be(250_000)),
        (OP_ASSET_MINT_DEPOSIT, actor_a + struct.pack(">qqq", 2, 0, 0) + bytes([0x55]) * 32),
        (OP_ECON_BOND_STAKE, vals[0] + u128_be(10_000_000_000)),
        (OP_SCHEDULE_EPOCH, struct.pack(">Q", 0)),
        (80, b"\x01\x02\x03"),  # INACTIVE Program/VM wall
        (200, b"\xFF"),  # Unassigned unknown tag
    ]
    batch1 = encode_batch(1, 0, ex.fork_ctx, 2, ops)
    batch1_tag, _ = content_tag("receipt", BATCH_SCHEMA, batch1)
    rcpt1_bytes, rcpt1_tag = ex.execute_batch_bytes(batch1)

    # Batch 2: negative controls (bedrock delete, bad balance, safe-halt, post-halt schedule)
    ops2 = [
        (OP_BASE_DELETE_VOXEL, struct.pack(">qqq", 0, 0, 0)),  # Bedrock immutable -> rollback
        (OP_ASSET_TRANSFER, actor_a + actor_b + u128_be(999_999_999)),  # Insufficient -> rollback
        (OP_GOV_SAFE_HALT, bytes([0xEE]) * 32 + bytes([1]) + struct.pack(">QQ", 3_000_000, 2_097_152)),
        (OP_SCHEDULE_EPOCH, struct.pack(">Q", 1)),  # Blocked by Safe-Halt -> rollback
    ]
    batch2 = encode_batch(2, 1, ex.fork_ctx, 3, ops2)
    rcpt2_bytes, rcpt2_tag = ex.execute_batch_bytes(batch2)

    if "--print-tags" in sys.argv:
        print("BATCH1_TAG =", batch1_tag)
        print("RCPT1_TAG  =", rcpt1_tag)
        print("RCPT2_TAG  =", rcpt2_tag)
        print("RCPT1_LEN  =", len(rcpt1_bytes))
        print("RCPT2_LEN  =", len(rcpt2_bytes))
        return 0

    # Verify independent golden parity tags (corroborated bit-for-bit against Rust EXECUTOR-1)
    assert batch1_tag == EXPECTED_BATCH1_TAG, f"batch1 mismatch: {batch1_tag}"
    assert rcpt1_tag == EXPECTED_RCPT1_TAG, f"rcpt1 mismatch: {rcpt1_tag}"
    assert rcpt2_tag == EXPECTED_RCPT2_TAG, f"rcpt2 mismatch: {rcpt2_tag}"

    # Powered negative control: fork-context mismatch and inactive Program/VM tag must never admit
    bad_ctx = bytes([ex.fork_ctx[0] ^ 0xFF]) + ex.fork_ctx[1:]
    try:
        ex.execute_batch_bytes(encode_batch(3, 1, bad_ctx, 1, []))
        raise AssertionError("fork-context mismatch must reject")
    except AssertionError as exc:
        if "must reject" in str(exc):
            raise
    # Verify step 6 (op_tag=80 Program/VM) and step 7 (op_tag=200 unknown) had write_mask == 0
    for step_idx, exp_disp in ((6, DISP_INACTIVE_BAND), (7, DISP_UNKNOWN_TAG)):
        off = 896 + step_idx * 68
        _, _, _, disp, gas = struct.unpack(">IBHBQ", rcpt1_bytes[off : off + 16])
        (wmask,) = struct.unpack(">I", rcpt1_bytes[off + 32 : off + 36])
        assert disp == exp_disp and gas == 0 and wmask == 0
    print(f"executor_1_vectors: PASS (batch1={batch1_tag[:26]}..., rcpt1={rcpt1_tag[:26]}..., rcpt2={rcpt2_tag[:26]}...)")
    return 0


EXPECTED_BATCH1_TAG = (
    "#bb.receipt.sha256.fs5gowdymm4iv5rafhzuqk4uh5aoupwiqwxu7wydzthq56jql7pq"
)
EXPECTED_RCPT1_TAG = (
    "#bb.receipt.sha256.d4ibxky7rkr5257wxkw6vt6nhladett7vrs5l2gkzgzlltazpwca"
)
EXPECTED_RCPT2_TAG = (
    "#bb.receipt.sha256.lewpee4g4wfchckl3ibszv5nxdyagqlt4sjwx7ee66tfcx3kf2nq"
)

if __name__ == "__main__":
    raise SystemExit(main())
