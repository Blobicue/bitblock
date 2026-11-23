//! `PROFILE-D1`: ID-bound candidate activated-feature manifest, `CE-43` initial/successor scope
//! lineage, ordered domain-slot projection, and stage×gate subclaim matrix (`PS-079`).
//!
//! Freezes one immutable candidate target without pretending unimplemented features have passed
//! `PROFILE-1` admission:
//! - **Non-self-referential derivation:** [`CandidateProfileManifestV1`] excludes its own
//!   `profile_id`; [`BoundProfileManifestV1`] binds `(candidate, derived profile_id)` and rejects
//!   self-reference, lineage cycles, or digest mismatches fail-closed.
//! - **Exact `CE-43` initial-vs-successor scope rules:** an `InitialLaunch` candidate binds
//!   `current_scope_digest == genesis_scope_digest` with `predecessor_profile_id = None`; a
//!   `SuccessorCandidate` chains `predecessor_profile_id = Some(prev_profile_id)` and resolves
//!   `current_scope_digest` to the successor scope while keeping immutable `genesis_id` fixed.
//! - **Complete stage×gate subclaim matrix:** `G5.core` and `G15.resource` are always `Required`;
//!   `G5.asset` and `G15.economic` are `ConditionalWithPredicate` on `asset-active` /
//!   `economic-charging`; each `G19.<feature>` (`program`, `verkle`, `upgrade`) is `Required`
//!   exactly when its predicate is active and otherwise `ExcludedWithReason`. Missing
//!   implementation evidence is explicitly tagged `EvidencePending`.
//!
//! A frozen candidate manifest is not an admitted `PROFILE-1` profile and does not implement or
//! approve any feature it names.

use std::collections::BTreeSet;
use std::fmt;

use crate::genesis_d1::CANONICAL_LAUNCH_PREDICATES_V1;
use crate::profile_bootstrap::verify_protocol_id_graph_acyclic;
use crate::protocol::{ContentTag, TagKind, hash_canonical};
use crate::protocol_id_1::PROTOCOL_ID_1_CANONICAL_DAG;

/// Canonical schema string for a pre-ID [`CandidateProfileManifestV1`] (`PROFILE-D1`).
pub const PROFILE_D1_CANDIDATE_MANIFEST_SCHEMA: &[u8] =
    b"bitblock/profile-d1/candidate-manifest/v1";
/// Canonical schema string for a bound [`BoundProfileManifestV1`] (`PROFILE-D1`).
pub const PROFILE_D1_BOUND_PROFILE_SCHEMA: &[u8] = b"bitblock/profile-d1/bound-profile-manifest/v1";

/// Direct predicate entailments (`PE-01`..`PE-10` from `CAPABILITY_TREE.md`) encoded as
/// `"antecedent>entailed"` pairs so candidate closure checks stay concise.
const PROFILE_PREDICATE_ENTAILMENTS: &str = concat!(
    "economic-charging>asset-active ",
    "stake-consensus>validator-economics stake-consensus>asset-active stake-consensus>economic-charging ",
    "validator-rewards>validator-economics validator-rewards>asset-active validator-rewards>economic-charging ",
    "post-genesis-program>program-active post-genesis-program>migration-profile ",
    "in-place-upgrade>migration-profile"
);

const REGISTERED_PROFILE_PREDICATES: &str = concat!(
    "asset-active economic-charging in-place-upgrade migration-profile ",
    "post-genesis-program program-active stake-consensus validator-economics ",
    "validator-rewards verkle-selected"
);

/// Errors returned by `PROFILE-D1` manifest validation, codec, lineage, or subclaim checks.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ProfileD1Error {
    /// Candidate manifest violated lineage, predicate closure, domain-slot, or subclaim rules.
    InvalidManifest(&'static str),
    /// Wire buffer was truncated, had invalid UTF-8, invalid discriminant, or trailing bytes.
    InvalidWire(&'static str),
    /// Derived `profile_id` did not match the canonical digest of `CandidateProfileManifestV1`.
    ProfileIdMismatch,
    /// Detected self-reference (`profile_id` leaked into a pre-ID digest or `predecessor_profile_id`).
    SelfReferenceDetected,
}

impl fmt::Display for ProfileD1Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}

impl std::error::Error for ProfileD1Error {}

/// Distinguishes initial launch candidates from `CE-43` successor candidates (`PROFILE-D1`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(u8)]
pub enum ProfileLineageKindV1 {
    /// Initial chain-launch candidate (`predecessor_profile_id = None`, `current == genesis` scope).
    InitialLaunch = 0,
    /// Post-genesis successor candidate (`predecessor_profile_id = Some(...)`, updated current scope).
    SuccessorCandidate = 1,
}

impl ProfileLineageKindV1 {
    /// Returns the canonical 1-byte wire discriminant.
    pub const fn code(self) -> u8 {
        self as u8
    }

    /// Decodes a canonical 1-byte lineage discriminant.
    pub const fn from_code(code: u8) -> Result<Self, ProfileD1Error> {
        match code {
            0 => Ok(Self::InitialLaunch),
            1 => Ok(Self::SuccessorCandidate),
            _ => Err(ProfileD1Error::InvalidWire("unknown ProfileLineageKindV1")),
        }
    }
}

/// Ordered active-or-inactive commitment domain slot projected in [`CandidateProfileManifestV1`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ProfileDomainSlotV1 {
    /// Canonical domain slot ordinal (`0..=7`).
    pub slot_id: u8,
    /// `true` when this domain is active in the candidate profile; `false` = typed inactive sentinel.
    pub active: bool,
    /// Commitment profile ID (`1` = Candidate A binary Patricia when active; `0` when inactive).
    pub commitment_profile_id: u16,
}

impl ProfileDomainSlotV1 {
    /// Fixed canonical wire byte length (`4` bytes).
    pub const WIRE_LEN: usize = 4;

    /// Returns the canonical 8-slot domain projection for a given `(asset_active, program_active, verkle_selected)`.
    pub const fn canonical_slots(
        asset_active: bool,
        program_active: bool,
        verkle_selected: bool,
    ) -> [Self; 8] {
        [
            Self {
                slot_id: 0,
                active: true,
                commitment_profile_id: 1,
            },
            Self {
                slot_id: 1,
                active: true,
                commitment_profile_id: 1,
            },
            Self {
                slot_id: 2,
                active: true,
                commitment_profile_id: 1,
            },
            Self {
                slot_id: 3,
                active: true,
                commitment_profile_id: 1,
            },
            Self {
                slot_id: 4,
                active: true,
                commitment_profile_id: 1,
            },
            Self {
                slot_id: 5,
                active: asset_active,
                commitment_profile_id: if asset_active { 1 } else { 0 },
            },
            Self {
                slot_id: 6,
                active: program_active,
                commitment_profile_id: if program_active { 1 } else { 0 },
            },
            Self {
                slot_id: 7,
                active: verkle_selected,
                commitment_profile_id: if verkle_selected { 2 } else { 0 },
            },
        ]
    }
}

/// Normative disposition for one entry in the `PROFILE-D1` stage×gate subclaim matrix.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(u8)]
pub enum SubclaimDispositionV1 {
    /// Mandatory gate subclaim for this candidate profile.
    Required = 0,
    /// Conditional gate subclaim tied to a named registered predicate token.
    ConditionalWithPredicate = 1,
    /// Excluded optional feature subclaim with an explicit non-empty exclusion reason.
    ExcludedWithReason = 2,
}

impl SubclaimDispositionV1 {
    /// Returns the canonical 1-byte wire discriminant.
    pub const fn code(self) -> u8 {
        self as u8
    }

    /// Decodes a canonical 1-byte subclaim disposition discriminant.
    pub const fn from_code(code: u8) -> Result<Self, ProfileD1Error> {
        match code {
            0 => Ok(Self::Required),
            1 => Ok(Self::ConditionalWithPredicate),
            2 => Ok(Self::ExcludedWithReason),
            _ => Err(ProfileD1Error::InvalidWire("unknown SubclaimDispositionV1")),
        }
    }
}

/// Explicit evidence reporting status inside a candidate manifest (never fakes `PROFILE-1` admission).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(u8)]
pub enum SubclaimEvidenceStatusV1 {
    /// Bounded or implemented evidence already exists for this subclaim in the candidate lineage.
    Satisfied = 0,
    /// Implementation evidence is explicitly pending (blocks `PROFILE-1`, allowed in `PROFILE-D1`).
    EvidencePending = 1,
}

impl SubclaimEvidenceStatusV1 {
    /// Returns the canonical 1-byte wire discriminant.
    pub const fn code(self) -> u8 {
        self as u8
    }

    /// Decodes a canonical 1-byte evidence status discriminant.
    pub const fn from_code(code: u8) -> Result<Self, ProfileD1Error> {
        match code {
            0 => Ok(Self::Satisfied),
            1 => Ok(Self::EvidencePending),
            _ => Err(ProfileD1Error::InvalidWire(
                "unknown SubclaimEvidenceStatusV1",
            )),
        }
    }
}

/// One row of the `PROFILE-D1` stage×gate subclaim matrix.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct StageGateSubclaimV1 {
    /// Canonical subclaim identifier (e.g. `"G5.core"`, `"G15.resource"`, `"G19.program"`).
    pub subclaim_id: String,
    /// Required, conditional-with-predicate, or excluded-with-reason disposition.
    pub disposition: SubclaimDispositionV1,
    /// Bound predicate token (for conditional / active feature) or explicit reason (for excluded).
    pub predicate_or_reason: String,
    /// Explicit evidence status (`Satisfied` or `EvidencePending`).
    pub evidence_status: SubclaimEvidenceStatusV1,
}

/// Non-self-referential candidate profile manifest (`bitblock/profile-d1/candidate-manifest/v1`).
/// Excludes its own derived `profile_id`.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct CandidateProfileManifestV1 {
    /// Manifest schema version (`1`).
    pub manifest_version: u16,
    /// Initial launch candidate or `CE-43` successor candidate.
    pub lineage_kind: ProfileLineageKindV1,
    /// `None` on `InitialLaunch`; `Some(prev_profile_id)` on `SuccessorCandidate`.
    pub predecessor_profile_id: Option<[u8; 32]>,
    /// Immutable derived `genesis_id` from `GENESIS-D1`.
    pub genesis_id: [u8; 32],
    /// Digest of the genesis-owned `ProfileScopeV1`.
    pub genesis_scope_digest: [u8; 32],
    /// Digest of the exact current `ProfileScopeV1` (`CE-43`: equals `genesis_scope_digest` on launch).
    pub current_scope_digest: [u8; 32],
    /// Digest of the canonical `DomainRegistryV1`.
    pub domain_registry_digest: [u8; 32],
    /// Digest of the validated `ProtocolBoundsProfileV1` (`BOUNDS-1`).
    pub protocol_bounds_digest: [u8; 32],
    /// Digest of the frozen `CryptoSuiteDescriptorV1` (`CRYPTO-D1`).
    pub crypto_suite_digest: [u8; 32],
    /// Digest of the frozen `ResourcePolicyV1` (`RESOURCE-1`).
    pub resource_policy_digest: [u8; 32],
    /// Strictly sorted unique active predicate tokens closed under `PE-01`..`PE-10`.
    pub activated_predicates: Vec<String>,
    /// Strictly sorted unique activated capability node IDs.
    pub activated_capabilities: Vec<String>,
    /// Ordered 8-slot domain activation projection (`0..=7`).
    pub domain_slots: Vec<ProfileDomainSlotV1>,
    /// Strictly sorted complete stage×gate subclaim matrix (7 canonical rows).
    pub subclaims: Vec<StageGateSubclaimV1>,
}

fn check_sorted_unique(items: &[String], empty_err: &'static str) -> Result<(), ProfileD1Error> {
    if items.is_empty() {
        return Err(ProfileD1Error::InvalidManifest(empty_err));
    }
    for s in items {
        if s.is_empty() || s.len() > 64 {
            return Err(ProfileD1Error::InvalidManifest("invalid string token len"));
        }
    }
    for w in items.windows(2) {
        if w[0] >= w[1] {
            return Err(ProfileD1Error::InvalidManifest(
                "tokens not strictly sorted/unique",
            ));
        }
    }
    Ok(())
}

fn write_str(out: &mut Vec<u8>, s: &str) {
    out.extend_from_slice(&(s.len() as u16).to_be_bytes());
    out.extend_from_slice(s.as_bytes());
}

fn read_str(bytes: &[u8], cur: &mut usize) -> Result<String, ProfileD1Error> {
    if bytes.len().saturating_sub(*cur) < 2 {
        return Err(ProfileD1Error::InvalidWire("truncated string length"));
    }
    let len = u16::from_be_bytes([bytes[*cur], bytes[*cur + 1]]) as usize;
    *cur += 2;
    if bytes.len().saturating_sub(*cur) < len {
        return Err(ProfileD1Error::InvalidWire("truncated string payload"));
    }
    let s = std::str::from_utf8(&bytes[*cur..*cur + len])
        .map_err(|_| ProfileD1Error::InvalidWire("invalid UTF-8"))?
        .to_string();
    *cur += len;
    Ok(s)
}

fn canonical_subclaims_for(pred_set: &BTreeSet<&str>) -> Vec<StageGateSubclaimV1> {
    let has = |k: &str| pred_set.contains(k);
    let mut rows = vec![
        StageGateSubclaimV1 {
            subclaim_id: "G15.economic".to_string(),
            disposition: SubclaimDispositionV1::ConditionalWithPredicate,
            predicate_or_reason: "economic-charging".to_string(),
            evidence_status: SubclaimEvidenceStatusV1::Satisfied,
        },
        StageGateSubclaimV1 {
            subclaim_id: "G15.resource".to_string(),
            disposition: SubclaimDispositionV1::Required,
            predicate_or_reason: String::new(),
            evidence_status: SubclaimEvidenceStatusV1::Satisfied,
        },
        StageGateSubclaimV1 {
            subclaim_id: "G19.program".to_string(),
            disposition: if has("program-active") {
                SubclaimDispositionV1::Required
            } else {
                SubclaimDispositionV1::ExcludedWithReason
            },
            predicate_or_reason: if has("program-active") {
                "program-active".to_string()
            } else {
                "program-active-inactive-at-launch".to_string()
            },
            evidence_status: if has("program-active") {
                SubclaimEvidenceStatusV1::EvidencePending
            } else {
                SubclaimEvidenceStatusV1::Satisfied
            },
        },
        StageGateSubclaimV1 {
            subclaim_id: "G19.upgrade".to_string(),
            disposition: if has("in-place-upgrade") {
                SubclaimDispositionV1::Required
            } else {
                SubclaimDispositionV1::ExcludedWithReason
            },
            predicate_or_reason: if has("in-place-upgrade") {
                "in-place-upgrade".to_string()
            } else {
                "in-place-upgrade-inactive".to_string()
            },
            evidence_status: SubclaimEvidenceStatusV1::EvidencePending,
        },
        StageGateSubclaimV1 {
            subclaim_id: "G19.verkle".to_string(),
            disposition: if has("verkle-selected") {
                SubclaimDispositionV1::Required
            } else {
                SubclaimDispositionV1::ExcludedWithReason
            },
            predicate_or_reason: if has("verkle-selected") {
                "verkle-selected".to_string()
            } else {
                "verkle-selected-inactive-at-launch".to_string()
            },
            evidence_status: SubclaimEvidenceStatusV1::Satisfied,
        },
        StageGateSubclaimV1 {
            subclaim_id: "G5.asset".to_string(),
            disposition: SubclaimDispositionV1::ConditionalWithPredicate,
            predicate_or_reason: "asset-active".to_string(),
            evidence_status: SubclaimEvidenceStatusV1::EvidencePending,
        },
        StageGateSubclaimV1 {
            subclaim_id: "G5.core".to_string(),
            disposition: SubclaimDispositionV1::Required,
            predicate_or_reason: String::new(),
            evidence_status: SubclaimEvidenceStatusV1::Satisfied,
        },
    ];
    rows.sort_by(|a, b| a.subclaim_id.cmp(&b.subclaim_id));
    rows
}

impl CandidateProfileManifestV1 {
    /// Builds the canonical v1 initial launch candidate aligned verbatim with `GENESIS-D1`
    /// (7 launch predicates; `program-active` & `verkle-selected` explicitly INACTIVE).
    pub fn canonical_launch_v1(
        genesis_id: [u8; 32],
        genesis_scope_digest: [u8; 32],
        domain_registry_digest: [u8; 32],
        protocol_bounds_digest: [u8; 32],
        crypto_suite_digest: [u8; 32],
        resource_policy_digest: [u8; 32],
    ) -> Result<Self, ProfileD1Error> {
        let preds: Vec<String> = CANONICAL_LAUNCH_PREDICATES_V1
            .iter()
            .map(|s| (*s).to_string())
            .collect();
        let pred_set: BTreeSet<&str> = preds.iter().map(String::as_str).collect();
        let subclaims = canonical_subclaims_for(&pred_set);
        let caps: Vec<String> = concat!(
            "ASSET-1 AUTH-COMMIT-1 BOUNDS-1 CODEC-1 COMMIT-1 CONSENSUS-1 CRYPTO-1 ",
            "EXECUTION-ROOT-1 EXECUTOR-1 GOVERNANCE-1 IDENTITY-1 PROTOCOL-ID-1 ",
            "RESOURCE-1 TRANSITION-SPEC-1 VALIDATOR-SCHEDULE-1"
        )
        .split_ascii_whitespace()
        .map(str::to_string)
        .collect();
        let manifest = Self {
            manifest_version: 1,
            lineage_kind: ProfileLineageKindV1::InitialLaunch,
            predecessor_profile_id: None,
            genesis_id,
            genesis_scope_digest,
            current_scope_digest: genesis_scope_digest,
            domain_registry_digest,
            protocol_bounds_digest,
            crypto_suite_digest,
            resource_policy_digest,
            activated_predicates: preds,
            activated_capabilities: caps,
            domain_slots: ProfileDomainSlotV1::canonical_slots(true, false, false).to_vec(),
            subclaims,
        };
        manifest.validate()?;
        Ok(manifest)
    }

    /// Builds a `CE-43` successor candidate activating `program-active` + `post-genesis-program`
    /// while chaining `predecessor_profile_id = Some(launch.profile_id)` and preserving immutable
    /// `genesis_id`.
    pub fn canonical_successor_program_v1(
        launch: &BoundProfileManifestV1,
        successor_scope_digest: [u8; 32],
    ) -> Result<Self, ProfileD1Error> {
        launch.verify()?;
        let mut preds = launch.candidate.activated_predicates.clone();
        preds.push("post-genesis-program".to_string());
        preds.push("program-active".to_string());
        preds.sort();
        let pred_set: BTreeSet<&str> = preds.iter().map(String::as_str).collect();
        let subclaims = canonical_subclaims_for(&pred_set);
        let mut caps = launch.candidate.activated_capabilities.clone();
        caps.push("PROGRAM-STATE-1".to_string());
        caps.push("VM-1".to_string());
        caps.sort();
        let manifest = Self {
            manifest_version: 1,
            lineage_kind: ProfileLineageKindV1::SuccessorCandidate,
            predecessor_profile_id: Some(launch.profile_id),
            genesis_id: launch.candidate.genesis_id,
            genesis_scope_digest: launch.candidate.genesis_scope_digest,
            current_scope_digest: successor_scope_digest,
            domain_registry_digest: launch.candidate.domain_registry_digest,
            protocol_bounds_digest: launch.candidate.protocol_bounds_digest,
            crypto_suite_digest: launch.candidate.crypto_suite_digest,
            resource_policy_digest: launch.candidate.resource_policy_digest,
            activated_predicates: preds,
            activated_capabilities: caps,
            domain_slots: ProfileDomainSlotV1::canonical_slots(true, true, false).to_vec(),
            subclaims,
        };
        manifest.validate()?;
        Ok(manifest)
    }

    /// Returns all pre-ID 32-byte digest slots used for non-self-reference checking.
    pub fn pre_id_digests(&self) -> Vec<[u8; 32]> {
        let mut out = vec![
            self.genesis_id,
            self.genesis_scope_digest,
            self.current_scope_digest,
            self.domain_registry_digest,
            self.protocol_bounds_digest,
            self.crypto_suite_digest,
            self.resource_policy_digest,
        ];
        if let Some(prev) = self.predecessor_profile_id {
            out.push(prev);
        }
        out
    }

    /// Validates lineage (`CE-43`), predicate entailment closure (`PE-01`..`PE-10`), domain slots,
    /// and every stage×gate subclaim invariant fail-closed.
    pub fn validate(&self) -> Result<(), ProfileD1Error> {
        if self.manifest_version != 1 {
            return Err(ProfileD1Error::InvalidManifest(
                "manifest_version must be 1",
            ));
        }
        verify_protocol_id_graph_acyclic(PROTOCOL_ID_1_CANONICAL_DAG)
            .map_err(|_| ProfileD1Error::InvalidManifest("identifier DAG cycle"))?;

        match self.lineage_kind {
            ProfileLineageKindV1::InitialLaunch => {
                if self.predecessor_profile_id.is_some() {
                    return Err(ProfileD1Error::InvalidManifest(
                        "InitialLaunch forbids predecessor_profile_id",
                    ));
                }
                if self.current_scope_digest != self.genesis_scope_digest {
                    return Err(ProfileD1Error::InvalidManifest(
                        "InitialLaunch current_scope_digest must equal genesis_scope_digest",
                    ));
                }
            }
            ProfileLineageKindV1::SuccessorCandidate => {
                let Some(prev) = self.predecessor_profile_id else {
                    return Err(ProfileD1Error::InvalidManifest(
                        "SuccessorCandidate requires predecessor_profile_id",
                    ));
                };
                if prev == [0u8; 32] || prev == self.genesis_id {
                    return Err(ProfileD1Error::InvalidManifest(
                        "invalid predecessor_profile_id",
                    ));
                }
                if self.current_scope_digest == self.genesis_scope_digest {
                    return Err(ProfileD1Error::InvalidManifest(
                        "SuccessorCandidate current_scope_digest must differ from genesis scope",
                    ));
                }
            }
        }
        if self.genesis_id == [0u8; 32] || self.current_scope_digest == [0u8; 32] {
            return Err(ProfileD1Error::InvalidManifest(
                "zero genesis or scope digest",
            ));
        }

        check_sorted_unique(&self.activated_predicates, "empty predicates")?;
        check_sorted_unique(&self.activated_capabilities, "empty capabilities")?;
        let pred_set: BTreeSet<&str> = self
            .activated_predicates
            .iter()
            .map(String::as_str)
            .collect();
        let registered: BTreeSet<&str> = REGISTERED_PROFILE_PREDICATES
            .split_ascii_whitespace()
            .collect();
        for &p in &pred_set {
            if !registered.contains(p) {
                return Err(ProfileD1Error::InvalidManifest(
                    "unregistered predicate token",
                ));
            }
        }
        for pair in PROFILE_PREDICATE_ENTAILMENTS.split_ascii_whitespace() {
            let (ant, ent) = pair.split_once('>').expect("static entailment pair");
            if pred_set.contains(ant) && !pred_set.contains(ent) {
                return Err(ProfileD1Error::InvalidManifest(
                    "predicate closure missing entailed token",
                ));
            }
        }

        // Exact 8-slot domain projection consistency.
        let expected_slots = ProfileDomainSlotV1::canonical_slots(
            pred_set.contains("asset-active"),
            pred_set.contains("program-active"),
            pred_set.contains("verkle-selected"),
        );
        if self.domain_slots != expected_slots {
            return Err(ProfileD1Error::InvalidManifest(
                "domain_slots disagrees with activated predicates",
            ));
        }

        // Complete, strictly ordered stage×gate subclaim matrix enforcement.
        if self.subclaims.len() != 7 {
            return Err(ProfileD1Error::InvalidManifest(
                "subclaim matrix must have 7 rows",
            ));
        }
        for w in self.subclaims.windows(2) {
            if w[0].subclaim_id >= w[1].subclaim_id {
                return Err(ProfileD1Error::InvalidManifest(
                    "subclaims not sorted/unique",
                ));
            }
        }
        let find = |id: &str| {
            self.subclaims.iter().find(|r| r.subclaim_id == id).ok_or(
                ProfileD1Error::InvalidManifest("missing mandatory subclaim row"),
            )
        };
        if find("G5.core")?.disposition != SubclaimDispositionV1::Required
            || find("G15.resource")?.disposition != SubclaimDispositionV1::Required
        {
            return Err(ProfileD1Error::InvalidManifest(
                "G5.core and G15.resource must always be Required",
            ));
        }
        let g5_asset = find("G5.asset")?;
        let g15_econ = find("G15.economic")?;
        if g5_asset.disposition != SubclaimDispositionV1::ConditionalWithPredicate
            || g5_asset.predicate_or_reason != "asset-active"
            || g15_econ.disposition != SubclaimDispositionV1::ConditionalWithPredicate
            || g15_econ.predicate_or_reason != "economic-charging"
        {
            return Err(ProfileD1Error::InvalidManifest(
                "economic subclaims must bind their named economic predicates",
            ));
        }
        for (sid, pred) in [
            ("G19.program", "program-active"),
            ("G19.verkle", "verkle-selected"),
            ("G19.upgrade", "in-place-upgrade"),
        ] {
            let row = find(sid)?;
            if pred_set.contains(pred) {
                if row.disposition != SubclaimDispositionV1::Required
                    || row.predicate_or_reason != pred
                {
                    return Err(ProfileD1Error::InvalidManifest(
                        "active G19 feature subclaim must be Required with its predicate",
                    ));
                }
            } else if row.disposition != SubclaimDispositionV1::ExcludedWithReason
                || row.predicate_or_reason.is_empty()
            {
                return Err(ProfileD1Error::InvalidManifest(
                    "inactive G19 feature subclaim must be ExcludedWithReason",
                ));
            }
        }
        Ok(())
    }

    /// Encodes the pre-ID candidate manifest into canonical big-endian wire bytes.
    pub fn encode_canonical(&self) -> Result<Vec<u8>, ProfileD1Error> {
        self.validate()?;
        let mut out = Vec::with_capacity(512);
        out.extend_from_slice(&self.manifest_version.to_be_bytes());
        out.push(self.lineage_kind.code());
        if let Some(prev) = self.predecessor_profile_id {
            out.push(1);
            out.extend_from_slice(&prev);
        } else {
            out.push(0);
            out.extend_from_slice(&[0u8; 32]);
        }
        let digests = [
            self.genesis_id,
            self.genesis_scope_digest,
            self.current_scope_digest,
            self.domain_registry_digest,
            self.protocol_bounds_digest,
            self.crypto_suite_digest,
            self.resource_policy_digest,
        ];
        for d in digests {
            out.extend_from_slice(&d);
        }
        out.extend_from_slice(&(self.activated_predicates.len() as u16).to_be_bytes());
        for p in &self.activated_predicates {
            write_str(&mut out, p);
        }
        out.extend_from_slice(&(self.activated_capabilities.len() as u16).to_be_bytes());
        for c in &self.activated_capabilities {
            write_str(&mut out, c);
        }
        out.extend_from_slice(&(self.domain_slots.len() as u16).to_be_bytes());
        for s in &self.domain_slots {
            out.push(s.slot_id);
            out.push(u8::from(s.active));
            out.extend_from_slice(&s.commitment_profile_id.to_be_bytes());
        }
        out.extend_from_slice(&(self.subclaims.len() as u16).to_be_bytes());
        for row in &self.subclaims {
            write_str(&mut out, &row.subclaim_id);
            out.push(row.disposition.code());
            write_str(&mut out, &row.predicate_or_reason);
            out.push(row.evidence_status.code());
        }
        Ok(out)
    }

    /// Decodes and validates a canonical [`CandidateProfileManifestV1`].
    pub fn decode_canonical(bytes: &[u8]) -> Result<Self, ProfileD1Error> {
        const FIXED_HEAD: usize = 2 + 1 + 1 + 32 + 7 * 32;
        if bytes.len() < FIXED_HEAD + 8 {
            return Err(ProfileD1Error::InvalidWire(
                "truncated CandidateProfileManifestV1",
            ));
        }
        let manifest_version = u16::from_be_bytes([bytes[0], bytes[1]]);
        let lineage_kind = ProfileLineageKindV1::from_code(bytes[2])?;
        let predecessor_profile_id = match bytes[3] {
            0 => {
                if bytes[4..36].iter().any(|&b| b != 0) {
                    return Err(ProfileD1Error::InvalidWire("non-zero absent predecessor"));
                }
                None
            }
            1 => Some(bytes[4..36].try_into().expect("32")),
            _ => return Err(ProfileD1Error::InvalidWire("invalid predecessor flag")),
        };
        let mut cur = 36;
        let mut next32 = || -> [u8; 32] {
            let arr: [u8; 32] = bytes[cur..cur + 32].try_into().expect("32");
            cur += 32;
            arr
        };
        let genesis_id = next32();
        let genesis_scope_digest = next32();
        let current_scope_digest = next32();
        let domain_registry_digest = next32();
        let protocol_bounds_digest = next32();
        let crypto_suite_digest = next32();
        let resource_policy_digest = next32();

        let read_u16 = |c: &mut usize| -> Result<usize, ProfileD1Error> {
            if bytes.len().saturating_sub(*c) < 2 {
                return Err(ProfileD1Error::InvalidWire("truncated count"));
            }
            let n = u16::from_be_bytes([bytes[*c], bytes[*c + 1]]) as usize;
            *c += 2;
            Ok(n)
        };

        let pred_n = read_u16(&mut cur)?;
        let mut activated_predicates = Vec::with_capacity(pred_n.min(32));
        for _ in 0..pred_n {
            activated_predicates.push(read_str(bytes, &mut cur)?);
        }
        let cap_n = read_u16(&mut cur)?;
        let mut activated_capabilities = Vec::with_capacity(cap_n.min(64));
        for _ in 0..cap_n {
            activated_capabilities.push(read_str(bytes, &mut cur)?);
        }
        let slot_n = read_u16(&mut cur)?;
        let mut domain_slots = Vec::with_capacity(slot_n.min(16));
        for _ in 0..slot_n {
            if bytes.len().saturating_sub(cur) < ProfileDomainSlotV1::WIRE_LEN {
                return Err(ProfileD1Error::InvalidWire("truncated domain slot"));
            }
            let active = match bytes[cur + 1] {
                0 => false,
                1 => true,
                _ => return Err(ProfileD1Error::InvalidWire("invalid slot bool")),
            };
            domain_slots.push(ProfileDomainSlotV1 {
                slot_id: bytes[cur],
                active,
                commitment_profile_id: u16::from_be_bytes([bytes[cur + 2], bytes[cur + 3]]),
            });
            cur += ProfileDomainSlotV1::WIRE_LEN;
        }
        let sub_n = read_u16(&mut cur)?;
        let mut subclaims = Vec::with_capacity(sub_n.min(16));
        for _ in 0..sub_n {
            let subclaim_id = read_str(bytes, &mut cur)?;
            if bytes.len().saturating_sub(cur) < 1 {
                return Err(ProfileD1Error::InvalidWire(
                    "truncated subclaim disposition",
                ));
            }
            let disposition = SubclaimDispositionV1::from_code(bytes[cur])?;
            cur += 1;
            let predicate_or_reason = read_str(bytes, &mut cur)?;
            if bytes.len().saturating_sub(cur) < 1 {
                return Err(ProfileD1Error::InvalidWire("truncated subclaim evidence"));
            }
            let evidence_status = SubclaimEvidenceStatusV1::from_code(bytes[cur])?;
            cur += 1;
            subclaims.push(StageGateSubclaimV1 {
                subclaim_id,
                disposition,
                predicate_or_reason,
                evidence_status,
            });
        }
        if cur != bytes.len() {
            return Err(ProfileD1Error::InvalidWire(
                "trailing bytes in CandidateProfileManifestV1",
            ));
        }
        let manifest = Self {
            manifest_version,
            lineage_kind,
            predecessor_profile_id,
            genesis_id,
            genesis_scope_digest,
            current_scope_digest,
            domain_registry_digest,
            protocol_bounds_digest,
            crypto_suite_digest,
            resource_policy_digest,
            activated_predicates,
            activated_capabilities,
            domain_slots,
            subclaims,
        };
        manifest.validate()?;
        Ok(manifest)
    }

    /// Computes the canonical `Policy` `ContentTag` whose 32-byte digest IS the derived `profile_id`.
    pub fn tag(&self) -> Result<ContentTag, ProfileD1Error> {
        let bytes = self.encode_canonical()?;
        Ok(hash_canonical(
            TagKind::Policy,
            PROFILE_D1_CANDIDATE_MANIFEST_SCHEMA,
            bytes.len() as u64,
            |w| w(&bytes),
        )
        .expect("static schema and bounded length"))
    }
}

/// Bound candidate profile manifest pairing `(CandidateProfileManifestV1, derived profile_id)`
/// (`bitblock/profile-d1/bound-profile-manifest/v1`).
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct BoundProfileManifestV1 {
    /// Complete pre-ID candidate manifest (excludes `profile_id`).
    pub candidate: CandidateProfileManifestV1,
    /// Derived domain-separated `profile_id` (`candidate.tag()?.digest()`).
    pub profile_id: [u8; 32],
}

impl BoundProfileManifestV1 {
    /// Seals a validated [`CandidateProfileManifestV1`] with its derived `profile_id`.
    pub fn seal(candidate: CandidateProfileManifestV1) -> Result<Self, ProfileD1Error> {
        let profile_id = candidate.tag()?.digest();
        let bound = Self {
            candidate,
            profile_id,
        };
        bound.verify()?;
        Ok(bound)
    }

    /// Verifies non-circularity and exact `profile_id` reconstruction fail-closed.
    pub fn verify(&self) -> Result<(), ProfileD1Error> {
        self.candidate.validate()?;
        if self.candidate.pre_id_digests().contains(&self.profile_id) {
            return Err(ProfileD1Error::SelfReferenceDetected);
        }
        let expected = self.candidate.tag()?.digest();
        if self.profile_id != expected {
            return Err(ProfileD1Error::ProfileIdMismatch);
        }
        Ok(())
    }

    /// Encodes the bound profile manifest into canonical big-endian wire bytes.
    pub fn encode_canonical(&self) -> Result<Vec<u8>, ProfileD1Error> {
        self.verify()?;
        let cbytes = self.candidate.encode_canonical()?;
        let mut out = Vec::with_capacity(4 + cbytes.len() + 32);
        out.extend_from_slice(&(cbytes.len() as u32).to_be_bytes());
        out.extend_from_slice(&cbytes);
        out.extend_from_slice(&self.profile_id);
        Ok(out)
    }

    /// Decodes and verifies a canonical [`BoundProfileManifestV1`].
    pub fn decode_canonical(bytes: &[u8]) -> Result<Self, ProfileD1Error> {
        if bytes.len() < 36 {
            return Err(ProfileD1Error::InvalidWire(
                "truncated BoundProfileManifestV1",
            ));
        }
        let clen = u32::from_be_bytes(bytes[0..4].try_into().expect("4")) as usize;
        if bytes.len() != 4 + clen + 32 {
            return Err(ProfileD1Error::InvalidWire(
                "BoundProfileManifestV1 len mismatch",
            ));
        }
        let candidate = CandidateProfileManifestV1::decode_canonical(&bytes[4..4 + clen])?;
        let profile_id: [u8; 32] = bytes[4 + clen..4 + clen + 32].try_into().expect("32");
        let bound = Self {
            candidate,
            profile_id,
        };
        bound.verify()?;
        Ok(bound)
    }

    /// Computes the canonical `Receipt` `ContentTag` for this bound profile manifest.
    pub fn tag(&self) -> Result<ContentTag, ProfileD1Error> {
        let bytes = self.encode_canonical()?;
        Ok(hash_canonical(
            TagKind::Receipt,
            PROFILE_D1_BOUND_PROFILE_SCHEMA,
            bytes.len() as u64,
            |w| w(&bytes),
        )
        .expect("static schema and bounded length"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn profile_d1_launch_and_successor_lineage_roundtrip() {
        let launch_cand = CandidateProfileManifestV1::canonical_launch_v1(
            [0x11; 32], [0x22; 32], [0x33; 32], [0x44; 32], [0x55; 32], [0x66; 32],
        )
        .unwrap();
        let launch_bound = BoundProfileManifestV1::seal(launch_cand).unwrap();
        let raw_l = launch_bound.encode_canonical().unwrap();
        assert_eq!(
            BoundProfileManifestV1::decode_canonical(&raw_l).unwrap(),
            launch_bound
        );

        let succ_cand =
            CandidateProfileManifestV1::canonical_successor_program_v1(&launch_bound, [0x77; 32])
                .unwrap();
        let succ_bound = BoundProfileManifestV1::seal(succ_cand).unwrap();
        let raw_s = succ_bound.encode_canonical().unwrap();
        assert_eq!(
            BoundProfileManifestV1::decode_canonical(&raw_s).unwrap(),
            succ_bound
        );
        assert_ne!(launch_bound.profile_id, succ_bound.profile_id);
        assert_eq!(
            succ_bound.candidate.genesis_id,
            launch_bound.candidate.genesis_id
        );
    }
}
