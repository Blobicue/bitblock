//! Canonical bundle for one checked presentation, GDI capture, and its surrounding topology.
//!
//! The display artifact remains the only pixel-bearing object. The two topology values are the
//! observations supplied from immediately before and after that capture; this byte format does
//! not independently prove their collection order or add a timestamp. Composition timing remains
//! session-level evidence and does not establish scanout or physical-light output.

use std::fmt;

use crate::display_artifact::{DisplayArtifactError, DisplayCaptureArtifact};
use crate::output_topology::{OutputTopologyError, OutputTopologyObservation};
use crate::presentation::{
    COMPOSITION_TIMING_ENVELOPE_PAYLOAD_LEN, CompositionTimingEnvelope, PresentationProtocolError,
};
use crate::protocol::{ContentTag, TagError, TagKind, hash_canonical};
use crate::storage::MAX_STORAGE_PAYLOAD_BYTES;

pub const PRESENTATION_OUTPUT_ARTIFACT_SCHEMA: &[u8] = b"bitblock/presentation-output-artifact/v1";
pub const MAX_PRESENTATION_OUTPUT_ARTIFACT_BYTES: usize = MAX_STORAGE_PAYLOAD_BYTES;

const LENGTH_BYTES: usize = 8;
pub const PRESENTATION_OUTPUT_ARTIFACT_FIXED_BYTES: usize =
    LENGTH_BYTES + COMPOSITION_TIMING_ENVELOPE_PAYLOAD_LEN + LENGTH_BYTES + LENGTH_BYTES;

/// A comparison derived from the two retained topology observations.
///
/// This value is deliberately absent from the canonical payload. It is recomputed from the
/// observations whenever requested.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RouteStability {
    Stable,
    Changed,
    Unavailable,
}

/// One additive presentation-output object. Nested canonical payloads are retained exactly once.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PresentationOutputArtifact {
    tag: ContentTag,
    encoded_len: usize,
    display: DisplayCaptureArtifact,
    timing: CompositionTimingEnvelope,
    pre_topology: OutputTopologyObservation,
    post_topology: OutputTopologyObservation,
}

impl PresentationOutputArtifact {
    pub fn new(
        display: DisplayCaptureArtifact,
        timing: CompositionTimingEnvelope,
        pre_topology: OutputTopologyObservation,
        post_topology: OutputTopologyObservation,
    ) -> Result<Self, PresentationOutputArtifactError> {
        let mut artifact = Self {
            tag: ContentTag::from_digest(TagKind::PresentationOutputArtifact, [0; 32]),
            encoded_len: 0,
            display,
            timing,
            pre_topology,
            post_topology,
        };
        let parts = artifact.prepare_parts()?;
        artifact.tag = tag_parts(&parts)?;
        artifact.encoded_len = parts.total_len;
        Ok(artifact)
    }

    pub const fn tag(&self) -> ContentTag {
        self.tag
    }

    pub const fn display(&self) -> &DisplayCaptureArtifact {
        &self.display
    }

    pub const fn timing(&self) -> CompositionTimingEnvelope {
        self.timing
    }

    pub const fn pre_topology(&self) -> &OutputTopologyObservation {
        &self.pre_topology
    }

    pub const fn post_topology(&self) -> &OutputTopologyObservation {
        &self.post_topology
    }

    pub fn route_stability(&self) -> RouteStability {
        match (
            self.pre_topology.available(),
            self.post_topology.available(),
        ) {
            (Some(_), Some(_)) if self.pre_topology.stable_topology_eq(&self.post_topology) => {
                RouteStability::Stable
            }
            (Some(_), Some(_)) => RouteStability::Changed,
            _ => RouteStability::Unavailable,
        }
    }

    pub fn encoded_len(&self) -> usize {
        self.encoded_len
    }

    pub fn encode_canonical(&self) -> Result<Vec<u8>, PresentationOutputArtifactError> {
        let parts = self.prepare_parts()?;
        let mut bytes = Vec::new();
        bytes.try_reserve_exact(parts.total_len).map_err(|_| {
            PresentationOutputArtifactError::AllocationFailed {
                requested: parts.total_len,
            }
        })?;
        feed_parts(&parts, &mut |chunk| bytes.extend_from_slice(chunk));
        debug_assert_eq!(bytes.len(), parts.total_len);
        Ok(bytes)
    }

    pub fn decode_canonical(bytes: &[u8]) -> Result<Self, PresentationOutputArtifactError> {
        if bytes.len() > MAX_PRESENTATION_OUTPUT_ARTIFACT_BYTES {
            return Err(PresentationOutputArtifactError::PayloadTooLarge {
                actual: bytes.len(),
                maximum: MAX_PRESENTATION_OUTPUT_ARTIFACT_BYTES,
            });
        }

        let mut reader = Reader::new(bytes);
        let display_len = reader.length("display artifact length")?;
        let display_bytes = reader.take("display artifact", display_len)?;
        let timing_bytes = reader.take(
            "composition timing envelope",
            COMPOSITION_TIMING_ENVELOPE_PAYLOAD_LEN,
        )?;
        let pre_topology_len = reader.length("pre-capture topology length")?;
        let pre_topology_bytes = reader.take("pre-capture topology", pre_topology_len)?;
        let post_topology_len = reader.length("post-capture topology length")?;
        let post_topology_bytes = reader.take("post-capture topology", post_topology_len)?;
        reader.finish()?;

        checked_payload_len(display_len, pre_topology_len, post_topology_len)?;

        let display = DisplayCaptureArtifact::decode_canonical(display_bytes)
            .map_err(PresentationOutputArtifactError::InvalidDisplayArtifact)?;
        let timing = CompositionTimingEnvelope::decode_canonical(timing_bytes)
            .map_err(PresentationOutputArtifactError::InvalidTimingEnvelope)?;
        let pre_topology = OutputTopologyObservation::decode_canonical(pre_topology_bytes)
            .map_err(PresentationOutputArtifactError::InvalidPreTopology)?;
        let post_topology = OutputTopologyObservation::decode_canonical(post_topology_bytes)
            .map_err(PresentationOutputArtifactError::InvalidPostTopology)?;

        let artifact = Self::new(display, timing, pre_topology, post_topology)?;
        if !artifact.payload_matches(bytes)? {
            return Err(PresentationOutputArtifactError::NonCanonicalEncoding);
        }
        Ok(artifact)
    }

    pub fn verify(&self) -> Result<(), PresentationOutputArtifactError> {
        let parts = self.prepare_parts()?;
        if self.tag.kind() != TagKind::PresentationOutputArtifact {
            return Err(PresentationOutputArtifactError::WrongTagKind {
                actual: self.tag.kind(),
            });
        }
        if self.tag != tag_parts(&parts)? {
            return Err(PresentationOutputArtifactError::ArtifactTagMismatch);
        }
        if self.encoded_len != parts.total_len {
            return Err(PresentationOutputArtifactError::NonCanonicalEncoding);
        }
        Ok(())
    }

    fn prepare_parts(&self) -> Result<EncodedParts, PresentationOutputArtifactError> {
        self.display
            .verify()
            .map_err(PresentationOutputArtifactError::InvalidDisplayArtifact)?;
        self.timing
            .verify()
            .map_err(PresentationOutputArtifactError::InvalidTimingEnvelope)?;
        self.pre_topology
            .verify()
            .map_err(PresentationOutputArtifactError::InvalidPreTopology)?;
        self.post_topology
            .verify()
            .map_err(PresentationOutputArtifactError::InvalidPostTopology)?;

        let display_receipt = self.display.receipt().expected_receipt();
        let timing_receipt = self.timing.receipt();
        if display_receipt != timing_receipt {
            return Err(PresentationOutputArtifactError::ReceiptMismatch {
                display: display_receipt.tag(),
                timing: timing_receipt.tag(),
            });
        }
        let capture = self.display.receipt().facts();
        let timing_frequency = self.timing.qpc_frequency();
        if timing_frequency.succeeded && capture.qpc_frequency > 0 {
            if timing_frequency.counts_per_second != capture.qpc_frequency {
                return Err(PresentationOutputArtifactError::QpcFrequencyMismatch {
                    timing: timing_frequency.counts_per_second,
                    capture: capture.qpc_frequency,
                });
            }
            let timing_post = self.timing.post().bracket.after;
            if timing_post.succeeded && capture.qpc_start < timing_post.value {
                return Err(PresentationOutputArtifactError::CapturePrecedesTimingPost {
                    timing_post: timing_post.value,
                    capture_start: capture.qpc_start,
                });
            }
        }

        let pre_topology = self
            .pre_topology
            .try_encode_canonical()
            .map_err(PresentationOutputArtifactError::InvalidPreTopology)?;
        let post_topology = self
            .post_topology
            .try_encode_canonical()
            .map_err(PresentationOutputArtifactError::InvalidPostTopology)?;
        let display_len = self.display.encoded_len();
        let total_len = checked_payload_len(display_len, pre_topology.len(), post_topology.len())?;

        // Materialize the pixel-bearing nested payload only after the outer cap has admitted it.
        let display = self
            .display
            .encode_canonical()
            .map_err(PresentationOutputArtifactError::InvalidDisplayArtifact)?;
        debug_assert_eq!(display.len(), display_len);

        Ok(EncodedParts {
            display,
            timing: self.timing.encode_canonical(),
            pre_topology,
            post_topology,
            total_len,
        })
    }

    fn payload_matches(&self, expected: &[u8]) -> Result<bool, PresentationOutputArtifactError> {
        let parts = self.prepare_parts()?;
        let mut cursor = 0_usize;
        let mut exact = true;
        feed_parts(&parts, &mut |chunk| {
            if !exact {
                return;
            }
            let Some(end) = cursor.checked_add(chunk.len()) else {
                exact = false;
                return;
            };
            if expected.get(cursor..end) != Some(chunk) {
                exact = false;
                return;
            }
            cursor = end;
        });
        Ok(exact && cursor == expected.len())
    }
}

struct EncodedParts {
    display: Vec<u8>,
    timing: [u8; COMPOSITION_TIMING_ENVELOPE_PAYLOAD_LEN],
    pre_topology: Vec<u8>,
    post_topology: Vec<u8>,
    total_len: usize,
}

fn checked_payload_len(
    display_len: usize,
    pre_topology_len: usize,
    post_topology_len: usize,
) -> Result<usize, PresentationOutputArtifactError> {
    let total = PRESENTATION_OUTPUT_ARTIFACT_FIXED_BYTES
        .checked_add(display_len)
        .and_then(|value| value.checked_add(pre_topology_len))
        .and_then(|value| value.checked_add(post_topology_len))
        .ok_or(PresentationOutputArtifactError::LengthOverflow)?;
    if total > MAX_PRESENTATION_OUTPUT_ARTIFACT_BYTES {
        return Err(PresentationOutputArtifactError::PayloadTooLarge {
            actual: total,
            maximum: MAX_PRESENTATION_OUTPUT_ARTIFACT_BYTES,
        });
    }
    Ok(total)
}

fn feed_parts(parts: &EncodedParts, sink: &mut dyn FnMut(&[u8])) {
    sink(&(parts.display.len() as u64).to_be_bytes());
    sink(&parts.display);
    sink(&parts.timing);
    sink(&(parts.pre_topology.len() as u64).to_be_bytes());
    sink(&parts.pre_topology);
    sink(&(parts.post_topology.len() as u64).to_be_bytes());
    sink(&parts.post_topology);
}

fn tag_parts(parts: &EncodedParts) -> Result<ContentTag, PresentationOutputArtifactError> {
    hash_canonical(
        TagKind::PresentationOutputArtifact,
        PRESENTATION_OUTPUT_ARTIFACT_SCHEMA,
        parts.total_len as u64,
        |sink| feed_parts(parts, sink),
    )
    .map_err(PresentationOutputArtifactError::Hash)
}

struct Reader<'a> {
    bytes: &'a [u8],
    cursor: usize,
}

impl<'a> Reader<'a> {
    const fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, cursor: 0 }
    }

    fn length(&mut self, field: &'static str) -> Result<usize, PresentationOutputArtifactError> {
        let bytes = self.take(field, LENGTH_BYTES)?;
        let value = u64::from_be_bytes(bytes.try_into().expect("fixed length lane"));
        usize::try_from(value)
            .map_err(|_| PresentationOutputArtifactError::DeclaredLengthOverflow { field, value })
    }

    fn take(
        &mut self,
        field: &'static str,
        length: usize,
    ) -> Result<&'a [u8], PresentationOutputArtifactError> {
        let remaining = self.bytes.len().saturating_sub(self.cursor);
        if length > remaining {
            return Err(PresentationOutputArtifactError::Truncated {
                field,
                needed: length,
                remaining,
            });
        }
        let end = self
            .cursor
            .checked_add(length)
            .ok_or(PresentationOutputArtifactError::LengthOverflow)?;
        let bytes = &self.bytes[self.cursor..end];
        self.cursor = end;
        Ok(bytes)
    }

    fn finish(self) -> Result<(), PresentationOutputArtifactError> {
        if self.cursor != self.bytes.len() {
            return Err(PresentationOutputArtifactError::TrailingBytes {
                count: self.bytes.len() - self.cursor,
            });
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PresentationOutputArtifactError {
    InvalidDisplayArtifact(DisplayArtifactError),
    InvalidTimingEnvelope(PresentationProtocolError),
    InvalidPreTopology(OutputTopologyError),
    InvalidPostTopology(OutputTopologyError),
    ReceiptMismatch {
        display: ContentTag,
        timing: ContentTag,
    },
    QpcFrequencyMismatch {
        timing: i64,
        capture: i64,
    },
    CapturePrecedesTimingPost {
        timing_post: i64,
        capture_start: i64,
    },
    WrongTagKind {
        actual: TagKind,
    },
    PayloadTooLarge {
        actual: usize,
        maximum: usize,
    },
    LengthOverflow,
    DeclaredLengthOverflow {
        field: &'static str,
        value: u64,
    },
    Truncated {
        field: &'static str,
        needed: usize,
        remaining: usize,
    },
    TrailingBytes {
        count: usize,
    },
    AllocationFailed {
        requested: usize,
    },
    NonCanonicalEncoding,
    ArtifactTagMismatch,
    Hash(TagError),
}

impl fmt::Display for PresentationOutputArtifactError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidDisplayArtifact(source) => {
                write!(formatter, "invalid nested display artifact: {source}")
            }
            Self::InvalidTimingEnvelope(source) => {
                write!(
                    formatter,
                    "invalid nested composition timing envelope: {source}"
                )
            }
            Self::InvalidPreTopology(source) => {
                write!(formatter, "invalid pre-capture output topology: {source}")
            }
            Self::InvalidPostTopology(source) => {
                write!(formatter, "invalid post-capture output topology: {source}")
            }
            Self::ReceiptMismatch { display, timing } => write!(
                formatter,
                "display expected receipt {display} differs from timing receipt {timing}"
            ),
            Self::QpcFrequencyMismatch { timing, capture } => write!(
                formatter,
                "composition timing uses QPC frequency {timing}, capture uses {capture}"
            ),
            Self::CapturePrecedesTimingPost {
                timing_post,
                capture_start,
            } => write!(
                formatter,
                "capture QPC start {capture_start} precedes timing post-call QPC {timing_post}"
            ),
            Self::WrongTagKind { actual } => write!(
                formatter,
                "presentation output has {} tag kind instead of presentation-output-artifact",
                actual.name()
            ),
            Self::PayloadTooLarge { actual, maximum } => write!(
                formatter,
                "presentation-output artifact requires {actual} bytes, exceeding {maximum}"
            ),
            Self::LengthOverflow => {
                formatter.write_str("presentation-output artifact length overflowed")
            }
            Self::DeclaredLengthOverflow { field, value } => {
                write!(formatter, "{field} declares unrepresentable length {value}")
            }
            Self::Truncated {
                field,
                needed,
                remaining,
            } => write!(
                formatter,
                "presentation-output {field} needs {needed} bytes but only {remaining} remain"
            ),
            Self::TrailingBytes { count } => {
                write!(
                    formatter,
                    "presentation-output artifact has {count} trailing bytes"
                )
            }
            Self::AllocationFailed { requested } => write!(
                formatter,
                "could not allocate {requested} presentation-output payload bytes"
            ),
            Self::NonCanonicalEncoding => {
                formatter.write_str("presentation-output bytes are not canonical")
            }
            Self::ArtifactTagMismatch => formatter
                .write_str("presentation-output artifact tag does not match its canonical bytes"),
            Self::Hash(source) => write!(formatter, "presentation-output hash failed: {source}"),
        }
    }
}

impl std::error::Error for PresentationOutputArtifactError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::InvalidDisplayArtifact(source) => Some(source),
            Self::InvalidTimingEnvelope(source) => Some(source),
            Self::InvalidPreTopology(source) | Self::InvalidPostTopology(source) => Some(source),
            Self::Hash(source) => Some(source),
            Self::ReceiptMismatch { .. }
            | Self::QpcFrequencyMismatch { .. }
            | Self::CapturePrecedesTimingPost { .. }
            | Self::WrongTagKind { .. }
            | Self::PayloadTooLarge { .. }
            | Self::LengthOverflow
            | Self::DeclaredLengthOverflow { .. }
            | Self::Truncated { .. }
            | Self::TrailingBytes { .. }
            | Self::AllocationFailed { .. }
            | Self::NonCanonicalEncoding
            | Self::ArtifactTagMismatch => None,
        }
    }
}

const _: () = assert!(PRESENTATION_OUTPUT_ARTIFACT_FIXED_BYTES == 972);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::display::{
        CaptureRect, CaptureStage, DisplayCaptureFacts, DisplayLedgerV2, DisplayOutcome,
    };
    use crate::frame::FrameRef;
    use crate::frame_v2::{FrameReceiptV2, FrameStream};
    use crate::output_topology::{
        AcquisitionAttempt, AcquisitionFacts, RawRational, RectI32, SourceFacts, SourceModeFacts,
        TargetIdentityObservation, TargetRouteFacts, TargetSignalTiming, Utf16Text,
    };
    use crate::presentation::{
        DwmFlushCall, DwmTimingCall, GdiFlushCall, PresenterEvidence, QpcBracket, QpcFrequency,
        QpcRead,
    };

    const SMALL_PIXELS: [u32; 2] = [0x0011_2233, 0x0044_5566];

    fn test_tag(kind: TagKind, lane: u8) -> ContentTag {
        hash_canonical(kind, b"bitblock/presentation-output-test/v1", 1, |sink| {
            sink(&[lane])
        })
        .unwrap()
    }

    fn frame_receipt(frame: FrameRef<'_>, lane: u8) -> FrameReceiptV2 {
        FrameReceiptV2::new(
            None,
            FrameStream::Composite,
            test_tag(TagKind::TickOutcome, lane),
            test_tag(TagKind::FrameInput, lane.wrapping_add(1)),
            frame.content_tag(),
        )
        .unwrap()
    }

    fn display_artifact(
        frame_receipt: FrameReceiptV2,
        width: u32,
        pixels: &[u32],
    ) -> DisplayCaptureArtifact {
        let expected = FrameRef::new(width, 1, pixels).unwrap();
        let mut ledger = DisplayLedgerV2::default();
        let receipt = *ledger
            .append(
                frame_receipt,
                width,
                1,
                DisplayCaptureFacts::unavailable(CaptureStage::FlushBefore, 5),
            )
            .unwrap();
        DisplayCaptureArtifact::new(receipt, expected, None).unwrap()
    }

    fn complete_display_artifact(
        frame_receipt: FrameReceiptV2,
        qpc_frequency: i64,
        qpc_start: i64,
    ) -> DisplayCaptureArtifact {
        let expected = FrameRef::new(2, 1, &SMALL_PIXELS).unwrap();
        let rect = CaptureRect {
            left: 0,
            top: 0,
            right: 2,
            bottom: 1,
        };
        let facts = DisplayCaptureFacts {
            outcome: DisplayOutcome::Match,
            stage: CaptureStage::Complete,
            error_code: 0,
            flush_before: true,
            compositor_flush: true,
            flush_after: true,
            rect_before: rect,
            rect_after: rect,
            dpi: 96,
            qpc_start,
            qpc_end: qpc_start + 10,
            qpc_frequency,
            captured: Some(expected.observe()),
            mismatch_count: 0,
            first_mismatch: None,
        };
        let mut ledger = DisplayLedgerV2::default();
        let receipt = *ledger.append(frame_receipt, 2, 1, facts).unwrap();
        DisplayCaptureArtifact::new(receipt, expected, Some(expected)).unwrap()
    }

    fn qpc(value: i64) -> QpcRead {
        QpcRead {
            succeeded: true,
            value,
            error_code: 0,
        }
    }

    fn bracket(before: i64, after: i64) -> QpcBracket {
        QpcBracket {
            before: qpc(before),
            after: qpc(after),
        }
    }

    fn timing(receipt: FrameReceiptV2) -> CompositionTimingEnvelope {
        CompositionTimingEnvelope::new(
            receipt,
            PresenterEvidence {
                policy: test_tag(TagKind::Policy, 9),
                requested_scanlines: 1,
                returned_scanlines: 1,
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
                hresult: -1,
                snapshot: None,
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
                hresult: -1,
                snapshot: None,
            },
        )
        .unwrap()
    }

    fn acquisition(seed: u32) -> AcquisitionFacts {
        AcquisitionFacts {
            profile_code_raw: seed,
            request_flags_raw: seed.rotate_left(3),
            attempts: vec![AcquisitionAttempt {
                retry_ordinal: 0,
                outcome_code_raw: seed,
                ..AcquisitionAttempt::default()
            }],
        }
    }

    fn source() -> SourceFacts {
        SourceFacts {
            gdi_device_name: Utf16Text::from_units(&[b'D' as u16]).unwrap(),
            client_rect: RectI32 {
                left: 0,
                top: 0,
                right: 2,
                bottom: 1,
            },
            monitor_rect: RectI32 {
                left: 0,
                top: 0,
                right: 1920,
                bottom: 1080,
            },
            work_rect: RectI32 {
                left: 0,
                top: 0,
                right: 1920,
                bottom: 1040,
            },
            monitor_flags_raw: 1,
            mode: SourceModeFacts {
                width: 1920,
                height: 1080,
                ..SourceModeFacts::default()
            },
            status_flags_raw: 0,
        }
    }

    fn route(path_flags_raw: u32) -> TargetRouteFacts {
        TargetRouteFacts {
            identity: TargetIdentityObservation::Unavailable {
                reason_code_raw: 7,
                error_code: 5,
            },
            output_technology_raw: 1,
            target_available: true,
            status_flags_raw: 2,
            rotation_raw: 1,
            scaling_raw: 1,
            refresh_rate: RawRational {
                numerator: 60,
                denominator: 1,
            },
            path_flags_raw,
            signal: TargetSignalTiming::default(),
            desktop_image: None,
        }
    }

    fn available_topology(seed: u32, path_flags_raw: u32) -> OutputTopologyObservation {
        OutputTopologyObservation::new_available(
            acquisition(seed),
            source(),
            vec![route(path_flags_raw)],
        )
        .unwrap()
    }

    fn unavailable_topology(seed: u32) -> OutputTopologyObservation {
        OutputTopologyObservation::new_unavailable(acquisition(seed), 11, 87).unwrap()
    }

    fn small_artifact() -> PresentationOutputArtifact {
        let frame = FrameRef::new(2, 1, &SMALL_PIXELS).unwrap();
        let receipt = frame_receipt(frame, 1);
        PresentationOutputArtifact::new(
            display_artifact(receipt, 2, &SMALL_PIXELS),
            timing(receipt),
            available_topology(1, 4),
            available_topology(2, 4),
        )
        .unwrap()
    }

    fn layout_offsets(bytes: &[u8]) -> (usize, usize, usize, usize, usize) {
        let display_len =
            usize::try_from(u64::from_be_bytes(bytes[..8].try_into().unwrap())).unwrap();
        let timing_start = LENGTH_BYTES + display_len;
        let pre_len_offset = timing_start + COMPOSITION_TIMING_ENVELOPE_PAYLOAD_LEN;
        let pre_len = usize::try_from(u64::from_be_bytes(
            bytes[pre_len_offset..pre_len_offset + 8]
                .try_into()
                .unwrap(),
        ))
        .unwrap();
        let pre_start = pre_len_offset + LENGTH_BYTES;
        let post_len_offset = pre_start + pre_len;
        let post_start = post_len_offset + LENGTH_BYTES;
        (
            LENGTH_BYTES,
            timing_start,
            pre_len_offset,
            pre_start,
            post_start,
        )
    }

    #[test]
    fn exact_nested_layout_round_trips_without_extra_pixel_or_tag_lanes() {
        let artifact = small_artifact();
        let bytes = artifact.encode_canonical().unwrap();
        let display = artifact.display().encode_canonical().unwrap();
        let timing = artifact.timing().encode_canonical();
        let pre = artifact.pre_topology().try_encode_canonical().unwrap();
        let post = artifact.post_topology().try_encode_canonical().unwrap();

        let (display_start, timing_start, pre_len_offset, pre_start, post_start) =
            layout_offsets(&bytes);
        assert_eq!(&bytes[display_start..timing_start], display.as_slice());
        assert_eq!(&bytes[timing_start..pre_len_offset], timing.as_slice());
        let post_len_offset = post_start - LENGTH_BYTES;
        assert_eq!(&bytes[pre_start..post_len_offset], pre.as_slice());
        assert_eq!(&bytes[post_start..], post.as_slice());
        assert_eq!(
            bytes.len(),
            display.len() + timing.len() + pre.len() + post.len() + 3 * LENGTH_BYTES
        );
        assert_eq!(bytes.len(), artifact.encoded_len());

        let decoded = PresentationOutputArtifact::decode_canonical(&bytes).unwrap();
        assert_eq!(decoded, artifact);
        assert_eq!(decoded.tag().kind(), TagKind::PresentationOutputArtifact);
        assert_eq!(
            decoded.tag(),
            hash_canonical(
                TagKind::PresentationOutputArtifact,
                PRESENTATION_OUTPUT_ARTIFACT_SCHEMA,
                bytes.len() as u64,
                |sink| sink(&bytes),
            )
            .unwrap()
        );
        decoded.verify().unwrap();
    }

    #[test]
    fn every_proper_prefix_and_trailing_suffix_are_rejected() {
        let bytes = small_artifact().encode_canonical().unwrap();
        for end in 0..bytes.len() {
            assert!(
                PresentationOutputArtifact::decode_canonical(&bytes[..end]).is_err(),
                "accepted prefix ending at {end}"
            );
        }
        let mut trailing = bytes;
        trailing.push(0);
        assert!(matches!(
            PresentationOutputArtifact::decode_canonical(&trailing),
            Err(PresentationOutputArtifactError::TrailingBytes { count: 1 })
        ));
    }

    #[test]
    fn structural_and_nested_mutations_fail_closed() {
        let bytes = small_artifact().encode_canonical().unwrap();
        let (display_start, timing_start, pre_len_offset, pre_start, post_start) =
            layout_offsets(&bytes);

        let mut display_length = bytes.clone();
        display_length[7] = display_length[7].wrapping_add(1);
        assert!(PresentationOutputArtifact::decode_canonical(&display_length).is_err());

        let mut display = bytes.clone();
        display[display_start] = 2;
        assert!(matches!(
            PresentationOutputArtifact::decode_canonical(&display),
            Err(PresentationOutputArtifactError::InvalidDisplayArtifact(_))
        ));

        let mut timing = bytes.clone();
        timing[timing_start] ^= 1;
        assert!(matches!(
            PresentationOutputArtifact::decode_canonical(&timing),
            Err(PresentationOutputArtifactError::InvalidTimingEnvelope(_))
        ));

        let mut pre = bytes.clone();
        pre[pre_start] = 0xff;
        assert!(matches!(
            PresentationOutputArtifact::decode_canonical(&pre),
            Err(PresentationOutputArtifactError::InvalidPreTopology(_))
        ));

        let mut post = bytes.clone();
        post[post_start] = 0xff;
        assert!(matches!(
            PresentationOutputArtifact::decode_canonical(&post),
            Err(PresentationOutputArtifactError::InvalidPostTopology(_))
        ));

        let mut pre_length = bytes;
        pre_length[pre_len_offset..pre_len_offset + LENGTH_BYTES]
            .copy_from_slice(&0_u64.to_be_bytes());
        assert!(PresentationOutputArtifact::decode_canonical(&pre_length).is_err());
    }

    #[test]
    fn receipt_equality_is_a_powered_admission_control() {
        let frame = FrameRef::new(2, 1, &SMALL_PIXELS).unwrap();
        let display_receipt = frame_receipt(frame, 20);
        let timing_receipt = frame_receipt(frame, 40);
        assert_ne!(display_receipt, timing_receipt);
        assert_eq!(display_receipt.frame(), timing_receipt.frame());

        let error = PresentationOutputArtifact::new(
            display_artifact(display_receipt, 2, &SMALL_PIXELS),
            timing(timing_receipt),
            unavailable_topology(1),
            unavailable_topology(2),
        )
        .unwrap_err();
        assert_eq!(
            error,
            PresentationOutputArtifactError::ReceiptMismatch {
                display: display_receipt.tag(),
                timing: timing_receipt.tag(),
            }
        );
    }

    #[test]
    fn capture_clock_must_match_and_follow_the_bound_timing_envelope() {
        let frame = FrameRef::new(2, 1, &SMALL_PIXELS).unwrap();
        let frequency_receipt = frame_receipt(frame, 50);
        let frequency_error = PresentationOutputArtifact::new(
            complete_display_artifact(frequency_receipt, 9_000_000, 100),
            timing(frequency_receipt),
            unavailable_topology(1),
            unavailable_topology(2),
        )
        .unwrap_err();
        assert_eq!(
            frequency_error,
            PresentationOutputArtifactError::QpcFrequencyMismatch {
                timing: 10_000_000,
                capture: 9_000_000,
            }
        );

        let order_receipt = frame_receipt(frame, 60);
        let order_error = PresentationOutputArtifact::new(
            complete_display_artifact(order_receipt, 10_000_000, 18),
            timing(order_receipt),
            unavailable_topology(1),
            unavailable_topology(2),
        )
        .unwrap_err();
        assert_eq!(
            order_error,
            PresentationOutputArtifactError::CapturePrecedesTimingPost {
                timing_post: 19,
                capture_start: 18,
            }
        );

        let admitted_receipt = frame_receipt(frame, 70);
        PresentationOutputArtifact::new(
            complete_display_artifact(admitted_receipt, 10_000_000, 20),
            timing(admitted_receipt),
            unavailable_topology(1),
            unavailable_topology(2),
        )
        .unwrap();
    }

    #[test]
    fn route_stability_uses_available_stable_comparison_and_is_not_encoded() {
        let stable_pre = available_topology(1, 4);
        let stable_post = available_topology(99, 4);
        assert_ne!(stable_pre, stable_post);
        assert!(stable_pre.stable_topology_eq(&stable_post));

        let frame = FrameRef::new(2, 1, &SMALL_PIXELS).unwrap();
        let receipt = frame_receipt(frame, 1);
        let stable = PresentationOutputArtifact::new(
            display_artifact(receipt, 2, &SMALL_PIXELS),
            timing(receipt),
            stable_pre,
            stable_post,
        )
        .unwrap();
        assert_eq!(stable.route_stability(), RouteStability::Stable);

        let receipt = frame_receipt(frame, 2);
        let changed = PresentationOutputArtifact::new(
            display_artifact(receipt, 2, &SMALL_PIXELS),
            timing(receipt),
            available_topology(1, 4),
            available_topology(2, 5),
        )
        .unwrap();
        assert_eq!(changed.route_stability(), RouteStability::Changed);

        let receipt = frame_receipt(frame, 3);
        let unavailable = PresentationOutputArtifact::new(
            display_artifact(receipt, 2, &SMALL_PIXELS),
            timing(receipt),
            available_topology(1, 4),
            unavailable_topology(2),
        )
        .unwrap();
        assert_eq!(unavailable.route_stability(), RouteStability::Unavailable);

        // The layout test accounts for every byte as a nested object or length lane; changing the
        // derived enum cannot change either canonical payload.
        assert_eq!(
            stable.encoded_len(),
            stable.encode_canonical().unwrap().len()
        );
    }

    #[test]
    fn exact_outer_cap_accepts_the_boundary_and_rejects_one_more_byte() {
        let topology_len = unavailable_topology(1)
            .try_encode_canonical()
            .unwrap()
            .len();
        let display_len = MAX_PRESENTATION_OUTPUT_ARTIFACT_BYTES
            - PRESENTATION_OUTPUT_ARTIFACT_FIXED_BYTES
            - 2 * topology_len;
        assert_eq!(
            checked_payload_len(display_len, topology_len, topology_len),
            Ok(MAX_PRESENTATION_OUTPUT_ARTIFACT_BYTES)
        );
        assert_eq!(
            checked_payload_len(display_len + 1, topology_len, topology_len),
            Err(PresentationOutputArtifactError::PayloadTooLarge {
                actual: MAX_PRESENTATION_OUTPUT_ARTIFACT_BYTES + 1,
                maximum: MAX_PRESENTATION_OUTPUT_ARTIFACT_BYTES,
            })
        );
    }

    #[test]
    fn individually_valid_near_cap_display_is_refused_before_outer_materialization() {
        const PIXELS: usize = 2_097_000;
        let pixels = vec![0_u32; PIXELS];
        let frame = FrameRef::new(PIXELS as u32, 1, &pixels).unwrap();
        let receipt = frame_receipt(frame, 70);
        let display = display_artifact(receipt, PIXELS as u32, &pixels);
        assert!(display.encoded_len() <= MAX_STORAGE_PAYLOAD_BYTES);

        let error = PresentationOutputArtifact::new(
            display,
            timing(receipt),
            unavailable_topology(1),
            unavailable_topology(2),
        )
        .unwrap_err();
        assert!(matches!(
            error,
            PresentationOutputArtifactError::PayloadTooLarge {
                maximum: MAX_PRESENTATION_OUTPUT_ARTIFACT_BYTES,
                ..
            }
        ));
    }
}
