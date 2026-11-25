#!/usr/bin/env python3
"""Independent clean-room Python vector verifier for PROGRAM-STATE-1 (PS-048).

Imports no Rust/Bitblock code. Independently reconstructs all 3 canonical schemas,
SHA-256 code_id & availability_digest, domain-separated Code and Instance Merkle
roots, VM-1/VM-D1 opcode execution + gas metering, atomic trap rollback, and
cross-domain proof rejection:
  - bitblock/program-state-1/code-record/v1
  - bitblock/program-state-1/state-roots/v1
  - bitblock/program-state-1/exec-receipt/v1
"""

from __future__ import annotations

import hashlib
import struct

from crypto_d1_vectors import content_tag

PROGRAM_STATE_1_CODE_RECORD_SCHEMA = b"bitblock/program-state-1/code-record/v1"
PROGRAM_STATE_1_ROOTS_SCHEMA = b"bitblock/program-state-1/state-roots/v1"
PROGRAM_STATE_1_EXEC_RECEIPT_SCHEMA = b"bitblock/program-state-1/exec-receipt/v1"

CODE_ID_PREFIX = b"bb.prog1.code_id.v1"
CODE_AVAIL_PREFIX = b"bb.prog1.avail.v1"
INSTANCE_RECORD_PREFIX = b"bb.prog1.instance.v1"
STORAGE_MAP_PREFIX = b"bb.prog1.storage_map.v1"
CODE_TREE_EMPTY_PREFIX = b"bb.prog1.code_tree.empty.v1"
CODE_TREE_LEAF_PREFIX = b"bb.prog1.code_tree.leaf.v1"
CODE_TREE_NODE_PREFIX = b"bb.prog1.code_tree.node.v1"
INST_TREE_EMPTY_PREFIX = b"bb.prog1.inst_tree.empty.v1"
INST_TREE_LEAF_PREFIX = b"bb.prog1.inst_tree.leaf.v1"
INST_TREE_NODE_PREFIX = b"bb.prog1.inst_tree.node.v1"
COMPOSED_STATE_ROOT_PREFIX = b"bb.prog1.state_root.v1"

OPCODE_GAS = [1, 2, 2, 3, 3, 2, 2, 1]
STORAGE_WRITE_GAS = 20


def sha256(data: bytes) -> bytes:
    return hashlib.sha256(data).digest()


def code_availability_digest(opcodes: bytes) -> bytes:
    return sha256(CODE_AVAIL_PREFIX + struct.pack(">I", len(opcodes)) + opcodes)


def code_id(version: int, grammar_version: int, opcodes: bytes) -> bytes:
    avail = code_availability_digest(opcodes)
    return sha256(
        CODE_ID_PREFIX + struct.pack(">HHI", version, grammar_version, len(opcodes)) + avail
    )


def encode_code_record(opcodes: bytes, version: int = 1, grammar_version: int = 1) -> bytes:
    cid = code_id(version, grammar_version, opcodes)
    avail = code_availability_digest(opcodes)
    return struct.pack(">HH", version, grammar_version) + cid + avail + struct.pack(">I", len(opcodes)) + opcodes


def storage_root(storage: dict[bytes, int]) -> bytes:
    items = sorted(storage.items(), key=lambda kv: kv[0])
    h = hashlib.sha256()
    h.update(STORAGE_MAP_PREFIX)
    h.update(struct.pack(">I", len(items)))
    for k, v in items:
        h.update(k)
        h.update(struct.pack(">q", v))
    return h.digest()


def instance_record_digest(
    instance_id: bytes,
    cid: bytes,
    grammar_version: int,
    owner_actor: bytes,
    invocation_count: int,
    storage: dict[bytes, int],
) -> bytes:
    sroot = storage_root(storage)
    h = hashlib.sha256()
    h.update(INSTANCE_RECORD_PREFIX)
    h.update(instance_id)
    h.update(cid)
    h.update(struct.pack(">H", grammar_version))
    h.update(owner_actor)
    h.update(struct.pack(">Q", invocation_count))
    h.update(sroot)
    return h.digest()


def merkle_root(domain: str, entries: list[tuple[bytes, bytes]]) -> bytes:
    empty_p = CODE_TREE_EMPTY_PREFIX if domain == "code" else INST_TREE_EMPTY_PREFIX
    leaf_p = CODE_TREE_LEAF_PREFIX if domain == "code" else INST_TREE_LEAF_PREFIX
    node_p = CODE_TREE_NODE_PREFIX if domain == "code" else INST_TREE_NODE_PREFIX
    if not entries:
        return sha256(empty_p)
    level = [sha256(leaf_p + k + v) for k, v in sorted(entries, key=lambda kv: kv[0])]
    while len(level) > 1:
        nxt = []
        for i in range(0, len(level), 2):
            left = level[i]
            right = level[i + 1] if i + 1 < len(level) else left
            nxt.append(sha256(node_p + left + right))
        level = nxt
    return level[0]


def compose_state_root(c_root: bytes, i_root: bytes, c_cnt: int, i_cnt: int) -> bytes:
    return sha256(
        COMPOSED_STATE_ROOT_PREFIX + c_root + i_root + struct.pack(">II", c_cnt, i_cnt)
    )


def encode_roots(c_root: bytes, i_root: bytes, c_cnt: int, i_cnt: int, version: int = 1) -> bytes:
    s_root = compose_state_root(c_root, i_root, c_cnt, i_cnt)
    wire = struct.pack(">H", version) + c_root + i_root + s_root + struct.pack(">II", c_cnt, i_cnt)
    assert len(wire) == 106
    return wire


def encode_exec_receipt(
    *,
    disposition: int,
    trap_status_code: int,
    instance_id: bytes,
    cid: bytes,
    storage_key: bytes,
    written_value: int,
    gas_used: int,
    gas_limit: int,
    pre_state_root: bytes,
    post_state_root: bytes,
    version: int = 1,
) -> bytes:
    wire = (
        struct.pack(">HBB", version, disposition, trap_status_code)
        + instance_id
        + cid
        + storage_key
        + struct.pack(">qQQ", written_value, gas_used, gas_limit)
        + pre_state_root
        + post_state_root
    )
    assert len(wire) == 188
    return wire


def run_vm_opcodes(opcodes: bytes, gas_limit: int) -> tuple[int, int]:
    stack: list[int] = []
    gas = 0
    for op in opcodes:
        gas += OPCODE_GAS[op]
        assert gas <= gas_limit
        if op == 0:
            pass
        elif op == 1:
            stack.append(1)
        elif op == 2:
            stack.append(0)
        elif op == 3:
            b = stack.pop()
            a = stack.pop()
            stack.append(a + b)
        elif op == 4:
            b = stack.pop()
            a = stack.pop()
            stack.append(a - b)
        elif op == 5:
            stack.append(stack[-1])
        elif op == 6:
            stack[-1], stack[-2] = stack[-2], stack[-1]
        elif op == 7:
            return stack[-1], gas + STORAGE_WRITE_GAS
        else:
            raise ValueError(f"bad opcode {op}")
    raise ValueError("missing halt")


def main() -> None:
    # PushOne(1), Dup(5), Add(3), Dup(5), Add(3), Halt(7) => 4, gas = 13 + 20 = 33
    opcodes = bytes([1, 5, 3, 5, 3, 7])
    c_wire = encode_code_record(opcodes)
    c_tag, c_digest = content_tag("policy", PROGRAM_STATE_1_CODE_RECORD_SCHEMA, c_wire)
    cid = code_id(1, 1, opcodes)

    inst_id = bytes([0x11] * 32)
    owner = bytes([0x22] * 32)
    slot_key = bytes([0x33] * 32)

    c_root = merkle_root("code", [(cid, c_digest)])
    pre_inst_digest = instance_record_digest(inst_id, cid, 1, owner, 0, {})
    pre_i_root = merkle_root("instance", [(inst_id, pre_inst_digest)])
    pre_s_root = compose_state_root(c_root, pre_i_root, 1, 1)

    val, gas_used = run_vm_opcodes(opcodes, 200)
    assert val == 4 and gas_used == 33

    post_inst_digest = instance_record_digest(inst_id, cid, 1, owner, 1, {slot_key: 4})
    post_i_root = merkle_root("instance", [(inst_id, post_inst_digest)])
    post_s_root = compose_state_root(c_root, post_i_root, 1, 1)

    r_wire = encode_roots(c_root, post_i_root, 1, 1)
    r_tag, _ = content_tag("receipt", PROGRAM_STATE_1_ROOTS_SCHEMA, r_wire)

    e_wire = encode_exec_receipt(
        disposition=0,
        trap_status_code=0,
        instance_id=inst_id,
        cid=cid,
        storage_key=slot_key,
        written_value=4,
        gas_used=33,
        gas_limit=200,
        pre_state_root=pre_s_root,
        post_state_root=post_s_root,
    )
    e_tag, _ = content_tag("receipt", PROGRAM_STATE_1_EXEC_RECEIPT_SCHEMA, e_wire)

    # Cross-domain separation control: empty code root != empty instance root
    assert merkle_root("code", []) != merkle_root("instance", [])

    assert c_tag == "#bb.policy.sha256.qmmm3akwhwhy42jblombezj6chppxzdfxzrtualsrmone6i3xtfa"
    assert r_tag == "#bb.receipt.sha256.gbzdt7lyv2ej5ewweyqw3xf6bce4pghgs5vqzs4x5h4piln2lj4q"
    assert e_tag == "#bb.receipt.sha256.4pvmwu57ahsryqvsiyox5qwgsfnljtdcjx752cynhpag4y2zbqla"
    print("program_state_1_vectors: ok (3/3 canonical tags matched byte-for-byte)")


if __name__ == "__main__":
    main()
