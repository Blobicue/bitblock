# Authenticated state research plan

This document records the proposed Merkle-Patricia-versus-sparse-Merkle checkpoint for Bitblock.
It is a **research and decision plan**, not an implemented commitment, selected consensus schema,
or security proof. The authenticated-map choice remains open until both candidates pass the same
vectors and their costs are measured. That choice is scoped to the first two-candidate hash-tree
tournament; the separate Verkle feasibility lane below is explicitly deferred rather than ranked
or silently treated as a losing candidate.

The corresponding capability nodes are `COMMIT-R1` and `COMMIT-1` in
[`CAPABILITY_TREE.md`](CAPABILITY_TREE.md). The wider root hierarchy and proof meanings are in
§5 of [`LAYER1_DESIGN_PAPER.md`](LAYER1_DESIGN_PAPER.md).

The current-source statements below were checked against source/proof snapshot
`6545d8e7d60e75607445fda564bbf11610a62658`, specifically `src/world.rs`, `src/history.rs`,
`src/protocol.rs`, and the named proof/tool artifacts. The planning audit input is
`162406819133c1fd57d0fd26cbe5fa0b495766b8`; post-input corrections are mapped in
[`FUTURE_PLAN_AUDIT.md`](FUTURE_PLAN_AUDIT.md). A change to a named source file, root/key contract,
or this plan invalidates the corresponding statement rather than inheriting its “checked” status.

## Outcome sought

Bitblock needs a way to answer questions such as:

```text
Is coordinate c on under world root r?
Is coordinate c absent under world root r?
Which live occupancy episode, if any, is bound to c under active-token root a?
Does episode e exist under episode root p?
```

The answer should require a bounded witness rather than the entire world. The structure must have
one canonical root for one logical map, deterministic updates and deletion, typed domains, strict
proof verification, persistent fork support, and predictable resource limits.

A valid witness proves only that a key/value claim is consistent with a supplied root. Separate
evidence must establish that the root came from valid execution, was accepted at a claimed
consensus status, and has available backing data.

## Current repository anchor

The present source has no authenticated trie:

- `src/world.rs` stores occupied coordinates in a `HashMap<ChunkCoord, Chunk>` with 16³ chunks,
  64 `u64` words per chunk, and a count. Missing chunks/bits are off; empty non-origin chunks are
  removed; bedrock keeps the origin chunk non-empty.
- `Coord` derives signed lexicographic ordering over `(x, y, z)`. Coordinate splitting is
  Euclidean, so coordinate `-1` belongs to chunk `-1` at local coordinate `15`.
- `src/history.rs::world_tag` scans all on coordinates, sorts them, and hashes their raw signed
  big-endian `x`, `y`, and `z` bytes under `bitblock/world/sorted-i64-be/v1`. It is deterministic
  and insertion-order independent, but it is a full-set content identity, not a membership or
  non-membership proof structure.
- `TagKind::World` and `ContentTag` do not encode an authenticated-map profile. Reusing that kind
  for a different root would make schema/type confusion too easy.

These facts create an integration rule:

> Keep the current chunked `World` as the renderer, collision, editing, and iteration hot path.
> Build authenticated commitments as a separately typed sidecar derived from the same logical
> state. Do not traverse a cryptographic trie to draw a frame or move the observer.

## Canonical logical maps

One generic implementation may serve several maps, but every root and node hash must bind its
logical domain. The first proposed map family is:

| Typed root | Key | Value | Absence means |
| --- | --- | --- | --- |
| `WorldRootV1` | `CoordinateKeyV1` | `OnV1` | Coordinate is off |
| `ActiveTokenRootV1` | `CoordinateKeyV1` | `MintIdV1` | No live mutable occupancy episode at the coordinate |
| `EpisodeRootV1` | `EventIdV1` | `EpisodeRecordV1` | No retained episode record under the selected policy |
| `AuthorizationRootV1` | `ActorIdV1` | `AuthorizationDescriptorV1` | No active actor authorization |
| `NonceRootV1` | `ActorIdV1` | canonical nonce record | No initialized nonce record under the selected account rule |
| `AssetStateRootV1` | model-dependent | ownership/balance/permission record | Defined only after `ASSET-D1` selects the asset model |

World, active-token, episode, authorization, nonce, and asset proofs are not substitutable even if
they use the same generic node codec. Their root wrapper type, tree-domain bytes, key codec, value
codec, version, and commitment profile must be explicit.

`COMMIT-R1` implements and compares only `WorldRootV1`. The other names reserve the required type
and domain separations; their key widths, value schemas, and authoritative integration remain
locked behind their owning capability nodes. `ActiveTokenRootV1` shares the 192-bit coordinate key
but still needs independent `MintIdV1` value-codec/root vectors. `EventIdV1` width and
`EpisodeRecordV1` encoding are not frozen; an episode root cannot inherit the world comparison.
Every later domain must freeze a profile-fixed key width and value codec, then rerun the applicable
schema, rebuild, update, proof, malformed-input, persistence, and independent-parity gates.

The local map root is context-free with respect to a not-yet-derived chain or genesis identifier.
Its node/root domain binds the commitment profile, typed map, codecs, and version only. After
`PROTOCOL-ID-1`, a separate typed `AuthenticatedRootRefV1` or application-root field binds those
exact root bytes to `chain_id`, `genesis_id`, `world_id`, protocol version, and an accepted header.
This avoids making local commitment research depend on an identifier graph that itself commits the
initial roots.

For `WorldRootV1`:

- an occupied coordinate is represented by one present leaf;
- absence is off;
- a stored `false`, tombstone, empty-value alias, or alternate “off leaf” is forbidden;
- bedrock is present explicitly because it is part of every valid world;
- the root commits occupancy only, not ownership, camera state, frame pixels, or history.

For the proposed occupancy-episode model, bedrock has no entry in `ActiveTokenRootV1`. This is the
current first-profile proposal, not a final economic ruling: `ASSET-D1` must explicitly retain or
change bedrock's economic treatment before an asset protocol is frozen.

## `CoordinateKeyV1`

The proposed fixed-width key is a 192-bit bijection. In language-neutral terms:

```text
Twos64(v) = v                  when v >= 0
          = 2^64 + v           when v < 0

BiasSigned64BE(v) = BE64(Twos64(v) XOR 2^63)

DecodeBiasSigned64BE(bytes):
    t = DecodeBE64(bytes) XOR 2^63
    return t                    when t < 2^63
    return t - 2^64             otherwise

CoordinateKeyV1(x, y, z) =
    BiasSigned64BE(x)
 || BiasSigned64BE(y)
 || BiasSigned64BE(z)
```

`Twos64`, sign-bit XOR, fixed-width big-endian encoding, and concatenation are each bijective over
their declared domains; their composition therefore gives one key for every current `i64³`
coordinate. Flipping the sign bit also maps signed numeric order to unsigned big-endian byte order,
preserving the current `(x, then y, then z)` tuple ordering, including signed extrema.

Required scalar vectors are:

```text
i64::MIN -> 0000000000000000
-1       -> 7fffffffffffffff
0        -> 8000000000000000
1        -> 8000000000000001
i64::MAX -> ffffffffffffffff
```

This is a **new schema**. It does not equal, reinterpret, or migrate silently from the legacy
`world_tag`, whose payload uses raw `i64::to_be_bytes()` after signed sorting. A future
implementation should introduce a new root type and domain such as `WorldRootV1`; it must not
silently relabel the old `TagKind::World` digest.

The scalar examples exercise the formula; they are not themselves a proof of bijection. Full key
vectors must include:

- `i64::MIN`, `-1`, `0`, `1`, and `i64::MAX` on every axis;
- tuple-order boundaries where one axis changes and later axes reset;
- chunk boundaries `-17`, `-16`, `-15`, `-1`, `0`, `1`, `15`, `16`, and `17`;
- round-trip uniqueness and rejection of every non-24-byte coordinate key.

Spatial locality is not the first key objective. Plain `x || y || z` supports canonical point
queries but may give poor viewport/range locality. A later chunk/Morton profile or multiproof
strategy can be evaluated without making renderer caches consensus state.

Bit traversal is also part of the key codec, not an implementation choice. For every depth
`d in 0..191`:

```text
KeyBit(key, d) = (key[floor(d / 8)] >> (7 - (d mod 8))) AND 1
```

Depth zero therefore consumes the most-significant bit of the first biased `x` byte; traversal then
continues MSB-first through `x`, `y`, and `z`. Bit `0` selects the left child and bit `1` the right.
Patricia packed paths use this order. Sparse-tree bitmap bit `d` names decision depth `d` and
carries the sibling subtree root after that decision, at consumed depth `d + 1`. Bits are packed
MSB-first inside each bitmap byte; included sibling hashes are serialized in increasing decision-
depth order, root to leaf. Reversing byte, bit, axis, child, bitmap, or sibling order must change or
reject the vector rather than silently describe a second tree.

## Commitment profile and proof policy

The two hash-tree candidates need one non-self-referential way to name every byte and limit that
affects their roots or accepted proofs. R0 freezes a protocol-level `ProfileIdHashV1` before any
candidate descriptor is admitted. A candidate cannot choose the hash function used to identify its
own descriptor. The same pre-frozen function is the sole identity-hash owner for the Layer 1
pre-admission chain—domain registry, profile scope, chain genesis, candidate profile, actor identity,
and profile-bound transition specification. Every invocation is domain separated and length frames
its canonical payload. Candidate-selected cryptographic-suite fields govern only later
profile-resolved operations and cannot alter `ProfileIdHashV1`.

```text
ProofPolicyDescriptorV1 {
    descriptor_schema_version,
    accepted_proof_codec_ids,
    max_proof_bytes,
    max_proof_records,
    max_reconstructed_depth,
    max_hash_invocations,
    max_hashed_preimage_bytes,
    max_single_encoded_node_bytes,
    max_total_encoded_node_bytes,
    max_total_decoded_node_bytes,
    max_decompressed_proof_bytes,
    max_allocation_count,
    max_total_allocation_bytes,
    max_multiproof_query_keys,
    max_multiproof_dag_nodes,
    max_multiproof_dag_edges
}

ProofPolicyIdV1 = ProfileIdHashV1(
    Domain("bitblock/proof-policy-id/v1")
 || LengthFrame(Canonical(ProofPolicyDescriptorV1))
)

CommitmentOperationalLimitsDescriptorV1 {
    descriptor_schema_version,
    max_full_build_keys,
    max_batch_updates,
    max_node_store_record_bytes,
    max_total_node_fetch_bytes,
    max_total_node_write_bytes,
    max_database_reads,
    max_database_writes,
    max_staged_nodes,
    max_staged_bytes,
    max_pinned_roots,
    max_provisional_roots,
    max_retained_forks,
    max_recovery_scan_records,
    max_recovery_scan_bytes,
    max_recovery_work_units,
    max_gc_mark_nodes,
    max_gc_sweep_nodes,
    max_gc_bytes,
    max_gc_work_units,
    max_persistent_growth_per_transition
}

CommitmentOperationalLimitsIdV1 = ProfileIdHashV1(
    Domain("bitblock/commitment-operational-limits-id/v1")
 || LengthFrame(Canonical(CommitmentOperationalLimitsDescriptorV1))
)

CommitmentProfileDescriptorV1 {
    descriptor_schema_version,
    tree_algorithm_id,
    root_type,
    tree_domain,
    key_codec_id,
    key_bit_width,
    value_codec_id,
    hash_suite_id,
    digest_byte_length,
    node_codec_version,
    root_codec_version,
    canonical_node_type_tags,
    empty_root_rule_id,
    proof_policy_id,
    operational_limits_id
}

CommitmentProfileIdV1 = ProfileIdHashV1(
    Domain("bitblock/commitment-profile-id/v1")
 || LengthFrame(Canonical(CommitmentProfileDescriptorV1))
)
```

No descriptor contains its derived identifier, a root produced under that identifier, a
`CommitmentSetV1` identifier, a chain/genesis identifier, or a header. The proof-policy and
operational-limit descriptors are identified first; their identifiers are then fields of the
commitment-profile descriptor; the commitment-profile identifier is then available to the node
hash. This is the complete acyclic direction. Any change to a field creates a different identifier
rather than silently changing the meaning of an existing one.

An immutable registry resolves each admitted `CommitmentProfileIdV1` to exactly one descriptor and
each `ProofPolicyIdV1` and `CommitmentOperationalLimitsIdV1` to exactly one descriptor. Unknown
identifiers, a different descriptor under the same identifier, or an expected context whose root
type, domain, key/value codec, digest length, node codec, proof policy, or operational-limit profile
disagrees with the resolved descriptors rejects before proof work.

Every `accepted_proof_codec_ids` list is a canonically sorted set of unique, fully versioned codec
identifiers; a bare version number has no cross-profile meaning. Every numeric limit is one
fixed-width non-negative integer with a frozen unit. `work_units` must name a deterministic charging
procedure in R0 rather than an implementation-dependent elapsed-time estimate. Duplicate codec IDs,
combined fields whose component bounds cannot be checked separately, unknown units, overflow, and
unbounded sentinel values reject.

An implementation may apply a smaller machine-local numeric proof cap to an untrusted local RPC or
proof generator, but it may never add a proof codec, widen a profile limit, or change a unit.
Consensus/authority verification uses the exact activated proof policy: a machine unable to honor
it must refuse activation or enter the declared safe-halt path rather than silently tighten the
rule and disagree with other verifiers.

## Common node-hash contract

Both candidates use one profile-fixed `NodeHash` framing. R0 still selects the concrete hash suite
and canonical byte codec, but the preimage structure is not optional:

```text
NodeHash(node_type, consumed_bit_depth, payload) =
    H(Canonical(
        commitment_profile_id,
        tree_domain,
        hash_suite_id,
        node_codec_version,
        node_type,
        consumed_bit_depth,
        LengthFrame(payload)
    ))
```

The resolved commitment-profile descriptor fixes the hash suite, all tags, codecs, key width,
value type, digest length, empty-root rule, proof policy, and operational-limit profile. Ordered
children remain ordered inside `payload`. Database metadata, compression, reference counts, cache
hints, and storage offsets remain outside the authoritative preimage. Changing profile, domain,
type, codec, or depth changes the preimage; treating two such preimages as the same node still
relies on the declared collision-resistance assumption of the selected suite.

## Candidate A — fixed-key binary Merkle Patricia trie

This is a constrained custom binary Patricia design over exactly 192 key bits. It deliberately
does not import a general blockchain trie codec with variable key kinds, branch values, or
inline-versus-hashed child alternatives.

The node grammar to freeze before implementation is:

```text
MptEmpty
MptLeaf(remaining_key_bits, typed_value_bytes)
MptExtension(nonempty_shared_bits, child_hash)
MptBranch(left_hash, right_hash)
```

`consumed_bit_depth` always means the number of key bits consumed **before** the node. The exact
depth formulas are:

```text
MptEmptyRoot = NodeHash(MptEmpty, 0, EmptyPayload)

MptLeaf(d, suffix, value):
    0 <= d <= 192
    bit_length(suffix) = 192 - d
    hash = NodeHash(MptLeaf, d,
                    BitLengthFrame(suffix) || CanonicalValue(value))

MptExtension(d, path, child_branch_hash):
    0 <= d < 191
    1 <= bit_length(path) <= 191 - d
    child is MptBranch at depth d + bit_length(path)
    hash = NodeHash(MptExtension, d,
                    BitLengthFrame(path) || child_branch_hash)

MptBranch(d, left_hash, right_hash):
    0 <= d < 192
    both children are non-empty nodes at depth d + 1
    hash = NodeHash(MptBranch, d, left_hash || right_hash)
```

The root of every non-empty Patricia map is a non-empty node at depth zero. A leaf at depth 192 has
one canonical zero-bit suffix. `MptEmptyRoot` is valid only for the whole empty map; no empty node
or empty hash is valid below a branch or extension. The canonical empty-map lookup proof contains
zero node records and verifies only when the caller-selected root equals `MptEmptyRoot`; carrying a
serialized `MptEmpty` record is an alternate encoding and rejects.

Canonical normalization must make the logical map-to-root function unique:

- every root starts at depth zero and every complete key consumes exactly 192 bits;
- a leaf at depth `d` stores exactly `192 - d` unconsumed bits and one canonical typed value;
- an extension path is non-empty, obeys the depth formula above, and points to a branch at the exact
  post-path depth;
- adjacent extensions are merged;
- a branch has two non-empty logical children;
- insertion of two divergent keys uses their unique longest common remaining prefix, one branch at
  the first differing bit, and canonically normalized children; no shorter or longer split is valid;
- deletion of one branch child prepends the surviving direction bit to the survivor: a leaf becomes
  `MptLeaf(bit || suffix, value)`, an extension becomes
  `MptExtension(bit || prefix, child_hash)`, and a surviving branch becomes
  `MptExtension(bit, branch_hash)`; any parent extension is then concatenated so adjacent
  extensions cannot remain;
- no terminal branch value exists because all keys have fixed length;
- unused packed-path bits are zero;
- children are always represented by hashes in the canonical node preimage;
- the empty map has one domain- and profile-specific root;
- stored-false/tombstone leaves and duplicate logical keys are rejected.

Every Patricia `MptEmpty`, `MptLeaf`, `MptExtension`, and `MptBranch` hash uses the common
`NodeHash` contract and the exact pre-node depth above. The payload includes the exact packed-path
bit length where applicable; unused packed bits are zero and rejected otherwise. Vectors must
exercise root leaves, the zero-suffix leaf at depth 192, branches at depths 0 and 191, maximum
shared prefixes, and every off-by-one child-depth mutation.

Expected advantage: path compression can make point witnesses shorter when long key prefixes have
no authenticated branch. Expected cost: insertion and especially deletion normalization become
consensus-critical, witness size varies with shape, and there are more malformed encodings to
reject. Work is proportional to authenticated branch structure and bounded by 192 key bits; it
must not be advertised as universally `O(log n)`.

## Candidate B — 192-level sparse Merkle tree

This candidate uses the same `CoordinateKeyV1` but fixes the logical shape at one binary level per
key bit:

```text
depth 0..191: branch selected by the next key bit
depth 192:    typed present leaf or the canonical empty leaf
```

Domain-specific default hashes are precomputed for every depth. A raw point witness contains one
sibling per level. Its wire form can omit default siblings using a fixed 192-bit bitmap followed by
the non-default hashes in depth order. Batch queries should use a canonical multiproof rather than
repeating shared paths.

The sparse-tree recurrence to instantiate in R0 is structurally fixed as follows, where
`CanonicalValue(v)` is length-framed by the common contract and `d` is the branch depth:

```text
PresentLeaf(v) = NodeHash(SmtPresentLeaf, 192, CanonicalValue(v))
D[192]         = NodeHash(SmtEmptyLeaf,   192, EmptyPayload)
Branch(d,l,r)  = NodeHash(SmtBranch,        d, l || r)
D[d]           = Branch(d, D[d+1], D[d+1])  for d = 191 down to 0
EmptyRoot      = D[0]
```

Witness indexing uses the **decision depth**, not the sibling subtree's own depth. For lookup key
`k`, record `sibling[d]` is the root of the unchosen subtree at child depth `d + 1` while
`KeyBit(k,d)` chooses the path child. A zero bitmap bit at decision depth `d` means exactly
`sibling[d] = D[d+1]`; a one bit means exactly one serialized non-default sibling hash at that
position. Reconstruction proceeds from decision depth 191 back to 0, forming
`Branch(d,path_child,sibling[d])` or `Branch(d,sibling[d],path_child)` according to `KeyBit(k,d)`.
The wire list remains serialized by increasing decision depth 0 through 191 even though hash
reconstruction runs in the opposite direction.

A present leaf is never an empty leaf, even when its value bytes are empty under some later value
type. Explicitly carrying a default sibling when its bitmap bit says omitted, omitting a
non-default sibling, swapping children, using the wrong depth, or using a default from another
profile/domain is noncanonical and rejects. Golden vectors must cover both default-only and
non-default present-leaf/branch paths, not only the empty root. They must independently flip bitmap
bits at decision depths 0 and 191, substitute `D[d]` for `D[d+1]`, reverse wire order, and reverse
reconstruction order; every mutation changes the root or rejects.

Expected advantage: update work, absence semantics, proof parsing, deletion, and formal refinement
have a fixed shape. Expected cost: 192 logical hash levels per update and potentially larger point
witnesses if defaults do not compress well.

The sparse tree is the lower-complexity baseline, not an automatic winner. The Patricia candidate
must beat it on measured costs strongly enough to justify its larger canonicalization and storage
surface.

## Separate Verkle feasibility lane — not Candidate C

Verkle trees are a legitimate later research question, but they are not silently a third arm of
the R0–R5 hash-tree tournament. Patricia and sparse Merkle share a hash-based security/implementation
surface and can use the common `NodeHash` experiment. A Verkle construction replaces internal
hashing with a concrete vector-commitment proof system and therefore changes the cryptographic
assumptions, trusted-computing base, setup/generator material, proof generation, verification, and
migration problem at once. Its root, node, and proof formats must not inherit the common `NodeHash`
contract or evidence merely because it also authenticates a map.

The first hash-tree profile explicitly defers Verkle. The R5 result selects at most one of the two
admitted hash-tree candidates; it does not claim that the selected candidate is globally better
than Verkle, Verkle-style commitments, or every other authenticated map. A later profile may admit a
Verkle candidate only after a separate feasibility record and capability-tree edge pass every gate
below. An incomplete gate means **deferred**, not “disproved” and not evidence that either hash tree
is superior.

The coordinate embedding for any 32-byte-key Verkle feasibility prototype is fixed and injective:

```text
VerkleCoordinateKeyV1(c) =
    0x0000000000000000 || CoordinateKeyV1(c)
```

Only a 32-byte key with that exact eight-byte zero prefix is a canonical coordinate key; any other
prefix, a hashed coordinate, truncation, alternate padding, or non-32-byte form rejects. A scheme
that consumes field elements or stem/suffix positions must additionally freeze an injective mapping
from those exact 32 bytes into its field/vector positions and prove that range reduction, endian
conversion, or stem packing creates no aliases. Non-coordinate maps require their own typed
embeddings and cannot reuse this one by resemblance. Present zero/empty values and absent positions
must remain distinct.

Before implementation, a `VectorCommitmentProfileDescriptorV1` must freeze at least:

- the exact vector-commitment and opening scheme, arity, curve/group, scalar field, security
  assumptions, commitment/proof byte sizes, and algorithm/version identifiers;
- canonical field, scalar, and group-point encodings; identity/infinity rules; on-curve, subgroup,
  cofactor, and noncanonical-coordinate rejection before use;
- the generator basis or structured reference string derivation, immutable digest, provenance,
  distribution, update/ceremony and compromise rules, and whether any trusted setup assumption
  exists;
- transcript and Fiat–Shamir domains, challenge derivation, statement ordering, batch-opening and
  batch-verification rules, duplicate-point handling, and rejection of unused proof material;
- exact empty, present, absence, update, overwrite, deletion, multiproof, and root-serialization
  semantics under separately typed tree domains and proof policies;
- a non-self-referential profile identifier, independent positive/negative vectors, and at least one
  verifier that does not reuse the producer's tree/proof code. Shared curve and arithmetic
  dependencies must be disclosed rather than counted as independent agreement.

A feasibility assay must run the same logical world maps, coordinate corpus, insertion/deletion
sequence, clustered/dispersed query sets, fork schedule, persistence policy, host/build profile, and
hard resource envelopes used by the two hash candidates. It must report end-to-end full rebuild,
incremental insert/delete, proof generation, single and batch verification, multiproof wire bytes,
group/MSM/pairing or other primitive work, precomputation/setup load, peak memory, allocations,
database reads/writes, persistent bytes, fork deduplication, GC, recovery, dependency/binary size,
audited-code surface, and malformed-input rejection. A smaller witness alone does not pass.

Admission also requires a migration plan with old/new typed roots, deterministic full-state
conversion, bounded per-transition or offline conversion work, dual-root/overlay semantics if used,
crash and reorg recovery, historical-proof verification, setup/generator rotation, data-availability
and archival obligations, downgrade rejection, and a reversal or new-profile path if the vector
commitment, curve, setup, or post-quantum assumptions become unacceptable. The original
[Verkle-tree paper](https://math.mit.edu/research/highschool/primes/materials/2018/Kuszmaul.pdf) and
Ethereum's [Verkle state proposal](https://eips.ethereum.org/EIPS/eip-6800),
[overlay](https://eips.ethereum.org/EIPS/eip-7612), and
[conversion](https://eips.ethereum.org/EIPS/eip-7748) documents are research inputs only; their
parameters, measurements, implementations, and protocol choices are not Bitblock evidence.

## Proof contract

The caller—not attacker-controlled proof bytes—selects one expected verification context:

```text
ExpectedProofContextV1 {
    commitment_profile_id,
    tree_domain,
    root_type,
    expected_root_bytes,
    key_codec_id,
    value_codec_id,
    proof_codec_id,
    proof_policy_id
}

VerifyLookup(expected_context, logical_query_key, bounded_proof_payload)
    -> Absent | Present(canonical_typed_value)

VerifyClaim(expected_context, logical_query_key,
            Absent | Present(expected_typed_value), bounded_proof_payload)
    -> verified | rejected
```

There is exactly one query-key input. The typed entry point canonically encodes
`logical_query_key` with the caller-selected key codec ID; a byte-level entry point instead accepts
exactly those canonical key bytes. A carrier cannot supply an independent “logical” key and
“canonical” key. `VerifyClaim` compares the verified lookup result to the caller-selected expected
absence/value; proof bytes do not choose what proposition the caller believes was proved.

If the carrier repeats a profile, domain, root type, root, or codec identifier, it must exactly
match the caller's expected value; the carrier cannot negotiate or downgrade it. The verifier must
check the root supplied by its caller and must not fetch a “latest” root from a mutable local
database during proof verification. A later network envelope separately verifies the
`AuthenticatedRootRefV1` chain/genesis/world/header context before passing its exact typed root into
this local verifier.

Before reading the proof payload, the verifier resolves `commitment_profile_id`, confirms that its
descriptor names the supplied root/domain/key/value codec IDs and `proof_policy_id`, resolves that
policy, and confirms that `proof_codec_id` is admitted by it. A carrier cannot install a profile
descriptor or policy definition. An optional machine-local cap is a separate host input; every
numeric field must be less than or equal to the corresponding resolved profile limit and it cannot
change the admitted codec set.

The profile's resolved `CommitmentOperationalLimitsDescriptorV1` governs work outside one proof:
full-build keys, batch updates, encoded and decoded node-store bytes, database reads/writes, staged
nodes/bytes, pinned and provisional roots/forks, recovery scan work, persistent growth, and GC
mark/sweep work. Authority artifacts cite its exact identifier and the exact proof-policy identifier
rather than accept a free-form limit tuple. All declared-length, multiplication, addition, count,
decompression, and work arithmetic must pass before attacker-sized allocation, node fetch,
decompression, or hashing begins.

For the Patricia candidate, membership terminates at the exact key leaf. Because canonical branches
have two non-empty children, non-membership terminates only at the authenticated empty-tree root, a
divergent extension, or a divergent leaf. A nearby leaf without the complete root path is not an
absence proof.

For both candidates, verification must reject:

- wrong caller-selected root, domain, profile/version, root type, key codec, value codec, or proof
  codec/policy; an internally inconsistent expected context also rejects, and a wrong
  chain/world/header binding rejects in the later typed root-reference verifier;
- reordered, duplicate, unused, or trailing witness elements;
- alternate encodings of the same logical path or node;
- a reconstructed path longer or shorter than 192 bits;
- a leaf whose reconstructed key is not the requested key;
- noncanonical empty/default encodings;
- proofs above byte, encoded/decoded-node, record, depth, hash-work, decompression, or allocation
  limits, and any attempt to widen a profile limit with a local cap;
- truncated, overlong, unknown-field, and integer-overflow encodings.

Multiproofs need one canonical form. Two acceptable research designs are an ordered traversal
witness or a hash-indexed DAG with canonical topological ordering and exact reference counts. An
unordered bag of nodes is not sufficient because alternate orderings and unused nodes would make
the proof malleable.

## World roots are not history roots

The following sequence deliberately returns to a prior world state:

```text
start:      coordinate c is off       -> world root W0
place c:    coordinate c is on        -> world root W1, episode E1
delete c:   coordinate c is off       -> world root W0, episode/burn history E2
re-place c: coordinate c is on        -> world root W1, fresh episode E3
```

Repeating `W0` and `W1` is correct because those roots commit only occupancy. The active-token map
is empty both initially and after deletion, so its empty root also correctly repeats; its first and
second occupied roots differ because the live mint IDs are `E1` and `E3`. Episode roots under a
retaining policy, plus receipt and history commitments, distinguish the placement, consuming burn,
and re-placement. Trying to force history uniqueness into `WorldRootV1` would destroy the meaning
of a state root.

Separate roots still do not by themselves prove the cross-map invariants. For ledger head `h`, let
`O_h` be the exact occupied-coordinate set including permanent origin, let `M_h` be the ledger's
live mutable coordinate-to-mint map, and let `R_h` be the complete episode map selected by the
activated retention policy:

```text
dom(WorldRootV1_h)       = O_h
value(WorldRootV1_h, c)  = OnV1                         for every c in O_h

dom(ActiveTokenRootV1_h) = O_h minus {(0,0,0)}          = dom(M_h)
value(ActiveTokenRootV1_h, c) = M_h[c]                  for every mutable occupied c

dom(EpisodeRootV1_h)     = dom(R_h)
value(EpisodeRootV1_h, e)= CanonicalEpisodeRecord(R_h[e]) for every retained e
```

These are set equalities in both directions: no root may omit a required world or episode entry or
contain an extra one. Every retained episode value must exactly reproduce live/consumed status,
mint provenance, burn provenance, and all other fields selected by `EpisodeRecordV1`; merely
checking that the records which happened to be present are individually valid is insufficient.
Changing the logical retention policy is an authorized transition that changes `R_h` and normally
changes `EpisodeRootV1`.

Genesis, one atomic transition function, and every recovery path must run an independent projection
rebuild from the same committed ledger head and retention policy and compare all three typed roots.
The `CommitmentSetV1` required-domain set and entries must contain those exact results under one
subject. Separately valid roots from different heads, subjects, retention policies, or commitment
sets do not pass. Refinement/conservation evidence remains separately required; equality of three
producer-computed digests is not independent evidence.

## Persistence, forks, and garbage collection

Both candidates should use immutable content-addressed nodes and path-copy updates. A candidate
branch is represented by its root; switching branches switches roots rather than applying an
untrusted sequence of inverse edits.

The crash-consistent commit unit is a small immutable manifest rather than an impossible atomic
write of every reachable node. R0 freezes a protocol-level `CommitmentSetIdHashV1` and these
canonical types:

```text
RequiredDomainV1 {
    domain_tag,
    composition_layer,
    root_type,
    tree_domain,
    root_schema_id,
    commitment_profile_id
}

DomainRegistryEntryV1 =
    ActiveDomainV1(RequiredDomainV1)
  | InactiveDomainV1(domain_tag, composition_layer, inactive_sentinel_schema_id)

DomainRegistryV1 = CanonicalSortedUniqueByDomainTag([
    DomainRegistryEntryV1...
])

DomainRegistryDigestV1 = ProfileIdHashV1(
    Domain("bitblock/domain-registry/v1")
 || LengthFrame(Canonical(DomainRegistryV1))
)

RootEntryV1 {
    required_domain,
    root_bytes
}

CommitmentSubjectV1 =
    LocalTransitionV1(local_storage_profile_id, scope_digest,
                      domain_registry_digest, ledger_sequence_u64)
  | NetworkHeightV1(chain_id, genesis_id, world_id,
                    ProfileContextV1, height_u64)

CommitmentSetPredecessorV1 =
    GenesisCommitmentSetV1
  | OrdinaryCommitmentSuccessorV1(prior_commitment_set_id)
  | MigrationCommitmentSuccessorV1(
        prior_commitment_set_id,
        InPlaceMigrationTransitionV1
    )

CommitmentSetV1 {
    schema_version,
    subject,
    ledger_head_id,
    required_domain_set,
    root_entries,
    predecessor
}

CommitmentSetIdV1 = CommitmentSetIdHashV1(
    Domain("bitblock/commitment-set-id/v1")
 || LengthFrame(Canonical(CommitmentSetV1))
)
```

`CommitmentSetV1` never contains its own identifier. `CommitmentSubjectV1` is a tagged union; the
same integer cannot be interpreted once as a local transition and once as a network height. The
network variant uses the sole `ProfileContextV1` defined by the Layer 1 byte-contract owner; it
binds protocol version, `profile_id`, scope digest, and domain-registry digest as external context
alongside immutable chain/genesis/world identity. This does not put `profile_id` inside a logical
root or inside the candidate bytes that derive it. The outer chain/genesis/world fields MUST
byte-equal their projection inside `ProfileContextV1`; disagreement rejects rather than selecting
one owner. A local storage profile resolves its own scope and registry under the explicit digest
fields rather than inheriting network semantics implicitly.

`RequiredDomainV1` is byte-for-byte the active-entry projection from the subject's canonical
`DomainRegistryV1`. `required_domain_set` is the canonically sorted set of unique
`RequiredDomainV1` values fixed by that registry. `root_entries` contains exactly one
`RootEntryV1(required_domain, root_bytes)` for every required value and no other entries, sorted by
ascending canonical `required_domain` bytes. The registry has one entry per `domain_tag`; an
inactive entry has no required-domain value and therefore cannot acquire a root entry. Duplicate,
missing, extra, unknown, active/inactive-flipped, or out-of-order entries reject, as does a scope or
registry digest that does not reproduce from the resolved subject.

For every entry, the immutable profile registry must resolve `commitment_profile_id` to a
descriptor whose `root_type` and `tree_domain` equal the nested required-domain value, whose proof
policy is activated for the subject, whose operational-limit profile is activated for the subject,
and whose digest length equals `root_bytes.len`. The resolved domain registry separately fixes the
same `domain_tag`, `composition_layer`, `root_schema_id`, and commitment-profile identifier.
Re-labeling a root under a different tag, layer, type, domain, schema, profile, policy, limit profile,
or digest length rejects even when the raw root bytes happen to be equal. If application or
execution roots are retained as entries, they are recomputed from exactly the registry-defined
active roots, typed inactive sentinels, and subject context. They never hash `CommitmentSetIdV1`;
the set identifier is derived only after every logical and composed root exists.

`GenesisCommitmentSetV1` is legal only for the declared local genesis transition or network genesis
height; R0 freezes the exact genesis counter for each kind. `OrdinaryCommitmentSuccessorV1` names an
existing complete parent set whose subject has the same tag and exactly the same non-counter
context—including network `ProfileContextV1`—and increments the local sequence or network height by
exactly one. It cannot cross a protocol, profile, scope, or registry boundary.

`MigrationCommitmentSuccessorV1` is network-only. Its prior set MUST have the same chain, immutable
`genesis_id`, world, and immediately preceding height as the candidate set. The prior subject's
profile context MUST byte-equal `old_active_profile_context`; the candidate subject's context MUST
byte-equal the distinct `new_admitted_profile_context`; that admitted profile's candidate lineage
MUST name the old admitted profile, and its `PROFILE-1` evidence MUST already have consumed the
matching concrete `UPGRADE-1` old-to-candidate migration evidence. The migration transition's
specification, activation authorization, and activation height MUST match the proposal/header and
parent-authorized governance state. Its typed old execution-root reference MUST recompute from the
prior set, and its typed new execution-root reference MUST recompute from the candidate set after
deterministic migration. Consensus acceptance activates the previously admitted successor;
admission, migration evidence, and the predecessor tag grant no activation authority by themselves.

Overflow rejects. Forks may produce several ordinary or migration children of one parent, but no
child may name itself, a descendant, a missing set, or a set from another chain/genesis/world
context. The ledger record/head and migration record are derived without the current
`CommitmentSetIdV1`; they may commit already-derived logical roots but cannot point back to the set
that contains them. The acyclic construction and publication direction is therefore:

```text
prior set + ordinary transition input
  or prior set + authorized migration input
    -> canonical post-transition ledger state
    -> logical map roots
    -> ledger record/head (which may embed those roots, never the current set ID)
    -> optional application/execution roots from those roots and fixed context
    -> CommitmentSetV1 bytes
    -> CommitmentSetIdV1
    -> recoverable current-head pointer
    -> later receipt/proposal/header references
```

Writers first acquire the exclusive pin/publication barrier described below. While holding it they
durably stage content-addressed nodes, reuse any old nodes, verify complete reachability and every
root-entry relation, construct the canonical manifest bytes, and recompute its identifier. They
then durably install a checksummed provisional-pin record binding an attempt identifier, expected
prior set identifier, candidate set identifier, and the canonically sorted complete node-store root
keys. Only after rereading that record and incrementing the pin epoch may they release the barrier.
They next durably write and reread the exact manifest under its identifier, recompute the identifier
again, and only then compare-and-swap one checksummed recoverable head from the expected prior set
identifier to the new identifier.

The commitment-set head is the sole mutable current-state pointer; consumers obtain
`ledger_head_id` and every root from it rather than advancing a second ledger pointer independently.
A failed compare-and-swap publishes no current-state authority; its immutable manifest and
provisional pin remain a recoverable losing candidate until policy retains or recovery reclaims
them. Recovery accepts exactly the previous complete set or the next complete set—never an old
ledger head with new roots, roots from different transitions, or a head whose reachable closure is
missing. The exact fsync/rename/journal mechanism belongs to `STORAGE-1`, but this tuple invariant
does not.

Logical map roots remain state identities and may repeat after delete/reinsert restoration. A
`CommitmentSetIdV1` is a manifest-occurrence identity and, when selected by a recovered head, a
publication identity: its subject and predecessor make distinct transitions distinct even when
every logical root repeats. Neither identifier is evidence of canonical-chain selection, finality,
or availability.

Node storage should be keyed by at least `(commitment_profile_id, tree_domain, node_hash)`. Hash
alone is not enough across tree types or cryptographic migrations. Every node read must decode
canonically and recompute its hash before use. A storage key that resolves to different bytes, a
noncanonical node, or a hash mismatch fails closed rather than being treated as a cache miss.

Garbage collection is outside the logical root but inside operational safety:

- explicitly pin genesis, accepted, finalized/confirmed, candidate, checkpoint, and archival roots
  required by the activated policy;
- before staging or reusing any potentially unpinned node, a writer takes the exclusive
  pin/publication barrier. While holding it, the writer stages all new nodes, verifies the complete
  candidate closure, constructs the candidate set ID, durably installs and rereads the checksummed
  provisional-pin record for that now-readable root set, increments the pin epoch, and only then
  releases the barrier. A crash before the valid pin leaves only unpublished orphans; a crash after
  it leaves one complete pinned closure. The protection is root-reachability based, so it covers
  deduplicated old nodes as well as newly staged nodes;
- a concurrent mark records `(pin_epoch, canonical_pin_set)`. Every pin addition or removal while
  marking passes through a write barrier that enqueues the affected root and increments the epoch.
  Mark completes only after its queue is empty and a fresh pin snapshot has the same epoch;
- sweep takes the exclusive pin/publication barrier, rechecks the stable epoch and complete mark,
  and permits no pin, unpin, root switch, or head publication until sweep finishes. If either check
  fails, sweep re-marks or stops without reclaiming anything;
- after a successful head compare-and-swap, the provisional pin remains valid until the writer
  rereads that head, idempotently promotes the pin, and only then releases superseded pins. A crash
  at any point replays the same order, so a head is never visible before its complete root set is at
  least provisionally pinned;
- recovery classifies every provisional pin: promote it when referenced by the recovered head or a
  retained candidate policy; otherwise release it only after proving that no recovered head,
  retained manifest, or permanent pin references it. Uncertain or incomplete recovery retains the
  pin and fails closed; a head naming a missing, corrupt, or incomplete manifest/pin also disables
  sweep and authority startup. Bounded later mark/sweep may reclaim verified orphan nodes;
- treat crash-sensitive reference counts as an optimization, not the only reachability authority;
- assay crash points before and after barrier acquisition/release, new-node staging, reuse of old
  unpinned nodes, closure verification, provisional-pin persistence, manifest persistence, head
  compare-and-swap, pin promotion/release, and every GC epoch boundary; harmless unreferenced nodes
  may survive a crash, but a published root may never lack one of its reachable nodes;
- make GC bounded and resumable, and reclaim nothing when a complete verified mark cannot finish;
- bound provisional-pin count, age, recovery scan, and orphan bytes so repeated crashed writers
  cannot create an unbounded permanently pinned leak;
- retain enough old-profile nodes to verify historical blocks under the upgrade policy.

Logical pruning and physical garbage collection are different transitions. Removing an episode
record from the logical retained set changes `EpisodeRootV1` and must be a canonical, authorized,
receipted transition under the activated retention policy. GC may remove only unreachable physical
node bodies; it must not change any logical root. Releasing an archival duty additionally requires
the declared availability/retention handoff—it is not licensed merely because a local root was
unpinned.

Repeated place/delete cycles can create unbounded historical nodes even when live occupancy returns
to the same root. Trie compression does not solve history growth, witness retention, archival duty,
or state spam. `RESOURCE-1` must eventually price or bound those obligations.

## Research checkpoint sequence

No step below changes the live authority path.

### R0 — freeze common logical inputs

Specify `CoordinateKeyV1`, the exact MSB-first traversal/bitmap/sibling order, typed values, tree
domains, `ProfileIdHashV1`, the canonical `ProofPolicyDescriptorV1` and
`CommitmentOperationalLimitsDescriptorV1` values and IDs, both `CommitmentProfileDescriptorV1`
values and IDs, every fully versioned proof-codec ID, the common node-hash framing, the exact
Patricia depth/empty rules, the complete SMT default recurrence and decision-depth bitmap rule, and
duplicate-key rejection. Freeze
`CommitmentSetIdHashV1`, `RequiredDomainV1`, the pre-ID `DomainRegistryV1` and its protocol-hash
digest, `RootEntryV1`, `CommitmentSubjectV1`, `CommitmentSetV1`, their canonical codecs, and the
genesis and predecessor rules before persistence tests.

Freeze one common logical node-store envelope, durability mode, cache-cold/cache-warm protocol,
build profile, toolchain, corpus and operation order, allocation instrument, and method for
reporting input-sort cost separately. Use the same hash suite and logical maps for the two hash-tree
candidates so the experiment measures structure rather than two changes at once. Also pre-register
numeric proof and operational hard bounds, the Patricia materiality threshold, and operational
definitions of every deterministic work unit, “simplest,” and “material” before observing results.
The separate Verkle feasibility lane remains deferred and supplies no result to R0–R5.

**Stop if:** a logical map, key, value, absence meaning, domain, profile/policy descriptor,
identifier, root entry, subject, or limit can be encoded two ways; a descriptor contains its own ID
or a derived root; or a local cap can widen an admitted proof policy.

### R1 — full rebuild prototypes

Build each candidate from a sorted, duplicate-rejecting stream derived from `World::iter_on()`.
Do not hook `World::set_on`, `set_off`, rendering, collision, history publication, or the default
executable yet. Each builder resolves its immutable profile/policy descriptors, recomputes their
identifiers independently, and emits only the typed root named by that profile.

**Stop if:** insertion permutations or distinct chunk allocation histories produce different roots
for the same occupied set, or two descriptor definitions resolve under one identifier.

### R2 — point proofs and negative corpus

Generate and independently verify membership and non-membership witnesses. Include empty maps in
generic tests, bedrock-only valid worlds, signed extrema, deep shared prefixes, maximally divergent
keys, reversal-sensitive bit/axis/child/bitmap/sibling vectors, non-default SMT branches, malformed
packing, redundant nodes, wrong domains/depths/profiles/policies, attempts to widen a local cap, and
over-limit proofs. Patricia vectors include the zero-record empty proof and illegal serialized-empty
alternatives, root/suffix/branch boundary depths 0/191/192, and wrong child depths. SMT vectors
distinguish bitmap decision depth `d` from sibling default depth `d+1` at both ends of the path.
Every proof-policy limit gets an independent boundary and one-over-limit vector; a passing aggregate
limit cannot hide a violation of one component limit.

**Stop if:** any alternate encoding verifies, any unused proof material is accepted, or absence can
be claimed without an authenticated terminal condition, or an inconsistent expected context is
accepted because its fields merely agree with one another.

### R3 — incremental update parity

Add functional insert/delete updates and compare the result after every operation with a fresh full
rebuild. Required identities include:

```text
Root(insert(S, c)) = rebuild(S union {c})
Root(delete(S, c)) = rebuild(S minus {c})
Root(delete(insert(S, c), c)) = Root(S)  when c was absent
```

Also test idempotent placement/deletion, immutable bedrock behavior at the world transition layer,
long delete/reinsert sequences, a leaf reaching the zero-bit suffix at depth 192, and insert/delete
collapses across branches at depths 0 and 191.

**Stop if:** root identity depends on operation history, insertion order, database layout, or a
noncanonical deletion shape.

### R4 — multiproof, persistence, and fork assay

Measure canonical multiproofs for clustered and dispersed coordinates. Exercise sibling forks,
root switching, restart, partial writes, pinning, mark-and-sweep, and old-root verification after
new writes. Independently reproduce `RequiredDomainV1`, the domain-registry digest, `RootEntryV1`,
`CommitmentSetV1`, and `CommitmentSetIdV1`; reject wrong subject tags or profile/scope/registry
context, missing/extra/duplicate/out-of-order entries, required-domain mismatches, wrong-profile
roots, active/inactive flips, invalid genesis/predecessors, and failed head compare-and-swap.

Exercise at least these exact schedules, with restart at every boundary:

```text
mark snapshot E -> writer barrier -> stage/reuse old unpinned fork -> pin at E+1 -> release
                -> sweep rechecks E+1 against mark E -> no reclaim/re-mark
writer barrier -> stage new/reuse old -> closure -> set ID -> provisional pin -> release
               -> manifest -> head CAS -> head reread -> pin promotion -> old-pin release
mark -> pin or unpin through the write barrier -> epoch recheck -> sweep
writer barrier -> crash before/after node, closure, set ID, provisional-pin write/reread, release,
                  manifest write/reread, CAS, head reread, promotion, and cleanup boundaries
failed head CAS -> candidate pin remains -> recovered non-reference proof -> bounded release
repeated crashed writers -> bounded provisional-pin/orphan recovery
old/new ledger, root-entry, subject, predecessor, manifest, and head mixtures
```

**Stop if:** reclaiming one branch can invalidate a pinned or provisionally staged root, restart can
publish any incomplete or mixed commitment set, a visible head precedes its complete provisional
pin, a new pin can race sweep, failed CAS changes authority, provisional-pin leaks exceed their
frozen bound, or proof validity depends on mutable cache metadata.

### R5 — independent parity and decision

Publish language-neutral vectors for keys, profile/policy descriptors and identifiers, nodes, empty
roots, updates, proofs, malformed inputs, multiproofs, root entries, commitment sets, and
commitment-set identifiers. Record each implementation's language, toolchain, source identity, and
shared dependencies; the second implementation must not reuse the authoritative encoder, node
logic, proof verifier, or manifest encoder. It must reproduce roots, identifiers, and rejection
results. Demonstrate that the gate fails under powered mutations to coordinate bit order, child
order, Patricia pre-node/child depth and empty-proof rules, SMT default recurrence and bitmap
sibling depth, proof trailing-material and policy-limit rejection, local-cap widening, proof- or
operational-policy ID substitution, profile-definition binding, root-entry/domain equality,
predecessor direction, and commitment-set subject/ID framing.

Compare:

- full rebuild time and peak memory by occupied keys and chunks;
- single insert/delete hashed bytes, compression-function invocations, allocations, bytes written,
  and path churn; raw hash-call counts are reported but not compared as equal work when preimage
  lengths differ;
- membership/non-membership logical witness content, compressed wire bytes, hashed bytes, and
  verification work;
- clustered/dispersed multiproof logical content, wire-compression result, and verification work;
- persistent database bytes and deduplication across forks;
- garbage-collection scan/write cost;
- provisional-pin, manifest, compare-and-swap, and crash-recovery work;
- worst-case accepted proof topology and parser allocation;
- complexity of the refinement argument and number of canonical rejection rules.

**Decision rule for the two admitted hash trees only:** apply only the numeric hard bounds,
“simplest” definition, and Patricia materiality threshold frozen in R0. Select Patricia only if it
crosses that pre-registered benefit threshold without an unresolved normalization, proof,
persistence, or refinement gap. Record the selected hash-tree profile, rejected hash-tree
alternative, evidence, falsifier, and reversal/migration cost. If neither meets the frozen bounds,
keep `COMMIT-1` locked rather than redefining the gate. The result makes no selection or ranking
claim about the separately deferred Verkle lane.

## Later integration order

After `LEDGER-1` and the research decision pass:

1. install the independently reproduced profile/policy descriptors, identifiers, and exact
   operational limits for every activated hash-tree domain;
2. introduce new typed root wrappers; do not reinterpret `ContentTag(TagKind::World)`;
3. independently project the exact world, active-token, and retained-episode maps from one ledger
   head and compute their full-rebuild candidate roots;
4. add incremental sidecar updates behind a research feature;
5. after every generated transition, assert incremental/full-rebuild and cross-map equality in tests
   and bounded debug/research modes;
6. under the exclusive pin/publication barrier, durably stage and verify every required reachable
   node, independently validate the required-domain/root-entry set, construct one canonical
   `CommitmentSetV1` and reproduce its `CommitmentSetIdV1`, then durably establish the complete
   whole-root provisional pin bound to that ID and release the barrier; durably write and reread the
   same manifest, reproduce its ID again, compare-and-swap its checksummed head, reread the head,
   and promote the pin; each non-world root must already have passed its own codec, proof-policy,
   numeric-limit, and domain profile;
7. provide independent decoders/verifiers for descriptors, roots, proofs, entries, and commitment
   sets that accept supplied carriers rather than trusting the producer;
8. only then allow receipts or later proposal headers to reference the new typed root or
   commitment-set identifier, following the acyclic direction above;
9. keep legacy `world_tag` versioned for historical receipt verification until an explicit
   migration policy says otherwise.

Direct `World::set_on` and `set_off` call sites exist, so hooking only the history path would not
guarantee commitment parity. The first live integration must either centralize authoritative
mutation or make bypasses structurally impossible.

## What this checkpoint cannot prove

Even a perfect authenticated-map implementation would not establish:

- that the supplied root is the canonical latest root;
- that the transition producing it was valid;
- that an actor authorized the transition;
- that independent validators agreed on it;
- that block bodies, witness nodes, or historical roots remain available;
- that occupancy and assets satisfy a cross-root conservation invariant;
- that a camera rendered the committed state or a physical display showed it;
- that a token has value, legal status, privacy, fair distribution, or one-human ownership;
- that SHA-256 or any future ternary hash has security beyond its declared assumptions;
- that any later vector-commitment, curve, generator basis, setup, or transcript is secure merely
  because a Verkle proof is smaller or verifies;
- that the address space or retained history can grow without resource limits.

The authenticated tree is useful precisely because its claim is narrow: exact key/value evidence
relative to an exact typed root.

## Open decisions retained

1. Binary Patricia or 192-level sparse Merkle after the common assay?
2. Exact `ProfileIdHashV1`/`CommitmentSetIdHashV1` suites, hash-tree node/root codecs, fully
   versioned proof-codec IDs, hash suite, descriptor bytes, tags, and resulting profile/policy
   identifiers?
3. Ordered traversal or canonical DAG multiproofs?
4. Does the first network profile commit one world only, and what exact `AuthenticatedRootRefV1`
   bytes bind the context-free local root to `chain_id`, `genesis_id`, and `world_id`?
5. Which roots and node bodies must every full verifier, validator, archive, and light client
   retain?
6. Which candidate/provisional roots are pinned, and when can each be reclaimed?
7. Is a future spatial/chunk/Morton key profile worth a migration, or are point and multiproofs
   sufficient?
8. What exact old/new-root migration evidence is required when the tree codec or hash changes?
9. Does the eventual asset decision keep bedrock non-economic, treat it as a locked genesis unit,
   or define a separate genesis allocation outside occupancy episodes?
10. What numeric materiality threshold must Patricia beat before its added normalization surface is
    accepted over the sparse-Merkle baseline?
11. What are the canonical widths/codecs and per-domain commitment profiles for `MintIdV1`,
    `EventIdV1`, episode records, actor IDs, authorization records, and later asset keys/values?
12. What exact numeric tuples become the immutable `ProofPolicyDescriptorV1` and
    `CommitmentOperationalLimitsDescriptorV1` before any remote proof format or durable authority
    profile is admitted?
13. After the separately declared feasibility, cryptographic, dependency, cost, and migration gates
    exist, does any later profile admit a Verkle construction? The first hash-tree profile is
    already explicitly deferred and supplies no answer to that later decision.

None of these choices is silently answered by this plan.
