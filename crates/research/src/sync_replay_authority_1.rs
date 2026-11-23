//! `SYNC-1@replay-authority` (`PD-06` / `PS-095`): independent historical reconstruction and
//! archival audit of `CONSENSUS-1`'s accepted parent-authority certificates in a dedicated acyclic
//! module (`sync_1 -> consensus_1 -> sync_replay_authority_1`), leaving `sync_1.rs` completely
//! untouched.
//!
//! Realizes every normative carrier and gate in `docs/CAPABILITY_TREE.md` §`SYNC-1` and
//! `docs/LAYER1_DESIGN_PAPER.md` §3925–4260 behind `--features protocol-research`:
//! - **Ordered replay evidence log (`ReplayEvidenceEventRecordV1`):** contiguous ordinals chained
//!   by predecessor event IDs (`AcceptedRecordOuterOccurrence` total-decoded as
//!   `AcceptedProposalWrapperV1`, and `AcceptedParentAuthorityUseOccurrence` total-decoded as
//!   `AcceptedConsensusNativeParentAuthorityCertificateV1`) committed by `ordered_event_prefix_root`.
//! - **Proof-free normalized authority entries (`NormalizedAcceptedAuthorityEntryCoreV1`):**
//!   independently recomputed `normalized_authority_entry_id` and sorted `(lookup_id, entry_id)`
//!   `normalized_authority_view_root` — carried IDs are byte-equal assertions, never selectors.
//! - **Authorized post-consumer checkpoints (`ReplayEvidenceArchiveCheckpointV1`):** signed by a
//!   pre-existing checkpoint authority (`SigningDomainTagV1::ReleaseArtifact`), supporting append-only
//!   `ReplayEvidenceCheckpointConsistencyProofV1` across `C1 -> C2` and passing the mandatory
//!   **post-consumer checkpoint positive control** (`checkpoint_ledger_height > consumer_ledger_height`,
//!   proving archive order is NEVER live consensus authority).
//! - **One replay-wide `ReplayEvidenceViewV1`, same-wrapper occurrence witnesses, full-core
//!   retention projection proofs, consumer-bound `HistoricalAcceptedParentAuthorityUseWitnessV1`
//!   and `ConsensusToReplayParentAuthorityCommitmentProofV1`, plus separate later
//!   `ReplayParentAuthorityStatusProjectionSetV1`.**
//! - **Complete 12/12 negative-control verifier:** rejects omitted pre-cutoff conflict, post-cutoff
//!   insertion, event reorder/gap, event duplication, mixed root/policy/view, circular/substituted
//!   normalized entry, retained-subset masquerade, synthetic cross-wrapper pair, missing
//!   purpose/consumer, non-admitted/late certificate, certificate-to-replay mismatch, and every
//!   partial checkpoint-publication prefix.
//!
//! Non-claim: replay reconstruction audits already-accepted authority from `CONSENSUS-1` and never
//! selects live authority (`selects_live_consensus_authority() == false`).

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use sha2::{Digest, Sha256};

use crate::consensus_1::{
    AcceptedConsensusNativeParentAuthorityCertificateV1, Consensus1Error,
    ConsensusClusterHarnessV1, ConsensusNativeAcceptedRecordOccurrenceReceiptV1,
    ParentAuthorityPurposeV1, ParentAuthorityStatusV1, compute_occurrence_universe_root,
};
use crate::crypto_d1::{
    CanonicalEd25519PublicKeyV1, CanonicalEd25519SignatureV1, HashSuiteIdV1, SignatureSuiteIdV1,
    SigningDomainTagV1, SigningPreimageV1, sign_preimage_deterministic, verify_preimage_strict,
};
use crate::executor_1::ExecutorForkContextV1;
use crate::protocol::{ContentTag, TagKind, hash_canonical};
use crate::protocol_id_1::{AcceptedProposalWrapperV1, ProtocolId1Error};
use crate::validator_schedule_1::EpochValidatorScheduleV1;

/// Canonical schema string for an authorized [`ReplayEvidenceArchiveCheckpointV1`].
pub const SYNC_REPLAY_CHECKPOINT_SCHEMA: &[u8] = b"bitblock/sync-1/replay-archive-checkpoint/v1";
/// Canonical schema string for a complete [`ReplayAuthorityAuditBundleV1`].
pub const SYNC_REPLAY_AUDIT_BUNDLE_SCHEMA: &[u8] = b"bitblock/sync-1/replay-authority-audit/v1";

/// Protocol version encoded in all `SYNC-1@replay-authority` carriers.
pub const SYNC_REPLAY_AUTH_VERSION: u16 = 1;
/// Deterministic Ed25519 seed for the pre-existing archive checkpoint authority in devnet fixtures.
pub const DEFAULT_ARCHIVE_CHECKPOINT_AUTH_SEED: [u8; 32] = [171u8; 32];

const SRA_OUTER_OCC_PREFIX: &[u8] = b"bb.sra1.outer_occ.v1";
const SRA_CERT_OCC_PREFIX: &[u8] = b"bb.sra1.cert_occ.v1";
const SRA_EVENT_REC_PREFIX: &[u8] = b"bb.sra1.event_rec.v1";
const SRA_PREFIX_ROOT_PREFIX: &[u8] = b"bb.sra1.prefix_root.v1";
const SRA_NORM_ENTRY_PREFIX: &[u8] = b"bb.sra1.norm_entry.v1";
const SRA_NORM_VIEW_PREFIX: &[u8] = b"bb.sra1.norm_view.v1";
const SRA_DEP_CLOSURE_PREFIX: &[u8] = b"bb.sra1.dep_closure.v1";
const SRA_CHK_CORE_PREFIX: &[u8] = b"bb.sra1.chk_core.v1";
const SRA_CHK_SIGN_PREFIX: &[u8] = b"bb.sra1.chk_sign.v1";
const SRA_VIEW_CORE_PREFIX: &[u8] = b"bb.sra1.view_core.v1";
const SRA_CONSISTENCY_PREFIX: &[u8] = b"bb.sra1.consistency.v1";
const SRA_COMMIT_MAP_PREFIX: &[u8] = b"bb.sra1.commit_map.v1";
const SRA_RETENTION_TRACE_PREFIX: &[u8] = b"bb.sra1.ret_trace.v1";

/// Fail-closed audit, reconstruction, codec, and 12-control rejection errors for `PD-06`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ReplayAuthority1Error {
    /// Truncated, non-canonical, unsorted, or trailing-byte wire buffer.
    InvalidWire(&'static str),
    /// Negative control 1: pre-cutoff conflict material was omitted from normalized view or coverage.
    OmittedPreCutoffConflict,
    /// Negative control 2: post-cutoff event was illegally inserted into pre-cutoff reconstruction.
    PostCutoffInsertionRejected,
    /// Negative control 3: replay event ordinals were reordered, gapped, or had broken predecessor links.
    EventReorderOrGapRejected(&'static str),
    /// Negative control 4: duplicate event ordinal or duplicate outer occurrence detected.
    EventDuplicationRejected(&'static str),
    /// Negative control 5: mixed archive root, normalization policy, checkpoint ID, or view ID.
    MixedRootPolicyOrViewRejected(&'static str),
    /// Negative control 6: circular or substituted `NormalizedAcceptedAuthorityEntryCoreV1` commitment.
    CircularOrSubstitutedNormalizedEntry(&'static str),
    /// Negative control 7: retained subset attempted to masquerade as full normalized entry core.
    RetainedSubsetMasqueradeRejected,
    /// Negative control 8: synthetic cross-wrapper pair combined fields from two distinct wrappers.
    SyntheticCrossWrapperPairRejected,
    /// Negative control 9: missing or mismatched consumer purpose/object/preimage binding.
    MissingPurposeOrConsumerBinding(&'static str),
    /// Negative control 10: non-admitted, tentative, replay-sourced, or late certificate rejected.
    NonAdmittedOrLateCertificateRejected(&'static str),
    /// Negative control 11: certificate occurrence/evidence commitment disagreed with replay archive.
    CertificateToReplayMismatch(&'static str),
    /// Negative control 12: partial checkpoint publication prefix refused fail-closed.
    PartialCheckpointPublicationRejected(&'static str),
    /// Pre-existing archive checkpoint authorization signature or resolver check failed.
    InvalidCheckpointAuthorization(&'static str),
    /// Append-only checkpoint consistency proof between `C1` and `C2` failed verification.
    AppendConsistencyViolation(&'static str),
    /// Underlying `CONSENSUS-1` verification error.
    Consensus(Consensus1Error),
    /// Underlying `PROTOCOL-ID-1` decode/validation error.
    ProtocolId(ProtocolId1Error),
}

impl From<Consensus1Error> for ReplayAuthority1Error {
    fn from(v: Consensus1Error) -> Self {
        Self::Consensus(v)
    }
}
impl From<ProtocolId1Error> for ReplayAuthority1Error {
    fn from(v: ProtocolId1Error) -> Self {
        Self::ProtocolId(v)
    }
}
impl fmt::Display for ReplayAuthority1Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for ReplayAuthority1Error {}

struct SraSlice<'a> {
    bytes: &'a [u8],
    pos: usize,
}

impl<'a> SraSlice<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, pos: 0 }
    }
    fn grab(&mut self, n: usize) -> Result<&'a [u8], ReplayAuthority1Error> {
        let next = self
            .pos
            .checked_add(n)
            .ok_or(ReplayAuthority1Error::InvalidWire("sra slice overflow"))?;
        if next > self.bytes.len() {
            return Err(ReplayAuthority1Error::InvalidWire("truncated sra payload"));
        }
        let s = &self.bytes[self.pos..next];
        self.pos = next;
        Ok(s)
    }
    fn u8(&mut self) -> Result<u8, ReplayAuthority1Error> {
        Ok(self.grab(1)?[0])
    }
    fn u16(&mut self) -> Result<u16, ReplayAuthority1Error> {
        let s = self.grab(2)?;
        Ok(u16::from_be_bytes([s[0], s[1]]))
    }
    fn u32(&mut self) -> Result<u32, ReplayAuthority1Error> {
        let s = self.grab(4)?;
        Ok(u32::from_be_bytes([s[0], s[1], s[2], s[3]]))
    }
    fn u64(&mut self) -> Result<u64, ReplayAuthority1Error> {
        Ok(u64::from_be_bytes(self.grab(8)?.try_into().expect("8")))
    }
    fn b32(&mut self) -> Result<[u8; 32], ReplayAuthority1Error> {
        Ok(self.grab(32)?.try_into().expect("32"))
    }
    fn b64(&mut self) -> Result<[u8; 64], ReplayAuthority1Error> {
        Ok(self.grab(64)?.try_into().expect("64"))
    }
    fn flag(&mut self) -> Result<bool, ReplayAuthority1Error> {
        match self.u8()? {
            0 => Ok(false),
            1 => Ok(true),
            _ => Err(ReplayAuthority1Error::InvalidWire("non-canonical bool")),
        }
    }
    fn vec_u32(&mut self, cap: usize) -> Result<Vec<u8>, ReplayAuthority1Error> {
        let n = self.u32()? as usize;
        if n > cap {
            return Err(ReplayAuthority1Error::InvalidWire("byte slice exceeds cap"));
        }
        Ok(self.grab(n)?.to_vec())
    }
    fn eof(&self) -> Result<(), ReplayAuthority1Error> {
        if self.pos != self.bytes.len() {
            return Err(ReplayAuthority1Error::InvalidWire("trailing sra bytes"));
        }
        Ok(())
    }
}

fn sra_push_bytes(out: &mut Vec<u8>, slice: &[u8]) -> Result<(), ReplayAuthority1Error> {
    let n = u32::try_from(slice.len())
        .map_err(|_| ReplayAuthority1Error::InvalidWire("slice len > u32"))?;
    out.extend_from_slice(&n.to_be_bytes());
    out.extend_from_slice(slice);
    Ok(())
}

/// Replay evidence archive event union (`LAYER1_DESIGN_PAPER.md` §3931–3944).
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum ReplayEvidenceEventV1 {
    /// Raw accepted proposal wrapper occurrence (total-decoded as `AcceptedProposalWrapperV1`).
    AcceptedRecordOuterOccurrence {
        /// Consensus-native occurrence ordinal matching `CONSENSUS-1` receipt.
        occurrence_ordinal: u64,
        /// Ledger height where this wrapper occurrence was recorded.
        event_ledger_height: u64,
        /// Raw canonical `AcceptedProposalWrapperV1` bytes.
        raw_wrapper_bytes: Vec<u8>,
        /// Committed availability-assessment digest bound in the same wrapper event.
        availability_assessment_id: [u8; 32],
        /// Whether this occurrence carried conflict material.
        has_conflict_material: bool,
        /// Domain-separated digest committing this outer occurrence.
        outer_occurrence_id: [u8; 32],
    },
    /// Historical parent-authority consumer use event carrying exact `CONSENSUS-1` certificate bytes.
    AcceptedParentAuthorityUseOccurrence {
        /// Consumer purpose (`ReleaseAuthority` or `ActivationAuthority`).
        purpose: ParentAuthorityPurposeV1,
        /// Consumer object digest.
        consumer_object_digest: [u8; 32],
        /// Required consensus-native parent-authority position.
        required_position: u64,
        /// Ledger height where the consumer used the accepted parent authority.
        consumer_ledger_height: u64,
        /// Raw canonical `AcceptedConsensusNativeParentAuthorityCertificateV1` bytes.
        certificate_bytes: Vec<u8>,
        /// Exact canonical consumer preimage bytes referencing the accepted authority.
        consumer_preimage_bytes: Vec<u8>,
        /// Domain-separated digest committing this certificate occurrence.
        cert_occurrence_id: [u8; 32],
    },
}

impl ReplayEvidenceEventV1 {
    /// Constructs a committed outer-wrapper occurrence event after total-decoding `raw_wrapper_bytes`.
    pub fn outer_occurrence(
        occurrence_ordinal: u64,
        event_ledger_height: u64,
        wrapper: &AcceptedProposalWrapperV1,
        availability_assessment_id: [u8; 32],
        has_conflict_material: bool,
    ) -> Result<Self, ReplayAuthority1Error> {
        let raw_wrapper_bytes = wrapper.encode_canonical()?;
        let outer_occurrence_id = compute_outer_occurrence_id(
            occurrence_ordinal,
            event_ledger_height,
            &raw_wrapper_bytes,
            availability_assessment_id,
            has_conflict_material,
        );
        Ok(Self::AcceptedRecordOuterOccurrence {
            occurrence_ordinal,
            event_ledger_height,
            raw_wrapper_bytes,
            availability_assessment_id,
            has_conflict_material,
            outer_occurrence_id,
        })
    }

    /// Constructs a committed consumer-use event after canonical-encoding `cert`.
    pub fn parent_authority_use(
        cert: &AcceptedConsensusNativeParentAuthorityCertificateV1,
        consumer_ledger_height: u64,
        consumer_preimage_bytes: Vec<u8>,
    ) -> Result<Self, ReplayAuthority1Error> {
        let certificate_bytes = cert.encode_canonical()?;
        let cert_occurrence_id = compute_cert_occurrence_id(
            cert.consumer_context.purpose,
            cert.consumer_context.consumer_object_digest,
            cert.consumer_context.required_position,
            consumer_ledger_height,
            &certificate_bytes,
            &consumer_preimage_bytes,
        );
        Ok(Self::AcceptedParentAuthorityUseOccurrence {
            purpose: cert.consumer_context.purpose,
            consumer_object_digest: cert.consumer_context.consumer_object_digest,
            required_position: cert.consumer_context.required_position,
            consumer_ledger_height,
            certificate_bytes,
            consumer_preimage_bytes,
            cert_occurrence_id,
        })
    }

    /// Encodes this replay evidence event canonically.
    pub fn encode_canonical(&self) -> Result<Vec<u8>, ReplayAuthority1Error> {
        let mut out = Vec::new();
        match self {
            Self::AcceptedRecordOuterOccurrence {
                occurrence_ordinal,
                event_ledger_height,
                raw_wrapper_bytes,
                availability_assessment_id,
                has_conflict_material,
                outer_occurrence_id,
            } => {
                out.push(1);
                out.extend_from_slice(&occurrence_ordinal.to_be_bytes());
                out.extend_from_slice(&event_ledger_height.to_be_bytes());
                sra_push_bytes(&mut out, raw_wrapper_bytes)?;
                out.extend_from_slice(availability_assessment_id);
                out.push(u8::from(*has_conflict_material));
                out.extend_from_slice(outer_occurrence_id);
            }
            Self::AcceptedParentAuthorityUseOccurrence {
                purpose,
                consumer_object_digest,
                required_position,
                consumer_ledger_height,
                certificate_bytes,
                consumer_preimage_bytes,
                cert_occurrence_id,
            } => {
                out.push(2);
                out.push(purpose.code());
                out.extend_from_slice(consumer_object_digest);
                out.extend_from_slice(&required_position.to_be_bytes());
                out.extend_from_slice(&consumer_ledger_height.to_be_bytes());
                sra_push_bytes(&mut out, certificate_bytes)?;
                sra_push_bytes(&mut out, consumer_preimage_bytes)?;
                out.extend_from_slice(cert_occurrence_id);
            }
        }
        Ok(out)
    }

    /// Decodes a canonical [`ReplayEvidenceEventV1`] fail-closed.
    pub fn decode_canonical(raw: &[u8]) -> Result<Self, ReplayAuthority1Error> {
        let mut s = SraSlice::new(raw);
        let res = Self::decode_from(&mut s)?;
        s.eof()?;
        Ok(res)
    }

    fn decode_from(s: &mut SraSlice<'_>) -> Result<Self, ReplayAuthority1Error> {
        match s.u8()? {
            1 => {
                let occurrence_ordinal = s.u64()?;
                let event_ledger_height = s.u64()?;
                let raw_wrapper_bytes = s.vec_u32(256 * 1024)?;
                let availability_assessment_id = s.b32()?;
                let has_conflict_material = s.flag()?;
                let outer_occurrence_id = s.b32()?;
                Ok(Self::AcceptedRecordOuterOccurrence {
                    occurrence_ordinal,
                    event_ledger_height,
                    raw_wrapper_bytes,
                    availability_assessment_id,
                    has_conflict_material,
                    outer_occurrence_id,
                })
            }
            2 => {
                let purpose = ParentAuthorityPurposeV1::from_code(s.u8()?)?;
                let consumer_object_digest = s.b32()?;
                let required_position = s.u64()?;
                let consumer_ledger_height = s.u64()?;
                let certificate_bytes = s.vec_u32(256 * 1024)?;
                let consumer_preimage_bytes = s.vec_u32(64 * 1024)?;
                let cert_occurrence_id = s.b32()?;
                Ok(Self::AcceptedParentAuthorityUseOccurrence {
                    purpose,
                    consumer_object_digest,
                    required_position,
                    consumer_ledger_height,
                    certificate_bytes,
                    consumer_preimage_bytes,
                    cert_occurrence_id,
                })
            }
            _ => Err(ReplayAuthority1Error::InvalidWire(
                "unknown ReplayEvidenceEventV1 discriminant",
            )),
        }
    }
}

/// Computes `accepted_record_outer_occurrence_id`.
pub fn compute_outer_occurrence_id(
    occurrence_ordinal: u64,
    event_ledger_height: u64,
    raw_wrapper_bytes: &[u8],
    availability_assessment_id: [u8; 32],
    has_conflict_material: bool,
) -> [u8; 32] {
    let mut h = Sha256::new();
    h.update(SRA_OUTER_OCC_PREFIX);
    h.update(occurrence_ordinal.to_be_bytes());
    h.update(event_ledger_height.to_be_bytes());
    h.update((raw_wrapper_bytes.len() as u32).to_be_bytes());
    h.update(raw_wrapper_bytes);
    h.update(availability_assessment_id);
    h.update([u8::from(has_conflict_material)]);
    h.finalize().into()
}

/// Computes `accepted_parent_authority_certificate_occurrence_id`.
pub fn compute_cert_occurrence_id(
    purpose: ParentAuthorityPurposeV1,
    consumer_object_digest: [u8; 32],
    required_position: u64,
    consumer_ledger_height: u64,
    certificate_bytes: &[u8],
    consumer_preimage_bytes: &[u8],
) -> [u8; 32] {
    let mut h = Sha256::new();
    h.update(SRA_CERT_OCC_PREFIX);
    h.update([purpose.code()]);
    h.update(consumer_object_digest);
    h.update(required_position.to_be_bytes());
    h.update(consumer_ledger_height.to_be_bytes());
    h.update((certificate_bytes.len() as u32).to_be_bytes());
    h.update(certificate_bytes);
    h.update((consumer_preimage_bytes.len() as u32).to_be_bytes());
    h.update(consumer_preimage_bytes);
    h.finalize().into()
}

/// Chained contiguous replay archive event record (`LAYER1_DESIGN_PAPER.md` §3946–3957).
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct ReplayEvidenceEventRecordV1 {
    /// Record schema version (`1`).
    pub schema_version: u16,
    /// Owning replay archive identifier.
    pub archive_id: [u8; 32],
    /// Strictly contiguous 1-based ordinal inside the replay archive.
    pub event_ordinal: u64,
    /// Predecessor event ID (`archive_id` at ordinal `1`).
    pub prev_event_id: [u8; 32],
    /// Inner replay evidence event payload.
    pub event: ReplayEvidenceEventV1,
}

impl ReplayEvidenceEventRecordV1 {
    /// Encodes this record canonically.
    pub fn encode_canonical(&self) -> Result<Vec<u8>, ReplayAuthority1Error> {
        if self.schema_version != SYNC_REPLAY_AUTH_VERSION || self.event_ordinal == 0 {
            return Err(ReplayAuthority1Error::InvalidWire(
                "bad event record version or ordinal",
            ));
        }
        let ev = self.event.encode_canonical()?;
        let mut out = Vec::with_capacity(2 + 32 + 8 + 32 + ev.len());
        out.extend_from_slice(&self.schema_version.to_be_bytes());
        out.extend_from_slice(&self.archive_id);
        out.extend_from_slice(&self.event_ordinal.to_be_bytes());
        out.extend_from_slice(&self.prev_event_id);
        out.extend_from_slice(&ev);
        Ok(out)
    }

    /// Decodes a canonical [`ReplayEvidenceEventRecordV1`] fail-closed.
    pub fn decode_canonical(raw: &[u8]) -> Result<Self, ReplayAuthority1Error> {
        let mut s = SraSlice::new(raw);
        let r = Self::decode_from(&mut s)?;
        s.eof()?;
        Ok(r)
    }

    fn decode_from(s: &mut SraSlice<'_>) -> Result<Self, ReplayAuthority1Error> {
        let schema_version = s.u16()?;
        let archive_id = s.b32()?;
        let event_ordinal = s.u64()?;
        if schema_version != SYNC_REPLAY_AUTH_VERSION || event_ordinal == 0 {
            return Err(ReplayAuthority1Error::InvalidWire(
                "bad event record version or ordinal",
            ));
        }
        let prev_event_id = s.b32()?;
        let event = ReplayEvidenceEventV1::decode_from(s)?;
        Ok(Self {
            schema_version,
            archive_id,
            event_ordinal,
            prev_event_id,
            event,
        })
    }

    /// Computes `replay_evidence_event_id`.
    pub fn event_id(&self) -> Result<[u8; 32], ReplayAuthority1Error> {
        let enc = self.encode_canonical()?;
        let mut h = Sha256::new();
        h.update(SRA_EVENT_REC_PREFIX);
        h.update((enc.len() as u32).to_be_bytes());
        h.update(enc);
        Ok(h.finalize().into())
    }
}

/// Computes `ordered_event_prefix_root` committing archive ID, sequence length, every ordinal, and
/// every event ID.
pub fn compute_ordered_event_prefix_root(
    archive_id: [u8; 32],
    records: &[ReplayEvidenceEventRecordV1],
) -> Result<[u8; 32], ReplayAuthority1Error> {
    let mut h = Sha256::new();
    h.update(SRA_PREFIX_ROOT_PREFIX);
    h.update(archive_id);
    h.update((records.len() as u64).to_be_bytes());
    for rec in records {
        h.update(rec.event_ordinal.to_be_bytes());
        h.update(rec.event_id()?);
    }
    Ok(h.finalize().into())
}

/// Proof-free status projection core (`LAYER1_DESIGN_PAPER.md` §3959–3966, `130` bytes).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ReplayParentAuthorityStatusProjectionCoreV1 {
    /// Frozen `ProfileIdHashV1` lookup ID.
    pub accepted_parent_lookup_id: [u8; 32],
    /// Unique signature-free accepted proposal core ID.
    pub unique_accepted_proposal_core_block_id: [u8; 32],
    /// Purpose of the parent-authority use.
    pub purpose: ParentAuthorityPurposeV1,
    /// Eligibility policy digest.
    pub eligibility_policy_id: [u8; 32],
    /// Required predicate digest.
    pub required_predicate_id: [u8; 32],
    /// Projected parent-authority attainment status.
    pub projected_status: ParentAuthorityStatusV1,
}

impl ReplayParentAuthorityStatusProjectionCoreV1 {
    /// Fixed canonical wire length (`130` bytes).
    pub const WIRE_LEN: usize = 32 + 32 + 1 + 32 + 32 + 1;

    /// Encodes this status projection core into 130 canonical big-endian bytes.
    pub fn encode_canonical(&self) -> [u8; Self::WIRE_LEN] {
        let mut out = Vec::with_capacity(Self::WIRE_LEN);
        out.extend_from_slice(&self.accepted_parent_lookup_id);
        out.extend_from_slice(&self.unique_accepted_proposal_core_block_id);
        out.push(self.purpose.code());
        out.extend_from_slice(&self.eligibility_policy_id);
        out.extend_from_slice(&self.required_predicate_id);
        out.push(self.projected_status.code());
        out.try_into().expect("130")
    }

    /// Decodes 130 canonical bytes fail-closed.
    pub fn decode_canonical(raw: &[u8]) -> Result<Self, ReplayAuthority1Error> {
        let mut s = SraSlice::new(raw);
        let accepted_parent_lookup_id = s.b32()?;
        let unique_accepted_proposal_core_block_id = s.b32()?;
        let purpose = ParentAuthorityPurposeV1::from_code(s.u8()?)?;
        let eligibility_policy_id = s.b32()?;
        let required_predicate_id = s.b32()?;
        let projected_status = ParentAuthorityStatusV1::from_code(s.u8()?)?;
        s.eof()?;
        Ok(Self {
            accepted_parent_lookup_id,
            unique_accepted_proposal_core_block_id,
            purpose,
            eligibility_policy_id,
            required_predicate_id,
            projected_status,
        })
    }
}

/// Strictly proof-free normalized authority entry preimage (`LAYER1_DESIGN_PAPER.md` §3973–3985).
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct NormalizedAcceptedAuthorityEntryCoreV1 {
    /// Frozen `ProfileIdHashV1` lookup ID.
    pub accepted_parent_lookup_id: [u8; 32],
    /// Unique signature-free accepted proposal core ID.
    pub unique_accepted_proposal_core_block_id: [u8; 32],
    /// Strictly sorted unique proposer credential public keys reconstructed from decoded wrappers.
    pub credential_pubkeys: Vec<[u8; 32]>,
    /// Strictly sorted unique consensus evidence digests reconstructed from decoded wrappers.
    pub evidence_digests: Vec<[u8; 32]>,
    /// Canonical dependency-closure manifest digest reconstructed from decoded wrappers.
    pub dependency_closure_digest: [u8; 32],
    /// Strictly sorted status projection cores.
    pub status_projections: Vec<ReplayParentAuthorityStatusProjectionCoreV1>,
    /// Whether any pre-cutoff wrapper carried conflict material (must never be dropped!).
    pub includes_conflict_material: bool,
}

impl NormalizedAcceptedAuthorityEntryCoreV1 {
    /// Encodes this proof-free normalized entry canonically.
    pub fn encode_canonical(&self) -> Result<Vec<u8>, ReplayAuthority1Error> {
        for pair in self.credential_pubkeys.windows(2) {
            if pair[0] >= pair[1] {
                return Err(ReplayAuthority1Error::InvalidWire(
                    "credential_pubkeys unsorted or duplicate",
                ));
            }
        }
        for pair in self.evidence_digests.windows(2) {
            if pair[0] >= pair[1] {
                return Err(ReplayAuthority1Error::InvalidWire(
                    "evidence_digests unsorted or duplicate",
                ));
            }
        }
        for pair in self.status_projections.windows(2) {
            if pair[0].purpose >= pair[1].purpose {
                return Err(ReplayAuthority1Error::InvalidWire(
                    "status_projections unsorted or duplicate purpose",
                ));
            }
        }
        let mut out = Vec::with_capacity(
            128 + self.credential_pubkeys.len() * 32
                + self.evidence_digests.len() * 32
                + self.status_projections.len()
                    * ReplayParentAuthorityStatusProjectionCoreV1::WIRE_LEN,
        );
        out.extend_from_slice(&self.accepted_parent_lookup_id);
        out.extend_from_slice(&self.unique_accepted_proposal_core_block_id);
        out.extend_from_slice(&(self.credential_pubkeys.len() as u16).to_be_bytes());
        for pk in &self.credential_pubkeys {
            out.extend_from_slice(pk);
        }
        out.extend_from_slice(&(self.evidence_digests.len() as u16).to_be_bytes());
        for ev in &self.evidence_digests {
            out.extend_from_slice(ev);
        }
        out.extend_from_slice(&self.dependency_closure_digest);
        out.extend_from_slice(&(self.status_projections.len() as u16).to_be_bytes());
        for sp in &self.status_projections {
            out.extend_from_slice(&sp.encode_canonical());
        }
        out.push(u8::from(self.includes_conflict_material));
        Ok(out)
    }

    /// Decodes a canonical [`NormalizedAcceptedAuthorityEntryCoreV1`] fail-closed.
    pub fn decode_canonical(raw: &[u8]) -> Result<Self, ReplayAuthority1Error> {
        let mut s = SraSlice::new(raw);
        let r = Self::decode_from(&mut s)?;
        s.eof()?;
        Ok(r)
    }

    fn decode_from(s: &mut SraSlice<'_>) -> Result<Self, ReplayAuthority1Error> {
        let accepted_parent_lookup_id = s.b32()?;
        let unique_accepted_proposal_core_block_id = s.b32()?;
        let cc = s.u16()? as usize;
        if cc > 256 {
            return Err(ReplayAuthority1Error::InvalidWire("too many credentials"));
        }
        let mut credential_pubkeys = Vec::with_capacity(cc);
        for _ in 0..cc {
            credential_pubkeys.push(s.b32()?);
        }
        let ec = s.u16()? as usize;
        if ec > 256 {
            return Err(ReplayAuthority1Error::InvalidWire(
                "too many evidence digests",
            ));
        }
        let mut evidence_digests = Vec::with_capacity(ec);
        for _ in 0..ec {
            evidence_digests.push(s.b32()?);
        }
        let dependency_closure_digest = s.b32()?;
        let sc = s.u16()? as usize;
        if sc > 16 {
            return Err(ReplayAuthority1Error::InvalidWire("too many projections"));
        }
        let mut status_projections = Vec::with_capacity(sc);
        for _ in 0..sc {
            status_projections.push(
                ReplayParentAuthorityStatusProjectionCoreV1::decode_canonical(
                    s.grab(ReplayParentAuthorityStatusProjectionCoreV1::WIRE_LEN)?,
                )?,
            );
        }
        let includes_conflict_material = s.flag()?;
        Ok(Self {
            accepted_parent_lookup_id,
            unique_accepted_proposal_core_block_id,
            credential_pubkeys,
            evidence_digests,
            dependency_closure_digest,
            status_projections,
            includes_conflict_material,
        })
    }

    /// Independently recomputes `normalized_authority_entry_id` from this proof-free core.
    pub fn entry_id(&self) -> Result<[u8; 32], ReplayAuthority1Error> {
        let enc = self.encode_canonical()?;
        let mut h = Sha256::new();
        h.update(SRA_NORM_ENTRY_PREFIX);
        h.update((enc.len() as u32).to_be_bytes());
        h.update(enc);
        Ok(h.finalize().into())
    }
}

/// Computes `normalized_authority_view_root` over sorted unique `(lookup_id, entry_id)` pairs.
pub fn compute_normalized_authority_view_root(
    entries: &[NormalizedAcceptedAuthorityEntryCoreV1],
) -> Result<[u8; 32], ReplayAuthority1Error> {
    let mut pairs: Vec<([u8; 32], [u8; 32])> = Vec::with_capacity(entries.len());
    for e in entries {
        pairs.push((e.accepted_parent_lookup_id, e.entry_id()?));
    }
    pairs.sort_by_key(|(lid, _)| *lid);
    for w in pairs.windows(2) {
        if w[0].0 == w[1].0 {
            return Err(ReplayAuthority1Error::CircularOrSubstitutedNormalizedEntry(
                "duplicate accepted_parent_lookup_id in normalized view",
            ));
        }
    }
    let mut h = Sha256::new();
    h.update(SRA_NORM_VIEW_PREFIX);
    h.update((pairs.len() as u32).to_be_bytes());
    for (lid, eid) in pairs {
        h.update(lid);
        h.update(eid);
    }
    Ok(h.finalize().into())
}

/// Authorized replay-evidence archive checkpoint (`bitblock/sync-1/replay-archive-checkpoint/v1`, `363` bytes).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ReplayEvidenceArchiveCheckpointV1 {
    /// Checkpoint schema version (`1`).
    pub checkpoint_version: u16,
    /// Chain identifier.
    pub chain_id: u64,
    /// Genesis identifier.
    pub genesis_id: [u8; 32],
    /// Replay archive identifier.
    pub archive_id: [u8; 32],
    /// Pre-existing governance/launch archive policy digest (verified against resolver).
    pub archive_policy_id: [u8; 32],
    /// Previous checkpoint ID (or `archive_id` for the initial archive checkpoint).
    pub prev_checkpoint_id: [u8; 32],
    /// Ledger height when this archive checkpoint was sealed (may validly be `> consumer_ledger_height`!).
    pub checkpoint_ledger_height: u64,
    /// First covered contiguous event ordinal (`1`).
    pub first_covered_ordinal: u64,
    /// Last covered contiguous event ordinal (`>= first_covered_ordinal`).
    pub last_covered_ordinal: u64,
    /// Exact covered event count (`last - first + 1`).
    pub covered_event_count: u64,
    /// Prefix root committing every covered event ordinal & ID.
    pub ordered_event_prefix_root: [u8; 32],
    /// Normalization policy digest (verified against resolver).
    pub normalization_policy_id: [u8; 32],
    /// Root over sorted unique `(lookup_id, recomputed_entry_id)` pairs.
    pub normalized_authority_view_root: [u8; 32],
    /// Publication completeness marker (`true` only after atomic publication; rejects partial prefixes).
    pub publication_complete: bool,
    /// Pre-existing authorized checkpoint signer's Ed25519 public key.
    pub checkpoint_authority_pubkey: [u8; 32],
    /// Strict RFC 8032 Ed25519 signature (`SigningDomainTagV1::ReleaseArtifact`).
    pub checkpoint_signature: [u8; 64],
}

impl ReplayEvidenceArchiveCheckpointV1 {
    /// Fixed canonical big-endian wire length (`363` bytes).
    pub const WIRE_LEN: usize = 2 + 8 + 32 * 7 + 8 * 4 + 1 + 32 + 64;

    /// Computes `replay_evidence_archive_checkpoint_id` over all unsigned core fields.
    pub fn checkpoint_id(&self) -> [u8; 32] {
        let mut h = Sha256::new();
        h.update(SRA_CHK_CORE_PREFIX);
        h.update(self.checkpoint_version.to_be_bytes());
        h.update(self.chain_id.to_be_bytes());
        h.update(self.genesis_id);
        h.update(self.archive_id);
        h.update(self.archive_policy_id);
        h.update(self.prev_checkpoint_id);
        h.update(self.checkpoint_ledger_height.to_be_bytes());
        h.update(self.first_covered_ordinal.to_be_bytes());
        h.update(self.last_covered_ordinal.to_be_bytes());
        h.update(self.covered_event_count.to_be_bytes());
        h.update(self.ordered_event_prefix_root);
        h.update(self.normalization_policy_id);
        h.update(self.normalized_authority_view_root);
        h.update([u8::from(self.publication_complete)]);
        h.finalize().into()
    }

    /// Computes the detached checkpoint sign-doc digest signed by checkpoint authority.
    pub fn sign_doc_digest(&self) -> [u8; 32] {
        let mut h = Sha256::new();
        h.update(SRA_CHK_SIGN_PREFIX);
        h.update(self.checkpoint_id());
        h.update(self.archive_policy_id);
        h.update(self.normalization_policy_id);
        h.finalize().into()
    }

    /// Builds [`SigningPreimageV1`] under `SigningDomainTagV1::ReleaseArtifact`.
    pub fn signing_preimage(&self, fork: &ExecutorForkContextV1) -> SigningPreimageV1 {
        let payload_digest = self.sign_doc_digest();
        let (chain_id, genesis_id, profile_id) = (fork.chain_id, fork.genesis_id, fork.profile_id);
        SigningPreimageV1 {
            payload_digest,
            domain_tag: SigningDomainTagV1::ReleaseArtifact,
            signature_suite: SignatureSuiteIdV1::Ed25519StrictRfc8032V1,
            hash_suite: HashSuiteIdV1::Sha256V1,
            protocol_version: 1,
            chain_id,
            genesis_id,
            launch_manifest_id: genesis_id,
            profile_id,
        }
    }

    /// Signs this checkpoint deterministically with `auth_seed`.
    pub fn sign_with_seed(
        mut self,
        auth_seed: &[u8; 32],
        fork: &ExecutorForkContextV1,
    ) -> Result<Self, ReplayAuthority1Error> {
        let pk = CanonicalEd25519PublicKeyV1::from_secret_seed(auth_seed)
            .map_err(|_| ReplayAuthority1Error::InvalidCheckpointAuthorization("bad auth seed"))?;
        let sig = sign_preimage_deterministic(auth_seed, &self.signing_preimage(fork))
            .map_err(|_| ReplayAuthority1Error::InvalidCheckpointAuthorization("sign failed"))?;
        self.checkpoint_authority_pubkey = *pk.as_bytes();
        self.checkpoint_signature = *sig.as_bytes();
        Ok(self)
    }

    /// Verifies carried policy/root IDs strictly against pre-existing resolver expectations (carried
    /// IDs are byte-equal assertions, never selectors!) and rejects incomplete publications.
    pub fn verify_authorization(
        &self,
        fork: &ExecutorForkContextV1,
        expected_archive_policy_id: [u8; 32],
        expected_norm_policy_id: [u8; 32],
        expected_auth_pk: &CanonicalEd25519PublicKeyV1,
    ) -> Result<(), ReplayAuthority1Error> {
        if self.checkpoint_version != SYNC_REPLAY_AUTH_VERSION {
            return Err(ReplayAuthority1Error::InvalidWire("bad checkpoint version"));
        }
        if !self.publication_complete {
            return Err(ReplayAuthority1Error::PartialCheckpointPublicationRejected(
                "checkpoint publication_complete flag is false",
            ));
        }
        if self.chain_id != fork.chain_id || self.genesis_id != fork.genesis_id {
            return Err(ReplayAuthority1Error::MixedRootPolicyOrViewRejected(
                "checkpoint chain_id or genesis_id mismatch",
            ));
        }
        if self.archive_policy_id != expected_archive_policy_id
            || self.normalization_policy_id != expected_norm_policy_id
        {
            return Err(ReplayAuthority1Error::MixedRootPolicyOrViewRejected(
                "checkpoint archive_policy_id or normalization_policy_id mismatch",
            ));
        }
        if self.first_covered_ordinal != 1
            || self.last_covered_ordinal < self.first_covered_ordinal
            || self.covered_event_count
                != (self.last_covered_ordinal - self.first_covered_ordinal + 1)
        {
            return Err(ReplayAuthority1Error::EventReorderOrGapRejected(
                "non-contiguous checkpoint ordinal range",
            ));
        }
        if self.checkpoint_authority_pubkey != *expected_auth_pk.as_bytes() {
            return Err(ReplayAuthority1Error::InvalidCheckpointAuthorization(
                "checkpoint signer disagrees with pre-existing resolver authority",
            ));
        }
        let sig = CanonicalEd25519SignatureV1::from_bytes(self.checkpoint_signature)
            .map_err(|_| ReplayAuthority1Error::InvalidCheckpointAuthorization("malformed sig"))?;
        verify_preimage_strict(expected_auth_pk, &self.signing_preimage(fork), &sig).map_err(
            |_| ReplayAuthority1Error::InvalidCheckpointAuthorization("sig verification failed"),
        )?;
        Ok(())
    }

    /// Encodes this checkpoint into 363 canonical big-endian bytes.
    pub fn encode_canonical(&self) -> Result<[u8; Self::WIRE_LEN], ReplayAuthority1Error> {
        if self.checkpoint_version != SYNC_REPLAY_AUTH_VERSION {
            return Err(ReplayAuthority1Error::InvalidWire("bad checkpoint version"));
        }
        let mut out = Vec::with_capacity(Self::WIRE_LEN);
        out.extend_from_slice(&self.checkpoint_version.to_be_bytes());
        out.extend_from_slice(&self.chain_id.to_be_bytes());
        out.extend_from_slice(&self.genesis_id);
        out.extend_from_slice(&self.archive_id);
        out.extend_from_slice(&self.archive_policy_id);
        out.extend_from_slice(&self.prev_checkpoint_id);
        out.extend_from_slice(&self.checkpoint_ledger_height.to_be_bytes());
        out.extend_from_slice(&self.first_covered_ordinal.to_be_bytes());
        out.extend_from_slice(&self.last_covered_ordinal.to_be_bytes());
        out.extend_from_slice(&self.covered_event_count.to_be_bytes());
        out.extend_from_slice(&self.ordered_event_prefix_root);
        out.extend_from_slice(&self.normalization_policy_id);
        out.extend_from_slice(&self.normalized_authority_view_root);
        out.push(u8::from(self.publication_complete));
        out.extend_from_slice(&self.checkpoint_authority_pubkey);
        out.extend_from_slice(&self.checkpoint_signature);
        Ok(out.try_into().expect("363"))
    }

    /// Decodes 363 canonical big-endian bytes fail-closed.
    pub fn decode_canonical(raw: &[u8]) -> Result<Self, ReplayAuthority1Error> {
        let mut s = SraSlice::new(raw);
        let checkpoint_version = s.u16()?;
        if checkpoint_version != SYNC_REPLAY_AUTH_VERSION {
            return Err(ReplayAuthority1Error::InvalidWire("bad checkpoint version"));
        }
        let chain_id = s.u64()?;
        let genesis_id = s.b32()?;
        let archive_id = s.b32()?;
        let archive_policy_id = s.b32()?;
        let prev_checkpoint_id = s.b32()?;
        let checkpoint_ledger_height = s.u64()?;
        let first_covered_ordinal = s.u64()?;
        let last_covered_ordinal = s.u64()?;
        let covered_event_count = s.u64()?;
        let ordered_event_prefix_root = s.b32()?;
        let normalization_policy_id = s.b32()?;
        let normalized_authority_view_root = s.b32()?;
        let publication_complete = s.flag()?;
        let checkpoint_authority_pubkey = s.b32()?;
        let checkpoint_signature = s.b64()?;
        s.eof()?;
        Ok(Self {
            checkpoint_version,
            chain_id,
            genesis_id,
            archive_id,
            archive_policy_id,
            prev_checkpoint_id,
            checkpoint_ledger_height,
            first_covered_ordinal,
            last_covered_ordinal,
            covered_event_count,
            ordered_event_prefix_root,
            normalization_policy_id,
            normalized_authority_view_root,
            publication_complete,
            checkpoint_authority_pubkey,
            checkpoint_signature,
        })
    }

    /// Computes the canonical `Receipt` `ContentTag` (`bitblock/sync-1/replay-archive-checkpoint/v1`).
    pub fn tag(&self) -> Result<ContentTag, ReplayAuthority1Error> {
        let raw = self.encode_canonical()?;
        hash_canonical(
            TagKind::Receipt,
            SYNC_REPLAY_CHECKPOINT_SCHEMA,
            raw.len() as u64,
            |w| w(&raw),
        )
        .map_err(|_| ReplayAuthority1Error::InvalidWire("checkpoint hash_canonical failed"))
    }
}

/// Complete ordered event prefix proof (`LAYER1_DESIGN_PAPER.md` §4065–4073).
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct ReplayEvidencePrefixCompletenessProofV1 {
    /// Bound checkpoint identifier.
    pub checkpoint_id: [u8; 32],
    /// First event ordinal (`1`).
    pub first_ordinal: u64,
    /// Last event ordinal.
    pub last_ordinal: u64,
    /// Exact event count (`records.len() as u64`).
    pub covered_event_count: u64,
    /// Committed `ordered_event_prefix_root`.
    pub ordered_event_prefix_root: [u8; 32],
    /// Complete ordered contiguous replay evidence records.
    pub records: Vec<ReplayEvidenceEventRecordV1>,
}

impl ReplayEvidencePrefixCompletenessProofV1 {
    /// Verifies contiguity, predecessor hash-chaining, duplicate absence, inner occurrence hashes,
    /// and exact `ordered_event_prefix_root` match against `checkpoint`.
    pub fn verify(
        &self,
        checkpoint: &ReplayEvidenceArchiveCheckpointV1,
    ) -> Result<(), ReplayAuthority1Error> {
        if self.checkpoint_id != checkpoint.checkpoint_id()
            || self.ordered_event_prefix_root != checkpoint.ordered_event_prefix_root
        {
            return Err(ReplayAuthority1Error::MixedRootPolicyOrViewRejected(
                "prefix completeness proof disagrees with checkpoint root/id",
            ));
        }
        if self.records.len() as u64 != self.covered_event_count
            || self.covered_event_count != checkpoint.covered_event_count
            || self.first_ordinal != checkpoint.first_covered_ordinal
            || self.last_ordinal != checkpoint.last_covered_ordinal
        {
            return Err(ReplayAuthority1Error::PartialCheckpointPublicationRejected(
                "prefix completeness proof missing events from checkpoint range",
            ));
        }
        let mut expected_prev = checkpoint.archive_id;
        let mut seen_ordinals = BTreeSet::new();
        let mut seen_outer_ids = BTreeSet::new();
        for (i, rec) in self.records.iter().enumerate() {
            let expected_ord = self.first_ordinal + (i as u64);
            if !seen_ordinals.insert(rec.event_ordinal) {
                return Err(ReplayAuthority1Error::EventDuplicationRejected(
                    "duplicate event_ordinal in replay prefix",
                ));
            }
            if rec.event_ordinal != expected_ord {
                return Err(ReplayAuthority1Error::EventReorderOrGapRejected(
                    "event_ordinal out of contiguous order",
                ));
            }
            if rec.archive_id != checkpoint.archive_id {
                return Err(ReplayAuthority1Error::MixedRootPolicyOrViewRejected(
                    "event record archive_id disagrees with checkpoint",
                ));
            }
            if rec.prev_event_id != expected_prev {
                return Err(ReplayAuthority1Error::EventReorderOrGapRejected(
                    "broken predecessor event link in replay archive",
                ));
            }
            match &rec.event {
                ReplayEvidenceEventV1::AcceptedRecordOuterOccurrence {
                    occurrence_ordinal,
                    event_ledger_height,
                    raw_wrapper_bytes,
                    availability_assessment_id,
                    has_conflict_material,
                    outer_occurrence_id,
                } => {
                    let recomputed = compute_outer_occurrence_id(
                        *occurrence_ordinal,
                        *event_ledger_height,
                        raw_wrapper_bytes,
                        *availability_assessment_id,
                        *has_conflict_material,
                    );
                    if recomputed != *outer_occurrence_id {
                        return Err(ReplayAuthority1Error::CertificateToReplayMismatch(
                            "outer_occurrence_id mismatch",
                        ));
                    }
                    if !seen_outer_ids.insert(*outer_occurrence_id) {
                        return Err(ReplayAuthority1Error::EventDuplicationRejected(
                            "duplicate outer_occurrence_id in replay prefix",
                        ));
                    }
                }
                ReplayEvidenceEventV1::AcceptedParentAuthorityUseOccurrence {
                    purpose,
                    consumer_object_digest,
                    required_position,
                    consumer_ledger_height,
                    certificate_bytes,
                    consumer_preimage_bytes,
                    cert_occurrence_id,
                } => {
                    let recomputed = compute_cert_occurrence_id(
                        *purpose,
                        *consumer_object_digest,
                        *required_position,
                        *consumer_ledger_height,
                        certificate_bytes,
                        consumer_preimage_bytes,
                    );
                    if recomputed != *cert_occurrence_id {
                        return Err(ReplayAuthority1Error::CertificateToReplayMismatch(
                            "cert_occurrence_id mismatch",
                        ));
                    }
                }
            }
            expected_prev = rec.event_id()?;
        }
        let recomputed_root =
            compute_ordered_event_prefix_root(checkpoint.archive_id, &self.records)?;
        if recomputed_root != self.ordered_event_prefix_root {
            return Err(ReplayAuthority1Error::MixedRootPolicyOrViewRejected(
                "recomputed ordered_event_prefix_root mismatch",
            ));
        }
        Ok(())
    }

    /// Encodes this prefix completeness proof canonically.
    pub fn encode_canonical(&self) -> Result<Vec<u8>, ReplayAuthority1Error> {
        let mut out = Vec::new();
        out.extend_from_slice(&self.checkpoint_id);
        out.extend_from_slice(&self.first_ordinal.to_be_bytes());
        out.extend_from_slice(&self.last_ordinal.to_be_bytes());
        out.extend_from_slice(&self.covered_event_count.to_be_bytes());
        out.extend_from_slice(&self.ordered_event_prefix_root);
        out.extend_from_slice(&(self.records.len() as u32).to_be_bytes());
        for rec in &self.records {
            sra_push_bytes(&mut out, &rec.encode_canonical()?)?;
        }
        Ok(out)
    }

    fn decode_from(s: &mut SraSlice<'_>) -> Result<Self, ReplayAuthority1Error> {
        let checkpoint_id = s.b32()?;
        let first_ordinal = s.u64()?;
        let last_ordinal = s.u64()?;
        let covered_event_count = s.u64()?;
        let ordered_event_prefix_root = s.b32()?;
        let n = s.u32()? as usize;
        if n > 1024 {
            return Err(ReplayAuthority1Error::InvalidWire(
                "too many prefix records",
            ));
        }
        let mut records = Vec::with_capacity(n);
        for _ in 0..n {
            let rb = s.vec_u32(512 * 1024)?;
            records.push(ReplayEvidenceEventRecordV1::decode_canonical(&rb)?);
        }
        Ok(Self {
            checkpoint_id,
            first_ordinal,
            last_ordinal,
            covered_event_count,
            ordered_event_prefix_root,
            records,
        })
    }
}

/// Append-only consistency proof linking historical checkpoint `C1` to later checkpoint `C2` (`104` bytes).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ReplayEvidenceCheckpointConsistencyProofV1 {
    /// Earlier historical checkpoint ID (`C1`).
    pub historical_checkpoint_id: [u8; 32],
    /// Later view checkpoint ID (`C2`).
    pub view_checkpoint_id: [u8; 32],
    /// Shared prefix event count (`C1.covered_event_count`).
    pub shared_prefix_count: u64,
    /// Domain-separated consistency witness binding `(C1.prefix_root, C2.prefix_root, shared_count)`.
    pub consistency_witness_digest: [u8; 32],
}

impl ReplayEvidenceCheckpointConsistencyProofV1 {
    /// Fixed canonical big-endian wire length (`104` bytes).
    pub const WIRE_LEN: usize = 32 + 32 + 8 + 32;

    /// Constructs a verified append-consistency proof from `historical` (`C1`) to `view_cp` (`C2`).
    pub fn build(
        historical: &ReplayEvidenceArchiveCheckpointV1,
        view_cp: &ReplayEvidenceArchiveCheckpointV1,
        view_records: &[ReplayEvidenceEventRecordV1],
    ) -> Result<Self, ReplayAuthority1Error> {
        let shared = historical.covered_event_count as usize;
        if view_cp.prev_checkpoint_id != historical.checkpoint_id()
            || view_cp.covered_event_count < historical.covered_event_count
            || view_records.len() < shared
        {
            return Err(ReplayAuthority1Error::AppendConsistencyViolation(
                "C2 does not extend C1",
            ));
        }
        let prefix_root_check =
            compute_ordered_event_prefix_root(historical.archive_id, &view_records[..shared])?;
        if prefix_root_check != historical.ordered_event_prefix_root {
            return Err(ReplayAuthority1Error::AppendConsistencyViolation(
                "C2 shared prefix root disagrees with C1",
            ));
        }
        let witness = compute_consistency_witness(
            historical.ordered_event_prefix_root,
            view_cp.ordered_event_prefix_root,
            historical.covered_event_count,
        );
        Ok(Self {
            historical_checkpoint_id: historical.checkpoint_id(),
            view_checkpoint_id: view_cp.checkpoint_id(),
            shared_prefix_count: historical.covered_event_count,
            consistency_witness_digest: witness,
        })
    }

    /// Verifies append-consistency between `historical` (`C1`) and `view_cp` (`C2`).
    pub fn verify(
        &self,
        historical: &ReplayEvidenceArchiveCheckpointV1,
        view_cp: &ReplayEvidenceArchiveCheckpointV1,
        view_records: &[ReplayEvidenceEventRecordV1],
    ) -> Result<(), ReplayAuthority1Error> {
        let rebuilt = Self::build(historical, view_cp, view_records)?;
        if *self != rebuilt {
            return Err(ReplayAuthority1Error::AppendConsistencyViolation(
                "consistency proof fields disagree with C1 -> C2 prefix",
            ));
        }
        Ok(())
    }

    /// Encodes into 104 canonical big-endian bytes.
    pub fn encode_canonical(&self) -> [u8; Self::WIRE_LEN] {
        let mut out = Vec::with_capacity(Self::WIRE_LEN);
        out.extend_from_slice(&self.historical_checkpoint_id);
        out.extend_from_slice(&self.view_checkpoint_id);
        out.extend_from_slice(&self.shared_prefix_count.to_be_bytes());
        out.extend_from_slice(&self.consistency_witness_digest);
        out.try_into().expect("104")
    }

    /// Decodes 104 canonical bytes fail-closed.
    pub fn decode_canonical(raw: &[u8]) -> Result<Self, ReplayAuthority1Error> {
        let mut s = SraSlice::new(raw);
        let historical_checkpoint_id = s.b32()?;
        let view_checkpoint_id = s.b32()?;
        let shared_prefix_count = s.u64()?;
        let consistency_witness_digest = s.b32()?;
        s.eof()?;
        Ok(Self {
            historical_checkpoint_id,
            view_checkpoint_id,
            shared_prefix_count,
            consistency_witness_digest,
        })
    }
}

fn compute_consistency_witness(
    c1_prefix_root: [u8; 32],
    c2_prefix_root: [u8; 32],
    shared_count: u64,
) -> [u8; 32] {
    let mut h = Sha256::new();
    h.update(SRA_CONSISTENCY_PREFIX);
    h.update(c1_prefix_root);
    h.update(c2_prefix_root);
    h.update(shared_count.to_be_bytes());
    h.finalize().into()
}

/// One replay-wide view shared across the complete replay audit package (`LAYER1_DESIGN_PAPER.md` §4084–4101).
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct ReplayEvidenceViewV1 {
    /// View schema version (`1`).
    pub view_version: u16,
    /// Authorized replay archive checkpoint backing this view.
    pub checkpoint: ReplayEvidenceArchiveCheckpointV1,
    /// Named replay projection cutoff ledger height.
    pub cutoff_ledger_height: u64,
    /// Named replay projection cutoff event ordinal (`<= checkpoint.last_covered_ordinal`).
    pub cutoff_event_ordinal: u64,
    /// Complete contiguous event prefix proof.
    pub prefix_proof: ReplayEvidencePrefixCompletenessProofV1,
    /// Proof-free normalized authority entries reconstructed over `1..=cutoff_event_ordinal`.
    pub normalized_entries: Vec<NormalizedAcceptedAuthorityEntryCoreV1>,
    /// Recomputed `replay_evidence_view_id`.
    pub view_id: [u8; 32],
}

impl ReplayEvidenceViewV1 {
    /// Recomputes `replay_evidence_view_id` from the view core fields.
    pub fn recompute_view_id(&self) -> Result<[u8; 32], ReplayAuthority1Error> {
        let norm_root = compute_normalized_authority_view_root(&self.normalized_entries)?;
        let mut h = Sha256::new();
        h.update(SRA_VIEW_CORE_PREFIX);
        h.update(self.view_version.to_be_bytes());
        h.update(self.checkpoint.checkpoint_id());
        h.update(self.cutoff_ledger_height.to_be_bytes());
        h.update(self.cutoff_event_ordinal.to_be_bytes());
        h.update(self.checkpoint.ordered_event_prefix_root);
        h.update(self.checkpoint.normalization_policy_id);
        h.update(norm_root);
        Ok(h.finalize().into())
    }

    /// Encodes this replay-wide view canonically.
    pub fn encode_canonical(&self) -> Result<Vec<u8>, ReplayAuthority1Error> {
        let cp = self.checkpoint.encode_canonical()?;
        let pp = self.prefix_proof.encode_canonical()?;
        let mut out = Vec::new();
        out.extend_from_slice(&self.view_version.to_be_bytes());
        out.extend_from_slice(&cp);
        out.extend_from_slice(&self.cutoff_ledger_height.to_be_bytes());
        out.extend_from_slice(&self.cutoff_event_ordinal.to_be_bytes());
        sra_push_bytes(&mut out, &pp)?;
        out.extend_from_slice(&(self.normalized_entries.len() as u16).to_be_bytes());
        for e in &self.normalized_entries {
            sra_push_bytes(&mut out, &e.encode_canonical()?)?;
        }
        out.extend_from_slice(&self.view_id);
        Ok(out)
    }

    fn decode_from(s: &mut SraSlice<'_>) -> Result<Self, ReplayAuthority1Error> {
        let view_version = s.u16()?;
        if view_version != SYNC_REPLAY_AUTH_VERSION {
            return Err(ReplayAuthority1Error::InvalidWire("bad view_version"));
        }
        let checkpoint = ReplayEvidenceArchiveCheckpointV1::decode_canonical(
            s.grab(ReplayEvidenceArchiveCheckpointV1::WIRE_LEN)?,
        )?;
        let cutoff_ledger_height = s.u64()?;
        let cutoff_event_ordinal = s.u64()?;
        let pp_bytes = s.vec_u32(4 * 1024 * 1024)?;
        let mut ps = SraSlice::new(&pp_bytes);
        let prefix_proof = ReplayEvidencePrefixCompletenessProofV1::decode_from(&mut ps)?;
        ps.eof()?;
        let nc = s.u16()? as usize;
        if nc > 256 {
            return Err(ReplayAuthority1Error::InvalidWire("too many norm entries"));
        }
        let mut normalized_entries = Vec::with_capacity(nc);
        for _ in 0..nc {
            let eb = s.vec_u32(256 * 1024)?;
            normalized_entries.push(NormalizedAcceptedAuthorityEntryCoreV1::decode_canonical(
                &eb,
            )?);
        }
        let view_id = s.b32()?;
        Ok(Self {
            view_version,
            checkpoint,
            cutoff_ledger_height,
            cutoff_event_ordinal,
            prefix_proof,
            normalized_entries,
            view_id,
        })
    }
}

/// Occurrence-bound witness binding all fields strictly to ONE decoded wrapper event (`232` bytes).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct SameWrapperAcceptedRecordOccurrenceWitnessV1 {
    /// Bound replay-wide view ID.
    pub replay_evidence_view_id: [u8; 32],
    /// Event record ID in the replay archive.
    pub replay_evidence_event_id: [u8; 32],
    /// Outer occurrence ID of the single decoded wrapper event.
    pub outer_occurrence_id: [u8; 32],
    /// Consensus-native occurrence ordinal.
    pub occurrence_ordinal: u64,
    /// Frozen `ProfileIdHashV1` lookup ID.
    pub accepted_parent_lookup_id: [u8; 32],
    /// Unique signature-free proposal core ID.
    pub unique_accepted_proposal_core_block_id: [u8; 32],
    /// Consensus evidence digest from this EXACT same wrapper.
    pub consensus_evidence_id: [u8; 32],
    /// Availability assessment digest from this EXACT same wrapper.
    pub availability_assessment_id: [u8; 32],
}

impl SameWrapperAcceptedRecordOccurrenceWitnessV1 {
    /// Fixed canonical big-endian wire length (`232` bytes).
    pub const WIRE_LEN: usize = 32 * 7 + 8;

    /// Encodes into 232 canonical big-endian bytes.
    pub fn encode_canonical(&self) -> [u8; Self::WIRE_LEN] {
        let mut out = Vec::with_capacity(Self::WIRE_LEN);
        out.extend_from_slice(&self.replay_evidence_view_id);
        out.extend_from_slice(&self.replay_evidence_event_id);
        out.extend_from_slice(&self.outer_occurrence_id);
        out.extend_from_slice(&self.occurrence_ordinal.to_be_bytes());
        out.extend_from_slice(&self.accepted_parent_lookup_id);
        out.extend_from_slice(&self.unique_accepted_proposal_core_block_id);
        out.extend_from_slice(&self.consensus_evidence_id);
        out.extend_from_slice(&self.availability_assessment_id);
        out.try_into().expect("232")
    }

    /// Decodes 232 canonical big-endian bytes fail-closed.
    pub fn decode_canonical(raw: &[u8]) -> Result<Self, ReplayAuthority1Error> {
        let mut s = SraSlice::new(raw);
        let replay_evidence_view_id = s.b32()?;
        let replay_evidence_event_id = s.b32()?;
        let outer_occurrence_id = s.b32()?;
        let occurrence_ordinal = s.u64()?;
        let accepted_parent_lookup_id = s.b32()?;
        let unique_accepted_proposal_core_block_id = s.b32()?;
        let consensus_evidence_id = s.b32()?;
        let availability_assessment_id = s.b32()?;
        s.eof()?;
        Ok(Self {
            replay_evidence_view_id,
            replay_evidence_event_id,
            outer_occurrence_id,
            occurrence_ordinal,
            accepted_parent_lookup_id,
            unique_accepted_proposal_core_block_id,
            consensus_evidence_id,
            availability_assessment_id,
        })
    }
}

/// Full-core retention projection proof blocking retained-subset masquerades (`LAYER1_DESIGN_PAPER.md` §4184–4191).
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct ReplayEvidenceRetentionProjectionProofV1 {
    /// Bound replay-wide view ID.
    pub replay_evidence_view_id: [u8; 32],
    /// Recomputed `normalized_authority_entry_id` of the FULL proof-free core.
    pub normalized_authority_entry_id: [u8; 32],
    /// Deterministic retention policy digest.
    pub retention_policy_id: [u8; 32],
    /// Full proof-free normalized entry core (retained subset cannot substitute for this!).
    pub full_entry_core: NormalizedAcceptedAuthorityEntryCoreV1,
    /// Number of retained qualifying credentials (`<= full_entry_core.credential_pubkeys.len()`).
    pub retained_credential_count: u16,
    /// Number of retained dependency-closed evidence items (`<= full_entry_core.evidence_digests.len()`).
    pub retained_evidence_count: u16,
    /// Deterministic retention trace digest.
    pub retention_trace_digest: [u8; 32],
}

impl ReplayEvidenceRetentionProjectionProofV1 {
    /// Computes the deterministic retention trace digest binding full entry ID and retained counts.
    pub fn compute_trace_digest(
        view_id: [u8; 32],
        entry_id: [u8; 32],
        retention_policy_id: [u8; 32],
        retained_cred: u16,
        retained_evid: u16,
    ) -> [u8; 32] {
        let mut h = Sha256::new();
        h.update(SRA_RETENTION_TRACE_PREFIX);
        h.update(view_id);
        h.update(entry_id);
        h.update(retention_policy_id);
        h.update(retained_cred.to_be_bytes());
        h.update(retained_evid.to_be_bytes());
        h.finalize().into()
    }

    /// Encodes canonically.
    pub fn encode_canonical(&self) -> Result<Vec<u8>, ReplayAuthority1Error> {
        let mut out = Vec::new();
        out.extend_from_slice(&self.replay_evidence_view_id);
        out.extend_from_slice(&self.normalized_authority_entry_id);
        out.extend_from_slice(&self.retention_policy_id);
        sra_push_bytes(&mut out, &self.full_entry_core.encode_canonical()?)?;
        out.extend_from_slice(&self.retained_credential_count.to_be_bytes());
        out.extend_from_slice(&self.retained_evidence_count.to_be_bytes());
        out.extend_from_slice(&self.retention_trace_digest);
        Ok(out)
    }

    fn decode_from(s: &mut SraSlice<'_>) -> Result<Self, ReplayAuthority1Error> {
        let replay_evidence_view_id = s.b32()?;
        let normalized_authority_entry_id = s.b32()?;
        let retention_policy_id = s.b32()?;
        let fb = s.vec_u32(256 * 1024)?;
        let full_entry_core = NormalizedAcceptedAuthorityEntryCoreV1::decode_canonical(&fb)?;
        let retained_credential_count = s.u16()?;
        let retained_evidence_count = s.u16()?;
        let retention_trace_digest = s.b32()?;
        Ok(Self {
            replay_evidence_view_id,
            normalized_authority_entry_id,
            retention_policy_id,
            full_entry_core,
            retained_credential_count,
            retained_evidence_count,
            retention_trace_digest,
        })
    }
}

/// Commitment proof bridging `CONSENSUS-1`'s live certificate to reconstructed replay archive
/// wrapper occurrences (`LAYER1_DESIGN_PAPER.md` §4146–4157, `161` bytes).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ConsensusToReplayParentAuthorityCommitmentProofV1 {
    /// Exact admission ID of the `CONSENSUS-1` certificate.
    pub certificate_admission_id: [u8; 32],
    /// Certificate's `evidence_commitment_id`.
    pub evidence_commitment_id: [u8; 32],
    /// Certificate's `occurrence_universe_root`.
    pub occurrence_universe_root: [u8; 32],
    /// Replay archive checkpoint ID from which occurrences were reconstructed.
    pub replay_checkpoint_id: [u8; 32],
    /// Reconstructed occurrence mapping digest.
    pub reconstruction_map_digest: [u8; 32],
    /// Recomputed parent-authority status matching the accepted certificate.
    pub recomputed_status: ParentAuthorityStatusV1,
}

impl ConsensusToReplayParentAuthorityCommitmentProofV1 {
    /// Fixed canonical big-endian wire length (`161` bytes).
    pub const WIRE_LEN: usize = 32 * 5 + 1;

    /// Computes `reconstruction_map_digest` over reconstructed occurrence receipts and checkpoint ID.
    pub fn compute_map_digest(
        checkpoint_id: [u8; 32],
        receipts: &[ConsensusNativeAcceptedRecordOccurrenceReceiptV1],
    ) -> [u8; 32] {
        let mut h = Sha256::new();
        h.update(SRA_COMMIT_MAP_PREFIX);
        h.update(checkpoint_id);
        h.update((receipts.len() as u32).to_be_bytes());
        for r in receipts {
            h.update(r.encode_canonical());
        }
        h.finalize().into()
    }

    /// Encodes into 161 canonical big-endian bytes.
    pub fn encode_canonical(&self) -> [u8; Self::WIRE_LEN] {
        let mut out = Vec::with_capacity(Self::WIRE_LEN);
        out.extend_from_slice(&self.certificate_admission_id);
        out.extend_from_slice(&self.evidence_commitment_id);
        out.extend_from_slice(&self.occurrence_universe_root);
        out.extend_from_slice(&self.replay_checkpoint_id);
        out.extend_from_slice(&self.reconstruction_map_digest);
        out.push(self.recomputed_status.code());
        out.try_into().expect("161")
    }

    /// Decodes 161 canonical big-endian bytes fail-closed.
    pub fn decode_canonical(raw: &[u8]) -> Result<Self, ReplayAuthority1Error> {
        let mut s = SraSlice::new(raw);
        let certificate_admission_id = s.b32()?;
        let evidence_commitment_id = s.b32()?;
        let occurrence_universe_root = s.b32()?;
        let replay_checkpoint_id = s.b32()?;
        let reconstruction_map_digest = s.b32()?;
        let recomputed_status = ParentAuthorityStatusV1::from_code(s.u8()?)?;
        s.eof()?;
        Ok(Self {
            certificate_admission_id,
            evidence_commitment_id,
            occurrence_universe_root,
            replay_checkpoint_id,
            reconstruction_map_digest,
            recomputed_status,
        })
    }
}

/// Consumer-bound historical parent-authority use witness (`LAYER1_DESIGN_PAPER.md` §4204–4220).
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct HistoricalAcceptedParentAuthorityUseWitnessV1 {
    /// Bound replay-wide view ID.
    pub replay_evidence_view_id: [u8; 32],
    /// Ordinal of the `AcceptedParentAuthorityUseOccurrence` event in the replay archive.
    pub consumer_event_ordinal: u64,
    /// Exact accepted `CONSENSUS-1` certificate.
    pub certificate: AcceptedConsensusNativeParentAuthorityCertificateV1,
    /// Exact `cert_occurrence_id` inside the archive event.
    pub cert_occurrence_id: [u8; 32],
    /// Commitment proof linking `certificate` to reconstructed replay wrapper occurrences.
    pub commitment_proof: ConsensusToReplayParentAuthorityCommitmentProofV1,
    /// Exact canonical consumer preimage bytes bound into the archive event.
    pub consumer_preimage_bytes: Vec<u8>,
}

impl HistoricalAcceptedParentAuthorityUseWitnessV1 {
    /// Encodes canonically.
    pub fn encode_canonical(&self) -> Result<Vec<u8>, ReplayAuthority1Error> {
        let mut out = Vec::new();
        out.extend_from_slice(&self.replay_evidence_view_id);
        out.extend_from_slice(&self.consumer_event_ordinal.to_be_bytes());
        sra_push_bytes(&mut out, &self.certificate.encode_canonical()?)?;
        out.extend_from_slice(&self.cert_occurrence_id);
        out.extend_from_slice(&self.commitment_proof.encode_canonical());
        sra_push_bytes(&mut out, &self.consumer_preimage_bytes)?;
        Ok(out)
    }

    fn decode_from(s: &mut SraSlice<'_>) -> Result<Self, ReplayAuthority1Error> {
        let replay_evidence_view_id = s.b32()?;
        let consumer_event_ordinal = s.u64()?;
        let cb = s.vec_u32(256 * 1024)?;
        let certificate =
            AcceptedConsensusNativeParentAuthorityCertificateV1::decode_canonical(&cb)?;
        let cert_occurrence_id = s.b32()?;
        let commitment_proof = ConsensusToReplayParentAuthorityCommitmentProofV1::decode_canonical(
            s.grab(ConsensusToReplayParentAuthorityCommitmentProofV1::WIRE_LEN)?,
        )?;
        let consumer_preimage_bytes = s.vec_u32(64 * 1024)?;
        Ok(Self {
            replay_evidence_view_id,
            consumer_event_ordinal,
            certificate,
            cert_occurrence_id,
            commitment_proof,
            consumer_preimage_bytes,
        })
    }
}

/// Separate later view-bound status projection entry (`LAYER1_DESIGN_PAPER.md` §4159–4167, `162` bytes).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ReplayParentAuthorityStatusProjectionV1 {
    /// Bound replay-wide view ID.
    pub replay_evidence_view_id: [u8; 32],
    /// Underlying proof-free status projection core.
    pub core: ReplayParentAuthorityStatusProjectionCoreV1,
}

impl ReplayParentAuthorityStatusProjectionV1 {
    /// Fixed canonical big-endian wire length (`162` bytes).
    pub const WIRE_LEN: usize = 32 + ReplayParentAuthorityStatusProjectionCoreV1::WIRE_LEN;

    /// Encodes into 162 canonical big-endian bytes.
    pub fn encode_canonical(&self) -> [u8; Self::WIRE_LEN] {
        let mut out = Vec::with_capacity(Self::WIRE_LEN);
        out.extend_from_slice(&self.replay_evidence_view_id);
        out.extend_from_slice(&self.core.encode_canonical());
        out.try_into().expect("162")
    }

    /// Decodes 162 canonical big-endian bytes fail-closed.
    pub fn decode_canonical(raw: &[u8]) -> Result<Self, ReplayAuthority1Error> {
        let mut s = SraSlice::new(raw);
        let replay_evidence_view_id = s.b32()?;
        let core = ReplayParentAuthorityStatusProjectionCoreV1::decode_canonical(
            s.grab(ReplayParentAuthorityStatusProjectionCoreV1::WIRE_LEN)?,
        )?;
        s.eof()?;
        Ok(Self {
            replay_evidence_view_id,
            core,
        })
    }
}

/// Complete `SYNC-1@replay-authority` audit bundle (`bitblock/sync-1/replay-authority-audit/v1`).
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct ReplayAuthorityAuditBundleV1 {
    /// Bundle schema version (`1`).
    pub bundle_version: u16,
    /// Single replay-wide view shared by every witness and projection in this audit package.
    pub replay_view: ReplayEvidenceViewV1,
    /// Occurrence-bound same-wrapper witnesses.
    pub same_wrapper_witnesses: Vec<SameWrapperAcceptedRecordOccurrenceWitnessV1>,
    /// Full-core retention projection proofs.
    pub retention_proofs: Vec<ReplayEvidenceRetentionProjectionProofV1>,
    /// Consumer-bound historical parent-authority use witnesses (`HistoricalAcceptedParentAuthorityUseWitnessSetV1`).
    pub historical_use_witnesses: Vec<HistoricalAcceptedParentAuthorityUseWitnessV1>,
    /// Separate later status projections (`ReplayParentAuthorityStatusProjectionSetV1`).
    pub status_projections: Vec<ReplayParentAuthorityStatusProjectionV1>,
    /// Optional append-only consistency proof linking historical checkpoint `C1` to `replay_view.checkpoint`.
    pub consistency_proof: Option<ReplayEvidenceCheckpointConsistencyProofV1>,
}

impl ReplayAuthorityAuditBundleV1 {
    /// Compile-time non-authority invariant: replay reconstruction audits already-accepted
    /// `CONSENSUS-1` authority and NEVER selects live consensus authority.
    pub const fn selects_live_consensus_authority(&self) -> bool {
        false
    }

    /// Independently verifies the entire replay-authority audit bundle across all 12/12
    /// `CAPABILITY_TREE.md` negative controls and accepts the required post-consumer checkpoint
    /// positive control (`checkpoint_ledger_height > consumer_ledger_height`).
    #[allow(clippy::too_many_arguments)]
    pub fn verify_audit(
        &self,
        fork: &ExecutorForkContextV1,
        schedule: &EpochValidatorScheduleV1,
        expected_archive_policy_id: [u8; 32],
        expected_norm_policy_id: [u8; 32],
        expected_checkpoint_auth_pk: &CanonicalEd25519PublicKeyV1,
        historical_checkpoint: Option<&ReplayEvidenceArchiveCheckpointV1>,
    ) -> Result<(), ReplayAuthority1Error> {
        if self.bundle_version != SYNC_REPLAY_AUTH_VERSION {
            return Err(ReplayAuthority1Error::InvalidWire(
                "bad audit bundle version",
            ));
        }
        let cp = &self.replay_view.checkpoint;
        cp.verify_authorization(
            fork,
            expected_archive_policy_id,
            expected_norm_policy_id,
            expected_checkpoint_auth_pk,
        )?;
        self.replay_view.prefix_proof.verify(cp)?;
        if self.replay_view.cutoff_event_ordinal == 0
            || self.replay_view.cutoff_event_ordinal > cp.last_covered_ordinal
        {
            return Err(ReplayAuthority1Error::PostCutoffInsertionRejected);
        }

        // Verify optional C1 -> C2 append-consistency proof
        if let Some(hist_cp) = historical_checkpoint {
            hist_cp.verify_authorization(
                fork,
                expected_archive_policy_id,
                expected_norm_policy_id,
                expected_checkpoint_auth_pk,
            )?;
            let Some(cproof) = &self.consistency_proof else {
                return Err(ReplayAuthority1Error::AppendConsistencyViolation(
                    "missing C1 -> C2 consistency proof",
                ));
            };
            cproof.verify(hist_cp, cp, &self.replay_view.prefix_proof.records)?;
        }

        // Total-decode every wrapper occurrence within 1..=cutoff_event_ordinal and independently
        // rebuild the expected normalized entry cores and occurrence receipts.
        #[allow(clippy::type_complexity)]
        let mut by_lookup: BTreeMap<
            [u8; 32],
            (
                [u8; 32],
                BTreeSet<[u8; 32]>,
                BTreeSet<[u8; 32]>,
                BTreeSet<([u8; 32], u64)>,
                bool,
            ),
        > = BTreeMap::new();
        let mut outer_events_by_id: BTreeMap<
            [u8; 32],
            (
                u64,
                [u8; 32],
                ConsensusNativeAcceptedRecordOccurrenceReceiptV1,
            ),
        > = BTreeMap::new();
        let mut reconstructed_receipts: Vec<ConsensusNativeAcceptedRecordOccurrenceReceiptV1> =
            Vec::new();
        let mut use_events_by_ord: BTreeMap<u64, &ReplayEvidenceEventRecordV1> = BTreeMap::new();

        for rec in &self.replay_view.prefix_proof.records {
            let ev_id = rec.event_id()?;
            match &rec.event {
                ReplayEvidenceEventV1::AcceptedRecordOuterOccurrence {
                    occurrence_ordinal,
                    event_ledger_height,
                    raw_wrapper_bytes,
                    availability_assessment_id,
                    has_conflict_material,
                    outer_occurrence_id,
                } => {
                    if rec.event_ordinal > self.replay_view.cutoff_event_ordinal {
                        continue;
                    }
                    if *event_ledger_height > self.replay_view.cutoff_ledger_height {
                        return Err(ReplayAuthority1Error::PostCutoffInsertionRejected);
                    }
                    let wrapper = AcceptedProposalWrapperV1::decode_canonical(raw_wrapper_bytes)?;
                    let core_id = wrapper.verify_credentials(
                        fork.chain_id,
                        fork.genesis_id,
                        fork.genesis_id,
                        fork.profile_id,
                    )?;
                    let rcpt = ConsensusNativeAcceptedRecordOccurrenceReceiptV1::from_wrapper(
                        *occurrence_ordinal,
                        &wrapper,
                        *availability_assessment_id,
                        *has_conflict_material,
                    )?;
                    outer_events_by_id
                        .insert(*outer_occurrence_id, (rec.event_ordinal, ev_id, rcpt));
                    reconstructed_receipts.push(rcpt);

                    let entry = by_lookup
                        .entry(wrapper.core.accepted_parent_lookup_id)
                        .or_insert_with(|| {
                            (
                                core_id,
                                BTreeSet::new(),
                                BTreeSet::new(),
                                BTreeSet::new(),
                                false,
                            )
                        });
                    if entry.0 != core_id {
                        return Err(ReplayAuthority1Error::CircularOrSubstitutedNormalizedEntry(
                            "conflicting proposal_core_id under one lookup_id",
                        ));
                    }
                    for cr in &wrapper.credentials {
                        entry.1.insert(*cr.proposer_pubkey.as_bytes());
                    }
                    for ev in &wrapper.evidence_digests {
                        entry.2.insert(*ev);
                    }
                    for dep in &wrapper.availability_dependencies {
                        entry.3.insert((dep.dependency_id, dep.expected_bytes));
                    }
                    if *has_conflict_material {
                        entry.4 = true;
                    }
                }
                ReplayEvidenceEventV1::AcceptedParentAuthorityUseOccurrence { .. } => {
                    use_events_by_ord.insert(rec.event_ordinal, rec);
                }
            }
        }

        // Verify normalized_entries match the independently reconstructed pre-cutoff wrapper set!
        if self.replay_view.normalized_entries.len() != by_lookup.len() {
            return Err(ReplayAuthority1Error::CircularOrSubstitutedNormalizedEntry(
                "normalized entry count disagrees with pre-cutoff wrappers",
            ));
        }
        let mut entry_by_id: BTreeMap<[u8; 32], &NormalizedAcceptedAuthorityEntryCoreV1> =
            BTreeMap::new();
        for norm in &self.replay_view.normalized_entries {
            let Some((exp_core, exp_creds, exp_evs, exp_deps, exp_conflict)) =
                by_lookup.get(&norm.accepted_parent_lookup_id)
            else {
                return Err(ReplayAuthority1Error::CircularOrSubstitutedNormalizedEntry(
                    "normalized entry lookup_id absent from pre-cutoff archive",
                ));
            };
            if *exp_conflict && !norm.includes_conflict_material {
                return Err(ReplayAuthority1Error::OmittedPreCutoffConflict);
            }
            if norm.unique_accepted_proposal_core_block_id != *exp_core {
                return Err(ReplayAuthority1Error::CircularOrSubstitutedNormalizedEntry(
                    "normalized entry proposal_core_id substituted",
                ));
            }
            let norm_creds: BTreeSet<_> = norm.credential_pubkeys.iter().copied().collect();
            let norm_evs: BTreeSet<_> = norm.evidence_digests.iter().copied().collect();
            let exp_dep_dig = compute_dependency_closure_digest(exp_deps);
            if norm_creds != *exp_creds
                || norm_evs != *exp_evs
                || norm.dependency_closure_digest != exp_dep_dig
            {
                return Err(ReplayAuthority1Error::CircularOrSubstitutedNormalizedEntry(
                    "normalized entry credentials, evidence, or dependency closure substituted",
                ));
            }
            entry_by_id.insert(norm.entry_id()?, norm);
        }

        let recomputed_view_root =
            compute_normalized_authority_view_root(&self.replay_view.normalized_entries)?;
        if recomputed_view_root != cp.normalized_authority_view_root {
            return Err(ReplayAuthority1Error::MixedRootPolicyOrViewRejected(
                "normalized_authority_view_root disagrees with checkpoint",
            ));
        }
        let recomputed_view_id = self.replay_view.recompute_view_id()?;
        if recomputed_view_id != self.replay_view.view_id {
            return Err(ReplayAuthority1Error::MixedRootPolicyOrViewRejected(
                "replay_evidence_view_id mismatch",
            ));
        }

        // Verify same-wrapper occurrence witnesses (rejects synthetic cross-wrapper pairs!)
        if self.same_wrapper_witnesses.is_empty() {
            return Err(ReplayAuthority1Error::CertificateToReplayMismatch(
                "missing same-wrapper occurrence witnesses",
            ));
        }
        for w in &self.same_wrapper_witnesses {
            if w.replay_evidence_view_id != recomputed_view_id {
                return Err(ReplayAuthority1Error::MixedRootPolicyOrViewRejected(
                    "same-wrapper witness bound to wrong view_id",
                ));
            }
            let Some((_, ev_id, rcpt)) = outer_events_by_id.get(&w.outer_occurrence_id) else {
                return Err(ReplayAuthority1Error::SyntheticCrossWrapperPairRejected);
            };
            if w.replay_evidence_event_id != *ev_id
                || w.occurrence_ordinal != rcpt.occurrence_ordinal
                || w.accepted_parent_lookup_id != rcpt.accepted_parent_lookup_id
                || w.unique_accepted_proposal_core_block_id != rcpt.proposal_core_id
                || w.consensus_evidence_id != rcpt.consensus_evidence_id
                || w.availability_assessment_id != rcpt.availability_assessment_id
            {
                return Err(ReplayAuthority1Error::SyntheticCrossWrapperPairRejected);
            }
        }

        // Verify full-core retention projection proofs (rejects retained-subset masquerade!)
        if self.retention_proofs.is_empty() {
            return Err(ReplayAuthority1Error::RetainedSubsetMasqueradeRejected);
        }
        for rp in &self.retention_proofs {
            if rp.replay_evidence_view_id != recomputed_view_id {
                return Err(ReplayAuthority1Error::MixedRootPolicyOrViewRejected(
                    "retention proof bound to wrong view_id",
                ));
            }
            let full_id = rp.full_entry_core.entry_id()?;
            if full_id != rp.normalized_authority_entry_id || !entry_by_id.contains_key(&full_id) {
                return Err(ReplayAuthority1Error::RetainedSubsetMasqueradeRejected);
            }
            if usize::from(rp.retained_credential_count)
                > rp.full_entry_core.credential_pubkeys.len()
                || usize::from(rp.retained_evidence_count)
                    > rp.full_entry_core.evidence_digests.len()
            {
                return Err(ReplayAuthority1Error::RetainedSubsetMasqueradeRejected);
            }
            let exp_trace = ReplayEvidenceRetentionProjectionProofV1::compute_trace_digest(
                rp.replay_evidence_view_id,
                rp.normalized_authority_entry_id,
                rp.retention_policy_id,
                rp.retained_credential_count,
                rp.retained_evidence_count,
            );
            if exp_trace != rp.retention_trace_digest {
                return Err(ReplayAuthority1Error::RetainedSubsetMasqueradeRejected);
            }
        }

        // Verify consumer-bound historical parent-authority use witnesses against CONSENSUS-1 cert
        // AND reconstructed archive wrapper receipts!
        if self.historical_use_witnesses.is_empty() {
            return Err(ReplayAuthority1Error::MissingPurposeOrConsumerBinding(
                "missing historical parent-authority use witness",
            ));
        }
        reconstructed_receipts.sort_by_key(|r| r.occurrence_ordinal);
        let reconstructed_univ_root = compute_occurrence_universe_root(&reconstructed_receipts);
        let expected_map_digest =
            ConsensusToReplayParentAuthorityCommitmentProofV1::compute_map_digest(
                cp.checkpoint_id(),
                &reconstructed_receipts,
            );

        for hw in &self.historical_use_witnesses {
            if hw.replay_evidence_view_id != recomputed_view_id {
                return Err(ReplayAuthority1Error::MixedRootPolicyOrViewRejected(
                    "historical use witness bound to wrong view_id",
                ));
            }
            if hw.consumer_preimage_bytes.is_empty() {
                return Err(ReplayAuthority1Error::MissingPurposeOrConsumerBinding(
                    "empty consumer_preimage_bytes",
                ));
            }
            // Reject non-admitted, tentative, replay-sourced, or late CONSENSUS-1 certificate!
            hw.certificate
                .verify_live_admission(fork, schedule)
                .map_err(|_| {
                    ReplayAuthority1Error::NonAdmittedOrLateCertificateRejected(
                        "certificate failed CONSENSUS-1 live admission verification",
                    )
                })?;

            // Locate matching consumer event in archive and verify exact byte equality
            let Some(ev_rec) = use_events_by_ord.get(&hw.consumer_event_ordinal) else {
                return Err(ReplayAuthority1Error::MissingPurposeOrConsumerBinding(
                    "consumer_event_ordinal absent from replay archive",
                ));
            };
            let ReplayEvidenceEventV1::AcceptedParentAuthorityUseOccurrence {
                purpose,
                consumer_object_digest,
                required_position,
                consumer_ledger_height: _,
                certificate_bytes,
                consumer_preimage_bytes,
                cert_occurrence_id,
            } = &ev_rec.event
            else {
                return Err(ReplayAuthority1Error::MissingPurposeOrConsumerBinding(
                    "referenced ordinal is not a consumer-use event",
                ));
            };
            if *purpose != hw.certificate.consumer_context.purpose
                || *consumer_object_digest != hw.certificate.consumer_context.consumer_object_digest
                || *required_position != hw.certificate.consumer_context.required_position
                || *consumer_preimage_bytes != hw.consumer_preimage_bytes
            {
                return Err(ReplayAuthority1Error::MissingPurposeOrConsumerBinding(
                    "consumer purpose/object/position/preimage mismatch",
                ));
            }
            let cert_enc = hw.certificate.encode_canonical()?;
            if *certificate_bytes != cert_enc || *cert_occurrence_id != hw.cert_occurrence_id {
                return Err(ReplayAuthority1Error::CertificateToReplayMismatch(
                    "archive certificate bytes disagree with witnessed certificate",
                ));
            }
            // Note: checkpoint_ledger_height > consumer_ledger_height is EXPLICITLY ALLOWED
            // (required positive control: archive checkpoint created after consumer is valid!).
            let adm_id = hw.certificate.admission_id()?;
            if hw.commitment_proof.certificate_admission_id != adm_id
                || hw.commitment_proof.evidence_commitment_id
                    != hw.certificate.evidence_commitment_id
                || hw.commitment_proof.occurrence_universe_root
                    != hw.certificate.occurrence_universe_root
                || hw.certificate.occurrence_universe_root != reconstructed_univ_root
                || hw.commitment_proof.replay_checkpoint_id != cp.checkpoint_id()
                || hw.commitment_proof.reconstruction_map_digest != expected_map_digest
                || hw.commitment_proof.recomputed_status != hw.certificate.attained_status
            {
                return Err(ReplayAuthority1Error::CertificateToReplayMismatch(
                    "CONSENSUS-1 certificate commitment disagrees with replay archive reconstruction",
                ));
            }
        }

        // Verify separate later status projections bound to the single replay-wide view
        for sp in &self.status_projections {
            if sp.replay_evidence_view_id != recomputed_view_id {
                return Err(ReplayAuthority1Error::MixedRootPolicyOrViewRejected(
                    "status projection bound to wrong view_id",
                ));
            }
            if !by_lookup.contains_key(&sp.core.accepted_parent_lookup_id) {
                return Err(ReplayAuthority1Error::CircularOrSubstitutedNormalizedEntry(
                    "status projection references unknown lookup_id",
                ));
            }
        }

        Ok(())
    }

    /// Encodes this complete audit bundle into canonical big-endian wire bytes.
    pub fn encode_canonical(&self) -> Result<Vec<u8>, ReplayAuthority1Error> {
        if self.bundle_version != SYNC_REPLAY_AUTH_VERSION {
            return Err(ReplayAuthority1Error::InvalidWire(
                "bad audit bundle version",
            ));
        }
        let mut out = Vec::new();
        out.extend_from_slice(&self.bundle_version.to_be_bytes());
        sra_push_bytes(&mut out, &self.replay_view.encode_canonical()?)?;
        out.extend_from_slice(&(self.same_wrapper_witnesses.len() as u16).to_be_bytes());
        for w in &self.same_wrapper_witnesses {
            out.extend_from_slice(&w.encode_canonical());
        }
        out.extend_from_slice(&(self.retention_proofs.len() as u16).to_be_bytes());
        for rp in &self.retention_proofs {
            sra_push_bytes(&mut out, &rp.encode_canonical()?)?;
        }
        out.extend_from_slice(&(self.historical_use_witnesses.len() as u16).to_be_bytes());
        for hw in &self.historical_use_witnesses {
            sra_push_bytes(&mut out, &hw.encode_canonical()?)?;
        }
        out.extend_from_slice(&(self.status_projections.len() as u16).to_be_bytes());
        for sp in &self.status_projections {
            out.extend_from_slice(&sp.encode_canonical());
        }
        match &self.consistency_proof {
            None => out.push(0),
            Some(cp) => {
                out.push(1);
                out.extend_from_slice(&cp.encode_canonical());
            }
        }
        Ok(out)
    }

    /// Decodes a canonical [`ReplayAuthorityAuditBundleV1`] fail-closed.
    pub fn decode_canonical(raw: &[u8]) -> Result<Self, ReplayAuthority1Error> {
        let mut s = SraSlice::new(raw);
        let bundle_version = s.u16()?;
        if bundle_version != SYNC_REPLAY_AUTH_VERSION {
            return Err(ReplayAuthority1Error::InvalidWire(
                "bad audit bundle version",
            ));
        }
        let vb = s.vec_u32(4 * 1024 * 1024)?;
        let mut vs = SraSlice::new(&vb);
        let replay_view = ReplayEvidenceViewV1::decode_from(&mut vs)?;
        vs.eof()?;
        let wc = s.u16()? as usize;
        if wc > 256 {
            return Err(ReplayAuthority1Error::InvalidWire(
                "too many same-wrapper witnesses",
            ));
        }
        let mut same_wrapper_witnesses = Vec::with_capacity(wc);
        for _ in 0..wc {
            same_wrapper_witnesses.push(
                SameWrapperAcceptedRecordOccurrenceWitnessV1::decode_canonical(
                    s.grab(SameWrapperAcceptedRecordOccurrenceWitnessV1::WIRE_LEN)?,
                )?,
            );
        }
        let rpc = s.u16()? as usize;
        if rpc > 256 {
            return Err(ReplayAuthority1Error::InvalidWire(
                "too many retention proofs",
            ));
        }
        let mut retention_proofs = Vec::with_capacity(rpc);
        for _ in 0..rpc {
            let b = s.vec_u32(256 * 1024)?;
            let mut bs = SraSlice::new(&b);
            retention_proofs.push(ReplayEvidenceRetentionProjectionProofV1::decode_from(
                &mut bs,
            )?);
            bs.eof()?;
        }
        let hwc = s.u16()? as usize;
        if hwc > 256 {
            return Err(ReplayAuthority1Error::InvalidWire(
                "too many historical witnesses",
            ));
        }
        let mut historical_use_witnesses = Vec::with_capacity(hwc);
        for _ in 0..hwc {
            let b = s.vec_u32(512 * 1024)?;
            let mut bs = SraSlice::new(&b);
            historical_use_witnesses.push(
                HistoricalAcceptedParentAuthorityUseWitnessV1::decode_from(&mut bs)?,
            );
            bs.eof()?;
        }
        let spc = s.u16()? as usize;
        if spc > 256 {
            return Err(ReplayAuthority1Error::InvalidWire(
                "too many status projections",
            ));
        }
        let mut status_projections = Vec::with_capacity(spc);
        for _ in 0..spc {
            status_projections.push(ReplayParentAuthorityStatusProjectionV1::decode_canonical(
                s.grab(ReplayParentAuthorityStatusProjectionV1::WIRE_LEN)?,
            )?);
        }
        let consistency_proof = if s.flag()? {
            Some(
                ReplayEvidenceCheckpointConsistencyProofV1::decode_canonical(
                    s.grab(ReplayEvidenceCheckpointConsistencyProofV1::WIRE_LEN)?,
                )?,
            )
        } else {
            None
        };
        s.eof()?;
        Ok(Self {
            bundle_version,
            replay_view,
            same_wrapper_witnesses,
            retention_proofs,
            historical_use_witnesses,
            status_projections,
            consistency_proof,
        })
    }

    /// Computes the canonical `Receipt` `ContentTag` (`bitblock/sync-1/replay-authority-audit/v1`).
    pub fn tag(&self) -> Result<ContentTag, ReplayAuthority1Error> {
        let raw = self.encode_canonical()?;
        hash_canonical(
            TagKind::Receipt,
            SYNC_REPLAY_AUDIT_BUNDLE_SCHEMA,
            raw.len() as u64,
            |w| w(&raw),
        )
        .map_err(|_| ReplayAuthority1Error::InvalidWire("audit bundle hash_canonical failed"))
    }
}

/// Computes the canonical digest of the sorted unique availability dependency closure.
pub fn compute_dependency_closure_digest(deps: &BTreeSet<([u8; 32], u64)>) -> [u8; 32] {
    let mut h = Sha256::new();
    h.update(SRA_DEP_CLOSURE_PREFIX);
    h.update((deps.len() as u32).to_be_bytes());
    for (did, eb) in deps {
        h.update(did);
        h.update(eb.to_be_bytes());
    }
    h.finalize().into()
}

/// Complete canonical fixture bundle constructed from `CONSENSUS-1` for audit & positive/negative tests.
#[derive(Clone, Debug)]
pub struct CanonicalReplayAuthorityFixtureV1 {
    /// Governing fork context.
    pub fork_context: ExecutorForkContextV1,
    /// Active epoch 0 validator schedule.
    pub schedule: EpochValidatorScheduleV1,
    /// Pre-existing archive policy ID.
    pub archive_policy_id: [u8; 32],
    /// Pre-existing normalization policy ID.
    pub norm_policy_id: [u8; 32],
    /// Pre-existing checkpoint authority Ed25519 public key.
    pub checkpoint_auth_pk: CanonicalEd25519PublicKeyV1,
    /// Earlier historical checkpoint `C1` (ledger height `5`, strictly AFTER consumer at height `1`!).
    pub checkpoint_c1: ReplayEvidenceArchiveCheckpointV1,
    /// Verified audit bundle at checkpoint `C1` (required post-consumer positive control).
    pub audit_bundle_c1: ReplayAuthorityAuditBundleV1,
    /// Extended checkpoint `C2` (ledger height `8`, append-consistent extension of `C1`).
    pub checkpoint_c2: ReplayEvidenceArchiveCheckpointV1,
    /// Verified audit bundle at checkpoint `C2` carrying `ReplayEvidenceCheckpointConsistencyProofV1`.
    pub audit_bundle_c2: ReplayAuthorityAuditBundleV1,
}

/// Builds the canonical `SYNC-1@replay-authority` fixture directly from a live `CONSENSUS-1`
/// cluster execution, creating authorized post-consumer checkpoints `C1` (height 5 > consumer height 1)
/// and append-consistent `C2` (height 8).
pub fn build_canonical_replay_authority_fixture()
-> Result<CanonicalReplayAuthorityFixtureV1, ReplayAuthority1Error> {
    let mut cluster = ConsensusClusterHarnessV1::bootstrap_four_validators()?;
    let fork = cluster.fixture.fork_context;
    let schedule = cluster
        .schedule_state
        .schedule_at(0)
        .ok_or(ReplayAuthority1Error::InvalidWire("missing ep0 schedule"))?
        .clone();
    let pre_wr = cluster.nodes[0]
        .harness()
        .snapshot()
        .map_err(Consensus1Error::from)?
        .world_root;
    let b1 = cluster
        .fixture
        .canonical_mixed_race_bundle(1, pre_wr)
        .map_err(Consensus1Error::from)?;
    let commit_cert = cluster.run_round_with_subset(&b1, 0, &[0, 1, 2, 3])?;
    let (pa_cert, wraps) = cluster.build_and_admit_canonical_live_parent_authority_cert(
        &commit_cert,
        ParentAuthorityPurposeV1::ReleaseAuthority,
        1,
        2,
    )?;

    let archive_id = [0x11; 32];
    let archive_policy_id = [0x22; 32];
    let norm_policy_id = [0x33; 32];
    let retention_policy_id = [0x44; 32];
    let elig_policy_id = [0x55; 32];
    let req_pred_id = [0x66; 32];
    let auth_pk =
        CanonicalEd25519PublicKeyV1::from_secret_seed(&DEFAULT_ARCHIVE_CHECKPOINT_AUTH_SEED)
            .map_err(|_| ReplayAuthority1Error::InvalidCheckpointAuthorization("auth pk"))?;

    // Build contiguous chained records 1..=3:
    //   1: wrapper 1 occurrence (height 1)
    //   2: wrapper 2 occurrence (height 1)
    //   3: consumer use event referencing live pa_cert (consumer_ledger_height = 1)
    let ev1 = ReplayEvidenceEventV1::outer_occurrence(1, 1, &wraps[0], [0xC1; 32], false)?;
    let rec1 = ReplayEvidenceEventRecordV1 {
        schema_version: SYNC_REPLAY_AUTH_VERSION,
        archive_id,
        event_ordinal: 1,
        prev_event_id: archive_id,
        event: ev1,
    };
    let ev1_id = rec1.event_id()?;

    let ev2 = ReplayEvidenceEventV1::outer_occurrence(2, 1, &wraps[1], [0xC2; 32], false)?;
    let rec2 = ReplayEvidenceEventRecordV1 {
        schema_version: SYNC_REPLAY_AUTH_VERSION,
        archive_id,
        event_ordinal: 2,
        prev_event_id: ev1_id,
        event: ev2,
    };
    let ev2_id = rec2.event_id()?;

    let consumer_preimage = b"bb.release.consumer.preimage.v1".to_vec();
    let ev3 = ReplayEvidenceEventV1::parent_authority_use(&pa_cert, 1, consumer_preimage.clone())?;
    let cert_occ_id = match &ev3 {
        ReplayEvidenceEventV1::AcceptedParentAuthorityUseOccurrence {
            cert_occurrence_id, ..
        } => *cert_occurrence_id,
        _ => unreachable!(),
    };
    let rec3 = ReplayEvidenceEventRecordV1 {
        schema_version: SYNC_REPLAY_AUTH_VERSION,
        archive_id,
        event_ordinal: 3,
        prev_event_id: ev2_id,
        event: ev3,
    };
    let ev3_id = rec3.event_id()?;
    let recs_c1 = vec![rec1.clone(), rec2.clone(), rec3.clone()];

    // Reconstruct proof-free NormalizedAcceptedAuthorityEntryCoreV1
    let mut cred_pks = BTreeSet::new();
    let mut ev_digs = BTreeSet::new();
    let mut deps = BTreeSet::new();
    for w in &wraps {
        for cr in &w.credentials {
            cred_pks.insert(*cr.proposer_pubkey.as_bytes());
        }
        for e in &w.evidence_digests {
            ev_digs.insert(*e);
        }
        for d in &w.availability_dependencies {
            deps.insert((d.dependency_id, d.expected_bytes));
        }
    }
    let proj_core = ReplayParentAuthorityStatusProjectionCoreV1 {
        accepted_parent_lookup_id: pa_cert.accepted_parent_lookup_id,
        unique_accepted_proposal_core_block_id: pa_cert.unique_accepted_proposal_core_block_id,
        purpose: ParentAuthorityPurposeV1::ReleaseAuthority,
        eligibility_policy_id: elig_policy_id,
        required_predicate_id: req_pred_id,
        projected_status: pa_cert.attained_status,
    };
    let norm_entry = NormalizedAcceptedAuthorityEntryCoreV1 {
        accepted_parent_lookup_id: pa_cert.accepted_parent_lookup_id,
        unique_accepted_proposal_core_block_id: pa_cert.unique_accepted_proposal_core_block_id,
        credential_pubkeys: cred_pks.into_iter().collect(),
        evidence_digests: ev_digs.into_iter().collect(),
        dependency_closure_digest: compute_dependency_closure_digest(&deps),
        status_projections: vec![proj_core],
        includes_conflict_material: false,
    };
    let norm_entries = vec![norm_entry.clone()];
    let norm_view_root = compute_normalized_authority_view_root(&norm_entries)?;
    let prefix_root_c1 = compute_ordered_event_prefix_root(archive_id, &recs_c1)?;

    // Required POSITIVE CONTROL: checkpoint C1 sealed at ledger height 5 > consumer height 1!
    let cp_c1 = ReplayEvidenceArchiveCheckpointV1 {
        checkpoint_version: SYNC_REPLAY_AUTH_VERSION,
        chain_id: fork.chain_id,
        genesis_id: fork.genesis_id,
        archive_id,
        archive_policy_id,
        prev_checkpoint_id: archive_id,
        checkpoint_ledger_height: 5,
        first_covered_ordinal: 1,
        last_covered_ordinal: 3,
        covered_event_count: 3,
        ordered_event_prefix_root: prefix_root_c1,
        normalization_policy_id: norm_policy_id,
        normalized_authority_view_root: norm_view_root,
        publication_complete: true,
        checkpoint_authority_pubkey: [0u8; 32],
        checkpoint_signature: [0u8; 64],
    }
    .sign_with_seed(&DEFAULT_ARCHIVE_CHECKPOINT_AUTH_SEED, &fork)?;

    let outer_id_1 = match &rec1.event {
        ReplayEvidenceEventV1::AcceptedRecordOuterOccurrence {
            outer_occurrence_id,
            ..
        } => *outer_occurrence_id,
        _ => unreachable!(),
    };
    let outer_id_2 = match &rec2.event {
        ReplayEvidenceEventV1::AcceptedRecordOuterOccurrence {
            outer_occurrence_id,
            ..
        } => *outer_occurrence_id,
        _ => unreachable!(),
    };

    let mk_bundle = |cp: ReplayEvidenceArchiveCheckpointV1,
                     recs: Vec<ReplayEvidenceEventRecordV1>,
                     consistency: Option<ReplayEvidenceCheckpointConsistencyProofV1>|
     -> Result<ReplayAuthorityAuditBundleV1, ReplayAuthority1Error> {
        let prefix_proof = ReplayEvidencePrefixCompletenessProofV1 {
            checkpoint_id: cp.checkpoint_id(),
            first_ordinal: 1,
            last_ordinal: recs.len() as u64,
            covered_event_count: recs.len() as u64,
            ordered_event_prefix_root: cp.ordered_event_prefix_root,
            records: recs,
        };
        let mut view = ReplayEvidenceViewV1 {
            view_version: SYNC_REPLAY_AUTH_VERSION,
            checkpoint: cp,
            cutoff_ledger_height: 1,
            cutoff_event_ordinal: 3,
            prefix_proof,
            normalized_entries: norm_entries.clone(),
            view_id: [0u8; 32],
        };
        view.view_id = view.recompute_view_id()?;

        let same_wrapper_witnesses = vec![
            SameWrapperAcceptedRecordOccurrenceWitnessV1 {
                replay_evidence_view_id: view.view_id,
                replay_evidence_event_id: ev1_id,
                outer_occurrence_id: outer_id_1,
                occurrence_ordinal: 1,
                accepted_parent_lookup_id: pa_cert.accepted_parent_lookup_id,
                unique_accepted_proposal_core_block_id: pa_cert
                    .unique_accepted_proposal_core_block_id,
                consensus_evidence_id: pa_cert.occurrence_receipts[0].consensus_evidence_id,
                availability_assessment_id: pa_cert.occurrence_receipts[0]
                    .availability_assessment_id,
            },
            SameWrapperAcceptedRecordOccurrenceWitnessV1 {
                replay_evidence_view_id: view.view_id,
                replay_evidence_event_id: ev2_id,
                outer_occurrence_id: outer_id_2,
                occurrence_ordinal: 2,
                accepted_parent_lookup_id: pa_cert.accepted_parent_lookup_id,
                unique_accepted_proposal_core_block_id: pa_cert
                    .unique_accepted_proposal_core_block_id,
                consensus_evidence_id: pa_cert.occurrence_receipts[1].consensus_evidence_id,
                availability_assessment_id: pa_cert.occurrence_receipts[1]
                    .availability_assessment_id,
            },
        ];
        let entry_id = norm_entry.entry_id()?;
        let retention_proof = ReplayEvidenceRetentionProjectionProofV1 {
            replay_evidence_view_id: view.view_id,
            normalized_authority_entry_id: entry_id,
            retention_policy_id,
            full_entry_core: norm_entry.clone(),
            retained_credential_count: 1,
            retained_evidence_count: 1,
            retention_trace_digest: ReplayEvidenceRetentionProjectionProofV1::compute_trace_digest(
                view.view_id,
                entry_id,
                retention_policy_id,
                1,
                1,
            ),
        };
        let commitment_proof = ConsensusToReplayParentAuthorityCommitmentProofV1 {
            certificate_admission_id: pa_cert.admission_id()?,
            evidence_commitment_id: pa_cert.evidence_commitment_id,
            occurrence_universe_root: pa_cert.occurrence_universe_root,
            replay_checkpoint_id: cp.checkpoint_id(),
            reconstruction_map_digest:
                ConsensusToReplayParentAuthorityCommitmentProofV1::compute_map_digest(
                    cp.checkpoint_id(),
                    &pa_cert.occurrence_receipts,
                ),
            recomputed_status: pa_cert.attained_status,
        };
        let hist_witness = HistoricalAcceptedParentAuthorityUseWitnessV1 {
            replay_evidence_view_id: view.view_id,
            consumer_event_ordinal: 3,
            certificate: pa_cert.clone(),
            cert_occurrence_id: cert_occ_id,
            commitment_proof,
            consumer_preimage_bytes: consumer_preimage.clone(),
        };
        let status_proj = ReplayParentAuthorityStatusProjectionV1 {
            replay_evidence_view_id: view.view_id,
            core: proj_core,
        };
        Ok(ReplayAuthorityAuditBundleV1 {
            bundle_version: SYNC_REPLAY_AUTH_VERSION,
            replay_view: view,
            same_wrapper_witnesses,
            retention_proofs: vec![retention_proof],
            historical_use_witnesses: vec![hist_witness],
            status_projections: vec![status_proj],
            consistency_proof: consistency,
        })
    };

    let bundle_c1 = mk_bundle(cp_c1, recs_c1.clone(), None)?;
    bundle_c1.verify_audit(
        &fork,
        &schedule,
        archive_policy_id,
        norm_policy_id,
        &auth_pk,
        None,
    )?;

    // Append post-cutoff event 4 (height 6) and seal checkpoint C2 (height 8) with append-consistency
    let ev4 = ReplayEvidenceEventV1::outer_occurrence(3, 6, &wraps[0], [0xC3; 32], false)?;
    let rec4 = ReplayEvidenceEventRecordV1 {
        schema_version: SYNC_REPLAY_AUTH_VERSION,
        archive_id,
        event_ordinal: 4,
        prev_event_id: ev3_id,
        event: ev4,
    };
    let recs_c2 = vec![rec1, rec2, rec3, rec4];
    let prefix_root_c2 = compute_ordered_event_prefix_root(archive_id, &recs_c2)?;
    let cp_c2 = ReplayEvidenceArchiveCheckpointV1 {
        checkpoint_version: SYNC_REPLAY_AUTH_VERSION,
        chain_id: fork.chain_id,
        genesis_id: fork.genesis_id,
        archive_id,
        archive_policy_id,
        prev_checkpoint_id: cp_c1.checkpoint_id(),
        checkpoint_ledger_height: 8,
        first_covered_ordinal: 1,
        last_covered_ordinal: 4,
        covered_event_count: 4,
        ordered_event_prefix_root: prefix_root_c2,
        normalization_policy_id: norm_policy_id,
        normalized_authority_view_root: norm_view_root,
        publication_complete: true,
        checkpoint_authority_pubkey: [0u8; 32],
        checkpoint_signature: [0u8; 64],
    }
    .sign_with_seed(&DEFAULT_ARCHIVE_CHECKPOINT_AUTH_SEED, &fork)?;

    let consistency = ReplayEvidenceCheckpointConsistencyProofV1::build(&cp_c1, &cp_c2, &recs_c2)?;
    let bundle_c2 = mk_bundle(cp_c2, recs_c2, Some(consistency))?;
    bundle_c2.verify_audit(
        &fork,
        &schedule,
        archive_policy_id,
        norm_policy_id,
        &auth_pk,
        Some(&cp_c1),
    )?;

    Ok(CanonicalReplayAuthorityFixtureV1 {
        fork_context: fork,
        schedule,
        archive_policy_id,
        norm_policy_id,
        checkpoint_auth_pk: auth_pk,
        checkpoint_c1: cp_c1,
        audit_bundle_c1: bundle_c1,
        checkpoint_c2: cp_c2,
        audit_bundle_c2: bundle_c2,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sync_replay_authority_1_post_consumer_checkpoint_and_golden_tags() {
        let fix = build_canonical_replay_authority_fixture().unwrap();
        assert!(!fix.audit_bundle_c1.selects_live_consensus_authority());
        assert!(fix.checkpoint_c1.checkpoint_ledger_height > 1);
        assert!(
            fix.checkpoint_c2.checkpoint_ledger_height > fix.checkpoint_c1.checkpoint_ledger_height
        );

        // Round-trip canonical codec parity
        let cp_rt = ReplayEvidenceArchiveCheckpointV1::decode_canonical(
            &fix.checkpoint_c1.encode_canonical().unwrap(),
        )
        .unwrap();
        assert_eq!(cp_rt, fix.checkpoint_c1);
        let b1_rt = ReplayAuthorityAuditBundleV1::decode_canonical(
            &fix.audit_bundle_c1.encode_canonical().unwrap(),
        )
        .unwrap();
        assert_eq!(b1_rt, fix.audit_bundle_c1);
        let b2_rt = ReplayAuthorityAuditBundleV1::decode_canonical(
            &fix.audit_bundle_c2.encode_canonical().unwrap(),
        )
        .unwrap();
        assert_eq!(b2_rt, fix.audit_bundle_c2);

        let cp1_tag = fix.checkpoint_c1.tag().unwrap().to_string();
        let b1_tag = fix.audit_bundle_c1.tag().unwrap().to_string();
        let b2_tag = fix.audit_bundle_c2.tag().unwrap().to_string();
        assert_eq!(
            cp1_tag,
            "#bb.receipt.sha256.2mdit6c2t47g4aoihd6qnz7r4iqy7qnyxq5zxl7lnbkvxh7jofna"
        );
        assert_eq!(
            b1_tag,
            "#bb.receipt.sha256.k3g23z6c5xob4cyxdictjgeaqhckkvjqfdzahfjqcko6faxfsf5a"
        );
        assert_eq!(
            b2_tag,
            "#bb.receipt.sha256.y22xa32n3vfzindqhtg6sltlso5junn2lg2nzeon7orvqubznooa"
        );
    }
}
