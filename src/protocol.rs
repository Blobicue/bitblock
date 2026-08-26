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
    OutputTopology,
    Policy,
    #[cfg(any(test, feature = "protocol-research"))]
    PresentationArtifact,
    #[cfg(any(test, feature = "protocol-research"))]
    PresentationOutputArtifact,
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
            Self::OutputTopology => "output-topology",
            Self::Policy => "policy",
            #[cfg(any(test, feature = "protocol-research"))]
            Self::PresentationArtifact => "presentation-artifact",
            #[cfg(any(test, feature = "protocol-research"))]
            Self::PresentationOutputArtifact => "presentation-output-artifact",
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
    pub(crate) fn from_name(name: &[u8]) -> Option<Self> {
        Some(match name {
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
            b"output-topology" => Self::OutputTopology,
            b"policy" => Self::Policy,
            #[cfg(any(test, feature = "protocol-research"))]
            b"presentation-artifact" => Self::PresentationArtifact,
            #[cfg(any(test, feature = "protocol-research"))]
            b"presentation-output-artifact" => Self::PresentationOutputArtifact,
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
    pub(crate) const fn from_digest(kind: TagKind, digest: [u8; 32]) -> Self {
        Self { kind, digest }
    }

    pub const fn kind(self) -> TagKind {
        self.kind
    }

    pub const fn digest(self) -> [u8; 32] {
        self.digest
    }
}

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
            TagKind::OutputTopology,
            TagKind::Policy,
            TagKind::PresentationArtifact,
            TagKind::PresentationOutputArtifact,
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
}
