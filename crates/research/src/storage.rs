//! Bounded, in-memory canonical storage envelopes.
//!
//! A [`StorageRecord`] carries one complete content-tagged payload and an optional parent
//! `StorageRecord` tag. The parent chain orders records without sequence numbers or numeric object
//! identifiers. Serialized records are self-authenticating under the protocol's SHA-256 assumption,
//! but this module provides no filesystem durability, rollback resistance, signatures, custody,
//! consensus, or integration with the existing coordinate history.

use std::fmt;

use crate::protocol::{ContentTag, TagError, TagKind, hash_canonical};

/// Schema in the domain of every canonical storage-record tag.
pub const STORAGE_RECORD_SCHEMA: &[u8] = b"bitblock/storage-record/envelope/v1";

const STORAGE_MAGIC: &[u8; 8] = b"BBSTRREC";
const STORAGE_VERSION: u16 = 1;
const DIGEST_BYTES: usize = 32;
const HEADER_BYTES: usize = STORAGE_MAGIC.len() + 2 + 8;
const FOOTER_BYTES: usize = DIGEST_BYTES;
const MAX_KIND_NAME_BYTES: usize = 64;
const MIN_RECORD_BYTES: usize = HEADER_BYTES
    + 1 // optional-parent discriminant
    + 2 // nonempty object-kind name length
    + 1 // at least one object-kind byte
    + DIGEST_BYTES
    + 2 // object schema length
    + 8 // payload length
    + FOOTER_BYTES;

/// Hard protocol cap for one serialized record, including its authentication footer.
pub const MAX_STORAGE_RECORD_BYTES: usize = 16 * 1024 * 1024;
/// Hard protocol cap for the schema that defines the stored object's canonical payload.
pub const MAX_STORAGE_SCHEMA_BYTES: usize = 4 * 1024;
/// Hard protocol cap for one stored canonical payload.
pub const MAX_STORAGE_PAYLOAD_BYTES: usize = 8 * 1024 * 1024;
/// Hard protocol cap for records admitted by one in-memory scanner or chain.
pub const MAX_STORAGE_RECORDS: usize = 65_536;
/// Hard protocol cap for bytes admitted by one in-memory scanner or chain.
pub const MAX_STORAGE_STREAM_BYTES: usize = 64 * 1024 * 1024;

/// Decoder and in-memory admission limits. Custom limits may tighten but never widen the hard caps.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StorageLimits {
    max_record_bytes: usize,
    max_schema_bytes: usize,
    max_payload_bytes: usize,
    max_records: usize,
    max_stream_bytes: usize,
}

impl StorageLimits {
    /// Default storage envelope and stream admission limits at the hard protocol caps.
    pub const DEFAULT: Self = Self {
        max_record_bytes: MAX_STORAGE_RECORD_BYTES,
        max_schema_bytes: MAX_STORAGE_SCHEMA_BYTES,
        max_payload_bytes: MAX_STORAGE_PAYLOAD_BYTES,
        max_records: MAX_STORAGE_RECORDS,
        max_stream_bytes: MAX_STORAGE_STREAM_BYTES,
    };

    /// Constructs custom storage admission limits if none exceed the hard protocol caps.
    pub fn new(
        max_record_bytes: usize,
        max_schema_bytes: usize,
        max_payload_bytes: usize,
        max_records: usize,
        max_stream_bytes: usize,
    ) -> Result<Self, StorageError> {
        if max_record_bytes > MAX_STORAGE_RECORD_BYTES
            || max_schema_bytes > MAX_STORAGE_SCHEMA_BYTES
            || max_payload_bytes > MAX_STORAGE_PAYLOAD_BYTES
            || max_records > MAX_STORAGE_RECORDS
            || max_stream_bytes > MAX_STORAGE_STREAM_BYTES
        {
            return Err(StorageError::LimitsExceedProtocolCaps);
        }
        Ok(Self {
            max_record_bytes,
            max_schema_bytes,
            max_payload_bytes,
            max_records,
            max_stream_bytes,
        })
    }

    /// Returns the maximum byte length allowed for one serialized storage record.
    pub const fn max_record_bytes(self) -> usize {
        self.max_record_bytes
    }

    /// Returns the maximum byte length allowed for an inner object schema.
    pub const fn max_schema_bytes(self) -> usize {
        self.max_schema_bytes
    }

    /// Returns the maximum byte length allowed for an inner canonical payload.
    pub const fn max_payload_bytes(self) -> usize {
        self.max_payload_bytes
    }

    /// Returns the maximum number of storage records admitted by a scanner or chain.
    pub const fn max_records(self) -> usize {
        self.max_records
    }

    /// Returns the maximum total byte length admitted by a scanner or chain.
    pub const fn max_stream_bytes(self) -> usize {
        self.max_stream_bytes
    }
}

impl Default for StorageLimits {
    fn default() -> Self {
        Self::DEFAULT
    }
}

/// One canonical, content-addressed storage envelope.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StorageRecord {
    record_tag: ContentTag,
    parent: Option<ContentTag>,
    object_tag: ContentTag,
    object_schema: Vec<u8>,
    payload: Vec<u8>,
}

impl StorageRecord {
    /// Builds a record and derives the stored object's full content tag from its canonical payload.
    pub fn new(
        parent: Option<ContentTag>,
        object_kind: TagKind,
        object_schema: &[u8],
        payload: &[u8],
    ) -> Result<Self, StorageError> {
        Self::new_with_limits(
            parent,
            object_kind,
            object_schema,
            payload,
            StorageLimits::DEFAULT,
        )
    }

    /// Builds a storage record and derives the inner object tag under explicit `StorageLimits`.
    pub fn new_with_limits(
        parent: Option<ContentTag>,
        object_kind: TagKind,
        object_schema: &[u8],
        payload: &[u8],
        limits: StorageLimits,
    ) -> Result<Self, StorageError> {
        validate_parent_kind(parent)?;
        preflight_record(
            parent,
            object_kind,
            object_schema.len(),
            payload.len(),
            limits,
        )?;
        let object_tag =
            hash_canonical(object_kind, object_schema, payload.len() as u64, |sink| {
                sink(payload)
            })?;
        Self::build(parent, object_tag, object_schema, payload, limits)
    }

    /// Admits a caller-supplied full object tag only when it matches the schema and payload.
    pub fn from_tagged_payload(
        parent: Option<ContentTag>,
        object_tag: ContentTag,
        object_schema: &[u8],
        payload: &[u8],
    ) -> Result<Self, StorageError> {
        Self::from_tagged_payload_with_limits(
            parent,
            object_tag,
            object_schema,
            payload,
            StorageLimits::DEFAULT,
        )
    }

    /// Builds a storage record from a pre-verified object tag under explicit `StorageLimits`.
    pub fn from_tagged_payload_with_limits(
        parent: Option<ContentTag>,
        object_tag: ContentTag,
        object_schema: &[u8],
        payload: &[u8],
        limits: StorageLimits,
    ) -> Result<Self, StorageError> {
        validate_parent_kind(parent)?;
        preflight_record(
            parent,
            object_tag.kind(),
            object_schema.len(),
            payload.len(),
            limits,
        )?;
        let expected = hash_canonical(
            object_tag.kind(),
            object_schema,
            payload.len() as u64,
            |sink| sink(payload),
        )?;
        if expected != object_tag {
            return Err(StorageError::ObjectTagMismatch);
        }
        Self::build(parent, object_tag, object_schema, payload, limits)
    }

    /// Decodes exactly one record and admits it only at `expected_parent`.
    ///
    /// Passing `None` is the explicit genesis condition. Trailing bytes are rejected; use
    /// [`StorageScanner`] for a concatenated stream.
    pub fn decode(
        encoded: &[u8],
        expected_parent: Option<ContentTag>,
    ) -> Result<Self, StorageError> {
        Self::decode_with_limits(encoded, expected_parent, StorageLimits::DEFAULT)
    }

    /// Decodes one complete storage record at `expected_parent` under explicit `StorageLimits`.
    pub fn decode_with_limits(
        encoded: &[u8],
        expected_parent: Option<ContentTag>,
        limits: StorageLimits,
    ) -> Result<Self, StorageError> {
        validate_parent_kind(expected_parent)?;
        let (record, consumed) = Self::decode_prefix(encoded, expected_parent, limits)?;
        if consumed != encoded.len() {
            return Err(StorageError::TrailingBytes {
                count: encoded.len() - consumed,
            });
        }
        Ok(record)
    }

    /// Returns the canonical `ContentTag` of this storage envelope.
    pub const fn record_tag(&self) -> ContentTag {
        self.record_tag
    }

    /// Returns the optional parent storage-record `ContentTag`.
    pub const fn parent(&self) -> Option<ContentTag> {
        self.parent
    }

    /// Returns the `ContentTag` of the inner canonical object.
    pub const fn object_tag(&self) -> ContentTag {
        self.object_tag
    }

    /// Returns the schema byte string of the inner canonical object.
    pub fn object_schema(&self) -> &[u8] {
        &self.object_schema
    }

    /// Returns the canonical payload bytes of the stored object.
    pub fn payload(&self) -> &[u8] {
        &self.payload
    }

    /// Returns the total serialized byte length of this storage record.
    pub fn encoded_len(&self) -> usize {
        encoded_len(
            self.parent,
            self.object_tag.kind(),
            self.object_schema.len(),
            self.payload.len(),
        )
        .expect("admitted storage-record lengths remain representable")
    }

    /// Returns the complete canonical envelope, including the full record-tag digest footer.
    pub fn encode(&self) -> Vec<u8> {
        let mut encoded = encode_prefix(
            self.parent,
            self.object_tag,
            &self.object_schema,
            &self.payload,
            self.encoded_len(),
        );
        encoded.extend_from_slice(&self.record_tag.digest());
        debug_assert_eq!(encoded.len(), self.encoded_len());
        encoded
    }

    fn build(
        parent: Option<ContentTag>,
        object_tag: ContentTag,
        object_schema: &[u8],
        payload: &[u8],
        limits: StorageLimits,
    ) -> Result<Self, StorageError> {
        let total_len = preflight_record(
            parent,
            object_tag.kind(),
            object_schema.len(),
            payload.len(),
            limits,
        )?;
        let prefix = encode_prefix(parent, object_tag, object_schema, payload, total_len);
        let record_tag = tag_prefix(&prefix)?;
        Ok(Self {
            record_tag,
            parent,
            object_tag,
            object_schema: object_schema.to_vec(),
            payload: payload.to_vec(),
        })
    }

    fn decode_prefix(
        input: &[u8],
        expected_parent: Option<ContentTag>,
        limits: StorageLimits,
    ) -> Result<(Self, usize), StorageError> {
        validate_parent_kind(expected_parent)?;
        if input.len() < HEADER_BYTES {
            return Err(StorageError::Truncated);
        }
        if &input[..STORAGE_MAGIC.len()] != STORAGE_MAGIC {
            return Err(StorageError::UnknownMagic);
        }
        let version = u16::from_be_bytes(
            input[STORAGE_MAGIC.len()..STORAGE_MAGIC.len() + 2]
                .try_into()
                .expect("version slice has fixed length"),
        );
        if version != STORAGE_VERSION {
            return Err(StorageError::UnsupportedVersion { actual: version });
        }
        let length_start = STORAGE_MAGIC.len() + 2;
        let declared_u64 = u64::from_be_bytes(
            input[length_start..HEADER_BYTES]
                .try_into()
                .expect("length slice has fixed length"),
        );
        let declared = usize::try_from(declared_u64).map_err(|_| StorageError::LengthOverflow)?;
        validate_record_len(declared, limits)?;
        if declared < MIN_RECORD_BYTES {
            return Err(StorageError::NonCanonicalLength);
        }
        if input.len() < declared {
            return Err(StorageError::Truncated);
        }

        let encoded = &input[..declared];
        let footer_start = declared
            .checked_sub(FOOTER_BYTES)
            .ok_or(StorageError::NonCanonicalLength)?;
        let mut cursor = Cursor::new(encoded, HEADER_BYTES, footer_start);

        let parent = match cursor.read_u8()? {
            0 => None,
            1 => Some(ContentTag::from_digest(
                TagKind::StorageRecord,
                cursor.read_digest()?,
            )),
            actual => return Err(StorageError::InvalidParentFlag { actual }),
        };

        let kind_len = usize::from(cursor.read_u16()?);
        if kind_len == 0 || kind_len > MAX_KIND_NAME_BYTES {
            return Err(StorageError::InvalidKindLength { actual: kind_len });
        }
        let kind_name = cursor.read(kind_len)?;
        let object_kind = TagKind::from_name(kind_name).ok_or(StorageError::UnknownObjectKind)?;
        let declared_object_digest = cursor.read_digest()?;

        let schema_len = usize::from(cursor.read_u16()?);
        if schema_len > limits.max_schema_bytes {
            return Err(StorageError::SchemaTooLarge {
                actual: schema_len,
                max: limits.max_schema_bytes,
            });
        }
        let object_schema = cursor.read(schema_len)?;

        let payload_u64 = cursor.read_u64()?;
        let payload_len = usize::try_from(payload_u64).map_err(|_| StorageError::LengthOverflow)?;
        if payload_len > limits.max_payload_bytes {
            return Err(StorageError::PayloadTooLarge {
                actual: payload_len,
                max: limits.max_payload_bytes,
            });
        }
        let payload = cursor.read(payload_len)?;
        if !cursor.is_at_end() {
            return Err(StorageError::NonCanonicalLength);
        }

        let object_tag = hash_canonical(object_kind, object_schema, payload_len as u64, |sink| {
            sink(payload)
        })?;
        if object_tag.digest() != declared_object_digest {
            return Err(StorageError::ObjectTagMismatch);
        }

        let record_tag = tag_prefix(&encoded[..footer_start])?;
        let footer_digest: [u8; DIGEST_BYTES] = encoded[footer_start..]
            .try_into()
            .expect("record footer has fixed length");
        if record_tag.digest() != footer_digest {
            return Err(StorageError::RecordTagMismatch);
        }
        if parent != expected_parent {
            return Err(StorageError::ParentMismatch {
                expected: expected_parent,
                actual: parent,
            });
        }

        let record = Self {
            record_tag,
            parent,
            object_tag,
            object_schema: object_schema.to_vec(),
            payload: payload.to_vec(),
        };
        if record.encode().as_slice() != encoded {
            return Err(StorageError::NonCanonicalEncoding);
        }
        Ok((record, declared))
    }
}

/// A bounded in-memory parent chain. It does not read or write a filesystem.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StorageChain {
    limits: StorageLimits,
    records: Vec<StorageRecord>,
    encoded_bytes: usize,
}

impl StorageChain {
    /// Creates an empty in-memory `StorageChain` with default `StorageLimits`.
    pub fn new() -> Self {
        Self::with_limits(StorageLimits::DEFAULT)
    }

    /// Creates an empty in-memory `StorageChain` enforcing explicit `StorageLimits`.
    pub fn with_limits(limits: StorageLimits) -> Self {
        Self {
            limits,
            records: Vec::new(),
            encoded_bytes: 0,
        }
    }

    /// Decodes a concatenated byte stream into a verified `StorageChain` with default limits.
    pub fn decode(encoded: &[u8]) -> Result<Self, StorageError> {
        Self::decode_with_limits(encoded, StorageLimits::DEFAULT)
    }

    /// Decodes a concatenated byte stream into a verified `StorageChain` under explicit limits.
    pub fn decode_with_limits(encoded: &[u8], limits: StorageLimits) -> Result<Self, StorageError> {
        let mut chain = Self::with_limits(limits);
        for result in StorageScanner::with_limits(encoded, None, limits) {
            chain.admit(result?)?;
        }
        Ok(chain)
    }

    /// Returns the `StorageLimits` enforced by this chain.
    pub const fn limits(&self) -> StorageLimits {
        self.limits
    }

    /// Returns the slice of admitted `StorageRecord` items in parent-chain order.
    pub fn records(&self) -> &[StorageRecord] {
        &self.records
    }

    /// Returns the `ContentTag` of the most recently admitted `StorageRecord`.
    pub fn head(&self) -> Option<ContentTag> {
        self.records.last().map(StorageRecord::record_tag)
    }

    /// Returns the cumulative serialized byte length of all records in the chain.
    pub const fn encoded_bytes(&self) -> usize {
        self.encoded_bytes
    }

    /// Encodes and appends a new payload linked to the current chain head.
    pub fn append(
        &mut self,
        object_kind: TagKind,
        object_schema: &[u8],
        payload: &[u8],
    ) -> Result<&StorageRecord, StorageError> {
        if self.records.len() >= self.limits.max_records {
            return Err(StorageError::RecordCountLimit {
                max: self.limits.max_records,
            });
        }
        let record = StorageRecord::new_with_limits(
            self.head(),
            object_kind,
            object_schema,
            payload,
            self.limits,
        )?;
        self.admit(record)
    }

    /// Verifies and appends an already constructed `StorageRecord` linked to the current head.
    pub fn admit(&mut self, record: StorageRecord) -> Result<&StorageRecord, StorageError> {
        if record.parent() != self.head() {
            return Err(StorageError::ParentMismatch {
                expected: self.head(),
                actual: record.parent(),
            });
        }
        if self.records.len() >= self.limits.max_records {
            return Err(StorageError::RecordCountLimit {
                max: self.limits.max_records,
            });
        }
        validate_component_limits(
            record.object_schema.len(),
            record.payload.len(),
            self.limits,
        )?;
        let record_len = record.encoded_len();
        validate_record_len(record_len, self.limits)?;
        let next_bytes = self
            .encoded_bytes
            .checked_add(record_len)
            .ok_or(StorageError::LengthOverflow)?;
        if next_bytes > self.limits.max_stream_bytes {
            return Err(StorageError::StreamTooLarge {
                actual: next_bytes,
                max: self.limits.max_stream_bytes,
            });
        }

        let encoded = record.encode();
        let decoded = StorageRecord::decode_with_limits(&encoded, self.head(), self.limits)?;
        if decoded != record {
            return Err(StorageError::NonCanonicalEncoding);
        }

        self.encoded_bytes = next_bytes;
        self.records.push(record);
        Ok(self
            .records
            .last()
            .expect("a just-pushed storage record is present"))
    }

    /// Serializes all admitted records into a contiguous canonical byte stream.
    pub fn encode(&self) -> Vec<u8> {
        let mut encoded = Vec::with_capacity(self.encoded_bytes);
        for record in &self.records {
            encoded.extend_from_slice(&record.encode());
        }
        debug_assert_eq!(encoded.len(), self.encoded_bytes);
        encoded
    }
}

impl Default for StorageChain {
    fn default() -> Self {
        Self::new()
    }
}

/// Fail-closed scanner for concatenated storage records.
///
/// The first invalid record is yielded as an error and permanently halts the scanner. Bytes after
/// that point are never searched for another magic header.
pub struct StorageScanner<'a> {
    encoded: &'a [u8],
    offset: usize,
    expected_parent: Option<ContentTag>,
    limits: StorageLimits,
    emitted: usize,
    halted: bool,
}

impl<'a> StorageScanner<'a> {
    /// Creates a fail-closed scanner starting from genesis (`None` parent) with default limits.
    pub fn new(encoded: &'a [u8]) -> Self {
        Self::with_limits(encoded, None, StorageLimits::DEFAULT)
    }

    /// Starts at an explicitly supplied parent, permitting bounded continuation decoding.
    pub fn after(encoded: &'a [u8], parent: ContentTag) -> Self {
        Self::with_limits(encoded, Some(parent), StorageLimits::DEFAULT)
    }

    /// Creates a fail-closed scanner starting at `expected_parent` under explicit `StorageLimits`.
    pub fn with_limits(
        encoded: &'a [u8],
        expected_parent: Option<ContentTag>,
        limits: StorageLimits,
    ) -> Self {
        Self {
            encoded,
            offset: 0,
            expected_parent,
            limits,
            emitted: 0,
            halted: false,
        }
    }

    /// Returns the current byte offset reached in the input slice.
    pub const fn offset(&self) -> usize {
        self.offset
    }

    /// Returns `true` if the scanner permanently halted after encountering an error.
    pub const fn is_halted(&self) -> bool {
        self.halted
    }
}

impl Iterator for StorageScanner<'_> {
    type Item = Result<StorageRecord, StorageError>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.halted {
            return None;
        }

        if let Err(error) = validate_parent_kind(self.expected_parent) {
            self.halted = true;
            return Some(Err(error));
        }

        if self.offset == self.encoded.len() {
            return None;
        }

        let result = if self.encoded.len() > self.limits.max_stream_bytes {
            Err(StorageError::StreamTooLarge {
                actual: self.encoded.len(),
                max: self.limits.max_stream_bytes,
            })
        } else if self.emitted >= self.limits.max_records {
            Err(StorageError::RecordCountLimit {
                max: self.limits.max_records,
            })
        } else {
            StorageRecord::decode_prefix(
                &self.encoded[self.offset..],
                self.expected_parent,
                self.limits,
            )
        };

        match result {
            Ok((record, consumed)) => {
                self.offset += consumed;
                self.expected_parent = Some(record.record_tag());
                self.emitted += 1;
                Some(Ok(record))
            }
            Err(error) => {
                self.halted = true;
                Some(Err(error))
            }
        }
    }
}

/// Error returned when constructing, encoding, decoding, or chaining storage envelopes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StorageError {
    Tag(TagError),
    LimitsExceedProtocolCaps,
    ParentKind {
        actual: TagKind,
    },
    SchemaTooLarge {
        actual: usize,
        max: usize,
    },
    PayloadTooLarge {
        actual: usize,
        max: usize,
    },
    RecordTooLarge {
        actual: usize,
        max: usize,
    },
    StreamTooLarge {
        actual: usize,
        max: usize,
    },
    RecordCountLimit {
        max: usize,
    },
    LengthOverflow,
    Truncated,
    UnknownMagic,
    UnsupportedVersion {
        actual: u16,
    },
    NonCanonicalLength,
    TrailingBytes {
        count: usize,
    },
    InvalidParentFlag {
        actual: u8,
    },
    InvalidKindLength {
        actual: usize,
    },
    UnknownObjectKind,
    ObjectTagMismatch,
    RecordTagMismatch,
    ParentMismatch {
        expected: Option<ContentTag>,
        actual: Option<ContentTag>,
    },
    NonCanonicalEncoding,
}

impl fmt::Display for StorageError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Tag(error) => write!(formatter, "storage tag error: {error}"),
            Self::LimitsExceedProtocolCaps => {
                formatter.write_str("storage limits exceed hard protocol caps")
            }
            Self::ParentKind { actual } => write!(
                formatter,
                "storage parent must be storage-record, received {}",
                actual.name()
            ),
            Self::SchemaTooLarge { actual, max } => {
                write!(
                    formatter,
                    "storage object schema has {actual} bytes; cap is {max}"
                )
            }
            Self::PayloadTooLarge { actual, max } => {
                write!(
                    formatter,
                    "storage payload has {actual} bytes; cap is {max}"
                )
            }
            Self::RecordTooLarge { actual, max } => {
                write!(formatter, "storage record has {actual} bytes; cap is {max}")
            }
            Self::StreamTooLarge { actual, max } => {
                write!(formatter, "storage stream has {actual} bytes; cap is {max}")
            }
            Self::RecordCountLimit { max } => {
                write!(formatter, "storage record count exceeds cap {max}")
            }
            Self::LengthOverflow => formatter.write_str("storage length is not representable"),
            Self::Truncated => formatter.write_str("storage record is truncated"),
            Self::UnknownMagic => formatter.write_str("unknown storage-record magic"),
            Self::UnsupportedVersion { actual } => {
                write!(formatter, "unsupported storage-record version {actual}")
            }
            Self::NonCanonicalLength => {
                formatter.write_str("storage record has a noncanonical declared length")
            }
            Self::TrailingBytes { count } => {
                write!(formatter, "storage record has {count} trailing bytes")
            }
            Self::InvalidParentFlag { actual } => {
                write!(formatter, "invalid storage parent flag {actual}")
            }
            Self::InvalidKindLength { actual } => {
                write!(formatter, "invalid storage object-kind length {actual}")
            }
            Self::UnknownObjectKind => formatter.write_str("unknown storage object kind"),
            Self::ObjectTagMismatch => {
                formatter.write_str("stored object tag does not match schema and payload")
            }
            Self::RecordTagMismatch => {
                formatter.write_str("storage-record footer does not match its canonical bytes")
            }
            Self::ParentMismatch { expected, actual } => write!(
                formatter,
                "storage parent mismatch: expected {expected:?}, received {actual:?}"
            ),
            Self::NonCanonicalEncoding => {
                formatter.write_str("storage record does not round-trip canonically")
            }
        }
    }
}

impl std::error::Error for StorageError {}

impl From<TagError> for StorageError {
    fn from(error: TagError) -> Self {
        Self::Tag(error)
    }
}

fn validate_parent_kind(parent: Option<ContentTag>) -> Result<(), StorageError> {
    if let Some(parent) = parent
        && parent.kind() != TagKind::StorageRecord
    {
        return Err(StorageError::ParentKind {
            actual: parent.kind(),
        });
    }
    Ok(())
}

fn validate_component_limits(
    schema_len: usize,
    payload_len: usize,
    limits: StorageLimits,
) -> Result<(), StorageError> {
    if schema_len > limits.max_schema_bytes {
        return Err(StorageError::SchemaTooLarge {
            actual: schema_len,
            max: limits.max_schema_bytes,
        });
    }
    if payload_len > limits.max_payload_bytes {
        return Err(StorageError::PayloadTooLarge {
            actual: payload_len,
            max: limits.max_payload_bytes,
        });
    }
    Ok(())
}

fn preflight_record(
    parent: Option<ContentTag>,
    object_kind: TagKind,
    schema_len: usize,
    payload_len: usize,
    limits: StorageLimits,
) -> Result<usize, StorageError> {
    validate_parent_kind(parent)?;
    validate_component_limits(schema_len, payload_len, limits)?;
    let record_len = encoded_len(parent, object_kind, schema_len, payload_len)?;
    validate_record_len(record_len, limits)?;
    Ok(record_len)
}

fn validate_record_len(record_len: usize, limits: StorageLimits) -> Result<(), StorageError> {
    if record_len > limits.max_record_bytes {
        return Err(StorageError::RecordTooLarge {
            actual: record_len,
            max: limits.max_record_bytes,
        });
    }
    Ok(())
}

fn encoded_len(
    parent: Option<ContentTag>,
    object_kind: TagKind,
    schema_len: usize,
    payload_len: usize,
) -> Result<usize, StorageError> {
    let parent_len = if parent.is_some() { DIGEST_BYTES } else { 0 };
    [
        HEADER_BYTES,
        1,
        parent_len,
        2,
        object_kind.name().len(),
        DIGEST_BYTES,
        2,
        schema_len,
        8,
        payload_len,
        FOOTER_BYTES,
    ]
    .into_iter()
    .try_fold(0_usize, |sum, field| sum.checked_add(field))
    .ok_or(StorageError::LengthOverflow)
}

fn encode_prefix(
    parent: Option<ContentTag>,
    object_tag: ContentTag,
    object_schema: &[u8],
    payload: &[u8],
    total_len: usize,
) -> Vec<u8> {
    let schema_len =
        u16::try_from(object_schema.len()).expect("admitted storage schema is bounded below u16");
    let kind_name = object_tag.kind().name().as_bytes();
    let kind_len = u16::try_from(kind_name.len()).expect("closed tag kind exceeds u16");
    let total_len = u64::try_from(total_len).expect("admitted storage length fits u64");
    let payload_len = u64::try_from(payload.len()).expect("admitted storage payload fits u64");

    let mut encoded = Vec::with_capacity(total_len as usize - FOOTER_BYTES);
    encoded.extend_from_slice(STORAGE_MAGIC);
    encoded.extend_from_slice(&STORAGE_VERSION.to_be_bytes());
    encoded.extend_from_slice(&total_len.to_be_bytes());
    match parent {
        None => encoded.push(0),
        Some(parent) => {
            encoded.push(1);
            encoded.extend_from_slice(&parent.digest());
        }
    }
    encoded.extend_from_slice(&kind_len.to_be_bytes());
    encoded.extend_from_slice(kind_name);
    encoded.extend_from_slice(&object_tag.digest());
    encoded.extend_from_slice(&schema_len.to_be_bytes());
    encoded.extend_from_slice(object_schema);
    encoded.extend_from_slice(&payload_len.to_be_bytes());
    encoded.extend_from_slice(payload);
    debug_assert_eq!(encoded.len() + FOOTER_BYTES, total_len as usize);
    encoded
}

fn tag_prefix(prefix: &[u8]) -> Result<ContentTag, StorageError> {
    Ok(hash_canonical(
        TagKind::StorageRecord,
        STORAGE_RECORD_SCHEMA,
        prefix.len() as u64,
        |sink| sink(prefix),
    )?)
}

struct Cursor<'a> {
    encoded: &'a [u8],
    position: usize,
    end: usize,
}

impl<'a> Cursor<'a> {
    const fn new(encoded: &'a [u8], position: usize, end: usize) -> Self {
        Self {
            encoded,
            position,
            end,
        }
    }

    fn read(&mut self, count: usize) -> Result<&'a [u8], StorageError> {
        let next = self
            .position
            .checked_add(count)
            .ok_or(StorageError::LengthOverflow)?;
        if next > self.end {
            return Err(StorageError::Truncated);
        }
        let bytes = &self.encoded[self.position..next];
        self.position = next;
        Ok(bytes)
    }

    fn read_u8(&mut self) -> Result<u8, StorageError> {
        Ok(self.read(1)?[0])
    }

    fn read_u16(&mut self) -> Result<u16, StorageError> {
        Ok(u16::from_be_bytes(
            self.read(2)?.try_into().expect("cursor returned two bytes"),
        ))
    }

    fn read_u64(&mut self) -> Result<u64, StorageError> {
        Ok(u64::from_be_bytes(
            self.read(8)?
                .try_into()
                .expect("cursor returned eight bytes"),
        ))
    }

    fn read_digest(&mut self) -> Result<[u8; DIGEST_BYTES], StorageError> {
        Ok(self
            .read(DIGEST_BYTES)?
            .try_into()
            .expect("cursor returned one digest"))
    }

    const fn is_at_end(&self) -> bool {
        self.position == self.end
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::program::{EDIT_SCHEMA, Edit, Operation};
    use crate::world::Coord;

    const OBJECT_SCHEMA: &[u8] = b"bitblock/test-object/v1";

    fn record(parent: Option<ContentTag>, payload: &[u8]) -> StorageRecord {
        StorageRecord::new(parent, TagKind::World, OBJECT_SCHEMA, payload).unwrap()
    }

    fn chain(payloads: &[&[u8]]) -> StorageChain {
        let mut chain = StorageChain::new();
        for payload in payloads {
            chain
                .append(TagKind::World, OBJECT_SCHEMA, payload)
                .unwrap();
        }
        chain
    }

    fn replace_footer(encoded: &mut [u8]) {
        let footer = encoded.len() - FOOTER_BYTES;
        let tag = tag_prefix(&encoded[..footer]).unwrap();
        encoded[footer..].copy_from_slice(&tag.digest());
    }

    /// Cross-check against tools/storage_record_vectors.py, which rebuilds the whole
    /// envelope -- magic, version, declared length, parent flag, kind name, object digest,
    /// schema, payload, and the record-tag footer -- without importing any Rust. The inner
    /// object is the `on 0 0 0` edit from tools/edit_vectors.py, so the two suites compose
    /// rather than each inventing a payload.
    #[test]
    fn envelope_tag_matches_the_independent_vector() {
        let edit = Edit::new(Operation::On, Coord::new(0, 0, 0));
        let record =
            StorageRecord::new(None, TagKind::Edit, EDIT_SCHEMA, &edit.encode_canonical()).unwrap();
        assert_eq!(
            record.record_tag().to_string(),
            "#bb.storage-record.sha256.3rmsngivykmpjsnfcz2gn7p6mtqvqiygjvxbpcjjnvmxsdbf6uiq"
        );
        assert_eq!(record.encode().len(), 153);
    }

    #[test]
    fn canonical_record_round_trips_with_named_kind_and_full_tags() {
        let first = record(None, b"alpha");
        let duplicate = record(None, b"alpha");
        let second = record(Some(first.record_tag()), b"beta");
        let encoded = second.encode();

        assert_eq!(&encoded[..STORAGE_MAGIC.len()], STORAGE_MAGIC);
        assert!(
            encoded
                .windows(TagKind::World.name().len())
                .any(|window| window == TagKind::World.name().as_bytes())
        );
        assert!(
            encoded
                .windows(second.object_tag().digest().len())
                .any(|window| window == second.object_tag().digest())
        );
        assert_eq!(
            StorageRecord::decode(&encoded, Some(first.record_tag())).unwrap(),
            second
        );
        assert_eq!(second.record_tag().kind(), TagKind::StorageRecord);
        assert_eq!(first.record_tag(), duplicate.record_tag());
        assert_ne!(first.record_tag(), second.record_tag());
    }

    #[test]
    fn every_proper_prefix_truncation_is_rejected() {
        let record = record(None, b"powered-truncation-control");
        let encoded = record.encode();
        assert!(StorageRecord::decode(&encoded, None).is_ok());

        for cut in 0..encoded.len() {
            assert!(
                StorageRecord::decode(&encoded[..cut], None).is_err(),
                "truncation at byte {cut} was admitted"
            );
        }
    }

    #[test]
    fn every_single_bit_flip_is_rejected() {
        let record = record(None, b"powered-bit-flip-control");
        let encoded = record.encode();
        assert!(StorageRecord::decode(&encoded, None).is_ok());

        for byte_index in 0..encoded.len() {
            for bit in 0..8 {
                let mut mutated = encoded.clone();
                mutated[byte_index] ^= 1 << bit;
                assert!(
                    StorageRecord::decode(&mutated, None).is_err(),
                    "bit {bit} of byte {byte_index} was admitted"
                );
            }
        }
    }

    #[test]
    fn every_distinct_prefix_suffix_splice_is_rejected() {
        let left = record(None, b"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa").encode();
        let right = record(None, b"bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb").encode();
        assert_eq!(left.len(), right.len());
        assert!(StorageRecord::decode(&left, None).is_ok());
        assert!(StorageRecord::decode(&right, None).is_ok());

        let mut tested = 0;
        for cut in 1..left.len() {
            let mut splice = left[..cut].to_vec();
            splice.extend_from_slice(&right[cut..]);
            if splice == left || splice == right {
                continue;
            }
            tested += 1;
            assert!(
                StorageRecord::decode(&splice, None).is_err(),
                "splice at byte {cut} was admitted"
            );
        }
        assert!(tested > 64, "powered splice control exercised too few cuts");
    }

    #[test]
    fn swapping_equal_width_fields_fails_closed() {
        let parent = record(None, b"parent");
        let child = record(Some(parent.record_tag()), b"child");
        let mut encoded = child.encode();
        let parent_start = HEADER_BYTES + 1;
        let kind_len_start = parent_start + DIGEST_BYTES;
        let kind_len = usize::from(u16::from_be_bytes(
            encoded[kind_len_start..kind_len_start + 2]
                .try_into()
                .unwrap(),
        ));
        let object_digest_start = kind_len_start + 2 + kind_len;
        for offset in 0..DIGEST_BYTES {
            encoded.swap(parent_start + offset, object_digest_start + offset);
        }

        assert!(
            StorageRecord::decode(&encoded, Some(parent.record_tag())).is_err(),
            "field-order mutation was admitted"
        );
    }

    #[test]
    fn nested_object_tag_is_checked_even_with_a_repaired_record_footer() {
        let original = record(None, b"nested-object-check");
        let mut encoded = original.encode();
        let kind_len_start = HEADER_BYTES + 1;
        let kind_len = usize::from(u16::from_be_bytes(
            encoded[kind_len_start..kind_len_start + 2]
                .try_into()
                .unwrap(),
        ));
        let object_digest_start = kind_len_start + 2 + kind_len;
        encoded[object_digest_start] ^= 1;
        replace_footer(&mut encoded);

        assert_eq!(
            StorageRecord::decode(&encoded, None),
            Err(StorageError::ObjectTagMismatch)
        );
    }

    #[test]
    fn scanner_rejects_spliced_parent_chain_and_never_resynchronizes() {
        let left = chain(&[b"left-0", b"left-1"]);
        let right = chain(&[b"right-0", b"right-1"]);
        let mut spliced = left.records()[0].encode();
        spliced.extend_from_slice(&right.records()[1].encode());
        spliced.extend_from_slice(&left.records()[1].encode());

        let mut scanner = StorageScanner::new(&spliced);
        assert_eq!(
            scanner.next().unwrap().unwrap().record_tag(),
            left.records()[0].record_tag()
        );
        assert!(matches!(
            scanner.next(),
            Some(Err(StorageError::ParentMismatch { .. }))
        ));
        let halt_offset = scanner.offset();
        assert!(scanner.is_halted());
        assert!(scanner.next().is_none());
        assert_eq!(scanner.offset(), halt_offset);
    }

    #[test]
    fn scanner_stops_at_corruption_without_searching_for_later_magic() {
        let source = chain(&[b"first", b"second", b"third"]);
        let mut encoded = source.encode();
        let first_len = source.records()[0].encoded_len();
        encoded[first_len] ^= 1;

        let mut scanner = StorageScanner::new(&encoded);
        assert!(scanner.next().unwrap().is_ok());
        assert_eq!(scanner.next(), Some(Err(StorageError::UnknownMagic)));
        assert!(scanner.next().is_none());
        assert_eq!(scanner.offset(), first_len);
    }

    #[test]
    fn empty_continuation_still_rejects_a_non_storage_parent() {
        let wrong_parent = hash_canonical(TagKind::World, b"wrong-parent/v1", 0, |_| {}).unwrap();
        let mut scanner =
            StorageScanner::with_limits(&[], Some(wrong_parent), StorageLimits::DEFAULT);

        assert_eq!(
            scanner.next(),
            Some(Err(StorageError::ParentKind {
                actual: TagKind::World,
            }))
        );
        assert!(scanner.is_halted());
        assert!(scanner.next().is_none());
        assert_eq!(scanner.offset(), 0);
    }

    #[test]
    fn decoder_rejects_unknown_header_lengths_trailing_bytes_and_wrong_parent() {
        let genesis = record(None, b"genesis");
        let encoded = genesis.encode();

        let mut bad_magic = encoded.clone();
        bad_magic[0] ^= 1;
        assert_eq!(
            StorageRecord::decode(&bad_magic, None),
            Err(StorageError::UnknownMagic)
        );

        let mut bad_version = encoded.clone();
        bad_version[STORAGE_MAGIC.len() + 1] = 2;
        assert_eq!(
            StorageRecord::decode(&bad_version, None),
            Err(StorageError::UnsupportedVersion { actual: 2 })
        );

        let mut trailing = encoded.clone();
        trailing.push(0);
        assert_eq!(
            StorageRecord::decode(&trailing, None),
            Err(StorageError::TrailingBytes { count: 1 })
        );

        let child = record(Some(genesis.record_tag()), b"child");
        assert!(matches!(
            StorageRecord::decode(&child.encode(), None),
            Err(StorageError::ParentMismatch { .. })
        ));

        let payload_len_offset = HEADER_BYTES
            + 1
            + 2
            + TagKind::World.name().len()
            + DIGEST_BYTES
            + 2
            + OBJECT_SCHEMA.len();
        let mut bad_payload_len = encoded.clone();
        let old_len = u64::from_be_bytes(
            bad_payload_len[payload_len_offset..payload_len_offset + 8]
                .try_into()
                .unwrap(),
        );
        bad_payload_len[payload_len_offset..payload_len_offset + 8]
            .copy_from_slice(&(old_len - 1).to_be_bytes());
        assert_eq!(
            StorageRecord::decode(&bad_payload_len, None),
            Err(StorageError::NonCanonicalLength)
        );
    }

    #[test]
    fn construction_decode_and_chain_admission_enforce_all_limits() {
        let probe = record(None, b"12345678");
        let exact = probe.encoded_len();
        let probe_bytes = probe.encode();
        let limits = StorageLimits::new(exact, OBJECT_SCHEMA.len(), 8, 1, exact).unwrap();
        assert!(
            StorageRecord::new_with_limits(
                None,
                TagKind::World,
                OBJECT_SCHEMA,
                b"12345678",
                limits
            )
            .is_ok()
        );
        assert!(matches!(
            StorageRecord::new_with_limits(
                None,
                TagKind::World,
                &[b'x'; OBJECT_SCHEMA.len() + 1],
                b"12345678",
                limits
            ),
            Err(StorageError::SchemaTooLarge { .. })
        ));
        assert!(matches!(
            StorageRecord::new_with_limits(
                None,
                TagKind::World,
                OBJECT_SCHEMA,
                b"123456789",
                limits
            ),
            Err(StorageError::PayloadTooLarge { .. })
        ));

        let short_record = StorageLimits::new(exact - 1, OBJECT_SCHEMA.len(), 8, 1, exact).unwrap();
        assert!(matches!(
            StorageRecord::decode_with_limits(&probe_bytes, None, short_record),
            Err(StorageError::RecordTooLarge { .. })
        ));
        let short_schema = StorageLimits::new(exact, OBJECT_SCHEMA.len() - 1, 8, 1, exact).unwrap();
        assert!(matches!(
            StorageRecord::decode_with_limits(&probe_bytes, None, short_schema),
            Err(StorageError::SchemaTooLarge { .. })
        ));
        let short_payload = StorageLimits::new(exact, OBJECT_SCHEMA.len(), 7, 1, exact).unwrap();
        assert!(matches!(
            StorageRecord::decode_with_limits(&probe_bytes, None, short_payload),
            Err(StorageError::PayloadTooLarge { .. })
        ));
        let short_stream = StorageLimits::new(exact, OBJECT_SCHEMA.len(), 8, 1, exact - 1).unwrap();
        let mut scanner = StorageScanner::with_limits(&probe_bytes, None, short_stream);
        assert!(matches!(
            scanner.next(),
            Some(Err(StorageError::StreamTooLarge { .. }))
        ));
        assert!(scanner.next().is_none());

        let mut chain = StorageChain::with_limits(limits);
        chain
            .append(TagKind::World, OBJECT_SCHEMA, b"12345678")
            .unwrap();
        assert!(matches!(
            chain.append(TagKind::World, OBJECT_SCHEMA, b"12345678"),
            Err(StorageError::RecordCountLimit { max: 1 })
        ));

        assert_eq!(
            StorageLimits::new(
                MAX_STORAGE_RECORD_BYTES + 1,
                MAX_STORAGE_SCHEMA_BYTES,
                MAX_STORAGE_PAYLOAD_BYTES,
                MAX_STORAGE_RECORDS,
                MAX_STORAGE_STREAM_BYTES,
            ),
            Err(StorageError::LimitsExceedProtocolCaps)
        );
    }

    #[test]
    fn chain_round_trip_preserves_parent_tags_and_payloads() {
        let source = chain(&[b"zero", b"one", b"two"]);
        let encoded = source.encode();
        let decoded = StorageChain::decode(&encoded).unwrap();
        assert_eq!(decoded, source);
        assert_eq!(decoded.records()[0].parent(), None);
        assert_eq!(
            decoded.records()[1].parent(),
            Some(decoded.records()[0].record_tag())
        );
        assert_eq!(
            decoded.records()[2].parent(),
            Some(decoded.records()[1].record_tag())
        );
    }
}
