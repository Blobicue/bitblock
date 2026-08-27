//! Canonical Windows composition-timing observations around one checked presentation.
//!
//! This module records a composite frame receipt, checked presenter facts, QPC brackets,
//! synchronization-call results, and complete raw `DWM_TIMING_INFO` snapshots. These facts can
//! establish only session-level timing progress. They do not identify the submitted frame in the
//! compositor, establish scanout, or establish physical-light output.

use std::fmt;

use crate::frame_v2::{
    FRAME_RECEIPT_V2_PAYLOAD_LEN, FrameReceiptV2, FrameReceiptV2Error, FrameStream,
};
use crate::protocol::{ContentTag, TagKind, hash_canonical};

pub const COMPOSITION_TIMING_ENVELOPE_SCHEMA: &[u8] = b"bitblock/composition-timing-envelope/v1";
pub const COMPOSITION_TIMING_ENVELOPE_PAYLOAD_LEN: usize = 948;

const COMPOSITION_TIMING_POLICY_SCHEMA: &[u8] =
    b"bitblock/composition-timing-policy/windows-dwm-null-hwnd/v1";
const COMPOSITION_TIMING_POLICY_PAYLOAD_LEN: u64 = 14;
const PRESENTER_EVIDENCE_LEN: usize = 52;
const QPC_READ_LEN: usize = 13;
const QPC_FREQUENCY_LEN: usize = 13;
const QPC_BRACKET_LEN: usize = 26;
const DWM_TIMING_SNAPSHOT_LEN: usize = 288;
const DWM_TIMING_CALL_LEN: usize = 319;
const GDI_FLUSH_CALL_LEN: usize = 27;
const DWM_FLUSH_CALL_LEN: usize = 30;

/// Content identity for the exact raw observation and conservative interpretation policy.
pub fn composition_timing_policy_tag() -> ContentTag {
    hash_canonical(
        TagKind::Policy,
        COMPOSITION_TIMING_POLICY_SCHEMA,
        COMPOSITION_TIMING_POLICY_PAYLOAD_LEN,
        |sink| {
            sink(&1_u16.to_be_bytes()); // policy revision
            sink(&[0]); // Windows desktop composition session
            sink(&[1]); // DwmGetCompositionTimingInfo receives a null HWND
            sink(&[1]); // every observation is QPC bracketed
            sink(&[1]); // retain checked presenter facts
            sink(&[1]); // retain GdiFlush result
            sink(&[1]); // retain DwmFlush HRESULT
            sink(&[1]); // retain the complete timing structure
            sink(&[1]); // retain before and after timing calls
            sink(&[1]); // progress assessment is derived, not encoded
            sink(&[1]); // exact-frame attribution remains unknown
            sink(&[0]); // no scanout claim
            sink(&[0]); // no physical-light claim
        },
    )
    .expect("composition timing policy encoding has a fixed length")
}

/// Checked scanline-submission facts, independent of the Windows collector type.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PresenterEvidence {
    pub policy: ContentTag,
    pub requested_scanlines: u32,
    pub returned_scanlines: i32,
    pub call_error_code: u32,
    pub release_result: i32,
    pub release_error_code: u32,
}

impl PresenterEvidence {
    pub const fn release_succeeded(self) -> bool {
        self.release_result != 0
    }

    fn validate(self) -> Result<(), PresentationProtocolError> {
        if self.policy.kind() != TagKind::Policy {
            return Err(PresentationProtocolError::WrongTagKind {
                field: "presenter policy",
                expected: TagKind::Policy,
                actual: self.policy.kind(),
            });
        }
        if self.requested_scanlines == 0
            || self.requested_scanlines > i32::MAX as u32
            || self.returned_scanlines != self.requested_scanlines as i32
            || self.call_error_code != 0
        {
            return Err(PresentationProtocolError::IncompletePresenterSubmission {
                requested: self.requested_scanlines,
                returned: self.returned_scanlines,
                error_code: self.call_error_code,
            });
        }
        Ok(())
    }

    fn encode(self, writer: &mut FixedWriter<'_>) {
        writer.digest(self.policy);
        writer.u32(self.requested_scanlines);
        writer.i32(self.returned_scanlines);
        writer.u32(self.call_error_code);
        writer.i32(self.release_result);
        writer.u32(self.release_error_code);
    }

    fn decode(reader: &mut FixedReader<'_>) -> Self {
        Self {
            policy: reader.tag(TagKind::Policy),
            requested_scanlines: reader.u32(),
            returned_scanlines: reader.i32(),
            call_error_code: reader.u32(),
            release_result: reader.i32(),
            release_error_code: reader.u32(),
        }
    }
}

/// One QueryPerformanceCounter read, including failure state.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct QpcRead {
    pub succeeded: bool,
    pub value: i64,
    pub error_code: u32,
}

impl QpcRead {
    fn validate(self, field: &'static str) -> Result<(), PresentationProtocolError> {
        if (self.succeeded && self.error_code != 0) || (!self.succeeded && self.value != 0) {
            return Err(PresentationProtocolError::InvalidQpcRead {
                field,
                succeeded: self.succeeded,
                value: self.value,
                error_code: self.error_code,
            });
        }
        Ok(())
    }

    fn encode(self, writer: &mut FixedWriter<'_>) {
        writer.boolean(self.succeeded);
        writer.i64(self.value);
        writer.u32(self.error_code);
    }

    fn decode(
        reader: &mut FixedReader<'_>,
        field: &'static str,
    ) -> Result<Self, PresentationProtocolError> {
        Ok(Self {
            succeeded: reader.boolean(field)?,
            value: reader.i64(),
            error_code: reader.u32(),
        })
    }
}

/// QueryPerformanceFrequency result for the timing session.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct QpcFrequency {
    pub succeeded: bool,
    pub counts_per_second: i64,
    pub error_code: u32,
}

impl QpcFrequency {
    fn validate(self) -> Result<(), PresentationProtocolError> {
        if (self.succeeded && (self.counts_per_second <= 0 || self.error_code != 0))
            || (!self.succeeded && self.counts_per_second != 0)
        {
            return Err(PresentationProtocolError::InvalidQpcFrequency {
                succeeded: self.succeeded,
                counts_per_second: self.counts_per_second,
                error_code: self.error_code,
            });
        }
        Ok(())
    }

    fn encode(self, writer: &mut FixedWriter<'_>) {
        writer.boolean(self.succeeded);
        writer.i64(self.counts_per_second);
        writer.u32(self.error_code);
    }

    fn decode(reader: &mut FixedReader<'_>) -> Result<Self, PresentationProtocolError> {
        Ok(Self {
            succeeded: reader.boolean("QPC frequency success")?,
            counts_per_second: reader.i64(),
            error_code: reader.u32(),
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct QpcBracket {
    pub before: QpcRead,
    pub after: QpcRead,
}

impl QpcBracket {
    fn validate(self, field: &'static str) -> Result<(), PresentationProtocolError> {
        self.before.validate(field)?;
        self.after.validate(field)?;
        Ok(())
    }

    fn encode(self, writer: &mut FixedWriter<'_>) {
        self.before.encode(writer);
        self.after.encode(writer);
    }

    fn decode(
        reader: &mut FixedReader<'_>,
        field: &'static str,
    ) -> Result<Self, PresentationProtocolError> {
        Ok(Self {
            before: QpcRead::decode(reader, field)?,
            after: QpcRead::decode(reader, field)?,
        })
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DwmRatio {
    pub numerator: u32,
    pub denominator: u32,
}

impl DwmRatio {
    fn encode(self, writer: &mut FixedWriter<'_>) {
        writer.u32(self.numerator);
        writer.u32(self.denominator);
    }

    fn decode(reader: &mut FixedReader<'_>) -> Self {
        Self {
            numerator: reader.u32(),
            denominator: reader.u32(),
        }
    }
}

/// Every field of `DWM_TIMING_INFO` in API order except the structural `cbSize` field.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DwmTimingSnapshot {
    pub rate_refresh: DwmRatio,
    pub qpc_refresh_period: u64,
    pub rate_compose: DwmRatio,
    pub qpc_vblank: u64,
    pub c_refresh: u64,
    pub c_dx_refresh: u32,
    pub qpc_compose: u64,
    pub c_frame: u64,
    pub c_dx_present: u32,
    pub c_refresh_frame: u64,
    pub c_frame_submitted: u64,
    pub c_dx_present_submitted: u32,
    pub c_frame_confirmed: u64,
    pub c_dx_present_confirmed: u32,
    pub c_refresh_confirmed: u64,
    pub c_dx_refresh_confirmed: u32,
    pub c_frames_late: u64,
    pub c_frames_outstanding: u32,
    pub c_frame_displayed: u64,
    pub qpc_frame_displayed: u64,
    pub c_refresh_frame_displayed: u64,
    pub c_frame_complete: u64,
    pub qpc_frame_complete: u64,
    pub c_frame_pending: u64,
    pub qpc_frame_pending: u64,
    pub c_frames_displayed: u64,
    pub c_frames_complete: u64,
    pub c_frames_pending: u64,
    pub c_frames_available: u64,
    pub c_frames_dropped: u64,
    pub c_frames_missed: u64,
    pub c_refresh_next_displayed: u64,
    pub c_refresh_next_presented: u64,
    pub c_refreshes_displayed: u64,
    pub c_refreshes_presented: u64,
    pub c_refresh_started: u64,
    pub c_pixels_received: u64,
    pub c_pixels_drawn: u64,
    pub c_buffers_empty: u64,
}

impl DwmTimingSnapshot {
    fn encode(self, writer: &mut FixedWriter<'_>) {
        let start = writer.cursor;
        self.rate_refresh.encode(writer);
        writer.u64(self.qpc_refresh_period);
        self.rate_compose.encode(writer);
        writer.u64(self.qpc_vblank);
        writer.u64(self.c_refresh);
        writer.u32(self.c_dx_refresh);
        writer.u64(self.qpc_compose);
        writer.u64(self.c_frame);
        writer.u32(self.c_dx_present);
        writer.u64(self.c_refresh_frame);
        writer.u64(self.c_frame_submitted);
        writer.u32(self.c_dx_present_submitted);
        writer.u64(self.c_frame_confirmed);
        writer.u32(self.c_dx_present_confirmed);
        writer.u64(self.c_refresh_confirmed);
        writer.u32(self.c_dx_refresh_confirmed);
        writer.u64(self.c_frames_late);
        writer.u32(self.c_frames_outstanding);
        writer.u64(self.c_frame_displayed);
        writer.u64(self.qpc_frame_displayed);
        writer.u64(self.c_refresh_frame_displayed);
        writer.u64(self.c_frame_complete);
        writer.u64(self.qpc_frame_complete);
        writer.u64(self.c_frame_pending);
        writer.u64(self.qpc_frame_pending);
        writer.u64(self.c_frames_displayed);
        writer.u64(self.c_frames_complete);
        writer.u64(self.c_frames_pending);
        writer.u64(self.c_frames_available);
        writer.u64(self.c_frames_dropped);
        writer.u64(self.c_frames_missed);
        writer.u64(self.c_refresh_next_displayed);
        writer.u64(self.c_refresh_next_presented);
        writer.u64(self.c_refreshes_displayed);
        writer.u64(self.c_refreshes_presented);
        writer.u64(self.c_refresh_started);
        writer.u64(self.c_pixels_received);
        writer.u64(self.c_pixels_drawn);
        writer.u64(self.c_buffers_empty);
        debug_assert_eq!(writer.cursor - start, DWM_TIMING_SNAPSHOT_LEN);
    }

    fn decode(reader: &mut FixedReader<'_>) -> Self {
        let start = reader.cursor;
        let snapshot = Self {
            rate_refresh: DwmRatio::decode(reader),
            qpc_refresh_period: reader.u64(),
            rate_compose: DwmRatio::decode(reader),
            qpc_vblank: reader.u64(),
            c_refresh: reader.u64(),
            c_dx_refresh: reader.u32(),
            qpc_compose: reader.u64(),
            c_frame: reader.u64(),
            c_dx_present: reader.u32(),
            c_refresh_frame: reader.u64(),
            c_frame_submitted: reader.u64(),
            c_dx_present_submitted: reader.u32(),
            c_frame_confirmed: reader.u64(),
            c_dx_present_confirmed: reader.u32(),
            c_refresh_confirmed: reader.u64(),
            c_dx_refresh_confirmed: reader.u32(),
            c_frames_late: reader.u64(),
            c_frames_outstanding: reader.u32(),
            c_frame_displayed: reader.u64(),
            qpc_frame_displayed: reader.u64(),
            c_refresh_frame_displayed: reader.u64(),
            c_frame_complete: reader.u64(),
            qpc_frame_complete: reader.u64(),
            c_frame_pending: reader.u64(),
            qpc_frame_pending: reader.u64(),
            c_frames_displayed: reader.u64(),
            c_frames_complete: reader.u64(),
            c_frames_pending: reader.u64(),
            c_frames_available: reader.u64(),
            c_frames_dropped: reader.u64(),
            c_frames_missed: reader.u64(),
            c_refresh_next_displayed: reader.u64(),
            c_refresh_next_presented: reader.u64(),
            c_refreshes_displayed: reader.u64(),
            c_refreshes_presented: reader.u64(),
            c_refresh_started: reader.u64(),
            c_pixels_received: reader.u64(),
            c_pixels_drawn: reader.u64(),
            c_buffers_empty: reader.u64(),
        };
        debug_assert_eq!(reader.cursor - start, DWM_TIMING_SNAPSHOT_LEN);
        snapshot
    }

    fn comparable_configuration(self, other: Self) -> bool {
        self.rate_refresh == other.rate_refresh
            && self.qpc_refresh_period == other.qpc_refresh_period
            && self.rate_compose == other.rate_compose
    }

    fn identity_regressed(self, later: Self) -> bool {
        later.qpc_vblank < self.qpc_vblank
            || later.c_refresh < self.c_refresh
            || later.c_dx_refresh < self.c_dx_refresh
            || later.qpc_compose < self.qpc_compose
            || later.c_frame < self.c_frame
            || later.c_dx_present < self.c_dx_present
            || later.c_refresh_frame < self.c_refresh_frame
            || later.c_frame_submitted < self.c_frame_submitted
            || later.c_dx_present_submitted < self.c_dx_present_submitted
            || later.c_frame_confirmed < self.c_frame_confirmed
            || later.c_dx_present_confirmed < self.c_dx_present_confirmed
            || later.c_refresh_confirmed < self.c_refresh_confirmed
            || later.c_dx_refresh_confirmed < self.c_dx_refresh_confirmed
            || later.c_frame_displayed < self.c_frame_displayed
            || later.qpc_frame_displayed < self.qpc_frame_displayed
            || later.c_refresh_frame_displayed < self.c_refresh_frame_displayed
            || later.c_frame_complete < self.c_frame_complete
            || later.qpc_frame_complete < self.qpc_frame_complete
            || later.c_frame_pending < self.c_frame_pending
            || later.qpc_frame_pending < self.qpc_frame_pending
            || later.c_refresh_next_displayed < self.c_refresh_next_displayed
            || later.c_refresh_next_presented < self.c_refresh_next_presented
    }

    fn has_progress_from(self, earlier: Self) -> bool {
        self.qpc_vblank > earlier.qpc_vblank
            || self.c_refresh > earlier.c_refresh
            || self.c_dx_refresh > earlier.c_dx_refresh
            || self.qpc_compose > earlier.qpc_compose
            || self.c_frame > earlier.c_frame
            || self.c_dx_present > earlier.c_dx_present
            || self.c_refresh_frame > earlier.c_refresh_frame
            || self.c_frame_submitted > earlier.c_frame_submitted
            || self.c_dx_present_submitted > earlier.c_dx_present_submitted
            || self.c_frame_confirmed > earlier.c_frame_confirmed
            || self.c_dx_present_confirmed > earlier.c_dx_present_confirmed
            || self.c_refresh_confirmed > earlier.c_refresh_confirmed
            || self.c_dx_refresh_confirmed > earlier.c_dx_refresh_confirmed
            || self.c_frame_displayed > earlier.c_frame_displayed
            || self.qpc_frame_displayed > earlier.qpc_frame_displayed
            || self.c_refresh_frame_displayed > earlier.c_refresh_frame_displayed
            || self.c_frame_complete > earlier.c_frame_complete
            || self.qpc_frame_complete > earlier.qpc_frame_complete
            || self.c_frame_pending > earlier.c_frame_pending
            || self.qpc_frame_pending > earlier.qpc_frame_pending
            || self.c_refresh_next_displayed > earlier.c_refresh_next_displayed
            || self.c_refresh_next_presented > earlier.c_refresh_next_presented
            || self.c_frames_displayed != 0
            || self.c_frames_available != 0
            || self.c_frames_dropped != 0
    }
}

/// One null-HWND `DwmGetCompositionTimingInfo` call and its QPC bracket.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DwmTimingCall {
    pub bracket: QpcBracket,
    pub hresult: i32,
    pub snapshot: Option<DwmTimingSnapshot>,
}

impl DwmTimingCall {
    fn validate(self, field: &'static str) -> Result<(), PresentationProtocolError> {
        self.bracket.validate(field)?;
        if (self.hresult == 0) != self.snapshot.is_some() {
            return Err(PresentationProtocolError::DwmSnapshotPresenceMismatch {
                field,
                hresult: self.hresult,
                snapshot_present: self.snapshot.is_some(),
            });
        }
        Ok(())
    }

    fn encode(self, writer: &mut FixedWriter<'_>) {
        self.bracket.before.encode(writer);
        writer.i32(self.hresult);
        writer.boolean(self.snapshot.is_some());
        if let Some(snapshot) = self.snapshot {
            snapshot.encode(writer);
        } else {
            writer.bytes(&[0; DWM_TIMING_SNAPSHOT_LEN]);
        }
        self.bracket.after.encode(writer);
    }

    fn decode(
        reader: &mut FixedReader<'_>,
        field: &'static str,
    ) -> Result<Self, PresentationProtocolError> {
        let before = QpcRead::decode(reader, field)?;
        let hresult = reader.i32();
        let present = reader.boolean("DWM snapshot presence")?;
        let snapshot = if present {
            Some(DwmTimingSnapshot::decode(reader))
        } else {
            let padding = reader.bytes(DWM_TIMING_SNAPSHOT_LEN);
            if padding.iter().any(|byte| *byte != 0) {
                return Err(PresentationProtocolError::NonZeroAbsentDwmSnapshotPadding { field });
            }
            None
        };
        let after = QpcRead::decode(reader, field)?;
        Ok(Self {
            bracket: QpcBracket { before, after },
            hresult,
            snapshot,
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GdiFlushCall {
    pub bracket: QpcBracket,
    pub succeeded: bool,
}

impl GdiFlushCall {
    fn validate(self) -> Result<(), PresentationProtocolError> {
        self.bracket.validate("GdiFlush")
    }

    fn encode(self, writer: &mut FixedWriter<'_>) {
        self.bracket.before.encode(writer);
        writer.boolean(self.succeeded);
        self.bracket.after.encode(writer);
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DwmFlushCall {
    pub bracket: QpcBracket,
    pub hresult: i32,
}

impl DwmFlushCall {
    fn validate(self) -> Result<(), PresentationProtocolError> {
        self.bracket.validate("DwmFlush")
    }

    fn encode(self, writer: &mut FixedWriter<'_>) {
        self.bracket.before.encode(writer);
        writer.i32(self.hresult);
        self.bracket.after.encode(writer);
    }
}

/// A deliberately session-level relationship between two successful timing snapshots.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CompositionProgress {
    Unavailable,
    ComparableNoProgress,
    ComparableAdvanced,
    DiscontinuityOrReset,
}

/// The timing APIs cannot bind a specific submitted frame to compositor activity.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FrameAttribution {
    Unknown,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CompositionTimingEnvelope {
    tag: ContentTag,
    timing_policy: ContentTag,
    receipt: FrameReceiptV2,
    presenter: PresenterEvidence,
    qpc_frequency: QpcFrequency,
    pre: DwmTimingCall,
    presenter_bracket: QpcBracket,
    gdi_flush: GdiFlushCall,
    dwm_flush: DwmFlushCall,
    post: DwmTimingCall,
}

impl CompositionTimingEnvelope {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        receipt: FrameReceiptV2,
        presenter: PresenterEvidence,
        qpc_frequency: QpcFrequency,
        pre: DwmTimingCall,
        presenter_bracket: QpcBracket,
        gdi_flush: GdiFlushCall,
        dwm_flush: DwmFlushCall,
        post: DwmTimingCall,
    ) -> Result<Self, PresentationProtocolError> {
        let mut envelope = Self {
            tag: ContentTag::from_digest(TagKind::PresentationArtifact, [0; 32]),
            timing_policy: composition_timing_policy_tag(),
            receipt,
            presenter,
            qpc_frequency,
            pre,
            presenter_bracket,
            gdi_flush,
            dwm_flush,
            post,
        };
        envelope.validate_fields()?;
        envelope.tag = envelope.recompute_tag();
        Ok(envelope)
    }

    pub const fn tag(self) -> ContentTag {
        self.tag
    }

    pub const fn timing_policy(self) -> ContentTag {
        self.timing_policy
    }

    pub const fn receipt(self) -> FrameReceiptV2 {
        self.receipt
    }

    pub const fn presenter(self) -> PresenterEvidence {
        self.presenter
    }

    pub const fn qpc_frequency(self) -> QpcFrequency {
        self.qpc_frequency
    }

    pub const fn pre(self) -> DwmTimingCall {
        self.pre
    }

    pub const fn presenter_bracket(self) -> QpcBracket {
        self.presenter_bracket
    }

    pub const fn gdi_flush(self) -> GdiFlushCall {
        self.gdi_flush
    }

    pub const fn dwm_flush(self) -> DwmFlushCall {
        self.dwm_flush
    }

    pub const fn post(self) -> DwmTimingCall {
        self.post
    }

    pub const fn frame_attribution(self) -> FrameAttribution {
        FrameAttribution::Unknown
    }

    pub fn progress(self) -> CompositionProgress {
        if !self.has_complete_pair() {
            return CompositionProgress::Unavailable;
        }
        let pre = self
            .pre
            .snapshot
            .expect("complete pair requires a pre snapshot");
        let post = self
            .post
            .snapshot
            .expect("complete pair requires a post snapshot");
        if !pre.comparable_configuration(post) || pre.identity_regressed(post) {
            return CompositionProgress::DiscontinuityOrReset;
        }
        if post.has_progress_from(pre) {
            CompositionProgress::ComparableAdvanced
        } else {
            CompositionProgress::ComparableNoProgress
        }
    }

    pub fn encode_canonical(self) -> [u8; COMPOSITION_TIMING_ENVELOPE_PAYLOAD_LEN] {
        let mut bytes = [0; COMPOSITION_TIMING_ENVELOPE_PAYLOAD_LEN];
        self.feed_payload(&mut bytes);
        bytes
    }

    pub fn decode_canonical(bytes: &[u8]) -> Result<Self, PresentationProtocolError> {
        if bytes.len() != COMPOSITION_TIMING_ENVELOPE_PAYLOAD_LEN {
            return Err(PresentationProtocolError::WrongPayloadLength {
                expected: COMPOSITION_TIMING_ENVELOPE_PAYLOAD_LEN,
                actual: bytes.len(),
            });
        }
        let mut reader = FixedReader::new(bytes);
        let timing_policy = reader.tag(TagKind::Policy);
        if timing_policy != composition_timing_policy_tag() {
            return Err(PresentationProtocolError::TimingPolicyMismatch {
                expected: composition_timing_policy_tag(),
                actual: timing_policy,
            });
        }
        let receipt = FrameReceiptV2::decode_canonical(reader.bytes(FRAME_RECEIPT_V2_PAYLOAD_LEN))
            .map_err(PresentationProtocolError::InvalidFrameReceipt)?;
        let presenter = PresenterEvidence::decode(&mut reader);
        let qpc_frequency = QpcFrequency::decode(&mut reader)?;
        let pre = DwmTimingCall::decode(&mut reader, "pre timing call")?;
        let presenter_bracket = QpcBracket::decode(&mut reader, "checked presenter")?;
        let gdi_flush = decode_gdi_flush(&mut reader)?;
        let dwm_flush = decode_dwm_flush(&mut reader)?;
        let post = DwmTimingCall::decode(&mut reader, "post timing call")?;
        debug_assert_eq!(reader.cursor, COMPOSITION_TIMING_ENVELOPE_PAYLOAD_LEN);

        let mut envelope = Self::new(
            receipt,
            presenter,
            qpc_frequency,
            pre,
            presenter_bracket,
            gdi_flush,
            dwm_flush,
            post,
        )?;
        envelope.timing_policy = timing_policy;
        Ok(envelope)
    }

    pub fn verify(self) -> Result<(), PresentationProtocolError> {
        if self.timing_policy != composition_timing_policy_tag() {
            return Err(PresentationProtocolError::TimingPolicyMismatch {
                expected: composition_timing_policy_tag(),
                actual: self.timing_policy,
            });
        }
        self.validate_fields()?;
        if self.tag != self.recompute_tag() {
            return Err(PresentationProtocolError::EnvelopeTagMismatch);
        }
        Ok(())
    }

    fn validate_fields(self) -> Result<(), PresentationProtocolError> {
        self.receipt
            .verify()
            .map_err(PresentationProtocolError::InvalidFrameReceipt)?;
        if self.receipt.stream() != FrameStream::Composite {
            return Err(PresentationProtocolError::NonCompositeReceipt {
                actual: self.receipt.stream(),
            });
        }
        self.presenter.validate()?;
        self.qpc_frequency.validate()?;
        self.pre.validate("pre timing call")?;
        self.presenter_bracket.validate("checked presenter")?;
        self.gdi_flush.validate()?;
        self.dwm_flush.validate()?;
        self.post.validate("post timing call")?;
        Ok(())
    }

    fn has_complete_pair(self) -> bool {
        self.qpc_frequency.succeeded
            && self.qpc_frequency.counts_per_second > 0
            && self.pre.hresult == 0
            && self.pre.snapshot.is_some()
            && self.gdi_flush.succeeded
            && self.dwm_flush.hresult == 0
            && self.post.hresult == 0
            && self.post.snapshot.is_some()
            && qpc_reads_are_complete_and_ordered([
                ("pre timing before", self.pre.bracket.before),
                ("pre timing after", self.pre.bracket.after),
                ("presenter before", self.presenter_bracket.before),
                ("presenter after", self.presenter_bracket.after),
                ("GdiFlush before", self.gdi_flush.bracket.before),
                ("GdiFlush after", self.gdi_flush.bracket.after),
                ("DwmFlush before", self.dwm_flush.bracket.before),
                ("DwmFlush after", self.dwm_flush.bracket.after),
                ("post timing before", self.post.bracket.before),
                ("post timing after", self.post.bracket.after),
            ])
    }

    fn feed_payload(self, bytes: &mut [u8; COMPOSITION_TIMING_ENVELOPE_PAYLOAD_LEN]) {
        let mut writer = FixedWriter::new(bytes);
        writer.digest(self.timing_policy);
        writer.bytes(&self.receipt.encode_canonical());
        self.presenter.encode(&mut writer);
        self.qpc_frequency.encode(&mut writer);
        self.pre.encode(&mut writer);
        self.presenter_bracket.encode(&mut writer);
        self.gdi_flush.encode(&mut writer);
        self.dwm_flush.encode(&mut writer);
        self.post.encode(&mut writer);
        debug_assert_eq!(writer.cursor, COMPOSITION_TIMING_ENVELOPE_PAYLOAD_LEN);
    }

    fn recompute_tag(self) -> ContentTag {
        let payload = self.encode_canonical();
        hash_canonical(
            TagKind::PresentationArtifact,
            COMPOSITION_TIMING_ENVELOPE_SCHEMA,
            COMPOSITION_TIMING_ENVELOPE_PAYLOAD_LEN as u64,
            |sink| sink(&payload),
        )
        .expect("composition timing envelope encoding has a fixed length")
    }
}

fn decode_gdi_flush(
    reader: &mut FixedReader<'_>,
) -> Result<GdiFlushCall, PresentationProtocolError> {
    let before = QpcRead::decode(reader, "GdiFlush")?;
    let succeeded = reader.boolean("GdiFlush success")?;
    let after = QpcRead::decode(reader, "GdiFlush")?;
    Ok(GdiFlushCall {
        bracket: QpcBracket { before, after },
        succeeded,
    })
}

fn decode_dwm_flush(
    reader: &mut FixedReader<'_>,
) -> Result<DwmFlushCall, PresentationProtocolError> {
    let before = QpcRead::decode(reader, "DwmFlush")?;
    let hresult = reader.i32();
    let after = QpcRead::decode(reader, "DwmFlush")?;
    Ok(DwmFlushCall {
        bracket: QpcBracket { before, after },
        hresult,
    })
}

fn qpc_reads_are_complete_and_ordered<const N: usize>(reads: [(&'static str, QpcRead); N]) -> bool {
    let mut preceding = None::<i64>;
    for (_, read) in reads {
        if !read.succeeded {
            return false;
        }
        if let Some(earlier) = preceding
            && read.value < earlier
        {
            return false;
        }
        preceding = Some(read.value);
    }
    true
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PresentationProtocolError {
    WrongPayloadLength {
        expected: usize,
        actual: usize,
    },
    InvalidFrameReceipt(FrameReceiptV2Error),
    NonCompositeReceipt {
        actual: FrameStream,
    },
    WrongTagKind {
        field: &'static str,
        expected: TagKind,
        actual: TagKind,
    },
    TimingPolicyMismatch {
        expected: ContentTag,
        actual: ContentTag,
    },
    IncompletePresenterSubmission {
        requested: u32,
        returned: i32,
        error_code: u32,
    },
    InvalidQpcRead {
        field: &'static str,
        succeeded: bool,
        value: i64,
        error_code: u32,
    },
    InvalidQpcFrequency {
        succeeded: bool,
        counts_per_second: i64,
        error_code: u32,
    },
    UnknownBooleanMarker {
        field: &'static str,
        marker: u8,
    },
    DwmSnapshotPresenceMismatch {
        field: &'static str,
        hresult: i32,
        snapshot_present: bool,
    },
    NonZeroAbsentDwmSnapshotPadding {
        field: &'static str,
    },
    EnvelopeTagMismatch,
}

impl fmt::Display for PresentationProtocolError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {
            Self::WrongPayloadLength { expected, actual } => write!(
                formatter,
                "composition timing envelope requires {expected} bytes, found {actual}"
            ),
            Self::InvalidFrameReceipt(ref source) => {
                write!(formatter, "invalid composite frame receipt: {source}")
            }
            Self::NonCompositeReceipt { actual } => {
                write!(
                    formatter,
                    "presentation envelope requires Composite, found {actual:?}"
                )
            }
            Self::WrongTagKind {
                field,
                expected,
                actual,
            } => write!(
                formatter,
                "{field} requires a {} tag, received {}",
                expected.name(),
                actual.name()
            ),
            Self::TimingPolicyMismatch { expected, actual } => write!(
                formatter,
                "composition timing policy is {actual}, expected {expected}"
            ),
            Self::IncompletePresenterSubmission {
                requested,
                returned,
                error_code,
            } => write!(
                formatter,
                "checked presenter returned {returned} of {requested} scanlines with error {error_code}"
            ),
            Self::InvalidQpcRead {
                field,
                succeeded,
                value,
                error_code,
            } => write!(
                formatter,
                "{field} has noncanonical QPC state success={succeeded} value={value} error={error_code}"
            ),
            Self::InvalidQpcFrequency {
                succeeded,
                counts_per_second,
                error_code,
            } => write!(
                formatter,
                "QPC frequency has noncanonical state success={succeeded} frequency={counts_per_second} error={error_code}"
            ),
            Self::UnknownBooleanMarker { field, marker } => {
                write!(formatter, "{field} uses unknown Boolean marker {marker}")
            }
            Self::DwmSnapshotPresenceMismatch {
                field,
                hresult,
                snapshot_present,
            } => write!(
                formatter,
                "{field} HRESULT {hresult} is inconsistent with snapshot presence {snapshot_present}"
            ),
            Self::NonZeroAbsentDwmSnapshotPadding { field } => {
                write!(formatter, "{field} absent DWM snapshot has nonzero padding")
            }
            Self::EnvelopeTagMismatch => {
                formatter.write_str("composition timing envelope tag does not match its fields")
            }
        }
    }
}

impl std::error::Error for PresentationProtocolError {}

struct FixedWriter<'a> {
    bytes: &'a mut [u8],
    cursor: usize,
}

impl<'a> FixedWriter<'a> {
    fn new(bytes: &'a mut [u8]) -> Self {
        Self { bytes, cursor: 0 }
    }

    fn bytes(&mut self, value: &[u8]) {
        let end = self.cursor + value.len();
        self.bytes[self.cursor..end].copy_from_slice(value);
        self.cursor = end;
    }

    fn boolean(&mut self, value: bool) {
        self.bytes(&[u8::from(value)]);
    }

    fn digest(&mut self, value: ContentTag) {
        self.bytes(&value.digest());
    }

    fn u32(&mut self, value: u32) {
        self.bytes(&value.to_be_bytes());
    }

    fn i32(&mut self, value: i32) {
        self.bytes(&value.to_be_bytes());
    }

    fn u64(&mut self, value: u64) {
        self.bytes(&value.to_be_bytes());
    }

    fn i64(&mut self, value: i64) {
        self.bytes(&value.to_be_bytes());
    }
}

struct FixedReader<'a> {
    bytes: &'a [u8],
    cursor: usize,
}

impl<'a> FixedReader<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, cursor: 0 }
    }

    fn bytes(&mut self, len: usize) -> &'a [u8] {
        let start = self.cursor;
        self.cursor += len;
        &self.bytes[start..self.cursor]
    }

    fn boolean(&mut self, field: &'static str) -> Result<bool, PresentationProtocolError> {
        match self.bytes(1)[0] {
            0 => Ok(false),
            1 => Ok(true),
            marker => Err(PresentationProtocolError::UnknownBooleanMarker { field, marker }),
        }
    }

    fn tag(&mut self, kind: TagKind) -> ContentTag {
        ContentTag::from_digest(kind, self.bytes(32).try_into().expect("fixed digest lane"))
    }

    fn u32(&mut self) -> u32 {
        u32::from_be_bytes(self.bytes(4).try_into().expect("fixed u32 lane"))
    }

    fn i32(&mut self) -> i32 {
        i32::from_be_bytes(self.bytes(4).try_into().expect("fixed i32 lane"))
    }

    fn u64(&mut self) -> u64 {
        u64::from_be_bytes(self.bytes(8).try_into().expect("fixed u64 lane"))
    }

    fn i64(&mut self) -> i64 {
        i64::from_be_bytes(self.bytes(8).try_into().expect("fixed i64 lane"))
    }
}

const _: () = assert!(PRESENTER_EVIDENCE_LEN == 32 + 4 + 4 + 4 + 4 + 4);
const _: () = assert!(QPC_READ_LEN == 1 + 8 + 4);
const _: () = assert!(QPC_FREQUENCY_LEN == QPC_READ_LEN);
const _: () = assert!(QPC_BRACKET_LEN == QPC_READ_LEN * 2);
const _: () = assert!(DWM_TIMING_CALL_LEN == QPC_READ_LEN + 4 + 1 + 288 + QPC_READ_LEN);
const _: () = assert!(GDI_FLUSH_CALL_LEN == QPC_READ_LEN + 1 + QPC_READ_LEN);
const _: () = assert!(DWM_FLUSH_CALL_LEN == QPC_READ_LEN + 4 + QPC_READ_LEN);
const _: () = assert!(
    COMPOSITION_TIMING_ENVELOPE_PAYLOAD_LEN
        == 32
            + FRAME_RECEIPT_V2_PAYLOAD_LEN
            + PRESENTER_EVIDENCE_LEN
            + QPC_FREQUENCY_LEN
            + DWM_TIMING_CALL_LEN
            + QPC_BRACKET_LEN
            + GDI_FLUSH_CALL_LEN
            + DWM_FLUSH_CALL_LEN
            + DWM_TIMING_CALL_LEN
);

#[cfg(test)]
mod tests {
    use super::*;

    fn tag(kind: TagKind, byte: u8) -> ContentTag {
        ContentTag::from_digest(kind, [byte; 32])
    }

    fn receipt(stream: FrameStream) -> FrameReceiptV2 {
        FrameReceiptV2::new(
            None,
            stream,
            tag(TagKind::TickOutcome, 1),
            tag(TagKind::FrameInput, 2),
            tag(TagKind::Frame, 3),
        )
        .unwrap()
    }

    fn read(value: i64) -> QpcRead {
        QpcRead {
            succeeded: true,
            value,
            error_code: 0,
        }
    }

    fn bracket(before: i64, after: i64) -> QpcBracket {
        QpcBracket {
            before: read(before),
            after: read(after),
        }
    }

    fn snapshot(frame: u64) -> DwmTimingSnapshot {
        DwmTimingSnapshot {
            rate_refresh: DwmRatio {
                numerator: 60,
                denominator: 1,
            },
            qpc_refresh_period: 166_667,
            rate_compose: DwmRatio {
                numerator: 60,
                denominator: 1,
            },
            c_refresh: frame,
            c_frame: frame,
            c_refresh_frame: frame,
            c_frame_submitted: frame,
            c_frame_confirmed: frame,
            c_refresh_confirmed: frame,
            c_frame_displayed: frame,
            c_refresh_frame_displayed: frame,
            c_frame_complete: frame,
            ..DwmTimingSnapshot::default()
        }
    }

    fn vector_presenter_policy() -> ContentTag {
        let tag = hash_canonical(
            TagKind::Policy,
            b"bitblock/windows-present-policy/set-dibits-to-device/v1",
            17,
            |sink| {
                sink(&1_u16.to_be_bytes());
                sink(&[0]);
                sink(&0_u32.to_be_bytes());
                sink(&32_u16.to_be_bytes());
                sink(&[1]);
                sink(&0_u32.to_be_bytes());
                sink(&[1, 1, 1]);
            },
        )
        .unwrap();
        assert_eq!(
            tag.to_string(),
            "#bb.policy.sha256.opgpybj32tw72pep2xusit4lzzgsiozs5f5reppq67iiejueetvq"
        );
        tag
    }

    fn vector_digest(kind: TagKind, start: u8) -> ContentTag {
        ContentTag::from_digest(
            kind,
            std::array::from_fn(|index| start.wrapping_add(index as u8)),
        )
    }

    fn vector_snapshot(epoch: u64) -> DwmTimingSnapshot {
        let qpc = |index: u64| 900_000 + epoch * 10_000 + index * 17;
        let lane64 = |index: u64| 5_000 + epoch * 100 + index;
        let lane32 = |index: u32| 1_000 + epoch as u32 * 100 + index;
        DwmTimingSnapshot {
            rate_refresh: DwmRatio {
                numerator: 60,
                denominator: 1,
            },
            qpc_refresh_period: 166_667,
            rate_compose: DwmRatio {
                numerator: 60,
                denominator: 1,
            },
            qpc_vblank: qpc(5),
            c_refresh: lane64(6),
            c_dx_refresh: lane32(7),
            qpc_compose: qpc(8),
            c_frame: lane64(9),
            c_dx_present: lane32(10),
            c_refresh_frame: lane64(11),
            c_frame_submitted: lane64(12),
            c_dx_present_submitted: lane32(13),
            c_frame_confirmed: lane64(14),
            c_dx_present_confirmed: lane32(15),
            c_refresh_confirmed: lane64(16),
            c_dx_refresh_confirmed: lane32(17),
            c_frames_late: lane64(18),
            c_frames_outstanding: lane32(19),
            c_frame_displayed: lane64(20),
            qpc_frame_displayed: qpc(21),
            c_refresh_frame_displayed: lane64(22),
            c_frame_complete: lane64(23),
            qpc_frame_complete: qpc(24),
            c_frame_pending: lane64(25),
            qpc_frame_pending: qpc(26),
            c_frames_displayed: lane64(27),
            c_frames_complete: lane64(28),
            c_frames_pending: lane64(29),
            c_frames_available: lane64(30),
            c_frames_dropped: lane64(31),
            c_frames_missed: lane64(32),
            c_refresh_next_displayed: lane64(33),
            c_refresh_next_presented: lane64(34),
            c_refreshes_displayed: lane64(35),
            c_refreshes_presented: lane64(36),
            c_refresh_started: lane64(37),
            c_pixels_received: lane64(38),
            c_pixels_drawn: lane64(39),
            c_buffers_empty: lane64(40),
        }
    }

    fn envelope(
        pre_snapshot: Option<DwmTimingSnapshot>,
        post_snapshot: Option<DwmTimingSnapshot>,
    ) -> CompositionTimingEnvelope {
        CompositionTimingEnvelope::new(
            receipt(FrameStream::Composite),
            PresenterEvidence {
                policy: tag(TagKind::Policy, 4),
                requested_scanlines: 720,
                returned_scanlines: 720,
                call_error_code: 0,
                release_result: 1,
                release_error_code: 0,
            },
            QpcFrequency {
                succeeded: true,
                counts_per_second: 10_000_000,
                error_code: 0,
            },
            DwmTimingCall {
                bracket: bracket(10, 11),
                hresult: if pre_snapshot.is_some() { 0 } else { -1 },
                snapshot: pre_snapshot,
            },
            bracket(12, 13),
            GdiFlushCall {
                bracket: bracket(14, 15),
                succeeded: true,
            },
            DwmFlushCall {
                bracket: bracket(16, 17),
                hresult: 0,
            },
            DwmTimingCall {
                bracket: bracket(18, 19),
                hresult: if post_snapshot.is_some() { 0 } else { -1 },
                snapshot: post_snapshot,
            },
        )
        .unwrap()
    }

    #[test]
    fn fixed_layout_round_trips_and_verifies() {
        let original = envelope(Some(snapshot(100)), Some(snapshot(101)));
        let encoded = original.encode_canonical();
        assert_eq!(encoded.len(), COMPOSITION_TIMING_ENVELOPE_PAYLOAD_LEN);
        let decoded = CompositionTimingEnvelope::decode_canonical(&encoded).unwrap();
        assert_eq!(decoded, original);
        assert_eq!(decoded.tag().kind(), TagKind::PresentationArtifact);
        assert_eq!(decoded.progress(), CompositionProgress::ComparableAdvanced);
        assert_eq!(decoded.frame_attribution(), FrameAttribution::Unknown);
        decoded.verify().unwrap();
    }

    #[test]
    fn exact_frame_attribution_cannot_be_inferred_from_progress() {
        let observation = envelope(Some(snapshot(100)), Some(snapshot(101)));
        assert_eq!(
            observation.progress(),
            CompositionProgress::ComparableAdvanced
        );
        assert_eq!(observation.frame_attribution(), FrameAttribution::Unknown);
    }

    #[test]
    fn derives_each_conservative_progress_state() {
        assert_eq!(
            envelope(None, Some(snapshot(1))).progress(),
            CompositionProgress::Unavailable
        );
        assert_eq!(
            envelope(Some(snapshot(1)), Some(snapshot(1))).progress(),
            CompositionProgress::ComparableNoProgress
        );
        assert_eq!(
            envelope(Some(snapshot(1)), Some(snapshot(2))).progress(),
            CompositionProgress::ComparableAdvanced
        );
        assert_eq!(
            envelope(Some(snapshot(2)), Some(snapshot(1))).progress(),
            CompositionProgress::DiscontinuityOrReset
        );
    }

    #[test]
    fn rejects_non_composite_receipt() {
        let error = CompositionTimingEnvelope::new(
            receipt(FrameStream::Game),
            envelope(Some(snapshot(1)), Some(snapshot(1))).presenter(),
            QpcFrequency {
                succeeded: true,
                counts_per_second: 1,
                error_code: 0,
            },
            DwmTimingCall {
                bracket: bracket(1, 2),
                hresult: 0,
                snapshot: Some(snapshot(1)),
            },
            bracket(3, 4),
            GdiFlushCall {
                bracket: bracket(5, 6),
                succeeded: true,
            },
            DwmFlushCall {
                bracket: bracket(7, 8),
                hresult: 0,
            },
            DwmTimingCall {
                bracket: bracket(9, 10),
                hresult: 0,
                snapshot: Some(snapshot(1)),
            },
        )
        .unwrap_err();
        assert!(matches!(
            error,
            PresentationProtocolError::NonCompositeReceipt { .. }
        ));
    }

    #[test]
    fn rejects_nonzero_absent_snapshot_padding() {
        let mut encoded = envelope(None, None).encode_canonical();
        let pre_start =
            32 + FRAME_RECEIPT_V2_PAYLOAD_LEN + PRESENTER_EVIDENCE_LEN + QPC_FREQUENCY_LEN;
        let pre_padding_start = pre_start + QPC_READ_LEN + 4 + 1;
        encoded[pre_padding_start + 17] = 1;
        assert!(matches!(
            CompositionTimingEnvelope::decode_canonical(&encoded),
            Err(PresentationProtocolError::NonZeroAbsentDwmSnapshotPadding { .. })
        ));
    }

    #[test]
    fn rejects_unknown_boolean_marker() {
        let mut encoded = envelope(Some(snapshot(1)), Some(snapshot(1))).encode_canonical();
        let frequency_start = 32 + FRAME_RECEIPT_V2_PAYLOAD_LEN + PRESENTER_EVIDENCE_LEN;
        encoded[frequency_start] = 2;
        assert!(matches!(
            CompositionTimingEnvelope::decode_canonical(&encoded),
            Err(PresentationProtocolError::UnknownBooleanMarker { .. })
        ));
    }

    #[test]
    fn qpc_regression_round_trips_but_makes_progress_unavailable() {
        let mut encoded = envelope(Some(snapshot(1)), Some(snapshot(1))).encode_canonical();
        let presenter_start = 32
            + FRAME_RECEIPT_V2_PAYLOAD_LEN
            + PRESENTER_EVIDENCE_LEN
            + QPC_FREQUENCY_LEN
            + DWM_TIMING_CALL_LEN;
        encoded[presenter_start + 1..presenter_start + 9].copy_from_slice(&9_i64.to_be_bytes());
        let decoded = CompositionTimingEnvelope::decode_canonical(&encoded).unwrap();
        assert_eq!(decoded.encode_canonical(), encoded);
        assert_eq!(decoded.progress(), CompositionProgress::Unavailable);
        assert_eq!(decoded.frame_attribution(), FrameAttribution::Unknown);
    }

    #[test]
    fn barrier_failure_round_trips_but_makes_progress_unavailable() {
        let mut observation = envelope(Some(snapshot(1)), Some(snapshot(2)));
        observation.gdi_flush.succeeded = false;
        observation.tag = observation.recompute_tag();

        observation.verify().unwrap();
        let decoded =
            CompositionTimingEnvelope::decode_canonical(&observation.encode_canonical()).unwrap();
        assert!(!decoded.gdi_flush().succeeded);
        assert_eq!(decoded.progress(), CompositionProgress::Unavailable);
        assert_eq!(decoded.frame_attribution(), FrameAttribution::Unknown);
    }

    #[test]
    fn policy_and_envelope_tags_are_stable_for_same_bytes() {
        assert_eq!(
            composition_timing_policy_tag(),
            composition_timing_policy_tag()
        );
        let first = envelope(Some(snapshot(7)), Some(snapshot(8)));
        let second =
            CompositionTimingEnvelope::decode_canonical(&first.encode_canonical()).unwrap();
        assert_eq!(first.tag(), second.tag());
    }

    #[test]
    fn rust_matches_the_independent_python_known_answer_vector() {
        assert_eq!(
            composition_timing_policy_tag().to_string(),
            "#bb.policy.sha256.t2zuewerksnmvhhjyaaplmlmfb2poptubqxpc6czwvmuwufpanrq"
        );
        let receipt = FrameReceiptV2::new(
            None,
            FrameStream::Composite,
            vector_digest(TagKind::TickOutcome, 0x10),
            vector_digest(TagKind::FrameInput, 0x40),
            vector_digest(TagKind::Frame, 0x70),
        )
        .unwrap();
        let envelope = CompositionTimingEnvelope::new(
            receipt,
            PresenterEvidence {
                policy: vector_presenter_policy(),
                requested_scanlines: 720,
                returned_scanlines: 720,
                call_error_code: 0,
                release_result: 1,
                release_error_code: 0,
            },
            QpcFrequency {
                succeeded: true,
                counts_per_second: 10_000_000,
                error_code: 0,
            },
            DwmTimingCall {
                bracket: bracket(1_000_000, 1_000_100),
                hresult: 0,
                snapshot: Some(vector_snapshot(0)),
            },
            bracket(1_000_200, 1_000_400),
            GdiFlushCall {
                bracket: bracket(1_000_500, 1_000_600),
                succeeded: true,
            },
            DwmFlushCall {
                bracket: bracket(1_000_700, 1_000_800),
                hresult: 0,
            },
            DwmTimingCall {
                bracket: bracket(1_000_900, 1_001_000),
                hresult: 0,
                snapshot: Some(vector_snapshot(1)),
            },
        )
        .unwrap();

        assert_eq!(
            envelope.tag().to_string(),
            "#bb.presentation-artifact.sha256.sw2i52veohfmc3lqe74tzd72n6ppwxxshejxmazclwikgniqx2sq"
        );
    }
}
