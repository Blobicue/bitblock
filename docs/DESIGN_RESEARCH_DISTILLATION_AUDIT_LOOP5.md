# Fifth bounded successor audit

**One read-only gap sweep followed by one coordinated fix sweep — 2026-08-27**

This successor audit freezes exact clean input
`2cb1cc02320d180ac9ee34bd103bd44459f128fa`, Git tree
`60e9714c6d691f383fc75be03f6c9927c7665b84`, on
`codex/bitblock-progress-2026-08-26`. The remote branch matched that commit and the worktree was
clean before discovery began.

The principal inspected blobs were:

```text
5da121396458d5caf7a17d102b877add6407a9d6  docs/LAYER1_DESIGN_PAPER.md
c923bcaa8877f21f08bbec5d79a7b952c0c364d9  docs/AUTHENTICATED_STATE_PLAN.md
5ef23dcb589abb4f33652930bda74ced5ce12143  docs/CAPABILITY_TREE.md
451af57d3df3668a92791d6ddbc11230a3c6ea46  docs/DESIGN_RESEARCH_DISTILLATION.md
f2929aa5567f24f4db8ec071840849769347547f  docs/HANDOFF.md
350d1af6279d7f441f45fba2689481c0dbea79a5  docs/DESIGN_RESEARCH_DISTILLATION_AUDIT_LOOP4.md
124e066b418a61279abf90c1b1b2c2cc0604ef2a  docs/LAYER1_DESIGN_AUDIT.md
d4bab12596440246197927ba65825c2a2750415c  docs/TRANSCRIPT_AUDIT.md
995f037f67217f1e92cb926a3cfb094d3816868d  docs/TIMELINES.md
2df2e0b922f973438a207bbbd7824c8ed98040b9  tools/check_planning_docs.py
df441fea73fd82c9709f14fb442a466fa13d314a  .github/workflows/ci.yml
11ffa977ae62f961d2c6b3c01eff173d4eff33c9  .gitattributes
52315110830df919363b2cc7513b0d2bb359ab10  README.md
```

Three independent read-only lenses attacked protocol/cryptographic inhabitation,
state/availability/replay closure, and capability-graph/provenance closure. A fourth local pass
reproduced the breaks, challenged overlaps and rejected weaker variants. A short adversarial
reconciliation then tried to falsify the eight surviving candidates and fixed their minimum
correction shapes. No audited input was edited and no Rust, Lean, broad Python-vector, headless,
network, storage-fault, cryptographic-implementation, performance, or physical-display test was run
before this finite set was frozen.

This record preserves findings and break witnesses, not verbatim agent transcripts or agent
identity. It must be committed before the correction. The correction may pin this file's
first-addition Git blob but must not mutate this file.

## Frozen finding set and one-sweep correction map

| ID | Priority | Gap at `2cb1cc0` | Concrete loss mechanism | One-sweep correction |
| --- | --- | --- | --- | --- |
| PG-01 | P0, new | In-place migration names a `new_admitted_profile_context`, migration specification, authorization, and activation height, but binds no successor admission-manifest or closure-bundle ID. No availability-covered carrier supplies the successor scope, registry, candidate, transition specification, admission evidence, admitted release, implementation artifacts, and concrete upgrade evidence. `CHAIN-LAUNCH-1` is initial-only. | A fresh verifier reaches an upgrade with a profile label but must consult an undeclared ambient registry to acquire the codec, transition rules, evidence, and release it is supposed to verify. Missing, substituted, or differently admitted successor artifacts cannot be distinguished from a valid activation. | Define a bounded canonical `SuccessorActivationBundleV1` or common profile-closure carrier, derive its fixed ID, bind that ID and the exact target admission ID into the migration context and activation authorization, availability-cover it under the old envelope, and verify its complete old-to-new closure before migration execution or activation. Give the capability graph an explicit owner and omission/substitution/unavailable/ambient-map controls. |
| PG-02 | P1, falsely closed TG-04 | The verifier decodes and canonically re-encodes the complete inner proposal in checklist step 3, but authority-selected profile limits are not resolved until step 5. The inner proposal already embeds the complete availability payload; step 7 then inconsistently says that payload is first obtained and bounded-decoded there. | An outer-cap-valid but profile-over-limit nested body, receipt, witness, count, compressed value, or parser workload can force allocation, materialization, hashing, decompression, or canonical re-encoding before deterministic rejection. A global byte cap does not enforce a tighter active profile's nested work limits. | Resolve authority-owned parse/work limits immediately after bounded lookup. Add a canonical shallow inner frame with independently bounded header, payload, and signature slices, or specify an equivalent allocation-free streaming preflight. Reject every nested length/count/decompression/work violation before materialization or hashing; perform whole-inner canonical equality only afterward and power a global-under/profile-over mutant. |
| PG-03 | P1, falsely closed QG-04 | The fixed outer union carries `AcceptedParentLookupV1(typed_parent_block_id_bytes)`, but `FixedPreProfileEncodeRefPayloadV1` is invoked and never defined. The referenced `block_id` is a bare output of the envelope-owner-selected hash suite, which may change across profile migration. | Before profile selection, implementations have no common suite/type namespace, digest-length relation, chain/launch binding, canonical reference bytes, or unique storage key. The same bytes can be zero-valued, multi-valued, or resolved under the wrong profile before inner projection checks are possible. | Define a protocol-frozen parent lookup-reference payload or fixed-hash lookup ID that binds launch/chain identity, envelope-owner profile, block-ID suite/type, exact bounded digest, and a unique index rule. Recompute it after parent retrieval and power wrong-chain/profile/suite/type/length/alias/collision controls. |
| PG-04 | P1, residual QG-01 | `LaunchClosureBundleV1` contains four counters, but no equation defines which of the seven explicit artifacts, supplementary artifacts, dependency nodes, nested repeated preimages, outer genesis package, metadata, or framing contributes to each count and byte total. Separately, the capability tree allows embedded or content-addressed preimages and mandatory locators, while the sole byte schema supports embedded preimages only. | Two implementations can compute different canonical totals or implement incompatible embedded-versus-locator packages while both satisfy the prose. A mismatch may be detected only after allocation, and an unavailable locator can masquerade as a complete launch carrier. | Choose embedded-only carriers or define one tagged, bounded carrier union with mandatory retrieval and hash verification. Freeze checked equations for explicit/supplementary/artifact/dependency/edge counts and the exact preimage-byte sum, state exclusions needed to avoid self-reference, preflight declared sizes, and power off-by-one, overflow, double-count, and unavailable-locator mutants. |
| PG-05 | P1, new | `AdmittedReleaseRecordV1` includes both a release authority policy ID and authorization bytes inside the same canonical bytes hashed as the record ID, but defines no authorization-free release core, sign document, signature domain, exact signed statement, or pre-existing policy/credential resolver. | An independent verifier cannot tell whether authorization covers the full record and ID, an omitted/zeroed authorization field, or a policy-specific subset; signing the full record or its final ID is circular. The record's own unverified policy ID can otherwise choose its verifier. | Define `AdmittedReleaseCoreV1`, derive a domain-separated core ID, and define detached/tagged `ReleaseAuthorizationV1` over that exact ID. Resolve policy, scheme, credentials, and codec from a named pre-existing authority rather than from the candidate record alone, then derive the final record ID from core plus verified authorization. Preserve policy-specific signature, threshold, or certificate shapes. |
| PG-06 | P1, new | The base dependency parser treats every backticked capability anywhere in a prose `Depends on` field as unconditional. It therefore manufactures `VERKLE-R1 -> COMMIT-R1` despite an explicit negation, `EMERGENCE-R1 -> VM-1` from a hypothetical later assay, and collapses phased dependencies for `REFINE-1` and `RELEASE-R1`. | The authoritative DAG can block independent research, contradict its own scheduling view, or claim that a reusable framework already consumes a candidate-only prerequisite. The graph checker can pass the wrong graph. | Restrict `Depends on` to true unconditional bases and move phase/instance prerequisites into a structured manifest. At this input the true bases are Verkle `{WORLD-0, PROOF-0}`, refinement `{WORLD-0}`, emergence `{RULE-1}`, and reusable release framework `{}`. Encode concrete refinement and release-candidate phase edges separately and power negation, hypothetical-prose, duplicate, undefined-node, cycle, and exact-manifest mutations. |
| PG-07 | P1, new | Multi-phase status is normative, but `_status_class` retains and digests only the first status prefix in each prose field. Changing `VERKLE-R1` from `LOCKED@prototype` to `READY@prototype` and `RELEASE-R1` from `LOCKED@candidate` to `READY@candidate` passed the complete checker self-test. | A ready research/framework phase can silently lend readiness to an authority/candidate phase while the advertised powered status guard remains green. | Make phase status a canonical parsed tuple manifest such as `node | phase | status`, cover every normative secondary status/open-decision marker, digest the exact tuples, and add promotion/demotion, duplicate, omission, unknown-phase/status, and subject-mutation controls. Keep explanatory prose non-authoritative. |
| PG-08 | P1/P2, residual QG-08 | README and transcript surfaces call `LAYER1_DESIGN_AUDIT.md` pinned or immutable historical evidence, but the file is absent from the frozen-file inventory, LF policy, blob checks, and powered mutations. Its current corrected blob is `124e066b418a61279abf90c1b1b2c2cc0604ef2a`; the original audit-result blob at commit `9eace50` was `ee2075547d7c7895c7d0ebaabfc64d1e09f5ba3a` before later provenance and F-12 errata. | Historical findings can be rewritten while the checker remains green, and calling the current file retrospectively immutable erases the difference between the original result and corrected reading copy. An in-memory `F-01` to `F-99` mutation passed the planning check. | Preserve both identities. Label the current file the canonical corrected historical reading copy frozen from this checkpoint forward, not the immutable first result; freeze its current blob and LF policy, add a powered content mutation, and update exact frozen-count/read-order/handoff claims. |

## Candidates rejected or classified after challenge

- The release finding does not require one universal authorization shape. The defect is the absence
  of any non-circular exact release statement and trusted policy resolver; a profile may still use a
  signature, threshold proof, certificate, or another frozen verified form.
- `ChainLaunchAuthorizationV1` is already detached over an already-derived manifest ID. Its
  policy-specific shape therefore does not reproduce PG-05.
- Generic migration inputs or witnesses do not close PG-01 because no typed field requires the exact
  admitted successor closure to be carried, bound, and available before execution.
- The initial launch bundle added for QG-01 does not cover successor activation. Reusing its carrier
  contract is appropriate, but the successor still needs a separately bound old-to-new closure.
- The global outer byte cap does not close PG-02: the active profile may impose tighter nested
  count, decompression, memory, proof, or work caps.
- `COST-1` method reuse is not a Verkle completion dependency; `COMMIT-R1` is explicitly not one.
  Hypothetical richer emergence work does not become a present edge until that phase is defined.
- Concrete cryptographic suites, numerical values, consensus choice, persistence, Verkle
  implementation, economic policy, custody, deployment, and Rust refinement remain visibly
  `OPEN` or `LOCKED`; they were not reclassified as hidden gaps merely because they are unfinished.

## Areas checked with no additional material gap

- The initial launch package now carries the formerly absent scope, registry, candidate,
  transition specification, admission, parity, release, and supplementary closure preimages.
- Canonical `BlockContext` is availability-covered and root-checked. The total
  `Execute(state, BlockContext, Body)` relation has disjoint ordinary/migration branches and a
  one-owner input partition.
- MPT path/deletion normalization, sparse-Merkle default-depth/bitmap semantics, state-versus-history
  separation, logical pruning versus physical garbage collection, and Verkle-selected CE-35 through
  CE-42 activation survived this sweep.
- Launch-profile versus target-profile conditional predicate contexts, the initial
  `CHAIN-LAUNCH-1` owner, and `TESTNET-1` launch consumption survived their direct checks.
- The exact 18-document planning inventory, seven already frozen files, contained relative links,
  powered checker path, CI command descriptions, correction commit, and remote branch pin matched
  the audited input. PG-08 is specifically the one historical file still outside that freeze.

## One-fix-sweep rule

The correction may change only the protocol/planning/checker/handoff surfaces needed to close
PG-01 through PG-08, connect this audit to the frozen inventory/read order, and power the new
structured controls. It may not activate a capability, implement a network or cryptographic
primitive, edit any previously frozen audit or this file, or silently add a ninth finding. Any
later discovery belongs to another pinned loop.

The correction remains documentation, checker, attribute-policy, and handoff work. Narrow checker
execution and whitespace checks are correction gates. Broad runtime/proof tests begin only after a
complete later read-only sweep finds neither a new material gap nor a falsely closed material
residual under its declared finite lenses.

## Residual boundary

Closing these findings cannot prove that the protocol is secure, that launch or successor-profile
artifacts are available in deployment, that a candidate profile is satisfiable, that cryptographic
schemes are sound, that the implementation respects the proposed byte graph, that CI infrastructure
is honest, or that Rust refines the planning documents. The next complete sweep must pin the
correction and may declare diminishing returns only within its explicit finite coverage.
