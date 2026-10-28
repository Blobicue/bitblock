//! Version-two presentation-output carrier with raw per-route output-color observations.
//!
//! V1 remains nested as its exact canonical bytes. This wrapper only adds raw Windows Advanced
//! Color reports collected with the pre- and post-capture topology observations. It does not
//! establish HDR presentation, DWM transform order, scanout, panel behavior, emitted light, or
//! physical-pixel correctness.

use std::fmt;

use crate::output_color::{OutputColorObservation, OutputColorObservationError};
use crate::presentation_output::{
    PresentationOutputArtifact, PresentationOutputArtifactError, RouteStability,
};
use crate::protocol::{ContentTag, TagError, TagKind, hash_canonical};
use crate::storage::MAX_STORAGE_PAYLOAD_BYTES;

/// Schema of the additive v2 wrapper. The tag kind deliberately remains presentation-output.
pub const PRESENTATION_OUTPUT_ARTIFACT_V2_SCHEMA: &[u8] =
    b"bitblock/presentation-output-artifact/v2";
/// Maximum canonical payload size in bytes for a `PresentationOutputArtifactV2`.
pub const MAX_PRESENTATION_OUTPUT_ARTIFACT_V2_BYTES: usize = MAX_STORAGE_PAYLOAD_BYTES;

const LENGTH_BYTES: usize = 8;
const FIXED_BYTES: usize = LENGTH_BYTES * 3;

/// Comparison derived only from complete raw configuration observations on stable topology routes.
///
/// This is deliberately absent from the canonical payload and does not label a mode as HDR/WCG.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReportedColorState {
    Stable,
    Changed,
    Unavailable,
}

/// Immutable v1 evidence plus exactly bound pre/post raw output-color observations.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PresentationOutputArtifactV2 {
    tag: ContentTag,
    encoded_len: usize,
    v1: PresentationOutputArtifact,
    pre_color: OutputColorObservation,
    post_color: OutputColorObservation,
}

impl PresentationOutputArtifactV2 {
    /// Constructs and validates a v2 artifact from v1 and pre/post color observations.
    pub fn new(
        v1: PresentationOutputArtifact,
        pre_color: OutputColorObservation,
        post_color: OutputColorObservation,
    ) -> Result<Self, PresentationOutputArtifactV2Error> {
        let mut artifact = Self {
            tag: ContentTag::from_digest(TagKind::PresentationOutputArtifact, [0; 32]),
            encoded_len: 0,
            v1,
            pre_color,
            post_color,
        };
        let parts = artifact.prepare_parts()?;
        artifact.tag = tag_parts(&parts)?;
        artifact.encoded_len = parts.total_len;
        Ok(artifact)
    }

    /// Returns the canonical content tag of this v2 presentation-output artifact.
    pub const fn tag(&self) -> ContentTag {
        self.tag
    }

    /// The byte-for-byte retained immutable v1 carrier.
    pub const fn v1(&self) -> &PresentationOutputArtifact {
        &self.v1
    }

    /// Borrows the raw pre-capture `OutputColorObservation`.
    pub const fn pre_color(&self) -> &OutputColorObservation {
        &self.pre_color
    }

    /// Borrows the raw post-capture `OutputColorObservation`.
    pub const fn post_color(&self) -> &OutputColorObservation {
        &self.post_color
    }

    /// Returns the canonical payload byte length of this v2 artifact.
    pub const fn encoded_len(&self) -> usize {
        self.encoded_len
    }

    /// Derives a raw configuration comparison only where both route sets are complete and aligned.
    pub fn reported_color_state(&self) -> ReportedColorState {
        if self.v1.route_stability() != RouteStability::Stable {
            return ReportedColorState::Unavailable;
        }
        let (Some(pre), Some(post)) = (self.pre_color.routes(), self.post_color.routes()) else {
            return ReportedColorState::Unavailable;
        };
        if pre.len() != post.len()
            || pre.iter().any(|fact| {
                !matches!(
                    fact,
                    crate::output_color::AdvancedColorRouteFacts::Available { .. }
                )
            })
            || post.iter().any(|fact| {
                !matches!(
                    fact,
                    crate::output_color::AdvancedColorRouteFacts::Available { .. }
                )
            })
        {
            return ReportedColorState::Unavailable;
        }
        if pre == post {
            ReportedColorState::Stable
        } else {
            ReportedColorState::Changed
        }
    }

    /// Encodes this v2 presentation-output artifact into its canonical payload byte sequence.
    pub fn encode_canonical(&self) -> Result<Vec<u8>, PresentationOutputArtifactV2Error> {
        let parts = self.prepare_parts()?;
        let mut bytes = Vec::new();
        bytes.try_reserve_exact(parts.total_len).map_err(|_| {
            PresentationOutputArtifactV2Error::AllocationFailed {
                requested: parts.total_len,
            }
        })?;
        feed_parts(&parts, &mut |chunk| bytes.extend_from_slice(chunk));
        debug_assert_eq!(bytes.len(), parts.total_len);
        Ok(bytes)
    }

    /// Decodes and validates a canonical `PresentationOutputArtifactV2` payload.
    pub fn decode_canonical(bytes: &[u8]) -> Result<Self, PresentationOutputArtifactV2Error> {
        if bytes.len() > MAX_PRESENTATION_OUTPUT_ARTIFACT_V2_BYTES {
            return Err(PresentationOutputArtifactV2Error::PayloadTooLarge {
                actual: bytes.len(),
                maximum: MAX_PRESENTATION_OUTPUT_ARTIFACT_V2_BYTES,
            });
        }
        let mut reader = Reader::new(bytes);
        let v1_len = reader.length("v1 presentation-output length")?;
        let v1_bytes = reader.take("v1 presentation-output", v1_len)?;
        let pre_color_len = reader.length("pre-capture output-color length")?;
        let pre_color_bytes = reader.take("pre-capture output-color", pre_color_len)?;
        let post_color_len = reader.length("post-capture output-color length")?;
        let post_color_bytes = reader.take("post-capture output-color", post_color_len)?;
        reader.finish()?;
        checked_payload_len(v1_len, pre_color_len, post_color_len)?;

        let v1 = PresentationOutputArtifact::decode_canonical(v1_bytes)
            .map_err(PresentationOutputArtifactV2Error::InvalidV1)?;
        let pre_color = OutputColorObservation::decode_canonical(pre_color_bytes)
            .map_err(PresentationOutputArtifactV2Error::InvalidPreColor)?;
        let post_color = OutputColorObservation::decode_canonical(post_color_bytes)
            .map_err(PresentationOutputArtifactV2Error::InvalidPostColor)?;
        let artifact = Self::new(v1, pre_color, post_color)?;
        if !artifact.payload_matches(bytes)? {
            return Err(PresentationOutputArtifactV2Error::NonCanonicalEncoding);
        }
        Ok(artifact)
    }

    /// Re-verifies nested v1/color carriers, canonical payload length, and content tag.
    pub fn verify(&self) -> Result<(), PresentationOutputArtifactV2Error> {
        let parts = self.prepare_parts()?;
        if self.tag.kind() != TagKind::PresentationOutputArtifact {
            return Err(PresentationOutputArtifactV2Error::WrongTagKind {
                actual: self.tag.kind(),
            });
        }
        if self.tag != tag_parts(&parts)? {
            return Err(PresentationOutputArtifactV2Error::ArtifactTagMismatch);
        }
        if self.encoded_len != parts.total_len {
            return Err(PresentationOutputArtifactV2Error::NonCanonicalEncoding);
        }
        Ok(())
    }

    fn prepare_parts(&self) -> Result<EncodedParts, PresentationOutputArtifactV2Error> {
        self.v1
            .verify()
            .map_err(PresentationOutputArtifactV2Error::InvalidV1)?;
        self.pre_color
            .verify_against(self.v1.pre_topology())
            .map_err(PresentationOutputArtifactV2Error::InvalidPreColor)?;
        self.post_color
            .verify_against(self.v1.post_topology())
            .map_err(PresentationOutputArtifactV2Error::InvalidPostColor)?;

        let v1_len = self.v1.encoded_len();
        let pre_color = self
            .pre_color
            .encode_canonical()
            .map_err(PresentationOutputArtifactV2Error::InvalidPreColor)?;
        let post_color = self
            .post_color
            .encode_canonical()
            .map_err(PresentationOutputArtifactV2Error::InvalidPostColor)?;
        let total_len = checked_payload_len(v1_len, pre_color.len(), post_color.len())?;

        let v1 = self
            .v1
            .encode_canonical()
            .map_err(PresentationOutputArtifactV2Error::InvalidV1)?;
        debug_assert_eq!(v1.len(), v1_len);
        Ok(EncodedParts {
            v1,
            pre_color,
            post_color,
            total_len,
        })
    }

    fn payload_matches(&self, expected: &[u8]) -> Result<bool, PresentationOutputArtifactV2Error> {
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
    v1: Vec<u8>,
    pre_color: Vec<u8>,
    post_color: Vec<u8>,
    total_len: usize,
}

fn checked_payload_len(
    v1_len: usize,
    pre_color_len: usize,
    post_color_len: usize,
) -> Result<usize, PresentationOutputArtifactV2Error> {
    let total = FIXED_BYTES
        .checked_add(v1_len)
        .and_then(|value| value.checked_add(pre_color_len))
        .and_then(|value| value.checked_add(post_color_len))
        .ok_or(PresentationOutputArtifactV2Error::LengthOverflow)?;
    if total > MAX_PRESENTATION_OUTPUT_ARTIFACT_V2_BYTES {
        return Err(PresentationOutputArtifactV2Error::PayloadTooLarge {
            actual: total,
            maximum: MAX_PRESENTATION_OUTPUT_ARTIFACT_V2_BYTES,
        });
    }
    Ok(total)
}

fn feed_parts(parts: &EncodedParts, sink: &mut dyn FnMut(&[u8])) {
    sink(&(parts.v1.len() as u64).to_be_bytes());
    sink(&parts.v1);
    sink(&(parts.pre_color.len() as u64).to_be_bytes());
    sink(&parts.pre_color);
    sink(&(parts.post_color.len() as u64).to_be_bytes());
    sink(&parts.post_color);
}

fn tag_parts(parts: &EncodedParts) -> Result<ContentTag, PresentationOutputArtifactV2Error> {
    hash_canonical(
        TagKind::PresentationOutputArtifact,
        PRESENTATION_OUTPUT_ARTIFACT_V2_SCHEMA,
        parts.total_len as u64,
        |sink| feed_parts(parts, sink),
    )
    .map_err(PresentationOutputArtifactV2Error::Hash)
}

struct Reader<'a> {
    bytes: &'a [u8],
    cursor: usize,
}

impl<'a> Reader<'a> {
    const fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, cursor: 0 }
    }

    fn length(&mut self, field: &'static str) -> Result<usize, PresentationOutputArtifactV2Error> {
        let bytes = self.take(field, LENGTH_BYTES)?;
        let value = u64::from_be_bytes(bytes.try_into().expect("fixed length lane"));
        usize::try_from(value)
            .map_err(|_| PresentationOutputArtifactV2Error::DeclaredLengthOverflow { field, value })
    }

    fn take(
        &mut self,
        field: &'static str,
        length: usize,
    ) -> Result<&'a [u8], PresentationOutputArtifactV2Error> {
        let remaining = self.bytes.len().saturating_sub(self.cursor);
        if length > remaining {
            return Err(PresentationOutputArtifactV2Error::Truncated {
                field,
                needed: length,
                remaining,
            });
        }
        let end = self
            .cursor
            .checked_add(length)
            .ok_or(PresentationOutputArtifactV2Error::LengthOverflow)?;
        let result = &self.bytes[self.cursor..end];
        self.cursor = end;
        Ok(result)
    }

    fn finish(self) -> Result<(), PresentationOutputArtifactV2Error> {
        if self.cursor == self.bytes.len() {
            Ok(())
        } else {
            Err(PresentationOutputArtifactV2Error::TrailingBytes {
                count: self.bytes.len() - self.cursor,
            })
        }
    }
}

/// Error returned when constructing, coding, or verifying a `PresentationOutputArtifactV2`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PresentationOutputArtifactV2Error {
    InvalidV1(PresentationOutputArtifactError),
    InvalidPreColor(OutputColorObservationError),
    InvalidPostColor(OutputColorObservationError),
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

impl fmt::Display for PresentationOutputArtifactV2Error {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidV1(source) => {
                write!(formatter, "invalid nested v1 presentation output: {source}")
            }
            Self::InvalidPreColor(source) => write!(
                formatter,
                "invalid pre-capture output-color observation: {source}"
            ),
            Self::InvalidPostColor(source) => write!(
                formatter,
                "invalid post-capture output-color observation: {source}"
            ),
            Self::WrongTagKind { actual } => write!(
                formatter,
                "presentation-output v2 has {} tag kind instead of presentation-output-artifact",
                actual.name()
            ),
            Self::PayloadTooLarge { actual, maximum } => write!(
                formatter,
                "presentation-output v2 requires {actual} bytes, exceeding {maximum}"
            ),
            Self::LengthOverflow => formatter.write_str("presentation-output v2 length overflowed"),
            Self::DeclaredLengthOverflow { field, value } => {
                write!(formatter, "{field} declares unrepresentable length {value}")
            }
            Self::Truncated {
                field,
                needed,
                remaining,
            } => write!(
                formatter,
                "presentation-output v2 {field} needs {needed} bytes but only {remaining} remain"
            ),
            Self::TrailingBytes { count } => {
                write!(
                    formatter,
                    "presentation-output v2 has {count} trailing bytes"
                )
            }
            Self::AllocationFailed { requested } => {
                write!(
                    formatter,
                    "could not allocate {requested} presentation-output v2 bytes"
                )
            }
            Self::NonCanonicalEncoding => {
                formatter.write_str("presentation-output v2 bytes are not canonical")
            }
            Self::ArtifactTagMismatch => {
                formatter.write_str("presentation-output v2 tag does not match canonical payload")
            }
            Self::Hash(source) => write!(formatter, "presentation-output v2 hash failed: {source}"),
        }
    }
}

impl std::error::Error for PresentationOutputArtifactV2Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::InvalidV1(source) => Some(source),
            Self::InvalidPreColor(source) | Self::InvalidPostColor(source) => Some(source),
            Self::Hash(source) => Some(source),
            Self::WrongTagKind { .. }
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::display::{CaptureStage, DisplayCaptureFacts, DisplayLedgerV2};
    use crate::display_artifact::DisplayCaptureArtifact;
    use crate::frame::FrameRef;
    use crate::frame_v2::{FrameReceiptV2, FrameStream};
    use crate::output_color::AdvancedColorRouteFacts;
    use crate::output_topology::{
        AcquisitionAttempt, AcquisitionFacts, OutputTopologyObservation, PixelSize, RawRational,
        RectI32, SourceFacts, SourceModeFacts, TargetIdentityObservation, TargetRouteFacts,
        TargetSignalTiming, Utf16Text,
    };
    use crate::presentation::{
        CompositionTimingEnvelope, DwmFlushCall, DwmTimingCall, GdiFlushCall, PresenterEvidence,
        QpcBracket, QpcFrequency, QpcRead,
    };

    const PIXELS: [u32; 2] = [0x0011_2233, 0x0044_5566];

    fn test_tag(kind: TagKind, lane: u8) -> ContentTag {
        hash_canonical(
            kind,
            b"bitblock/presentation-output-v2-test/v1",
            1,
            |sink| sink(&[lane]),
        )
        .unwrap()
    }

    fn frame_receipt(frame: FrameRef<'_>) -> FrameReceiptV2 {
        FrameReceiptV2::new(
            None,
            FrameStream::Composite,
            test_tag(TagKind::TickOutcome, 1),
            test_tag(TagKind::FrameInput, 2),
            frame.content_tag(),
        )
        .unwrap()
    }

    fn display_artifact(receipt: FrameReceiptV2) -> DisplayCaptureArtifact {
        let expected = FrameRef::new(2, 1, &PIXELS).unwrap();
        let mut ledger = DisplayLedgerV2::default();
        let display_receipt = *ledger
            .append(
                receipt,
                2,
                1,
                DisplayCaptureFacts::unavailable(CaptureStage::FlushBefore, 5),
            )
            .unwrap();
        DisplayCaptureArtifact::new(display_receipt, expected, None).unwrap()
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
                policy: test_tag(TagKind::Policy, 3),
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

    fn topology(seed: u32) -> OutputTopologyObservation {
        OutputTopologyObservation::new_available(
            acquisition(seed),
            SourceFacts {
                gdi_device_name: Utf16Text::from_units(&[b'D' as u16]).unwrap(),
                client_rect: RectI32 {
                    right: 2,
                    bottom: 1,
                    ..RectI32::default()
                },
                monitor_rect: RectI32 {
                    right: 1920,
                    bottom: 1080,
                    ..RectI32::default()
                },
                work_rect: RectI32 {
                    right: 1920,
                    bottom: 1040,
                    ..RectI32::default()
                },
                monitor_flags_raw: 1,
                mode: SourceModeFacts {
                    width: 1920,
                    height: 1080,
                    ..SourceModeFacts::default()
                },
                status_flags_raw: 0,
            },
            vec![TargetRouteFacts {
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
                path_flags_raw: 4,
                signal: TargetSignalTiming {
                    pixel_rate: 1,
                    h_sync: RawRational {
                        numerator: 1,
                        denominator: 1,
                    },
                    v_sync: RawRational {
                        numerator: 1,
                        denominator: 1,
                    },
                    active_size: PixelSize {
                        width: 1,
                        height: 1,
                    },
                    total_size: PixelSize {
                        width: 1,
                        height: 1,
                    },
                    video_standard_raw: 0,
                    scanline_ordering_raw: 0,
                },
                desktop_image: None,
            }],
        )
        .unwrap()
    }

    fn v1() -> PresentationOutputArtifact {
        let frame = FrameRef::new(2, 1, &PIXELS).unwrap();
        let receipt = frame_receipt(frame);
        PresentationOutputArtifact::new(
            display_artifact(receipt),
            timing(receipt),
            topology(1),
            topology(2),
        )
        .unwrap()
    }

    fn color(topology: &OutputTopologyObservation, flags: u32) -> OutputColorObservation {
        OutputColorObservation::new_available(
            topology,
            vec![AdvancedColorRouteFacts::Available {
                flags_raw: flags,
                color_encoding_raw: 0xffff_fffe,
                bits_per_color_channel: 10,
            }],
        )
        .unwrap()
    }

    fn small() -> PresentationOutputArtifactV2 {
        let v1 = v1();
        let pre = color(v1.pre_topology(), 1);
        let post = color(v1.post_topology(), 1);
        PresentationOutputArtifactV2::new(v1, pre, post).unwrap()
    }

    fn offsets(bytes: &[u8]) -> (usize, usize, usize, usize, usize) {
        let v1_len = usize::try_from(u64::from_be_bytes(bytes[..8].try_into().unwrap())).unwrap();
        let v1_start = LENGTH_BYTES;
        let pre_len_offset = v1_start + v1_len;
        let pre_len = usize::try_from(u64::from_be_bytes(
            bytes[pre_len_offset..pre_len_offset + LENGTH_BYTES]
                .try_into()
                .unwrap(),
        ))
        .unwrap();
        let pre_start = pre_len_offset + LENGTH_BYTES;
        let post_len_offset = pre_start + pre_len;
        let post_start = post_len_offset + LENGTH_BYTES;
        (
            v1_start,
            pre_len_offset,
            pre_start,
            post_len_offset,
            post_start,
        )
    }

    #[test]
    fn exact_v1_nested_layout_round_trips_and_derives_only_raw_state() {
        let artifact = small();
        let bytes = artifact.encode_canonical().unwrap();
        let v1 = artifact.v1().encode_canonical().unwrap();
        let pre = artifact.pre_color().encode_canonical().unwrap();
        let post = artifact.post_color().encode_canonical().unwrap();
        let (v1_start, pre_len_offset, pre_start, post_len_offset, post_start) = offsets(&bytes);
        assert_eq!(&bytes[v1_start..pre_len_offset], v1.as_slice());
        assert_eq!(&bytes[pre_start..post_len_offset], pre.as_slice());
        assert_eq!(&bytes[post_start..], post.as_slice());
        assert_eq!(bytes.len(), v1.len() + pre.len() + post.len() + FIXED_BYTES);
        assert_eq!(bytes.len(), artifact.encoded_len());
        assert_eq!(artifact.reported_color_state(), ReportedColorState::Stable);

        let decoded = PresentationOutputArtifactV2::decode_canonical(&bytes).unwrap();
        assert_eq!(decoded, artifact);
        assert_eq!(decoded.tag().kind(), TagKind::PresentationOutputArtifact);
        decoded.verify().unwrap();
    }

    #[test]
    fn raw_state_changed_and_unavailable_are_not_relabelled() {
        let v1 = v1();
        let changed = PresentationOutputArtifactV2::new(
            v1.clone(),
            color(v1.pre_topology(), 1),
            color(v1.post_topology(), 2),
        )
        .unwrap();
        assert_eq!(changed.reported_color_state(), ReportedColorState::Changed);

        let unavailable = PresentationOutputArtifactV2::new(
            v1.clone(),
            color(v1.pre_topology(), 1),
            OutputColorObservation::new_unavailable(v1.post_topology(), 9, 87).unwrap(),
        )
        .unwrap();
        assert_eq!(
            unavailable.reported_color_state(),
            ReportedColorState::Unavailable
        );
    }

    #[test]
    fn color_parent_binding_and_nested_mutations_fail_closed() {
        let v1 = v1();
        let error = PresentationOutputArtifactV2::new(
            v1.clone(),
            color(v1.pre_topology(), 1),
            color(v1.pre_topology(), 1),
        )
        .unwrap_err();
        assert!(matches!(
            error,
            PresentationOutputArtifactV2Error::InvalidPostColor(
                OutputColorObservationError::ParentTagMismatch { .. }
            )
        ));

        let bytes = small().encode_canonical().unwrap();
        let (v1_start, _, pre_start, _, post_start) = offsets(&bytes);
        for end in 0..bytes.len() {
            assert!(
                PresentationOutputArtifactV2::decode_canonical(&bytes[..end]).is_err(),
                "accepted prefix ending at {end}"
            );
        }
        let mut v1_mutation = bytes.clone();
        v1_mutation[v1_start] ^= 1;
        assert!(matches!(
            PresentationOutputArtifactV2::decode_canonical(&v1_mutation),
            Err(PresentationOutputArtifactV2Error::InvalidV1(_))
        ));
        let mut pre_mutation = bytes.clone();
        pre_mutation[pre_start] ^= 1;
        assert!(matches!(
            PresentationOutputArtifactV2::decode_canonical(&pre_mutation),
            Err(PresentationOutputArtifactV2Error::InvalidPreColor(_))
        ));
        let mut post_mutation = bytes;
        post_mutation[post_start] ^= 1;
        assert!(matches!(
            PresentationOutputArtifactV2::decode_canonical(&post_mutation),
            Err(PresentationOutputArtifactV2Error::InvalidPostColor(_))
        ));
    }
}
