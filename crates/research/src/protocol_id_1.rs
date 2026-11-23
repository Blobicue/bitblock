//! `PROTOCOL-ID-1`: exact 9-field signature-free `CanonicalAcceptedProposalCoreV1`, credential-
//! normalizing `AcceptedProposalWrapperV1`, atomic same-wrapper availability dependencies, exact
//! two-row `AcceptedParentCollisionDispositionV1` safe-halt audit, and 12-node acyclic protocol
//! identifier graph (`PS-027`).
//!
//! Realizes every normative invariant from `docs/CAPABILITY_TREE.md` §`PROTOCOL-ID-1` without
//! modifying `profile_bootstrap.rs` or any existing canonical byte:
//! - **Exact 9-field core (`AcceptedProposalCoreFieldManifestV1`):** encodes version,
//!   `accepted_parent_lookup_id`, `authority_ref_kind`, exact 32/64-bit length frames, and raw
//!   authority-ref / proposal-header / availability-payload byte slices while strictly excluding
//!   all 8 forbidden credential/evidence fields of `AcceptedProposalCoreExcludedFieldManifestV1`.
//! - **Credential normalization without weight multiplication:** two valid proposal wrappers for
//!   identical unsigned core bytes deduplicate proposer credentials and union evidence without
//!   multiplying voting weight or creating duplicate parents, inserting same-wrapper availability
//!   dependencies atomically all-or-nothing.
//! - **Two-row collision disposition (`AcceptedParentCollisionDispositionV1`):**
//!   - Row 1 (`ProtocolIdentityHashCompromiseV1`): byte-distinct lookup payload preimages hashing
//!     to one `ProfileIdHashV1` output -> terminal pre-profile identity safe-halt.
//!   - Row 2 (`ProfileOwnedAuthorityBindingCompromiseV1`): byte-equal lookup payload with distinct
//!     otherwise-valid signature-free cores -> live authority-binding safe-halt (never selects one
//!     core or treats as an ordinary in-place successor).
//!   - Invalid lookup reconstruction rejects as `InvalidLookupReconstruction` (never a third row).
//!
//! Identifiers do not prove trusted distribution, honest execution, consensus, or SHA-256 soundness.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use crate::crypto_d1::{
    CanonicalEd25519PublicKeyV1, CanonicalEd25519SignatureV1, CryptoD1Error, HashSuiteIdV1,
    SignatureSuiteIdV1, SigningDomainTagV1, SigningPreimageV1, verify_preimage_strict,
};
use crate::profile_bootstrap::{BootstrapError, verify_protocol_id_graph_acyclic};
use crate::protocol::{ContentTag, TagKind, hash_canonical};

/// Canonical schema string for the exact 9-field [`CanonicalAcceptedProposalCoreV1`].
pub const PROTOCOL_ID_1_EXACT_CORE_SCHEMA: &[u8] = b"bitblock/protocol-id-1/exact-proposal-core/v1";
/// Canonical schema string for [`AcceptedProposalWrapperV1`].
pub const PROTOCOL_ID_1_WRAPPER_SCHEMA: &[u8] = b"bitblock/protocol-id-1/proposal-wrapper/v1";
/// Canonical schema string for [`TwoRowParentCollisionAuditV1`].
pub const PROTOCOL_ID_1_COLLISION_AUDIT_SCHEMA: &[u8] =
    b"bitblock/protocol-id-1/parent-collision-audit/v1";

/// Domain schema used by frozen `ProfileIdHashV1` (`SHA-256`) to derive `accepted_parent_lookup_id`.
const PARENT_LOOKUP_PREIMAGE_SCHEMA: &[u8] = b"bitblock/protocol-id-1/parent-lookup-preimage/v1";

/// Maximum authority reference payload bytes inside [`CanonicalAcceptedProposalCoreV1`] (`4 KiB`).
pub const MAX_AUTHORITY_REF_BYTES: usize = 4 * 1024;
/// Maximum proposal header payload bytes inside [`CanonicalAcceptedProposalCoreV1`] (`64 KiB`).
pub const MAX_PROPOSAL_HEADER_BYTES: usize = 64 * 1024;
/// Maximum availability payload bytes inside [`CanonicalAcceptedProposalCoreV1`] (`2 MiB`).
pub const MAX_AVAILABILITY_PAYLOAD_BYTES: usize = 2 * 1024 * 1024;
/// Maximum bounded proposer credentials carried in one [`AcceptedProposalWrapperV1`].
pub const MAX_WRAPPER_CREDENTIALS: usize = 64;
/// Maximum bounded evidence digests carried in one [`AcceptedProposalWrapperV1`].
pub const MAX_WRAPPER_EVIDENCE: usize = 64;
/// Maximum bounded same-wrapper availability dependencies carried in one wrapper.
pub const MAX_WRAPPER_AVAILABILITY_DEPS: usize = 64;

const N_GENESIS: &str = "genesis_id";
const N_PROFILE: &str = "profile_id";
const N_CORE: &str = "proposal_core_id";

/// Canonical 12-node, 17-edge non-circular protocol identifier derivation DAG (`PROTOCOL-ID-1`).
pub const PROTOCOL_ID_1_CANONICAL_DAG: &[(&str, &str)] = &[
    ("profile_scope_digest", "domain_registry_digest"),
    (N_GENESIS, "profile_scope_digest"),
    (N_GENESIS, "domain_registry_digest"),
    (N_PROFILE, N_GENESIS),
    (N_PROFILE, "profile_scope_digest"),
    ("tx_body_id", N_PROFILE),
    ("tx_id", "tx_body_id"),
    ("tx_id", N_GENESIS),
    ("tx_id", N_PROFILE),
    (N_CORE, N_GENESIS),
    (N_CORE, N_PROFILE),
    ("block_id", N_CORE),
    ("block_id", "tx_id"),
    ("receipt_id", "block_id"),
    ("effect_id", "receipt_id"),
    ("availability_id", N_CORE),
    ("evidence_id", N_CORE),
];

/// Errors returned by `PROTOCOL-ID-1` core, wrapper, availability, collision, and DAG checks.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ProtocolId1Error {
    /// Underlying RFC 8032 cryptographic check failed.
    Crypto(CryptoD1Error),
    /// Identifier graph cycle check failed.
    Bootstrap(BootstrapError),
    /// Core, wrapper, or audit contract invariant was violated.
    InvalidContract(&'static str),
    /// Canonical wire framing was truncated, overlong, or had trailing bytes.
    InvalidWire(&'static str),
    /// Candidate preimage failed to reconstruct its claimed `accepted_parent_lookup_id`
    /// (explicitly invalid input, never a third collision row).
    InvalidLookupReconstruction,
    /// Two wrappers carried distinct unsigned cores; caller must invoke two-row collision audit.
    DistinctCoreCollisionRequiresAudit,
    /// Two inputs shared both lookup preimage and unsigned core (not a collision; normalize instead).
    NotACollisionUseWrapperNormalization,
    /// Atomic availability-dependency insertion failed and rolled back all entries.
    AvailabilityAtomicityViolation(&'static str),
}

impl From<CryptoD1Error> for ProtocolId1Error {
    fn from(value: CryptoD1Error) -> Self {
        Self::Crypto(value)
    }
}

impl From<BootstrapError> for ProtocolId1Error {
    fn from(value: BootstrapError) -> Self {
        Self::Bootstrap(value)
    }
}

impl fmt::Display for ProtocolId1Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}

impl std::error::Error for ProtocolId1Error {}

/// Frozen `ProfileIdHashV1` (`SHA-256`) helper deriving `accepted_parent_lookup_id` from a
/// canonical parent lookup preimage byte slice.
pub fn profile_id_hash_parent_lookup(preimage: &[u8]) -> [u8; 32] {
    hash_canonical(
        TagKind::Receipt,
        PARENT_LOOKUP_PREIMAGE_SCHEMA,
        preimage.len() as u64,
        |w| w(preimage),
    )
    .expect("static schema and bounded length")
    .digest()
}

/// Exact 9-field signature-free `CanonicalAcceptedProposalCoreV1` matching
/// `AcceptedProposalCoreFieldManifestV1` in `docs/CAPABILITY_TREE.md`.
///
/// Strictly excludes all 8 forbidden signature, credential, consensus-evidence, and
/// availability-assessment fields (`AcceptedProposalCoreExcludedFieldManifestV1`).
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct CanonicalAcceptedProposalCoreV1 {
    /// Field 1: `accepted_proposal_core_version_u16_be` (`1`).
    pub accepted_proposal_core_version: u16,
    /// Field 2: `accepted_parent_lookup_id` (`32` non-zero bytes).
    pub accepted_parent_lookup_id: [u8; 32],
    /// Field 3: `authority_ref_kind_u8` (`1 = LaunchGenesisRef`, `2 = AdmittedProfileRef`).
    pub authority_ref_kind: u8,
    /// Field 7 (length framed by Field 4): `canonical_authority_ref_bytes`.
    pub canonical_authority_ref_bytes: Vec<u8>,
    /// Field 8 (length framed by Field 5): `canonical_proposal_header_bytes`.
    pub canonical_proposal_header_bytes: Vec<u8>,
    /// Field 9 (length framed by Field 6): `canonical_availability_payload_bytes`.
    pub canonical_availability_payload_bytes: Vec<u8>,
}

impl CanonicalAcceptedProposalCoreV1 {
    /// Validates version, non-zero parent lookup ID, authority ref kind, and bounded non-empty slices.
    pub fn validate(&self) -> Result<(), ProtocolId1Error> {
        if self.accepted_proposal_core_version != 1 {
            return Err(ProtocolId1Error::InvalidContract(
                "accepted_proposal_core_version must be 1",
            ));
        }
        if self.accepted_parent_lookup_id == [0u8; 32] {
            return Err(ProtocolId1Error::InvalidContract(
                "accepted_parent_lookup_id must be non-zero",
            ));
        }
        if self.authority_ref_kind != 1 && self.authority_ref_kind != 2 {
            return Err(ProtocolId1Error::InvalidContract(
                "authority_ref_kind must be 1 (LaunchGenesis) or 2 (AdmittedProfile)",
            ));
        }
        if self.canonical_authority_ref_bytes.is_empty()
            || self.canonical_authority_ref_bytes.len() > MAX_AUTHORITY_REF_BYTES
        {
            return Err(ProtocolId1Error::InvalidContract(
                "canonical_authority_ref_bytes must be 1..=MAX_AUTHORITY_REF_BYTES",
            ));
        }
        if self.canonical_proposal_header_bytes.is_empty()
            || self.canonical_proposal_header_bytes.len() > MAX_PROPOSAL_HEADER_BYTES
        {
            return Err(ProtocolId1Error::InvalidContract(
                "canonical_proposal_header_bytes must be 1..=MAX_PROPOSAL_HEADER_BYTES",
            ));
        }
        if self.canonical_availability_payload_bytes.is_empty()
            || self.canonical_availability_payload_bytes.len() > MAX_AVAILABILITY_PAYLOAD_BYTES
        {
            return Err(ProtocolId1Error::InvalidContract(
                "canonical_availability_payload_bytes must be 1..=MAX_AVAILABILITY_PAYLOAD_BYTES",
            ));
        }
        Ok(())
    }

    /// Encodes all 9 fields in exact `AcceptedProposalCoreFieldManifestV1` order (fields 1..=9).
    pub fn encode_canonical(&self) -> Result<Vec<u8>, ProtocolId1Error> {
        self.validate()?;
        let cap = 2
            + 32
            + 1
            + 4
            + 8
            + 8
            + self.canonical_authority_ref_bytes.len()
            + self.canonical_proposal_header_bytes.len()
            + self.canonical_availability_payload_bytes.len();
        let mut out = Vec::with_capacity(cap);
        out.extend_from_slice(&self.accepted_proposal_core_version.to_be_bytes());
        out.extend_from_slice(&self.accepted_parent_lookup_id);
        out.push(self.authority_ref_kind);
        out.extend_from_slice(&(self.canonical_authority_ref_bytes.len() as u32).to_be_bytes());
        out.extend_from_slice(&(self.canonical_proposal_header_bytes.len() as u64).to_be_bytes());
        out.extend_from_slice(
            &(self.canonical_availability_payload_bytes.len() as u64).to_be_bytes(),
        );
        out.extend_from_slice(&self.canonical_authority_ref_bytes);
        out.extend_from_slice(&self.canonical_proposal_header_bytes);
        out.extend_from_slice(&self.canonical_availability_payload_bytes);
        Ok(out)
    }

    /// Decodes and validates a canonical 9-field [`CanonicalAcceptedProposalCoreV1`].
    pub fn decode_canonical(bytes: &[u8]) -> Result<Self, ProtocolId1Error> {
        if bytes.len() < 55 {
            return Err(ProtocolId1Error::InvalidWire(
                "truncated CanonicalAcceptedProposalCoreV1 header",
            ));
        }
        let accepted_proposal_core_version = u16::from_be_bytes([bytes[0], bytes[1]]);
        let accepted_parent_lookup_id: [u8; 32] = bytes[2..34].try_into().expect("32");
        let authority_ref_kind = bytes[34];
        let auth_len = u32::from_be_bytes(bytes[35..39].try_into().expect("4")) as usize;
        let hdr_len = u64::from_be_bytes(bytes[39..47].try_into().expect("8"));
        let avail_len = u64::from_be_bytes(bytes[47..55].try_into().expect("8"));
        if auth_len > MAX_AUTHORITY_REF_BYTES
            || hdr_len > MAX_PROPOSAL_HEADER_BYTES as u64
            || avail_len > MAX_AVAILABILITY_PAYLOAD_BYTES as u64
        {
            return Err(ProtocolId1Error::InvalidWire(
                "proposal core field length exceeds cap",
            ));
        }
        let h_len = hdr_len as usize;
        let a_len = avail_len as usize;
        let expected_total = 55usize
            .checked_add(auth_len)
            .and_then(|v| v.checked_add(h_len))
            .and_then(|v| v.checked_add(a_len))
            .ok_or(ProtocolId1Error::InvalidWire("proposal core size overflow"))?;
        if bytes.len() != expected_total {
            return Err(ProtocolId1Error::InvalidWire(
                "proposal core length mismatch or trailing bytes",
            ));
        }
        let off1 = 55 + auth_len;
        let off2 = off1 + h_len;
        let core = Self {
            accepted_proposal_core_version,
            accepted_parent_lookup_id,
            authority_ref_kind,
            canonical_authority_ref_bytes: bytes[55..off1].to_vec(),
            canonical_proposal_header_bytes: bytes[off1..off2].to_vec(),
            canonical_availability_payload_bytes: bytes[off2..].to_vec(),
        };
        core.validate()?;
        Ok(core)
    }

    /// Computes the canonical `Receipt` `ContentTag` (`proposal_core_id`) of this unsigned core.
    pub fn tag(&self) -> Result<ContentTag, ProtocolId1Error> {
        let bytes = self.encode_canonical()?;
        Ok(hash_canonical(
            TagKind::Receipt,
            PROTOCOL_ID_1_EXACT_CORE_SCHEMA,
            bytes.len() as u64,
            |w| w(&bytes),
        )
        .expect("static schema and bounded length"))
    }
}

/// One typed validator proposer credential bound outside the signature-free proposal core.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ProposalCredentialEntryV1 {
    /// Strict RFC 8032 Ed25519 validator proposal public key (`ValidatorProposal`).
    pub proposer_pubkey: CanonicalEd25519PublicKeyV1,
    /// Strict RFC 8032 Ed25519 signature over `SigningPreimageV1(BlockProposal, core_digest)`.
    pub signature: CanonicalEd25519SignatureV1,
}

/// Consensus-entailed same-wrapper availability dependency inserted atomically with the wrapper.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SameWrapperAvailabilityDependencyV1 {
    /// Non-zero digest identifying the required availability blob/segment.
    pub dependency_id: [u8; 32],
    /// Declared canonical byte length (`> 0`).
    pub expected_bytes: u64,
}

/// Bounded proposal wrapper separating one signature-free [`CanonicalAcceptedProposalCoreV1`]
/// from its typed credentials, evidence digests, and same-wrapper availability dependencies.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct AcceptedProposalWrapperV1 {
    /// Signature-free unsigned proposal core.
    pub core: CanonicalAcceptedProposalCoreV1,
    /// Strictly sorted unique proposer credentials (by `proposer_pubkey`).
    pub credentials: Vec<ProposalCredentialEntryV1>,
    /// Strictly sorted unique consensus-evidence digests (`AcceptedEvidenceSetV1`).
    pub evidence_digests: Vec<[u8; 32]>,
    /// Strictly sorted unique same-wrapper availability dependencies (by `dependency_id`).
    pub availability_dependencies: Vec<SameWrapperAvailabilityDependencyV1>,
}

impl AcceptedProposalWrapperV1 {
    /// Validates structural bounds, strict sorting/uniqueness, and non-zero digests.
    pub fn validate_structure(&self) -> Result<(), ProtocolId1Error> {
        self.core.validate()?;
        if self.credentials.is_empty() || self.credentials.len() > MAX_WRAPPER_CREDENTIALS {
            return Err(ProtocolId1Error::InvalidContract(
                "wrapper credentials must lie in 1..=MAX_WRAPPER_CREDENTIALS",
            ));
        }
        for pair in self.credentials.windows(2) {
            if pair[0].proposer_pubkey >= pair[1].proposer_pubkey {
                return Err(ProtocolId1Error::InvalidContract(
                    "wrapper credentials must be strictly sorted by unique proposer_pubkey",
                ));
            }
        }
        if self.evidence_digests.len() > MAX_WRAPPER_EVIDENCE {
            return Err(ProtocolId1Error::InvalidContract(
                "evidence_digests exceeds MAX_WRAPPER_EVIDENCE",
            ));
        }
        for (i, d) in self.evidence_digests.iter().enumerate() {
            if *d == [0u8; 32] {
                return Err(ProtocolId1Error::InvalidContract(
                    "evidence digest must be non-zero",
                ));
            }
            if i > 0 && self.evidence_digests[i - 1] >= *d {
                return Err(ProtocolId1Error::InvalidContract(
                    "evidence_digests must be strictly sorted and unique",
                ));
            }
        }
        if self.availability_dependencies.len() > MAX_WRAPPER_AVAILABILITY_DEPS {
            return Err(ProtocolId1Error::InvalidContract(
                "availability_dependencies exceeds MAX_WRAPPER_AVAILABILITY_DEPS",
            ));
        }
        for (i, dep) in self.availability_dependencies.iter().enumerate() {
            if dep.dependency_id == [0u8; 32] || dep.expected_bytes == 0 {
                return Err(ProtocolId1Error::InvalidContract(
                    "availability dependency requires non-zero digest and positive bytes",
                ));
            }
            if i > 0 && self.availability_dependencies[i - 1].dependency_id >= dep.dependency_id {
                return Err(ProtocolId1Error::InvalidContract(
                    "availability_dependencies must be strictly sorted by unique dependency_id",
                ));
            }
        }
        Ok(())
    }

    /// Verifies every proposer credential signature strictly against `(chain_id, genesis_id,
    /// launch_manifest_id, profile_id)` and `SigningDomainTagV1::BlockProposal`.
    pub fn verify_credentials(
        &self,
        chain_id: u64,
        genesis_id: [u8; 32],
        launch_manifest_id: [u8; 32],
        profile_id: [u8; 32],
    ) -> Result<[u8; 32], ProtocolId1Error> {
        self.validate_structure()?;
        let core_digest = self.core.tag()?.digest();
        let preimage = SigningPreimageV1 {
            protocol_version: 1,
            chain_id,
            genesis_id,
            launch_manifest_id,
            profile_id,
            domain_tag: SigningDomainTagV1::BlockProposal,
            signature_suite: SignatureSuiteIdV1::Ed25519StrictRfc8032V1,
            hash_suite: HashSuiteIdV1::Sha256V1,
            payload_digest: core_digest,
        };
        for cred in &self.credentials {
            verify_preimage_strict(&cred.proposer_pubkey, &preimage, &cred.signature)?;
        }
        Ok(core_digest)
    }

    /// Encodes the proposal wrapper into canonical big-endian wire bytes.
    pub fn encode_canonical(&self) -> Result<Vec<u8>, ProtocolId1Error> {
        self.validate_structure()?;
        let core_bytes = self.core.encode_canonical()?;
        let mut out = Vec::with_capacity(
            4 + core_bytes.len()
                + 2
                + self.credentials.len() * 96
                + 2
                + self.evidence_digests.len() * 32
                + 2
                + self.availability_dependencies.len() * 40,
        );
        out.extend_from_slice(&(core_bytes.len() as u32).to_be_bytes());
        out.extend_from_slice(&core_bytes);
        out.extend_from_slice(&(self.credentials.len() as u16).to_be_bytes());
        for c in &self.credentials {
            out.extend_from_slice(c.proposer_pubkey.as_bytes());
            out.extend_from_slice(c.signature.as_bytes());
        }
        out.extend_from_slice(&(self.evidence_digests.len() as u16).to_be_bytes());
        for e in &self.evidence_digests {
            out.extend_from_slice(e);
        }
        out.extend_from_slice(&(self.availability_dependencies.len() as u16).to_be_bytes());
        for d in &self.availability_dependencies {
            out.extend_from_slice(&d.dependency_id);
            out.extend_from_slice(&d.expected_bytes.to_be_bytes());
        }
        Ok(out)
    }

    /// Decodes and structurally validates a canonical [`AcceptedProposalWrapperV1`].
    pub fn decode_canonical(bytes: &[u8]) -> Result<Self, ProtocolId1Error> {
        let mut cur = 0usize;
        let take = |n: usize, c: &mut usize| -> Result<&[u8], ProtocolId1Error> {
            if bytes.len().saturating_sub(*c) < n {
                return Err(ProtocolId1Error::InvalidWire(
                    "truncated AcceptedProposalWrapperV1",
                ));
            }
            let sl = &bytes[*c..*c + n];
            *c += n;
            Ok(sl)
        };
        let core_len = u32::from_be_bytes(take(4, &mut cur)?.try_into().expect("4")) as usize;
        let core = CanonicalAcceptedProposalCoreV1::decode_canonical(take(core_len, &mut cur)?)?;

        let cred_count = u16::from_be_bytes(take(2, &mut cur)?.try_into().expect("2")) as usize;
        if cred_count > MAX_WRAPPER_CREDENTIALS {
            return Err(ProtocolId1Error::InvalidWire("excessive credential count"));
        }
        let mut credentials = Vec::with_capacity(cred_count);
        for _ in 0..cred_count {
            let pk_raw: [u8; 32] = take(32, &mut cur)?.try_into().expect("32");
            let sig_raw: [u8; 64] = take(64, &mut cur)?.try_into().expect("64");
            credentials.push(ProposalCredentialEntryV1 {
                proposer_pubkey: CanonicalEd25519PublicKeyV1::from_bytes(pk_raw)?,
                signature: CanonicalEd25519SignatureV1::from_bytes(sig_raw)?,
            });
        }

        let ev_count = u16::from_be_bytes(take(2, &mut cur)?.try_into().expect("2")) as usize;
        if ev_count > MAX_WRAPPER_EVIDENCE {
            return Err(ProtocolId1Error::InvalidWire("excessive evidence count"));
        }
        let mut evidence_digests = Vec::with_capacity(ev_count);
        for _ in 0..ev_count {
            evidence_digests.push(take(32, &mut cur)?.try_into().expect("32"));
        }

        let dep_count = u16::from_be_bytes(take(2, &mut cur)?.try_into().expect("2")) as usize;
        if dep_count > MAX_WRAPPER_AVAILABILITY_DEPS {
            return Err(ProtocolId1Error::InvalidWire(
                "excessive availability dep count",
            ));
        }
        let mut availability_dependencies = Vec::with_capacity(dep_count);
        for _ in 0..dep_count {
            let dependency_id: [u8; 32] = take(32, &mut cur)?.try_into().expect("32");
            let expected_bytes = u64::from_be_bytes(take(8, &mut cur)?.try_into().expect("8"));
            availability_dependencies.push(SameWrapperAvailabilityDependencyV1 {
                dependency_id,
                expected_bytes,
            });
        }
        if cur != bytes.len() {
            return Err(ProtocolId1Error::InvalidWire(
                "trailing AcceptedProposalWrapperV1 bytes",
            ));
        }
        let w = Self {
            core,
            credentials,
            evidence_digests,
            availability_dependencies,
        };
        w.validate_structure()?;
        Ok(w)
    }

    /// Computes the canonical `Receipt` `ContentTag` for this proposal wrapper.
    pub fn tag(&self) -> Result<ContentTag, ProtocolId1Error> {
        let bytes = self.encode_canonical()?;
        Ok(hash_canonical(
            TagKind::Receipt,
            PROTOCOL_ID_1_WRAPPER_SCHEMA,
            bytes.len() as u64,
            |w| w(&bytes),
        )
        .expect("static schema and bounded length"))
    }
}

/// Normalizes two valid wrappers carrying identical unsigned core bytes by deduplicating
/// credentials (zero weight multiplication) and unioning evidence without multiplying parents.
pub fn normalize_proposal_wrappers(
    existing: &AcceptedProposalWrapperV1,
    incoming: &AcceptedProposalWrapperV1,
    chain_id: u64,
    genesis_id: [u8; 32],
    launch_manifest_id: [u8; 32],
    profile_id: [u8; 32],
) -> Result<AcceptedProposalWrapperV1, ProtocolId1Error> {
    existing.verify_credentials(chain_id, genesis_id, launch_manifest_id, profile_id)?;
    incoming.verify_credentials(chain_id, genesis_id, launch_manifest_id, profile_id)?;
    if existing.core.encode_canonical()? != incoming.core.encode_canonical()? {
        return Err(ProtocolId1Error::DistinctCoreCollisionRequiresAudit);
    }
    let mut cred_map: BTreeMap<CanonicalEd25519PublicKeyV1, CanonicalEd25519SignatureV1> =
        BTreeMap::new();
    for c in existing.credentials.iter().chain(&incoming.credentials) {
        cred_map.entry(c.proposer_pubkey).or_insert(c.signature);
    }
    if cred_map.len() > MAX_WRAPPER_CREDENTIALS {
        return Err(ProtocolId1Error::InvalidContract(
            "normalized credential set exceeds cap",
        ));
    }
    let mut ev_set = BTreeSet::new();
    for &e in existing
        .evidence_digests
        .iter()
        .chain(&incoming.evidence_digests)
    {
        ev_set.insert(e);
    }
    if ev_set.len() > MAX_WRAPPER_EVIDENCE {
        return Err(ProtocolId1Error::InvalidContract(
            "normalized evidence set exceeds cap",
        ));
    }
    let mut dep_map: BTreeMap<[u8; 32], u64> = BTreeMap::new();
    for d in existing
        .availability_dependencies
        .iter()
        .chain(&incoming.availability_dependencies)
    {
        if let Some(&prev) = dep_map.get(&d.dependency_id)
            && prev != d.expected_bytes
        {
            return Err(ProtocolId1Error::AvailabilityAtomicityViolation(
                "conflicting expected_bytes for identical availability dependency",
            ));
        }
        dep_map.insert(d.dependency_id, d.expected_bytes);
    }
    if dep_map.len() > MAX_WRAPPER_AVAILABILITY_DEPS {
        return Err(ProtocolId1Error::InvalidContract(
            "normalized availability dependency set exceeds cap",
        ));
    }
    let merged = AcceptedProposalWrapperV1 {
        core: existing.core.clone(),
        credentials: cred_map
            .into_iter()
            .map(|(proposer_pubkey, signature)| ProposalCredentialEntryV1 {
                proposer_pubkey,
                signature,
            })
            .collect(),
        evidence_digests: ev_set.into_iter().collect(),
        availability_dependencies: dep_map
            .into_iter()
            .map(
                |(dependency_id, expected_bytes)| SameWrapperAvailabilityDependencyV1 {
                    dependency_id,
                    expected_bytes,
                },
            )
            .collect(),
    };
    merged.validate_structure()?;
    Ok(merged)
}

/// Atomic same-wrapper consensus-entailed availability index (`PROTOCOL-ID-1`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AtomicAvailabilityIndexV1 {
    /// Committed `(proposal_core_id -> ordered availability dependencies)`.
    pub by_core: BTreeMap<[u8; 32], Vec<SameWrapperAvailabilityDependencyV1>>,
    /// Committed `(dependency_id -> expected_bytes)` global consistency map.
    pub global_sizes: BTreeMap<[u8; 32], u64>,
}

impl AtomicAvailabilityIndexV1 {
    /// Atomically inserts every availability dependency carried by `wrapper`, rolling back all
    /// changes if any entry conflicts with an already-committed size.
    pub fn admit_wrapper(
        &mut self,
        wrapper: &AcceptedProposalWrapperV1,
    ) -> Result<[u8; 32], ProtocolId1Error> {
        wrapper.validate_structure()?;
        let core_id = wrapper.core.tag()?.digest();
        let mut staged_sizes = self.global_sizes.clone();
        for dep in &wrapper.availability_dependencies {
            if let Some(&prev) = staged_sizes.get(&dep.dependency_id)
                && prev != dep.expected_bytes
            {
                return Err(ProtocolId1Error::AvailabilityAtomicityViolation(
                    "dependency byte-length conflict; entire wrapper rolled back",
                ));
            }
            staged_sizes.insert(dep.dependency_id, dep.expected_bytes);
        }
        self.global_sizes = staged_sizes;
        self.by_core
            .insert(core_id, wrapper.availability_dependencies.clone());
        Ok(core_id)
    }
}

/// Exact two-row `AcceptedParentCollisionDispositionV1` safe-halt classification.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum ParentCollisionRowV1 {
    /// Row 1: byte-distinct lookup payload preimages under one `ProfileIdHashV1` output ->
    /// `ProtocolIdentityHashCompromiseV1` (terminal pre-profile identity safe-halt).
    ProtocolIdentityHashCompromise = 1,
    /// Row 2: byte-equal lookup payload with distinct valid unsigned cores ->
    /// `ProfileOwnedAuthorityBindingCompromiseV1` (live authority-binding safe-halt).
    ProfileOwnedAuthorityBindingCompromise = 2,
}

impl ParentCollisionRowV1 {
    /// Returns the canonical 1-byte row code (`1` or `2`).
    pub const fn code(self) -> u8 {
        self as u8
    }

    /// Decodes a canonical 1-byte collision row code (`1` or `2`).
    pub const fn from_code(code: u8) -> Result<Self, ProtocolId1Error> {
        match code {
            1 => Ok(Self::ProtocolIdentityHashCompromise),
            2 => Ok(Self::ProfileOwnedAuthorityBindingCompromise),
            _ => Err(ProtocolId1Error::InvalidWire(
                "unknown ParentCollisionRowV1 code (only rows 1 and 2 exist)",
            )),
        }
    }
}

/// Canonical two-row safe-halt audit receipt (`bitblock/protocol-id-1/parent-collision-audit/v1`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct TwoRowParentCollisionAuditV1 {
    /// Schema version (`1`).
    pub schema_version: u16,
    /// Exact collision row (`1 = ProtocolIdentityHashCompromise`, `2 = ProfileOwnedAuthorityBindingCompromise`).
    pub collision_row: ParentCollisionRowV1,
    /// Shared claimed `accepted_parent_lookup_id`.
    pub claimed_parent_lookup_id: [u8; 32],
    /// Digest of first reconstructed signature-free core.
    pub core_a_digest: [u8; 32],
    /// Digest of second reconstructed signature-free core.
    pub core_b_digest: [u8; 32],
    /// Must always be `true`: both collision rows safe-halt and never select one core.
    pub safe_halt_required: bool,
}

impl TwoRowParentCollisionAuditV1 {
    /// Fixed canonical big-endian wire length (`100` bytes).
    pub const WIRE_LEN: usize = 2 + 1 + 32 + 32 + 32 + 1;

    /// Classifies two candidate `(lookup_preimage, unsigned_core)` pairs strictly under
    /// `AcceptedParentCollisionDispositionV1` using `lookup_hash_fn` (default:
    /// [`profile_id_hash_parent_lookup`]).
    ///
    /// Rejects any candidate that fails lookup reconstruction as
    /// [`ProtocolId1Error::InvalidLookupReconstruction`] (never creating a third collision row).
    pub fn classify_with_hasher<F>(
        claimed_parent_lookup_id: [u8; 32],
        preimage_a: &[u8],
        core_a: &CanonicalAcceptedProposalCoreV1,
        preimage_b: &[u8],
        core_b: &CanonicalAcceptedProposalCoreV1,
        lookup_hash_fn: F,
    ) -> Result<Self, ProtocolId1Error>
    where
        F: Fn(&[u8]) -> [u8; 32],
    {
        if claimed_parent_lookup_id == [0u8; 32] {
            return Err(ProtocolId1Error::InvalidLookupReconstruction);
        }
        core_a.validate()?;
        core_b.validate()?;
        if core_a.accepted_parent_lookup_id != claimed_parent_lookup_id
            || core_b.accepted_parent_lookup_id != claimed_parent_lookup_id
            || lookup_hash_fn(preimage_a) != claimed_parent_lookup_id
            || lookup_hash_fn(preimage_b) != claimed_parent_lookup_id
        {
            return Err(ProtocolId1Error::InvalidLookupReconstruction);
        }
        let bytes_a = core_a.encode_canonical()?;
        let bytes_b = core_b.encode_canonical()?;
        let row = if preimage_a != preimage_b {
            ParentCollisionRowV1::ProtocolIdentityHashCompromise
        } else if bytes_a != bytes_b {
            ParentCollisionRowV1::ProfileOwnedAuthorityBindingCompromise
        } else {
            return Err(ProtocolId1Error::NotACollisionUseWrapperNormalization);
        };
        Ok(Self {
            schema_version: 1,
            collision_row: row,
            claimed_parent_lookup_id,
            core_a_digest: core_a.tag()?.digest(),
            core_b_digest: core_b.tag()?.digest(),
            safe_halt_required: true,
        })
    }

    /// Encodes the 100-byte canonical collision audit receipt.
    pub fn encode_canonical(&self) -> Result<[u8; Self::WIRE_LEN], ProtocolId1Error> {
        if self.schema_version != 1 || !self.safe_halt_required {
            return Err(ProtocolId1Error::InvalidContract(
                "schema_version must be 1 and safe_halt_required must be true",
            ));
        }
        let mut out = [0u8; Self::WIRE_LEN];
        out[0..2].copy_from_slice(&self.schema_version.to_be_bytes());
        out[2] = self.collision_row.code();
        out[3..35].copy_from_slice(&self.claimed_parent_lookup_id);
        out[35..67].copy_from_slice(&self.core_a_digest);
        out[67..99].copy_from_slice(&self.core_b_digest);
        out[99] = u8::from(self.safe_halt_required);
        Ok(out)
    }

    /// Decodes and validates a 100-byte canonical [`TwoRowParentCollisionAuditV1`].
    pub fn decode_canonical(bytes: &[u8]) -> Result<Self, ProtocolId1Error> {
        if bytes.len() != Self::WIRE_LEN {
            return Err(ProtocolId1Error::InvalidWire(
                "wrong TwoRowParentCollisionAuditV1 wire length",
            ));
        }
        let safe_halt_required = match bytes[99] {
            1 => true,
            _ => {
                return Err(ProtocolId1Error::InvalidContract(
                    "safe_halt_required must be true (1)",
                ));
            }
        };
        let schema_version = u16::from_be_bytes([bytes[0], bytes[1]]);
        if schema_version != 1 {
            return Err(ProtocolId1Error::InvalidContract(
                "schema_version must be 1",
            ));
        }
        Ok(Self {
            schema_version,
            collision_row: ParentCollisionRowV1::from_code(bytes[2])?,
            claimed_parent_lookup_id: bytes[3..35].try_into().expect("32"),
            core_a_digest: bytes[35..67].try_into().expect("32"),
            core_b_digest: bytes[67..99].try_into().expect("32"),
            safe_halt_required,
        })
    }

    /// Computes the canonical `Receipt` `ContentTag` for this two-row collision audit.
    pub fn tag(&self) -> Result<ContentTag, ProtocolId1Error> {
        let bytes = self.encode_canonical()?;
        Ok(hash_canonical(
            TagKind::Receipt,
            PROTOCOL_ID_1_COLLISION_AUDIT_SCHEMA,
            bytes.len() as u64,
            |w| w(&bytes),
        )
        .expect("static schema and fixed length"))
    }
}

/// Verifies the full 12-node canonical `PROTOCOL-ID-1` identifier derivation graph is strictly acyclic.
pub fn verify_canonical_protocol_id_dag() -> Result<(), ProtocolId1Error> {
    verify_protocol_id_graph_acyclic(PROTOCOL_ID_1_CANONICAL_DAG)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crypto_d1::sign_preimage_deterministic;

    #[test]
    fn protocol_id_1_core_wrapper_normalization_and_two_row_audit() {
        verify_canonical_protocol_id_dag().unwrap();
        let preimage = b"parent-lookup-block-height-0";
        let lookup_id = profile_id_hash_parent_lookup(preimage);
        let core = CanonicalAcceptedProposalCoreV1 {
            accepted_proposal_core_version: 1,
            accepted_parent_lookup_id: lookup_id,
            authority_ref_kind: 1,
            canonical_authority_ref_bytes: b"genesis-ref-v1".to_vec(),
            canonical_proposal_header_bytes: b"hdr-height-1-epoch-0".to_vec(),
            canonical_availability_payload_bytes: b"avail-root-bytes".to_vec(),
        };
        let raw_c = core.encode_canonical().unwrap();
        assert_eq!(
            CanonicalAcceptedProposalCoreV1::decode_canonical(&raw_c).unwrap(),
            core
        );

        let core_digest = core.tag().unwrap().digest();
        let sign_doc = SigningPreimageV1 {
            protocol_version: 1,
            chain_id: 1,
            genesis_id: [0x11; 32],
            launch_manifest_id: [0x22; 32],
            profile_id: [0x33; 32],
            domain_tag: SigningDomainTagV1::BlockProposal,
            signature_suite: SignatureSuiteIdV1::Ed25519StrictRfc8032V1,
            hash_suite: HashSuiteIdV1::Sha256V1,
            payload_digest: core_digest,
        };
        let pk1 = CanonicalEd25519PublicKeyV1::from_secret_seed(&[41u8; 32]).unwrap();
        let sig1 = sign_preimage_deterministic(&[41u8; 32], &sign_doc).unwrap();
        let w1 = AcceptedProposalWrapperV1 {
            core: core.clone(),
            credentials: vec![ProposalCredentialEntryV1 {
                proposer_pubkey: pk1,
                signature: sig1,
            }],
            evidence_digests: vec![[0xAA; 32]],
            availability_dependencies: vec![SameWrapperAvailabilityDependencyV1 {
                dependency_id: [0xBB; 32],
                expected_bytes: 4096,
            }],
        };
        let raw_w = w1.encode_canonical().unwrap();
        assert_eq!(
            AcceptedProposalWrapperV1::decode_canonical(&raw_w).unwrap(),
            w1
        );

        let merged =
            normalize_proposal_wrappers(&w1, &w1, 1, [0x11; 32], [0x22; 32], [0x33; 32]).unwrap();
        assert_eq!(merged.credentials.len(), 1);

        let mut core_b = core.clone();
        core_b.canonical_proposal_header_bytes = b"hdr-height-1-equivocation".to_vec();
        let audit_r2 = TwoRowParentCollisionAuditV1::classify_with_hasher(
            lookup_id,
            preimage,
            &core,
            preimage,
            &core_b,
            profile_id_hash_parent_lookup,
        )
        .unwrap();
        assert_eq!(
            audit_r2.collision_row,
            ParentCollisionRowV1::ProfileOwnedAuthorityBindingCompromise
        );
        let raw_a = audit_r2.encode_canonical().unwrap();
        assert_eq!(
            TwoRowParentCollisionAuditV1::decode_canonical(&raw_a).unwrap(),
            audit_r2
        );
    }
}
