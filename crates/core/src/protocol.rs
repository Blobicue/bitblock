//! Content-addressed protocol tags.
//!
//! A tag commits to canonical bytes under a semantic kind and schema. It does not establish
//! that those bytes satisfy the rules associated with that kind; callers must run the separate
//! verifier for the object they are naming.

use std::fmt;

use sha2::{Digest, Sha256};

const TAG_DOMAIN: &[u8] = b"bitblock/content-tag/v1";
const HASH_SUITE: &[u8] = b"sha256";
const BASE32: &[u8; 32] = b"abcdefghijklmnopqrstuvwxyz234567";

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum TagKind {
    #[cfg(any(test, feature = "protocol-research"))]
    Build,
    #[cfg(any(test, feature = "protocol-research"))]
    DisplayArtifact,
    DisplayReceipt,
    Edit,
    Event,
    Frame,
    FrameInput,
    FrameReceipt,
    Intent,
    #[cfg(any(test, feature = "protocol-research"))]
    JournalRejectedTail,
    Key,
    #[cfg(any(test, feature = "protocol-research"))]
    LadderQuoteReceipt,
    #[cfg(any(test, feature = "protocol-research"))]
    LadderTerm,
    MetricDefinition,
    MetricSnapshot,
    #[cfg(any(test, feature = "protocol-research"))]
    OutputColorObservation,
    #[cfg(any(test, feature = "protocol-research"))]
    OutputTopology,
    #[cfg(any(test, feature = "protocol-research"))]
    PhysicalSensorObservation,
    Policy,
    #[cfg(any(test, feature = "protocol-research"))]
    PresentationArtifact,
    #[cfg(any(test, feature = "protocol-research"))]
    PresentationOutputArtifact,
    #[cfg(any(test, feature = "protocol-research"))]
    ReplayEvidence,
    Receipt,
    Registry,
    SequentialHistory,
    #[cfg(any(test, feature = "protocol-research"))]
    StorageRecord,
    TickOutcome,
    World,
}

impl TagKind {
    pub const fn name(self) -> &'static str {
        match self {
            #[cfg(any(test, feature = "protocol-research"))]
            Self::Build => "build",
            #[cfg(any(test, feature = "protocol-research"))]
            Self::DisplayArtifact => "display-artifact",
            Self::DisplayReceipt => "display-receipt",
            Self::Edit => "edit",
            Self::Event => "event",
            Self::Frame => "frame",
            Self::FrameInput => "frame-input",
            Self::FrameReceipt => "frame-receipt",
            Self::Intent => "intent",
            #[cfg(any(test, feature = "protocol-research"))]
            Self::JournalRejectedTail => "journal-rejected-tail",
            Self::Key => "key",
            #[cfg(any(test, feature = "protocol-research"))]
            Self::LadderQuoteReceipt => "ladder-quote-receipt",
            #[cfg(any(test, feature = "protocol-research"))]
            Self::LadderTerm => "ladder-term",
            Self::MetricDefinition => "metric-definition",
            Self::MetricSnapshot => "metric-snapshot",
            #[cfg(any(test, feature = "protocol-research"))]
            Self::OutputColorObservation => "output-color-observation",
            #[cfg(any(test, feature = "protocol-research"))]
            Self::OutputTopology => "output-topology",
            #[cfg(any(test, feature = "protocol-research"))]
            Self::PhysicalSensorObservation => "physical-sensor-observation",
            Self::Policy => "policy",
            #[cfg(any(test, feature = "protocol-research"))]
            Self::PresentationArtifact => "presentation-artifact",
            #[cfg(any(test, feature = "protocol-research"))]
            Self::PresentationOutputArtifact => "presentation-output-artifact",
            #[cfg(any(test, feature = "protocol-research"))]
            Self::ReplayEvidence => "replay-evidence",
            Self::Receipt => "receipt",
            Self::Registry => "registry",
            Self::SequentialHistory => "sequential-history",
            #[cfg(any(test, feature = "protocol-research"))]
            Self::StorageRecord => "storage-record",
            Self::TickOutcome => "tick-outcome",
            Self::World => "world",
        }
    }

    #[cfg(any(test, feature = "protocol-research"))]
    /// Resolve a kind from its canonical name, for decoding a storage envelope.
    pub fn from_name(name: &[u8]) -> Option<Self> {
        Some(match name {
            #[cfg(any(test, feature = "protocol-research"))]
            b"build" => Self::Build,
            #[cfg(any(test, feature = "protocol-research"))]
            b"display-artifact" => Self::DisplayArtifact,
            b"display-receipt" => Self::DisplayReceipt,
            b"edit" => Self::Edit,
            b"event" => Self::Event,
            b"frame" => Self::Frame,
            b"frame-input" => Self::FrameInput,
            b"frame-receipt" => Self::FrameReceipt,
            b"intent" => Self::Intent,
            #[cfg(any(test, feature = "protocol-research"))]
            b"journal-rejected-tail" => Self::JournalRejectedTail,
            b"key" => Self::Key,
            #[cfg(any(test, feature = "protocol-research"))]
            b"ladder-quote-receipt" => Self::LadderQuoteReceipt,
            #[cfg(any(test, feature = "protocol-research"))]
            b"ladder-term" => Self::LadderTerm,
            b"metric-definition" => Self::MetricDefinition,
            b"metric-snapshot" => Self::MetricSnapshot,
            #[cfg(any(test, feature = "protocol-research"))]
            b"output-color-observation" => Self::OutputColorObservation,
            #[cfg(any(test, feature = "protocol-research"))]
            b"output-topology" => Self::OutputTopology,
            #[cfg(any(test, feature = "protocol-research"))]
            b"physical-sensor-observation" => Self::PhysicalSensorObservation,
            b"policy" => Self::Policy,
            #[cfg(any(test, feature = "protocol-research"))]
            b"presentation-artifact" => Self::PresentationArtifact,
            #[cfg(any(test, feature = "protocol-research"))]
            b"presentation-output-artifact" => Self::PresentationOutputArtifact,
            #[cfg(any(test, feature = "protocol-research"))]
            b"replay-evidence" => Self::ReplayEvidence,
            b"receipt" => Self::Receipt,
            b"registry" => Self::Registry,
            b"sequential-history" => Self::SequentialHistory,
            #[cfg(any(test, feature = "protocol-research"))]
            b"storage-record" => Self::StorageRecord,
            b"tick-outcome" => Self::TickOutcome,
            b"world" => Self::World,
            _ => return None,
        })
    }
}

/// A full SHA-256 content tag. No shortened form has authority.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ContentTag {
    kind: TagKind,
    digest: [u8; 32],
}

impl ContentTag {
    /// Rebuild a tag from a digest that is already known to be canonical.
    ///
    /// This constructs a tag *without* hashing anything, so it can only be
    /// applied to a digest some canonical encoding already produced -- decoding
    /// a stored record, or re-reading a receipt. Handing it arbitrary bytes
    /// makes a well-formed tag for content nobody computed.
    ///
    /// It became `pub` when the research modules moved to their own crate. The
    /// set of callers did not change -- they were all inside this crate before
    /// -- but the boundary is now the compiler's rather than a convention's.
    pub const fn from_digest(kind: TagKind, digest: [u8; 32]) -> Self {
        Self { kind, digest }
    }

    pub const fn kind(self) -> TagKind {
        self.kind
    }

    pub const fn digest(self) -> [u8; 32] {
        self.digest
    }

    /// Parses one complete canonical content-tag display form.
    ///
    /// Parsing establishes only syntax and the named kind/digest pair. It does not recover or
    /// validate the canonical object whose bytes the tag is intended to name.
    #[cfg(any(test, feature = "protocol-research"))]
    pub fn parse_full(text: &str) -> Result<Self, ContentTagParseError> {
        let Some(rest) = text.strip_prefix("#bb.") else {
            return Err(ContentTagParseError::InvalidPrefix);
        };
        let mut fields = rest.split('.');
        let Some(kind_name) = fields.next() else {
            return Err(ContentTagParseError::InvalidShape);
        };
        let Some(hash_suite) = fields.next() else {
            return Err(ContentTagParseError::InvalidShape);
        };
        let Some(encoded_digest) = fields.next() else {
            return Err(ContentTagParseError::InvalidShape);
        };
        if fields.next().is_some() {
            return Err(ContentTagParseError::InvalidShape);
        }
        if hash_suite != "sha256" {
            return Err(ContentTagParseError::UnexpectedHashSuite);
        }
        let kind =
            TagKind::from_name(kind_name.as_bytes()).ok_or(ContentTagParseError::UnknownKind)?;
        let digest = decode_base32_digest(encoded_digest)?;
        let tag = Self { kind, digest };
        if tag.to_string() != text {
            return Err(ContentTagParseError::NonCanonical);
        }
        Ok(tag)
    }
}

/// Why a complete textual content tag was refused.
#[cfg(any(test, feature = "protocol-research"))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ContentTagParseError {
    InvalidPrefix,
    InvalidShape,
    UnknownKind,
    UnexpectedHashSuite,
    DigestLength { actual: usize },
    DigestCharacter { index: usize, byte: u8 },
    NonCanonical,
}

#[cfg(any(test, feature = "protocol-research"))]
impl fmt::Display for ContentTagParseError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {
            Self::InvalidPrefix => formatter.write_str("content tag must start with '#bb.'"),
            Self::InvalidShape => formatter
                .write_str("content tag must contain exactly kind, hash suite, and digest fields"),
            Self::UnknownKind => formatter.write_str("content tag has an unknown kind"),
            Self::UnexpectedHashSuite => {
                formatter.write_str("content tag must name the sha256 hash suite")
            }
            Self::DigestLength { actual } => write!(
                formatter,
                "content tag digest has {actual} base32 bytes, expected 52"
            ),
            Self::DigestCharacter { index, byte } => write!(
                formatter,
                "content tag digest byte {byte:#04x} at index {index} is not canonical base32"
            ),
            Self::NonCanonical => {
                formatter.write_str("content tag is not in canonical display form")
            }
        }
    }
}

#[cfg(any(test, feature = "protocol-research"))]
impl std::error::Error for ContentTagParseError {}

impl fmt::Display for ContentTag {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let encoded = base32_digest(self.digest);
        write!(
            formatter,
            "#bb.{}.sha256.{}",
            self.kind.name(),
            std::str::from_utf8(&encoded).expect("base32 alphabet is ASCII")
        )
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TagError {
    SchemaTooLong(usize),
    PayloadLengthMismatch {
        expected: u64,
        actual: u64,
    },
    PayloadLengthOverflow,
    #[cfg(any(test, feature = "protocol-research"))]
    PreimageLengthOverflow,
    #[cfg(any(test, feature = "protocol-research"))]
    PreimageAllocationFailed {
        requested: usize,
    },
}

impl fmt::Display for TagError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {
            Self::SchemaTooLong(actual) => {
                write!(formatter, "tag schema length {actual} exceeds u16")
            }
            Self::PayloadLengthMismatch { expected, actual } => write!(
                formatter,
                "tag declared {expected} payload bytes but encoded {actual}"
            ),
            Self::PayloadLengthOverflow => {
                formatter.write_str("tag payload byte count overflowed u64")
            }
            #[cfg(any(test, feature = "protocol-research"))]
            Self::PreimageLengthOverflow => {
                formatter.write_str("canonical tag preimage length is not representable")
            }
            #[cfg(any(test, feature = "protocol-research"))]
            Self::PreimageAllocationFailed { requested } => write!(
                formatter,
                "could not allocate {requested} canonical tag preimage bytes"
            ),
        }
    }
}

impl std::error::Error for TagError {}

/// Hashes one already-canonical payload under a kind and schema.
///
/// `encode` may feed the payload in any chunk sizes. The declared byte length is hashed before
/// the payload and checked afterward, so omitted, duplicated, or overlong encodings fail closed.
pub fn hash_canonical(
    kind: TagKind,
    schema: &[u8],
    payload_len: u64,
    encode: impl FnOnce(&mut dyn FnMut(&[u8])),
) -> Result<ContentTag, TagError> {
    let mut hasher = Sha256::new();
    write_canonical_preimage(kind, schema, payload_len, encode, &mut |bytes| {
        hasher.update(bytes);
    })?;

    Ok(ContentTag {
        kind,
        digest: hasher.finalize().into(),
    })
}

/// Materializes the exact byte preimage consumed by [`hash_canonical`].
///
/// This is intended for bounded protocol bridges and independent parity checks. The caller still
/// chooses the payload limit and canonical object verifier; materializing a preimage does not make
/// its payload valid for `kind`.
#[cfg(any(test, feature = "protocol-research"))]
pub fn encode_canonical_preimage(
    kind: TagKind,
    schema: &[u8],
    payload_len: u64,
    encode: impl FnOnce(&mut dyn FnMut(&[u8])),
) -> Result<Vec<u8>, TagError> {
    let header_len = canonical_header_len(kind, schema)?;
    let payload_len = usize::try_from(payload_len).map_err(|_| TagError::PreimageLengthOverflow)?;
    let preimage_len = header_len
        .checked_add(payload_len)
        .ok_or(TagError::PreimageLengthOverflow)?;
    let mut preimage = Vec::new();
    preimage
        .try_reserve_exact(preimage_len)
        .map_err(|_| TagError::PreimageAllocationFailed {
            requested: preimage_len,
        })?;
    let mut materialization_overflowed = false;
    write_canonical_preimage(kind, schema, payload_len as u64, encode, &mut |bytes| {
        let Some(next_len) = preimage.len().checked_add(bytes.len()) else {
            materialization_overflowed = true;
            return;
        };
        if next_len > preimage_len {
            materialization_overflowed = true;
            return;
        }
        preimage.extend_from_slice(bytes);
    })?;
    if materialization_overflowed || preimage.len() != preimage_len {
        return Err(TagError::PreimageLengthOverflow);
    }
    Ok(preimage)
}

#[cfg(any(test, feature = "protocol-research"))]
fn canonical_header_len(kind: TagKind, schema: &[u8]) -> Result<usize, TagError> {
    u16::try_from(schema.len()).map_err(|_| TagError::SchemaTooLong(schema.len()))?;
    Ok(2 + TAG_DOMAIN.len() + 2 + kind.name().len() + 2 + HASH_SUITE.len() + 2 + schema.len() + 8)
}

fn write_canonical_preimage(
    kind: TagKind,
    schema: &[u8],
    payload_len: u64,
    encode: impl FnOnce(&mut dyn FnMut(&[u8])),
    sink: &mut dyn FnMut(&[u8]),
) -> Result<(), TagError> {
    let schema_len =
        u16::try_from(schema.len()).map_err(|_| TagError::SchemaTooLong(schema.len()))?;
    let kind_bytes = kind.name().as_bytes();
    let kind_len = u16::try_from(kind_bytes.len()).expect("closed tag kind exceeds u16");
    let domain_len = u16::try_from(TAG_DOMAIN.len()).expect("tag domain exceeds u16");
    let suite_len = u16::try_from(HASH_SUITE.len()).expect("hash suite name exceeds u16");

    sink(&domain_len.to_be_bytes());
    sink(TAG_DOMAIN);
    sink(&kind_len.to_be_bytes());
    sink(kind_bytes);
    sink(&suite_len.to_be_bytes());
    sink(HASH_SUITE);
    sink(&schema_len.to_be_bytes());
    sink(schema);
    sink(&payload_len.to_be_bytes());

    let mut actual_len = 0_u64;
    let mut overflowed = false;
    let mut payload_sink = |bytes: &[u8]| {
        match actual_len.checked_add(bytes.len() as u64) {
            Some(next) => actual_len = next,
            None => overflowed = true,
        }
        sink(bytes);
    };
    encode(&mut payload_sink);

    if overflowed {
        return Err(TagError::PayloadLengthOverflow);
    }
    if actual_len != payload_len {
        return Err(TagError::PayloadLengthMismatch {
            expected: payload_len,
            actual: actual_len,
        });
    }

    Ok(())
}

fn base32_digest(digest: [u8; 32]) -> [u8; 52] {
    let mut encoded = [0_u8; 52];
    let mut accumulator = 0_u16;
    let mut bits = 0_u8;
    let mut output = 0_usize;

    for byte in digest {
        accumulator = (accumulator << 8) | u16::from(byte);
        bits += 8;
        while bits >= 5 {
            bits -= 5;
            encoded[output] = BASE32[((accumulator >> bits) & 0x1f) as usize];
            output += 1;
        }
        accumulator &= (1_u16 << bits) - 1;
    }

    if bits != 0 {
        encoded[output] = BASE32[((accumulator << (5 - bits)) & 0x1f) as usize];
        output += 1;
    }
    debug_assert_eq!(output, encoded.len());
    encoded
}

#[cfg(any(test, feature = "protocol-research"))]
fn decode_base32_digest(encoded: &str) -> Result<[u8; 32], ContentTagParseError> {
    let bytes = encoded.as_bytes();
    if bytes.len() != 52 {
        return Err(ContentTagParseError::DigestLength {
            actual: bytes.len(),
        });
    }

    let mut digest = [0_u8; 32];
    let mut accumulator = 0_u16;
    let mut bits = 0_u8;
    let mut output = 0_usize;
    for (index, &byte) in bytes.iter().enumerate() {
        let Some(value) = BASE32.iter().position(|&candidate| candidate == byte) else {
            return Err(ContentTagParseError::DigestCharacter { index, byte });
        };
        accumulator = (accumulator << 5) | value as u16;
        bits += 5;
        while bits >= 8 {
            bits -= 8;
            if output >= digest.len() {
                return Err(ContentTagParseError::NonCanonical);
            }
            digest[output] = (accumulator >> bits) as u8;
            output += 1;
            accumulator &= (1_u16 << bits) - 1;
        }
    }

    if output != digest.len() || bits != 4 || accumulator != 0 {
        return Err(ContentTagParseError::NonCanonical);
    }
    Ok(digest)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn declared_payload_length_fails_closed() {
        assert_eq!(
            hash_canonical(TagKind::World, b"test/v1", 4, |sink| sink(b"abc")),
            Err(TagError::PayloadLengthMismatch {
                expected: 4,
                actual: 3,
            })
        );
    }

    #[test]
    fn feed_chunking_does_not_change_a_tag() {
        let whole = hash_canonical(TagKind::World, b"test/v1", 6, |sink| sink(b"abcdef")).unwrap();
        let chunked = hash_canonical(TagKind::World, b"test/v1", 6, |sink| {
            sink(b"ab");
            sink(b"");
            sink(b"cdef");
        })
        .unwrap();
        assert_eq!(whole, chunked);
    }

    #[test]
    fn kind_schema_and_payload_are_domain_separated() {
        let base = hash_canonical(TagKind::World, b"test/v1", 3, |sink| sink(b"abc")).unwrap();
        let kind = hash_canonical(TagKind::Frame, b"test/v1", 3, |sink| sink(b"abc")).unwrap();
        let schema = hash_canonical(TagKind::World, b"test/v2", 3, |sink| sink(b"abc")).unwrap();
        let payload = hash_canonical(TagKind::World, b"test/v1", 3, |sink| sink(b"abd")).unwrap();
        assert_ne!(base, kind);
        assert_ne!(base, schema);
        assert_ne!(base, payload);
    }

    #[test]
    fn materialized_preimage_is_exactly_what_hash_canonical_hashes() {
        let preimage = encode_canonical_preimage(TagKind::StorageRecord, b"bridge/v1", 6, |sink| {
            sink(b"ab");
            sink(b"cdef");
        })
        .unwrap();
        let independently_hashed: [u8; 32] = Sha256::digest(&preimage).into();
        let streamed = hash_canonical(TagKind::StorageRecord, b"bridge/v1", 6, |sink| {
            sink(b"abcdef")
        })
        .unwrap();

        assert_eq!(independently_hashed, streamed.digest());
        assert!(
            preimage
                .windows(14)
                .any(|window| window == b"storage-record")
        );
        assert!(preimage.ends_with(b"abcdef"));
    }

    #[test]
    fn materialized_preimage_never_grows_past_its_declared_length() {
        let overlong = [0_u8; 4096];
        assert_eq!(
            encode_canonical_preimage(TagKind::World, b"bounded/v1", 0, |sink| { sink(&overlong) }),
            Err(TagError::PayloadLengthMismatch {
                expected: 0,
                actual: overlong.len() as u64,
            })
        );
    }

    #[test]
    fn every_kind_name_has_one_canonical_inverse() {
        let kinds = [
            TagKind::Build,
            TagKind::DisplayArtifact,
            TagKind::DisplayReceipt,
            TagKind::Edit,
            TagKind::Event,
            TagKind::Frame,
            TagKind::FrameInput,
            TagKind::FrameReceipt,
            TagKind::Intent,
            TagKind::JournalRejectedTail,
            TagKind::Key,
            TagKind::LadderQuoteReceipt,
            TagKind::LadderTerm,
            TagKind::MetricDefinition,
            TagKind::MetricSnapshot,
            TagKind::OutputColorObservation,
            TagKind::OutputTopology,
            TagKind::PhysicalSensorObservation,
            TagKind::Policy,
            TagKind::PresentationArtifact,
            TagKind::PresentationOutputArtifact,
            TagKind::ReplayEvidence,
            TagKind::Receipt,
            TagKind::Registry,
            TagKind::SequentialHistory,
            TagKind::StorageRecord,
            TagKind::TickOutcome,
            TagKind::World,
        ];
        for kind in kinds {
            assert_eq!(TagKind::from_name(kind.name().as_bytes()), Some(kind));
        }
        assert_eq!(TagKind::from_name(b"World"), None);
        assert_eq!(TagKind::from_name(b"storage_record"), None);
    }

    #[test]
    fn tag_wrapper_matches_independent_python_sha256_vector() {
        let tag = hash_canonical(TagKind::World, b"test/v1", 3, |sink| sink(b"abc")).unwrap();
        assert_eq!(
            tag.digest(),
            [
                0xe6, 0x1a, 0xba, 0x12, 0x3c, 0x7c, 0x69, 0x59, 0x9b, 0x5a, 0xf0, 0x3c, 0xac, 0x4b,
                0xfe, 0xf4, 0xb6, 0xe3, 0x77, 0x68, 0x37, 0xec, 0x70, 0x71, 0xd2, 0x2a, 0x62, 0xc2,
                0x7a, 0xd1, 0xa0, 0xae,
            ]
        );
        assert_eq!(
            tag.to_string(),
            "#bb.world.sha256.4ynluer4pruvtg226a6kys766s3og53ig7wha4osfjrme6wrucxa"
        );
    }

    #[test]
    fn display_is_a_full_lowercase_unpadded_tag() {
        let tag = hash_canonical(TagKind::World, b"test/v1", 0, |_| {}).unwrap();
        let display = tag.to_string();
        assert!(display.starts_with("#bb.world.sha256."));
        assert_eq!(display.len(), "#bb.world.sha256.".len() + 52);
        assert!(display.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'#' | b'.' | b'-')
        }));
    }

    #[test]
    fn full_tag_parser_round_trips_and_refuses_noncanonical_forms() {
        let tag = hash_canonical(TagKind::StorageRecord, b"tag-parser/v1", 3, |sink| {
            sink(b"abc")
        })
        .unwrap();
        let text = tag.to_string();
        assert_eq!(ContentTag::parse_full(&text), Ok(tag));

        assert_eq!(
            ContentTag::parse_full(
                "#bb.world.sha512.aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
            ),
            Err(ContentTagParseError::UnexpectedHashSuite)
        );
        assert_eq!(
            ContentTag::parse_full(
                "#bb.World.sha256.aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
            ),
            Err(ContentTagParseError::UnknownKind)
        );
        assert_eq!(
            ContentTag::parse_full("#bb.world.sha256.a"),
            Err(ContentTagParseError::DigestLength { actual: 1 })
        );

        let mut invalid_character = text.clone().into_bytes();
        invalid_character["#bb.storage-record.sha256.".len()] = b'0';
        let invalid_character = String::from_utf8(invalid_character).unwrap();
        assert!(matches!(
            ContentTag::parse_full(&invalid_character),
            Err(ContentTagParseError::DigestCharacter { .. })
        ));

        let mut noncanonical_padding = text.into_bytes();
        *noncanonical_padding.last_mut().unwrap() = b'b';
        let noncanonical_padding = String::from_utf8(noncanonical_padding).unwrap();
        assert_eq!(
            ContentTag::parse_full(&noncanonical_padding),
            Err(ContentTagParseError::NonCanonical)
        );
    }
}
