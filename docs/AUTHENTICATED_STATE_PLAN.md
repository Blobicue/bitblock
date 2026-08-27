# Authenticated state research plan

This document records the proposed Merkle-Patricia-versus-sparse-Merkle checkpoint for Bitblock.
It is a **research and decision plan**, not an implemented commitment, selected consensus schema,
or security proof. The authenticated-map choice remains open until both candidates pass the same
vectors and their costs are measured.

The corresponding capability nodes are `COMMIT-R1` and `COMMIT-1` in
[`CAPABILITY_TREE.md`](CAPABILITY_TREE.md). The wider root hierarchy and proof meanings are in
§5 of [`LAYER1_DESIGN_PAPER.md`](LAYER1_DESIGN_PAPER.md).

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

## Candidate A — fixed-key binary Merkle Patricia trie

This is a constrained custom binary Patricia design over exactly 192 key bits. It deliberately
does not import a general blockchain trie codec with variable key kinds, branch values, or
inline-versus-hashed child alternatives.

The node grammar to freeze before implementation is:

```text
Empty
Leaf(remaining_key_bits, typed_value_bytes)
Extension(nonempty_shared_bits, child_hash)
Branch(left_hash, right_hash)
```

Canonical normalization must make the logical map-to-root function unique:

- every key consumes exactly 192 bits;
- a leaf stores the complete unconsumed suffix and one canonical typed value;
- an extension path is non-empty and points to a branch;
- adjacent extensions are merged;
- a branch has two non-empty logical children;
- deletion of one branch child prepends the surviving direction bit to the survivor: a leaf becomes
  `Leaf(bit || suffix, value)`, an extension becomes
  `Extension(bit || prefix, child_hash)`, and a surviving branch becomes
  `Extension(bit, branch_hash)`; any parent extension is then concatenated so adjacent extensions
  cannot remain;
- no terminal branch value exists because all keys have fixed length;
- unused packed-path bits are zero;
- children are always represented by hashes in the canonical node preimage;
- the empty map has one domain- and profile-specific root;
- stored-false/tombstone leaves and duplicate logical keys are rejected.

Node hashes must bind at least:

```text
commitment_profile_id
tree_domain
node_codec_version
node_type
consumed_bit_depth
canonical node payload
```

Depth and tree-domain binding prevent a valid node from being transplanted to a different level or
logical map. Database metadata, compression, reference counts, cache hints, and storage offsets
must remain outside the authoritative node preimage.

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

Expected advantage: update work, absence semantics, proof parsing, deletion, and formal refinement
have a fixed shape. Expected cost: 192 logical hash levels per update and potentially larger point
witnesses if defaults do not compress well.

The sparse tree is the lower-complexity baseline, not an automatic winner. The Patricia candidate
must beat it on measured costs strongly enough to justify its larger canonicalization and storage
surface.

## Proof contract

The caller—not attacker-controlled proof bytes—selects one expected verification context:

```text
ExpectedProofContextV1 {
    commitment_profile_id,
    tree_domain,
    root_type,
    expected_root_bytes,
    key_codec,
    value_codec,
    proof_codec_version,
    ResearchLimitsV1
}

query_key and canonical key
claimed presence/value or claimed absence
bounded proof payload
```

If the carrier repeats a profile, domain, root type, root, or codec identifier, it must exactly
match the caller's expected value; the carrier cannot negotiate or downgrade it. The verifier must
check the root supplied by its caller and must not fetch a “latest” root from a mutable local
database during proof verification. A later network envelope separately verifies the
`AuthenticatedRootRefV1` chain/genesis/world/header context before passing its exact typed root into
this local verifier.

`ResearchLimitsV1` must give checked maxima for proof bytes, proof records/nodes, hash work,
multiproof query keys and DAG edges, full-build keys, batch updates, decoded node bytes, allocations,
pinned roots/forks, and GC mark/sweep work. All declared-length, multiplication, addition, count,
and work arithmetic must pass before attacker-sized allocation, node fetch, or hashing begins.

For the Patricia candidate, membership terminates at the exact key leaf. Because canonical branches
have two non-empty children, non-membership terminates only at the authenticated empty-tree root, a
divergent extension, or a divergent leaf. A nearby leaf without the complete root path is not an
absence proof.

For both candidates, verification must reject:

- wrong caller-selected root, domain, profile/version, root type, key codec, value codec, or proof
  codec; a wrong chain/world/header binding rejects in the later typed root-reference verifier;
- reordered, duplicate, unused, or trailing witness elements;
- alternate encodings of the same logical path or node;
- a reconstructed path longer or shorter than 192 bits;
- a leaf whose reconstructed key is not the requested key;
- noncanonical empty/default encodings;
- proofs above byte, node, depth, hash-work, or allocation limits;
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

Separate roots still do not by themselves prove the cross-map invariant:

```text
domain(active_token_map) = mutable_occupied_coordinates
```

That equality and the live-episode bijection must be enforced by genesis, one atomic transition
function, full-rebuild checks, and refinement/conservation evidence.

## Persistence, forks, and garbage collection

Both candidates should use immutable content-addressed nodes and path-copy updates. A candidate
branch is represented by its root; switching branches switches roots rather than applying an
untrusted sequence of inverse edits.

Node storage should be keyed by at least `(commitment_profile_id, tree_domain, node_hash)`. Hash
alone is not enough across tree types or cryptographic migrations. Every node read must decode
canonically and recompute its hash before use. A storage key that resolves to different bytes, a
noncanonical node, or a hash mismatch fails closed rather than being treated as a cache miss.

Garbage collection is outside the logical root but inside operational safety:

- explicitly pin genesis, accepted, finalized/confirmed, candidate, checkpoint, and archival roots
  required by the activated policy;
- mark reachable nodes from one authenticated pin-set snapshot/epoch, or forbid pin changes for the
  complete mark/sweep operation;
- treat crash-sensitive reference counts as an optimization, not the only reachability authority;
- assay crash points before and after node staging, root-pin publication, and ledger/root-head
  publication; harmless unreferenced nodes may survive a crash, but a published root may never lack
  one of its reachable nodes;
- make GC bounded and resumable, and reclaim nothing when a complete verified mark cannot finish;
- retain enough old-profile nodes to verify historical blocks under the upgrade policy.

Repeated place/delete cycles can create unbounded historical nodes even when live occupancy returns
to the same root. Trie compression does not solve history growth, witness retention, archival duty,
or state spam. `RESOURCE-1` must eventually price or bound those obligations.

## Research checkpoint sequence

No step below changes the live authority path.

### R0 — freeze common logical inputs

Specify `CoordinateKeyV1`, typed values, tree domains, commitment profile identifiers, canonical
hash framing, empty roots, `ResearchLimitsV1`, and duplicate-key rejection. Freeze one common
logical node-store envelope, durability mode, cache-cold/cache-warm protocol, build profile,
toolchain, corpus and operation order, allocation instrument, and method for reporting input-sort
cost separately. Use the same hash suite and logical maps for both candidates so the experiment
measures structure rather than two changes at once. Also pre-register numeric hard bounds, the
Patricia materiality threshold, and operational definitions of “simplest” and “material” before
observing results.

**Stop if:** a logical map, key, value, absence meaning, or domain can be encoded two ways.

### R1 — full rebuild prototypes

Build each candidate from a sorted, duplicate-rejecting stream derived from `World::iter_on()`.
Do not hook `World::set_on`, `set_off`, rendering, collision, history publication, or the default
executable yet.

**Stop if:** insertion permutations or distinct chunk allocation histories produce different roots
for the same occupied set.

### R2 — point proofs and negative corpus

Generate and independently verify membership and non-membership witnesses. Include empty maps in
generic tests, bedrock-only valid worlds, signed extrema, deep shared prefixes, maximally divergent
keys, malformed packing, redundant nodes, wrong domains, and over-limit proofs.

**Stop if:** any alternate encoding verifies, any unused proof material is accepted, or absence can
be claimed without an authenticated terminal condition.

### R3 — incremental update parity

Add functional insert/delete updates and compare the result after every operation with a fresh full
rebuild. Required identities include:

```text
Root(insert(S, c)) = rebuild(S union {c})
Root(delete(S, c)) = rebuild(S minus {c})
Root(delete(insert(S, c), c)) = Root(S)  when c was absent
```

Also test idempotent placement/deletion, immutable bedrock behavior at the world transition layer,
and long delete/reinsert sequences.

**Stop if:** root identity depends on operation history, insertion order, database layout, or a
noncanonical deletion shape.

### R4 — multiproof, persistence, and fork assay

Measure canonical multiproofs for clustered and dispersed coordinates. Exercise sibling forks,
root switching, restart, partial writes, pinning, mark-and-sweep, and old-root verification after
new writes.

**Stop if:** reclaiming one branch can invalidate a pinned root, restart can publish a ledger head
without its root nodes, or proof validity depends on mutable cache metadata.

### R5 — independent parity and decision

Publish language-neutral vectors for keys, nodes, empty roots, updates, proofs, malformed inputs,
and multiproofs. Record each implementation's language, toolchain, source identity, and shared
dependencies; the second implementation must not reuse the authoritative encoder, node logic, or
proof verifier. It must reproduce roots and rejection results. Demonstrate that the gate fails
under powered mutations to coordinate bit order, child order, default/empty-hash recurrence,
Patricia deletion normalization, proof trailing-material rejection, and domain/profile binding.

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
- worst-case accepted proof topology and parser allocation;
- complexity of the refinement argument and number of canonical rejection rules.

**Decision rule:** apply only the numeric hard bounds, “simplest” definition, and Patricia
materiality threshold frozen in R0. Select Patricia only if it crosses that pre-registered benefit
threshold without an unresolved normalization, proof, persistence, or refinement gap. Record the
selected profile, rejected alternative, evidence, falsifier, and reversal/migration cost. If
neither meets the frozen bounds, keep `COMMIT-1` locked rather than redefining the gate.

## Later integration order

After `LEDGER-1` and the research decision pass:

1. introduce new typed root wrappers; do not reinterpret `ContentTag(TagKind::World)`;
2. compute a full-rebuild candidate root from the committed logical state;
3. add incremental sidecar updates behind a research feature;
4. after every generated transition, assert incremental/full-rebuild parity in tests and bounded
   debug/research modes;
5. publish the ledger head, `WorldRootV1`, `ActiveTokenRootV1`, `EpisodeRootV1`, and every required
   reachable node atomically; each non-world root must already have passed its own domain profile;
6. provide a decoder/verifier that accepts supplied carriers rather than trusting the producer;
7. only then allow receipts or later proposal headers to reference the new root type;
8. keep legacy `world_tag` versioned for historical receipt verification until an explicit
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
- that the address space or retained history can grow without resource limits.

The authenticated tree is useful precisely because its claim is narrow: exact key/value evidence
relative to an exact typed root.

## Open decisions retained

1. Binary Patricia or 192-level sparse Merkle after the common assay?
2. Exact node and proof codec, hash suite, and commitment-profile identifier?
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
12. What exact numeric tuple becomes `ResearchLimitsV1` before any remote proof format is admitted?

None of these choices is silently answered by this plan.
