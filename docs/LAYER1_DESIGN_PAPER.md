# Bitblock: From Binary Occupancy to a Verifiable Layer 1

**Current post-checkpoint descendant proposed reading — 2026-08-27**

This mutable paper is not checkpoint 0.3. The historical checkpoint-0.3 paper remains pinned at
commit `034b7b2b5f129625f9c6dcec3e14038b7c4aee5f`, with its corrected historical reading in
[`LAYER1_DESIGN_AUDIT.md`](LAYER1_DESIGN_AUDIT.md). The review chronology leading to this descendant
is, in order:

1. [`FUTURE_PLAN_AUDIT.md`](FUTURE_PLAN_AUDIT.md)
2. [`FUTURE_PLAN_AUDIT_LOOP2.md`](FUTURE_PLAN_AUDIT_LOOP2.md)
3. [`DESIGN_RESEARCH_DISTILLATION_AUDIT.md`](DESIGN_RESEARCH_DISTILLATION_AUDIT.md)
4. [`DESIGN_RESEARCH_DISTILLATION_AUDIT_LOOP2.md`](DESIGN_RESEARCH_DISTILLATION_AUDIT_LOOP2.md)
5. [`DESIGN_RESEARCH_DISTILLATION_AUDIT_LOOP3.md`](DESIGN_RESEARCH_DISTILLATION_AUDIT_LOOP3.md)
6. [`DESIGN_RESEARCH_DISTILLATION_AUDIT_LOOP4.md`](DESIGN_RESEARCH_DISTILLATION_AUDIT_LOOP4.md)
7. [`DESIGN_RESEARCH_DISTILLATION_AUDIT_LOOP5.md`](DESIGN_RESEARCH_DISTILLATION_AUDIT_LOOP5.md)
8. [`DESIGN_RESEARCH_DISTILLATION_AUDIT_LOOP6.md`](DESIGN_RESEARCH_DISTILLATION_AUDIT_LOOP6.md)
9. [`DESIGN_RESEARCH_DISTILLATION_AUDIT_LOOP7.md`](DESIGN_RESEARCH_DISTILLATION_AUDIT_LOOP7.md)
10. [`DESIGN_RESEARCH_DISTILLATION_AUDIT_LOOP8.md`](DESIGN_RESEARCH_DISTILLATION_AUDIT_LOOP8.md)
11. [`DESIGN_RESEARCH_DISTILLATION_AUDIT_LOOP9.md`](DESIGN_RESEARCH_DISTILLATION_AUDIT_LOOP9.md)

Each audit freezes the input it attacked; none audits its own later correction. This descendant
incorporates the proposed corrections through the frozen Loop 9 finding set, but it has not passed a
later zero-gap sweep and does not activate any protocol, network, cryptographic primitive, or Layer 1
security claim. [`TRANSCRIPT_AUDIT.md`](TRANSCRIPT_AUDIT.md) records the broader source chronology.

This paper makes the Bitblock idea explicit as a protocol design. It starts from the
current Rust implementation and describes the additional machinery required before any
cryptocurrency or Layer 1 claim would be honest.

It is a design document, not a claim that the current executable is a blockchain. The
current project deliberately stops at a deterministic local coordinate world, local
history evidence, and bounded formal models.

## Reading the status labels

- **Implemented** — present in the current source tree.
- **Bounded** — covered by a finite test, vector, or abstract proof whose scope is named.
- **Proposed** — a protocol shape that could be implemented later.
- **Open** — a decision or proof obligation that is not settled.
- **Not claimed** — a property the current system explicitly refuses to imply.

Normative words such as **MUST**, **SHOULD**, and **MAY** describe the proposed future
protocol, not the current executable.

## Security-claim boundary

“Full Layer 1 security coverage” is not an unconditional promise. It means that every
claimed property is tied to a protocol version, an adversary model, explicit cryptographic
and network assumptions, and evidence that reaches the named boundary. A protocol cannot
prove safety against an unspecified validator set, an unspecified network, an unknown
implementation, or an unavailable history.

This paper therefore treats security as a coverage matrix, not a badge. A passing local
test can close a local invariant while leaving consensus, custody, or availability open.
The final public claim MUST state both what is covered and what remains assumed or excluded.

For this paper, a **Layer 1** means the base replicated state machine that accepts
transactions, commits canonical state roots, and provides a finality/availability path to
independent nodes. It does not mean the renderer, a game server with an internal database,
a local hash chain, a bridge, or a token that has merely been given a market name.

## Abstract

Bitblock begins with one simple rule: a coordinate is a bit. If a block exists at a
coordinate, the bit is `1`; if it does not, the bit is `0`. The intended protocol is an
ordered fold over canonical transactions. The current fixed tick is a smaller local reducer:
it consumes aggregate live-input state plus ordered edit attempts, while supplied
`InteractionTape` values provide bounded replay input. It is not yet a complete live action
recorder. Rendering is a projection of state, not a second simulation authority.

That model has a useful accounting interpretation. A successful transition from `off` to
`on` can open one issuance episode, and a transition from `on` to `off` can close it. This
gives exact local conservation and provenance, but it is not yet a cryptocurrency. A
cryptocurrency additionally needs authenticated participants, authorization, transfer
semantics, replay and double-spend protection, an agreed ordering, finality, data
availability, resource resistance, and independent verification.

The proposed architecture therefore separates five things that are easy to accidentally
collapse:

1. **World state** — which coordinates are occupied.
2. **Asset state** — which occupancy episodes have live token records.
3. **Authority** — which identity may submit or authorize an action.
4. **Replication** — which machines agree on one ordered history.
5. **Observation** — how a camera and renderer display the committed state.

Only the first four are relevant to a Layer 1 state machine. Pixels, camera floats, and
window timing are not consensus evidence.

## 1. Current foundation

The starting point is intentionally small:

| Surface | Current truth | Status |
| --- | --- | --- |
| Coordinate domain | Signed 64-bit `(x, y, z)` addresses; absent means off | Implemented |
| Storage | Lazy 16×16×16 sparse chunks; empty non-origin chunks can be released | Implemented |
| Spatial origin anchor | `(0, 0, 0)` is permanent bedrock and cannot be removed | Implemented |
| Observer | Camera state beside bedrock; no entity, gravity, terrain, inventory, or autonomous rules | Implemented |
| Tick | Fixed 60 Hz reducer consumes aggregate live-input state and ordered edit attempts | Implemented |
| World/history identity | Canonical SHA-256 content tags and parent-linked local receipts | Implemented |
| Interaction replay | Supplied bounded canonical `InteractionTape` values and headless replay reports; no complete live recorder | Implemented, bounded |
| Mint/burn evidence | `CoordinateEpisodeEvidence` records local issuance episodes and consuming burns | Implemented, feature-gated, bounded/in-memory |
| Abstract proof | Lean finite-support models cover bedrock/edit replay and per-edit supply delta/no-op theorems; not token identity or Rust refinement | Bounded |
| Ownership | No owners, accounts, signatures, or transfer records | Not claimed |
| Networking | No replicated nodes or wire consensus protocol | Not claimed |
| Persistence | Research storage exists, but the default live history is not durable custody | Not claimed |
| Layer 1 security | No finality, validator set, fork choice, or availability protocol | Not claimed |

The existing SHA-256 tags identify canonical bytes under named schemas. They do not prove
that the bytes are true, current, owned, persisted, or agreed by other machines. The
research-only coordinate-episode surface records and locally verifies bounded issuance
provenance; it has no balances, owners, asset transfers, durable custody, or monetary value.
The immutable origin is a spatial invariant. The current local history genesis commits an
initial world digest, but it is not a chain identity, validator trust anchor, or future
protocol genesis package.

## 2. The semantic state machine

Let the coordinate domain be

```text
C = { -2^63, ..., 2^63 - 1 }^3
```

for the current version, and let `o = (0, 0, 0)` be the immutable origin. A world state
contains an occupied set:

```text
B_h : C -> {0, 1}
B_h(o) = 1
```

The absence of a stored bit is semantically `0`, not “unloaded” or “unknown”. Storage is
an implementation of this function; it is not the definition of the world.

A future deterministic execution state can be written as:

```text
ApplicationState_h = (B_h, M_h, E_h, X_h)
ProtocolState_h    = (A_h, N_h, P_h, G_h, Q_h)
ExecutionState_h   = (ApplicationState_h, ProtocolState_h)

ConsensusHead_(i,h) = (height, block_id, execution_state_root, history_root,
                       consensus_status, availability_status,
                       availability_assessment_ref)

NodeLocalView_i = (UI_i, mempool_i, candidate_branches_i, current_votes_i,
                   timeouts_i, peers_i, sync_i, caches_i)
```

where:

- `B_h` is the occupied-coordinate function;
- `M_h` is the active mint map from mutable occupied coordinates to token identifiers;
- `E_h` is canonical mint/burn episode and event state, including live/consumed status and
  provenance needed by the chosen pruning policy;
- `ApplicationState_h` MAY also contain `X_h`, the selected ownership, transfer, balance,
  fee, or stake state. `X_h` is empty until an asset/economic model is chosen, but any later
  asset domain MUST have its own canonical state and root;
- `A_h` is committed authorization, identity, delegation, rotation, and revocation state;
- `N_h` is committed per-identity replay state such as nonces;
- `P_h` is committed protocol-version, codec, resource, and economic parameter state;
- `G_h` is committed governance, upgrade, and emergency-policy state;
- `Q_h` is the validator/sequencer schedule selected by the verified envelope authority: the
  launch-authorized initial schedule at the first ledger height and the accepted-parent-authorized
  schedule thereafter, under the selected consensus rule, including
  active and delayed-next authority sets where the selected consensus needs them;
- `ConsensusHead_(i,h)` identifies node `i`'s rule-verified ledger prefix, exact
  finality/confirmation status, and typed availability status/assessment reference. For an
  accepted head, those values are the deterministic profile-owned projection of the verified
  proposal-credential and evidence sets for one byte-exact `AcceptedProposalCoreV1`; they are not
  selected from an arbitrary `AcceptedBlockRecord` wrapper. A node cannot attach a status not
  derived by that projection.
  Before qualifying evidence verifies, a candidate view uses
  `(Unverified, None)` and cannot satisfy a profile that requires availability before acceptance.
  A probabilistic tip may be node-relative; a public safety claim covers only the prefix satisfying
  its declared confirmation/common-prefix rule; and
- `NodeLocalView_i` contains facts that may legitimately differ between honest nodes and is
  never covered by `execution_state_root`.

Candidate branches, mempool contents, current-round votes, timeouts, peer state, retrieval
progress, caches, camera state, and rendering state are not deterministic results of user
transactions. They MUST NOT be placed inside `ExecutionState_h`. Finality evidence is
detached consensus evidence over a proposed block identifier; actual byte availability is a network
property evaluated under the selected rule, not a Boolean manufactured by `Execute`.

The current executable has `B_h`, local camera/presentation state, ordered edit-attempt
history, aggregate tick outcomes, and local content tags. Its supplied interaction tapes are
replay inputs rather than proof that every live input was recorded. It does not implement
the proposed authorization, nonce, governance, validator, consensus, or availability
protocols. Resize, sidebar-toggle, and clear-motion actions also mutate local state when
accepted by the application, before `fixed_tick`; that local orchestration is not consensus
execution.

Protocol bootstrap uses distinct canonical producers. `TRANSITION-SPEC-D1` first freezes only the
base operation, result, and system-transition meanings plus the extension interface that scope
production is allowed to consume. `PROFILE-SCOPE-D0` then selects already designed schemas,
suites, roles, root domains, and optional features; `GENESIS-D1` supplies the concrete initial
logical state and the measured limits that passed for that scope; and `PROFILE-D1` emits the
candidate profile bytes. Only after that candidate has an identifier does `TRANSITION-SPEC-1`
resolve every active extension slot into the exact versioned `TRANSITION-SPEC-1@profile` artifact
and powered corpus consumed by `EXECUTOR-1@profile`. These are proposed future byte contracts and
producer phases, not implemented codecs.

To avoid self-reference, the exact pre-ID pipeline is:

```text
RequiredDomainV1 = Canonical(
    domain_tag,
    composition_layer,
    root_type,
    tree_domain,
    root_schema_id,
    commitment_profile_id
)

DomainRegistryEntryV1 =
    ActiveDomainV1(RequiredDomainV1)
  | InactiveDomainV1(domain_tag, composition_layer, inactive_sentinel_schema_id)

DomainRegistryV1 = CanonicalSortedUniqueByDomainTag([
    DomainRegistryEntryV1...
])

domain_registry_digest = ProfileIdHashV1(
    Domain("bitblock/domain-registry/v1")
 || LengthFrame(Canonical(DomainRegistryV1))
)

ProfileScopeV1 = Canonical(
    profile_scope_schema_version,
    base_transition_spec_id,
    activated_operation_and_system_transition_schema_ids,
    activated_state_schema_ids,
    DomainRegistryV1,
    identity_and_authorization_schema_ids,
    cryptographic_suite_ids,
    consensus_governance_and_network_role_policy_ids,
    optional_feature_ids,
    proposed_limit_vector
)

profile_scope_digest = ProfileIdHashV1(
    Domain("bitblock/profile-scope/v1")
 || LengthFrame(Canonical(ProfileScopeV1))
)

ChainGenesisPayloadV1 = Canonical(
    genesis_schema_version,
    initial_profile_scope_digest,
    initial_domain_registry_digest,
    chain_id,
    world_id,
    identity_namespace,
    B_0, M_0, E_0, X_0,
    A_0, N_0, P_0, G_0, Q_0,
    validated_limit_vector,
    bootstrap_and_availability_policy
)

genesis_id = ProfileIdHashV1(
    Domain("bitblock/chain-genesis/v1")
 || LengthFrame(Canonical(ChainGenesisPayloadV1))
)
GenesisPackageV1 = Canonical(ChainGenesisPayloadV1, genesis_id)
genesis_package = GenesisPackageV1  // pre-ID launch input only

ProfileLineageV1 =
    InitialProfileV1
  | InPlaceSuccessorProfileV1(predecessor_admitted_profile_id)

CandidateProfileV1 = Canonical(
    candidate_profile_schema_version,
    profile_scope_digest,
    domain_registry_digest,
    genesis_id,
    profile_lineage,
    protocol_version,
    canonical_scope_projection,
    canonical_chain_genesis_projection,
    declared_capability_closure,
    canonical_stage_gate_matrix,
    required_artifact_and_loss_condition_slots,
    candidate_release_configuration
)

profile_id = ProfileIdHashV1(
    Domain("bitblock/candidate-profile/v1")
 || LengthFrame(Canonical(CandidateProfileV1))
)
```

`ProfileScopeV1` owns feature, schema, suite, role, and active-or-inactive domain choices. The
domain registry is pre-ID: neither entry variant contains `profile_id`. Its independent
`domain_registry_digest` is projected into both later objects and MUST reproduce from the exact
registry inside the resolved scope. `ChainGenesisPayloadV1` owns the immutable chain/world identity,
the concrete launch state, the initial profile's scope/registry references, and the validated launch
limits. A proposed limit is not silently changed during validation: `validated_limit_vector` MUST
byte-equal the corresponding initial-scope projection, or the scope is regenerated and receives a
new digest before chain genesis is derived.

`ProfileLineageV1` makes launch and later activation different. `InitialProfileV1` requires the
candidate scope and registry to equal the genesis-owned initial scope and registry. An
`InPlaceSuccessorProfileV1` keeps the same immutable `genesis_id`, names one already admitted
predecessor from that chain, and MAY select a different scope or registry; it does not pretend that
the launch package was regenerated. Its eventual activation is separately authorized by the
migration transition defined below. A profile that names another `genesis_id` is a different chain,
not an in-place successor.

`CandidateProfileV1` owns admission metadata and references its current scope, immutable chain
genesis, and lineage. Its scope projection MUST byte-equal the current scope owner. Its chain-genesis
projection contains only the specified immutable chain/world/identity fields and MUST byte-equal the
launch package; a successor does not project its new scope back into genesis. Repeating any other
semantic value is forbidden. Missing, additional, or unequal projected values reject the candidate.

`ChainGenesisPayloadV1` excludes `genesis_id`, and `CandidateProfileV1` excludes `profile_id`. The
resulting `profile_id` identifies exact candidate bytes but cannot authorize a network merely by
existing. `PROFILE-1` admission must separately bind that identifier to the complete passing
capability/evidence closure. `RELEASE-R1` may separately bind a reproducible candidate build to the
same identifier; `RELEASE-1` can bind that candidate artifact to the admitted profile only after
admission. Candidate builds and admitted releases therefore remain different states even when they
use the same candidate `profile_id`.

After `profile_id` is derived, the profile-specific transition producer emits:

```text
TransitionSpecAtProfileV1 = Canonical(
    transition_spec_schema_version,
    profile_id,
    base_transition_spec_id,
    canonical_resolved_active_extension_spec_refs,
    canonical_validated_bound_projection,
    complete_operation_result_and_system_transition_table,
    powered_transition_corpus_digest
)

transition_spec_at_profile_id =
    ProfileIdHashV1(
        Domain("bitblock/transition-spec-at-profile/v1")
     || LengthFrame(Canonical(TransitionSpecAtProfileV1))
    )

ProfileAdmissionManifestV1 = Canonical(
    profile_admission_manifest_schema_version,
    admitted_profile_id,
    admitted_transition_spec_at_profile_id,
    admitted_profile_scope_digest,
    admitted_domain_registry_digest,
    admission_policy_id,
    canonical_sorted_unique_required_capability_ids,
    canonical_complete_evidence_closure_manifest,
    admission_disposition
)

profile_admission_manifest_id = ProfileIdHashV1(
    Domain("bitblock/profile-admission-manifest/v1")
 || LengthFrame(Canonical(ProfileAdmissionManifestV1))
)

AcceptedParentAuthorityUseV1 =
    EnvelopeParentAuthorityV1
  | SuccessorReleaseAuthorityV1
  | SuccessorActivationAuthorityV1

AcceptedParentAuthorityRefV1 = Canonical(
    accepted_parent_lookup_id,
    accepted_parent_authority_use,
    parent_authority_eligibility_policy_id,
    required_parent_authority_predicate_id
)

ParentAuthorityCertificateConsumerContextV1 =
    EnvelopeChildParentAuthorityConsumerContextV1(
        chain_launch_manifest_id,
        child_profile_transition_context,
        child_world_id,
        child_height,
        child_epoch,
        child_round_or_view,
        child_parent_block_id,
        child_consensus_policy_id
    )
  | SuccessorReleaseParentAuthorityConsumerContextV1(
        admitted_release_core_id,
        release_authority_context_ref,
        release_authority_policy_id
    )
  | SuccessorActivationParentAuthorityConsumerContextV1(
        chain_launch_manifest_id,
        predecessor_accepted_parent_authority_ref,
        old_active_profile_context,
        new_admitted_profile_context,
        successor_profile_admission_manifest_id,
        migration_spec_id,
        successor_activation_bundle_id,
        activation_height,
        activation_authority_policy_id
    )

parent_authority_certificate_consumer_context_id = EnvelopeHashV1(
    OrdinaryProfileTransitionV1(parent_profile_context),
    parent_authority_certificate_consumer_context_domain,
    ParentAuthorityCertificateConsumerContextV1
)

ConsensusNativeParentAuthorityPositionV1 = Canonical(
    chain_launch_manifest_id,
    authority_certificate_ledger_height_u64,
    authority_certificate_epoch_u64,
    authority_certificate_round_or_view,
    authority_certificate_intra_consensus_order_u64,
    authority_certificate_parent_history_root,
    authority_certificate_consensus_rule_id
)

ConsensusNativeParentAuthorityEvidenceCutoffPositionV1 = Canonical(
    chain_launch_manifest_id,
    evidence_cutoff_ledger_height_u64,
    evidence_cutoff_epoch_u64,
    evidence_cutoff_round_or_view,
    evidence_cutoff_intra_consensus_order_u64,
    evidence_cutoff_parent_history_root,
    evidence_cutoff_consensus_rule_id
)

ConsensusNativeAcceptedRecordOccurrencePositionV1 = Canonical(
    chain_launch_manifest_id,
    accepted_record_occurrence_ledger_height_u64,
    accepted_record_occurrence_epoch_u64,
    accepted_record_occurrence_round_or_view,
    accepted_record_occurrence_intra_consensus_order_u64,
    accepted_record_occurrence_parent_history_root,
    accepted_record_occurrence_consensus_rule_id
)

ConsensusNativeAcceptedRecordOccurrenceIndexKeyV1 = Canonical(
    accepted_parent_lookup_id,
    unique_accepted_proposal_core_block_id,
    ConsensusNativeAcceptedRecordOccurrencePositionV1
)

ConsensusNativeAcceptedRecordOccurrenceReceiptV1 = Canonical(
    accepted_record_occurrence_receipt_schema_version,
    accepted_parent_lookup_id,
    unique_accepted_proposal_core_block_id,
    ConsensusNativeAcceptedRecordOccurrencePositionV1,
    canonical_accepted_record_outer_length_u64,
    canonical_accepted_record_outer_bytes,
    typed_proposal_credential_id,
    ConsensusEvidenceIdV1,
    AvailabilityAssessmentIdV1
)

consensus_native_accepted_record_occurrence_receipt_id = EnvelopeHashV1(
    OrdinaryProfileTransitionV1(parent_profile_context),
    accepted_record_occurrence_receipt_domain,
    ConsensusNativeAcceptedRecordOccurrenceReceiptV1
)

ConsensusNativeAcceptedRecordOccurrenceReceiptSetV1 =
    CanonicalSortedUniqueByConsensusNativeAcceptedRecordOccurrenceIndexKey([
        Canonical(
            ConsensusNativeAcceptedRecordOccurrenceIndexKeyV1,
            ConsensusNativeAcceptedRecordOccurrenceReceiptV1,
            consensus_native_accepted_record_occurrence_receipt_id
        )...
    ])

consensus_native_accepted_record_occurrence_receipt_set_id = EnvelopeHashV1(
    OrdinaryProfileTransitionV1(parent_profile_context),
    accepted_record_occurrence_receipt_set_domain,
    ConsensusNativeAcceptedRecordOccurrenceReceiptSetV1
)

ConsensusNativeAcceptedRecordOccurrenceUniverseEntryV1 = Canonical(
    ConsensusNativeAcceptedRecordOccurrenceIndexKeyV1,
    consensus_native_accepted_record_occurrence_receipt_id
)

consensus_native_accepted_record_occurrence_universe_root =
    EnvelopeAuthenticatedOrderedMapRootV1(
        OrdinaryProfileTransitionV1(parent_profile_context),
        accepted_record_occurrence_universe_root_domain,
        accepted_record_occurrence_universe_entry_count_u64,
        CanonicalSortedUniqueByConsensusNativeAcceptedRecordOccurrenceIndexKey([
            ConsensusNativeAcceptedRecordOccurrenceUniverseEntryV1...
        ])
    )

ConsensusNativeParentAuthorityEvidenceCutoffV1 = Canonical(
    parent_authority_evidence_cutoff_schema_version,
    chain_launch_manifest_id,
    parent_profile_context,
    AcceptedParentAuthorityRefV1,
    unique_accepted_proposal_core_block_id,
    ConsensusNativeParentAuthorityPositionV1,
    accepted_record_occurrence_index_policy_id,
    ConsensusNativeParentAuthorityEvidenceCutoffPositionV1,
    accepted_record_occurrence_universe_entry_count_u64,
    consensus_native_accepted_record_occurrence_universe_root
)

consensus_native_parent_authority_evidence_cutoff_id = EnvelopeHashV1(
    OrdinaryProfileTransitionV1(parent_profile_context),
    parent_authority_evidence_cutoff_domain,
    ConsensusNativeParentAuthorityEvidenceCutoffV1
)

ConsensusNativeParentAuthorityEvidenceCutoffConsensusProofV1 = Canonical(
    ConsensusNativeParentAuthorityEvidenceCutoffV1,
    consensus_native_parent_authority_evidence_cutoff_id,
    canonical_evidence_cutoff_consensus_evidence_length_u64,
    canonical_evidence_cutoff_consensus_evidence_bytes,
    evidence_cutoff_consensus_evidence_id,
    canonical_occurrence_universe_root_state_inclusion_witness
)

ConsensusNativeAcceptedRecordOccurrenceUniverseScopeV1 = Canonical(
    accepted_parent_lookup_id,
    unique_accepted_proposal_core_block_id,
    consensus_history_start_position_inclusive,
    ConsensusNativeParentAuthorityEvidenceCutoffPositionV1
)

ConsensusNativeAcceptedRecordOccurrenceUniverseRangeCompletenessProofV1 = Canonical(
    consensus_native_parent_authority_evidence_cutoff_id,
    consensus_native_accepted_record_occurrence_universe_root,
    ConsensusNativeAcceptedRecordOccurrenceUniverseScopeV1,
    consensus_native_accepted_record_occurrence_receipt_set_id,
    canonical_authenticated_lower_boundary_exclusion_witness,
    canonical_authenticated_gap_free_range_witness,
    canonical_authenticated_upper_boundary_exclusion_witness
)

CompleteConsensusNativeAcceptedRecordOccurrenceUniverseV1 = Canonical(
    ConsensusNativeParentAuthorityEvidenceCutoffConsensusProofV1,
    ConsensusNativeAcceptedRecordOccurrenceUniverseScopeV1,
    ConsensusNativeAcceptedRecordOccurrenceReceiptSetV1,
    consensus_native_accepted_record_occurrence_receipt_set_id,
    ConsensusNativeAcceptedRecordOccurrenceUniverseRangeCompletenessProofV1
)

AcceptedParentAuthorityReceiptComponentProjectionV1 = Canonical(
    consensus_native_accepted_record_occurrence_receipt_id,
    proposal_credential_component = ProposalCredentialCommittedWitnessIdV1(
        TypedProposalCredentialIdV1
    ),
    consensus_evidence_component = AcceptedEvidenceCommittedWitnessIdV1(
        ConsensusEvidenceIdV1
    ),
    availability_assessment_component = AcceptedEvidenceCommittedWitnessIdV1(
        AvailabilityAssessmentIdV1
    ),
    availability_dependency_component = EvidenceDependencyCommittedWitnessIdV1(
        AcceptedEvidenceDependencyWitnessIdV1
    )
)

AcceptedParentAuthorityWitnessReceiptCoverageV1 = Canonical(
    CanonicalSortedUniqueByConsensusNativeAcceptedRecordOccurrenceReceiptId([
        AcceptedParentAuthorityReceiptComponentProjectionV1...
    ])
)

AcceptedParentAuthorityEvidenceCommitmentV1 = Canonical(
    CompleteConsensusNativeAcceptedRecordOccurrenceUniverseV1,
    canonical_qualifying_proposal_credential_witness_set,
    canonical_receipt_component_total_dependency_closed_evidence_set,
    canonical_dependency_closure_witness_manifest,
    AcceptedParentAuthorityWitnessReceiptCoverageV1
)

accepted_parent_authority_evidence_commitment_id = EnvelopeHashV1(
    OrdinaryProfileTransitionV1(parent_profile_context),
    parent_authority_evidence_commitment_domain,
    AcceptedParentAuthorityEvidenceCommitmentV1
)

ConsensusNativeParentAuthorityCertificateCoreV1 = Canonical(
    parent_authority_certificate_schema_version,
    parent_profile_context,
    AcceptedParentAuthorityRefV1,
    unique_accepted_proposal_core_block_id,
    ParentAuthorityCertificateConsumerContextV1,
    parent_authority_certificate_consumer_context_id,
    accepted_parent_authority_evidence_commitment_id,
    consensus_native_parent_authority_evidence_cutoff_id,
    consensus_native_accepted_record_occurrence_universe_root,
    attained_parent_authority_status,
    ConsensusNativeParentAuthorityPositionV1,
    parent_authority_certificate_policy_id
)

consensus_native_parent_authority_certificate_id = EnvelopeHashV1(
    OrdinaryProfileTransitionV1(parent_profile_context),
    parent_authority_certificate_domain,
    ConsensusNativeParentAuthorityCertificateCoreV1
)

ConsensusNativeParentAuthorityCertificateSignDocV1 = Canonical(
    consensus_native_parent_authority_certificate_id,
    ConsensusNativeParentAuthorityPositionV1,
    parent_authority_certificate_policy_id
)

ConsensusNativeParentAuthorityCertificateAuthorizationV1 = Canonical(
    ConsensusNativeParentAuthorityCertificateSignDocV1,
    parent_authority_certificate_authorization_scheme_id,
    parent_authority_certificate_credential_set_id,
    canonical_parent_authority_certificate_authorization_bytes
)

consensus_native_parent_authority_certificate_authorization_id = EnvelopeHashV1(
    OrdinaryProfileTransitionV1(parent_profile_context),
    parent_authority_certificate_authorization_domain,
    ConsensusNativeParentAuthorityCertificateAuthorizationV1
)

AuthorizedConsensusNativeParentAuthorityCertificateV1 = Canonical(
    ConsensusNativeParentAuthorityCertificateCoreV1,
    consensus_native_parent_authority_certificate_id,
    AcceptedParentAuthorityEvidenceCommitmentV1,
    ConsensusNativeParentAuthorityCertificateAuthorizationV1,
    consensus_native_parent_authority_certificate_authorization_id
)

ConsensusNativeParentAuthorityCertificateAdmissionCoreV1 = Canonical(
    consensus_native_parent_authority_certificate_id,
    consensus_native_parent_authority_certificate_authorization_id,
    parent_authority_certificate_consumer_context_id,
    accepted_parent_authority_evidence_commitment_id,
    consensus_native_parent_authority_evidence_cutoff_id,
    consensus_native_accepted_record_occurrence_universe_root,
    ConsensusNativeParentAuthorityPositionV1,
    certificate_consensus_policy_id,
    certificate_availability_policy_id,
    required_certificate_acceptance_status
)

consensus_native_parent_authority_certificate_admission_core_id = EnvelopeHashV1(
    OrdinaryProfileTransitionV1(parent_profile_context),
    parent_authority_certificate_admission_core_domain,
    ConsensusNativeParentAuthorityCertificateAdmissionCoreV1
)

ConsensusNativeParentAuthorityCertificateAdmissionV1 = Canonical(
    AuthorizedConsensusNativeParentAuthorityCertificateV1,
    ConsensusNativeParentAuthorityCertificateAdmissionCoreV1,
    consensus_native_parent_authority_certificate_admission_core_id,
    canonical_certificate_consensus_evidence_length_u64,
    canonical_certificate_consensus_evidence_bytes,
    certificate_consensus_evidence_id,
    canonical_certificate_availability_assessment_length_u64,
    canonical_certificate_availability_assessment_bytes,
    certificate_availability_assessment_id,
    attained_certificate_acceptance_status
)

consensus_native_parent_authority_certificate_admission_id = EnvelopeHashV1(
    OrdinaryProfileTransitionV1(parent_profile_context),
    parent_authority_certificate_admission_domain,
    ConsensusNativeParentAuthorityCertificateAdmissionV1
)

AcceptedConsensusNativeParentAuthorityCertificateV1 = Canonical(
    ConsensusNativeParentAuthorityCertificateAdmissionV1,
    consensus_native_parent_authority_certificate_admission_id
)

ConsensusNativeParentAuthorityCertificateRefV1 = Canonical(
    consensus_native_parent_authority_certificate_id,
    consensus_native_parent_authority_certificate_authorization_id,
    consensus_native_parent_authority_certificate_admission_id,
    parent_authority_certificate_consumer_context_id,
    accepted_parent_authority_evidence_commitment_id,
    consensus_native_parent_authority_evidence_cutoff_id,
    consensus_native_accepted_record_occurrence_universe_root,
    ConsensusNativeParentAuthorityPositionV1
)

CertificateEvidenceReplayCheckpointRefV1 = Canonical(
    replay_evidence_archive_id,
    replay_evidence_archive_checkpoint_id,
    replay_evidence_archive_checkpoint_authorization_id,
    archive_reconstruction_cutoff_event_ordinal_u64,
    ordered_event_prefix_root,
    evidence_normalization_policy_id
)

HistoricalAcceptedParentAuthorityUseRefV1 = Canonical(
    AcceptedParentAuthorityRefV1,
    unique_accepted_proposal_core_block_id,
    ConsensusNativeParentAuthorityCertificateRefV1,
    attained_parent_authority_status
)

EnvelopeHistoricalAuthorityRefV1 =
    InitialLaunchHistoricalAuthorityRefV1(chain_launch_manifest_id)
  | AcceptedParentEnvelopeHistoricalAuthorityRefV1(
        HistoricalAcceptedParentAuthorityUseRefV1
    )

ReleaseHistoricalAuthorityRefV1 =
    GenesisReleaseHistoricalAuthorityRefV1(
        genesis_id,
        genesis_release_authority_record_id
    )
  | AcceptedParentReleaseHistoricalAuthorityRefV1(
        HistoricalAcceptedParentAuthorityUseRefV1
    )

ReleaseAuthorityContextRefV1 =
    GenesisReleaseAuthorityV1(
        genesis_id,
        genesis_release_authority_record_id
    )
  | AcceptedParentReleaseAuthorityV1(
        predecessor_accepted_parent_authority_ref,
        predecessor_governance_authority_root
    )

AdmittedReleaseCoreV1 = Canonical(
    admitted_release_core_schema_version,
    admitted_profile_id,
    admitted_transition_spec_at_profile_id,
    profile_admission_manifest_id,
    candidate_release_configuration,
    exact_source_and_build_recipe_digests,
    exact_feature_toolchain_dependency_and_binary_digests,
    exact_genesis_and_runtime_configuration_digests,
    release_authority_context_ref,
    release_authority_policy_id
)

admitted_release_core_id = ProfileIdHashV1(
    Domain("bitblock/admitted-release-core/v1")
 || LengthFrame(Canonical(AdmittedReleaseCoreV1))
)

ReleaseSignDocV1 = Canonical(
    release_sign_doc_schema_version,
    admitted_release_core_id,
    release_authority_context_ref,
    release_historical_authority_ref,
    release_authority_policy_id
)

release_sign_doc_id = ProfileIdHashV1(
    Domain("bitblock/release-sign-doc/v1")
 || LengthFrame(Canonical(ReleaseSignDocV1))
)

ReleaseAuthorizationV1 = Canonical(
    release_authorization_schema_version,
    ReleaseSignDocV1,
    release_sign_doc_id,
    release_authorization_scheme_id,
    release_authority_credential_set_id,
    canonical_release_authorization_bytes
)

AdmittedReleaseRecordV1 = Canonical(
    admitted_release_record_schema_version,
    AdmittedReleaseCoreV1,
    admitted_release_core_id,
    ReleaseAuthorizationV1
)

admitted_release_record_id = ProfileIdHashV1(
    Domain("bitblock/admitted-release-record/v1")
 || LengthFrame(Canonical(AdmittedReleaseRecordV1))
)

GenesisRootParityRecordV1 = Canonical(
    genesis_root_parity_record_schema_version,
    genesis_id,
    initial_profile_id,
    initial_transition_spec_at_profile_id,
    required_domain_set_digest,
    canonical_initial_root_entry_set_digest,
    initial_application_root,
    initial_execution_root_ref,
    genesis_history_root,
    independent_reconstruction_artifact_digests
)

genesis_root_parity_record_id = ProfileIdHashV1(
    Domain("bitblock/genesis-root-parity-record/v1")
 || LengthFrame(Canonical(GenesisRootParityRecordV1))
)

ChainLaunchManifestV1 = Canonical(
    chain_launch_manifest_schema_version,
    genesis_id,
    initial_profile_id,
    initial_transition_spec_at_profile_id,
    initial_profile_admission_manifest_id,
    genesis_root_parity_record_id,
    initial_execution_root_ref,
    initial_release_record_id,
    launch_closure_bundle_id,
    launch_authority_policy_id,
    genesis_parent_block_id,
    genesis_history_root,
    first_ledger_height
)

chain_launch_manifest_id = ProfileIdHashV1(
    Domain("bitblock/chain-launch-manifest/v1")
 || LengthFrame(Canonical(ChainLaunchManifestV1))
)

ChainLaunchAuthorizationV1 = Canonical(
    chain_launch_manifest_id,
    launch_authority_policy_id,
    launch_authorization_scheme_id,
    canonical_launch_authorization_bytes
)

SupplementaryLaunchArtifactV1 = Canonical(
    typed_artifact_schema_id,
    typed_artifact_id,
    canonical_artifact_preimage_length_u64,
    canonical_artifact_preimage_bytes
)

LaunchClosureInternalDependencyNodeV1 = Canonical(
    topological_rank_u32,
    typed_artifact_id,
    canonical_sorted_unique_dependency_refs
)

ExternalGenesisIncomingDependencyClassV1 =
    InitialCandidateGenesisIdentityDependencyV1
  | GenesisStateDerivationDependencyV1
  | GenesisReleaseAuthorityDependencyV1

ExternalGenesisOutgoingDependencyClassV1 =
    GenesisInitialScopeDependencyV1
  | GenesisInitialDomainRegistryDependencyV1

LaunchClosureDependencyRefV1 =
    InternalLaunchArtifactDependencyV1(typed_artifact_id)
  | ExternalGenesisRootDependencyV1(
        genesis_id,
        ExternalGenesisIncomingDependencyClassV1
    )

ExternalGenesisToInternalDependencyRefV1 = Canonical(
    typed_internal_artifact_id,
    ExternalGenesisOutgoingDependencyClassV1
)

LaunchClosureExternalGenesisDependencyNodeV1 = Canonical(
    external_root_schema_id,
    external_root_topological_rank_u32,
    genesis_id,
    canonical_outer_genesis_package_length_u64,
    CanonicalSortedUniqueByTypedArtifactId([
        ExternalGenesisToInternalDependencyRefV1...
    ])
)

LaunchClosureBundleV1 = Canonical(
    launch_closure_bundle_schema_version,
    internal_artifact_count_u32,
    external_root_count_u32,
    complete_artifact_count_u32,
    supplementary_artifact_count_u32,
    internal_to_internal_dependency_edge_count_u32,
    internal_to_external_dependency_edge_count_u32,
    external_to_internal_dependency_edge_count_u32,
    complete_dependency_edge_count_u32,
    internal_canonical_preimage_bytes_u64,
    external_root_canonical_preimage_bytes_u64,
    total_canonical_preimage_bytes_u64,
    LaunchClosureExternalGenesisDependencyNodeV1,
    explicit_preimage_lengths_u64[
        profile_scope,
        domain_registry,
        candidate_profile,
        transition_spec_at_profile,
        profile_admission_manifest,
        genesis_root_parity_record,
        admitted_release_record
    ],
    ProfileScopeV1,
    DomainRegistryV1,
    CandidateProfileV1,
    TransitionSpecAtProfileV1,
    ProfileAdmissionManifestV1,
    GenesisRootParityRecordV1,
    AdmittedReleaseRecordV1,
    CanonicalSortedUniqueByTypedArtifactId([
        LaunchClosureInternalDependencyNodeV1...
    ]),
    CanonicalSortedUniqueByTypedArtifactId([
        SupplementaryLaunchArtifactV1...
    ])
)

launch_closure_bundle_id = ProfileIdHashV1(
    Domain("bitblock/launch-closure-bundle/v1")
 || LengthFrame(Canonical(LaunchClosureBundleV1))
)

chain_launch_package = Canonical(
    genesis_package,
    LaunchClosureBundleV1,
    launch_closure_bundle_id,
    ChainLaunchManifestV1,
    chain_launch_manifest_id,
    ChainLaunchAuthorizationV1
)
```

`EnvelopeAuthenticatedOrderedMapRootV1` denotes the exact profile-owned ordered-map commitment
algorithm admitted under `accepted_record_occurrence_index_policy_id`; it is not a primitive in the
current Rust system. Its future specification MUST domain-bind the profile and entry count, reject
duplicate/noncanonical keys, commit every key and receipt ID, and define the verifier for the exact
range proof above. A concrete hash-tree, Verkle, or other commitment cannot occupy this slot until its
codec, proof algorithm, resource bounds, implementation closure, and migration behavior are admitted.

`ProfileAdmissionManifestV1` is the exact canonical `PROFILE-1` output, not a boolean supplied by
the candidate. `admission_disposition` MUST be the one passing disposition under the named policy,
and its evidence-closure manifest MUST name every artifact consumed to derive that disposition.
`AdmittedReleaseRecordV1` is the exact `RELEASE-1` output, not a `RELEASE-R1` candidate-build
receipt. Its profile, specification, admission, candidate-release configuration, source, feature,
toolchain, dependency, binary, genesis, and runtime-configuration projections MUST byte-equal their
already identified owners. `AdmittedReleaseCoreV1` contains no authorization bytes. Its fixed
`admitted_release_core_id` is derived first, and the detached authorization covers exactly
`Domain("bitblock/release-authorization-sign-doc/v1") ||
LengthFrame(Canonical(ReleaseSignDocV1))`; the sign document in turn names that exact core ID,
authority context, exact `release_historical_authority_ref`, and its own recomputed
`release_sign_doc_id`. `ReleaseAuthorizationV1` may carry a signature, threshold proof, certificate,
or another policy-frozen form, but its codec, scheme, credential set, and verification rule MUST
come from `ResolveTrustedReleaseAuthorityV1(release_authority_context_ref)`. For an initial release,
the historical ref MUST be `GenesisReleaseHistoricalAuthorityRefV1` and the resolver consumes the
release-authority record already committed in `G_0`. For a successor it MUST instead be
`AcceptedParentReleaseHistoricalAuthorityRefV1`, whose embedded
`HistoricalAcceptedParentAuthorityUseRefV1` names an already accepted consensus-native parent-
authority certificate, its authorization-dependent ID, evidence commitment, authenticated evidence
cutoff and occurrence-universe root, ledger/consensus position, release-purpose parent ref, and
attained status. The selected consensus profile, not the
replay archive, determines which certificate is accepted at that position.
That successor resolver consumes the exact governance authority root reached through the purpose-tagged
`VerifiedEligibleAcceptedParentRefV1` derived from
`predecessor_accepted_parent_authority_ref`; that ref MUST use
`SuccessorReleaseAuthorityV1`. A release cannot substitute a raw lookup ID, weaker envelope-parent
predicate, particular evidence wrapper, or the distinct activation-purpose result for that semantic
authority. During successor verification it MUST byte-equal `release_parent_authority_ref` and
`verified_release_parent_eligibility` in `VerifiedSuccessorPredecessorBaseContextV1`. Once
`admitted_release_core_id` exists, the release certificate's exact pre-signing context and historical
ref MUST byte-equal `VerifiedSuccessorReleaseAuthorityContextV1`; neither value is part of the
pre-bundle base context.
The policy ID in the core is a required byte-equal projection of that resolver, never a selector for
it. Only after the consensus-native certificate/ref and release authorization verify is
`admitted_release_record_id` derived from the core, core ID, and detached authorization. Neither
authorization signs the final record ID, and neither
record contains the later launch-manifest or closure-bundle ID, so no identifier cycle is possible.
All release IDs shown here use protocol-frozen `ProfileIdHashV1` because a fresh launch or successor
verifier must identify them before resolving the admitted profile's object hash.

`LaunchClosureBundleV1` is an **embedded-only** bounded retrieval closure for a fresh verifier. Its
seven explicit preimages are mandatory even where one repeats another projection: the separate
registry MUST byte-equal the registry embedded by the scope, and every typed ID or digest is
independently recomputed. The supplementary inventory embeds every other canonical preimage
transitively referenced by the admission or release records and required to verify them. There is no
locator, content-address-only, optional-fetch, or ambient-database variant in this schema. A locator
tag or an omitted/unavailable preimage is an unknown variant and rejects rather than masquerading as
a complete carrier. The sole exception to internal embedding is the one `genesis_package` already
carried immediately outside the bundle by `chain_launch_package`; it is represented inside the
bundle graph by exactly one typed `LaunchClosureExternalGenesisDependencyNodeV1`, never by a second
copy of its bytes. Unlike a root-only descriptor, that node carries the genesis artifact's actual
outgoing dependency references while its preimage remains solely owned by the outer package.

Let `E` be the ordered seven-element tuple of the exact canonical preimage byte strings named by
`explicit_preimage_lengths_u64`, let `S` be the supplementary-artifact sequence, let `D` be the
internal dependency-node sequence, let `X` be the one external genesis dependency node, and let `G`
be the exact canonical byte string of the outer `genesis_package`. Let `InternalRefs(D)` and
`ExternalGenesisRefs(D)` partition every dependency reference in `D` by its tag, and let
`GenesisInternalRefs(X)` be every external-to-internal dependency ref carried by `X`. The checked
equations are:

```text
Length(E)                              = 7
supplementary_artifact_count_u32       = Length(S)
internal_artifact_count_u32             = 7 + Length(S) = Length(D)
external_root_count_u32                 = 1
complete_artifact_count_u32             = internal_artifact_count_u32
                                          + external_root_count_u32
                                        = 8 + Length(S) = Length(D) + 1
internal_to_internal_dependency_edge_count_u32 = Length(InternalRefs(D))
internal_to_external_dependency_edge_count_u32 = Length(ExternalGenesisRefs(D))
external_to_internal_dependency_edge_count_u32 = Length(GenesisInternalRefs(X))
complete_dependency_edge_count_u32     =
    internal_to_internal_dependency_edge_count_u32
  + internal_to_external_dependency_edge_count_u32
  + external_to_internal_dependency_edge_count_u32
internal_canonical_preimage_bytes_u64   =
    SumChecked(e in E, Length(e))
  + SumChecked(s in S, Length(s.canonical_artifact_preimage_bytes))
external_root_canonical_preimage_bytes_u64 = Length(G)
total_canonical_preimage_bytes_u64      =
    internal_canonical_preimage_bytes_u64
  + external_root_canonical_preimage_bytes_u64
explicit_preimage_lengths_u64[i]       = Length(E[i])              for every i
s.canonical_artifact_preimage_length_u64 =
    Length(s.canonical_artifact_preimage_bytes)                    for every s
LaunchClosureExternalGenesisDependencyNodeV1.external_root_schema_id
                                        = GenesisPackageV1
LaunchClosureExternalGenesisDependencyNodeV1.genesis_id = genesis_package.genesis_id
LaunchClosureExternalGenesisDependencyNodeV1.canonical_outer_genesis_package_length_u64
                                        = Length(G)
Length(GenesisInternalRefs(X))           = 2
```

Every sum and conversion is checked before use. A top-level carried artifact contributes exactly
once to the artifact count and once to the byte sum. Bytes embedded inside one canonical preimage
are already included in that preimage's length and are not recursively counted again. Where the
same semantic registry is deliberately carried both inside `ProfileScopeV1` and as the separate
`DomainRegistryV1` preimage, its bytes occur once in each of those two top-level preimages and no
third time. `G` is counted once as the external root even though its bytes are physically owned only
by the outer package. The bundle MUST require its external-root length and `genesis_id` to match that
outer byte string exactly, MUST bounded-decode that exact outer `GenesisPackageV1`, and MUST
recompute the ID from its outer `ChainGenesisPayloadV1`; an in-bundle duplicate is forbidden. Bundle
schema/counter/length fields, the external-genesis dependency node, dependency-node bytes,
collection framing, `launch_closure_bundle_id`, `ChainLaunchManifestV1`, and launch authorization
are excluded from `total_canonical_preimage_bytes_u64`; their bytes remain covered by the independent
protocol-frozen total package/frame cap, avoiding a self-referential total.

The protocol-frozen bundle grammar places the fixed-width count/total prefix and the seven explicit
lengths before their variable slices; each supplementary entry likewise places its declared length
before its bytes. A verifier first streaming-scans those length/count prefixes, uses checked
arithmetic to enforce the exact equations plus launch count/byte/nesting/depth bounds, and confirms
that the declared slices consume the exact enclosing frame. It does so without allocation, copying,
hashing, nested decode, or profile-owned work. Only then may it materialize and verify artifacts.
Every explicit and supplementary artifact occurs in exactly one
`LaunchClosureInternalDependencyNodeV1`; each typed internal artifact ID occurs exactly once.
Together those nodes and `X` form one combined dependency DAG. Every dependency target—internal or
external—MUST occur exactly once in that combined node set and have a strictly lower checked
topological rank than its referrer. The external genesis node is therefore not rank zero: because
the actual `ChainGenesisPayloadV1` names both `initial_profile_scope_digest` and
`initial_domain_registry_digest`, `X` MUST carry exactly one `GenesisInitialScopeDependencyV1` ref
to the carried `ProfileScopeV1` and exactly one `GenesisInitialDomainRegistryDependencyV1` ref to
the carried `DomainRegistryV1`, with byte-equal digests. No other external-to-internal class is
legal. One concrete valid prefix is `DomainRegistryV1` at rank 0, `ProfileScopeV1` at rank 1, `X`
at rank 2, and `CandidateProfileV1` at rank 3 or higher; later nodes take ranks above every
dependency they name.

The only permitted internal-to-external classes are: initial-candidate genesis identity from
`CandidateProfileV1`; genesis-state derivation from `GenesisRootParityRecordV1` or an exact
reconstruction artifact named by that record; and genesis release authority from
`AdmittedReleaseRecordV1` or the exact `G_0` authority artifact named by that record. The direct
`CandidateProfileV1 -> X` identity edge is mandatory and its `genesis_id` MUST equal both candidate
and outer genesis bytes. Every other direct external edge is required exactly when its canonical
preimage directly names that genesis role. `ProfileScopeV1` and `DomainRegistryV1` do not point to
genesis: reversing either actual genesis-to-scope/registry edge rejects. `TransitionSpecAtProfileV1`
and `ProfileAdmissionManifestV1` reach genesis only through their declared internal dependencies and
MUST NOT invent a direct external edge. Every permitted direct genesis dependency must carry its
matching class; a class on any other referrer rejects. Any second external root, other external
dependency, duplicated edge, dependency to an unranked node, outer/root byte or ID mismatch,
rank-zero genesis, duplicate internal ID or semantic role, self/forward reference, unknown schema,
unreachable extra, missing reference, or equation mismatch rejects before profile resolution. Thus
the combined graph is finite and acyclic; a content hash does not excuse an undeclared local lookup.

Powered launch-carrier controls increment and decrement each count and declared length, overflow
each checked sum, double-count an embedded registry, alter a dependency edge total, truncate or
append a slice, omit one preimage or required genesis edge, inject a second or differently typed
external root, reverse either genesis-to-scope/registry edge, omit one external-genesis outgoing
ref, force the genesis node to rank zero, mismatch the outer/root bytes or ID, and replace an
embedded preimage with an unavailable locator. The positive control realizes the concrete
registry-0, scope-1, genesis-2, candidate-3 ranking. The corpus mutates internal-to-internal,
internal-to-external, and external-to-internal edge counts independently as well as their checked
combined total. Every mutant rejects during preflight or closure verification before profile
resolution.

`TransitionSpecAtProfileV1` is an admission prerequisite, not an input to `CandidateProfileV1`;
putting its digest back into the candidate preimage would create a candidate/specification
identifier cycle.
Every bound or active-schema value it repeats MUST be the canonical byte-equal projection of its
scope/genesis/candidate owner. `PROFILE-1` verifies that the produced specification is the
deterministic resolution of the candidate's declared scope and closure.

`genesis_package` is deliberately only the immutable pre-ID input. It is not, by itself, a complete
network launch trust anchor. After the initial candidate ID, transition specification, `PROFILE-1`
admission-manifest ID, genesis-root-parity result, and `RELEASE-1` record exist, their exact
identities and their complete retrieval bundle are bound by `ChainLaunchManifestV1`.
`launch_closure_bundle_id` MUST reproduce from the exact bundle carried in `chain_launch_package`;
every manifest reference MUST resolve to the one byte-exact corresponding bundle preimage.
`initial_execution_root_ref` is the typed
`ExecutionRootRefV1` defined in §4.4 and MUST contain the initial admitted profile context;
`GenesisRootParityRecordV1` and the manifest MUST carry the same byte-exact reference and history
root. `initial_release_record_id` identifies the admitted-profile signed release, not a
`RELEASE-R1` candidate build. That referenced `RELEASE-1` record MUST be derived without
`chain_launch_manifest_id`; otherwise the release and manifest would form a forbidden identifier
cycle. `launch_authority_policy_id` MUST be the byte-exact projection of the policy already
committed in `G_0`, not a policy selected by the candidate or manifest.

The manifest excludes its own identifier and all future block identifiers. Its authorization is a
detached envelope over the already-derived `chain_launch_manifest_id`, verified under the launch
authority policy committed in `G_0`; neither the initial candidate nor its selected cryptographic
suites may choose that authority. The canonical derivation is therefore:

```text
genesis_package
  -> exact DomainRegistryV1 + ProfileScopeV1
  -> initial CandidateProfileV1 / profile_id
  -> transition_spec_at_profile_id
  -> PROFILE-1 admission + genesis root parity + RELEASE-1
  -> LaunchClosureBundleV1 / launch_closure_bundle_id
  -> ChainLaunchManifestV1 / chain_launch_manifest_id
  -> ChainLaunchAuthorizationV1
  -> first AcceptedBlockRecord
```

The first accepted record MUST name `chain_launch_manifest_id`; use the manifest's initial profile
as an ordinary profile context; use its exact initial execution-root reference as the pre-state;
and match its `genesis_parent_block_id`, `genesis_history_root`, and `first_ledger_height`. Every
later record retains the same launch-manifest identifier. A different initial candidate,
transition-specification ID, admission-manifest ID, parity record, release record, closure bundle,
or initial typed root
is a different launch trust anchor even when `genesis_id` is unchanged.

A fresh launch verifier first applies the protocol-frozen launch-package count, byte, nesting, and
preallocation caps. It then recomputes, in this order, the domain-registry digest; scope digest and
its byte-identical registry projection; `genesis_id`; candidate `profile_id` and every
scope/genesis/lineage projection; transition-specification ID and deterministic candidate
resolution; admission-manifest ID and complete passing evidence closure; parity-record ID and every
initial typed root; admitted-release core and core ID; trusted `G_0` release-authority resolution;
exact release sign document and detached authorization; admitted-release-record ID;
closure-bundle ID; manifest ID and all its projections; and finally the detached launch
authorization under the policy committed in `G_0`. Bootstrap and replay MUST complete this whole
chain before resolving the first envelope.
No hash, signature, codec, artifact, policy, or ID found only in an ambient local map can satisfy
that verification.

The launch corpus requires one complete valid package plus powered removal, substitution,
duplicate-role, wrong-ID, wrong-projection, unreachable-extra, over-count/bytes/depth, missing
dependency, and rank-cycle mutants. Each mutant must fail at its named closure step before the first
profile codec is used.

`ProfileIdHashV1` is a protocol-level identity function frozen before any candidate scope or
candidate-selected cryptographic suite is interpreted. Every pre-admission identifier above uses
the exact `Domain(...) || LengthFrame(Canonical(...))` framing shown. The
`cryptographic_suite_ids` inside `ProfileScopeV1` govern only profile-resolved post-admission
operations; they cannot select or alter `ProfileIdHashV1`. Bare `H` below denotes such a separately
resolved post-admission object hash and MUST NOT be substituted into this pre-admission chain.

This creates three explicitly different conditions. `PreventiveProfileOwnedCryptographicMigrationV1`
may replace an admitted profile's still-sound but deprecated object-hash, proposal/vote-signature,
proof, state-commitment, governance/release credential, or other profile-owned suite through the
parent-authorized in-place successor path. It is permitted only while the verifier can identify one
exact last-safe accepted parent and independently verify that every live parent lookup, proposal,
consensus, availability, replay, activation, state/proof, and historical-verification binding needed
to authorize and check the transition remains sound and unambiguous. It requires downgrade
protection, any declared dual-validation window, and preserved historical verification. It MUST NOT
claim to replace or recover `ProfileIdHashV1`.

`ProfileOwnedAuthorityBindingCompromiseV1` is instead an established collision, forgery, or
equivalent loss of unique authority in any live profile-owned role. A valid pair of different
signature-free proposal cores under one recomputed lookup, a forged proposal or vote accepted by the
active verifier, an ambiguous governance/release credential, or a proof/state binding that admits
different authority-bearing meanings is this condition, not a preventive upgrade opportunity. Nodes
MUST stop proposing, voting, authorizing releases or activations, and publishing new accepted
indexes. A parent, vote set, migration envelope, governance state, or recovery assertion selected
only through that failed binding cannot non-circularly choose its own repair. The current protocol's
only inhabited disposition is exact:

```text
CurrentEstablishedAuthorityCompromiseDispositionV1 =
    SafeHaltAndSeparatelyTrustedNewGenesisV1

PrecommittedIndependentRecoveryEnvelopeV1 = ReservedFutureUninhabitedTypeV1
```

`PrecommittedIndependentRecoveryEnvelopeV1` has no current constructor, canonical bytes, identifier,
acceptance branch, or positive vector. An established `ProfileOwnedAuthorityBindingCompromiseV1`
therefore always safe-halts and requires a new genesis under a separately trusted recovery anchor in
this protocol version.

`FutureIndependentRecoveryExtensionV1` is only a conditional design label. Before a later protocol
may inhabit the reserved type or add a positive in-place-recovery control, it MUST separately freeze
the exact envelope schema and ID; an independent pre-genesis commitment and authority; a domain-
separated signed statement and credential resolver; an unambiguous last-safe-parent selection rule;
the complete bounded transition bytes; availability and publication/crash semantics; independence
from every failed role; historical verification; and powered matched/mutant tests. None of those
requirements is satisfied by naming the type, adding an anchor after compromise, or using ordinary
parent/governance authorization.

A demonstrated collision, practical second-preimage break, or equivalent loss of identity binding
in `ProfileIdHashV1` is the still stronger `ProtocolIdentityHashCompromiseV1`. A candidate, release,
launch object, lookup ref, or successor identified or authorized through the compromised function
cannot non-circularly repair that function.

This paper defines no separately trusted transition precommitted before launch. Its disposition for
`ProtocolIdentityHashCompromiseV1` is therefore terminal safe halt for the affected chain and a new
genesis under a separately distributed recovery trust anchor and independently selected protocol-
identity function. The replacement is a new protocol generation, not an in-place successor and not
a continuation proved by a `ProfileIdHashV1` link. A future design may precommit a non-self-selecting
recovery authority and encoding, independent of `ProfileIdHashV1`, to coordinate that new genesis;
it still cannot turn the compromised identity chain into a proved in-place continuation. Adding an
anchor only after compromise is not recovery evidence.

Powered controls MUST distinguish all three conditions. Preventive controls deprecate, one at a
time, a still-sound profile object hash, proposal/vote signature, governance/release credential, and
proof/state suite and accept only the fully authorized successor migration while the exact last-safe
parent and every authority binding remain independently sound. Compromise controls supply a genuine
distinct unsigned-core collision, accepted proposal or vote forgery, ambiguous governance/release
credential, and proof/state collision under an active profile and MUST reach
`ProfileOwnedAuthorityBindingCompromiseV1`. A matched control with a recovery envelope precommitted
under an actually independent anchor is not a current vector because the reserved envelope is
uninhabited. Every current established-compromise control—including no-precommit, hypothetical
precommit, post-compromise-precommit, failed-role-authorized, and ambiguous-last-safe-parent cases—
MUST produce `CurrentEstablishedAuthorityCompromiseDispositionV1` and require new genesis. A future
positive control becomes admissible only in the separately specified and activated
`FutureIndependentRecoveryExtensionV1`; it cannot be fabricated to satisfy this gate. The protocol-
identity control supplies colliding or
second-preimage-distinct launch, release, lookup, and successor objects under one
`ProfileIdHashV1` output and MUST reach `ProtocolIdentityHashCompromiseV1`; it must never select one
object, activate a successor, reuse the old chain identity, or report an in-place migration pass.

Chain genesis has two separate launch gates for `InitialProfileV1`. **Logical-byte completion**
under `GENESIS-D1` means independent encoders reconstruct the same initial-scope-bound concrete
state and validated limits. It does not require or certify an implementation root.
**Genesis-root parity** is later work under `COMMIT-1`, `GENESIS-ROOT-PARITY-1`, and
`EXECUTION-ROOT-1`: independent implementations rebuild every initial-profile-required domain from
those logical bytes and agree with the published initial root tuple. A root mismatch does not
rewrite genesis bytes or select whichever implementation looked canonical; it keeps the initial
profile locked. A successor profile uses migration parity from an accepted old root, not a
reinterpretation of this launch gate.

For the first proposed network profile, one chain contains exactly one `world_id`; a future
multi-world version would replace the single world with a committed `worlds_root`. Initially
`support(B_0) = {o}`, `M_0` and `E_0` are empty, and every allocation, authority, parameter,
validator, and governance record is explicit. The package MUST be immutable and reproducible. For
this paper's provisional non-economic first-profile sketch, origin bedrock is an application
invariant rather than an occupancy-episode token, observer property, or validator trust anchor.
That is not a final asset ruling: `ASSET-D1` may instead define a separate permanently locked
genesis allocation outside mutable occupancy episodes, and the selected genesis profile must make
the choice explicit.

The authoritative execution transition is a pure, total function over the exact committed body:

```text
(ExecutionState_(h+1), OrderedEffectsAndReceipts_(h+1)) = Execute(
    ExecutionState_h,
    BlockContext_(h+1),
    Body_(h+1)
)
```

`ExecutionState_h` is the authority-selected pre-state: the launch-authorized initial execution
state for the first record, or the accepted parent's post-state at the profile's named consensus
status thereafter. The candidate successor has height `h+1`. `BlockContext_(h+1)` contains only
committed inputs required by the selected version, such as height, parent identifier, and any
authenticated protocol randomness. Execution MUST depend only on that canonical pre-state,
committed context, and exact canonical body bytes. Proposal,
validation, voting, certification, and publication are separate transitions. Wall-clock
timing MAY schedule local work and consensus timeouts, but MUST NOT change deterministic
execution of a given candidate body.

Consensus height is independent of the current 60 Hz simulation tick. A version MAY commit
a deterministic number of application sub-ticks per block, or omit application sub-ticks
and execute only ordered transactions. Any mapping is explicit in `P_h`; a validator MUST
NOT infer consensus height or finality from elapsed time, a local window, or a frame render.

`Execute` has exactly the two total branches defined in §4.4. The ordinary branch consumes the
ordered signed transactions plus the ordinary body's deterministic auxiliary inputs under the
context's schedules. The migration branch consumes the dedicated migration body's deterministic
migration inputs and rejects unless the signed-transaction and application-subtick sequences are
empty and the context contains exactly the one authorized migration system transition. Every
deterministic input has one canonical owner: envelope identity, height, committed randomness, and
schedule/ordinal values live in `BlockContext`; transaction and auxiliary value bytes live only in
`OrdinaryBodyV1`; migration value bytes live only in `DedicatedMigrationBodyV1`. A schedule may
refer to a body input by position but MUST NOT repeat its value. Missing, duplicated, unconsumed, or
wrong-branch inputs make the whole block invalid rather than selecting an implementation default.

### 2.1 Occupancy rules

For a mutable coordinate `c`:

```text
off -> on   = open one mint episode
on  -> off   = consume one live mint episode
on  -> on   = idempotent no-op
off -> off  = idempotent no-op
origin edit = protected no-op
```

If the same coordinate is deleted and later placed again, the later placement MUST receive
a new token identifier. The protocol must choose whether identity is transaction-stable or
inclusion-stable across provisional forks. One non-circular inclusion-stable candidate is:

```text
UserEventPosition = Canonical(
    user_event_position_domain,
    candidate_execution_context_id,
    transaction_index,
    action_index,
    effect_index
)

SystemEventPosition = Canonical(
    system_event_position_domain,
    candidate_execution_context_id,
    system_phase,
    system_rule_id,
    system_transition_ordinal,
    effect_index
)

event_position = H(Canonical(UserEventPosition | SystemEventPosition))

mint_id = H(
    Canonical(mint_domain, event_position, coordinate)
)

burn_event_id = H(Canonical(
    burn_domain,
    event_position,
    coordinate,
    consumed_mint_id
))
```

`candidate_execution_context_id` is defined in §4.4 from the verified parent-or-launch authority,
exact body, committed block context, and execution policy before execution begins. Both tagged position variants
are therefore known before post-state and `block_id` computation and create no hash cycle.
The system namespace covers rent, reward, migration, or governance
transitions that are not user transaction actions; its ordinals and phases are deterministic
protocol rules. Re-inclusion under another parent creates another user event position. A transaction-stable
alternative would deliberately preserve identity across such re-inclusion and must define
the collision and duplicate-inclusion rules. The final choice MUST include an unambiguous
event identity or equivalent canonical nonce; a coordinate alone is never a token ID.
Consensus status remains separate. The historical mint and its consuming burn remain in
the ledger prefix accepted under the selected rule.

Cardinality alone is too weak: it could pass while tokens are attached to the wrong
coordinates. Let `E_h[id]` be canonical episode state. The core correspondence is:

```text
dom(M_h) = { c in C : c != o and B_h(c) = 1 }
M_h is injective

for every c in dom(M_h):
    E_h[M_h(c)].coordinate = c
    E_h[M_h(c)].status = live

range(M_h) = {
    mint_id : E_h[mint_id] is a live MintEpisode
}

no burned mint_id occurs in range(M_h)
each MintEpisode has at most one consuming BurnEvent
each BurnEvent names exactly one existing MintEpisode
E_h[mint_id].status = consumed iff exactly one BurnEvent has
    consumed_mint_id = mint_id
MintEpisode IDs and BurnEvent IDs use distinct typed domains
```

Therefore `|M_h| = |{c != o : B_h(c) = 1}|` is a corollary. Each successful burn consumes
the exact live mint ID at that coordinate and emits a distinct burn-event ID; a re-placement
emits a fresh mint ID. A transfer preserves the mint ID, coordinate episode, and active
supply while changing only the selected authorization/ownership state.

This is **active supply**, not cumulative issuance. A user can produce arbitrarily many
historical mint and burn events by repeatedly placing and deleting a coordinate unless a
separate issuance, fee, or rate policy is introduced.

World and asset transitions MUST commit atomically. A receipt cannot report a successful
mint while the coordinate write failed, or a successful burn while the token-spend update
failed. A failed transaction may charge a declared fee, but it must not leave a partial
application state.

### 2.2 What “one block is one coin” means

A coordinate-specific occupancy token is naturally a unique UTXO-like or non-fungible
asset. It is not automatically a fungible cryptocurrency unit. The protocol MUST choose
one of these meanings:

1. **Occupancy token:** the token represents the right to claim one occupied coordinate.
2. **Edit right:** the token represents authority to change a coordinate, while the world
   remains world-owned.
3. **Fungible resource:** blocks consume or represent a separate interchangeable balance.
4. **Dual model:** occupancy tokens and a separate fungible fee/resource asset coexist.

Until that choice is made, “mint” and “burn” should be read as provenance terms, not as a
promise of financial value.

Transfers, if added, preserve active supply while changing ownership metadata. They MUST
not silently rewrite the coordinate episode. A burn becomes irreversible only under the
selected deterministic-finality rule; a probabilistic profile carries its declared residual
reversal risk. Governance MAY authorize a later, explicit compensating mint or
recovery transition, but it cannot erase the burn. A governance hard fork creates a newly
versioned history and must be disclosed as such.

## 3. Identity, authorization, and ownership

Multiplayer is useful because it creates competing action sources, but multiplayer alone
does not create secure identity. A username, session ID, or random UUID can be copied. A
future protocol needs a committed authorization model. A stable-actor candidate is:

```text
actor_id = ProfileIdHashV1(
    Domain("bitblock/actor-identity/v1")
 || LengthFrame(Canonical(
        chain_id,
        identity_namespace,
        initial_authorization_descriptor
    ))
)

A_h[actor_id] = current versioned authorization descriptor
```

`identity_namespace` is selected inside `genesis_payload` before `genesis_id` is derived, so
genesis authorization records do not create an identifier cycle. Transactions still bind
`genesis_id`, preventing cross-genesis replay. The descriptor can name one key, a threshold set, delegated capabilities, and an explicit
rotation/recovery policy. A consensus-accepted authorization transition changes the descriptor while
preserving `actor_id`. The simpler alternative is a public-key-hash address, in which case
“rotation” is an authorized ownership transfer to a new address rather than preservation of
identity. A network version MUST select one model; two valid keys cannot silently alias one
actor unless the committed multisignature or delegation policy says they do.

Throughout this paper, `Canonical(...)` and every remaining `a || b` sketch mean a typed,
unambiguously length-framed tuple encoding, never raw concatenation. The protocol MUST pin
integer width and endianness, enum and optional-field tags, list lengths and ordering,
normalization rules, and rejection of unknown, duplicate, non-minimal, or trailing fields.

A proposed signed transaction separates the signed payload from its signature:

```text
TransactionSignDoc =
    protocol_version
 || chain_id
 || genesis_id
 || chain_launch_manifest_id
 || profile_id
 || world_id
 || actor_id
 || authorizing_key_id
 || signature_suite_id
 || signing_hash_suite_id
 || actor_nonce
 || valid_after
 || valid_until
 || optional_anchor_block_id
 || optional_expected_state_root
 || fee_and_resource_offer
 || ordered_operations

signing_digest    = H[signing_hash_suite_id](
    action_signature_domain || Canonical(TransactionSignDoc)
)
signature         = Sign[signature_suite_id](actor_secret_key, signing_digest)
SignedTransaction = Canonical(
    TransactionSignDoc,
    signature
)

tx_id = H(transaction_id_domain || SignedTransaction)
```

The signature algorithm, key encoding, and hash suite are **Open** decisions. They MUST be
versioned and pinned before a network protocol is declared stable. The signed domain MUST include
the chain, genesis, launch-manifest, and admitted profile identifiers so an action cannot be replayed under another
world, launch authority, or candidate configuration. `profile_id` is added only after candidate derivation and
admission; it is context for signatures, headers, and releases, not a field inside the initial
application- or execution-root preimages.
The transaction identifier hashes the exact canonical signed envelope. The chosen signature
scheme MUST either be non-malleable or define one canonical signature encoding. No alternate
text spelling, omitted signature, or decoded-and-reencoded variant may create another
identifier for the same submitted transaction. SHA-256 content tags are not a substitute
for signatures.

The authorization rules MUST state:

- who may mint at a coordinate;
- who may burn or transfer the resulting token;
- whether ownership follows the token or remains with the world;
- whether an owner may delegate edit rights;
- how key rotation and recovery work;
- what happens when a key is lost or compromised.

Key custody is part of the security boundary. The protocol needs a documented key lifecycle:
generation, secure storage, backup, rotation, revocation, recovery, and compromise response.
If identity is permissionless, the Sybil cost or admission rule must also be explicit. A
public key proves control of a key under the signature assumption; it does not prove a
real-world name or unique human.

The cryptographic operations boundary MUST additionally specify:

- CSPRNG and entropy assumptions for key generation;
- public-key, subgroup, low-order-point, and proof-of-possession checks where applicable;
- canonical signature verification and a safe individual-verification fallback if batch or
  aggregate verification fails;
- separate signing domains and preferably separate keys for actor transactions, proposals,
  votes, validator transport, rewards/withdrawals, governance/emergency authority, and
  release signing;
- durable anti-double-sign state that survives validator restart, backup restore, signer
  failover, and clock rollback; and
- the trusted computing base, remote-signer or HSM boundary if used, and any physical or
  side-channel attacks explicitly excluded from the claim.

Client presentation is not proof of signing intent. A wallet SHOULD render the exact decoded
semantic operations, chain/genesis/launch/profile/world, fee cap, validity window, and authorization being
signed; the signer still authorizes only the canonical bytes above.

Actor/observer keys and validator keys are separate authority namespaces. An observer
signature authorizes only the action rights granted to that actor; it does not grant a vote,
proposal right, or validator reward unless a separate validator-state transition says so.

The observer can remain only camera state. It does not need to become an entity merely to
carry an identity or sign actions. However, a local camera position is not automatically a
consensus fact. If “the observer cannot enter a block” is an economic validity rule, the
protocol MUST either make the relevant pose a canonical state component or treat the
submitted coordinate and permission as the authoritative action. A client-only raycast
cannot authorize a transaction.

## 4. Transactions, ticks, and receipts

The current 60 Hz tick is a local simulation clock. Future consensus height is independent.
If a network version retains application ticks, `BlockContext_h` MUST supply their exact
logical ordinals and deterministic per-block count. No validator is required to execute at
60 wall-clock ticks per second.

### 4.1 Transaction validity

Before an action can affect state, a validator MUST check at least:

1. canonical decoding and protocol version;
2. signature and identity binding;
3. chain, genesis, launch manifest, admitted profile, world, and any selected anchor/state precondition;
4. nonce freshness and replay protection;
5. coordinate range and operation limits;
6. bedrock protection;
7. current occupancy and token-reference correctness;
8. ownership or delegated authorization;
9. fee/resource budget and per-tick work limits;
10. any explicitly consensus-relevant observer constraint.

The validator MUST also apply block-level limits before execution: maximum transaction
count and byte size, maximum coordinate/work budget, maximum state growth, and checked
integer arithmetic. Resource accounting must be deterministic; allocation failure,
overflow, and an exhausted budget are validity results, not process crashes.

`valid_after` and `valid_until` are interpreted against a declared logical height or epoch,
not an untrusted local wall clock. Signature verification, proof verification, and state
lookups also consume bounded work; otherwise a transaction can be syntactically valid yet
still exhaust a validator before the state transition begins.

Validity, admission, execution failure, and semantic no-op are different outcomes:

| Outcome | Consensus effect |
| --- | --- |
| `RejectedBeforeAdmission` | Malformed, bad-signature, wrong-domain, expired, or pre-admission-unaffordable bytes are excluded. They cause no consensus-state mutation; a local rejection response need not be permanently committed. |
| `DroppedAfterAdmission` | A node-local mempool may evict or expire an admitted but unincluded transaction under declared queue rules. This causes no consensus-state mutation and needs a local disposition, not a canonical receipt. |
| `BlockInvalid` | A proposed block that includes a transaction forbidden by block validation, has a wrong root, exceeds limits, or cannot execute deterministically is rejected wholesale. No candidate post-root becomes canonical. |
| `IncludedSuccess` | Ordered operations commit atomically with the declared nonce and fee disposition. |
| `IncludedNoOp` | An allowed idempotent `on -> on` or `off -> off` records an explicit success-no-op and applies the versioned nonce/fee rule, with no occupancy or token change. |
| `IncludedFailure` | A transaction admitted under the selected policy can record failure. It performs no partial execution-domain writes; only the explicitly selected nonce/fee effects may commit atomically. |

The proposed initial transaction model is transaction-local atomicity. Operations evaluate
in order against a staged transaction state; allowed no-ops may coexist with successes, but
any failing operation discards every staged execution-domain write for that transaction
except the explicitly versioned nonce/fee disposition of `IncludedFailure`. Blocks execute
transactions sequentially, so each receipt's actual pre-state
is the previous transaction's post-state. Candidate execution and durable publication are a
separate atomicity boundary: a node publishes either the fully validated candidate body and
roots or no candidate root, never a partial batch.

Every included transaction MUST have exactly one canonical receipt. Every submitted item
MUST receive a local disposition, but the protocol need not make rejected network spam part
of permanent consensus history. The submission, admission, execution, rejection, and
finalization lanes must reconcile without silently losing an included action.

### 4.2 Ordering and conflicts

Two observers may target the same coordinate in one logical tick. Arrival order on one
machine is not a safe distributed rule. A batch accepted at the selected consensus status
MUST carry one canonical order,
chosen by a sequencer or consensus mechanism. Under that order:

- the first valid `on` may open the coordinate;
- later `on` actions become explicit no-ops or rejections;
- an `off` must name a live token and satisfy authorization;
- a later `on` after a valid burn creates a fresh token identifier.

The tie-break rule and its error dispositions MUST be part of the state-machine version,
not an undocumented property of a particular network implementation.

If `toggle` remains a network operation, its receipt SHOULD include the observed pre-state
and post-state, and the transaction MAY carry an expected-state/token precondition. A
client-local assumption about what `toggle` will hit is not an authorization proof. A safer
wire format may lower `toggle` into explicit `on` or `off` after canonical ordering.

### 4.3 Transaction lifecycle

The protocol MUST distinguish these states:

```text
submitted -> rejected-before-admission / locally-expired
         \-> admitted -> dropped-or-expired-before-inclusion
                      \-> included-and-executed -> candidate
                                                 \-> finalized-or-threshold-confirmed
                                                 \-> orphaned-or-reorged under the selected rule
```

Mempool contents are node-local and MUST NOT be spendable or treated as ownership proof. A
transaction that references a stale anchor, expired validity window, or consumed nonce must
have a deterministic replacement or rejection rule. Exact-parent binding and windowed
anchor/precondition binding are different designs: a version MUST select one. If exact-parent
binding is selected, every new parent requires explicit re-signing or replacement; if a
windowed anchor is selected, the admissible ancestor window and expected-state failures are
consensus rules.

An unfinalized mint is provisional. Wallets and clients MUST distinguish candidate,
executed, and finalized or threshold-confirmed receipts. If a pre-finality fork removes a transaction, its mint
or burn is not part of the canonical supply. The same signed transaction MAY be re-included
if it remains valid under the surviving nonce, anchor, and validity-window rules; otherwise
the actor must authorize a replacement.

### 4.4 Proposed block, identifier, consensus evidence, and receipts

The signed object must not contain its own signature or certificate. A consensus-neutral
envelope separates the ordered body, deterministic execution commitments, proposal
signature, detached consensus evidence, and the separately verified availability assessment:

`CommitOrdered(domain, items)` below means a domain-separated authenticated sequence
commitment that binds the sequence type, exact length, every zero-based index, and each
canonical leaf byte string. Its inclusion proof also binds domain, length, index, and leaf;
it is not an undifferentiated Merkle root over raw concatenation. Within the proposal envelope,
`EnvelopeCommitOrderedV1` is the only instantiation of that operation: it uses the envelope owner's
canonical codec and object-hash suite over `CanonicalOrderedSequence(items)` rather than selecting a
second header-controlled commitment algorithm.

`CommitAvailability(envelope_owner, verified_parent_or_launch_policy, payload)` means the uniquely
resolved binding, domain-separated commitment to the exact canonical `AvailabilityPayload` bytes.
The resolver takes the envelope owner plus policy bytes already authenticated by the accepted parent
or initial launch manifest; a raw proposal field is not authority. Any erasure-coded shares, sampling
positions, or recovery proofs are derived from and verify against that same payload and policy; a
commitment to unrelated bytes cannot satisfy proposal validity.

`system_transition_schedule` contains only pre-execution rule invocations and deterministic
ordinals. It excludes event IDs, effects, post-state values, and anything derived from
`candidate_execution_context_id`, preventing a context/identifier cycle.

The bytes needed to find authority cannot use a profile-owned parser, because that authority is
what selects the profile. The wire therefore starts with this fixed, profile-independent grammar:

```text
AcceptedParentLookupPayloadV1 = FixedPreProfileCanonicalV1(
    accepted_parent_lookup_payload_version_u16_be,
    accepted_parent_lookup_namespace_tag,
    accepted_proposal_core_type_tag,
    block_id_type_tag,
    genesis_id,
    chain_launch_manifest_id,
    envelope_owner_profile_id,
    block_id_hash_suite_id,
    block_id_digest_length_u16_be,
    block_id_digest_bytes
)

accepted_parent_lookup_id = ProfileIdHashV1(
    Domain("bitblock/accepted-parent-lookup/v1")
 || LengthFrame(FixedPreProfileCanonicalV1(AcceptedParentLookupPayloadV1))
)

ParentOrLaunchLookupRefV1 =
    AcceptedParentLookupV1(accepted_parent_lookup_id_bytes)
  | InitialLaunchLookupV1(chain_launch_manifest_id_bytes)

FixedPreProfileEncodeRefPayloadV1(ref) =
    match ref {
        AcceptedParentLookupV1(lookup_id) => FixedWidthProfileIdHashBytes(lookup_id)
        InitialLaunchLookupV1(manifest_id) => FixedWidthProfileIdHashBytes(manifest_id)
    }

ProposalInnerShallowFrameV1 = FixedPreProfileShallowFrameV1(
    proposal_inner_magic,
    proposal_inner_version_u16_be,
    canonical_header_length_u64_be,
    canonical_availability_payload_length_u64_be,
    canonical_proposer_signature_length_u64_be,
    opaque_canonical_header_bytes,
    opaque_canonical_availability_payload_bytes,
    opaque_canonical_proposer_signature_bytes
)

ProposalOuterV1 = FixedPreProfileFrameV1(
    proposal_outer_magic,
    proposal_outer_version_u16_be,
    authority_ref_kind_u8,
    authority_ref_length_u32_be,
    inner_proposal_length_u64_be,
    opaque_authority_ref_bytes,
    opaque_inner_proposal_bytes
)

AcceptedRecordOuterV1 = FixedPreProfileFrameV1(
    accepted_record_outer_magic,
    accepted_record_outer_version_u16_be,
    authority_ref_kind_u8,
    authority_ref_length_u32_be,
    inner_proposal_length_u64_be,
    consensus_evidence_length_u64_be,
    availability_assessment_length_u64_be,
    opaque_authority_ref_bytes,
    opaque_inner_proposal_bytes,
    opaque_consensus_evidence_bytes,
    opaque_availability_assessment_bytes
)

AcceptedProposalCoreFieldManifestV1 = ExactOrderedFieldManifest(
    1 => accepted_proposal_core_version_u16_be,
    2 => accepted_parent_lookup_id,
    3 => authority_ref_kind_u8,
    4 => canonical_authority_ref_length_u32_be,
    5 => canonical_proposal_header_length_u64_be,
    6 => canonical_availability_payload_length_u64_be,
    7 => canonical_authority_ref_bytes,
    8 => canonical_proposal_header_bytes,
    9 => canonical_availability_payload_bytes
)

AcceptedProposalCoreV1 = FixedPreProfileCanonicalV1(
    FieldsExactly(AcceptedProposalCoreFieldManifestV1)
)

AcceptedProposalCoreExcludedFieldManifestV1 = ExactForbiddenFieldManifest(
    canonical_proposer_signature_length_u64_be,
    canonical_proposer_signature_bytes,
    typed_proposal_credential_id,
    proposal_credential_id,
    canonical_consensus_evidence_bytes,
    consensus_evidence_id,
    canonical_availability_assessment_bytes,
    availability_assessment_id
)

TypedProposalCredentialIdV1 = ProposalCredentialIdV1(
    proposal_credential_id_hash_suite_id,
    proposal_credential_id_digest_length_u16_be,
    proposal_credential_id_digest_bytes
)

VerifiedProposalCredentialEntryV1 = FixedPreProfileCanonicalV1(
    TypedProposalCredentialIdV1,
    canonical_proposer_signature_length_u64_be,
    canonical_proposer_signature_bytes
)

AcceptedProposalCredentialSetV1 = CanonicalSortedUniqueByTypedProposalCredentialId([
    VerifiedProposalCredentialEntryV1...
])
```

`FixedPreProfileFrameV1`, `FixedPreProfileShallowFrameV1`, and
`FixedPreProfileCanonicalV1` have one protocol-frozen byte grammar: fixed magic, namespace, type,
version, and tag values; big-endian unsigned lengths; no optional spelling or duplicate field; and
no trailing byte. A protocol-frozen `OuterFrameBoundsV1` caps each outer length, their checked sum,
and the total frame before allocation, copying, nested decode, lookup-dependent hashing, or
cryptographic work. Unknown versions/tags, a non-exact authority-reference length, integer overflow,
truncation, excess bytes, or a segment above the global bound rejects.

The accepted-parent outer value is a fixed-width `accepted_parent_lookup_id`, not a bare block
digest. Its hashed payload binds the fixed lookup namespace and record/block types, exact
genesis/launch identity, envelope-owner profile, block-ID hash suite, declared digest length, and
digest bytes. `block_id_digest_length_u16_be` MUST equal both the carried byte length and the fixed
output length registered for `block_id_hash_suite_id`. The launch-manifest ID transitively binds the
chain identity through its genesis ID. `FixedPreProfileEncodeRefPayloadV1` is the complete ref
encoding: the outer branch tag selects one of two exact, fixed-width `ProfileIdHashV1` outputs and no
profile codec participates.

The local accepted-parent index is keyed only by this typed lookup ID and MUST map it to exactly one
byte-exact, locally verified `AcceptedProposalCoreV1`, one bounded
`AcceptedProposalCredentialSetV1`, and one bounded `AcceptedEvidenceSetV1`. The core contains exactly
the proposal's fixed authority reference, canonical header, and canonical availability payload under
the recomputed lookup. `AcceptedProposalCoreFieldManifestV1` is the complete ordered field tuple,
not an illustrative prefix; `AcceptedProposalCoreExcludedFieldManifestV1` is forbidden at every
nested or flattened spelling. The core therefore contains no proposer signature, proposal
credential, consensus evidence, or availability-assessment field. Zero cores or two distinct
signature-free core byte strings under one lookup ID rejects rather than selecting an arbitrary row.
Before assigning the safe-halt class, the verifier reconstructs the complete
`AcceptedParentLookupPayloadV1` for each core. The only two valid collision-class rows, in canonical
order, are:

```text
AcceptedParentCollisionDispositionV1 = CanonicalTwoRowMap(
    row_1 = (
        ByteDistinctAcceptedParentLookupPayloadPreimagesSameProfileIdHashV1OutputV1,
        ProtocolIdentityHashCompromiseV1
    ),
    row_2 = (
        ByteEqualAcceptedParentLookupPayloadAndBlockIdDistinctValidAcceptedProposalCoresV1,
        ProfileOwnedAuthorityBindingCompromiseV1
    )
)
```

The mapping is exact: its two rows may not be omitted, merged, reordered, or swapped. Row 1 means
two different complete lookup-payload byte strings produce one `ProfileIdHashV1` output. Row 2
means that the complete lookup payload, including the old-profile `block_id`, is byte-identical but
two distinct signature-free cores otherwise validate, proving loss of the active profile's header,
authority-reference, or availability-payload binding. A core whose authority/header/payload
projections do not reconstruct its claimed lookup is simply invalid and never becomes collision
evidence or a third disposition row.

Multiple `AcceptedBlockRecord` carriers with that same unsigned core are expected. Each carried
signature is bounded-decoded under the authority-selected proposal credential resolution, verified
over the exact core `block_id`, assigned its complete tagged `TypedProposalCredentialIdV1`, and only
then inserted into `AcceptedProposalCredentialSetV1`. The set is normalized by tagged ID, independent
of wrapper arrival order. An exact duplicate is idempotent; an invalid, wrong-key, wrong-suite,
wrong-domain, or wrong-block credential is rejected without changing the set. Two different
canonical credential byte strings under one typed credential ID enter
`ProfileOwnedAuthorityBindingCompromiseV1`; a node never selects one by arrival order. The
profile-owned bounded retention rule MUST preserve at least one canonical qualifying proposal witness
for every retained core. Acceptance and parent eligibility require at least one such witness, but
additional valid credentials have exactly zero proposal, vote, eligibility, or fork-choice weight.

The same wrappers' independently verified consensus and availability entries are unioned into the
separate evidence set by their typed evidence rules, not treated as competing core records. After
retrieval, the verifier recomputes the parent's `block_id` with the core's envelope owner,
reconstructs every field of `AcceptedParentLookupPayloadV1`, recomputes the lookup ID, and requires
exact equality before evaluating credentials or eligibility. Wrong launch/genesis, profile, suite,
core or block type, digest length, digest, alias, or a true unsigned-core collision therefore fails
before the parent can supply authority. Credential or evidence multiplicity by itself is not a core
collision and cannot poison this index. `InitialLaunchLookupV1` instead must resolve the complete
`chain_launch_package`,
recompute its entire launch closure, and verify `ChainLaunchAuthorizationV1`; it is legal only for
the manifest's `first_ledger_height`, `genesis_parent_block_id`, initial ordinary profile context,
initial root, and history root. Launch authorization is not an accepted-parent consensus status and
is never passed through that predicate. The first record's own consensus evidence is still required
before that record becomes accepted.

```text
VerifiedEnvelopeAuthorityV1(lookup_ref) =
    match lookup_ref {
        AcceptedParentLookupV1(lookup_id) =>
            ResolveEligibleAcceptedParentCoreV1(
                lookup_id,
                EnvelopeParentAuthorityV1
            )
        InitialLaunchLookupV1(manifest_id) =>
            VerifyCompleteLaunchClosureAndAuthorizationV1(manifest_id)
    }
```

These branches return the same bounded resolver interface but remain distinct tagged authority
proofs. Neither function may call, coerce, or fall through to the other.
`ResolveEligibleAcceptedParentCoreV1` constructs the exact `AcceptedParentAuthorityRefV1` for the
requested use from the unique core's authority-owned policy and predicate. A carried release or
activation ref must byte-equal that derivation, including its purpose tag; its policy and predicate
fields are assertions, never selectors. Successful resolution returns a semantic
`VerifiedEligibleAcceptedParentRefV1` bound to the unique core and the attained profile-owned status,
not to one evidence wrapper or evidence-set serialization. For the accepted-parent branch, the
header's `envelope_historical_authority_ref` MUST be
`AcceptedParentEnvelopeHistoricalAuthorityRefV1`; its embedded
`HistoricalAcceptedParentAuthorityUseRefV1` MUST byte-equal that envelope-purpose result, exact
accepted consensus-native certificate and authorization IDs, evidence commitment, authenticated
evidence cutoff and occurrence-universe root, position, and attained status. The launch branch instead requires
`InitialLaunchHistoricalAuthorityRefV1` for the exact launch manifest and contains no fabricated
accepted-parent certificate. Because this tagged value is inside `ProposalHeader`, `block_id` and
the proposer credential bind the already accepted certificate before any later archive event can
record the child as a consumer.
Throughout §4.4, **authority-selected** means selected by that verified tagged result: the complete
authorized launch package supplies the initial values at the first ledger height, while the
verified accepted parent supplies them thereafter. A rule written only for an accepted parent does
not apply to the first-record branch unless it separately names the launch-authorized initial
equivalent.

Immediately after that branch selects the envelope owner, and before any profile-owned decode or
hash, the verifier resolves `AuthorityOwnedParseAndWorkLimitsV1` from the verified parent or launch
package. Candidate/header fields cannot select or widen it. The verifier then reads only the fixed
three-length prefix of `ProposalInnerShallowFrameV1`, checks each slice and their sum against both
the global outer cap and the equal-or-tighter authority-owned caps, and runs the selected codec's
admission-tested streaming preflight over the header, availability-payload, and signature slices.
That preflight must validate every nested length/count, nesting depth, declared decompressed size,
collection element bound, proof/witness/signature size, and parser/work budget without allocating,
copying, decompressing, materializing, hashing, canonically re-encoding, or verifying cryptography.
A profile whose codec cannot provide this deterministic bounded preflight is not admissible.

Only after preflight succeeds may the verifier materialize the three slices under the uniquely
resolved owner codec/scheme. It canonically re-encodes each object, requires equality with its exact
slice, rebuilds the fixed shallow frame, and requires equality with the complete outer inner
segment. The decoded proposal's parent or launch projection MUST match the verified outer branch.
For `AcceptedRecordOuterV1`, its proposal segment MUST equal the standalone `ProposalOuterV1` inner
bytes for that candidate, and the two evidence segments MUST be the exact canonical bytes that later
verify against it. Thus neither an inner parser nor a decoded header chooses authority or its work
budget.

Powered outer-frame controls include one accepted-parent proposal and one first-record launch
proposal, then flip the branch tag, substitute either authority reference, mismatch each declared
length, overflow the checked sum, append trailing bytes, change an inner byte after lookup, and try
to pass launch authorization through the parent-consensus branch (and vice versa). Each malformed
frame rejects before profile-owned parsing or expensive verification. Lookup controls additionally
mutate namespace, launch/genesis, profile, suite, record/block type, digest length, digest, index
aliasing, and collision multiplicity. Shallow-frame controls place a body, receipt, witness,
compressed value, nested count, or parser-work declaration below the global cap but above the active
profile cap, then mutate each slice length and decompression/work declaration. Every such mutant
rejects during lookup or allocation-free preflight, before materialization or hashing.

Profile context and root context are explicit. Admission status is checked by the verifier and is
not a bit that a candidate may set inside these bytes:

```text
ProfileContextV1 = Canonical(
    protocol_version,
    chain_id,
    genesis_id,
    world_id,
    profile_id,
    profile_scope_digest,
    domain_registry_digest
)

ProfileTransitionContextV1 =
    OrdinaryProfileTransitionV1(
        active_profile_context
    )
  | InPlaceMigrationTransitionV1(
        old_active_profile_context,
        new_admitted_profile_context,
        successor_profile_admission_manifest_id,
        migration_spec_id,
        successor_activation_bundle_id,
        activation_authorization_id,
        activation_height
    )

EnvelopeOwnerProfileContextV1(profile_transition_context) =
    match profile_transition_context {
        OrdinaryProfileTransitionV1(active) => active,
        InPlaceMigrationTransitionV1(
            old, new, admission, spec, bundle, authorization, height
        ) => old
    }

EnvelopeResolutionV1(profile_transition_context) =
    ResolveAdmittedEnvelopeProfile(
        EnvelopeOwnerProfileContextV1(profile_transition_context)
    )

EnvelopeHashV1(profile_transition_context, object_domain, payload) =
    H[EnvelopeResolutionV1(profile_transition_context).object_hash_suite_id](
        Domain(object_domain)
     || LengthFrame(Canonical[
            EnvelopeResolutionV1(profile_transition_context).canonical_codec_id
        ](
            EnvelopeOwnerProfileContextV1(profile_transition_context)
        ))
     || LengthFrame(Canonical[
            EnvelopeResolutionV1(profile_transition_context).canonical_codec_id
        ](payload))
    )

EnvelopeCommitOrderedV1(profile_transition_context, sequence_domain, sequence) =
    EnvelopeHashV1(
        profile_transition_context,
        sequence_domain,
        CanonicalOrderedSequence(sequence)
    )

ExecutionRootRefV1 = Canonical(
    profile_context,
    execution_state_root
)

SuccessorUpgradeEvidenceManifestV1 = Canonical(
    successor_upgrade_evidence_manifest_schema_version,
    predecessor_admitted_profile_id,
    successor_candidate_profile_id,
    successor_transition_spec_at_profile_id,
    migration_spec_id,
    old_execution_root_ref,
    expected_new_execution_root_ref,
    canonical_parameter_and_state_delta_digest,
    powered_bounded_migration_replay_artifact_ids,
    required_migration_implementation_artifact_ids,
    upgrade_1_disposition
)

successor_upgrade_evidence_manifest_id = ProfileIdHashV1(
    Domain("bitblock/successor-upgrade-evidence-manifest/v1")
 || LengthFrame(Canonical(SuccessorUpgradeEvidenceManifestV1))
)

VerifiedSuccessorPredecessorBaseContextV1 = SemanticVerifierResult(
    shared_predecessor_accepted_parent_lookup_id,
    shared_predecessor_core_block_id,
    release_parent_authority_ref =
        AcceptedParentAuthorityRefV1(SuccessorReleaseAuthorityV1),
    verified_release_parent_eligibility =
        VerifiedEligibleAcceptedParentRefV1(SuccessorReleaseAuthorityV1),
    release_parent_authority_status,
    activation_parent_authority_ref =
        AcceptedParentAuthorityRefV1(SuccessorActivationAuthorityV1),
    verified_activation_parent_eligibility =
        VerifiedEligibleAcceptedParentRefV1(SuccessorActivationAuthorityV1),
    activation_parent_authority_status,
    predecessor_admitted_profile_id,
    old_active_profile_context,
    genesis_id,
    chain_launch_manifest_id,
    predecessor_governance_authority_root,
    activation_old_execution_root_ref
)

VerifiedSuccessorReleaseAuthorityContextV1 = SemanticVerifierResult(
    VerifiedSuccessorPredecessorBaseContextV1,
    admitted_release_core_id,
    release_parent_authority_certificate_consumer_context =
        SuccessorReleaseParentAuthorityConsumerContextV1,
    historical_release_parent_authority_use_ref =
        HistoricalAcceptedParentAuthorityUseRefV1(SuccessorReleaseAuthorityV1)
)

VerifiedSuccessorActivationAuthorityContextV1 = SemanticVerifierResult(
    VerifiedSuccessorPredecessorBaseContextV1,
    VerifiedSuccessorReleaseAuthorityContextV1,
    successor_activation_bundle_id,
    activation_parent_authority_certificate_consumer_context =
        SuccessorActivationParentAuthorityConsumerContextV1,
    historical_activation_parent_authority_use_ref =
        HistoricalAcceptedParentAuthorityUseRefV1(SuccessorActivationAuthorityV1)
)

SupplementarySuccessorActivationArtifactV1 = Canonical(
    typed_artifact_schema_id,
    typed_artifact_id,
    canonical_artifact_preimage_length_u64,
    canonical_artifact_preimage_bytes
)

SuccessorActivationInternalDependencyRefV1 =
    InternalSuccessorActivationArtifactDependencyV1(typed_internal_artifact_id)

SuccessorActivationInternalDependencyNodeV1 = Canonical(
    topological_rank_u32,
    typed_internal_artifact_id,
    canonical_sorted_unique_internal_dependency_refs
)

SuccessorActivationBundleV1 = Canonical(
    successor_activation_bundle_schema_version,
    predecessor_admitted_profile_id,
    successor_admitted_profile_id,
    successor_profile_admission_manifest_id,
    successor_transition_spec_at_profile_id,
    successor_release_record_id,
    migration_spec_id,
    successor_upgrade_evidence_manifest_id,
    internal_artifact_count_u32,
    supplementary_internal_artifact_count_u32,
    internal_dependency_edge_count_u32,
    internal_canonical_preimage_bytes_u64,
    explicit_internal_preimage_lengths_u64[
        successor_profile_scope,
        successor_domain_registry,
        successor_candidate_profile,
        successor_transition_spec_at_profile,
        successor_profile_admission_manifest,
        successor_admitted_release_record,
        successor_upgrade_evidence_manifest
    ],
    ProfileScopeV1,
    DomainRegistryV1,
    CandidateProfileV1,
    TransitionSpecAtProfileV1,
    ProfileAdmissionManifestV1,
    AdmittedReleaseRecordV1,
    SuccessorUpgradeEvidenceManifestV1,
    CanonicalSortedUniqueByTypedArtifactId([
        SuccessorActivationInternalDependencyNodeV1...
    ]),
    CanonicalSortedUniqueByTypedArtifactId([
        SupplementarySuccessorActivationArtifactV1...
    ])
)

successor_activation_bundle_id = ProfileIdHashV1(
    Domain("bitblock/successor-activation-bundle/v1")
 || LengthFrame(Canonical(SuccessorActivationBundleV1))
)

SuccessorActivationSignDocV1 = Canonical(
    successor_activation_sign_doc_schema_version,
    chain_launch_manifest_id,
    predecessor_accepted_parent_authority_ref,
    historical_predecessor_activation_authority_use_ref,
    old_active_profile_context,
    new_admitted_profile_context,
    successor_profile_admission_manifest_id,
    migration_spec_id,
    successor_activation_bundle_id,
    activation_height,
    activation_old_execution_root_ref,
    activation_expected_new_execution_root_ref,
    activation_authority_policy_id
)

successor_activation_sign_doc_id = EnvelopeHashV1(
    OrdinaryProfileTransitionV1(old_active_profile_context),
    "bitblock/successor-activation-sign-doc/v1",
    SuccessorActivationSignDocV1
)

ActivationAuthorizationV1 = Canonical(
    activation_authorization_schema_version,
    SuccessorActivationSignDocV1,
    successor_activation_sign_doc_id,
    activation_authority_policy_id,
    activation_authorization_scheme_id,
    activation_authority_credential_set_id,
    canonical_activation_authorization_bytes
)

activation_authorization_id = EnvelopeHashV1(
    OrdinaryProfileTransitionV1(old_active_profile_context),
    "bitblock/successor-activation-authorization/v1",
    ActivationAuthorizationV1
)

SuccessorActivationCarrierV1 =
    NoSuccessorActivationV1
  | EmbeddedSuccessorActivationV1(
        SuccessorActivationBundleV1,
        successor_activation_bundle_id,
        ActivationAuthorizationV1,
        activation_authorization_id
    )

Body =
    OrdinaryBodyV1(
        ordered_signed_transactions,
        deterministic_auxiliary_inputs
    )
  | DedicatedMigrationBodyV1(
        empty_signed_transaction_sequence,
        deterministic_migration_inputs
    )

Execute(state, block_context, body) =
    match (block_context.profile_transition_context, body) {
        (OrdinaryProfileTransitionV1(active),
         OrdinaryBodyV1(transactions, auxiliary_inputs)) =>
            ExecuteOrdinaryV1(
                state,
                block_context,
                transactions,
                auxiliary_inputs
            )
        (InPlaceMigrationTransitionV1(
             old, new, admission, spec, bundle, authorization, height
         ),
         DedicatedMigrationBodyV1(empty_transactions, migration_inputs)) =>
            ExecuteDedicatedMigrationV1(
                state,
                block_context,
                migration_inputs
            )
        _ => BlockInvalid
    }

transactions_root = EnvelopeCommitOrderedV1(
    profile_transition_context,
    transaction_sequence_domain,
    SignedTransactionSequence(Body)
)
body_root = EnvelopeHashV1(profile_transition_context, body_domain, Body)

BlockContext = Canonical(
    protocol_version,
    chain_id,
    genesis_id,
    chain_launch_manifest_id,
    profile_transition_context,
    world_id,
    height,
    parent_block_id,
    epoch,
    round_or_view,
    optional_committed_randomness,
    application_subtick_schedule,
    system_transition_schedule
)

block_context_root = EnvelopeHashV1(
    profile_transition_context,
    block_context_domain,
    BlockContext
)

candidate_execution_context_id = EnvelopeHashV1(
    profile_transition_context,
    candidate_execution_context_domain,
    Canonical(
    protocol_version,
    chain_id,
    genesis_id,
    chain_launch_manifest_id,
    profile_transition_context,
    world_id,
    height,
    parent_block_id,
    body_root,
    block_context_root,
    execution_parameter_root,
    governance_authority_root,
    active_validator_root,
    consensus_policy_id,
    proposer_identity
))

receipts_root = EnvelopeCommitOrderedV1(
    profile_transition_context,
    execution_receipt_sequence_domain,
    all_transaction_and_system_receipts_in_execution_order
)

AvailabilityPayload = Canonical(
    BlockContext,
    Body,
    SuccessorActivationCarrierV1,
    all_transaction_and_system_receipts_in_execution_order,
    consensus_required_execution_witnesses
)

data_availability_commitment = CommitAvailability(
    EnvelopeOwnerProfileContextV1(profile_transition_context),
    VerifiedEnvelopeAvailabilityPolicyV1(
        VerifiedEnvelopeAuthorityV1(ParentOrLaunchLookupRefV1),
        data_availability_policy_id
    ),
    AvailabilityPayload
)

post_history_root = EnvelopeHashV1(
    profile_transition_context,
    history_domain,
    Canonical(
    parent_history_root,
    height,
    body_root,
    receipts_root,
    post_execution_state_root_ref
))

ProposalHeader = Canonical(
    protocol_version,
    chain_id,
    genesis_id,
    chain_launch_manifest_id,
    profile_transition_context,
    world_id,
    height,
    epoch,
    round_or_view,
    parent_block_id,
    envelope_historical_authority_ref,
    parent_history_root,
    pre_execution_state_root_ref,
    block_context_root,
    candidate_execution_context_id,
    body_root,
    transactions_root,
    receipts_root,
    post_execution_state_root_ref,
    post_history_root,
    execution_parameter_root,
    execution_policy_id,
    governance_authority_root,
    active_validator_root,
    optional_next_validator_root,
    consensus_policy_id,
    data_availability_policy_id,
    data_availability_commitment,
    proposer_identity,
    proposer_key_id,
    proposal_signature_suite_id,
    proposal_signing_hash_suite_id
)

block_id = EnvelopeHashV1(
    profile_transition_context,
    block_header_domain,
    ProposalHeader
)
proposal_credential_resolution = ResolveAuthorityAuthorizedProposalCredentialV1(
    EnvelopeOwnerProfileContextV1(profile_transition_context),
    VerifiedEnvelopeAuthorityV1(ParentOrLaunchLookupRefV1),
    proposer_identity,
    proposer_key_id
)
require proposal_signature_suite_id
    == proposal_credential_resolution.signature_suite_id
require proposal_signing_hash_suite_id
    == proposal_credential_resolution.signing_hash_suite_id
proposer_signature = Sign[proposal_credential_resolution.signature_suite_id](
    proposer_secret_key_for(proposer_key_id),
    H[proposal_credential_resolution.signing_hash_suite_id](
        proposal_signature_domain || block_id
    )
)
canonical_proposal_header_bytes = Canonical[
    EnvelopeResolutionV1(profile_transition_context).canonical_codec_id
](ProposalHeader)
canonical_availability_payload_bytes = Canonical[
    EnvelopeResolutionV1(profile_transition_context).canonical_codec_id
](AvailabilityPayload)
canonical_proposer_signature_bytes = Canonical[
    EnvelopeResolutionV1(profile_transition_context).canonical_codec_id
](proposer_signature)
proposal_credential_id = H[
    proposal_credential_resolution.credential_id_hash_suite_id
](
    Domain("bitblock/proposal-credential/v1")
 || LengthFrame(Canonical(
        block_id,
        proposer_identity,
        proposer_key_id,
        proposal_signature_suite_id,
        proposal_signing_hash_suite_id,
        canonical_proposer_signature_bytes
    ))
)
typed_proposal_credential_id = ProposalCredentialIdV1(
    proposal_credential_resolution.credential_id_hash_suite_id,
    Length(proposal_credential_id),
    proposal_credential_id
)
inner_proposal_bytes = FixedPreProfileEncodeShallowFrameV1(
    ProposalInnerShallowFrameV1(
        proposal_inner_magic,
        proposal_inner_version_u16_be,
        Length(canonical_proposal_header_bytes),
        Length(canonical_availability_payload_bytes),
        Length(canonical_proposer_signature_bytes),
        canonical_proposal_header_bytes,
        canonical_availability_payload_bytes,
        canonical_proposer_signature_bytes
    )
)
ConsensusEvidence =
    DeterministicFinalityEvidence(block_id, ...)
  | ProbabilisticConfirmationEvidence(block_id, ...)
  | CentralAuthorityEvidence(block_id, ...)

consensus_evidence_resolution = ResolveAuthorityAuthorizedConsensusEvidenceV1(
    EnvelopeOwnerProfileContextV1(profile_transition_context),
    VerifiedEnvelopeConsensusPolicyV1(
        VerifiedEnvelopeAuthorityV1(ParentOrLaunchLookupRefV1),
        consensus_policy_id
    )
)
canonical_consensus_evidence_bytes = Canonical[
    consensus_evidence_resolution.canonical_codec_id
](ConsensusEvidence)
consensus_evidence_id = H[consensus_evidence_resolution.hash_suite_id](
    Domain("bitblock/consensus-evidence/v1")
 || LengthFrame(canonical_consensus_evidence_bytes)
)

AvailabilityClaimCoreV1 = Canonical(
    block_id,
    data_availability_policy_id,
    epoch,
    active_validator_root,
    availability_predicate_id,
    availability_status
)

AvailabilityEvidenceV1 = Canonical(
    AvailabilityClaimCoreV1,
    evidence_scheme_id,
    canonical_evidence_bytes
)

availability_evidence_resolution = ResolveAuthorityAuthorizedAvailabilityEvidenceV1(
    EnvelopeOwnerProfileContextV1(profile_transition_context),
    VerifiedEnvelopeAvailabilityPolicyV1(
        VerifiedEnvelopeAuthorityV1(ParentOrLaunchLookupRefV1),
        data_availability_policy_id
    ),
    evidence_scheme_id
)
require evidence_scheme_id
    == availability_evidence_resolution.evidence_scheme_id
availability_evidence_id = H[availability_evidence_resolution.hash_suite_id](
    Domain("bitblock/availability-evidence/v1")
 || LengthFrame(Canonical[
        availability_evidence_resolution.canonical_codec_id
    ](AvailabilityEvidenceV1))
)

AvailabilityAssessmentV1 =
    ConsensusEntailedAvailabilityV1(
        AvailabilityClaimCoreV1,
        consensus_evidence_id
    )
  | DetachedAvailabilityEvidenceV1(
        AvailabilityEvidenceV1,
        availability_evidence_id
    )

AvailabilityAssessmentRefV1 =
    ConsensusEntailedAssessmentRefV1(consensus_evidence_id)
  | DetachedEvidenceAssessmentRefV1(availability_evidence_id)

availability_assessment_resolution =
    ResolveAuthorityAuthorizedAvailabilityAssessmentEncodingV1(
        EnvelopeOwnerProfileContextV1(profile_transition_context),
        VerifiedEnvelopeAvailabilityPolicyV1(
            VerifiedEnvelopeAuthorityV1(ParentOrLaunchLookupRefV1),
            data_availability_policy_id
        )
    )
canonical_availability_assessment_bytes = Canonical[
    availability_assessment_resolution.canonical_codec_id
](AvailabilityAssessmentV1)
availability_assessment_id = H[availability_assessment_resolution.hash_suite_id](
    Domain("bitblock/availability-assessment/v1")
 || LengthFrame(canonical_availability_assessment_bytes)
)

TypedAcceptedEvidenceIdV1 =
    ConsensusEvidenceIdV1(consensus_evidence_id)
  | AvailabilityAssessmentIdV1(availability_assessment_id)

AcceptedEvidenceDependencyWitnessV1 =
    SameWrapperConsensusDependencyWitnessV1(
        ConsensusEvidenceIdV1(consensus_evidence_id),
        canonical_consensus_evidence_length_u64,
        canonical_consensus_evidence_bytes
    )
  | DetachedAvailabilityDependencyWitnessV1(
        AvailabilityEvidenceIdV1(availability_evidence_id)
    )

AcceptedEvidenceDependencyWitnessIdV1(witness) = EnvelopeHashV1(
    OrdinaryProfileTransitionV1(parent_profile_context),
    accepted_evidence_dependency_witness_domain,
    Canonical(AcceptedEvidenceDependencyWitnessV1(witness))
)

SameWrapperAcceptedEvidenceDependencyWitnessIdV1(witness) =
    AcceptedEvidenceDependencyWitnessIdV1(witness)
    where AcceptedEvidenceDependencyWitnessV1(witness)
          is SameWrapperConsensusDependencyWitnessV1

AcceptedParentAuthorityCommittedWitnessIdV1 =
    ProposalCredentialCommittedWitnessIdV1(
        TypedProposalCredentialIdV1
    )
  | AcceptedEvidenceCommittedWitnessIdV1(
        TypedAcceptedEvidenceIdV1
    )
  | EvidenceDependencyCommittedWitnessIdV1(
        AcceptedEvidenceDependencyWitnessIdV1
    )

VerifiedAcceptedEvidenceEntryV1 =
    VerifiedConsensusEvidenceEntryV1(
        ConsensusEvidenceIdV1(consensus_evidence_id),
        canonical_consensus_evidence_bytes
    )
  | VerifiedAvailabilityAssessmentEntryV1(
        AvailabilityAssessmentIdV1(availability_assessment_id),
        canonical_availability_assessment_bytes,
        AcceptedEvidenceDependencyWitnessV1
    )

VerifiedConsensusEntailedAvailabilityDeltaV1 = Canonical(
    verified_same_wrapper_consensus_entry,
    verified_same_wrapper_availability_assessment_entry,
    SameWrapperConsensusDependencyWitnessV1
)

AcceptedEvidenceSetV1 = CanonicalSortedUniqueByTypedEvidenceId([
    VerifiedAcceptedEvidenceEntryV1...
])

outer_authority_ref_bytes =
    FixedPreProfileEncodeRefPayloadV1(ParentOrLaunchLookupRefV1)

accepted_proposal_core = AcceptedProposalCoreV1(
    accepted_proposal_core_version_u16_be,
    accepted_parent_lookup_id,
    ParentOrLaunchLookupRefV1.tag,
    Length(outer_authority_ref_bytes),
    Length(canonical_proposal_header_bytes),
    Length(canonical_availability_payload_bytes),
    outer_authority_ref_bytes,
    canonical_proposal_header_bytes,
    canonical_availability_payload_bytes
)

verified_proposal_credential_entry = VerifiedProposalCredentialEntryV1(
    typed_proposal_credential_id,
    Length(canonical_proposer_signature_bytes),
    canonical_proposer_signature_bytes
)

Proposal = ProposalOuterV1(
    proposal_outer_magic,
    proposal_outer_version_u16_be,
    ParentOrLaunchLookupRefV1.tag,
    Length(outer_authority_ref_bytes),
    Length(inner_proposal_bytes),
    outer_authority_ref_bytes,
    inner_proposal_bytes
)

AcceptedBlockRecord = AcceptedRecordOuterV1(
    accepted_record_outer_magic,
    accepted_record_outer_version_u16_be,
    ParentOrLaunchLookupRefV1.tag,
    Length(outer_authority_ref_bytes),
    Length(inner_proposal_bytes),
    Length(canonical_consensus_evidence_bytes),
    Length(canonical_availability_assessment_bytes),
    outer_authority_ref_bytes,
    inner_proposal_bytes,
    canonical_consensus_evidence_bytes,
    canonical_availability_assessment_bytes
)
```

`ConsensusEvidence` may be a quorum certificate, declared work/ancestry and confirmation
context, or a trusted sequencer signature according to the selected consensus profile. It is never inside the
`block_id` it authenticates. Consensus-specific header fields must replace, not informally
reinterpret, generic fields such as `round_or_view`.

`TypedAcceptedEvidenceIdV1` makes consensus evidence and availability assessments separate ID
namespaces even if their underlying profile-owned hash bytes happen to match. Every entry MUST be
bounded-decoded under the authority-selected codec, recompute its typed ID, verify against the exact
core `block_id`, policy, epoch/authority, status, and predicate, and only then enter
`AcceptedEvidenceSetV1`. The set is sorted by the complete tagged ID. An exact duplicate is
idempotent. Two different canonical byte strings claiming one tagged evidence ID are an evidence-ID
collision and, when both otherwise verify under a live authority role, enter
`ProfileOwnedAuthorityBindingCompromiseV1`; a node never chooses one by arrival order. An invalid
entry is rejected without changing the set. A valid but nonqualifying entry may be retained for
diagnosis or later status projection, but cannot by itself make a core eligible.

`ConsensusEntailedAvailabilityV1` has a stronger same-wrapper rule. The verifier first verifies the
exact consensus-evidence segment carried by that `AcceptedRecordOuterV1` and recomputes its
`ConsensusEvidenceIdV1`. The assessment's named `consensus_evidence_id` MUST equal that recomputed
same-wrapper ID; later or earlier evidence from another wrapper cannot satisfy it. Only after the
selected consensus policy proves the exact availability claim as a consequence of those bytes may
the verifier construct `VerifiedConsensusEntailedAvailabilityDeltaV1`. The delta inserts the
consensus entry and availability-assessment entry atomically or inserts neither. Its retained
`SameWrapperConsensusDependencyWitnessV1` includes the exact bounded consensus bytes and ID, so
live normalization, retention, and eviction cannot orphan the assessment. This semantic constructor
does not by itself prove historical co-carriage to a fresh replay verifier; §9 additionally requires
the proof-bearing `SameWrapperAcceptedRecordOccurrenceWitnessV1` under the shared archive view. No
pending-assessment queue or arrival-order retry is part of authority state.
`DetachedAvailabilityEvidenceV1` remains a separate evidence entry with its own typed evidence
witness and is never upgraded into the entailed variant by finding consensus evidence in another
carrier.

For a fixed core and requested `AcceptedParentAuthorityUseV1`, the envelope owner's admitted profile
MUST freeze `ParentAuthorityEligibilityV1`: the authorized proposal-credential and
consensus/availability evidence codecs, typed-ID rules, both set bounds and deterministic
retention/summary rules, a status partial order and deterministic join, the purpose-specific
eligibility predicate, and the exact conflict predicate/disposition. Admission MUST establish, for
conflict-free verified credential sets `C subset C2` and evidence sets `E subset E2`,

```text
ParentAuthorityStatusV1(core, use, C, E)
    <= ParentAuthorityStatusV1(core, use, C2, E2)

ParentAuthorityEligibilityV1(core, use, C, E) = Eligible
    implies ParentAuthorityEligibilityV1(core, use, C2, E2) = Eligible
```

The retention/summary rule MUST preserve the attained status, one canonical verifying witness for
each attained requirement, every availability dependency witness, and all conflict evidence needed
by the declared disposition; resource bounds cannot evict a dependency, split an atomic pair, or
silently downgrade an already eligible core. Additional qualifying evidence may upgrade the
deterministic status but cannot invalidate eligibility. Verified evidence satisfying the frozen
conflict predicate is retained and invokes its predeclared disposition separately: a safety-rule
violation enters safe halt, while a profile-permitted non-safety alternative remains nonqualifying.
Conflict handling never deletes the unique core, selects an evidence wrapper, or turns a different
core into the value of the same lookup ID.

`ParentAuthorityEligibilityV1` returns `VerifiedEligibleAcceptedParentRefV1` only when the unique
core, at least one qualifying proposal credential, and purpose-specific evidence predicate pass.
That semantic result binds the exact
`AcceptedParentAuthorityRefV1`, unique core `block_id`, and attained status, but neither the
evidence-set bytes nor one quorum/assessment occurrence becomes parent identity. Release and
successor activation therefore may name a stable parent. A node's partial local evidence may support
only a preliminary semantic result; it is not the complete input to consumable authority.

That local semantic result is not yet consumable authority.
`ResolveConsensusNativeParentAuthorityCertificatePolicyV1` takes the verified parent profile,
purpose, unique core, and exact `ParentAuthorityCertificateConsumerContextV1` and returns the only
permitted evidence-normalization rule, `RequiredParentAuthorityCertificatePositionV1`, certificate
policy/codec/hash/signature scheme and credential set, consensus rule, availability rule, required
acceptance status, accepted-record occurrence index/root and range-proof schemes, exact evidence
cutoff rule, bounds, and equivocation/liveness disposition. The required cutoff is the last complete
consensus state immediately preceding the certificate admission in the profile's total order. That
order includes an intra-consensus-position ordinal, so a wrapper accepted earlier in the same height,
round, or view is pre-cutoff and a wrapper ordered after the certificate is not. The cutoff/root is
derived from the required certificate position and its parent history; it is never selected by the
certificate candidate. The typed consumer context is derived only from fields available before the
historical authority ref is inserted: it excludes every historical ref,
certificate/authorization/admission ID, certificate-derived field, final child `block_id`, release
sign-document ID, activation sign-document ID, and their authorizations. The certificate core carries
its exact canonical bytes and domain-separated context ID. A consumer verifier re-derives those bytes
from the proposal-header, release-core, or activation-sign-document prefix fields and requires byte
equality before deriving the final consumer ID. Every similarly named carried field is a byte-equal
assertion; a certificate candidate cannot select its resolver, context, position, cutoff, or root.

The parent consensus state maintains one authenticated ordered occurrence universe. Every accepted
`AcceptedRecordOuterV1` occurrence is keyed by exact parent lookup, unique core, and
`ConsensusNativeAcceptedRecordOccurrencePositionV1`. The corresponding
`ConsensusNativeAcceptedRecordOccurrenceReceiptV1` binds that live accepted position and the exact
bounded outer bytes, so byte-identical wrappers accepted at different positions have different
receipt IDs. Wrapper acceptance and insertion of that key/receipt into the universe are one atomic
consensus transition; a transition that cannot insert the unique key does not accept the wrapper.
The resulting universe root and entry count are consensus-state components, not an indexer or
certificate-builder projection. `ConsensusNativeParentAuthorityEvidenceCutoffConsensusProofV1` proves, under the
parent-profile consensus verifier, that the cutoff, occurrence-universe root, entry count, and root's
state inclusion are authorized by the certificate position's parent history.
`ConsensusNativeAcceptedRecordOccurrenceUniverseRangeCompletenessProofV1` then proves the complete
ordered-map range for the exact `(lookup, core)` prefix from the profile's lower boundary through the
inclusive cutoff. Its authenticated lower boundary, gap-free range, and upper-boundary exclusion
witnesses must establish both inclusion of every returned receipt and non-membership of any omitted
in-range key. An empty or over-bound range takes the resolver's predeclared reject/liveness
disposition; it is never truncated, sampled, or replaced by a candidate assertion.
`CompleteConsensusNativeAcceptedRecordOccurrenceUniverseV1` is the verified scoped universe result,
not the global map itself.

Only the receipt set derived from that complete range may enter
`AcceptedParentAuthorityEvidenceCommitmentV1`, which is bound with the semantic result, exact cutoff,
universe root, and required certificate position in
`AuthorizedConsensusNativeParentAuthorityCertificateV1`, then admitted only through
`ConsensusNativeParentAuthorityCertificateAdmissionV1`. Before nested work, admission checks each
declared outer length against the resolver's bound, total-decodes the exact
`canonical_accepted_record_outer_bytes` under the selected codec, canonically re-encodes them, and
requires byte equality. The decoded wrapper MUST derive the receipt's exact index key, accepted
position, parent lookup, unique core, proposal-credential ID, consensus-evidence ID, and availability-
assessment ID; changing any repeated field rejects even when a replacement receipt hash recomputes.
Admission derives exactly one `AcceptedParentAuthorityReceiptComponentProjectionV1` from every
range-derived receipt and requires its receipt-ID set to equal the complete range. The projection's
credential IDs MUST equal the complete qualifying credential-witness set; the union of its consensus-
evidence and availability-assessment IDs MUST equal the complete component-total dependency-closed
evidence set; and its dependency IDs MUST equal the complete dependency-witness manifest. Every
projected credential must qualify. Every decoded evidence and assessment component—including valid
nonqualifying or conflict material—must be committed and evaluated by the frozen status/conflict
rule; an invalid component invokes the frozen invalid-occurrence disposition and cannot disappear
from completeness accounting. `AcceptedParentAuthorityWitnessReceiptCoverageV1` is recomputed rather
than trusted. Missing components, an entire missing wrapper, missing receipts, unused extra receipts,
duplicate keys/projections, a wrong cutoff/root/boundary, or any set inequality rejects. Every
`ConsensusEntailedAvailabilityV1` entry MUST map its consensus evidence and assessment to its same
receipt through `SameWrapperAcceptedEvidenceDependencyWitnessIdV1`; detached evidence follows its
separately typed dependency mapping. Thus the live same-wrapper fact and its position exist in the
consensus universe before admission. A replay archive may recover those byte-identical committed
bytes later, but it cannot create a new live occurrence or position absent from the cutoff root.

Admission then verifies the certificate authorization, consensus evidence over the exact admission-
core ID, certificate-byte availability assessment, required position/history anchor, and attained
acceptance status. Its domain-separated admission ID is derived only after those exact evidence bytes
exist. A child, successor release, or activation may consume only an
`AcceptedConsensusNativeParentAuthorityCertificateV1` whose complete admission is available and whose
`consensus_native_parent_authority_certificate_admission_id` is bound by the consumer. For one
`(parent core, purpose, consumer context, required consensus position)` the authorized cutoff and
complete occurrence range yield one canonical evidence commitment; a partial local view is not a
second valid certificate candidate. The profile MUST accept exactly one admission at that required
position on the canonical consensus history. After acceptance, every consumer binds that exact
certificate, authorization-dependent ID, admission ID, position, cutoff ID, occurrence-universe root,
evidence commitment, purpose, and status through
`ConsensusNativeParentAuthorityCertificateRefV1`. Two conflicting admissions both
satisfying the claimed finality rule are a consensus safety fault and safe-halt/equivocation input,
not two views from which a consumer may choose. A deterministic-finality profile requires its final
admission proof; a probabilistic profile names its confirmation/common-prefix rule and makes
certificate use revert with the same reorganization semantics as that canonical history. Missing
certificate bytes or availability beyond the admitted horizon blocks consumption under the frozen
liveness fallback; it does not authorize an archive substitution. The replay archive may later
reconstruct and audit the accepted certificate's evidence, but it never selects live authority.

Powered proposal-credential controls MUST include two byte-distinct valid signatures for one
signature-free core, every arrival-order permutation, an exact duplicate, invalid/wrong-key,
wrong-suite and wrong-domain credentials, two different bytes under one credential ID, deterministic
bounded retention, and a multiplicity control proving the second valid credential adds no weight.
Field-manifest controls add, remove, or reorder every ordered core field and inject each explicitly
forbidden field—especially both proposer-signature fields. They also force two genuinely different
unsigned cores under one lookup. Collision controls parse the exact two canonical rows, then omit,
merge, reorder, singly swap, and doubly swap their conditions/dispositions. A matched pair with the
same lookup payload and old-profile `block_id` MUST take row 2 and reject with
`ProfileOwnedAuthorityBindingCompromiseV1`; different lookup-payload preimages under one protocol-
level output MUST take row 1 and reject with `ProtocolIdentityHashCompromiseV1`. Neither control may
select one core, accept a forbidden core field, or infer a third row.

Powered evidence/parent-index controls MUST include: two byte-distinct valid accepted-record wrappers
for one core, which union their evidence and still resolve one parent; an entailed assessment naming
a different wrapper's evidence; reversed wrapper order; assessment-before-evidence; invalid named
evidence; dependency eviction; exact duplicates; and a failed atomic insertion that leaves neither
half. Addition of qualifying evidence may upgrade status without changing the lookup/ref or
invalidating prior eligibility; valid nonqualifying evidence cannot create or downgrade eligibility;
verified conflicting evidence follows the frozen conflict disposition without wrapper selection.
Arrival-order permutations of every control MUST produce the same credential set, dependency-closed
evidence set, status, and disposition.

Powered certificate-universe controls MUST place a good wrapper and a separate valid conflict
wrapper for the same lookup/core before the required cutoff, then omit the entire conflict wrapper;
the authenticated range proof must reject before component projection. They also omit a benign whole
wrapper, move either wrapper across the inclusive cutoff, choose an older cutoff instead of the state
immediately preceding admission, mutate the certificate position, intra-position ordinal, parent-
history root, universe root, global entry count, index policy, receipt-set ID, or either range
boundary, remove one gap-free-map node, duplicate one index key, and substitute a proof from another
lookup/core prefix. A byte-identical wrapper accepted at a different live position MUST produce a
different receipt ID and cannot replace the committed occurrence. As a matched positive control, a
replay archive created after admission MAY recover the exact bytes and metadata of an already
committed pre-cutoff receipt; it verifies only by reconstructing that same live position, receipt ID,
cutoff, and universe-root membership. A later archive event never becomes a second live occurrence.

For `OrdinaryProfileTransitionV1`, the profile context MUST resolve to one admitted `PROFILE-1`
record whose candidate bytes, scope/genesis projections, and capability closure all verify; both
root references MUST contain that exact context. For `InPlaceMigrationTransitionV1`, the old context
MUST resolve to the parent-admitted profile, the distinct new context MUST resolve to the exact
successor profile already admitted by `PROFILE-1` after consuming the concrete `UPGRADE-1`
old-to-candidate migration evidence; both contexts MUST retain the same chain genesis and world,
and the pre/post root references MUST respectively contain the old/new contexts. The transition
context's `successor_profile_admission_manifest_id` MUST be the exact `PROFILE-1` record for the new
context, and its `successor_activation_bundle_id` MUST reproduce from the exact embedded bundle in
`AvailabilityPayload`. Admission does not activate that profile. Candidate bytes, offline migration
evidence, an admission manifest, a release record, or a bundle alone cannot self-activate. An
ordinary block cannot carry cross-profile roots, and a migration block cannot use the ordinary
same-profile rule to relabel a root.

Before any successor-bundle processing, the old envelope MUST resolve exactly one
`VerifiedSuccessorPredecessorBaseContextV1` from the same unique predecessor lookup/core and
governance source under two independent purpose-specific evaluations. This base context contains no
historical authority ref, certificate-derived value, admitted release core ID, bundle ID, or final
consumer ID. `verified_release_parent_eligibility`
MUST be a `VerifiedEligibleAcceptedParentRefV1(SuccessorReleaseAuthorityV1)` whose complete
`release_parent_authority_ref`, policy, predicate, and attained status remain release-specific.
`verified_activation_parent_eligibility` MUST independently be a
`VerifiedEligibleAcceptedParentRefV1(SuccessorActivationAuthorityV1)` whose complete
`activation_parent_authority_ref`, policy, predicate, and attained status remain activation-
specific. Their shared lookup ID, core `block_id`, admitted predecessor profile, and governance root
MUST byte-equal; their use tags, policy/predicate fields, status results, and consumer bindings MUST
NOT be coerced, copied, swapped, or required to be equal.

The successor verifier then follows one mandatory acyclic order. First it bounded-decodes the
internal closure and verifies the internal DAG only through `AdmittedReleaseCoreV1`, deriving
`admitted_release_core_id` without a release authorization. Second it derives
`SuccessorReleaseParentAuthorityConsumerContextV1`, verifies the independently accepted release-
purpose certificate, and emits `VerifiedSuccessorReleaseAuthorityContextV1`; only then may it derive
and verify `ReleaseSignDocV1`, its authorization, and `AdmittedReleaseRecordV1`. Third it completes
the internal DAG, verifies that exact admitted release, and derives `successor_activation_bundle_id`.
Fourth it derives `SuccessorActivationParentAuthorityConsumerContextV1`, verifies the independently
accepted activation-purpose certificate, and emits `VerifiedSuccessorActivationAuthorityContextV1`;
only then may it derive the activation sign-document ID and authorization. The activation certificate
or authorization is never an input to the bundle whose ID its context names.

All three contexts are typed semantic verifier results, not canonical bundle bytes: none is
serialized into or counted by `SuccessorActivationBundleV1`, and no accepted-record wrapper or
evidence-set serialization may be substituted for a result. Each staged authority context uses only
the certificate derived from its purpose-specific authorized cutoff and complete occurrence range,
then independently accepted by consensus at its required position. One purpose's result or
certificate never proves the other. A partial evidence view, different parent, old
profile, governance root, old execution root, or non-accepted certificate does not resolve the same
staged context.

`SuccessorActivationBundleV1` is the canonical, protocol-frozen, **internal-only** embedded closure
needed to verify one admitted successor relative to that already verified predecessor base context and
without an ambient registry. Its seven explicit internal preimages are the successor scope,
registry, candidate, resolved transition specification, admission manifest, admitted release, and
concrete `SuccessorUpgradeEvidenceManifestV1`. The latter is the exact `UPGRADE-1` output: it binds
predecessor and candidate, migration specification, typed old and expected-new roots,
parameter/state deltas, powered bounded replay, required implementation artifacts, and the passing
disposition. It is derived before `PROFILE-1`; the later admission manifest MUST name its ID in the
evidence closure, avoiding a backward identifier edge. The bundle's supplementary internal sequence
embeds every source, implementation, migration-specification, corpus, and other preimage transitively
needed to verify `UPGRADE-1`, `PROFILE-1`, and `RELEASE-1`.

Let `E_s` be the ordered seven-element explicit internal-preimage tuple, `S_s` the supplementary
internal-artifact sequence, `D_s` the internal dependency-node sequence, and
`InternalRefs(D_s)` all dependency refs. The successor equations are independently stated and do not
reuse launch's external-root grammar:

```text
Length(E_s)                                      = 7
supplementary_internal_artifact_count_u32         = Length(S_s)
internal_artifact_count_u32                       = 7 + Length(S_s) = Length(D_s)
internal_dependency_edge_count_u32                = Length(InternalRefs(D_s))
internal_canonical_preimage_bytes_u64              =
    SumChecked(e in E_s, Length(e))
  + SumChecked(s in S_s, Length(s.canonical_artifact_preimage_bytes))
explicit_internal_preimage_lengths_u64[i]          = Length(E_s[i])   for every i
s.canonical_artifact_preimage_length_u64            =
    Length(s.canonical_artifact_preimage_bytes)                         for every s
```

Every count, conversion, and sum is checked before allocation. Every explicit or supplementary
internal artifact occurs in exactly one `SuccessorActivationInternalDependencyNodeV1`; every typed
ID occurs once; every `SuccessorActivationInternalDependencyRefV1` resolves to exactly one node of
`D_s` with lower rank. An external/root/launch/genesis/predecessor ID is not a legal dependency-ref
variant. The launch package, genesis package, predecessor core, predecessor evidence or credential
sets, old profile preimages, old execution state, `VerifiedSuccessorPredecessorBaseContextV1`,
`VerifiedSuccessorReleaseAuthorityContextV1`, and
`VerifiedSuccessorActivationAuthorityContextV1` are all excluded from the three successor counts and the byte total and MUST NOT be duplicated as
supplementary bytes. Instead, every predecessor, chain-launch, genesis, old-profile, governance, and
old-root projection carried by an internal successor preimage MUST byte-equal the unique semantic
base context before the internal DAG can begin. The `AdmittedReleaseRecordV1` consumer's complete
predecessor authority ref MUST byte-equal `release_parent_authority_ref` and the release eligibility
result, and its release sign document MUST bind the release authority context's exact
`historical_release_parent_authority_use_ref`; after the completed DAG yields the bundle ID, the
activation sign document and authorization
consumer MUST byte-equal `activation_parent_authority_ref` and the activation eligibility result and
bind the activation authority context's exact `historical_activation_parent_authority_use_ref`. Shared core/governance
equality cannot erase either purpose-specific consumer check.

The fixed prefix, all seven explicit lengths, and every supplementary length precede variable bytes.
Prefix-first streaming preflight enforces the equations, exact enclosing frame, and protocol bounds
without allocation, hashing, nested decode, or profile-owned work. Every identifier in the fixed
prefix MUST then recompute from the corresponding carried preimage; every successor,
transition-specification, admission, release, migration-specification, evidence, expected-new-root,
and lineage projection MUST agree. The fixed `successor_activation_bundle_id` is derived with
`ProfileIdHashV1`, not with a suite selected by the inactive successor.

Powered successor controls increment/decrement every internal count and length, mutate the internal
edge total, overflow each checked sum, omit or duplicate one internal preimage, inject an external
dependency-ID variant, duplicate prior launch/genesis/predecessor bytes, substitute an ambient
registry value, and mismatch each predecessor-context projection. Authority controls omit the
release result; omit the activation result; supply only one purpose; coerce or swap either purpose
tag; change either policy, predicate, status, cutoff, universe root, or consuming reference; use
different cores with the same governance root; select one wrapper or replay checkpoint as authority;
submit a partial-view certificate omitting a pre-cutoff occurrence; substitute a certificate or
authorization ID bound to another complete universe; and preserve the same semantic parent identity
without accepting either malformed certificate. Every malformed case rejects
before successor profile resolution or migration
execution.

`SuccessorActivationSignDocV1` binds that bundle ID and exact target admission ID to the launch,
purpose-tagged `predecessor_accepted_parent_authority_ref`, exact
`historical_predecessor_activation_authority_use_ref`, old and new contexts, migration
specification, activation height, and exact old/expected-new typed roots. That historical ref MUST
byte-equal `historical_activation_parent_authority_use_ref` from the verified predecessor context;
it names the exact accepted consensus-native certificate, authorization-dependent ID, evidence
commitment, position, and activation-purpose status that qualified before these sign-document bytes
existed. A replay checkpoint is not a live activation selector.
`ResolveParentAuthorizedActivationAuthorityV1` requires
`SuccessorActivationAuthorityV1`, resolves a `VerifiedEligibleAcceptedParentRefV1` from the unique
parent core, at least one qualifying proposal credential, and a qualifying dependency-closed
evidence set, and consumes that core's committed governance/upgrade authority state, then
returns the only allowed policy, codec, scheme, and credential set. Every corresponding field in
`ActivationAuthorizationV1` is a byte-equal assertion against that resolver; neither the successor,
bundle, sign document, nor authorization object selects its verifier. The authorization covers
exactly `Domain("bitblock/successor-activation-sign-doc/v1") ||
LengthFrame(Canonical(SuccessorActivationSignDocV1))`. Its final ID is derived under
`OrdinaryProfileTransitionV1(old_active_profile_context)`, never the inactive new profile, and does
not enter its own sign document. The sign document's complete activation-purpose parent authority
ref, old root, and expected-new root MUST byte-equal `activation_parent_authority_ref`,
`verified_activation_parent_eligibility`, and the proposal's typed pre/post root references. It may
not substitute the release-purpose result even when both derive from the same core. Only the
activation-purpose certificate derived from the authorized cutoff's complete occurrence range is
admissible at the bound consensus position; every valid sign document must name that exact
certificate and authorization ID. A partial local view, distinct core, or certificate bound to a
different cutoff/root under the same lookup ID rejects.

A migration payload MUST carry exactly one `EmbeddedSuccessorActivationV1` containing the complete
bundle and authorization preimages plus their recomputed IDs. An ordinary payload MUST carry
exactly `NoSuccessorActivationV1`. Because the canonical `AvailabilityPayload` is encoded and
committed under `EnvelopeOwnerProfileContextV1`, the old profile's availability rule covers the
bytes needed to verify its successor. Before migration execution, the verifier completes bundle
preflight, closure/release/admission/upgrade verification, trusted activation-authorization
resolution, signature/certificate verification, and every equality above. Missing, unavailable,
substituted, duplicated, wrong-parent, wrong-admission, wrong-root, wrong-height, wrong-policy, or
ambient-map-only data invalidates the proposal. Only later consensus acceptance of that exact
migration block activates the admitted successor at its declared height.

`EnvelopeOwnerProfileContextV1` is the sole codec and identifier owner for the proposal envelope. It
is the active profile context for an ordinary block and the old parent-admitted profile context for
a migration block. The owner resolves the top-level protocol version, canonical header/body/receipt
codec, envelope object-hash suite, ordered-sequence commitment framing, identifier domains, and the
allowed proposal credential suites. The new profile may define post-migration execution roots and
future envelopes, but it cannot encode or hash the block that activates itself. Every `Canonical`,
`EnvelopeHashV1`, and `EnvelopeCommitOrderedV1` operation in §4.4 uses that resolution unless an
expression explicitly names a separately authority-selected policy suite. `CommitAvailability`,
`ResolveAuthorityAuthorizedConsensusEvidenceV1`, and
`ResolveAuthorityAuthorizedAvailabilityEvidenceV1` resolve through policy bytes that the same owner
and verified accepted-parent or launch branch authorizes; a header field cannot select an alternate
resolver.
The explicit proposal-credential and evidence resolvers above take that owner plus an already
verified parent-or-launch authority object. They MUST NOT consume an unverified header policy ID as
authority. The selected evidence codec and hash suite, like the proposal signature and signing-hash
suite, are therefore deterministic projections of the owner and verified authority branch rather than
candidate choices. `evidence_scheme_id` is only a bounded lookup key into the already verified
availability policy: resolution MUST return exactly one authorized scheme with the same identifier,
and an unknown, duplicate, disabled, or mismatched scheme rejects before evidence verification.

The first supported migration body is deliberately dedicated. For
`InPlaceMigrationTransitionV1`, `Body` MUST be `DedicatedMigrationBodyV1`,
`SignedTransactionSequence(Body)` MUST be the canonical empty sequence,
`application_subtick_schedule` MUST be empty, and `system_transition_schedule` MUST contain exactly
one `AuthorizedMigrationSystemTransitionV1(successor_profile_admission_manifest_id,
migration_spec_id, successor_activation_bundle_id, activation_authorization_id,
activation_height)`, with every field byte-equal to `ProfileTransitionContextV1`. Its deterministic
inputs are exactly those required by the admitted migration
specification, and execution emits exactly one position-specific migration system receipt and no
transaction receipt. An ordinary context MUST use `OrdinaryBodyV1`. A future block that mixes user
transactions with migration would require a separately versioned, profile/phase-tagged body and a
total ordering rule; this contract grants no such interpretation.

`Execute(state, BlockContext, Body)` MUST consume the selected branch completely. The ordinary
branch consumes every ordered transaction, every deterministic auxiliary-input value, and every
scheduled ordinary system invocation exactly once. The migration branch consumes every migration
input exactly once under the admitted specification and emits the one required system receipt.
`BlockContext` is the sole owner of envelope/height metadata, committed randomness, and schedules;
the selected body variant is the sole owner of transaction, auxiliary, or migration value bytes.
An implementation may use a context ordinal to address a body value, but it may not source the
value from both objects, reread an oracle, infer a default, ignore an extra, or reinterpret a value
under the other branch. The function returns the complete ordered effects/receipts with the
post-state or returns `BlockInvalid`; it has no partial-success or unconsumed-input case.

The canonical `BlockContext` preimage is itself part of `AvailabilityPayload`, so the availability
commitment covers the exact bytes whose `block_context_root` appears in the header. Before
execution, the verifier MUST require its protocol, chain, genesis, launch manifest, transition
context, world, height, parent, epoch, and round/view projections to equal the already verified
header and accepted-parent or launch authority; enforce the selected schedule/body relation; and
recompute `block_context_root`. Withholding that context, substituting another preimage with any
repeated field or schedule changed, or supplying context bytes whose canonical re-encoding or root
differs invalidates the proposal and cannot satisfy its availability assessment. Receipts and
witnesses without those context bytes are not a complete replay payload.

The powered transition corpus includes a valid ordinary case and a valid dedicated migration case,
then removes the context, substitutes each repeated context projection and each schedule, swaps an
ordinary/migration body, duplicates one deterministic value across context and body, appends an
unconsumed input, and removes one required input. Successor controls also omit or substitute each
bundle/authorization preimage and ID, change the target admission, predecessor, migration spec,
activation height, old/new root, authority policy/scheme/credential, or make a required artifact
available only through an ambient map. Every mutant must reject before execution or root
publication; no executor-local clock, cache, registry, or default may make it pass.

A reproducible `RELEASE-R1` candidate build does not satisfy either admission lookup, while artifact
deployment separately requires a matching `RELEASE-1` record. Profile IDs remain outside logical
execution-root preimages but inside the typed root references, `block_id`, and proposal signatures.
They contextualize roots without making a candidate identifier depend on a root that contains
itself. The top-level `protocol_version`, chain, genesis, and world fields MUST equal their exact
projection from `EnvelopeOwnerProfileContextV1`. For a migration, chain, genesis, and world MUST
also byte-equal their projections from the distinct new execution context, while the top-level
protocol version is exactly the old owner's envelope version. `chain_launch_manifest_id` MUST equal
the verified launch package for the chain and is retained unchanged across ordinary and migration
blocks.

`execution_parameter_root`, `governance_authority_root`, and `active_validator_root` are the
authority-selected values governing this proposal: launch-authorized initial values for the first
record and accepted-parent-authorized values thereafter. `optional_next_validator_root` and the
post-execution root commit successor changes. No header label may ambiguously mean both a
pre-transition authority and a post-transition result. `execution_policy_id` in each receipt
names the exact versioned execution, no-op, fee, and failure rules selected by
`execution_parameter_root`. `consensus_policy_id` and `data_availability_policy_id` likewise
name uniquely versioned policies authorized by that verified authority context;
`optional_next_validator_root` MUST equal the delayed-next schedule committed in the
post-execution `Q` state.

`proposer_identity`, key ID, and signature/hash suite IDs MUST resolve to exactly one active
proposal credential in the envelope owner's authority-selected validator record. A proposer cannot
select an alternate key, codec, object hash, or downgrade suite merely by changing header fields.

Every field of `AvailabilityClaimCoreV1` MUST equal the corresponding header and authority-selected
policy/authority context. `ConsensusEntailedAvailabilityV1` is valid only when the selected
consensus policy explicitly makes that exact predicate and status a verified consequence of the
named `ConsensusEvidence`, that evidence is the exact consensus segment carried by the same
`AcceptedRecordOuterV1`, its recomputed ID equals the assessment reference, and the pair entered the
normalized set atomically with its retained `SameWrapperConsensusDependencyWitnessV1`; otherwise
detached evidence is mandatory. Cross-wrapper lookup, a pending assessment, or later evidence cannot
repair a failed entailed pair. Detached evidence is bounded-decoded and verified under the uniquely
resolved authority-selected availability evidence scheme. A payload
commitment alone, possession by one verifier, or a node-local retrieval observation cannot populate
either canonical variant. Such observations remain local diagnostics outside `AcceptedBlockRecord`
and cannot overwrite its assessment. `ConsensusHead.availability_assessment_ref` is the matching
tagged `AvailabilityAssessmentRefV1` projection selected by the deterministic profile-owned status
join. Its verified evidence entry binds the claim core to that reference; no one accepted-record
wrapper is the canonical parent identity.

```text
execution_policy_id = EnvelopeHashV1(
    profile_transition_context,
    execution_policy_domain,
    Canonical(execution_parameter_root)
)
consensus_policy_id = EnvelopeHashV1(
    profile_transition_context,
    consensus_policy_domain,
    Canonical(execution_parameter_root, governance_authority_root)
)
data_availability_policy_id = EnvelopeHashV1(
    profile_transition_context,
    availability_policy_domain,
    Canonical(execution_parameter_root)
)
```

`AcceptedBlockRecord` is a neutral carrier name. A deterministic-finality profile may call
it finalized only after its finality rule passes. A probabilistic profile must retain its
confidence/confirmation status and cannot turn work ancestry into absolute finality by using
this envelope. The carrier is one evidence occurrence, not a unique database row: parent identity
is the indexed `AcceptedProposalCoreV1`, and authority is the profile-owned projection of its
`AcceptedProposalCredentialSetV1` and dependency-closed `AcceptedEvidenceSetV1`.

A valid proposal requires this explicit checklist to pass:

1. apply the profile-independent bounded outer framing, fixed-width length/count checks, and checked
   preallocation arithmetic before nested decoding, hashing, signature/proof work, or allocation;
2. resolve exactly one authority branch from the bounded outer reference. For an
   `AcceptedParentLookupV1`, resolve exactly one indexed proposal core, recompute its fixed lookup
   payload and ID, normalize its verified proposal-credential and dependency-closed evidence sets,
   require a qualifying proposal credential, and satisfy the monotone purpose-specific
   `ParentAuthorityEligibilityV1` predicate. For an `InitialLaunchLookupV1`, verify
   the complete launch closure and detached launch authorization plus all
   first-height/genesis-parent/initial-root conditions, without applying an accepted-parent
   consensus predicate;
3. derive the expected `EnvelopeOwnerProfileContextV1` from that tagged authority and immediately
   resolve `AuthorityOwnedParseAndWorkLimitsV1`, the owner codec/object-hash suite, and the maximum
   permitted suite-specific work without consulting candidate-selected fields;
4. read the fixed `ProposalInnerShallowFrameV1` prefix and run allocation-free streaming preflight
   over its independently length-framed header, availability-payload, and signature slices. Enforce
   global and authority-owned byte/count/nesting/decompression/proof/witness/signature/work bounds
   before materializing, copying, hashing, canonical re-encoding, decompression, or cryptographic
   verification;
5. materialize and bounded-decode only the canonical header slice, re-encode it to the identical
   slice, and require its parent block ID and lookup-bound launch/genesis/profile/suite projections
   to match the uniquely retrieved parent—or its genesis-parent projection to match the authorized
   launch branch—then verify launch-manifest identity, height, epoch, round/view, parent history,
   chain/genesis/world continuity, ordinary-or-migration context, candidate lineage, and typed
   pre/post root projections;
6. resolve the authority-selected execution parameters, governance authority,
   validator schedule, consensus/availability policies, and allowed credential suites from the
   verified launch package at the first height or the verified accepted parent thereafter; require
   every corresponding header field to match before it can select work or interpretation;
7. materialize and bounded-decode the already preflighted canonical availability-payload and
   proposer-signature slices exactly once under those resolutions; enforce every semantic header,
   body, transaction, receipt, effect, witness, proof/signature, state-growth, and execution bound;
   re-encode each slice, rebuild the fixed shallow frame byte-for-byte, and verify proposer
   eligibility and signature over the bounded header;
8. for an ordinary transition require `NoSuccessorActivationV1`; for a migration require exactly
   one embedded bundle and authorization, recompute both IDs, verify the complete successor
   admission/release/upgrade/implementation closure and trusted parent-authorized sign document,
   and reject any missing, substituted, unavailable, or ambient-only artifact before execution;
9. require every `BlockContext`, `Body`, receipt/effect, witness, and successor-carrier projection to
   match the verified header and authority, recompute
   `block_context_root` before execution, enforce the ordinary or dedicated-migration branch and
   one-owner input partition, and recompute candidate execution context, body/transaction roots,
   exact availability payload, and `data_availability_commitment` under the already verified
   policies;
10. evaluate the one total `Execute(typed_pre_state, BlockContext, Body)` relation under the already
   verified execution or migration policy;
11. require its complete ordered effects and receipts to byte-equal the claimed payload, then
    recompute `receipts_root`, typed post-state root, and `post_history_root`; and
12. require optional-next-validator and every other explicitly permitted successor value to equal
    the recomputed post-state role. No pre-transition authority or policy check is deferred to this
    post-state phase.

Turning that valid proposal into an `AcceptedBlockRecord` additionally requires profile-specific
`ConsensusEvidence` for the exact claimed finality or confirmation status and a valid
`AvailabilityAssessmentV1` for the exact profile-required availability predicate. Accepted-parent
status/authorization or first-launch closure/authorization is the mutually exclusive anchor check;
execution validity, the candidate's own consensus status, and availability are separate checks. A
mismatch in any required field invalidates the proposal or accepted record rather than producing a
partial successor.

The accepted-record carrier has the same topological input discipline. Its profile-independent
outer framing MUST bound the proposal, consensus-evidence, availability-assessment, proof/signature,
and canonical-evidence byte lengths before nested decode, hashing, allocation, or verification.
Immediately after the authority branch resolves, the same authority-owned limits MUST preflight the
detached evidence slices before they are materialized. Only after the proposal path establishes the
accepted parent or launch anchor, envelope owner, resource limits, context/root correspondence, and
authority-selected consensus/availability policies may the verifier bounded-decode the two detached
evidence objects under their uniquely resolved codecs and schemes.
It then hashes and verifies them against the exact `block_id`, status, predicate, authority/epoch,
and policy fields. No unverified evidence variant, scheme ID, codec ID, hash-suite ID, length, or
status may select its own parser or verification work.

Each included transaction and deterministic system transition has a canonical,
position-specific receipt:

```text
effects_root_j = EnvelopeCommitOrderedV1(
    profile_transition_context,
    execution_effect_sequence_domain,
    effects_in_subject_order_for_receipt_j
)

ReceiptSubject =
    UserTransaction(tx_id, transaction_index)
  | SystemTransition(system_phase, system_rule_id, system_transition_ordinal)

ReceiptCore = Canonical(
    ReceiptSubject,
    included_height,
    result,
    pre_subject_state_root_ref,
    post_subject_state_root_ref,
    nonce_disposition,
    fee_disposition,
    effects_root_j,
    execution_policy_id
)

Receipt = Canonical(ReceiptCore, effects_in_subject_order)

OccupancyEffect = Canonical(
    occupancy_effect_tag,
    effect_index,
    optional_source_action_index,
    event_position,
    status,
    coordinate,
    pre_occupancy,
    post_occupancy,
    optional_mint_id,
    optional_consumed_mint_id,
    optional_burn_event_id
)

Effect =
    OccupancyEffect
  | OwnershipOrTransferEffect(...)
  | BalanceFeeRewardOrRentEffect(...)
  | AuthorizationEffect(...)
  | GovernanceOrUpgradeEffect(...)
  | ValidatorScheduleEffect(...)
  | other versioned tagged execution-domain effect
```

`receipts_root` commits the full ordered receipts, so it transitively binds each effect list
and its per-receipt `effects_root_j`. `block_id` is deliberately not inside a receipt
whose root helps form that identifier. A
receipt-inclusion proof links the receipt and its canonical subject position to `receipts_root`; an
authenticated header links that root to `block_id`; detached consensus evidence establishes
its consensus status. Execution, inclusion, finality, and availability remain four separate
claims. A receipt hash alone proves only the receipt bytes and schema.
For a `SystemTransition` receipt, `nonce_disposition` and `fee_disposition` are the canonical
`NotApplicable` variants; rewards, rent, migrations, and other system changes appear as
tagged effects instead.

### 4.5 Genesis, versioning, and upgrades

`ProfileScopeV1` MUST contain the selected transition/schema versions, the canonical pre-ID domain
registry, cryptographic suites, consensus/governance/role choices, optional feature set, and
proposed limit vector. `ChainGenesisPayloadV1` MUST reference the initial profile's scope and
domain-registry digest and contain, at minimum:

- `chain_id` and the first profile's single `world_id`;
- the origin coordinate and complete initial mutable world/episode state;
- initial authorization, parameter, governance, and validator-schedule state;
- the validated resource, fee-if-activated, and size limits; and
- bootstrap and availability policy.

An initial candidate profile references `profile_scope_digest`, `domain_registry_digest`, the
immutable `genesis_id`, and `InitialProfileV1`; its scope and registry MUST equal the corresponding
genesis-owned launch values. An in-place successor instead references the same `genesis_id`, its own
scope/registry, and `InPlaceSuccessorProfileV1(predecessor_admitted_profile_id)`. It does not derive a
second genesis or require the launch package to claim that its later scope existed at height zero.
Any denormalized suite, role, feature, limit, state-domain, or root-schema value must be the exact
canonical projection of its current owner.

For launch, `genesis_id` and then the initial `profile_id` are derived in that order; the transition
specification, admission record, root-parity record, admitted release, and `chain_launch_manifest_id`
follow without entering either earlier preimage. For an in-place
successor, the immutable `genesis_id` and predecessor profile already exist before the new candidate
`profile_id` is derived. `TRANSITION-SPEC-1@profile` is then produced separately by resolving that
candidate's active extension slots against the base contract and validated bounds; the candidate
does not pre-commit a self-dependent resolved-specification digest. Every block, proof, and signed
action MUST identify the exact profile context that interpreted it. Unknown versions, an unadmitted
ordinary profile, a migration with an unknown predecessor or a target not already admitted through
the required `UPGRADE-1` evidence, missing chain-genesis or chain-launch
data, unequal projections, and incompatible parameter roots MUST fail closed.

Completing launch bytes is not genesis-root evidence. `GENESIS-ROOT-PARITY-1` rebuilds the initial
profile-required root tuple independently from `ChainGenesisPayloadV1`; only after that gate may the
initial candidate publish roots used by block-zero/header verification. A successor profile does not
rerun that launch claim: `UPGRADE-1` independently derives its new typed roots from the accepted old
state through the migration relation. Root parity and admission still do not launch the chain by
themselves: the canonical launch manifest and detached launch authorization bind the one root/profile
combination accepted by bootstrap and the first header.

An upgrade is a deterministic, consensus-accepted state transition, not a changed constant in one
binary. Its `InPlaceMigrationTransitionV1` and typed root references MUST bind the old and new
protocol/profile/scope/registry contexts, activation height, migration specification or code hash,
old root, expected new root, parameter deltas, exact target admission-manifest ID,
`successor_activation_bundle_id`, and detached parent-authorized activation proof. The old context
remains the parent authority for envelope codec/hash, proposer, governance, consensus, availability,
and bounded preflight during this dedicated transition unless an admitted upgrade framework
specifies an equally explicit handoff. The concrete `UPGRADE-1` bounded replay is verified before
`PROFILE-1` admits the successor. The old-envelope availability payload must then embed the complete
successor closure and activation authorization; a migration-block validator verifies those bytes and
replays that already admitted dedicated transition from the exact parent root before accepting its
new root. Only consensus acceptance activates the successor. Bedrock, supply, ownership, and
authorization changes must appear as explicit deltas
rather than emerge from serialization drift. A
`PreventiveProfileOwnedCryptographicMigrationV1` must prevent downgrade, define any dual-validation
window, preserve verification of historical blocks, and prove the exact last-safe parent plus every
authority binding still sound before authorization; it cannot migrate `ProfileIdHashV1`. Once
`ProfileOwnedAuthorityBindingCompromiseV1` is established, this ordinary successor path is forbidden
in the current protocol. The reserved `PrecommittedIndependentRecoveryEnvelopeV1` is uninhabited,
so `CurrentEstablishedAuthorityCompromiseDispositionV1` requires safe halt and separately trusted
new genesis. Only a separately specified future `FutureIndependentRecoveryExtensionV1` could alter
that rule after satisfying every prerequisite above. Emergency and
rollback policy cannot silently rewrite history that the selected rule treats as irreversible.

Cross-world or cross-chain transfers MUST be explicit bridge transactions with their own
proof and trust boundary; matching coordinate names do not imply shared state.

### 4.6 Programmability boundary

The current edit language is data entry, not executable code: blocks are memory and ticks
are the execution clock, but no block transforms another block autonomously. If the future
visual language adds programs, its instruction set becomes another consensus surface. It
will need a deterministic interpreter, a versioned grammar, bounded execution/gas, memory
limits, failure semantics, and a refinement proof or independent reference executor.

Program execution MUST remain separate from rendering and camera state. A visual pattern
can be a program input or a projection, but a pixel pattern is not authority. Randomness,
if a later rule needs it, MUST be supplied by an explicitly committed and bias-resistant
protocol source; wall-clock timing or a renderer seed is not consensus randomness.

## 5. Authenticated state commitments

The current world tag is a canonical content identity. A Layer 1 needs a commitment that
also supports independently verifiable membership and non-membership claims.

A proposed root hierarchy is derived from the pre-ID registry rather than from a hard-coded list of
optional roots. `CommitmentSetV1` holds roots only for active `RequiredDomainV1` values; composition
walks every registry slot and supplies a typed sentinel for each inactive one:

```text
required_domain_set = CanonicalSorted([
    entry.RequiredDomainV1
    for each ActiveDomainV1 entry in DomainRegistryV1
])

application_domain_entries = ComposeRegistrySlots([
    RootEntryV1(RequiredDomainV1, root_bytes)
  | InactiveDomainSentinelV1(domain_tag, composition_layer,
                             inactive_sentinel_schema_id)
    for each application-layer entry in DomainRegistryV1
], CommitmentSetV1.root_entries)

application_root = H(Canonical(
    application_state_domain,
    protocol_version,
    chain_id,
    genesis_id,
    world_id,
    profile_scope_digest,
    domain_registry_digest,
    application_domain_entries
))

protocol_domain_entries = ComposeRegistrySlots([
    RootEntryV1(RequiredDomainV1, root_bytes)
  | InactiveDomainSentinelV1(domain_tag, composition_layer,
                             inactive_sentinel_schema_id)
    for each protocol-layer entry in DomainRegistryV1
], CommitmentSetV1.root_entries)

execution_state_root = H(Canonical(
    execution_state_domain,
    protocol_version,
    chain_id,
    genesis_id,
    world_id,
    profile_scope_digest,
    domain_registry_digest,
    application_root,
    protocol_domain_entries
))
```

The base occupancy profile requires world, active-token, episode, authorization, nonce, parameter,
governance, and `VALIDATOR-SCHEDULE-1` domains. `active_token_root` commits `M_h`;
`episode_root` commits `E_h` under the selected history and pruning policy; authorization commits
current descriptors, delegations, rotation, revocation, and recovery; the validator-schedule domain
commits active and delayed-next schedules but not current-round votes. If a profile activates
program state, `PROGRAM-STATE-1` owns its typed domain and root. If it activates an asset or wider
economic state, the selected asset decision plus `ECONOMIC-STATE-1` own those typed domains;
validator incentive semantics are first specified separately by `VALIDATOR-ECON-D1`.

An inactive asset, program, fee, reward, stake, or other optional domain occupies its registry slot
with `InactiveDomainSentinelV1`; it has no `RequiredDomainV1`, root bytes, or valid proof. A domain
may instead be active with an empty root only if its owning decision defines one typed empty state
and canonical empty value. The composer never hashes an undefined placeholder. Entry tags are
unique and registry ordered. A missing required tag, duplicate tag, unknown tag, wrong layer/type/
tree-domain/root-schema/commitment-profile, active/inactive flip, root supplied for an inactive slot,
sentinel supplied for an active slot, or different ordering rejects. The required set, registry
digest, and every repeated scope/genesis/profile projection must match exactly.

The `commitment_profile_id` inside each `RequiredDomainV1` is the self-excluding identifier for that
domain's selected authenticated-map construction and proof policy under `COMMIT-1`; it is not the
candidate network `profile_id`. A network `CommitmentSubjectV1` binds the later `profile_id`,
`profile_scope_digest`, and `domain_registry_digest` externally to the exact active root set.

`profile_id` is deliberately not in `application_root`, `execution_state_root`, or any map-root
preimage. Including it there would make root derivation depend on the candidate identifier while the
candidate's own projections describe the root domains and schemas. Scope/genesis identifiers,
protocol version, authenticated parameter roots, and typed domain entries carry execution semantics.
`ExecutionRootRefV1`, `ProposalHeader`, transaction signing documents, release metadata, and
`AuthenticatedRootRefV1` carry the profile ID as external context and MUST project the same
scope/genesis/version/root choices. Ordinary pre/post references use one admitted context; migration
pre/post references use the explicitly named old/new contexts. This avoids a profile/root identifier
cycle without allowing two profiles to reinterpret one root.

`chain_launch_manifest_id` is likewise excluded from the initial application/execution root,
genesis-parity record, and initial `RELEASE-1` preimages: all three must exist before the manifest ID
can be derived. The manifest binds those already-derived objects, and transaction signing documents,
proposal headers, accepted records, checkpoints, and bootstrap inputs carry its ID externally. This
one-way binding avoids a launch self-hash cycle without letting a verifier substitute another initial
profile, transition specification, parity record, release, or root under the same `genesis_id`.

The `ProposalHeader` in §4.4 binds those typed roots to the ordinary-or-migration profile context,
parent, transaction/receipt/history commitments, consensus policy, active validator set,
governance/parameter roots, and data-availability commitment. `block_id` is the header identifier;
`ConsensusEvidence` and the policy-bound `AvailabilityAssessmentV1` are detached from that identifier
and carried by `AcceptedBlockRecord`.

Application roots, execution-state roots, block identifiers, and consensus evidence must not
be interchangeable. A valid world root is not a valid finality certificate, and a valid vote
certificate is not a proof that a coordinate is occupied.

The current `COMMIT-R1` comparison keeps one controlled choice: a constrained fixed-key binary
Patricia trie versus a 192-level sparse Merkle tree. Either must define canonical child ordering,
empty-subtree values, coordinate encoding, and both occupied and absent proofs. No structure is
selected yet.

### 5.1 Separate Verkle feasibility lane

The user's Verkle question opens `VERKLE-R1`, a separate feasibility lane rather than a third arm
silently added to the controlled `COMMIT-R1` assay. The original
[John Kuszmaul, “Verkle Trees” paper](https://math.mit.edu/research/highschool/primes/materials/2018/Kuszmaul.pdf)
develops the use of vector commitments in a high-branching authenticated tree to reduce proof
bandwidth. The
[Ethereum Foundation's concrete structure overview](https://blog.ethereum.org/2021/12/02/verkle-tree-structure)
illustrates a 256-way design using vector commitments and a stem/suffix layout. Those sources
motivate a hypothesis for Bitblock—smaller aggregated witnesses for many coordinate or state
queries—not a selected construction or inherited security result.

`VERKLE-R1` must first pin the vector-commitment scheme, curve/field and transcript rules, opening
and batch-verification equations, key/value layout, absence and deletion semantics, canonical
encoding, and numeric limits. It must measure witness bytes and verification work against the same
logical maps and workloads as the binary candidates. The cost side must separately report update
and witness-generation work, group/field operations, storage churn, cache behavior, and worst-case
accepted inputs; a small witness does not imply cheap mutation or verification.

The lane must also account for the larger cryptographic and trusted-computing surface: generator
derivation or scheme-specific setup/custody assumptions, implementation and side-channel risk,
independent-library diversity, external review, historical-proof retention, and migration from any
already admitted root. A Verkle prototype cannot reuse Ethereum's key layout, curve choices,
security analysis, or deployment experience as Bitblock evidence by citation. It remains
research-only until its own canonical vectors, powered malformed-opening controls, independent
implementation, refinement story, cost measurements, and reversal/migration plan pass. Failure to
beat the selected simpler baseline by a predeclared material threshold closes the lane without
changing live authority.

This is also where the practical-infinity idea meets cryptography. Lazy chunks are good
storage engineering, but a commitment cannot leave “unloaded” ambiguous. Every coordinate
must have a deterministic default value, and every touched region must have a bounded proof
path. The current signed-64-bit domain is large and finite; a later wider or mathematically
unbounded coordinate model would require a versioned commitment format.

The current `world_tag` is a full sorted-coordinate hash. It has no durable world decoder and
no membership or non-membership witness, so proving one coordinate currently requires more
than presenting the tag. An authenticated map is a new data structure and migration, not a
new name for that local digest.

The user's intended balanced-trit hashing implementation and contract have not been located
in this repository. Separately, the examined Aethor TSH-512/PDH-512 constructions are already
disqualified from authority here: the recorded audit found a concrete padding collision,
unframed node composition, and an unused PDH depth argument. IterHash is diagnostic only and
makes no security claim. Those negative results do not evaluate an unidentified balanced-trit
construction. Any future ternary candidate must pin its source, alphabet, output size,
security claim, canonical framing, domains, independent implementation, adversarial vectors,
and cryptanalysis before a versioned migration is considered. Agreement with itself cannot
promote it into authority.

Every proof object SHOULD state its type and scope:

- **membership** — a coordinate/token/nonce/authorization or receipt is included under a
  named root and position;
- **non-membership** — an absent key has a scheme-specific non-membership witness;
- **execution** — an ordered block body and committed `BlockContext`, starting at the
  authority-selected pre-state root (the launch-authorized initial root at the first height and the
  accepted-parent execution root thereafter), deterministically yield the complete receipt and
  state-root sequence;
- **receipt inclusion** — a receipt path reaches the header's `receipts_root`, and the
  authenticated header reaches `block_id`;
- **consensus status** — profile-specific evidence covers one `block_id` under the validator or
  sequencer authority selected by the verified launch branch at the first height or by its accepted
  parent thereafter;
- **availability** — the bytes needed to replay an accepted block can be retrieved under
  the stated guarantee.

The verifier MUST reject a proof presented under the wrong root kind, schema, chain, genesis,
admitted profile, world, height, or protocol version. A compact digest without the corresponding witness is
only an identity, not a membership or availability proof.

## 6. Replication, consensus, and finality

Local parent-linked history explains how one process derived a state. Consensus answers a
different question: which history independent processes accept. A versioned claim MUST
separate at least these properties:

In the remainder of this paper, “finalized” means irreversible under a selected
deterministic-finality rule. A probabilistic profile must substitute “reached the declared
confirmation threshold,” carry the residual reversal probability and horizon, and must not
reuse absolute-finality wording.

- **execution validity** — every accepted successor is the deterministic result of the
  declared parent, context, body, and version;
- **prefix safety** — any two deterministically finalized histories accepted under the claim
  are prefix-comparable, not merely non-conflicting at one height; a probabilistic profile
  states the corresponding common-prefix probability and horizon;
- **finality** — the exact rule after which a block is irreversible under the stated fault
  assumptions, or the confirmation/confidence rule if finality is probabilistic;
- **availability** — consensus-required bodies and witnesses are retrievable under the
  declared retention, coding, sampling, and network assumptions;
- **chain liveness** — new valid blocks continue to finalize or reach the declared
  confirmation threshold after the stated synchrony condition while faults remain within
  the bound;
- **transaction inclusion/fairness** — the narrower conditions, if any, under which an
  eligible transaction is included;
- **accountability** — conflicting signed evidence can be attributed where the selected
  construction supports it; and
- **cross-implementation agreement** — conforming executors obtain the same receipts and
  roots for the same accepted inputs.

An **eligible transaction** is canonical, continuously available to the protocol's required
honest network fraction, valid or correctly resubmitted against canonical state, within its
validity window, paying the declared admission price, and submitted while sustained offered
load is within the measured capacity envelope after synchrony. Outside that envelope, the
protocol may claim only its explicit queue, expiry, and priority behavior—not eventual
inclusion. “Unless censored” cannot make liveness unfalsifiable.

All assumptions must be quantitative enough to test. They include the network model, the
maximum Byzantine and offline weight, quorum intersection, corruption timing, capacity,
and cryptographic hardness. “A majority is honest” is incomplete until the protocol says
which majority, over which authority-selected set, for how long, and how membership changes.

A consensus protocol MUST specify:

- validator or sequencer membership and admission;
- the fault model and maximum faulty fraction;
- proposal and vote messages;
- round/view-change, timeout, and pacemaker behavior;
- quorum and certificate encoding;
- fork choice and equivocation handling;
- finality and recovery after partitions;
- validator key rotation and revocation;
- resource and fee policy for spam resistance;
- how a new node obtains and verifies the canonical history.

### 6.1 Finality, forks, and validator-set changes

For a certificate-based construction, each vote signs an unambiguous domain:

```text
VoteSignDoc = Canonical(
    vote_domain,
    protocol_version,
    chain_id,
    genesis_id,
    epoch,
    active_validator_root,
    height,
    round_or_view,
    vote_phase,
    block_id
)
```

Certificate verification MUST reject duplicate signers or duplicated voting weight, derive power
from the authority-selected active set—the launch-authorized initial schedule at the first height
or the schedule authorized by the parent finalized under that certificate profile thereafter—and
use exact checked threshold
arithmetic. Aggregation must retain signer attribution and apply the selected scheme's
public-key validation and proof-of-possession defenses. A candidate cannot choose the set
that certifies itself. A block may commit an `optional_next_validator_root`, but delayed
activation and a specified cross-epoch handoff authorize it only from a prior finalized
state.

Each validator vote key and signature/hash suite MUST resolve uniquely from the
authority-selected active-validator record and `consensus_policy_id`; a vote message cannot
select an alternate key or downgrade suite.

These bindings are mandatory for a quorum-certificate design even though the consensus
choice remains open. A probabilistic work-based design instead needs a precise work/fork and
confirmation rule and must not label confidence evidence a certificate. A node MUST NOT
treat a state root as final merely because it is newest or locally observed unless that is
the declared consensus rule.

A validator MUST NOT vote for execution validity or finality until it has obtained all
consensus-required execution bytes, validated resource limits, and recomputed the transition,
unless the selected protocol separately specifies a sound validity-proof system bound to the
same `block_id`, body, receipts, and post-state root. Data-availability sampling or a DA
certificate never substitutes for execution validity.

A separately typed availability attestation may use sampling instead of full possession only
under a declared construction. It must state the sampling threshold, erasure-code and
honest-share assumptions, recovery threshold, withholding horizon, commitment-to-payload
binding, and exact weaker guarantee. A combined validity/finality vote must satisfy both the
execution rule and the DA rule. A public claim must say whether finality entails availability
or requires separate evidence.

In a deterministic-finality profile, a branch can be replaced before finality; after
finality, a conflicting root is a safety violation rather than an ordinary reorganization.
In a probabilistic profile, reorganization remains possible with the bounded probability and
tail described by its common-prefix/confirmation claim. Every profile MUST specify orphan
handling, transaction re-inclusion, nonce behavior, receipt status, and visibility of
provisional mint/burn records. Validator-set changes need an activation rule, overlap or
handoff proof, key revocation behavior, and protection against long-range or stale-checkpoint
attacks.

### 6.2 Ordering fairness and censorship

Consensus ordering is an authority surface. A proposer or sequencer can reorder, delay, or
exclude transactions even when it cannot forge signatures. The protocol SHOULD define
eligible-transaction conditions, inclusion deadlines, fee/priority rules, duplicate
suppression, maximum queue age, and an evidence path for censorship or equivocation. If no
fairness guarantee is intended, the liveness claim must say so explicitly.

Proof of History-like sequential clocking is not consensus or finality by itself. Proof of
Stake is a Sybil/economic weighting mechanism commonly combined with a finality protocol;
BFT is a fault-tolerant agreement family; Proof of Work supplies a work-weighted fork rule;
and a trusted central sequencer is an explicit single-authority construction. The current
SHA-256 history chain is none of these. A cheap Rust implementation can begin with one
authority and then a deterministic multi-node testnet, but staging is not decentralized
security evidence.

If a future design uses stake, it must define deposits, validator selection, unbonding,
slashing, evidence validity, recovery, and the attack cost. If it uses work, it must define
difficulty, chain-work comparison, issuance, and energy/security assumptions. If it uses
permissioning, the admission authority and compromise response are part of the trust model.
The choice remains open until the desired decentralization, cost, and fault model are
measured rather than assumed.

## 7. Threat model and required defenses

The protocol MUST name its adversary before selecting a consensus mechanism. Every versioned
safety, liveness, availability, custody, or economic claim cites the applicable parts of:

```text
AdversaryModel = (
    security_parameter_and_computational_bounds,
    static_or_adaptive_corruption_and_corruption_latency,
    maximum_byzantine_weight_and_maximum_offline_weight,
    network_power_to_observe_drop_delay_reorder_duplicate_partition_or_eclipse,
    key_client_rpc_storage_release_and_governance_compromise_powers,
    economic_or_bribery_budget_and_security_time_horizon,
    physical_and_side_channel_assumptions_or_exclusions
)
```

The initial threat inventory is:

| Threat | Required defense or explicit assumption |
| --- | --- |
| Forged action | Signature verification and canonical signed bytes |
| Ambiguous or malleable encoding | Typed length framing, strict decoding, canonical signatures, and negative vectors |
| Replay | Per-identity nonces, chain/genesis/launch/profile/world binding, selected anchor rules, and bounded retention |
| Double spend / duplicate occupancy | Single canonical state transition and live-token checks |
| Same-coordinate race | Consensus-defined ordering and deterministic conflict results |
| Validator equivocation | Signed votes, conflicting-certificate detection, and penalty/expulsion policy if applicable |
| Fork or rollback | Declared consensus/finality rule and evidence, checkpoint policy, and recovery rules |
| Data withholding | Full transaction availability, archival policy, and retrievability proofs or explicit trust boundary |
| Sybil participation | Admission, stake, proof-of-work, permissioning, or another declared cost |
| Spam and unbounded coordinates | Fees, quotas, work budgets, chunk limits, and allocation-failure handling |
| Malicious implementation | Independent executor, differential replay, fuzzing, and formal refinement where practical |
| Preventive deprecation of a still-sound profile-owned hash, signature, proof, or credential suite | Versioned admitted suite, conservative assumptions, and parent-authorized `PreventiveProfileOwnedCryptographicMigrationV1` only from one exact last-safe parent while every live authority binding remains independently sound |
| Established collision, forgery, or authority ambiguity in a live profile-owned role | `ProfileOwnedAuthorityBindingCompromiseV1`: current `CurrentEstablishedAuthorityCompromiseDispositionV1` always safe-halts to separately trusted new genesis; the reserved recovery-envelope type has no current constructor or acceptance vector |
| `ProfileIdHashV1` collision or second-preimage compromise | `ProtocolIdentityHashCompromiseV1`: terminal safe halt and new genesis under a separately trusted recovery anchor; the compromised identity chain cannot authorize its own successor |
| Key loss or compromise | Rotation, recovery, revocation, and incident procedures |
| Validator restore or signer failover | Durable anti-double-sign state, restore monotonicity, and slashing/equivocation protection |
| Clock manipulation | Logical height for validity; wall-clock only for scheduling/timeouts |
| Client/render mismatch | Keep pixels non-authoritative; validate submitted semantic actions |
| State corruption | Checksummed storage, parent-root validation, snapshots, and restore tests |
| Malformed peer or parser exhaustion | Strict codecs, bounded allocation/work, backpressure, fuzzing, and fail-closed disconnect |
| RPC/indexer deception | Return proofs with queries, verify roots locally, and treat unproved API responses as hints |
| Mempool replacement or branch confusion | Explicit lifecycle, expiry, replacement, and finalized-versus-provisional status |
| MEV/order manipulation | Declared ordering/fairness policy, inclusion evidence, and bounded queue rules |
| Eclipse or peer isolation | Peer diversity, authenticated discovery, anti-eclipse checks, and multi-source sync |
| Long-range or stale checkpoint attack | Finality anchors, validator-set history, checkpoint provenance, and recovery policy |
| Randomness bias | No consensus randomness by default; use a committed protocol beacon if required |
| State-growth/rent exhaustion | Deterministic size, storage, and lifetime accounting; pruning and rent policy |
| Supply-chain or binary substitution | Reproducible builds, pinned toolchains/dependencies, signed releases, and independent executors |
| Privacy leakage | Explicit public-state model, metadata analysis, and cryptographic privacy scheme if required |
| Bridge/oracle compromise | Keep external claims outside base consensus or specify authenticated adapters and rollback rules |
| Governance capture | Explicit upgrade authority, timelocks, emergency rules, and audit trail |
| Upgrade or migration divergence | Committed bounded migration, old/new root checks, downgrade prevention, and cross-version replay |
| Operator/admin compromise | Least privilege, authenticated admin/RPC, pinned genesis/config, secret-safe logs, and incident isolation |

No defense is free. Each row needs a named assumption, version, artifact, test or proof
target, status, and halt condition. The threat inventory is not itself a proof or an
exhaustive production matrix; new findings append rows rather than being forced into an
existing label.

## 8. Availability, storage, and recovery

An L1 cannot be only a chain of hashes. A new node must be able to recover the bytes needed
to recompute the state. The future protocol MUST define:

- canonical block and transaction storage;
- snapshots/checkpoints and replay limits;
- archival retention and pruning rules;
- peer discovery and authenticated transport;
- data-availability commitments and sampling, if used;
- crash recovery and atomic publication of state roots;
- detection of a missing or conflicting segment;
- recovery after a node loses local storage;
- a reproducible genesis package containing protocol parameters and initial state.

The current research journal and typed coordinate-history layer are useful experiments, but
they are not external custody or rollback resistance. A local file, even if hash-linked,
does not establish availability to an independent observer.

### 8.1 Network and synchronization surface

The future network protocol must distinguish at least these roles:

- **full verifying nodes** — retrieve and execute blocks and verify roots/finality but need
  not propose or vote;
- **consensus validators/proposers** — perform the selected proposal/vote role and satisfy
  the selected data-possession rule before signing;
- **archive/recovery nodes** — retain historical proposal cores, plural accepted-record ingestion
  carriers, canonical replay-evidence events, typed credential/evidence entries, same-wrapper
  occurrence witnesses, authorized archive checkpoints, complete replay views, bodies, receipts,
  proofs, and state snapshots;
- **light clients** — verify headers, consensus evidence/status, and state/receipt witnesses without
  trusting a server's claimed coordinate state; and
- **optional RPC/indexer nodes** — accelerate queries but remain non-authoritative.

Peer discovery, handshake, capability negotiation, request authentication, message size
limits, deduplication, backpressure, retry, and disconnect behavior are part of the attack
surface. A node MUST be able to reject a peer that supplies a valid-looking hash with no
replayable body, and it SHOULD obtain critical headers/data from more than one independent
peer to reduce eclipse risk.

RPC and indexer responses are query surfaces, not authority surfaces. A client that asks
whether a coordinate is occupied or who owns a token SHOULD receive the relevant root and
witness, then verify it locally. Pagination, historical queries, and event indexes need a
consistency rule so an API cannot mix records from different verified consensus-status roots without saying
so.

Bootstrap MUST name its trust anchor: a pinned and authorized `chain_launch_package` carrying its
complete `LaunchClosureBundleV1`, a verified
checkpoint that binds `chain_launch_manifest_id`, or a declared weak-subjectivity source that binds
the same launch identity. A raw `genesis_package` is insufficient because it intentionally predates
the initial profile, transition specification, admission, parity, and release records. A snapshot is
admissible only after its launch-manifest ID, typed state root, validator-set root, consensus
evidence/status proof, parameter/version root, and exact authorized `ReplayEvidenceViewV1` ID and
checkpoint authorization are checked. A new node must also obtain the bodies or witnesses required
by the declared availability rule. An evidence archive without its canonical ordered events,
checkpoint transition and authorization, cutoff position, normalization policy, inclusion and
prefix-completeness proofs, dependency/occurrence witnesses, and shared replay-view binding is not a
deterministic finality/availability replay source; a carried archive-root label proves none of them.

For genesis replay, the verifier MUST independently recompute and pin both the embedded `genesis_id`
and the authorized `chain_launch_manifest_id`, verify every uniquely carried launch-closure preimage
and typed reference through `launch_closure_bundle_id`, verify the one combined internal-plus-external
DAG including the external genesis node's actual outgoing scope/registry refs and exact outer
`GenesisPackageV1` length/ID equality, then verify the detached launch authorization against the
`G_0` launch-authority policy. Neither identifier substitutes for the other, and no referenced ID
may be resolved only from ambient local state. Checkpoint and
weak-subjectivity paths MUST transitively bind the same pair and the exact initial-profile identity;
otherwise they are a different, explicitly named trust anchor rather than bootstrap for this chain.

The protocol must choose its recovery guarantee. **Genesis replay** requires durable archival
availability of every required historical body and every referenced protocol implementation.
**Checkpoint or weak-subjectivity bootstrap** trades that requirement for a named trust/freshness
assumption. A pruned network cannot promise mandatory full replay unless separate archive
nodes actually provide it. If both paths are claimed, both must be tested independently.

### 8.2 Persistence and publication atomicity

Publishing a block, its state root, and its availability record must be crash-safe. The
protocol/storage boundary needs an ordering such as:

```text
receive -> validate -> durably stage body -> execute -> durably stage roots
         -> publish candidate -> verify selected consensus status -> publish accepted index
```

The exact order is an **Open** implementation choice, but a crash between any two steps
must leave a recoverable prefix rather than a root that claims bytes the node cannot
reconstruct. Power-loss, torn-write, corruption, restore, pruning, and recovery tests are
required before a custody or durability claim.

Validator recovery has an additional monotonicity requirement: restoring a backup or moving
to a failover signer MUST NOT regress vote, nonce, slashing-protection, accepted-head, or
upgrade state and thereby authorize a double-sign or stale history. Recovery tooling must
verify exact roots and authority epochs before proposal/vote service resumes.

### 8.3 Operational security, monitoring, and safe halt

Production operation is part of the security envelope. Nodes SHOULD separate consensus,
execution, networking, RPC/indexing, administration, and signing privileges; authenticate
admin/RPC surfaces; pin genesis and configuration; keep secrets out of logs; and verify
signed, reproducible updates with a downgrade-safe rollback procedure.

Operators must monitor finalized or threshold-confirmed head continuity, conflicting
deterministic-finality evidence, probabilistic fork-choice anomalies, validator
participation and double-sign warnings, active/next validator roots, availability retrieval,
peer diversity/eclipsing, execution-root agreement, storage and recovery health, supply
correspondence, resource saturation, pending upgrades, and any credential/evidence ID collision,
valid distinct-core lookup collision, or forged authority-bearing object. Alerts must carry the exact
height, roots, version, and signed evidence needed for independent diagnosis. Metrics are
diagnostic projections, never consensus authority.

A node MUST enter a declared safe-halt mode—stop proposing, voting, or publishing new
accepted indexes—when evidence violates the selected profile's safety rule, it observes an
unknown active version, consensus-required bytes remain unavailable beyond the permitted
horizon, a state-root mismatch is irrecoverable, anti-double-sign state has regressed, or
`ProfileOwnedAuthorityBindingCompromiseV1` or `ProtocolIdentityHashCompromiseV1` is established. For
either authority-binding compromise, the ordinary coordinated-upgrade restart path is forbidden.
For a profile-owned compromise, the current disposition is always
`SafeHaltAndSeparatelyTrustedNewGenesisV1`; the reserved recovery-envelope type defines no current
in-place branch. A future independently anchored recovery design is conditional on first specifying
and activating `FutureIndependentRecoveryExtensionV1`, not an exception a current node may infer.
For a `ProfileIdHashV1` compromise, an independent recovery anchor may authorize only a new genesis,
never an in-place continuation. Restart here therefore requires the separately trusted new-genesis
procedure defined above. A preventive migration of a suite that remains sound and unambiguous is not
this disposition.
Ordinary competing branches in a probabilistic protocol go through its fork-choice and
confirmation rules; their mere existence is not a safe-halt trigger. Safe halt does not automatically
roll back or rewrite finalized history. The incident procedure must define evidence capture,
key isolation/revocation, peer quarantine, resynchronization, coordinated upgrade handling,
restart criteria, and a public post-incident account for any widened trust assumption.

## 9. Determinism and formal refinement

The single most important inherited property is replayability across every activated
protocol version. Accepted-record wrappers are plural ingestion and archive carriers, not ledger
rows. Independent chain validation consumes the complete and authorized chain launch package and a
parent-ordered sequence containing exactly one signature-free proposal core per height. Every core
is paired with either a bounded normalized evidence snapshot or a bounded retained eligibility
witness derived from one proof-bearing replay-evidence view. Historical authority use and later
status projection are separate claims:

```text
ReplayEvidenceArchiveResolutionV1 =
    ResolveAuthorizedReplayEvidenceArchivePolicyV1(
        chain_launch_manifest_id,
        replay_evidence_archive_policy_id
    )

ReplayEvidenceArchiveHashV1(framed_input) =
    H[ReplayEvidenceArchiveResolutionV1.hash_suite_id](framed_input)

AcceptedRecordOuterOccurrenceV1 = Canonical(
    accepted_record_outer_occurrence_schema_version,
    accepted_parent_lookup_id,
    unique_accepted_proposal_core_block_id,
    ConsensusNativeAcceptedRecordOccurrencePositionV1,
    consensus_native_accepted_record_occurrence_receipt_id,
    canonical_accepted_record_outer_length_u64,
    canonical_accepted_record_outer_bytes,
    typed_proposal_credential_id,
    ConsensusEvidenceIdV1,
    AvailabilityAssessmentIdV1
)

accepted_record_outer_occurrence_id = ReplayEvidenceArchiveHashV1(
    Domain("bitblock/accepted-record-outer-occurrence/v1")
 || LengthFrame(Canonical(AcceptedRecordOuterOccurrenceV1))
)

AcceptedParentAuthorityConsumerRefV1 =
    EnvelopeChildAuthorityConsumerV1(
        child_accepted_parent_lookup_id,
        child_block_id
    )
  | SuccessorReleaseAuthorityConsumerV1(
        release_sign_doc_id
    )
  | SuccessorActivationAuthorityConsumerV1(
        successor_activation_sign_doc_id
    )

AcceptedConsensusNativeParentAuthorityCertificateOccurrenceV1 = Canonical(
    accepted_parent_authority_certificate_occurrence_schema_version,
    consensus_native_parent_authority_certificate_id,
    consensus_native_parent_authority_certificate_authorization_id,
    consensus_native_parent_authority_certificate_admission_id,
    accepted_parent_authority_evidence_commitment_id,
    consensus_native_parent_authority_evidence_cutoff_id,
    consensus_native_accepted_record_occurrence_universe_root,
    canonical_accepted_parent_authority_certificate_length_u64,
    canonical_accepted_parent_authority_certificate_bytes
)

accepted_parent_authority_certificate_occurrence_id = ReplayEvidenceArchiveHashV1(
    Domain("bitblock/accepted-parent-authority-certificate-occurrence/v1")
 || LengthFrame(Canonical(AcceptedConsensusNativeParentAuthorityCertificateOccurrenceV1))
)

ReplayEvidenceEventV1 =
    AcceptedRecordOuterOccurrenceEventV1(
        AcceptedRecordOuterOccurrenceV1,
        accepted_record_outer_occurrence_id
    )
  | AcceptedParentAuthorityUseOccurrenceEventV1(
        AcceptedParentAuthorityUseV1,
        AcceptedParentAuthorityConsumerRefV1,
        HistoricalAcceptedParentAuthorityUseRefV1,
        AcceptedConsensusNativeParentAuthorityCertificateOccurrenceV1,
        accepted_parent_authority_certificate_occurrence_id,
        canonical_authority_consumer_preimage_length_u64,
        canonical_authority_consumer_preimage_bytes
    )

ReplayEvidenceEventRecordV1 = Canonical(
    replay_evidence_event_record_schema_version,
    replay_evidence_archive_id,
    evidence_event_ordinal_u64,
    previous_replay_evidence_event_id_or_archive_start,
    ReplayEvidenceEventV1
)

replay_evidence_event_id = ReplayEvidenceArchiveHashV1(
    Domain("bitblock/replay-evidence-event/v1")
 || LengthFrame(Canonical(ReplayEvidenceEventRecordV1))
)

ReplayParentAuthorityStatusProjectionCoreV1 = Canonical(
    accepted_parent_lookup_id,
    unique_accepted_proposal_core_block_id,
    AcceptedParentAuthorityUseV1,
    parent_authority_eligibility_policy_id,
    required_parent_authority_predicate_id,
    projected_parent_authority_status
)

ReplayParentAuthorityStatusProjectionCoreSetV1 =
    CanonicalSortedUniqueByAcceptedParentAuthorityUse([
        ReplayParentAuthorityStatusProjectionCoreV1...
    ])

NormalizedAcceptedAuthorityEntryCoreV1 = Canonical(
    accepted_parent_lookup_id,
    unique_accepted_proposal_core_block_id,
    AcceptedProposalCredentialSetV1,
    AcceptedEvidenceSetV1,
    canonical_dependency_closure_witness_manifest,
    ReplayParentAuthorityStatusProjectionCoreSetV1
)

normalized_authority_entry_id = ReplayEvidenceArchiveHashV1(
    Domain("bitblock/normalized-accepted-authority-entry/v1")
 || LengthFrame(Canonical(NormalizedAcceptedAuthorityEntryCoreV1))
)

NormalizedAuthorityViewEntryV1 = Canonical(
    accepted_parent_lookup_id,
    normalized_authority_entry_id
)

NormalizedAuthorityViewEntrySetV1 =
    CanonicalSortedUniqueByAcceptedParentLookupId([
        NormalizedAuthorityViewEntryV1...
    ])

normalized_authority_view_root = ReplayEvidenceArchiveHashV1(
    Domain("bitblock/normalized-authority-view/v1")
 || LengthFrame(Canonical(NormalizedAuthorityViewEntrySetV1))
)

ReplayEvidenceArchiveCheckpointCoreV1 = Canonical(
    replay_evidence_archive_checkpoint_schema_version,
    chain_id,
    genesis_id,
    chain_launch_manifest_id,
    replay_evidence_archive_id,
    replay_evidence_archive_policy_id,
    previous_checkpoint_id_or_archive_genesis,
    checkpoint_ledger_height_u64,
    first_covered_event_ordinal_u64,
    last_covered_event_ordinal_u64,
    covered_event_count_u64,
    ordered_event_prefix_root,
    evidence_normalization_policy_id,
    normalized_authority_view_root,
    checkpoint_authority_context_ref,
    checkpoint_authority_policy_id
)

replay_evidence_archive_checkpoint_id = ReplayEvidenceArchiveHashV1(
    Domain("bitblock/replay-evidence-archive-checkpoint/v1")
 || LengthFrame(Canonical(ReplayEvidenceArchiveCheckpointCoreV1))
)

ReplayEvidenceArchiveCheckpointSignDocV1 = Canonical(
    replay_evidence_archive_checkpoint_id,
    checkpoint_authority_context_ref,
    checkpoint_authority_policy_id
)

ReplayEvidenceArchiveCheckpointAuthorizationV1 = Canonical(
    ReplayEvidenceArchiveCheckpointSignDocV1,
    checkpoint_authorization_scheme_id,
    checkpoint_authority_credential_set_id,
    canonical_checkpoint_authorization_bytes
)

replay_evidence_archive_checkpoint_authorization_id = ReplayEvidenceArchiveHashV1(
    Domain("bitblock/replay-evidence-archive-checkpoint-authorization/v1")
 || LengthFrame(Canonical(ReplayEvidenceArchiveCheckpointAuthorizationV1))
)

ReplayEvidenceArchiveCheckpointV1 = Canonical(
    ReplayEvidenceArchiveCheckpointCoreV1,
    replay_evidence_archive_checkpoint_id,
    ReplayEvidenceArchiveCheckpointAuthorizationV1,
    replay_evidence_archive_checkpoint_authorization_id
)

ReplayEvidenceCheckpointConsistencyProofV1 = Canonical(
    historical_replay_evidence_archive_checkpoint_id,
    replay_view_replay_evidence_archive_checkpoint_id,
    canonical_append_consistency_witness
)

ReplayEvidenceEventInclusionProofV1 = Canonical(
    replay_evidence_archive_checkpoint_id,
    evidence_event_ordinal_u64,
    replay_evidence_event_id,
    ordered_event_prefix_root,
    canonical_event_inclusion_witness
)

ReplayEvidencePrefixCompletenessProofV1 = Canonical(
    replay_evidence_archive_checkpoint_id,
    first_event_ordinal_u64,
    last_event_ordinal_u64,
    covered_event_count_u64,
    ordered_event_prefix_root,
    canonical_complete_ordered_event_records,
    canonical_event_inclusion_proofs
)

ReplayEvidenceCutoffV1 = Canonical(
    cutoff_kind = NamedReplayProjectionCutoffV1,
    cutoff_ledger_height_u64,
    cutoff_evidence_event_ordinal_u64,
    replay_evidence_archive_checkpoint_id,
    ordered_event_prefix_root,
    evidence_normalization_policy_id
)

ReplayEvidenceViewCoreV1 = Canonical(
    replay_evidence_view_schema_version,
    replay_evidence_archive_checkpoint_id,
    ReplayEvidenceCutoffV1,
    ReplayEvidencePrefixCompletenessProofV1,
    normalized_authority_view_root
)

replay_evidence_view_id = ReplayEvidenceArchiveHashV1(
    Domain("bitblock/replay-evidence-view/v1")
 || LengthFrame(Canonical(ReplayEvidenceViewCoreV1))
)

ReplayEvidenceViewV1 = Canonical(
    ReplayEvidenceViewCoreV1,
    replay_evidence_view_id,
    ReplayEvidenceArchiveCheckpointV1
)

SameWrapperAcceptedRecordOccurrenceWitnessV1 = Canonical(
    replay_evidence_view_id,
    replay_evidence_event_id,
    accepted_record_outer_occurrence_id,
    ConsensusNativeAcceptedRecordOccurrencePositionV1,
    consensus_native_accepted_record_occurrence_receipt_id,
    accepted_parent_lookup_id,
    unique_accepted_proposal_core_block_id,
    ConsensusEvidenceIdV1,
    AvailabilityAssessmentIdV1,
    ReplayEvidenceEventInclusionProofV1
)

ReplayEvidenceNormalizedEntryProofV1 = Canonical(
    replay_evidence_view_id,
    accepted_parent_lookup_id,
    normalized_authority_entry_id,
    normalized_authority_view_root,
    canonical_normalized_entry_membership_witness,
    CanonicalSortedUniqueByAcceptedRecordOuterOccurrenceId([
        SameWrapperAcceptedRecordOccurrenceWitnessV1...
    ])
)

CertificateEvidenceArchiveOccurrenceWitnessV1 = Canonical(
    CertificateEvidenceReplayCheckpointRefV1,
    replay_evidence_event_id,
    accepted_record_outer_occurrence_id,
    accepted_parent_lookup_id,
    unique_accepted_proposal_core_block_id,
    ConsensusNativeAcceptedRecordOccurrencePositionV1,
    consensus_native_accepted_record_occurrence_receipt_id,
    typed_proposal_credential_id,
    ConsensusEvidenceIdV1,
    AvailabilityAssessmentIdV1,
    ReplayEvidenceEventInclusionProofV1
)

CertificateEvidenceArchiveOccurrenceWitnessSetV1 =
    CanonicalSortedUniqueByAcceptedRecordOuterOccurrenceId([
        CertificateEvidenceArchiveOccurrenceWitnessV1...
    ])

ConsensusToReplayParentAuthorityCommitmentProofV1 = Canonical(
    consensus_native_parent_authority_certificate_admission_id,
    accepted_parent_authority_evidence_commitment_id,
    consensus_native_parent_authority_evidence_cutoff_id,
    consensus_native_accepted_record_occurrence_universe_root,
    CertificateEvidenceReplayCheckpointRefV1,
    AcceptedParentAuthorityRefV1,
    CertificateEvidenceArchiveOccurrenceWitnessSetV1,
    canonical_certificate_commitment_to_occurrence_reconstruction_map,
    canonical_certificate_dependency_closure_recomputation_trace,
    recomputed_certificate_parent_authority_status
)

ReplayParentAuthorityStatusProjectionV1 = Canonical(
    replay_evidence_view_id,
    ReplayParentAuthorityStatusProjectionCoreV1
)

ReplayParentAuthorityStatusProjectionSetV1 =
    CanonicalSortedUniqueByAcceptedParentAuthorityUse([
        ReplayParentAuthorityStatusProjectionV1...
    ])

NormalizedAcceptedEvidenceSnapshotV1 = Canonical(
    NormalizedAcceptedAuthorityEntryCoreV1,
    normalized_authority_entry_id,
    replay_evidence_view_id,
    ReplayEvidenceNormalizedEntryProofV1
)

RetainedAcceptedEligibilityProjectionV1 = Canonical(
    accepted_parent_lookup_id,
    unique_accepted_proposal_core_block_id,
    canonical_purpose_scoped_qualifying_proposal_credential_witnesses,
    canonical_purpose_scoped_dependency_closed_evidence_subsets,
    ReplayParentAuthorityStatusProjectionSetV1
)

ReplayEvidenceRetentionProjectionProofV1 = Canonical(
    replay_evidence_view_id,
    normalized_authority_entry_id,
    deterministic_retention_policy_id,
    NormalizedAcceptedAuthorityEntryCoreV1,
    RetainedAcceptedEligibilityProjectionV1,
    canonical_deterministic_retention_trace
)

RetainedAcceptedEligibilityWitnessV1 = Canonical(
    replay_evidence_view_id,
    normalized_authority_entry_id,
    ReplayEvidenceNormalizedEntryProofV1,
    ReplayEvidenceRetentionProjectionProofV1
)

ReplayAcceptedAuthorityInputV1 =
    NormalizedAcceptedEvidenceSnapshotV1
  | RetainedAcceptedEligibilityWitnessV1

HistoricalAcceptedParentAuthorityUseWitnessV1 = Canonical(
    HistoricalAcceptedParentAuthorityUseRefV1,
    AcceptedParentAuthorityConsumerRefV1,
    AcceptedConsensusNativeParentAuthorityCertificateV1,
    accepted_parent_authority_certificate_occurrence_id,
    replay_evidence_view_id,
    certificate_evidence_replay_checkpoint = ReplayEvidenceArchiveCheckpointV1,
    ReplayEvidenceCheckpointConsistencyProofV1,
    ConsensusToReplayParentAuthorityCommitmentProofV1,
    authority_consumer_event_ordinal_u64,
    certificate_evidence_prefix_completeness_proof = ReplayEvidencePrefixCompletenessProofV1,
    authority_consumer_event_inclusion_proof = ReplayEvidenceEventInclusionProofV1,
    canonical_authority_consumer_preimage_length_u64,
    canonical_authority_consumer_preimage_bytes,
    canonical_qualifying_proposal_credential_preimages,
    canonical_dependency_closed_evidence_preimages
)

HistoricalAcceptedParentAuthorityUseWitnessSetV1 =
    CanonicalSortedUniqueByParentUseAndConsumer([
        HistoricalAcceptedParentAuthorityUseWitnessV1...
    ])

AcceptedCoreReplayEntryV1 = Canonical(
    AcceptedProposalCoreV1,
    ReplayAcceptedAuthorityInputV1,
    HistoricalAcceptedParentAuthorityUseWitnessSetV1
)

(chain_launch_package,
 ReplayEvidenceViewV1,
 parent-ordered unique AcceptedCoreReplayEntryV1 sequence,
 implementation/specification for every referenced protocol and consensus version)
    -> exactly one verification disposition,
       execution-state-root sequence,
       receipt-root sequence,
       finality-or-confirmation status sequence,
       and canonical availability-status sequence
```

`ReplayEvidenceArchiveHashV1`, event grammar, checkpoint commitment, count/byte/work bounds,
normalization policy, and checkpoint-authority resolver are selected only by the launch- or accepted-
governance-authorized `replay_evidence_archive_policy_id`; carried IDs are byte-equal assertions,
never selectors. Event ordinals start at the policy's fixed archive-start ordinal and are contiguous.
Every event record names its predecessor ID. `ordered_event_prefix_root` commits the archive ID,
exact sequence length, every ordinal, and every event ID. A checkpoint transition verifies the prior
checkpoint, appends exactly the declared contiguous range, recomputes the prefix and normalized-view
roots, derives the domain-separated checkpoint ID, and verifies the detached sign document under the
pre-existing checkpoint authority. That authorization covers exactly
`Domain("bitblock/replay-evidence-archive-checkpoint-authorization-sign-doc/v1") ||
LengthFrame(Canonical(ReplayEvidenceArchiveCheckpointSignDocV1))`; the carried context, policy,
scheme, and credential-set fields MUST equal the resolver and never select it. When the appended
range contains `AcceptedParentAuthorityUseOccurrenceEventV1`, its exact consumer preimage MUST carry
the byte-equal `HistoricalAcceptedParentAuthorityUseRefV1`. The same authenticated event MUST also
carry `AcceptedConsensusNativeParentAuthorityCertificateOccurrenceV1`: after bounded length
preflight, the archive verifier total-decodes and canonically re-encodes its exact certificate bytes
as `AcceptedConsensusNativeParentAuthorityCertificateV1`, byte-equals that value to the historical
witness, and recomputes the certificate, authorization, admission, evidence-commitment, consumer-
context, purpose, position, and consumer IDs. Every repeated occurrence field MUST equal the decoded
certificate; a reference or admission ID without this authenticated preimage is insufficient. The
checkpoint signature never replaces that admission. An archive event ordinal records archival
observation order only. It neither proves when live consensus accepted a certificate or record
occurrence nor authorizes a consumer, and the archive checkpoint containing the event MAY be created
later. An `AcceptedRecordOuterOccurrenceV1` therefore carries the exact previously committed
`ConsensusNativeAcceptedRecordOccurrencePositionV1` and receipt ID in addition to the recovered
outer bytes. The archive verifier recomputes that receipt ID, but only the certificate's authenticated
cutoff/universe proof establishes that this was a live accepted occurrence. Unknown policy, missing prior checkpoint,
gap, duplicate event or occurrence, reordered event, wrong predecessor, noncontiguous range, root
mismatch, unauthorized checkpoint, invalid certificate admission, or consumer/ref mismatch rejects.
The archive authority is an explicit replay-custody boundary; its signature does not make the
archive, snapshot, or replay cutoff consensus authority.

`ReplayEvidencePrefixCompletenessProofV1` is deliberately inhabited here only by the complete,
bounded ordered event range plus each inclusion proof. A later admitted succinct proof may replace
those explicit records only if it proves the identical range-completeness and deterministic-
normalization relation; a bare root, bloom filter, sampled range, or provider assertion is not that
proof. Prefix-first streaming preflight checks all event, record, proof, count, byte, nesting, and
work bounds before allocation or cryptographic work. Checkpoint publication is atomic: authorization,
checkpoint core/ID, complete range, and availability are durably published as one object or the new
checkpoint does not exist. A crash after only a root, authorization, or partial range leaves the
prior checkpoint authoritative.

Exactly one `ReplayEvidenceViewV1` occurs outside and is shared by the complete replay package.
Every entry, normalized proof, historical-use witness, same-wrapper occurrence witness, and later
status projection MUST name its exact `replay_evidence_view_id`; mixed checkpoint, root, cutoff, or
normalization-policy views reject. `ReplayEvidenceCutoffV1` is now only the one replay-wide named
status-projection cutoff. Its height, event ordinal, checkpoint ID, prefix root, and normalization
policy must equal the verified checkpoint/view, and evidence after that ordinal is excluded from the
later projection. Bootstrap or a verified state snapshot that claims replayed authority/finality
status MUST bind this checkpoint ID, authorization, and replay-view ID in its manifest; an archive
root label alone is insufficient. Each historical-use witness additionally supplies the exact older
authorized checkpoint and a `ReplayEvidenceCheckpointConsistencyProofV1` proving it is an append-
only ancestor of the outer replay-view checkpoint.

`NormalizedAcceptedAuthorityEntryCoreV1` is the only normalized-entry commitment preimage. It is
proof-free: the core contains the unique lookup/core, complete verified credential/evidence sets,
dependency-closure manifest, and purpose-scoped status-projection cores, but excludes every view,
checkpoint, membership/occurrence proof, and retained-projection proof. The verifier recomputes
`normalized_authority_entry_id` from that exact core, then recomputes
`normalized_authority_view_root` from the sorted unique `(accepted_parent_lookup_id,
normalized_authority_entry_id)` pairs. `ReplayEvidenceNormalizedEntryProofV1` proves membership of
that recomputed pair and binds the exact same-wrapper occurrences to the complete authorized event
prefix. A carried ID, proof, or root can never define its own preimage.

For either authority-input variant, all credential and evidence IDs recompute, the proposal set
contains the required qualifying witnesses, every evidence subset is dependency-closed, and every
purpose-scoped status is recomputed before projection. `NormalizedAcceptedEvidenceSnapshotV1`
carries the exact full proof-free core and its ID. The current
`ReplayEvidenceRetentionProjectionProofV1` is inhabited only by that same full committed core plus a
deterministic retention trace; therefore a retained subset cannot masquerade as the committed entry.
A later succinct retention proof may omit the full core only after an admitted proof system freezes
and proves the identical projection relation. In either form every projected-status witness and
required conflict witness is preserved. A pre-cutoff conflict cannot be omitted and a post-cutoff
entry cannot be inserted relative to that authorized replay-archive prefix. This is archive-custody
completeness, not proof that the archive observed every live consensus occurrence; historical
certificate validation additionally requires the consensus-native cutoff universe and range proof.

`SameWrapperConsensusDependencyWitnessV1` remains the live-ingestion semantic witness constructed
atomically in §4.4. Independent replay additionally requires
`SameWrapperAcceptedRecordOccurrenceWitnessV1`. Its included event contains the exact bounded
`AcceptedRecordOuterV1` bytes, committed live position, and consensus-native receipt ID; a fresh
verifier reconstructs the signature-free core, credential, consensus evidence, and availability
assessment from that one occurrence, recomputes all IDs, and requires byte equality with the
normalized entries. Consensus bytes from one occurrence and an assessment from another cannot be
synthesized into this proof. Archive creation time is deliberately irrelevant: byte-identical later
recovery is valid only as recovery of the already root-committed receipt, never as proof of a new live
occurrence.

`HistoricalAcceptedParentAuthorityUseWitnessV1` proves a different claim from
`ReplayParentAuthorityStatusProjectionV1`. The historical witness binds one exact unique parent
lookup/core, purpose tag, policy, predicate, consumer reference, attained status, and exact
`AcceptedConsensusNativeParentAuthorityCertificateV1`. The authenticated authority-use event
commits its exact accepted-certificate bytes through
`AcceptedConsensusNativeParentAuthorityCertificateOccurrenceV1`; an admission ID alone cannot
support the witness. The certificate's consensus position and admission evidence, not archive order,
prove its parent-profile acceptance.
`ConsensusToReplayParentAuthorityCommitmentProofV1` independently reconstructs
the certificate's exact `AcceptedParentAuthorityEvidenceCommitmentV1` from byte-exact
`AcceptedRecordOuterOccurrenceV1` events included in one complete authorized archive prefix. It
requires its cutoff ID and occurrence-universe root to byte-equal the accepted certificate, and the
archive-witness set to be exactly total over the range-derived receipt set in
`CompleteConsensusNativeAcceptedRecordOccurrenceUniverseV1`. Every occurrence's committed live
position, exact outer bytes, and recomputed consensus-native receipt ID must byte-equal one receipt in
that set. The verifier rechecks the cutoff consensus proof and authenticated range-completeness proof,
then recomputes every credential/evidence ID, same-wrapper dependency, dependency closure, purpose,
and status and byte-equals all parent/core/policy fields. Archive bytes absent from the pre-admission
universe cannot justify a certificate even if their content matches another receipt. Conversely, a
later archive may losslessly recover an exact committed occurrence; its later event ordinal does not
falsify the earlier authenticated live position. Later normalized-entry retention or replacement
cannot erase the raw occurrence bytes and live-position metadata that the accepted certificate names.
Those occurrence records and proofs are mandatory replay-retention pins until an admitted lossless or
succinct replacement proves the same reconstruction relation. The replay checkpoint may postdate the consumer;
`ReplayEvidenceCheckpointConsistencyProofV1` proves only that this chosen evidence checkpoint is an
append-only ancestor of the shared outer replay view. The separate
`ReplayAcceptedAuthorityInputV1` still proves the later normalized status view.
`EnvelopeParentAuthorityV1` may consume only
`EnvelopeChildAuthorityConsumerV1`; `SuccessorReleaseAuthorityV1` only
`SuccessorReleaseAuthorityConsumerV1`; and `SuccessorActivationAuthorityV1` only
`SuccessorActivationAuthorityConsumerV1`. The child `ProposalHeader`, successor
`ReleaseSignDocV1`, or `SuccessorActivationSignDocV1` respectively MUST carry the byte-identical
historical ref before its block/sign-document ID and authorization are derived. The verifier derives
the exact certificate-independent `ParentAuthorityCertificateConsumerContextV1` from the remaining
consumer fields and byte-equals its ID to the accepted certificate. The consumer preimage and ID
recompute from its included archive event, but that event may be archived later. For an envelope
child, the certificate admission position MUST be a strict predecessor of the admitted child under
the profile's exact consensus-order relation. Release and activation sign documents have no implied
consensus position: their enforceable chronology is the serialization/signing dependency requiring
the complete accepted certificate to verify before the sign-document ID and authorization are
derived. A profile that later consensus-admits either document must state and check that separate
position relation explicitly. Evidence added to a later archive view may update a named status projection, but it cannot
alter the already accepted certificate, its exact evidence commitment, or the consumer-bound
admission ID.

The authorized cutoff and complete consensus-native occurrence range determine one canonical
certificate evidence commitment for the exact purpose and consumer context. A provider's partial
view cannot produce an alternative admissible certificate. Only the admission accepted at the
required position determines the
`HistoricalAcceptedParentAuthorityUseRefV1`, child `block_id`, release sign-document ID, or
activation sign-document ID; a replay provider cannot switch candidates afterward.

The witness set provides exact coverage: every non-launch child has exactly one envelope-parent
witness; every replayed successor release has exactly one release-purpose witness; every replayed
activation has exactly one activation-purpose witness; the first launch record has no coerced
accepted-parent witness; and no unconsumed, duplicated, wrong-purpose, or wrong-consumer witness is
legal. Status projection is separately sorted by purpose under the one shared named cutoff, so one
untagged attained status or per-entry named cutoff cannot substitute for this set.

The core sequence MUST begin at the authorized launch parent and form one continuous parent relation,
with strictly increasing height and exactly one recomputed lookup/core at each height. Replaying two
wrappers for one core as two heights, choosing one wrapper, or selecting a credential/evidence subset
by arrival order is invalid. Exact outer-wrapper occurrences are evidence events, but they do not
determine ledger order or count. A later status upgrade appears only in the shared named projection;
it does not rewrite any earlier historical-use witness.

Version activation, validator scheduling, committed randomness, rewards, governance, and
migrations come from the accepted, availability-covered `BlockContext`/body/header state context.
At every migration, replay verifies the embedded successor bundle, admitted release, target
admission and `UPGRADE-1` evidence, parent-authorized sign document, and all required implementation
preimages under the old envelope before execution; no ambient registry completes that input. Replay
checks every context projection and root, then evaluates the same total
`Execute(state, BlockContext, Body)` relation. Ordered user actions alone are not a
complete replay input once any of those mechanisms exists. An execution-only replay may
start from a supplied trusted parent and omit consensus evidence, but its claim MUST then say
that parent selection and finality verification were out of scope.

Powered replay controls split qualifying credentials, consensus evidence, and availability evidence
across several wrappers for one core; permute those wrappers; duplicate every wrapper and typed
entry; omit one qualifying or dependency witness; replay one wrapper as an extra height; substitute
two cores at one height; and place a status-upgrading entry immediately before and after the shared
named cutoff. Archive controls permute or duplicate event ordinals, omit a pre-cutoff conflict,
insert a post-cutoff entry, mutate either cutoff boundary, substitute root/policy/checkpoint or
authorization, mix views between entries, synthesize a consensus/availability pair from separate
outer occurrences, alter any proof-free normalized-entry field without changing its ID, substitute
an ID/root for a different core, include a view/proof field in the ID preimage, project a retained
subset without the full-core retention proof, and crash publication after each strict prefix.
Historical controls substitute every purpose; omit or duplicate one required purpose; change policy,
predicate, status, consumer, certificate-independent consumer context, consumer event,
certificate/evidence-commitment/authorization/admission/cutoff ID, required consensus position,
occurrence-universe root, history anchor, consensus or availability policy; remove the
historical ref from the child header or release/activation sign document; use an authorized but non-
admitted certificate; use a partial-view certificate that omits a complete-range receipt; accept two
admissions at one required position; make consensus evidence authenticate a different admission
core; omit certificate availability; place an envelope admission after its admitted child; derive or
sign a release/activation document before the complete accepted certificate exists; and exercise the
profile's exact equivocation and reorganization rules. Receipt controls mutate every repeated outer
field or live occurrence-position field, make decoding partial/noncanonical/over-bound, omit one
receipt-component projection, omit an entire pre-cutoff benign or conflict wrapper, add an unused
receipt or projection, duplicate a key/projection, alter any of the four typed components, map one
same-wrapper dependency to a different receipt, select an earlier cutoff, forge either range boundary
or a gap-free witness, and pair a qualifying credential with valid conflicting evidence in a separate
pre-cutoff wrapper while trying to omit that wrapper from commitment or status evaluation. Archive bridge controls may create the replay
checkpoint after the consumer as a positive control, then substitute its root/policy/view, forge
append consistency, omit one certificate-committed credential/evidence preimage, justify a bad
certificate with bytes absent from the accepted occurrence universe, substitute a byte-identical
receipt at a different live position, fabricate a pre-cutoff position absent from the authenticated
range, change or omit its consensus-native receipt ID, omit or mutate the
authenticated accepted-certificate occurrence bytes or occurrence ID, retain only the historical
reference/admission ID, evict a certificate-pinned raw occurrence before checkpointing, replace it
with an equivalent retained normalized witness,
break dependency closure/status recomputation, or mix historical and named views. Equivalent
normalized-witness replacement and post-admission archival recovery are positive controls only when
every certificate receipt's exact pre-cutoff live position, receipt ID, and outer bytes remain
available and byte-equal. Every valid carrier
permutation normalizes to the same one-core-
per-height result and purpose-scoped named status. Omission, dependency break, incomplete range,
wrong occurrence, ambiguous or circular entry commitment, non-accepted or late certificate,
certificate-to-replay mismatch, mixed view, extra/missing core, or any publication prefix rejects.

To make that a network property rather than a local intention:

1. Canonical bytes MUST have one decoder and one serialization.
2. All consensus-relevant integers, floats, limits, and error dispositions MUST be
   specified. Consensus should avoid unrestricted floating-point camera state.
3. The Rust executor SHOULD be compared with an independent reference implementation.
4. Property-based tests MUST cover idempotence, bedrock preservation, re-mint identity,
   nonce monotonicity, and conflict ordering.
5. The Lean model SHOULD grow from abstract occupancy/accounting into a refinement theorem
   for the chosen canonical state transition.
6. A node must fail closed on an unknown protocol version, malformed proof, a missing or invalid
   required parent-or-launch authority, or resource-bound violation.

The authoritative execution path must be auditable separately from non-authority paths:

- client input normalization may propose an action but cannot decide validity;
- renderers, sidebars, metrics, and camera floats are projections and cannot mint, burn, or
  finalize state;
- learned or heuristic components, if ever used for tooling, cannot influence consensus
  validity or certificate creation;
- randomness and wall-clock readings cannot enter execution unless they are explicit,
  committed protocol inputs with a defined bias/failure model.

The release itself is part of the proof boundary. A production candidate SHOULD publish
reproducible build instructions, pinned compiler/toolchain/dependency versions, source and
binary digests, signed release metadata, an independent build, and a procedure for handling
a compromised dependency or compiler. A consensus proof over one local binary does not
cover a substituted binary.

Release production also has two phases. `RELEASE-R1` may exercise the reusable build/reproduction
framework and emit a clearly labeled candidate artifact bound to `CandidateProfileV1` and its
`profile_id`. That artifact is not an admitted network release. Only after `PROFILE-1` binds the
same `profile_id` to a complete passing evidence closure may `RELEASE-1` publish profile-authority
metadata that also binds the typed profile-admission-manifest ID, exact source, features, genesis,
binary, and
configuration. A candidate build must never inherit “admitted” from reproducibility alone, and an
admitted record must reject any semantic projection or binary that differs from its candidate.
`RELEASE-1` first freezes `AdmittedReleaseCoreV1` and its fixed core ID, resolves the release policy,
codec, scheme, and credential set from `G_0` for the initial profile or from the independently
verified accepted predecessor for a successor, and verifies `ReleaseAuthorizationV1` over the exact
`ReleaseSignDocV1`. It derives the final record ID only afterward. Omitting or zeroing the core ID;
altering a source, binary, configuration, authority context, policy, scheme, credential, or signed
byte; choosing a verifier from the release object; signing the final record ID; or accepting a
non-canonical authorization encoding MUST fail powered controls. For
the initial launch, `ChainLaunchManifestV1` then binds that exact `RELEASE-1` record alongside the
admission, transition specification, and root-parity record; later release replacement cannot
silently rewrite the launch anchor.

The current Lean capsules prove only their declared abstract finite-support models: bedrock
and edit/replay properties plus per-edit supply increments, decrements, and no-op
preservation. They do not prove token identifiers, consuming-burn provenance, a global
episode-ledger theorem, Rust refinement, exact camera floats over the full coordinate domain,
or any network implementation.

## 10. Economic semantics

Cryptographic validity and monetary value are separate. Even a perfectly replicated token
ledger does not create a market.

The protocol must decide:

- whether tokens are transferable;
- whether they are fungible, unique, or both via separate asset classes;
- whether minting is unlimited, capped, scheduled, or resource-priced;
- whether burning is voluntary or required, and whether a later explicit compensating
  mint/recovery transition is permitted without rewriting the finalized burn;
- who pays transaction and storage costs;
- whether validators receive rewards;
- whether failed actions pay fees;
- whether an occupied coordinate is scarce enough to have intended value.

The default Bitblock rule has a particularly important consequence: place/delete loops
preserve active supply but can grow history without bound. A production protocol therefore
needs a resource policy even if it never assigns financial value to a block.

Fees and balances require their own state semantics. The protocol must choose an account,
UTXO, or capability model; define fee-payer authorization; specify whether fees are burned,
paid to a proposer, or split; and make failed-transaction charging deterministic. A block
token cannot silently double as a validator stake, a fee balance, and a user-owned asset.
Those assets need distinct domains, roots, and transfer rules.

If validator rewards or stake are introduced, the issuance schedule, reward timing,
unbonding delay, slashing evidence, and maximum monetary supply become consensus state.
Their economics must be analyzed against censorship, bribery, collusion, and griefing rather
than inferred from occupancy scarcity. Resource pricing must cover worst-case signature and
proof verification, execution, bandwidth, state writes, persistent history, witness
generation, and recovery—not only block count or payload bytes.

State rent, leases, storage deposits, or pruning credits may be needed to stop the large
coordinate space from becoming an unbounded obligation for every validator. Any such rule is
a state transition and MUST define its payer, price-update rule, grace period, expiry height,
warning/evidence path, automatic action, token/ownership/supply effect, reactivation right,
and behavior during partitions. Expiry cannot silently delete a coordinate or token outside
the same atomic accounting rules as an ordinary burn or transfer.

## 11. Layered implementation plan

The following checkpoints preserve the current cheap Rust core and add one security surface
at a time. They are conceptual security milestones, not the current scheduling authority. Exact
dependencies and status live only in [`CAPABILITY_TREE.md`](CAPABILITY_TREE.md): checkpoint A maps
primarily to `ACCOUNT-1`, `LOCAL-BOUNDS-1`, `STORAGE-1`, and `LEDGER-1`; B to
`TRANSITION-SPEC-D1`, identity/authorization, `EXECUTOR-1`, and `MULTI-1`; C to the commitment,
execution-root, sequencer, and synchronization nodes; D to the consensus/profile/testnet nodes; E
to `L1-CANDIDATE`; and F to `L1-PRODUCTION`. A later tree correction therefore does not require
silently rewriting this historical milestone summary.

### Checkpoint A — local occupancy ledger

Promote and extend the existing feature-gated, in-memory coordinate-episode research path
into an optional authoritative local ledger wrapper. This is a change of status and
durability, not a claim that no prototype exists.

Required properties:

- one live mint record per mutable occupied coordinate;
- one consuming burn record per deletion;
- no mutable occupancy-episode token for bedrock; any separate locked genesis allocation remains an
  explicit `ASSET-D1` choice;
- fresh token identity after re-placement;
- explicit no-op/rejection records and a reconciled submitted/admitted/executed/result lane;
- exact active-map correspondence, injective token identities, and parent-root checks;
- deterministic canonical bytes and a bounded replay limit.

This checkpoint proves provenance and conservation. It is not multiplayer or currency.

### Checkpoint B — two-observer deterministic harness

Keep observers as signed action streams, not entities. Run two or more identities through the
same in-process tick reducer and test:

- same-coordinate races;
- ownership and delegated edit rights;
- nonce/replay failures;
- deterministic ordering;
- independent replay from the same genesis.

This is the cheapest place to find semantic ambiguity.

### Checkpoint C — authenticated centralized multiplayer

Introduce a server/sequencer that validates signed actions, chooses canonical batch order,
stores the append-only log, and returns receipts with state roots. Clients can independently
replay every accepted batch. The server is still a trust and availability boundary; the
system should be described as a cryptographically accountable game ledger.

### Checkpoint D — replicated testnet

Run independent Rust processes with a declared consensus prototype. Test partitions,
duplicate proposals, equivocation, restart, missing data, slow validators, and conflicting
roots. Do not call this secure until safety and liveness assumptions are measured against
adversarial scenarios.

### Checkpoint E — Layer 1 candidate

Freeze the transaction, block, root, proof, and consensus schemas. Publish an independent
executor, genesis package, threat model, validator assumptions, upgrade rules, and recovery
procedure. Require external review before production value is introduced.

### Checkpoint F — production security claim

Only after the previous gates can the project make a scoped Layer 1 claim. The claim must
name the exact version, validator assumptions, finality rule, availability guarantee,
cryptographic assumptions, and excluded properties. “Layer 1” must never mean “the hash
looked convincing”.

## 12. Verification gates

Each gate answers one question and must not be promoted into a broader claim. “Bounded” below
describes the current evidence class; this documentation pass did not rerun it.

| Gate | Claim boundary | Required passing evidence | Current status / stop condition |
| --- | --- | --- | --- |
| G0 | Current local occupancy invariants | Pinned Rust tests and exact abstract Lean theorems | **Bounded**; no Rust-refinement claim |
| G1 | Feature-gated coordinate-episode correspondence | Map-domain/injectivity, mint/burn, re-mint, and origin vectors | **Partial bounded/local; OPEN**—the independent vector does not replay history, and no durable asset claim exists |
| G2 | Complete action/result accounting | Reconcile live submission, tape, tick outcome, edit history, rejection, and idle/control lanes | **Open**; any silently lost included action stops admission |
| G3 | Canonical codec, genesis, and domain separation | Cross-implementation corpus, prefix/suffix/duplicate/unknown-field negatives, collision controls | **Open**; any alternate accepted bytes or ID collision stops the version |
| G4 | Actor authentication and replay safety | Signature, key-validation, nonce, chain/genesis/launch/profile/world, anchor, rotation/recovery, and mutation controls | **Open**; any unauthorized or cross-domain replay stops admission |
| G5 | Transaction/interleaving semantics and conservation | **G5.core:** multi-actor schedules; failure/no-op/nonce atomicity; occupancy/episode conservation. **G5.asset:** transfer, fee, and wider asset/supply conservation only when activated | **Open; G5.core is mandatory**—any executor-dependent disposition or partial write stops every candidate. G5.asset is conditional, not a reason to exclude the core |
| G6 | Authenticated state and receipt proofs | Scheme-specific membership/non-membership plus receipt path through an authenticated header accepted at the selected consensus status | **Open**; a bare digest or unbound witness does not pass |
| G7 | Prefix safety and finality/confirmation | Formal argument plus partition/equivocation/fork adversarial harness under the named fault model | **Open**; evidence violating the selected profile's safety rule triggers safe halt, while ordinary probabilistic forks use fork choice |
| G8 | Chain liveness and eligible inclusion | Protocol/model argument for the scoped eventual claim plus bounded fault-injection measurements with synchrony, capacity, fee, and dissemination preconditions | **Open/bounded when run**; samples alone never prove an unstated universal |
| G9 | Availability, durability, and recovery | DA-before-vote controls, fresh-node recovery, pruning/archive, crash, power-loss, and restore tests | **Open**; unavailable required bytes or regressed signer state stops service |
| G10 | Implementation/specification refinement | Independent executor across every activated version plus formal or differential evidence | **Open**; same-code self-replay is insufficient |
| G11 | Cryptographic/key-operations boundary | Published TCB and assumptions, ceremonies, role-by-role preventive-migration versus active-compromise drills, independent audit, dispositioned findings | **Open**; an established authority-binding break cannot be reported as a migration pass, and merely receiving an audit is not a pass |
| G12 | Validator-set transitions and partitions | Parent-authorized active/next roots, certificate vectors, handoff/quorum proof, long-range recovery | **Open**; candidate-selected authority never passes |
| G13 | Peer sync and bootstrap | Multi-peer body/witness retrieval, parser/backpressure controls, checkpoint/genesis trust tests, eclipse tests | **Open**; one unproved peer response is not authority |
| G14 | Provisional versus accepted client state | Reorg, receipt-inclusion, nonce/replacement, wallet proof, and finality/confirmation-status tests | **Open**; a candidate receipt cannot be presented beyond its verified status |
| G15 | Resource and economic bounds | **G15.resource:** worst-case codec/signature/proof/execution/storage/history/witness/recovery measurements. **G15.economic:** fee/rent/denomination invariants only when activated | **Open; G15.resource is mandatory**—an over-bound remote input rejects/disconnects. Safe halt is reserved for a consensus-accepted artifact or local invariant that cannot be processed within the already committed bound; limits are never silently raised |
| G16 | Reviewed source versus shipped binary | Pinned toolchain/dependencies, reproducible independent builds, signed release metadata | **Open**; unmatched binary has no inherited proof claim |
| G17 | Upgrade/governance migration | Old/new root vectors, bounded migration replay, authorization/timelock, downgrade, historical-verification, last-safe-parent, and precommit/no-precommit recovery tests | **Open**; divergent migration roots or an unresolved authority-binding compromise stop activation |
| G18 | Production operations and incident response | Privilege/RPC tests, monitoring controls, backup/failover drill, safe-halt and key-compromise exercise | **Open**; recovery cannot resume through an unresolved authority/root fault |
| G19 | Future programs and randomness | Versioned VM/gas/refinement tests and committed-randomness adversarial analysis | **Open** until either feature exists |

A production coverage matrix must map each public property and threat row to the exact
protocol version, adversary assumption, gate, artifact, result, and failure condition. A
green G0 or G1 does not imply G7. A matching hash does not imply liveness or availability.
A passing network harness does not prove physical display output or economic value.

For every base occupancy profile, `G5.core` and `G15.resource` are required. Only the asset/economic
subparts of G5/G15 and the program/randomness features in G19 are conditional on activation; their
absence cannot be used to exclude core transition or resource evidence. Stage ownership still
matters—G18 is production work rather than a pre-testnet pass—but a later stage cannot silently
remove a base gate that its profile already requires.

## 13. Open decisions

These are deliberately not silently decided by this paper:

1. Is a block an occupancy token, an edit right, a fungible resource, or a dual asset?
2. Does ownership attach to coordinates, token episodes, or permissions?
3. Does identity use a stable actor descriptor or public-key-hash addresses, and what exact
   threshold, delegation, rotation, revocation, and recovery rules apply?
4. Which signature, hash, aggregation, and canonical-key suites are accepted for the first network version?
5. Which authenticated map supports sparse coordinates and scheme-specific non-membership proofs?
6. Which consensus/finality model matches the desired trust, availability, and cost assumptions?
7. What adversary, synchrony, capacity, validator-fault, and quorum-intersection assumptions apply?
8. Which validator-set activation, checkpoint, handoff, and long-range-attack rules are required?
9. Is the coordinate domain permanently signed-64-bit, and will a chain always contain one world?
10. Are observer reach/collision rules consensus state or local interaction rules?
11. Does the protocol retain deterministic application sub-ticks, and how do they map to block context?
12. Is one transaction one operation or a bounded vector, and what are its atomicity, no-op,
    admitted-failure, nonce, and fee dispositions?
13. Does a transaction require an exact parent, a windowed anchor, an expected-state precondition,
    or no state anchor, and how are stale transactions replaced?
14. Does the network retain `toggle`, require an expected-state precondition, or lower it to `on`/`off`?
15. Is mint identity transaction-stable or inclusion-stable across provisional forks?
16. Which transaction lifecycle, re-inclusion, and reorganization receipt rules are canonical?
17. Which live inputs and local rejections must be retained so submitted, admitted, executed,
    finalized-or-confirmed, rejected, camera-only, and idle lanes reconcile?
18. What exact fee, storage-rent, state-growth, and failed-action charging rules prevent denial of service?
19. What data-availability, archival, pruning, genesis-replay, checkpoint, or weak-subjectivity guarantee applies?
20. Which full-verifier, consensus-validator, archive, light-client, and RPC/indexer roles and retention windows exist?
21. What upgrade, migration, key-recovery, safe-halt, emergency, and governance rules are acceptable?
22. Is privacy required, and if so, which state is public versus concealed?
23. What exact versioned claim will “Layer 1 security” be allowed to mean for Bitblock?
24. Will a future visual program be consensus code, user data, or a separate non-authoritative projection?
25. What randomness source, if any, is admissible to a deterministic future program?
26. What reproducible-build, dependency, key-ceremony, audit, monitoring, backup, and incident-response standard is required?
27. Which external bridges or oracles, if any, are allowed to make claims outside the coordinate world?
28. Which actions are actor-authorized, validator-authorized, governance-authorized, or emergency-authorized,
    and how are those namespaces enforced?
29. Does `VERKLE-R1` beat the selected simpler hash-tree baseline by a predeclared end-to-end
    witness, update, verification, dependency/TCB, setup-generator, availability, and migration
    threshold strongly enough to justify a later profile candidate, or should the lane close?

Each decision should be recorded with its assumptions, rejected alternatives, falsifier, and
the smallest test that could disprove the chosen design. Until then, the current system
should retain its narrower and truthful description.

## 14. Historical checkpoint-0.3 obligations snapshot (not current descendant status)

The checkpoint-0.3 paper at commit `034b7b2b5f129625f9c6dcec3e14038b7c4aee5f` was checked
against source/proof snapshot `6545d8e7d60e75607445fda564bbf11610a62658`, the Lean/LadderLang capsules,
`docs/SEMANTICS.md`, `docs/HANDOFF.md`, and the open-obligation and hash-suite sections of
`docs/PROTOCOL.md`. The table below belongs only to that pinned historical checkpoint and records what
was open there. It is preserved for provenance; it is neither the audit status nor an exhaustive
obligation list for this mutable descendant. Current descendant review chronology and limitations are
the frozen audit chain linked at the top of this paper. The historical gaps were not covered merely by
the existence of the checkpoint paper:

| Repository boundary | What is still missing before a Layer 1 claim |
| --- | --- |
| Rust world versus Lean world | A refinement proof or independently checked correspondence for the chunked implementation |
| Canonical byte domains | Independent corpus parity for every authority-relevant schema and key/signature codec |
| Local journal/storage | Durable journal receipts, process-death/power-loss evidence, external custody, and rollback detection |
| History branches | Independently retained heads and a canonical fork/latest-history authority |
| Tick sidecar | Independent replay/refinement and shared durable storage/pruning for consensus-accepted outcomes |
| Action/result completeness | One canonical reconciliation of live submission, aggregate non-edit controls, ordered edit attempts, misses/rejections, camera-only actions, idle ticks, admission, execution, and finalization |
| Coordinate episodes | Promotion of the feature-gated, bounded in-memory sidecar into durable, independently verifiable asset evidence |
| World commitment | An authenticated sparse map, durable decoder, and membership/non-membership witnesses instead of the current full sorted-coordinate tag |
| Local replay evidence | A durable supplied-carrier verifier and independent executor; ordinary stdout and locally self-verified in-memory evidence are not run receipts |
| Sequential/elapsed-time evidence | No VDF/sequentiality analysis or external anchor; serial SHA-256 history is not Proof of History |
| LadderLang bridge | The current bridge is representation-focused and does not prove live edit execution or network authority |
| Network | Peer protocol, sync, validator messages, consensus/finality evidence, data-before-vote rule, and adversarial multi-node evidence |
| Cryptography | Pinned signature/hash suite, key lifecycle, proof witnesses, and migration controls |
| Economics | A chosen asset/account model, fees/rent, validator incentives, and supply policy |
| Ternary hashing | The intended balanced-trit implementation, exact contract, independent vectors, and analysis; examined Aethor TSH/PDH and IterHash remain non-authoritative |
| Operations | Privilege separation, monitoring, safe-halt, backup/failover, key compromise, and incident evidence |
| Upgrades | Deterministic migration format, old/new-root replay, authorization, downgrade, and historical-verification evidence |
| Programmability | A versioned deterministic VM and refinement/resource proof if blocks become executable |
| Presentation | Physical display evidence remains outside the Layer 1 state-machine claim |

The current branch still has a broad, versioned **design inventory of the presently
named surfaces**, not a proof of completeness and not security coverage. A future
implementation checkpoint should remove one row only when its evidence is independently
reproducible and its assumptions are written into the public claim.

## Conclusion

Bitblock already has the right seed for a verifiable system: a small semantic core,
an ordered tick boundary, sparse storage, canonical bytes, parent-linked history, and
bounded proofs. What it lacks is not more visual complexity. It lacks the authority and
replication layers that let independent participants agree on who did what, in what order,
with what finality and availability.

The strongest path is therefore incremental:

```text
binary occupancy
  -> deterministic local ledger
  -> signed multi-observer actions
  -> authenticated centralized history
  -> replicated consensus testnet
  -> independently audited Layer 1 candidate
```

At every arrow, the project should state exactly what has become provable and exactly what
remains outside the claim. That discipline is part of the security model.
