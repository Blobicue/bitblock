//! LOCAL-BOUNDS-1: local record, history, and recovery limits.
//!
//! Numeric limits for canonical tick/ledger record bytes, records per commit, staged
//! writes, retained local history, restart scan work, backup/restore work, and local pruning
//! batches, with checked preallocation arithmetic and a versioned local-storage profile.
//!
//! Every limit is checked before allocation or mutation; records exceeding a limit fail closed.
//! Local storage limits do not establish network consensus safety, storage-fee sufficiency,
//! or production capacity.

use std::fmt;

use bitblock_core::protocol::{ContentTag, TagKind, hash_canonical};

use crate::history_store::HistoryReplayLimits;
use crate::storage::{MAX_STORAGE_PAYLOAD_BYTES, MAX_STORAGE_SCHEMA_BYTES, StorageLimits};

/// Schema for the canonical local bounds profile.
pub const LOCAL_BOUNDS_PROFILE_SCHEMA: &[u8] = b"bitblock/local-bounds-profile/v1";

pub const DEFAULT_MAX_RECORD_BYTES: u64 = 16 * 1024 * 1024;
pub const DEFAULT_MAX_RECORDS_PER_COMMIT: u64 = 65_536;
pub const DEFAULT_MAX_STAGED_WRITE_BYTES: u64 = 64 * 1024 * 1024;
pub const DEFAULT_MAX_RETAINED_HISTORY_RECORDS: u64 = 262_144;
pub const DEFAULT_MAX_RESTART_SCAN_BYTES: u64 = 512 * 1024 * 1024;
pub const DEFAULT_MAX_BACKUP_RESTORE_BYTES: u64 = 1024 * 1024 * 1024;
pub const DEFAULT_MAX_PRUNING_BATCH_RECORDS: u64 = 4_096;

pub const LOCAL_BOUNDS_PROFILE_PAYLOAD_LEN: usize = 7 * size_of::<u64>();

/// Local storage profile defining exact operational and recovery bounds.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LocalStorageProfile {
    pub max_record_bytes: u64,
    pub max_records_per_commit: u64,
    pub max_staged_write_bytes: u64,
    pub max_retained_history_records: u64,
    pub max_restart_scan_bytes: u64,
    pub max_backup_restore_bytes: u64,
    pub max_pruning_batch_records: u64,
}

impl Default for LocalStorageProfile {
    fn default() -> Self {
        Self::DEFAULT
    }
}

impl LocalStorageProfile {
    pub const DEFAULT: Self = Self {
        max_record_bytes: DEFAULT_MAX_RECORD_BYTES,
        max_records_per_commit: DEFAULT_MAX_RECORDS_PER_COMMIT,
        max_staged_write_bytes: DEFAULT_MAX_STAGED_WRITE_BYTES,
        max_retained_history_records: DEFAULT_MAX_RETAINED_HISTORY_RECORDS,
        max_restart_scan_bytes: DEFAULT_MAX_RESTART_SCAN_BYTES,
        max_backup_restore_bytes: DEFAULT_MAX_BACKUP_RESTORE_BYTES,
        max_pruning_batch_records: DEFAULT_MAX_PRUNING_BATCH_RECORDS,
    };

    /// Encodes this profile into 56 canonical big-endian bytes.
    pub fn encode_canonical(&self) -> [u8; LOCAL_BOUNDS_PROFILE_PAYLOAD_LEN] {
        let mut buf = [0u8; LOCAL_BOUNDS_PROFILE_PAYLOAD_LEN];
        buf[0..8].copy_from_slice(&self.max_record_bytes.to_be_bytes());
        buf[8..16].copy_from_slice(&self.max_records_per_commit.to_be_bytes());
        buf[16..24].copy_from_slice(&self.max_staged_write_bytes.to_be_bytes());
        buf[24..32].copy_from_slice(&self.max_retained_history_records.to_be_bytes());
        buf[32..40].copy_from_slice(&self.max_restart_scan_bytes.to_be_bytes());
        buf[40..48].copy_from_slice(&self.max_backup_restore_bytes.to_be_bytes());
        buf[48..56].copy_from_slice(&self.max_pruning_batch_records.to_be_bytes());
        buf
    }

    /// Decodes a 56-byte canonical profile payload.
    pub fn decode_canonical(bytes: &[u8]) -> Result<Self, BoundsError> {
        if bytes.len() != LOCAL_BOUNDS_PROFILE_PAYLOAD_LEN {
            return Err(BoundsError::InvalidPayloadLength {
                expected: LOCAL_BOUNDS_PROFILE_PAYLOAD_LEN,
                actual: bytes.len(),
            });
        }
        let read_u64 = |idx: usize| -> u64 {
            let mut b = [0u8; 8];
            b.copy_from_slice(&bytes[idx..idx + 8]);
            u64::from_be_bytes(b)
        };
        Ok(Self {
            max_record_bytes: read_u64(0),
            max_records_per_commit: read_u64(8),
            max_staged_write_bytes: read_u64(16),
            max_retained_history_records: read_u64(24),
            max_restart_scan_bytes: read_u64(32),
            max_backup_restore_bytes: read_u64(40),
            max_pruning_batch_records: read_u64(48),
        })
    }

    /// Canonical ContentTag for this profile.
    pub fn tag(&self) -> ContentTag {
        let payload = self.encode_canonical();
        hash_canonical(
            TagKind::Policy,
            LOCAL_BOUNDS_PROFILE_SCHEMA,
            payload.len() as u64,
            |sink| sink(&payload),
        )
        .expect("canonical local bounds profile tag must compute")
    }

    /// Checked preallocation arithmetic for staged writes.
    ///
    /// Returns the required bytes on success, or fails closed if multiplication overflows
    /// or exceeds the profile limit.
    pub fn check_staged_allocation(
        &self,
        count: u64,
        bytes_per_record: u64,
    ) -> Result<u64, BoundsError> {
        let total = count
            .checked_mul(bytes_per_record)
            .ok_or(BoundsError::AllocationOverflow)?;
        if total > self.max_staged_write_bytes {
            return Err(BoundsError::StagedWriteExceeded {
                requested: total,
                limit: self.max_staged_write_bytes,
            });
        }
        Ok(total)
    }

    /// Checks whether scan progress is within the restart scan limits.
    pub fn check_restart_scan(
        &self,
        scanned_bytes: u64,
        scanned_records: u64,
    ) -> Result<(), BoundsError> {
        if scanned_bytes > self.max_restart_scan_bytes {
            return Err(BoundsError::RestartScanBytesExceeded {
                scanned: scanned_bytes,
                limit: self.max_restart_scan_bytes,
            });
        }
        if scanned_records > self.max_retained_history_records {
            return Err(BoundsError::RestartScanRecordsExceeded {
                scanned: scanned_records,
                limit: self.max_retained_history_records,
            });
        }
        Ok(())
    }

    /// Checks whether a pruning batch size is within bounds.
    pub fn check_prune_batch(&self, batch_records: u64) -> Result<(), BoundsError> {
        if batch_records > self.max_pruning_batch_records {
            return Err(BoundsError::PruningBatchExceeded {
                requested: batch_records,
                limit: self.max_pruning_batch_records,
            });
        }
        Ok(())
    }

    /// Translates this profile into storage envelope limits.
    pub fn to_storage_limits(&self) -> Result<StorageLimits, BoundsError> {
        let record_bytes = usize::try_from(self.max_record_bytes)
            .map_err(|_| BoundsError::LimitConversionOverflow)?;
        let records = usize::try_from(self.max_records_per_commit)
            .map_err(|_| BoundsError::LimitConversionOverflow)?;
        let stream_bytes = usize::try_from(self.max_staged_write_bytes)
            .map_err(|_| BoundsError::LimitConversionOverflow)?;

        StorageLimits::new(
            record_bytes,
            MAX_STORAGE_SCHEMA_BYTES,
            MAX_STORAGE_PAYLOAD_BYTES.min(record_bytes),
            records,
            stream_bytes,
        )
        .map_err(BoundsError::StorageLimitsInvalid)
    }

    /// Translates this profile into history replay limits.
    pub fn to_history_replay_limits(&self) -> Result<HistoryReplayLimits, BoundsError> {
        let receipts = usize::try_from(self.max_retained_history_records)
            .map_err(|_| BoundsError::LimitConversionOverflow)?;

        Ok(HistoryReplayLimits::new(
            receipts,
            256,
            receipts.saturating_mul(256),
            receipts,
            receipts,
            receipts.saturating_mul(16),
        ))
    }
}

/// Recovery scanner state that bounds reading on unanchored or malicious files.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BoundedRecoveryScanner {
    profile: LocalStorageProfile,
    scanned_bytes: u64,
    scanned_records: u64,
}

impl BoundedRecoveryScanner {
    pub const fn new(profile: LocalStorageProfile) -> Self {
        Self {
            profile,
            scanned_bytes: 0,
            scanned_records: 0,
        }
    }

    pub const fn scanned_bytes(&self) -> u64 {
        self.scanned_bytes
    }

    pub const fn scanned_records(&self) -> u64 {
        self.scanned_records
    }

    /// Records additional scanned bytes, checking against restart scan budget.
    pub fn record_bytes(&mut self, bytes: u64) -> Result<(), BoundsError> {
        self.scanned_bytes = self
            .scanned_bytes
            .checked_add(bytes)
            .ok_or(BoundsError::AllocationOverflow)?;
        self.profile
            .check_restart_scan(self.scanned_bytes, self.scanned_records)
    }

    /// Records an admitted record, checking against history records budget.
    pub fn record_record(&mut self) -> Result<(), BoundsError> {
        self.scanned_records = self
            .scanned_records
            .checked_add(1)
            .ok_or(BoundsError::AllocationOverflow)?;
        self.profile
            .check_restart_scan(self.scanned_bytes, self.scanned_records)
    }
}

/// Errors originating from local storage and recovery bounds checks.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BoundsError {
    InvalidPayloadLength { expected: usize, actual: usize },
    AllocationOverflow,
    StagedWriteExceeded { requested: u64, limit: u64 },
    RestartScanBytesExceeded { scanned: u64, limit: u64 },
    RestartScanRecordsExceeded { scanned: u64, limit: u64 },
    PruningBatchExceeded { requested: u64, limit: u64 },
    LimitConversionOverflow,
    StorageLimitsInvalid(crate::storage::StorageError),
}

impl fmt::Display for BoundsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidPayloadLength { expected, actual } => {
                write!(
                    f,
                    "invalid profile payload length: expected {expected}, got {actual}"
                )
            }
            Self::AllocationOverflow => {
                write!(f, "arithmetic overflow during allocation check")
            }
            Self::StagedWriteExceeded { requested, limit } => {
                write!(
                    f,
                    "staged write allocation of {requested} bytes exceeds limit of {limit}"
                )
            }
            Self::RestartScanBytesExceeded { scanned, limit } => {
                write!(
                    f,
                    "restart scan consumed {scanned} bytes, exceeding limit of {limit}"
                )
            }
            Self::RestartScanRecordsExceeded { scanned, limit } => {
                write!(
                    f,
                    "restart scan reached {scanned} records, exceeding limit of {limit}"
                )
            }
            Self::PruningBatchExceeded { requested, limit } => {
                write!(
                    f,
                    "pruning batch of {requested} records exceeds limit of {limit}"
                )
            }
            Self::LimitConversionOverflow => {
                write!(f, "profile limit exceeds usize address space")
            }
            Self::StorageLimitsInvalid(err) => {
                write!(f, "invalid storage limits derived from profile: {err}")
            }
        }
    }
}

impl std::error::Error for BoundsError {}

#[cfg(test)]
mod tests {
    use super::*;

    const EXPECTED_DEFAULT_TAG: &str =
        "#bb.policy.sha256.gvc7zlvkyaeidfzsivb4xdlx32wc5q7kmfmlk274cd6szzhgfx7a";

    #[test]
    fn default_profile_tag_matches_independent_vector() {
        let profile = LocalStorageProfile::DEFAULT;
        let tag = profile.tag().to_string();
        assert_eq!(tag, EXPECTED_DEFAULT_TAG);
    }

    #[test]
    fn canonical_round_trip() {
        let profile = LocalStorageProfile::DEFAULT;
        let bytes = profile.encode_canonical();
        assert_eq!(bytes.len(), LOCAL_BOUNDS_PROFILE_PAYLOAD_LEN);
        let decoded = LocalStorageProfile::decode_canonical(&bytes).unwrap();
        assert_eq!(decoded, profile);
    }

    #[test]
    fn preallocation_checks_overflow_and_bounds() {
        let profile = LocalStorageProfile::DEFAULT;

        // Normal within limit
        let bytes = profile.check_staged_allocation(100, 1024).unwrap();
        assert_eq!(bytes, 102400);

        // Exceeds limit
        let err = profile
            .check_staged_allocation(1_000_000, 1024)
            .unwrap_err();
        assert!(matches!(err, BoundsError::StagedWriteExceeded { .. }));

        // Arithmetic overflow
        let err = profile.check_staged_allocation(u64::MAX, 2).unwrap_err();
        assert_eq!(err, BoundsError::AllocationOverflow);
    }

    #[test]
    fn recovery_scanner_stops_at_scan_bounds() {
        let profile = LocalStorageProfile {
            max_restart_scan_bytes: 1024,
            max_retained_history_records: 5,
            ..LocalStorageProfile::DEFAULT
        };
        let mut scanner = BoundedRecoveryScanner::new(profile);

        assert!(scanner.record_bytes(500).is_ok());
        assert!(scanner.record_record().is_ok());
        assert!(scanner.record_bytes(500).is_ok());
        // Exceed byte limit
        assert!(scanner.record_bytes(25).is_err());

        let mut record_scanner = BoundedRecoveryScanner::new(profile);
        for _ in 0..5 {
            assert!(record_scanner.record_record().is_ok());
        }
        // Exceed record limit
        assert!(record_scanner.record_record().is_err());
    }

    #[test]
    fn conversion_to_storage_and_replay_limits() {
        let profile = LocalStorageProfile::DEFAULT;
        let storage_limits = profile.to_storage_limits().unwrap();
        assert_eq!(
            storage_limits.max_record_bytes(),
            DEFAULT_MAX_RECORD_BYTES as usize
        );

        let replay_limits = profile.to_history_replay_limits().unwrap();
        assert_eq!(
            replay_limits.max_receipts(),
            DEFAULT_MAX_RETAINED_HISTORY_RECORDS as usize
        );
    }

    #[test]
    fn pruning_batch_limit_checks() {
        let profile = LocalStorageProfile::DEFAULT;
        assert!(profile.check_prune_batch(100).is_ok());
        assert!(
            profile
                .check_prune_batch(DEFAULT_MAX_PRUNING_BATCH_RECORDS)
                .is_ok()
        );
        let err = profile
            .check_prune_batch(DEFAULT_MAX_PRUNING_BATCH_RECORDS + 1)
            .unwrap_err();
        assert!(matches!(err, BoundsError::PruningBatchExceeded { .. }));
    }
}
