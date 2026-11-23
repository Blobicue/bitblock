//! `PROFILE-1@base` (`PS-084`): admitted activated-feature closure, canonical
//! [`ProfileAdmissionManifestV1`] (`bitblock/profile-admission-manifest/v1`),
//! [`ProfileClosureEvidenceSetV1`] (`bitblock/profile-1/evidence-closure/v1`), two-context
//! [`PredicateEnvironmentV1`] (`launch-profile` / `target-profile`) entailment validation,
//! role-aware acyclic closure walking (`CE-01`–`CE-44`), and stage×gate subclaim reconciliation.
//!
//! Strictly behind `--features protocol-research`, 100% `wasm32`-clean, with zero new Cargo
//! dependencies and zero edits to frozen `genesis_d1.rs`, `profile_d1.rs`, or `crates/core/**`.
//!
//! # Normative Boundaries (`CAPABILITY_TREE.md` / `LAYER1_DESIGN_PAPER.md` §2)
//! - **Initial launch vs successor migration:** On `ProfileLineageKindV1::InitialLaunch`
//!   (`predecessor_profile_id = None`), frozen `GENESIS-D1`/`PROFILE-D1` carry
//!   `in-place-upgrade` and `migration-profile` in `CANONICAL_LAUNCH_PREDICATES_V1` while no
//!   prior admitted profile exists to migrate from; `PROFILE-1` explicitly scopes
//!   `G19.upgrade`/`CE-29` to `UPGRADE-FRAMEWORK-1` readiness (`initial_launch_upgrade_framework_scoped = true`)
//!   and forbids fabricated `UPGRADE-1` evidence. On every `SuccessorCandidate`
//!   (`predecessor_profile_id = Some(old_id)`), `PROFILE-1` strictly requires concrete
//!   `UPGRADE-1@old->new` migration evidence with verified polarity (`old_id != new_candidate_id`).
//! - **Optional branches fail closed:** Activating `program-active` requires passing `CE-26`
//!   `PROGRAM-CONSENSUS` evidence; activating `verkle-selected` requires passing `CE-42`
//!   `VERKLE-R1@prototype` evidence whose `CE-40` candidate ID equals `admitted_profile_id`.
//! - **Non-claim:** Profile admission does not prove properties outside the activated closure,
//!   sign a release (`RELEASE-1`), authorize launch (`CHAIN-LAUNCH-1`), or prove runtime safety.

use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::fmt;

use sha2::{Digest, Sha256};

use crate::genesis_d1::GenesisPackageV1;
use crate::genesis_root_parity_1::GenesisRootParityRecordV1;
use crate::profile_d1::{
    BoundProfileManifestV1, ProfileDomainSlotV1, ProfileLineageKindV1, StageGateSubclaimV1,
    SubclaimDispositionV1, SubclaimEvidenceStatusV1,
};
use crate::protocol::{ContentTag, TagKind, hash_canonical};
use crate::transition_spec_1::ResolvedProfileTransitionSpecV1;

/// Canonical schema version for `PROFILE-1` objects.
pub const PROFILE_1_VERSION: u16 = 1;

/// Canonical schema string for [`ProfileClosureEvidenceSetV1`].
pub const PROFILE_1_EVIDENCE_CLOSURE_SCHEMA: &[u8] = b"bitblock/profile-1/evidence-closure/v1";

/// Canonical schema string for [`ProfileAdmissionManifestV1`] (`LAYER1_DESIGN_PAPER.md` §2).
pub const PROFILE_1_ADMISSION_MANIFEST_SCHEMA: &[u8] = b"bitblock/profile-admission-manifest/v1";

const PROF1_POLICY_PREFIX: &[u8] = b"bb.prof1.admission_policy.v1";
const PROF1_ASSUMPTION_PREFIX: &[u8] = b"bb.prof1.assumption.v1";
const PROF1_LOSS_PREFIX: &[u8] = b"bb.prof1.loss_condition.v1";

const PE_RULES_SPEC: &str = concat!(
    "economic-charging>asset-active ",
    "stake-consensus>validator-economics stake-consensus>asset-active stake-consensus>economic-charging ",
    "validator-rewards>validator-economics validator-rewards>asset-active validator-rewards>economic-charging ",
    "post-genesis-program>program-active post-genesis-program>migration-profile ",
    "in-place-upgrade>migration-profile"
);

const KNOWN_PREDICATES_SPEC: &str = concat!(
    "asset-active economic-charging in-place-upgrade migration-profile ",
    "post-genesis-program program-active stake-consensus validator-economics ",
    "validator-rewards verkle-selected"
);

/// Fail-closed validation, codec, closure, or conditional-edge error for `PROFILE-1`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Profile1Error {
    /// Malformed, truncated, overlong, or non-canonical wire encoding.
    InvalidWire(&'static str),
    /// Predicate environment violated registration, `PE-01`..`PE-10` closure, or context isolation.
    PredicateClosureError(&'static str),
    /// Required unconditional or active conditional capability evidence was missing or non-passing.
    MissingPrerequisite(&'static str),
    /// Evidence entry had wrong version, zero commit/digest, or stale status.
    StaleOrWrongVersionEvidence(&'static str),
    /// Conditional edge table disagreed with active predicates or upstream artifacts.
    ConditionalEdgeViolation(&'static str),
    /// `CE-29`/`CE-30`/`CE-31` migration binding had inverted polarity or illegal lineage state.
    MigrationPolarityViolation(&'static str),
    /// `CE-42` `verkle-selected` prototype's `CE-40` candidate ID did not match the target profile.
    VerkleCandidateMismatch,
    /// Inactive domain slot carried non-zero commitment profile or conflicted with predicates.
    InactiveDomainEncodingError(&'static str),
    /// Stage×gate subclaim matrix violated mandatory `G5`/`G15`/`G19` rules or omitted exclusion.
    SubclaimViolation(&'static str),
    /// Realized role-aware dependency graph or compatible union contained a directed cycle.
    RoleGraphCycle,
    /// Attempted to encode or accept a non-passing `ProfileAdmissionDispositionV1`.
    AdmissionRejected(&'static str),
    /// Upstream `PROFILE-D1`, `TRANSITION-SPEC-1`, `GENESIS-D1`, or parity verification failed.
    UpstreamVerification(&'static str),
}

/// Seven unconditional `PROFILE-1` base prerequisites declared in `CAPABILITY_TREE.md`.
pub const UNCONDITIONAL_PROFILE_1_PREREQS: &str = concat!(
    "BOUNDS-1 EXECUTION-ROOT-1 GENESIS-ROOT-PARITY-1 ",
    "GOVERNANCE-1 PROFILE-D1 TRANSITION-SPEC-1 UPGRADE-FRAMEWORK-1"
);

impl fmt::Display for Profile1Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}

impl std::error::Error for Profile1Error {}

/// Evidence passing status for one node in [`ProfileClosureEvidenceSetV1`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(u8)]
pub enum ProfileEvidenceStatusV1 {
    /// Pending, failed, or unverified evidence (always rejected by `PROFILE-1`).
    PendingOrFailed = 0,
    /// Verified `BOUNDED` evidence on merged `origin/main`.
    PassedBounded = 1,
    /// Verified `IMPLEMENTED` evidence on merged `origin/main`.
    PassedImplemented = 2,
}

impl ProfileEvidenceStatusV1 {
    /// Returns the canonical 1-byte wire discriminant.
    pub const fn code(self) -> u8 {
        self as u8
    }

    /// Decodes a canonical 1-byte evidence status discriminant.
    pub const fn from_code(code: u8) -> Result<Self, Profile1Error> {
        match code {
            0 => Ok(Self::PendingOrFailed),
            1 => Ok(Self::PassedBounded),
            2 => Ok(Self::PassedImplemented),
            _ => Err(Profile1Error::InvalidWire(
                "unknown ProfileEvidenceStatusV1",
            )),
        }
    }

    /// Returns `true` iff this status qualifies as passing closure evidence.
    pub const fn is_passing(self) -> bool {
        matches!(self, Self::PassedBounded | Self::PassedImplemented)
    }
}

/// Admission disposition carried by [`ProfileAdmissionManifestV1`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(u8)]
pub enum ProfileAdmissionDispositionV1 {
    /// Rejected admission (never valid in an admitted [`ProfileAdmissionManifestV1`]).
    Rejected = 0,
    /// Sole passing v1 bounded admission disposition.
    AdmittedBoundedV1 = 1,
}

impl ProfileAdmissionDispositionV1 {
    /// Returns the canonical 1-byte wire discriminant.
    pub const fn code(self) -> u8 {
        self as u8
    }

    /// Decodes a canonical 1-byte admission disposition discriminant.
    pub const fn from_code(code: u8) -> Result<Self, Profile1Error> {
        match code {
            0 => Ok(Self::Rejected),
            1 => Ok(Self::AdmittedBoundedV1),
            _ => Err(Profile1Error::InvalidWire(
                "unknown ProfileAdmissionDispositionV1",
            )),
        }
    }
}

/// Declared conditional-edge evaluation context (`CAPABILITY_TREE.md` CE table).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(u8)]
pub enum PredicateContextKindV1 {
    /// Evaluated strictly against immutable launch-profile predicates.
    LaunchProfile = 1,
    /// Evaluated strictly against the candidate target-profile predicates.
    TargetProfile = 2,
}

impl PredicateContextKindV1 {
    /// Returns the canonical 1-byte wire discriminant.
    pub const fn code(self) -> u8 {
        self as u8
    }

    /// Decodes a canonical 1-byte context discriminant.
    pub const fn from_code(code: u8) -> Result<Self, Profile1Error> {
        match code {
            1 => Ok(Self::LaunchProfile),
            2 => Ok(Self::TargetProfile),
            _ => Err(Profile1Error::InvalidWire("unknown PredicateContextKindV1")),
        }
    }
}

/// Computes the deterministic v1 `PROFILE-1` admission policy identifier.
pub fn canonical_admission_policy_id(label: &str) -> [u8; 32] {
    let mut h = Sha256::new();
    h.update(PROF1_POLICY_PREFIX);
    h.update(label.as_bytes());
    h.finalize().into()
}

/// Computes a domain-separated assumption digest for one capability evidence entry.
pub fn digest_capability_assumption(capability_id: &str, summary: &str) -> [u8; 32] {
    let mut h = Sha256::new();
    h.update(PROF1_ASSUMPTION_PREFIX);
    h.update(capability_id.as_bytes());
    h.update(b":");
    h.update(summary.as_bytes());
    h.finalize().into()
}

/// Computes a domain-separated loss-condition digest for one capability evidence entry.
pub fn digest_capability_loss_condition(capability_id: &str, summary: &str) -> [u8; 32] {
    let mut h = Sha256::new();
    h.update(PROF1_LOSS_PREFIX);
    h.update(capability_id.as_bytes());
    h.update(b":");
    h.update(summary.as_bytes());
    h.finalize().into()
}

/// Two-context predicate environment `PredicateEnvironmentV1(launch_profile, target_profile)`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PredicateEnvironmentV1 {
    /// Sorted unique predicates active in the immutable `launch-profile` context.
    pub launch_predicates: Vec<String>,
    /// Sorted unique predicates active in the `target-profile` context being admitted.
    pub target_predicates: Vec<String>,
}

impl PredicateEnvironmentV1 {
    /// Validates independent context-local `PE-01`..`PE-10` entailment closure and lineage rules.
    pub fn validate(&self, lineage: ProfileLineageKindV1) -> Result<(), Profile1Error> {
        validate_pred_list(&self.launch_predicates)?;
        validate_pred_list(&self.target_predicates)?;
        if lineage == ProfileLineageKindV1::InitialLaunch
            && self.launch_predicates != self.target_predicates
        {
            return Err(Profile1Error::PredicateClosureError(
                "InitialLaunch requires identical launch_profile and target_profile predicates",
            ));
        }
        Ok(())
    }

    /// Evaluates one predicate strictly inside its declared context without cross-context leakage.
    pub fn holds(&self, ctx: PredicateContextKindV1, predicate: &str) -> bool {
        if predicate == "always" {
            return true;
        }
        let list = match ctx {
            PredicateContextKindV1::LaunchProfile => &self.launch_predicates,
            PredicateContextKindV1::TargetProfile => &self.target_predicates,
        };
        list.iter().any(|p| p == predicate)
    }
}

fn validate_pred_list(preds: &[String]) -> Result<(), Profile1Error> {
    if preds.is_empty() {
        return Err(Profile1Error::PredicateClosureError("empty predicate set"));
    }
    for w in preds.windows(2) {
        if w[0] >= w[1] {
            return Err(Profile1Error::PredicateClosureError(
                "predicates must be strictly sorted and unique",
            ));
        }
    }
    let set: BTreeSet<&str> = preds.iter().map(String::as_str).collect();
    let known: BTreeSet<&str> = KNOWN_PREDICATES_SPEC.split_ascii_whitespace().collect();
    for &p in &set {
        if p == "always" || !known.contains(p) {
            return Err(Profile1Error::PredicateClosureError(
                "unregistered or reserved predicate token in candidate environment",
            ));
        }
    }
    for pair in PE_RULES_SPEC.split_ascii_whitespace() {
        let (ant, ent) = pair.split_once('>').expect("static entailment");
        if set.contains(ant) && !set.contains(ent) {
            return Err(Profile1Error::PredicateClosureError(
                "missing PE-01..PE-10 entailed predicate in context",
            ));
        }
    }
    Ok(())
}

/// Directed role-aware dependency edge `(from_node, from_role) -> (to_node, to_role)`.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct RoleGraphEdgeV1 {
    /// Owner capability base identifier.
    pub from_node: String,
    /// Owner artifact role token (e.g. `"new-admission"`).
    pub from_role: String,
    /// Prerequisite capability base identifier.
    pub to_node: String,
    /// Prerequisite artifact role token (e.g. `"old-admitted-profile"`).
    pub to_role: String,
}

/// Verifies acyclicity of the realized role-aware dependency graph via Kahn's algorithm.
pub fn verify_role_aware_closure_acyclic(edges: &[RoleGraphEdgeV1]) -> Result<(), Profile1Error> {
    let mut vertices: BTreeSet<(&str, &str)> = BTreeSet::new();
    let mut adj: BTreeMap<(&str, &str), Vec<(&str, &str)>> = BTreeMap::new();
    let mut indeg: BTreeMap<(&str, &str), usize> = BTreeMap::new();

    for e in edges {
        let u = (e.from_node.as_str(), e.from_role.as_str());
        let v = (e.to_node.as_str(), e.to_role.as_str());
        if u == v {
            return Err(Profile1Error::RoleGraphCycle);
        }
        vertices.insert(u);
        vertices.insert(v);
        indeg.entry(u).or_insert(0);
        *indeg.entry(v).or_insert(0) += 1;
        adj.entry(u).or_default().push(v);
    }

    let mut q: VecDeque<(&str, &str)> = indeg
        .iter()
        .filter_map(|(&k, &d)| (d == 0).then_some(k))
        .collect();
    let mut visited = 0usize;
    while let Some(u) = q.pop_front() {
        visited += 1;
        if let Some(nbrs) = adj.get(&u) {
            for &v in nbrs {
                let d = indeg.get_mut(&v).expect("vertex present");
                *d -= 1;
                if *d == 0 {
                    q.push_back(v);
                }
            }
        }
    }
    if visited != vertices.len() {
        return Err(Profile1Error::RoleGraphCycle);
    }
    Ok(())
}

/// One node/version evidence record inside [`ProfileClosureEvidenceSetV1`].
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct ProfileEvidenceNodeEntryV1 {
    /// Canonical capability node identifier (e.g. `"ASSET-1"`, `"PROFILE-D1"`).
    pub capability_id: String,
    /// Exact artifact schema version (`1`).
    pub artifact_version: u16,
    /// Merged `origin/main` 20-byte Git commit SHA pinning the executable artifact.
    pub commit_sha1: [u8; 20],
    /// Canonical 32-byte digest of the verified golden receipt or policy `ContentTag`.
    pub receipt_tag_digest: [u8; 32],
    /// Verified evidence disposition (`PassedBounded` or `PassedImplemented`).
    pub evidence_status: ProfileEvidenceStatusV1,
    /// Domain-separated assumptions digest for this capability.
    pub assumptions_digest: [u8; 32],
    /// Domain-separated loss-condition digest for this capability.
    pub loss_condition_digest: [u8; 32],
}

impl ProfileEvidenceNodeEntryV1 {
    /// Validates non-empty ID, exact version, passing status, and non-zero provenance hashes.
    pub fn validate(&self) -> Result<(), Profile1Error> {
        if self.capability_id.is_empty() || self.capability_id.len() > 64 {
            return Err(Profile1Error::InvalidWire("bad capability_id length"));
        }
        if self.artifact_version != PROFILE_1_VERSION {
            return Err(Profile1Error::StaleOrWrongVersionEvidence(
                "evidence artifact_version must be 1",
            ));
        }
        if !self.evidence_status.is_passing() {
            return Err(Profile1Error::MissingPrerequisite(
                "evidence entry is not passing",
            ));
        }
        if self.commit_sha1 == [0u8; 20]
            || self.receipt_tag_digest == [0u8; 32]
            || self.assumptions_digest == [0u8; 32]
            || self.loss_condition_digest == [0u8; 32]
        {
            return Err(Profile1Error::StaleOrWrongVersionEvidence(
                "zero commit SHA or evidence digest forbidden",
            ));
        }
        Ok(())
    }
}

/// One resolved conditional-edge instance (`CE-01`–`CE-44`) inside [`ProfileClosureEvidenceSetV1`].
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct ResolvedConditionalEdgeV1 {
    /// Canonical conditional edge identifier (e.g. `"CE-25"`).
    pub edge_id: String,
    /// Owner capability base name.
    pub owner_base: String,
    /// Owner role token.
    pub owner_role: String,
    /// Declared evaluation context (`LaunchProfile` or `TargetProfile`).
    pub context: PredicateContextKindV1,
    /// Predicate token that triggered this edge (or `"always"` for `CE-43`).
    pub predicate: String,
    /// Prerequisite capability base name.
    pub dep_base: String,
    /// Prerequisite role token.
    pub dep_role: String,
    /// Exact 32-byte artifact digest settling this dependency role.
    pub dep_artifact_digest: [u8; 32],
}

/// Concrete `CE-29`/`CE-30`/`CE-31` migration binding required for `SuccessorCandidate` admissions.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct MigrationEvidenceBindingV1 {
    /// Already admitted immutable `PROFILE-1@old-profile-id`.
    pub old_admitted_profile_id: [u8; 32],
    /// Distinct `PROFILE-D1@new-candidate-id` being admitted.
    pub new_candidate_profile_id: [u8; 32],
    /// Canonical `UPGRADE-1@old->new` artifact digest.
    pub upgrade_1_artifact_digest: [u8; 32],
}

/// Concrete `CE-40`/`CE-42` Verkle prototype binding required iff `verkle-selected` is active.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct VerklePrototypeBindingV1 {
    /// Exact `CE-40` candidate `profile_id` the Verkle prototype was built against.
    pub ce40_candidate_profile_id: [u8; 32],
    /// Passing `VERKLE-R1@prototype` artifact digest.
    pub verkle_prototype_digest: [u8; 32],
}

/// Complete machine-readable evidence closure (`bitblock/profile-1/evidence-closure/v1`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProfileClosureEvidenceSetV1 {
    /// Schema version (`1`).
    pub closure_version: u16,
    /// Initial launch vs post-genesis successor lineage.
    pub lineage_kind: ProfileLineageKindV1,
    /// `true` iff `InitialLaunch` scopes `G19.upgrade`/`CE-29` to `UPGRADE-FRAMEWORK-1` readiness.
    pub initial_launch_upgrade_framework_scoped: bool,
    /// Strictly sorted unique node evidence records.
    pub evidence_entries: Vec<ProfileEvidenceNodeEntryV1>,
    /// Strictly sorted unique resolved conditional edges.
    pub resolved_conditional_edges: Vec<ResolvedConditionalEdgeV1>,
    /// Exact 8-slot domain activation projection matching `PROFILE-D1`.
    pub domain_slots: Vec<ProfileDomainSlotV1>,
    /// Reconciled 7-row stage×gate subclaim matrix.
    pub subclaims: Vec<StageGateSubclaimV1>,
}

impl ProfileClosureEvidenceSetV1 {
    /// Validates internal sorting, non-empty evidence, inactive-domain encodings, and subclaims.
    pub fn validate(&self) -> Result<(), Profile1Error> {
        if self.closure_version != PROFILE_1_VERSION {
            return Err(Profile1Error::InvalidWire("closure_version must be 1"));
        }
        if self.evidence_entries.is_empty() {
            return Err(Profile1Error::MissingPrerequisite(
                "evidence_entries cannot be empty",
            ));
        }
        for (idx, entry) in self.evidence_entries.iter().enumerate() {
            entry.validate()?;
            if idx > 0 && self.evidence_entries[idx - 1].capability_id >= entry.capability_id {
                return Err(Profile1Error::InvalidWire(
                    "evidence_entries not strictly sorted",
                ));
            }
        }
        for (idx, ce) in self.resolved_conditional_edges.iter().enumerate() {
            if ce.edge_id.is_empty() || ce.dep_artifact_digest == [0u8; 32] {
                return Err(Profile1Error::ConditionalEdgeViolation(
                    "malformed resolved conditional edge",
                ));
            }
            if idx > 0 && self.resolved_conditional_edges[idx - 1].edge_id >= ce.edge_id {
                return Err(Profile1Error::InvalidWire(
                    "resolved_conditional_edges not strictly sorted",
                ));
            }
        }
        if self.domain_slots.len() != 8 {
            return Err(Profile1Error::InactiveDomainEncodingError(
                "domain_slots must carry all 8 canonical slots",
            ));
        }
        for (i, slot) in self.domain_slots.iter().enumerate() {
            if usize::from(slot.slot_id) != i {
                return Err(Profile1Error::InactiveDomainEncodingError(
                    "domain_slots out of order",
                ));
            }
            if !slot.active && slot.commitment_profile_id != 0 {
                return Err(Profile1Error::InactiveDomainEncodingError(
                    "inactive domain slot must encode commitment_profile_id = 0 sentinel",
                ));
            }
            if slot.active && slot.commitment_profile_id == 0 {
                return Err(Profile1Error::InactiveDomainEncodingError(
                    "active domain slot cannot encode zero commitment_profile_id",
                ));
            }
        }
        if self.subclaims.len() != 7 {
            return Err(Profile1Error::SubclaimViolation(
                "subclaim matrix must contain exactly 7 canonical rows",
            ));
        }
        for w in self.subclaims.windows(2) {
            if w[0].subclaim_id >= w[1].subclaim_id {
                return Err(Profile1Error::SubclaimViolation(
                    "subclaims must be strictly sorted",
                ));
            }
        }
        if self.lineage_kind == ProfileLineageKindV1::SuccessorCandidate
            && self.initial_launch_upgrade_framework_scoped
        {
            return Err(Profile1Error::MigrationPolarityViolation(
                "SuccessorCandidate cannot claim initial_launch_upgrade_framework_scoped",
            ));
        }
        Ok(())
    }

    /// Encodes into canonical big-endian wire bytes.
    pub fn encode_canonical(&self) -> Result<Vec<u8>, Profile1Error> {
        self.validate()?;
        let mut buf = Vec::with_capacity(2048);
        buf.extend_from_slice(&self.closure_version.to_be_bytes());
        buf.push(self.lineage_kind.code());
        buf.push(u8::from(self.initial_launch_upgrade_framework_scoped));
        buf.extend_from_slice(&(self.evidence_entries.len() as u16).to_be_bytes());
        for e in &self.evidence_entries {
            put_lp_str(&mut buf, &e.capability_id)?;
            buf.extend_from_slice(&e.artifact_version.to_be_bytes());
            buf.extend_from_slice(&e.commit_sha1);
            buf.extend_from_slice(&e.receipt_tag_digest);
            buf.push(e.evidence_status.code());
            buf.extend_from_slice(&e.assumptions_digest);
            buf.extend_from_slice(&e.loss_condition_digest);
        }
        buf.extend_from_slice(&(self.resolved_conditional_edges.len() as u16).to_be_bytes());
        for ce in &self.resolved_conditional_edges {
            put_lp_str(&mut buf, &ce.edge_id)?;
            put_lp_str(&mut buf, &ce.owner_base)?;
            put_lp_str(&mut buf, &ce.owner_role)?;
            buf.push(ce.context.code());
            put_lp_str(&mut buf, &ce.predicate)?;
            put_lp_str(&mut buf, &ce.dep_base)?;
            put_lp_str(&mut buf, &ce.dep_role)?;
            buf.extend_from_slice(&ce.dep_artifact_digest);
        }
        buf.extend_from_slice(&(self.domain_slots.len() as u16).to_be_bytes());
        for s in &self.domain_slots {
            buf.push(s.slot_id);
            buf.push(u8::from(s.active));
            buf.extend_from_slice(&s.commitment_profile_id.to_be_bytes());
        }
        buf.extend_from_slice(&(self.subclaims.len() as u16).to_be_bytes());
        for sc in &self.subclaims {
            put_lp_str(&mut buf, &sc.subclaim_id)?;
            buf.push(sc.disposition.code());
            put_lp_str(&mut buf, &sc.predicate_or_reason)?;
            buf.push(sc.evidence_status.code());
        }
        Ok(buf)
    }

    /// Decodes and validates a canonical [`ProfileClosureEvidenceSetV1`].
    pub fn decode_canonical(raw: &[u8]) -> Result<Self, Profile1Error> {
        let mut p = 0usize;
        let closure_version = get_u16(raw, &mut p)?;
        let lineage_kind = ProfileLineageKindV1::from_code(get_u8(raw, &mut p)?)
            .map_err(|_| Profile1Error::InvalidWire("bad ProfileLineageKindV1"))?;
        let initial_launch_upgrade_framework_scoped = get_bool(raw, &mut p)?;
        let n_ev = usize::from(get_u16(raw, &mut p)?);
        let mut evidence_entries = Vec::with_capacity(n_ev.min(64));
        for _ in 0..n_ev {
            let capability_id = get_lp_str(raw, &mut p)?;
            let artifact_version = get_u16(raw, &mut p)?;
            let commit_sha1 = get_arr::<20>(raw, &mut p)?;
            let receipt_tag_digest = get_arr::<32>(raw, &mut p)?;
            let evidence_status = ProfileEvidenceStatusV1::from_code(get_u8(raw, &mut p)?)?;
            let assumptions_digest = get_arr::<32>(raw, &mut p)?;
            let loss_condition_digest = get_arr::<32>(raw, &mut p)?;
            evidence_entries.push(ProfileEvidenceNodeEntryV1 {
                capability_id,
                artifact_version,
                commit_sha1,
                receipt_tag_digest,
                evidence_status,
                assumptions_digest,
                loss_condition_digest,
            });
        }
        let n_ce = usize::from(get_u16(raw, &mut p)?);
        let mut resolved_conditional_edges = Vec::with_capacity(n_ce.min(64));
        for _ in 0..n_ce {
            let edge_id = get_lp_str(raw, &mut p)?;
            let owner_base = get_lp_str(raw, &mut p)?;
            let owner_role = get_lp_str(raw, &mut p)?;
            let context = PredicateContextKindV1::from_code(get_u8(raw, &mut p)?)?;
            let predicate = get_lp_str(raw, &mut p)?;
            let dep_base = get_lp_str(raw, &mut p)?;
            let dep_role = get_lp_str(raw, &mut p)?;
            let dep_artifact_digest = get_arr::<32>(raw, &mut p)?;
            resolved_conditional_edges.push(ResolvedConditionalEdgeV1 {
                edge_id,
                owner_base,
                owner_role,
                context,
                predicate,
                dep_base,
                dep_role,
                dep_artifact_digest,
            });
        }
        let n_sl = usize::from(get_u16(raw, &mut p)?);
        let mut domain_slots = Vec::with_capacity(n_sl.min(16));
        for _ in 0..n_sl {
            let slot_id = get_u8(raw, &mut p)?;
            let active = get_bool(raw, &mut p)?;
            let commitment_profile_id = get_u16(raw, &mut p)?;
            domain_slots.push(ProfileDomainSlotV1 {
                slot_id,
                active,
                commitment_profile_id,
            });
        }
        let n_sc = usize::from(get_u16(raw, &mut p)?);
        let mut subclaims = Vec::with_capacity(n_sc.min(16));
        for _ in 0..n_sc {
            let subclaim_id = get_lp_str(raw, &mut p)?;
            let disposition = SubclaimDispositionV1::from_code(get_u8(raw, &mut p)?)
                .map_err(|_| Profile1Error::InvalidWire("bad SubclaimDispositionV1"))?;
            let predicate_or_reason = get_lp_str(raw, &mut p)?;
            let evidence_status = SubclaimEvidenceStatusV1::from_code(get_u8(raw, &mut p)?)
                .map_err(|_| Profile1Error::InvalidWire("bad SubclaimEvidenceStatusV1"))?;
            subclaims.push(StageGateSubclaimV1 {
                subclaim_id,
                disposition,
                predicate_or_reason,
                evidence_status,
            });
        }
        if p != raw.len() {
            return Err(Profile1Error::InvalidWire(
                "trailing bytes in ProfileClosureEvidenceSetV1",
            ));
        }
        let out = Self {
            closure_version,
            lineage_kind,
            initial_launch_upgrade_framework_scoped,
            evidence_entries,
            resolved_conditional_edges,
            domain_slots,
            subclaims,
        };
        out.validate()?;
        Ok(out)
    }

    /// Computes the canonical `Policy` `ContentTag` over `PROFILE_1_EVIDENCE_CLOSURE_SCHEMA`.
    pub fn tag(&self) -> Result<ContentTag, Profile1Error> {
        let enc = self.encode_canonical()?;
        Ok(hash_canonical(
            TagKind::Policy,
            PROFILE_1_EVIDENCE_CLOSURE_SCHEMA,
            enc.len() as u64,
            |w| w(&enc),
        )
        .expect("static schema and bounded length"))
    }
}

/// Canonical `PROFILE-1` output (`bitblock/profile-admission-manifest/v1`, `LAYER1_DESIGN_PAPER.md` §2).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProfileAdmissionManifestV1 {
    /// Schema version (`1`).
    pub schema_version: u16,
    /// Admitted `PROFILE-D1` `profile_id`.
    pub admitted_profile_id: [u8; 32],
    /// Admitted `TRANSITION-SPEC-1` `transition_spec_at_profile_id`.
    pub admitted_transition_spec_at_profile_id: [u8; 32],
    /// Admitted `PROFILE-SCOPE-D0` `profile_scope_digest`.
    pub admitted_profile_scope_digest: [u8; 32],
    /// Admitted `DomainRegistryV1` `domain_registry_digest`.
    pub admitted_domain_registry_digest: [u8; 32],
    /// Frozen `PROFILE-1` admission policy identifier.
    pub admission_policy_id: [u8; 32],
    /// Strictly sorted unique required capability identifiers verified in this closure.
    pub required_capability_ids: Vec<String>,
    /// Canonical `ContentTag` digest of [`ProfileClosureEvidenceSetV1`].
    pub evidence_closure_digest: [u8; 32],
    /// Must strictly be [`ProfileAdmissionDispositionV1::AdmittedBoundedV1`].
    pub admission_disposition: ProfileAdmissionDispositionV1,
}

impl ProfileAdmissionManifestV1 {
    /// Validates every digest, required capability ordering, and passing disposition fail-closed.
    pub fn validate(&self) -> Result<(), Profile1Error> {
        if self.schema_version != PROFILE_1_VERSION {
            return Err(Profile1Error::InvalidWire("schema_version must be 1"));
        }
        if self.admission_disposition != ProfileAdmissionDispositionV1::AdmittedBoundedV1 {
            return Err(Profile1Error::AdmissionRejected(
                "ProfileAdmissionManifestV1 requires AdmittedBoundedV1 disposition",
            ));
        }
        for d in [
            self.admitted_profile_id,
            self.admitted_transition_spec_at_profile_id,
            self.admitted_profile_scope_digest,
            self.admitted_domain_registry_digest,
            self.admission_policy_id,
            self.evidence_closure_digest,
        ] {
            if d == [0u8; 32] {
                return Err(Profile1Error::InvalidWire(
                    "zero digest forbidden in ProfileAdmissionManifestV1",
                ));
            }
        }
        if self.required_capability_ids.is_empty() {
            return Err(Profile1Error::MissingPrerequisite(
                "required_capability_ids cannot be empty",
            ));
        }
        for w in self.required_capability_ids.windows(2) {
            if w[0] >= w[1] {
                return Err(Profile1Error::InvalidWire(
                    "required_capability_ids must be strictly sorted and unique",
                ));
            }
        }
        Ok(())
    }

    /// Encodes into canonical big-endian wire bytes.
    pub fn encode_canonical(&self) -> Result<Vec<u8>, Profile1Error> {
        self.validate()?;
        let mut buf = Vec::with_capacity(512);
        buf.extend_from_slice(&self.schema_version.to_be_bytes());
        buf.extend_from_slice(&self.admitted_profile_id);
        buf.extend_from_slice(&self.admitted_transition_spec_at_profile_id);
        buf.extend_from_slice(&self.admitted_profile_scope_digest);
        buf.extend_from_slice(&self.admitted_domain_registry_digest);
        buf.extend_from_slice(&self.admission_policy_id);
        buf.extend_from_slice(&(self.required_capability_ids.len() as u16).to_be_bytes());
        for cap in &self.required_capability_ids {
            put_lp_str(&mut buf, cap)?;
        }
        buf.extend_from_slice(&self.evidence_closure_digest);
        buf.push(self.admission_disposition.code());
        Ok(buf)
    }

    /// Decodes and validates a canonical [`ProfileAdmissionManifestV1`].
    pub fn decode_canonical(raw: &[u8]) -> Result<Self, Profile1Error> {
        let mut p = 0usize;
        let schema_version = get_u16(raw, &mut p)?;
        let admitted_profile_id = get_arr::<32>(raw, &mut p)?;
        let admitted_transition_spec_at_profile_id = get_arr::<32>(raw, &mut p)?;
        let admitted_profile_scope_digest = get_arr::<32>(raw, &mut p)?;
        let admitted_domain_registry_digest = get_arr::<32>(raw, &mut p)?;
        let admission_policy_id = get_arr::<32>(raw, &mut p)?;
        let n_caps = usize::from(get_u16(raw, &mut p)?);
        let mut required_capability_ids = Vec::with_capacity(n_caps.min(64));
        for _ in 0..n_caps {
            required_capability_ids.push(get_lp_str(raw, &mut p)?);
        }
        let evidence_closure_digest = get_arr::<32>(raw, &mut p)?;
        let admission_disposition = ProfileAdmissionDispositionV1::from_code(get_u8(raw, &mut p)?)?;
        if p != raw.len() {
            return Err(Profile1Error::InvalidWire(
                "trailing bytes in ProfileAdmissionManifestV1",
            ));
        }
        let out = Self {
            schema_version,
            admitted_profile_id,
            admitted_transition_spec_at_profile_id,
            admitted_profile_scope_digest,
            admitted_domain_registry_digest,
            admission_policy_id,
            required_capability_ids,
            evidence_closure_digest,
            admission_disposition,
        };
        out.validate()?;
        Ok(out)
    }

    /// Computes the canonical `Receipt` `ContentTag` whose 32-byte digest IS `profile_admission_manifest_id`.
    pub fn tag(&self) -> Result<ContentTag, Profile1Error> {
        let enc = self.encode_canonical()?;
        Ok(hash_canonical(
            TagKind::Receipt,
            PROFILE_1_ADMISSION_MANIFEST_SCHEMA,
            enc.len() as u64,
            |w| w(&enc),
        )
        .expect("static schema and bounded length"))
    }
}

/// Complete admission request handed to [`admit_candidate_profile`].
#[derive(Clone, Debug)]
pub struct ProfileAdmissionRequestV1<'a> {
    /// Immutable `GENESIS-D1` launch genesis package.
    pub genesis_package: &'a GenesisPackageV1,
    /// Target `PROFILE-D1` bound manifest being admitted.
    pub bound_manifest: &'a BoundProfileManifestV1,
    /// Resolved `TRANSITION-SPEC-1` specification for `bound_manifest`.
    pub transition_spec: &'a ResolvedProfileTransitionSpecV1,
    /// Verified `GENESIS-ROOT-PARITY-1` record for `genesis_package`.
    pub parity_record: &'a GenesisRootParityRecordV1,
    /// Two-context predicate environment (`launch-profile`, `target-profile`).
    pub pred_env: PredicateEnvironmentV1,
    /// Supplied capability evidence records (verified for completeness and freshness).
    pub evidence_entries: Vec<ProfileEvidenceNodeEntryV1>,
    /// Realized role-aware dependency graph checked for strict acyclicity.
    pub role_graph_edges: Vec<RoleGraphEdgeV1>,
    /// Optional `UPGRADE-1` old->new migration binding (required iff `SuccessorCandidate` + `migration-profile`).
    pub migration_binding: Option<MigrationEvidenceBindingV1>,
    /// Optional `VERKLE-R1@prototype` binding (required iff `verkle-selected` active in `target-profile`).
    pub verkle_binding: Option<VerklePrototypeBindingV1>,
    /// Frozen `PROFILE-1` admission policy identifier.
    pub admission_policy_id: [u8; 32],
}

/// Executes the complete fail-closed `PROFILE-1` closure walker and emits both
/// [`ProfileClosureEvidenceSetV1`] and [`ProfileAdmissionManifestV1`].
pub fn admit_candidate_profile(
    req: &ProfileAdmissionRequestV1<'_>,
) -> Result<(ProfileClosureEvidenceSetV1, ProfileAdmissionManifestV1), Profile1Error> {
    req.genesis_package
        .verify()
        .map_err(|_| Profile1Error::UpstreamVerification("GenesisPackageV1 verify failed"))?;
    req.bound_manifest
        .verify()
        .map_err(|_| Profile1Error::UpstreamVerification("BoundProfileManifestV1 verify failed"))?;
    req.transition_spec.validate().map_err(|_| {
        Profile1Error::UpstreamVerification("ResolvedProfileTransitionSpecV1 validate failed")
    })?;
    if req.transition_spec.profile_id != req.bound_manifest.profile_id {
        return Err(Profile1Error::UpstreamVerification(
            "ResolvedProfileTransitionSpecV1 profile_id disagrees with bound manifest",
        ));
    }
    req.parity_record.validate().map_err(|_| {
        Profile1Error::UpstreamVerification("GenesisRootParityRecordV1 validate failed")
    })?;

    let cand = &req.bound_manifest.candidate;
    if cand.genesis_id != req.genesis_package.genesis_id {
        return Err(Profile1Error::UpstreamVerification(
            "bound manifest genesis_id disagrees with GenesisPackageV1",
        ));
    }
    if req.parity_record.genesis_id != req.genesis_package.genesis_id {
        return Err(Profile1Error::UpstreamVerification(
            "parity record genesis_id disagrees with GenesisPackageV1",
        ));
    }
    if cand.lineage_kind == ProfileLineageKindV1::InitialLaunch {
        if req.parity_record.initial_profile_id != req.bound_manifest.profile_id {
            return Err(Profile1Error::UpstreamVerification(
                "InitialLaunch parity record profile_id disagrees with bound manifest",
            ));
        }
        if cand.current_scope_digest != req.genesis_package.state.initial_profile_scope_digest {
            return Err(Profile1Error::UpstreamVerification(
                "InitialLaunch scope digest disagrees with GenesisPackageV1",
            ));
        }
        if cand.domain_registry_digest != req.genesis_package.state.initial_domain_registry_digest {
            return Err(Profile1Error::UpstreamVerification(
                "InitialLaunch domain registry disagrees with GenesisPackageV1",
            ));
        }
    }

    // 1. Validate two-context predicate environment independently of candidate claims.
    req.pred_env.validate(cand.lineage_kind)?;
    if req.pred_env.launch_predicates != req.genesis_package.state.launch_predicates {
        return Err(Profile1Error::PredicateClosureError(
            "launch_profile predicates disagree with immutable GenesisPackageV1",
        ));
    }
    if req.pred_env.target_predicates != cand.activated_predicates {
        return Err(Profile1Error::PredicateClosureError(
            "target_profile predicates disagree with CandidateProfileManifestV1",
        ));
    }

    // 2. Verify role-aware closure graph is strictly acyclic.
    verify_role_aware_closure_acyclic(&req.role_graph_edges)?;

    // 3. Index and validate all supplied evidence records.
    let mut ev_map: BTreeMap<&str, &ProfileEvidenceNodeEntryV1> = BTreeMap::new();
    for entry in &req.evidence_entries {
        entry.validate()?;
        if ev_map.insert(entry.capability_id.as_str(), entry).is_some() {
            return Err(Profile1Error::InvalidWire(
                "duplicate capability_id in evidence_entries",
            ));
        }
    }

    // 4. Require every unconditional prerequisite + resolve active target-context conditional edges.
    let mut required_caps: BTreeSet<String> = UNCONDITIONAL_PROFILE_1_PREREQS
        .split_ascii_whitespace()
        .map(str::to_string)
        .collect();
    let mut resolved_edges: Vec<ResolvedConditionalEdgeV1> = Vec::new();

    // CE-43 (`always`): PROFILE-D1 -> PROFILE-SCOPE-D0@candidate-scope
    resolved_edges.push(ResolvedConditionalEdgeV1 {
        edge_id: "CE-43".to_string(),
        owner_base: "PROFILE-D1".to_string(),
        owner_role: "new-candidate-profile".to_string(),
        context: PredicateContextKindV1::TargetProfile,
        predicate: "always".to_string(),
        dep_base: "PROFILE-SCOPE-D0".to_string(),
        dep_role: "candidate-scope".to_string(),
        dep_artifact_digest: cand.current_scope_digest,
    });

    let target_has = |p: &str| req.pred_env.holds(PredicateContextKindV1::TargetProfile, p);
    for (edge_id, pred, dep_base, dep_role) in [
        ("CE-25", "asset-active", "ASSET-1", "implementation"),
        ("CE-26", "program-active", "PROGRAM-CONSENSUS", "authority"),
        ("CE-27", "economic-charging", "RESOURCE-1", "policy"),
        (
            "CE-28",
            "validator-economics",
            "VALIDATOR-ECON-1",
            "authority",
        ),
    ] {
        if target_has(pred) {
            required_caps.insert(dep_base.to_string());
            let dep_ev = ev_map
                .get(dep_base)
                .ok_or(Profile1Error::MissingPrerequisite(
                    "active target-profile conditional prerequisite missing from evidence",
                ))?;
            resolved_edges.push(ResolvedConditionalEdgeV1 {
                edge_id: edge_id.to_string(),
                owner_base: "PROFILE-1".to_string(),
                owner_role: "new-admission".to_string(),
                context: PredicateContextKindV1::TargetProfile,
                predicate: pred.to_string(),
                dep_base: dep_base.to_string(),
                dep_role: dep_role.to_string(),
                dep_artifact_digest: dep_ev.receipt_tag_digest,
            });
        }
    }

    // CE-29 (`migration-profile`): strict InitialLaunch vs SuccessorCandidate enforcement.
    let mut initial_launch_upgrade_framework_scoped = false;
    if target_has("migration-profile") {
        match cand.lineage_kind {
            ProfileLineageKindV1::InitialLaunch => {
                if req.migration_binding.is_some() {
                    return Err(Profile1Error::MigrationPolarityViolation(
                        "InitialLaunch forbids concrete UPGRADE-1 migration_binding (no prior profile)",
                    ));
                }
                let ufw =
                    ev_map
                        .get("UPGRADE-FRAMEWORK-1")
                        .ok_or(Profile1Error::MissingPrerequisite(
                            "UPGRADE-FRAMEWORK-1 missing",
                        ))?;
                initial_launch_upgrade_framework_scoped = true;
                resolved_edges.push(ResolvedConditionalEdgeV1 {
                    edge_id: "CE-29".to_string(),
                    owner_base: "PROFILE-1".to_string(),
                    owner_role: "new-admission".to_string(),
                    context: PredicateContextKindV1::TargetProfile,
                    predicate: "migration-profile".to_string(),
                    dep_base: "UPGRADE-FRAMEWORK-1".to_string(),
                    dep_role: "initial-launch-framework-readiness".to_string(),
                    dep_artifact_digest: ufw.receipt_tag_digest,
                });
            }
            ProfileLineageKindV1::SuccessorCandidate => {
                let mig = req
                    .migration_binding
                    .ok_or(Profile1Error::MissingPrerequisite(
                        "SuccessorCandidate migration-profile requires concrete UPGRADE-1 binding",
                    ))?;
                let expected_old = cand.predecessor_profile_id.ok_or(
                    Profile1Error::MigrationPolarityViolation("missing predecessor_profile_id"),
                )?;
                if mig.old_admitted_profile_id == mig.new_candidate_profile_id
                    || mig.old_admitted_profile_id != expected_old
                    || mig.new_candidate_profile_id != req.bound_manifest.profile_id
                    || mig.upgrade_1_artifact_digest == [0u8; 32]
                {
                    return Err(Profile1Error::MigrationPolarityViolation(
                        "CE-29/CE-30/CE-31 migration polarity mismatch",
                    ));
                }
                required_caps.insert("UPGRADE-1".to_string());
                let u1 = ev_map
                    .get("UPGRADE-1")
                    .ok_or(Profile1Error::MissingPrerequisite(
                        "UPGRADE-1 evidence missing",
                    ))?;
                if u1.receipt_tag_digest != mig.upgrade_1_artifact_digest {
                    return Err(Profile1Error::StaleOrWrongVersionEvidence(
                        "UPGRADE-1 evidence digest disagrees with migration_binding",
                    ));
                }
                resolved_edges.push(ResolvedConditionalEdgeV1 {
                    edge_id: "CE-29".to_string(),
                    owner_base: "PROFILE-1".to_string(),
                    owner_role: "new-admission".to_string(),
                    context: PredicateContextKindV1::TargetProfile,
                    predicate: "migration-profile".to_string(),
                    dep_base: "UPGRADE-1".to_string(),
                    dep_role: "migration".to_string(),
                    dep_artifact_digest: mig.upgrade_1_artifact_digest,
                });
            }
        }
    }

    // CE-42 (`verkle-selected`): candidate-bound CE-40 prototype identity check.
    if target_has("verkle-selected") {
        let vk = req
            .verkle_binding
            .ok_or(Profile1Error::MissingPrerequisite(
                "verkle-selected requires CE-42 VERKLE-R1@prototype binding",
            ))?;
        if vk.ce40_candidate_profile_id != req.bound_manifest.profile_id {
            return Err(Profile1Error::VerkleCandidateMismatch);
        }
        if vk.verkle_prototype_digest == [0u8; 32] {
            return Err(Profile1Error::StaleOrWrongVersionEvidence(
                "zero VERKLE-R1 prototype digest",
            ));
        }
        required_caps.insert("VERKLE-R1".to_string());
        let vr = ev_map
            .get("VERKLE-R1")
            .ok_or(Profile1Error::MissingPrerequisite(
                "VERKLE-R1 prototype evidence missing",
            ))?;
        if vr.receipt_tag_digest != vk.verkle_prototype_digest {
            return Err(Profile1Error::StaleOrWrongVersionEvidence(
                "VERKLE-R1 evidence digest disagrees with verkle_binding",
            ));
        }
        resolved_edges.push(ResolvedConditionalEdgeV1 {
            edge_id: "CE-42".to_string(),
            owner_base: "PROFILE-1".to_string(),
            owner_role: "new-admission".to_string(),
            context: PredicateContextKindV1::TargetProfile,
            predicate: "verkle-selected".to_string(),
            dep_base: "VERKLE-R1".to_string(),
            dep_role: "prototype".to_string(),
            dep_artifact_digest: vk.verkle_prototype_digest,
        });
    }

    // Verify every required capability is present and passing.
    for cap in &required_caps {
        if !ev_map.contains_key(cap.as_str()) {
            return Err(Profile1Error::MissingPrerequisite(
                "unconditional or active conditional capability missing from evidence set",
            ));
        }
    }

    // 5. Verify inactive-domain encodings match target predicates strictly.
    let expected_slots = ProfileDomainSlotV1::canonical_slots(
        target_has("asset-active"),
        target_has("program-active"),
        target_has("verkle-selected"),
    );
    if cand.domain_slots != expected_slots {
        return Err(Profile1Error::InactiveDomainEncodingError(
            "candidate domain_slots disagrees with target predicates",
        ));
    }

    // 6. Reconcile stage×gate subclaim matrix for admission.
    let mut reconciled_subclaims = Vec::with_capacity(7);
    for sc in &cand.subclaims {
        let mut row = sc.clone();
        match row.subclaim_id.as_str() {
            "G5.core" | "G15.resource" => {
                if row.disposition != SubclaimDispositionV1::Required {
                    return Err(Profile1Error::SubclaimViolation(
                        "G5.core and G15.resource cannot be excluded",
                    ));
                }
                row.evidence_status = SubclaimEvidenceStatusV1::Satisfied;
            }
            "G5.asset" => {
                if target_has("asset-active") {
                    row.evidence_status = SubclaimEvidenceStatusV1::Satisfied;
                }
            }
            "G15.economic" => {
                if target_has("economic-charging") {
                    row.evidence_status = SubclaimEvidenceStatusV1::Satisfied;
                }
            }
            "G19.program" => {
                if target_has("program-active") {
                    row.evidence_status = SubclaimEvidenceStatusV1::Satisfied;
                } else if row.disposition != SubclaimDispositionV1::ExcludedWithReason
                    || row.predicate_or_reason.is_empty()
                {
                    return Err(Profile1Error::SubclaimViolation(
                        "inactive G19.program requires ExcludedWithReason",
                    ));
                }
            }
            "G19.verkle" => {
                if target_has("verkle-selected") {
                    row.evidence_status = SubclaimEvidenceStatusV1::Satisfied;
                } else if row.disposition != SubclaimDispositionV1::ExcludedWithReason
                    || row.predicate_or_reason.is_empty()
                {
                    return Err(Profile1Error::SubclaimViolation(
                        "inactive G19.verkle requires ExcludedWithReason",
                    ));
                }
            }
            "G19.upgrade" => {
                if target_has("in-place-upgrade") {
                    // Initial launch keeps G19.upgrade explicitly bounded to UPGRADE-FRAMEWORK-1
                    // readiness (live migration evidence stays EvidencePending until UPGRADE-1).
                    row.evidence_status = if initial_launch_upgrade_framework_scoped {
                        SubclaimEvidenceStatusV1::EvidencePending
                    } else {
                        SubclaimEvidenceStatusV1::Satisfied
                    };
                } else if row.disposition != SubclaimDispositionV1::ExcludedWithReason {
                    return Err(Profile1Error::SubclaimViolation(
                        "inactive G19.upgrade requires ExcludedWithReason",
                    ));
                }
            }
            _ => {
                return Err(Profile1Error::SubclaimViolation("unknown subclaim_id"));
            }
        }
        reconciled_subclaims.push(row);
    }

    let mut sorted_evidence = req.evidence_entries.clone();
    sorted_evidence.sort_by(|a, b| a.capability_id.cmp(&b.capability_id));
    resolved_edges.sort_by(|a, b| a.edge_id.cmp(&b.edge_id));

    let closure = ProfileClosureEvidenceSetV1 {
        closure_version: PROFILE_1_VERSION,
        lineage_kind: cand.lineage_kind,
        initial_launch_upgrade_framework_scoped,
        evidence_entries: sorted_evidence,
        resolved_conditional_edges: resolved_edges,
        domain_slots: cand.domain_slots.clone(),
        subclaims: reconciled_subclaims,
    };
    let closure_digest = closure.tag()?.digest();

    let spec_id = req
        .transition_spec
        .tag()
        .map_err(|_| Profile1Error::UpstreamVerification("transition_spec tag failed"))?
        .digest();
    let manifest = ProfileAdmissionManifestV1 {
        schema_version: PROFILE_1_VERSION,
        admitted_profile_id: req.bound_manifest.profile_id,
        admitted_transition_spec_at_profile_id: spec_id,
        admitted_profile_scope_digest: cand.current_scope_digest,
        admitted_domain_registry_digest: cand.domain_registry_digest,
        admission_policy_id: req.admission_policy_id,
        required_capability_ids: required_caps.into_iter().collect(),
        evidence_closure_digest: closure_digest,
        admission_disposition: ProfileAdmissionDispositionV1::AdmittedBoundedV1,
    };
    manifest.validate()?;
    Ok((closure, manifest))
}

/// Real merged `origin/main` evidence records for all 10 active launch `PROFILE-1` capabilities
/// (`7` unconditional base prerequisites + `CE-25` `ASSET-1`, `CE-27` `RESOURCE-1`, `CE-28`
/// `VALIDATOR-ECON-1`).
pub fn canonical_devnet_launch_evidence_entries() -> Vec<ProfileEvidenceNodeEntryV1> {
    let rows: [(&str, &str, &str, &str, &str); 10] = [
        (
            "ASSET-1",
            "25fbbf645940461531d98d3c5f80235441df09f2",
            "#bb.receipt.sha256.d4c462nqb6gg3rzzrraufi4cjv752ys33gfmep44wkltiposlvkq",
            "dual-domain-conservation-bedrock-excluded",
            "supply-overflow-or-deposit-leak",
        ),
        (
            "BOUNDS-1",
            "ec5b76358b06351652fde5e9bed05a8d52bb5cad",
            "#bb.policy.sha256.yecxf5k5a5gs3y7qm76pzax2f5e6bkidmxytmsthsxhdqozzplaa",
            "400ms-verify-budget-2mib-block-preflight",
            "unbounded-allocation-or-dos-saturation",
        ),
        (
            "EXECUTION-ROOT-1",
            "6f66ea4652841f2c06b16fb73870ae049b1a2cef",
            "#bb.policy.sha256.3wkwyds7dgkm5hd4vwj26lkokfi6xkeft6d7yl3hzddm3tq6i62q",
            "two-layer-11-slot-inactive-sentinel-composition",
            "cross-domain-root-substitution",
        ),
        (
            "GENESIS-ROOT-PARITY-1",
            "ac4fe8a88896d474cf29e756913d02511fcc5257",
            "#bb.receipt.sha256.3xkh3lfjjgfxg52qhgoqw5zl34zr4d5zax5d6apqufpyiyrpysoa",
            "4-way-preimage-verified-logical-to-typed-root-parity",
            "cold-incremental-executor-python-divergence",
        ),
        (
            "GOVERNANCE-1",
            "35b26afd0f0abe8416a22edd009ce25f8e077be1",
            "#bb.receipt.sha256.ro25qjtz5x6jglbdtk4pzqtlm5tqwfkxfpod53hn2amne7otuhxq",
            "timelocked-quorum-pause-only-council-no-rewrite",
            "unauthorized-upgrade-or-history-rewrite",
        ),
        (
            "PROFILE-D1",
            "7a710fc26abf100c6e4e3f8ddb33058227127f84",
            "#bb.policy.sha256.r4ldc6k33p7evc4zg67ksajcpr6dxeiwuazimy6pxhwgaepepaoq",
            "non-circular-ce43-candidate-manifest-7-subclaims",
            "self-referential-profile-id-or-scope-drift",
        ),
        (
            "RESOURCE-1",
            "438f08e0b09f6ba40e829d46b25c9083b830475f",
            "#bb.policy.sha256.6yvp73mvdkouoxk2xiolnj33lbxucxta5ikhjcwnvd267tzfrfyq",
            "eip1559-burn-capped-tip-refundable-deposit",
            "uncharged-state-growth-or-fee-diversion",
        ),
        (
            "TRANSITION-SPEC-1",
            "27ede9a7c2b71930a8434f0c8744270134139d7f",
            "#bb.policy.sha256.xhy32kud2wdzknglo6p2n3gulbnlikjn3eqhmmquzl2jcofjg6wa",
            "disjoint-256-tag-bands-inactive-vm-wall",
            "tag-collision-or-partial-write-rollback-leak",
        ),
        (
            "UPGRADE-FRAMEWORK-1",
            "d283dedaac924065c80f560cb0d7b1c1f8fadb8b",
            "#bb.policy.sha256.udwdlgfcdx3lmezbw6ffttvoom2bqjv2tlyxnbekhmtswgeyy74q",
            "preventive-lifecycle-terminal-row1-safe-halt",
            "compromised-authority-self-recovery",
        ),
        (
            "VALIDATOR-ECON-1",
            "7ac86ef897b86f28de8a7abae111ea68994443c0",
            "#bb.receipt.sha256.kwwsv73gxzeuug576j55yktxgcdn6zo5zuv4pguns5ewzq4nukda",
            "receipt-exact-consensus-weight-reward-slash-7-vectors",
            "phantom-stake-or-double-slash-bypass",
        ),
    ];
    rows.into_iter()
        .map(
            |(cap, sha_hex, tag_str, assume, loss)| ProfileEvidenceNodeEntryV1 {
                capability_id: cap.to_string(),
                artifact_version: PROFILE_1_VERSION,
                commit_sha1: decode_hex_20(sha_hex),
                receipt_tag_digest: ContentTag::parse_full(tag_str)
                    .expect("valid pinned receipt tag")
                    .digest(),
                evidence_status: ProfileEvidenceStatusV1::PassedBounded,
                assumptions_digest: digest_capability_assumption(cap, assume),
                loss_condition_digest: digest_capability_loss_condition(cap, loss),
            },
        )
        .collect()
}

/// Returns the canonical acyclic role-aware closure edges for initial launch `PROFILE-1`.
pub fn canonical_launch_role_graph_edges() -> Vec<RoleGraphEdgeV1> {
    let raw = [
        (
            "PROFILE-1",
            "new-admission",
            "PROFILE-D1",
            "new-candidate-profile",
        ),
        (
            "PROFILE-1",
            "new-admission",
            "TRANSITION-SPEC-1",
            "resolved-spec",
        ),
        ("PROFILE-1", "new-admission", "BOUNDS-1", "candidate-bounds"),
        (
            "PROFILE-1",
            "new-admission",
            "UPGRADE-FRAMEWORK-1",
            "initial-launch-framework-readiness",
        ),
        (
            "PROFILE-1",
            "new-admission",
            "GENESIS-ROOT-PARITY-1",
            "parity",
        ),
        (
            "PROFILE-1",
            "new-admission",
            "GOVERNANCE-1",
            "implementation",
        ),
        (
            "PROFILE-1",
            "new-admission",
            "EXECUTION-ROOT-1",
            "root-composer",
        ),
        ("PROFILE-1", "new-admission", "ASSET-1", "implementation"),
        ("PROFILE-1", "new-admission", "RESOURCE-1", "policy"),
        (
            "PROFILE-1",
            "new-admission",
            "VALIDATOR-ECON-1",
            "authority",
        ),
        (
            "PROFILE-D1",
            "new-candidate-profile",
            "PROFILE-SCOPE-D0",
            "candidate-scope",
        ),
        (
            "TRANSITION-SPEC-1",
            "resolved-spec",
            "PROFILE-D1",
            "new-candidate-profile",
        ),
        (
            "GENESIS-ROOT-PARITY-1",
            "parity",
            "EXECUTION-ROOT-1",
            "root-composer",
        ),
        (
            "EXECUTION-ROOT-1",
            "root-composer",
            "ASSET-1",
            "implementation",
        ),
    ];
    raw.into_iter()
        .map(|(fn_, fr, tn, tr)| RoleGraphEdgeV1 {
            from_node: fn_.to_string(),
            from_role: fr.to_string(),
            to_node: tn.to_string(),
            to_role: tr.to_string(),
        })
        .collect()
}

/// Builds and admits the canonical `bitblock.devnet.v1` initial launch profile closure.
pub fn build_canonical_devnet_profile_admission(
    genesis_package: &GenesisPackageV1,
    bound_manifest: &BoundProfileManifestV1,
    transition_spec: &ResolvedProfileTransitionSpecV1,
    parity_record: &GenesisRootParityRecordV1,
) -> Result<(ProfileClosureEvidenceSetV1, ProfileAdmissionManifestV1), Profile1Error> {
    let req = ProfileAdmissionRequestV1 {
        genesis_package,
        bound_manifest,
        transition_spec,
        parity_record,
        pred_env: PredicateEnvironmentV1 {
            launch_predicates: genesis_package.state.launch_predicates.clone(),
            target_predicates: bound_manifest.candidate.activated_predicates.clone(),
        },
        evidence_entries: canonical_devnet_launch_evidence_entries(),
        role_graph_edges: canonical_launch_role_graph_edges(),
        migration_binding: None,
        verkle_binding: None,
        admission_policy_id: canonical_admission_policy_id("bitblock-profile-1-launch-policy-v1"),
    };
    admit_candidate_profile(&req)
}

fn decode_hex_20(hex: &str) -> [u8; 20] {
    let bytes = hex.as_bytes();
    assert_eq!(bytes.len(), 40);
    let mut out = [0u8; 20];
    for i in 0..20 {
        let hi = hex_nibble(bytes[2 * i]);
        let lo = hex_nibble(bytes[2 * i + 1]);
        out[i] = (hi << 4) | lo;
    }
    out
}

fn hex_nibble(b: u8) -> u8 {
    match b {
        b'0'..=b'9' => b - b'0',
        b'a'..=b'f' => b - b'a' + 10,
        _ => panic!("invalid lowercase hex nibble"),
    }
}

fn put_lp_str(buf: &mut Vec<u8>, s: &str) -> Result<(), Profile1Error> {
    let raw = s.as_bytes();
    let len = u16::try_from(raw.len()).map_err(|_| Profile1Error::InvalidWire("string > u16"))?;
    buf.extend_from_slice(&len.to_be_bytes());
    buf.extend_from_slice(raw);
    Ok(())
}

fn get_u8(raw: &[u8], p: &mut usize) -> Result<u8, Profile1Error> {
    if *p >= raw.len() {
        return Err(Profile1Error::InvalidWire("unexpected eof u8"));
    }
    let v = raw[*p];
    *p += 1;
    Ok(v)
}

fn get_bool(raw: &[u8], p: &mut usize) -> Result<bool, Profile1Error> {
    match get_u8(raw, p)? {
        0 => Ok(false),
        1 => Ok(true),
        _ => Err(Profile1Error::InvalidWire("non-boolean byte")),
    }
}

fn get_u16(raw: &[u8], p: &mut usize) -> Result<u16, Profile1Error> {
    if raw.len().saturating_sub(*p) < 2 {
        return Err(Profile1Error::InvalidWire("unexpected eof u16"));
    }
    let v = u16::from_be_bytes([raw[*p], raw[*p + 1]]);
    *p += 2;
    Ok(v)
}

fn get_arr<const N: usize>(raw: &[u8], p: &mut usize) -> Result<[u8; N], Profile1Error> {
    if raw.len().saturating_sub(*p) < N {
        return Err(Profile1Error::InvalidWire("unexpected eof fixed array"));
    }
    let out: [u8; N] = raw[*p..*p + N].try_into().expect("N");
    *p += N;
    Ok(out)
}

fn get_lp_str(raw: &[u8], p: &mut usize) -> Result<String, Profile1Error> {
    let len = usize::from(get_u16(raw, p)?);
    if raw.len().saturating_sub(*p) < len {
        return Err(Profile1Error::InvalidWire("unexpected eof string"));
    }
    let s = std::str::from_utf8(&raw[*p..*p + len])
        .map_err(|_| Profile1Error::InvalidWire("invalid utf8"))?
        .to_string();
    *p += len;
    Ok(s)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::genesis_root_parity_1::{
        CanonicalDevnetGenesisFixtureV1, GenesisParityInputV1, verify_genesis_root_parity,
    };

    #[test]
    fn profile_1_canonical_devnet_admission_roundtrip_and_golden_tags() {
        let fx = CanonicalDevnetGenesisFixtureV1::build().unwrap();
        let raw_pkg = fx.package.encode_canonical().unwrap();
        let raw_bnd = fx.bundle.encode_canonical().unwrap();
        let parity = verify_genesis_root_parity(&GenesisParityInputV1 {
            raw_genesis_package: &raw_pkg,
            raw_preimage_bundle: &raw_bnd,
            profile_scope: &fx.profile_scope,
            commit_domain_registry: &fx.commit_domain_registry,
            bound_manifest: &fx.bound_manifest,
            transition_spec: &fx.transition_spec,
            exec_registry: &fx.exec_registry,
        })
        .unwrap();

        let (closure, manifest) = build_canonical_devnet_profile_admission(
            &fx.package,
            &fx.bound_manifest,
            &fx.transition_spec,
            &parity.record,
        )
        .unwrap();

        assert!(closure.initial_launch_upgrade_framework_scoped);
        assert_eq!(closure.evidence_entries.len(), 10);
        assert_eq!(closure.resolved_conditional_edges.len(), 5);
        assert_eq!(
            manifest.admission_disposition,
            ProfileAdmissionDispositionV1::AdmittedBoundedV1
        );

        assert_eq!(
            ProfileClosureEvidenceSetV1::decode_canonical(&closure.encode_canonical().unwrap())
                .unwrap(),
            closure
        );
        assert_eq!(
            ProfileAdmissionManifestV1::decode_canonical(&manifest.encode_canonical().unwrap())
                .unwrap(),
            manifest
        );

        let closure_tag = closure.tag().unwrap().to_string();
        let manifest_tag = manifest.tag().unwrap().to_string();
        assert_eq!(
            closure_tag,
            "#bb.policy.sha256.rdkfncmp3sn6smjeyud6xb3xac35i44ihsl5piicglx3p6pydwga"
        );
        assert_eq!(
            manifest_tag,
            "#bb.receipt.sha256.nmcfo64s3rjesde2eumymbeemqggiqvlcsnvlcpx6ggxm4meynjq"
        );
    }
}
