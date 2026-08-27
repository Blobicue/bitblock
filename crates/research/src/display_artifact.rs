//! Canonical, self-contained bytes for one typed display-capture attempt.
//!
//! An artifact retains the submitted composite frame, the exact captured pixels when available,
//! and the [`DisplayReceiptV2`] that binds their comparison facts. It is an in-memory protocol
//! object. Encoding an artifact does not persist it, establish external custody, prove sudden-
//! power durability, or strengthen a GDI desktop-raster sample into scanout or photon evidence.

use std::fmt;

use crate::display::{
    CaptureStage, DISPLAY_RECEIPT_V2_PAYLOAD_LEN, DisplayOutcome, DisplayProtocolError,
    DisplayReceiptV2,
};
use crate::frame::{CANONICAL_FRAME_DOMAIN, FrameObservation, FrameRef, FrameShapeError};
use crate::protocol::{ContentTag, TagError, TagKind, hash_canonical};
use crate::storage::MAX_STORAGE_PAYLOAD_BYTES;

pub const DISPLAY_CAPTURE_ARTIFACT_SCHEMA: &[u8] = b"bitblock/display-capture-artifact/v1";
pub const MAX_DISPLAY_CAPTURE_ARTIFACT_BYTES: usize = MAX_STORAGE_PAYLOAD_BYTES;

const LENGTH_BYTES: usize = 8;
const FORM_BYTES: usize = 1;
const FIXED_PAYLOAD_BYTES: usize =
    DISPLAY_RECEIPT_V2_PAYLOAD_LEN + LENGTH_BYTES + FORM_BYTES + LENGTH_BYTES;
const FRAME_FIXED_BYTES: usize = 2 + CANONICAL_FRAME_DOMAIN.len() + 4 + 4 + 8;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum CapturedPixelForm {
    Absent = 0,
    ReuseExpected = 1,
    Inline = 2,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum CapturedPixels {
    Absent,
    ReuseExpected,
    Inline(Vec<u32>),
}

/// One canonical display-capture object suitable for a later single-record journal append.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DisplayCaptureArtifact {
    tag: ContentTag,
    receipt: DisplayReceiptV2,
    expected_pixels: Vec<u32>,
    captured_pixels: CapturedPixels,
}

impl DisplayCaptureArtifact {
    pub fn new(
        receipt: DisplayReceiptV2,
        expected: FrameRef<'_>,
        captured: Option<FrameRef<'_>>,
    ) -> Result<Self, DisplayArtifactError> {
        receipt
            .verify()
            .map_err(DisplayArtifactError::InvalidDisplayReceipt)?;
        let (width, height) = receipt.expected_viewport();
        if (expected.width(), expected.height()) != (width, height) {
            return Err(DisplayArtifactError::ViewportMismatch {
                field: "expected frame",
                expected: (width, height),
                actual: (expected.width(), expected.height()),
            });
        }
        let facts = receipt.facts();
        if facts.captured.is_some() != captured.is_some() {
            return Err(DisplayArtifactError::CapturedPresenceMismatch {
                facts_present: facts.captured.is_some(),
                pixels_present: captured.is_some(),
            });
        }
        if let Some(captured) = captured
            && (captured.width(), captured.height()) != (width, height)
        {
            return Err(DisplayArtifactError::ViewportMismatch {
                field: "captured frame",
                expected: (width, height),
                actual: (captured.width(), captured.height()),
            });
        }
        let (captured_form, source_payload_len) = preflight_source_frames(expected, captured)?;

        let expected_observation = expected.observe();
        if expected_observation.facts.invalid_high_byte != 0 {
            return Err(DisplayArtifactError::InvalidHighByte {
                field: "expected frame",
                count: expected_observation.facts.invalid_high_byte,
            });
        }
        if expected_observation.tag != receipt.expected_frame() {
            return Err(DisplayArtifactError::FrameTagMismatch {
                field: "expected frame",
                expected: receipt.expected_frame(),
                actual: expected_observation.tag,
            });
        }

        let captured_observation = if let Some(captured) = captured {
            let observation = captured.observe();
            if observation.facts.invalid_high_byte != 0 {
                return Err(DisplayArtifactError::InvalidHighByte {
                    field: "captured frame",
                    count: observation.facts.invalid_high_byte,
                });
            }
            if Some(observation) != facts.captured {
                return Err(DisplayArtifactError::CapturedObservationMismatch {
                    expected: facts.captured.map(|observation| observation.tag),
                    actual: Some(observation.tag),
                });
            }
            Some(observation)
        } else {
            None
        };

        if facts.stage == CaptureStage::Complete {
            let captured = captured.ok_or(DisplayArtifactError::CapturedPresenceMismatch {
                facts_present: true,
                pixels_present: false,
            })?;
            let mut mismatch_count = 0_u64;
            let mut first_mismatch = None;
            for (index, (&wanted, &seen)) in
                expected.pixels().iter().zip(captured.pixels()).enumerate()
            {
                if wanted != seen {
                    mismatch_count += 1;
                    first_mismatch.get_or_insert(index as u64);
                }
            }
            let outcome = if mismatch_count == 0 {
                DisplayOutcome::Match
            } else {
                DisplayOutcome::Mismatch
            };
            if (facts.outcome, facts.mismatch_count, facts.first_mismatch)
                != (outcome, mismatch_count, first_mismatch)
            {
                return Err(DisplayArtifactError::ComparisonFactsMismatch {
                    expected_outcome: facts.outcome,
                    actual_outcome: outcome,
                    expected_count: facts.mismatch_count,
                    actual_count: mismatch_count,
                    expected_first: facts.first_mismatch,
                    actual_first: first_mismatch,
                });
            }
        } else if facts.outcome != DisplayOutcome::Unavailable {
            return Err(DisplayArtifactError::InvalidDisplayReceipt(
                DisplayProtocolError::InconsistentFacts,
            ));
        }

        let expected_pixels = clone_pixels("expected frame", expected.pixels())?;
        let captured_pixels = match (captured_form, captured) {
            (CapturedPixelForm::Absent, None) => CapturedPixels::Absent,
            (CapturedPixelForm::ReuseExpected, Some(_)) => CapturedPixels::ReuseExpected,
            (CapturedPixelForm::Inline, Some(captured)) => {
                CapturedPixels::Inline(clone_pixels("captured frame", captured.pixels())?)
            }
            _ => unreachable!("source-frame preflight fixes captured presence and form"),
        };
        let mut artifact = Self {
            tag: ContentTag::from_digest(TagKind::DisplayArtifact, [0; 32]),
            receipt,
            expected_pixels,
            captured_pixels,
        };
        debug_assert_eq!(artifact.preflight_payload_len(), Ok(source_payload_len));
        artifact.tag = artifact.recompute_tag()?;
        debug_assert_eq!(artifact.captured_observation(), captured_observation);
        Ok(artifact)
    }

    pub const fn tag(&self) -> ContentTag {
        self.tag
    }

    pub const fn receipt(&self) -> DisplayReceiptV2 {
        self.receipt
    }

    pub fn expected_pixels(&self) -> &[u32] {
        &self.expected_pixels
    }

    pub fn captured_pixels(&self) -> Option<&[u32]> {
        match &self.captured_pixels {
            CapturedPixels::Absent => None,
            CapturedPixels::ReuseExpected => Some(&self.expected_pixels),
            CapturedPixels::Inline(pixels) => Some(pixels),
        }
    }

    pub const fn captured_form(&self) -> CapturedPixelForm {
        match self.captured_pixels {
            CapturedPixels::Absent => CapturedPixelForm::Absent,
            CapturedPixels::ReuseExpected => CapturedPixelForm::ReuseExpected,
            CapturedPixels::Inline(_) => CapturedPixelForm::Inline,
        }
    }

    pub fn encoded_len(&self) -> usize {
        self.preflight_payload_len()
            .expect("admitted display artifact length remains valid")
    }

    pub fn encode_canonical(&self) -> Result<Vec<u8>, DisplayArtifactError> {
        let length = self.preflight_payload_len()?;
        let mut encoded = Vec::new();
        encoded
            .try_reserve_exact(length)
            .map_err(|_| DisplayArtifactError::AllocationFailed {
                field: "artifact payload",
                requested: length,
            })?;
        self.feed_payload(&mut |bytes| encoded.extend_from_slice(bytes));
        debug_assert_eq!(encoded.len(), length);
        Ok(encoded)
    }

    pub fn decode_canonical(bytes: &[u8]) -> Result<Self, DisplayArtifactError> {
        if bytes.len() > MAX_DISPLAY_CAPTURE_ARTIFACT_BYTES {
            return Err(DisplayArtifactError::PayloadTooLarge {
                actual: bytes.len(),
                max: MAX_DISPLAY_CAPTURE_ARTIFACT_BYTES,
            });
        }
        if bytes.len() < DISPLAY_RECEIPT_V2_PAYLOAD_LEN + LENGTH_BYTES {
            return Err(DisplayArtifactError::Truncated {
                needed: DISPLAY_RECEIPT_V2_PAYLOAD_LEN + LENGTH_BYTES,
                actual: bytes.len(),
            });
        }
        let receipt = DisplayReceiptV2::decode_canonical(&bytes[..DISPLAY_RECEIPT_V2_PAYLOAD_LEN])
            .map_err(DisplayArtifactError::InvalidDisplayReceipt)?;
        let mut cursor = DISPLAY_RECEIPT_V2_PAYLOAD_LEN;
        let expected_len = decode_length("expected frame", &bytes[cursor..cursor + 8])?;
        cursor += 8;
        let expected_end =
            cursor
                .checked_add(expected_len)
                .ok_or(DisplayArtifactError::LengthOverflow {
                    field: "expected frame",
                })?;
        if expected_end > bytes.len() {
            return Err(DisplayArtifactError::Truncated {
                needed: expected_end,
                actual: bytes.len(),
            });
        }
        let expected = decode_frame("expected frame", &bytes[cursor..expected_end])?;
        cursor = expected_end;
        let metadata_end = cursor.checked_add(FORM_BYTES + LENGTH_BYTES).ok_or(
            DisplayArtifactError::LengthOverflow {
                field: "capture metadata",
            },
        )?;
        if metadata_end > bytes.len() {
            return Err(DisplayArtifactError::Truncated {
                needed: metadata_end,
                actual: bytes.len(),
            });
        }
        let declared_form = match bytes[cursor] {
            0 => CapturedPixelForm::Absent,
            1 => CapturedPixelForm::ReuseExpected,
            2 => CapturedPixelForm::Inline,
            form => return Err(DisplayArtifactError::UnknownCapturedForm { form }),
        };
        cursor += 1;
        let captured_len = decode_length("captured frame", &bytes[cursor..cursor + 8])?;
        cursor += 8;
        let captured_end =
            cursor
                .checked_add(captured_len)
                .ok_or(DisplayArtifactError::LengthOverflow {
                    field: "captured frame",
                })?;
        if captured_end > bytes.len() {
            return Err(DisplayArtifactError::Truncated {
                needed: captured_end,
                actual: bytes.len(),
            });
        }
        if captured_end != bytes.len() {
            return Err(DisplayArtifactError::TrailingBytes {
                count: bytes.len() - captured_end,
            });
        }

        let captured = match declared_form {
            CapturedPixelForm::Absent => {
                if captured_len != 0 {
                    return Err(DisplayArtifactError::NonCanonicalCapturedForm);
                }
                None
            }
            CapturedPixelForm::ReuseExpected => {
                if captured_len != 0 {
                    return Err(DisplayArtifactError::NonCanonicalCapturedForm);
                }
                Some(expected.as_frame())
            }
            CapturedPixelForm::Inline => {
                if captured_len == 0 {
                    return Err(DisplayArtifactError::NonCanonicalCapturedForm);
                }
                None
            }
        };
        let inline = if declared_form == CapturedPixelForm::Inline {
            Some(decode_frame(
                "captured frame",
                &bytes[cursor..captured_end],
            )?)
        } else {
            None
        };
        let captured = inline.as_ref().map(DecodedFrame::as_frame).or(captured);
        let artifact = Self::new(receipt, expected.as_frame(), captured)?;
        if artifact.captured_form() != declared_form {
            return Err(DisplayArtifactError::NonCanonicalCapturedForm);
        }
        if !artifact.payload_matches(bytes) {
            return Err(DisplayArtifactError::NonCanonicalEncoding);
        }
        Ok(artifact)
    }

    pub fn verify(&self) -> Result<(), DisplayArtifactError> {
        self.receipt
            .verify()
            .map_err(DisplayArtifactError::InvalidDisplayReceipt)?;
        let (width, height) = self.receipt.expected_viewport();
        let expected = FrameRef::new(width, height, &self.expected_pixels)
            .map_err(DisplayArtifactError::FrameShape)?;
        let captured = match &self.captured_pixels {
            CapturedPixels::Absent => None,
            CapturedPixels::ReuseExpected => Some(expected),
            CapturedPixels::Inline(pixels) => Some(
                FrameRef::new(width, height, pixels).map_err(DisplayArtifactError::FrameShape)?,
            ),
        };
        self.preflight_payload_len()?;
        if self.tag != self.recompute_tag()? {
            return Err(DisplayArtifactError::ArtifactTagMismatch);
        }
        let rebuilt = Self::new(self.receipt, expected, captured)?;
        if rebuilt.tag != self.tag || rebuilt.captured_form() != self.captured_form() {
            return Err(DisplayArtifactError::NonCanonicalEncoding);
        }
        Ok(())
    }

    fn expected_frame(&self) -> FrameRef<'_> {
        let (width, height) = self.receipt.expected_viewport();
        FrameRef::new(width, height, &self.expected_pixels)
            .expect("admitted display artifact retains the expected frame shape")
    }

    fn inline_frame(&self) -> Option<FrameRef<'_>> {
        let (width, height) = self.receipt.expected_viewport();
        match &self.captured_pixels {
            CapturedPixels::Inline(pixels) => Some(
                FrameRef::new(width, height, pixels)
                    .expect("admitted display artifact retains the captured frame shape"),
            ),
            CapturedPixels::Absent | CapturedPixels::ReuseExpected => None,
        }
    }

    fn captured_observation(&self) -> Option<FrameObservation> {
        self.captured_pixels().map(|pixels| {
            let (width, height) = self.receipt.expected_viewport();
            FrameRef::new(width, height, pixels).unwrap().observe()
        })
    }

    fn preflight_payload_len(&self) -> Result<usize, DisplayArtifactError> {
        checked_payload_len(
            self.expected_pixels.len(),
            match &self.captured_pixels {
                CapturedPixels::Inline(pixels) => Some(pixels.len()),
                CapturedPixels::Absent | CapturedPixels::ReuseExpected => None,
            },
        )
    }

    fn feed_payload(&self, sink: &mut dyn FnMut(&[u8])) {
        sink(&self.receipt.encode_canonical());
        let expected = self.expected_frame();
        sink(&(expected.canonical_len() as u64).to_be_bytes());
        expected.encode_canonical(&mut *sink);
        sink(&[self.captured_form() as u8]);
        let inline = self.inline_frame();
        sink(
            &inline
                .map_or(0_u64, |frame| frame.canonical_len() as u64)
                .to_be_bytes(),
        );
        if let Some(frame) = inline {
            frame.encode_canonical(&mut *sink);
        }
    }

    fn payload_matches(&self, expected: &[u8]) -> bool {
        let mut cursor = 0_usize;
        let mut exact = true;
        self.feed_payload(&mut |chunk| {
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
        exact && cursor == expected.len()
    }

    fn recompute_tag(&self) -> Result<ContentTag, DisplayArtifactError> {
        let length = self.preflight_payload_len()?;
        hash_canonical(
            TagKind::DisplayArtifact,
            DISPLAY_CAPTURE_ARTIFACT_SCHEMA,
            length as u64,
            |sink| self.feed_payload(sink),
        )
        .map_err(DisplayArtifactError::Hash)
    }
}

fn preflight_source_frames(
    expected: FrameRef<'_>,
    captured: Option<FrameRef<'_>>,
) -> Result<(CapturedPixelForm, usize), DisplayArtifactError> {
    let form = match captured {
        None => CapturedPixelForm::Absent,
        Some(captured) if captured.pixels() == expected.pixels() => {
            CapturedPixelForm::ReuseExpected
        }
        Some(_) => CapturedPixelForm::Inline,
    };
    let inline_pixels = match (form, captured) {
        (CapturedPixelForm::Inline, Some(captured)) => Some(captured.pixels().len()),
        _ => None,
    };
    Ok((
        form,
        checked_payload_len(expected.pixels().len(), inline_pixels)?,
    ))
}

fn checked_payload_len(
    expected_pixels: usize,
    inline_pixels: Option<usize>,
) -> Result<usize, DisplayArtifactError> {
    let expected_len = checked_frame_len("expected frame", expected_pixels)?;
    let inline_len = inline_pixels
        .map(|pixels| checked_frame_len("captured frame", pixels))
        .transpose()?
        .unwrap_or(0);
    let length = FIXED_PAYLOAD_BYTES
        .checked_add(expected_len)
        .and_then(|value| value.checked_add(inline_len))
        .ok_or(DisplayArtifactError::LengthOverflow {
            field: "artifact payload",
        })?;
    if length > MAX_DISPLAY_CAPTURE_ARTIFACT_BYTES {
        return Err(DisplayArtifactError::PayloadTooLarge {
            actual: length,
            max: MAX_DISPLAY_CAPTURE_ARTIFACT_BYTES,
        });
    }
    Ok(length)
}

fn checked_frame_len(
    field: &'static str,
    pixel_count: usize,
) -> Result<usize, DisplayArtifactError> {
    pixel_count
        .checked_mul(size_of::<u32>())
        .and_then(|bytes| FRAME_FIXED_BYTES.checked_add(bytes))
        .ok_or(DisplayArtifactError::LengthOverflow { field })
}

fn clone_pixels(field: &'static str, pixels: &[u32]) -> Result<Vec<u32>, DisplayArtifactError> {
    let mut cloned = Vec::new();
    cloned
        .try_reserve_exact(pixels.len())
        .map_err(|_| DisplayArtifactError::AllocationFailed {
            field,
            requested: pixels.len().saturating_mul(size_of::<u32>()),
        })?;
    cloned.extend_from_slice(pixels);
    Ok(cloned)
}

fn decode_length(field: &'static str, bytes: &[u8]) -> Result<usize, DisplayArtifactError> {
    let value = u64::from_be_bytes(bytes.try_into().expect("fixed length lane"));
    usize::try_from(value)
        .map_err(|_| DisplayArtifactError::DeclaredLengthOverflow { field, value })
}

struct DecodedFrame {
    width: u32,
    height: u32,
    pixels: Vec<u32>,
}

impl DecodedFrame {
    fn as_frame(&self) -> FrameRef<'_> {
        FrameRef::new(self.width, self.height, &self.pixels)
            .expect("strict canonical-frame decoding admitted its exact shape")
    }
}

fn decode_frame(field: &'static str, bytes: &[u8]) -> Result<DecodedFrame, DisplayArtifactError> {
    if bytes.len() < FRAME_FIXED_BYTES {
        return Err(DisplayArtifactError::InvalidCanonicalFrame {
            field,
            source: CanonicalFrameError::Truncated,
        });
    }
    let domain_len = usize::from(u16::from_be_bytes(bytes[0..2].try_into().unwrap()));
    if domain_len != CANONICAL_FRAME_DOMAIN.len() {
        return Err(DisplayArtifactError::InvalidCanonicalFrame {
            field,
            source: CanonicalFrameError::WrongDomainLength { actual: domain_len },
        });
    }
    if bytes[2..2 + domain_len] != *CANONICAL_FRAME_DOMAIN {
        return Err(DisplayArtifactError::InvalidCanonicalFrame {
            field,
            source: CanonicalFrameError::WrongDomain,
        });
    }
    let mut cursor = 2 + domain_len;
    let width = u32::from_be_bytes(bytes[cursor..cursor + 4].try_into().unwrap());
    cursor += 4;
    let height = u32::from_be_bytes(bytes[cursor..cursor + 4].try_into().unwrap());
    cursor += 4;
    let count = u64::from_be_bytes(bytes[cursor..cursor + 8].try_into().unwrap());
    cursor += 8;
    let shape_count = u64::from(width) * u64::from(height);
    if count != shape_count {
        return Err(DisplayArtifactError::InvalidCanonicalFrame {
            field,
            source: CanonicalFrameError::PixelCountMismatch {
                width,
                height,
                declared: count,
            },
        });
    }
    let count =
        usize::try_from(count).map_err(|_| DisplayArtifactError::DeclaredLengthOverflow {
            field,
            value: shape_count,
        })?;
    let expected_len = cursor
        .checked_add(
            count
                .checked_mul(size_of::<u32>())
                .ok_or(DisplayArtifactError::LengthOverflow { field })?,
        )
        .ok_or(DisplayArtifactError::LengthOverflow { field })?;
    if bytes.len() != expected_len {
        return Err(DisplayArtifactError::InvalidCanonicalFrame {
            field,
            source: CanonicalFrameError::WrongLength {
                expected: expected_len,
                actual: bytes.len(),
            },
        });
    }
    let mut pixels = Vec::new();
    pixels
        .try_reserve_exact(count)
        .map_err(|_| DisplayArtifactError::AllocationFailed {
            field,
            requested: count.saturating_mul(size_of::<u32>()),
        })?;
    pixels.extend(
        bytes[cursor..]
            .chunks_exact(4)
            .map(|chunk| u32::from_be_bytes(chunk.try_into().unwrap())),
    );
    Ok(DecodedFrame {
        width,
        height,
        pixels,
    })
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CanonicalFrameError {
    Truncated,
    WrongDomainLength {
        actual: usize,
    },
    WrongDomain,
    PixelCountMismatch {
        width: u32,
        height: u32,
        declared: u64,
    },
    WrongLength {
        expected: usize,
        actual: usize,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DisplayArtifactError {
    InvalidDisplayReceipt(DisplayProtocolError),
    FrameShape(FrameShapeError),
    ViewportMismatch {
        field: &'static str,
        expected: (u32, u32),
        actual: (u32, u32),
    },
    InvalidHighByte {
        field: &'static str,
        count: u64,
    },
    FrameTagMismatch {
        field: &'static str,
        expected: ContentTag,
        actual: ContentTag,
    },
    CapturedPresenceMismatch {
        facts_present: bool,
        pixels_present: bool,
    },
    CapturedObservationMismatch {
        expected: Option<ContentTag>,
        actual: Option<ContentTag>,
    },
    ComparisonFactsMismatch {
        expected_outcome: DisplayOutcome,
        actual_outcome: DisplayOutcome,
        expected_count: u64,
        actual_count: u64,
        expected_first: Option<u64>,
        actual_first: Option<u64>,
    },
    PayloadTooLarge {
        actual: usize,
        max: usize,
    },
    LengthOverflow {
        field: &'static str,
    },
    DeclaredLengthOverflow {
        field: &'static str,
        value: u64,
    },
    Truncated {
        needed: usize,
        actual: usize,
    },
    TrailingBytes {
        count: usize,
    },
    UnknownCapturedForm {
        form: u8,
    },
    NonCanonicalCapturedForm,
    InvalidCanonicalFrame {
        field: &'static str,
        source: CanonicalFrameError,
    },
    AllocationFailed {
        field: &'static str,
        requested: usize,
    },
    NonCanonicalEncoding,
    ArtifactTagMismatch,
    Hash(TagError),
}

impl fmt::Display for DisplayArtifactError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidDisplayReceipt(source) => {
                write!(formatter, "invalid display receipt: {source}")
            }
            Self::FrameShape(source) => write!(formatter, "invalid frame shape: {source}"),
            Self::ViewportMismatch {
                field,
                expected,
                actual,
            } => write!(
                formatter,
                "{field} viewport mismatch: expected {}x{}, found {}x{}",
                expected.0, expected.1, actual.0, actual.1
            ),
            Self::InvalidHighByte { field, count } => {
                write!(formatter, "{field} contains {count} non-XRGB high bytes")
            }
            Self::FrameTagMismatch {
                field,
                expected,
                actual,
            } => write!(
                formatter,
                "{field} tag mismatch: expected {expected}, found {actual}"
            ),
            Self::CapturedPresenceMismatch {
                facts_present,
                pixels_present,
            } => write!(
                formatter,
                "captured-pixel presence {pixels_present} disagrees with receipt facts {facts_present}"
            ),
            Self::CapturedObservationMismatch { .. } => {
                formatter.write_str("captured pixels disagree with the receipt observation")
            }
            Self::ComparisonFactsMismatch { .. } => {
                formatter.write_str("exact pixel comparison disagrees with the receipt facts")
            }
            Self::PayloadTooLarge { actual, max } => {
                write!(
                    formatter,
                    "display artifact has {actual} bytes; maximum is {max}"
                )
            }
            Self::LengthOverflow { field } => write!(formatter, "{field} length overflowed"),
            Self::DeclaredLengthOverflow { field, value } => {
                write!(formatter, "{field} declares unrepresentable length {value}")
            }
            Self::Truncated { needed, actual } => {
                write!(
                    formatter,
                    "display artifact needs {needed} bytes, found {actual}"
                )
            }
            Self::TrailingBytes { count } => {
                write!(formatter, "display artifact has {count} trailing bytes")
            }
            Self::UnknownCapturedForm { form } => {
                write!(formatter, "display artifact uses captured form {form}")
            }
            Self::NonCanonicalCapturedForm => {
                formatter.write_str("display artifact uses a noncanonical captured form")
            }
            Self::InvalidCanonicalFrame { field, source } => {
                write!(formatter, "invalid canonical {field}: {source:?}")
            }
            Self::AllocationFailed { field, requested } => {
                write!(
                    formatter,
                    "could not allocate {requested} bytes for {field}"
                )
            }
            Self::NonCanonicalEncoding => {
                formatter.write_str("display artifact encoding is not canonical")
            }
            Self::ArtifactTagMismatch => {
                formatter.write_str("display artifact tag does not match its canonical bytes")
            }
            Self::Hash(source) => write!(formatter, "display artifact hash failed: {source}"),
        }
    }
}

impl std::error::Error for DisplayArtifactError {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::display::{CaptureRect, DisplayCaptureFacts, DisplayLedgerV2, DisplayOutcome};
    use crate::frame_v2::{FrameReceiptV2, FrameStream};
    use crate::storage::{StorageError, StorageRecord};

    const EXPECTED_PIXELS: [u32; 2] = [0x0011_2233, 0x0044_5566];

    fn tag(kind: TagKind, label: &[u8]) -> ContentTag {
        hash_canonical(
            kind,
            b"bitblock/display-artifact-test/v1",
            label.len() as u64,
            |sink| sink(label),
        )
        .unwrap()
    }

    fn frame(width: u32, height: u32, pixels: &[u32]) -> FrameRef<'_> {
        FrameRef::new(width, height, pixels).unwrap()
    }

    fn frame_bytes(frame: FrameRef<'_>) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(frame.canonical_len());
        frame.encode_canonical(|chunk| bytes.extend_from_slice(chunk));
        bytes
    }

    fn expected_receipt(expected: FrameRef<'_>) -> FrameReceiptV2 {
        FrameReceiptV2::new(
            None,
            FrameStream::Composite,
            tag(TagKind::TickOutcome, b"outcome"),
            tag(TagKind::FrameInput, b"input"),
            expected.content_tag(),
        )
        .unwrap()
    }

    fn complete_facts(expected: FrameRef<'_>, captured: FrameRef<'_>) -> DisplayCaptureFacts {
        assert_eq!(
            (expected.width(), expected.height()),
            (captured.width(), captured.height())
        );
        let rect = CaptureRect {
            left: -20,
            top: 30,
            right: -20 + i32::try_from(expected.width()).unwrap(),
            bottom: 30 + i32::try_from(expected.height()).unwrap(),
        };
        let mut mismatch_count = 0_u64;
        let mut first_mismatch = None;
        for (index, (&wanted, &seen)) in expected.pixels().iter().zip(captured.pixels()).enumerate()
        {
            if wanted != seen {
                mismatch_count += 1;
                first_mismatch.get_or_insert(index as u64);
            }
        }
        DisplayCaptureFacts {
            outcome: if mismatch_count == 0 {
                DisplayOutcome::Match
            } else {
                DisplayOutcome::Mismatch
            },
            stage: CaptureStage::Complete,
            error_code: 0,
            flush_before: true,
            compositor_flush: true,
            flush_after: true,
            rect_before: rect,
            rect_after: rect,
            dpi: 144,
            qpc_start: 100,
            qpc_end: 120,
            qpc_frequency: 10_000_000,
            captured: Some(captured.observe()),
            mismatch_count,
            first_mismatch,
        }
    }

    fn receipt(expected: FrameRef<'_>, facts: DisplayCaptureFacts) -> DisplayReceiptV2 {
        *DisplayLedgerV2::default()
            .append(
                expected_receipt(expected),
                expected.width(),
                expected.height(),
                facts,
            )
            .unwrap()
    }

    fn complete_artifact(captured_pixels: &[u32]) -> DisplayCaptureArtifact {
        let expected = frame(2, 1, &EXPECTED_PIXELS);
        let captured = frame(2, 1, captured_pixels);
        DisplayCaptureArtifact::new(
            receipt(expected, complete_facts(expected, captured)),
            expected,
            Some(captured),
        )
        .unwrap()
    }

    fn unavailable_artifact(captured_pixels: Option<&[u32]>) -> DisplayCaptureArtifact {
        let expected = frame(2, 1, &EXPECTED_PIXELS);
        let captured = captured_pixels.map(|pixels| frame(2, 1, pixels));
        let facts = if let Some(captured) = captured {
            let mut facts = complete_facts(expected, captured);
            facts.outcome = DisplayOutcome::Unavailable;
            facts.stage = CaptureStage::GeometryAfter;
            facts.error_code = 5;
            facts.rect_after = CaptureRect {
                left: 0,
                top: 0,
                right: 0,
                bottom: 0,
            };
            facts.mismatch_count = 0;
            facts.first_mismatch = None;
            facts
        } else {
            DisplayCaptureFacts::unavailable(CaptureStage::FlushBefore, 5)
        };
        DisplayCaptureArtifact::new(receipt(expected, facts), expected, captured).unwrap()
    }

    fn captured_form_offset(encoded: &[u8]) -> usize {
        let expected_len = u64::from_be_bytes(
            encoded[DISPLAY_RECEIPT_V2_PAYLOAD_LEN..DISPLAY_RECEIPT_V2_PAYLOAD_LEN + LENGTH_BYTES]
                .try_into()
                .unwrap(),
        ) as usize;
        DISPLAY_RECEIPT_V2_PAYLOAD_LEN + LENGTH_BYTES + expected_len
    }

    #[test]
    fn canonical_forms_round_trip_and_fit_the_storage_envelope() {
        let matched = complete_artifact(&EXPECTED_PIXELS);
        let mismatch_pixels = [0x0011_2233, 0x0044_5567];
        let mismatched = complete_artifact(&mismatch_pixels);
        let unavailable = unavailable_artifact(None);
        let unavailable_with_distinct_pixels =
            unavailable_artifact(Some(&[0x0011_2231, 0x0044_5566]));

        assert_eq!(matched.captured_form(), CapturedPixelForm::ReuseExpected);
        assert_eq!(mismatched.captured_form(), CapturedPixelForm::Inline);
        assert_eq!(unavailable.captured_form(), CapturedPixelForm::Absent);
        assert_eq!(
            unavailable_with_distinct_pixels.captured_form(),
            CapturedPixelForm::Inline
        );
        assert_eq!(
            matched.tag().to_string(),
            "#bb.display-artifact.sha256.fbd5hyvucb3zycqdqletiyrasrflufmnkjuo4zkdkd6iddprjs6q"
        );
        assert_eq!(
            mismatched.tag().to_string(),
            "#bb.display-artifact.sha256.6seho25bidubdwpxr3s5jwku2jm6k252qiibuuqdz7x7inkxt42q"
        );
        assert_eq!(
            unavailable.tag().to_string(),
            "#bb.display-artifact.sha256.prtxaadxkb7lfpawawthysg2jutx2dqikujmp6osg6tvpodkkzmq"
        );

        let expected_frame_len = frame(2, 1, &EXPECTED_PIXELS).canonical_len();
        assert_eq!(expected_frame_len, 62);
        assert_eq!(matched.encoded_len(), 457);
        assert_eq!(mismatched.encoded_len(), 519);
        assert_eq!(unavailable.encoded_len(), 457);
        assert_eq!(unavailable_with_distinct_pixels.encoded_len(), 519);

        for artifact in [
            matched,
            mismatched,
            unavailable,
            unavailable_with_distinct_pixels,
        ] {
            let encoded = artifact.encode_canonical().unwrap();
            assert_eq!(encoded.len(), artifact.encoded_len());
            assert_eq!(
                DisplayCaptureArtifact::decode_canonical(&encoded),
                Ok(artifact.clone())
            );
            assert_eq!(artifact.verify(), Ok(()));

            let record = StorageRecord::from_tagged_payload(
                None,
                artifact.tag(),
                DISPLAY_CAPTURE_ARTIFACT_SCHEMA,
                &encoded,
            )
            .unwrap();
            assert_eq!(record.object_tag(), artifact.tag());
            assert_eq!(record.object_schema(), DISPLAY_CAPTURE_ARTIFACT_SCHEMA);
            assert_eq!(record.payload(), encoded);
            let record_bytes = record.encode();
            let decoded_record = StorageRecord::decode(&record_bytes, None).unwrap();
            assert_eq!(decoded_record, record);
            assert_eq!(
                DisplayCaptureArtifact::decode_canonical(decoded_record.payload()),
                Ok(artifact)
            );
        }
    }

    #[test]
    fn strict_decoder_refuses_noncanonical_lengths_forms_frames_and_suffixes() {
        let mismatch_pixels = [0x0011_2232, 0x0044_5566];
        let mismatched = complete_artifact(&mismatch_pixels);
        let encoded = mismatched.encode_canonical().unwrap();

        let expected_start = DISPLAY_RECEIPT_V2_PAYLOAD_LEN + LENGTH_BYTES;
        let expected_end = captured_form_offset(&encoded);
        let metadata_end = expected_end + FORM_BYTES + LENGTH_BYTES;
        for end in 0..encoded.len() {
            let needed = if end < expected_start {
                expected_start
            } else if end < expected_end {
                expected_end
            } else if end < metadata_end {
                metadata_end
            } else {
                encoded.len()
            };
            assert_eq!(
                DisplayCaptureArtifact::decode_canonical(&encoded[..end]),
                Err(DisplayArtifactError::Truncated {
                    needed,
                    actual: end,
                })
            );
        }
        let mut mutant = encoded.clone();
        mutant.push(0);
        assert_eq!(
            DisplayCaptureArtifact::decode_canonical(&mutant),
            Err(DisplayArtifactError::TrailingBytes { count: 1 })
        );

        let form_offset = captured_form_offset(&encoded);
        mutant = encoded.clone();
        mutant[form_offset] = 3;
        assert_eq!(
            DisplayCaptureArtifact::decode_canonical(&mutant),
            Err(DisplayArtifactError::UnknownCapturedForm { form: 3 })
        );

        let matched = complete_artifact(&EXPECTED_PIXELS);
        let matched_bytes = matched.encode_canonical().unwrap();
        let matched_form_offset = captured_form_offset(&matched_bytes);
        mutant = matched_bytes.clone();
        mutant[matched_form_offset + 1..matched_form_offset + 9]
            .copy_from_slice(&1_u64.to_be_bytes());
        mutant.push(0);
        assert_eq!(
            DisplayCaptureArtifact::decode_canonical(&mutant),
            Err(DisplayArtifactError::NonCanonicalCapturedForm)
        );

        let unavailable = unavailable_artifact(None);
        let unavailable_bytes = unavailable.encode_canonical().unwrap();
        let unavailable_form_offset = captured_form_offset(&unavailable_bytes);
        mutant = unavailable_bytes.clone();
        mutant[unavailable_form_offset + 1..unavailable_form_offset + 9]
            .copy_from_slice(&1_u64.to_be_bytes());
        mutant.push(0);
        assert_eq!(
            DisplayCaptureArtifact::decode_canonical(&mutant),
            Err(DisplayArtifactError::NonCanonicalCapturedForm)
        );

        mutant = matched_bytes.clone();
        mutant[matched_form_offset] = CapturedPixelForm::Inline as u8;
        let expected_len = frame(2, 1, &EXPECTED_PIXELS).canonical_len();
        mutant[matched_form_offset + 1..matched_form_offset + 9]
            .copy_from_slice(&(expected_len as u64).to_be_bytes());
        mutant.extend_from_slice(&frame_bytes(frame(2, 1, &EXPECTED_PIXELS)));
        assert_eq!(
            DisplayCaptureArtifact::decode_canonical(&mutant),
            Err(DisplayArtifactError::NonCanonicalCapturedForm)
        );

        let expected_end = matched_form_offset;
        mutant = matched_bytes.clone();
        mutant[expected_start..expected_end].copy_from_slice(&frame_bytes(frame(
            1,
            2,
            &EXPECTED_PIXELS,
        )));
        assert_eq!(
            DisplayCaptureArtifact::decode_canonical(&mutant),
            Err(DisplayArtifactError::ViewportMismatch {
                field: "expected frame",
                expected: (2, 1),
                actual: (1, 2),
            })
        );

        mutant = matched_bytes.clone();
        mutant[expected_start + FRAME_FIXED_BYTES] = 1;
        assert_eq!(
            DisplayCaptureArtifact::decode_canonical(&mutant),
            Err(DisplayArtifactError::InvalidHighByte {
                field: "expected frame",
                count: 1,
            })
        );

        mutant = matched_bytes.clone();
        mutant[expected_start + 2] ^= 1;
        assert_eq!(
            DisplayCaptureArtifact::decode_canonical(&mutant),
            Err(DisplayArtifactError::InvalidCanonicalFrame {
                field: "expected frame",
                source: CanonicalFrameError::WrongDomain,
            })
        );

        mutant = matched_bytes.clone();
        mutant[DISPLAY_RECEIPT_V2_PAYLOAD_LEN..DISPLAY_RECEIPT_V2_PAYLOAD_LEN + LENGTH_BYTES]
            .copy_from_slice(&0_u64.to_be_bytes());
        assert_eq!(
            DisplayCaptureArtifact::decode_canonical(&mutant),
            Err(DisplayArtifactError::InvalidCanonicalFrame {
                field: "expected frame",
                source: CanonicalFrameError::Truncated,
            })
        );

        mutant = matched_bytes.clone();
        mutant[expected_start..expected_start + 2]
            .copy_from_slice(&((CANONICAL_FRAME_DOMAIN.len() - 1) as u16).to_be_bytes());
        assert_eq!(
            DisplayCaptureArtifact::decode_canonical(&mutant),
            Err(DisplayArtifactError::InvalidCanonicalFrame {
                field: "expected frame",
                source: CanonicalFrameError::WrongDomainLength {
                    actual: CANONICAL_FRAME_DOMAIN.len() - 1,
                },
            })
        );

        mutant = matched_bytes.clone();
        let count_offset = expected_start + 2 + CANONICAL_FRAME_DOMAIN.len() + 4 + 4;
        mutant[count_offset..count_offset + 8].copy_from_slice(&3_u64.to_be_bytes());
        assert_eq!(
            DisplayCaptureArtifact::decode_canonical(&mutant),
            Err(DisplayArtifactError::InvalidCanonicalFrame {
                field: "expected frame",
                source: CanonicalFrameError::PixelCountMismatch {
                    width: 2,
                    height: 1,
                    declared: 3,
                },
            })
        );

        mutant = matched_bytes.clone();
        mutant[DISPLAY_RECEIPT_V2_PAYLOAD_LEN..DISPLAY_RECEIPT_V2_PAYLOAD_LEN + LENGTH_BYTES]
            .copy_from_slice(&61_u64.to_be_bytes());
        assert_eq!(
            DisplayCaptureArtifact::decode_canonical(&mutant),
            Err(DisplayArtifactError::InvalidCanonicalFrame {
                field: "expected frame",
                source: CanonicalFrameError::WrongLength {
                    expected: 62,
                    actual: 61,
                },
            })
        );

        mutant = matched_bytes.clone();
        mutant[66] = FrameStream::Game as u8;
        assert!(matches!(
            DisplayCaptureArtifact::decode_canonical(&mutant),
            Err(DisplayArtifactError::InvalidDisplayReceipt(
                DisplayProtocolError::WrongFrameStream {
                    expected: FrameStream::Composite,
                    actual: FrameStream::Game,
                }
            ))
        ));

        mutant = matched_bytes.clone();
        mutant[DISPLAY_RECEIPT_V2_PAYLOAD_LEN..DISPLAY_RECEIPT_V2_PAYLOAD_LEN + LENGTH_BYTES]
            .copy_from_slice(&(usize::MAX as u64).to_be_bytes());
        assert_eq!(
            DisplayCaptureArtifact::decode_canonical(&mutant),
            Err(DisplayArtifactError::LengthOverflow {
                field: "expected frame",
            })
        );

        assert_eq!(
            DisplayCaptureArtifact::decode_canonical(&vec![
                0;
                MAX_DISPLAY_CAPTURE_ARTIFACT_BYTES + 1
            ]),
            Err(DisplayArtifactError::PayloadTooLarge {
                actual: MAX_DISPLAY_CAPTURE_ARTIFACT_BYTES + 1,
                max: MAX_DISPLAY_CAPTURE_ARTIFACT_BYTES,
            })
        );
    }

    #[test]
    fn artifact_recomputes_observations_and_exact_comparison_facts_from_pixels() {
        let expected = frame(2, 1, &EXPECTED_PIXELS);
        let actual_pixels = [0x0011_2232, 0x0044_5566];
        let actual = frame(2, 1, &actual_pixels);

        let different_same_mismatch_position = [0x0011_2231, 0x0044_5566];
        let stale_observation = frame(2, 1, &different_same_mismatch_position).observe();
        let mut stale_facts = complete_facts(expected, actual);
        stale_facts.captured = Some(stale_observation);
        let stale_receipt = receipt(expected, stale_facts);
        assert_eq!(
            DisplayCaptureArtifact::new(stale_receipt, expected, Some(actual)),
            Err(DisplayArtifactError::CapturedObservationMismatch {
                expected: Some(stale_observation.tag),
                actual: Some(actual.observe().tag),
            })
        );

        let actual_second_pixels = [0x0011_2233, 0x0044_5567];
        let actual_second = frame(2, 1, &actual_second_pixels);
        let mut stale_comparison = complete_facts(expected, actual_second);
        stale_comparison.first_mismatch = Some(0);
        let stale_comparison_receipt = receipt(expected, stale_comparison);
        assert_eq!(
            DisplayCaptureArtifact::new(stale_comparison_receipt, expected, Some(actual_second)),
            Err(DisplayArtifactError::ComparisonFactsMismatch {
                expected_outcome: DisplayOutcome::Mismatch,
                actual_outcome: DisplayOutcome::Mismatch,
                expected_count: 1,
                actual_count: 1,
                expected_first: Some(0),
                actual_first: Some(1),
            })
        );

        let absent_receipt = receipt(
            expected,
            DisplayCaptureFacts::unavailable(CaptureStage::FlushBefore, 5),
        );
        assert_eq!(
            DisplayCaptureArtifact::new(absent_receipt, expected, Some(expected)),
            Err(DisplayArtifactError::CapturedPresenceMismatch {
                facts_present: false,
                pixels_present: true,
            })
        );

        let mut inline_bytes = complete_artifact(&actual_pixels)
            .encode_canonical()
            .unwrap();
        let form_offset = captured_form_offset(&inline_bytes);
        let captured_pixel_offset = form_offset + FORM_BYTES + LENGTH_BYTES + FRAME_FIXED_BYTES;
        inline_bytes[captured_pixel_offset + 3] ^= 3;
        assert!(matches!(
            DisplayCaptureArtifact::decode_canonical(&inline_bytes),
            Err(DisplayArtifactError::CapturedObservationMismatch { .. })
        ));
    }

    #[test]
    fn initial_viewport_artifact_sizes_respect_the_declared_payload_cap() {
        let pixels = 1201_usize * 801;
        let frame_bytes = FRAME_FIXED_BYTES + pixels * size_of::<u32>();
        let reuse_bytes = FIXED_PAYLOAD_BYTES + frame_bytes;
        let distinct_bytes = reuse_bytes + frame_bytes;
        assert_eq!(pixels, 962_001);
        assert_eq!(frame_bytes, 3_848_058);
        assert_eq!(reuse_bytes, 3_848_453);
        assert_eq!(distinct_bytes, 7_696_511);
        assert!(reuse_bytes <= MAX_DISPLAY_CAPTURE_ARTIFACT_BYTES);
        assert!(distinct_bytes <= MAX_DISPLAY_CAPTURE_ARTIFACT_BYTES);
        assert_eq!(MAX_DISPLAY_CAPTURE_ARTIFACT_BYTES - distinct_bytes, 692_097);

        assert_eq!(checked_payload_len(2_097_039, None), Ok(8_388_605));
        assert_eq!(
            checked_payload_len(2_097_040, None),
            Err(DisplayArtifactError::PayloadTooLarge {
                actual: 8_388_609,
                max: MAX_DISPLAY_CAPTURE_ARTIFACT_BYTES,
            })
        );
        assert_eq!(
            checked_payload_len(1_048_513, Some(1_048_513)),
            Ok(8_388_607)
        );
        assert_eq!(
            checked_payload_len(1_048_514, Some(1_048_514)),
            Err(DisplayArtifactError::PayloadTooLarge {
                actual: 8_388_615,
                max: MAX_DISPLAY_CAPTURE_ARTIFACT_BYTES,
            })
        );
    }

    #[test]
    fn in_memory_verifier_refuses_tag_shape_and_noncanonical_form_tampering() {
        let mut bad_tag = complete_artifact(&EXPECTED_PIXELS);
        bad_tag.tag = tag(TagKind::DisplayArtifact, b"tampered");
        assert_eq!(
            bad_tag.verify(),
            Err(DisplayArtifactError::ArtifactTagMismatch)
        );

        let mut bad_shape = complete_artifact(&EXPECTED_PIXELS);
        bad_shape.expected_pixels.pop();
        assert!(matches!(
            bad_shape.verify(),
            Err(DisplayArtifactError::FrameShape(
                FrameShapeError::PixelCount {
                    width: 2,
                    height: 1,
                    expected: 2,
                    actual: 1,
                }
            ))
        ));

        let mut duplicate_inline = complete_artifact(&EXPECTED_PIXELS);
        duplicate_inline.captured_pixels = CapturedPixels::Inline(EXPECTED_PIXELS.to_vec());
        duplicate_inline.tag = duplicate_inline.recompute_tag().unwrap();
        assert_eq!(
            duplicate_inline.verify(),
            Err(DisplayArtifactError::NonCanonicalEncoding)
        );
    }

    #[test]
    fn storage_envelope_refuses_a_valid_payload_mutation_under_the_stale_object_tag() {
        let artifact = complete_artifact(&EXPECTED_PIXELS);
        let mut mutant = artifact.encode_canonical().unwrap();
        let dpi_last_byte = 203 + 44;
        mutant[dpi_last_byte] ^= 1;

        let decoded = DisplayCaptureArtifact::decode_canonical(&mutant).unwrap();
        assert_ne!(decoded.tag(), artifact.tag());
        assert_eq!(
            StorageRecord::from_tagged_payload(
                None,
                artifact.tag(),
                DISPLAY_CAPTURE_ARTIFACT_SCHEMA,
                &mutant,
            ),
            Err(StorageError::ObjectTagMismatch)
        );
    }
}
