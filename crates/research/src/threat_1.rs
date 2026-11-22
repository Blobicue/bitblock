//! `THREAT-1`: quantitative adversary model and 11-class security claim map filling the
//! `OPEN (UNQUANTIFIED)` parameter slots of `docs/THREAT_D0_ENVELOPE.md`.
//!
//! Binds the v1 conservative consumer-node BFT adversary envelope selected over
//! `PROFILE-SCOPE-D0`:
//! - Strict `< 1/3` Byzantine stake weight (`max_corrupt_weight_bps = 3333`, quorum `> 2/3`).
//! - `2_000 ms` target block interval, `10_000 ms` GST upper network-delay bound, `>= 1` epoch
//!   adaptive corruption delay, `21`-epoch unbonding (`<= 28`-epoch rolling retention).
//! - `>= 8` honest peer connections and `3-of-5` independent deterministic release builders.
//! - Staged censorship model: `SEQUENCER-1` explicitly carries zero censorship-resistance
//!   guarantee (`sequencer_censorship_guaranteed = false`); `CONSENSUS-1` enforces weighted
//!   proposer rotation with bounded inclusion `<= 3 * active_validators` blocks under GST.
//! - `128`-bit classical Ed25519 + SHA-256 suite assumption and `external_bridges_admitted = false`.
//!
//! Every public property maps to an exact numeric threshold, loss condition, mitigation code, and
//! explicit exclusion flag. A threat model quantifies assumptions; it does not prove security.

use std::fmt;

use crate::asset_d1::ValidatorEconPolicyV1;
use crate::consensus_gov_d1::ConsensusPolicyDescriptorV1;
use crate::profile_bootstrap::ProfileScopeV1;
use crate::protocol::{ContentTag, TagKind, hash_canonical};

/// Canonical schema string for a [`ThreatModelV1`] quantitative adversary policy.
pub const THREAT_1_MODEL_SCHEMA: &[u8] = b"bitblock/threat-1/adversary-model/v1";
/// Canonical schema string for a [`ThreatClaimMapV1`] 11-class property/loss/mitigation receipt.
pub const THREAT_1_CLAIM_MAP_SCHEMA: &[u8] = b"bitblock/threat-1/property-claim-map/v1";

/// Exact number of attacker-power classes enumerated by `THREAT-D0` and quantified by `THREAT-1`.
pub const THREAT_1_CLASS_COUNT: usize = 11;
/// Maximum allowed Byzantine stake weight in basis points (`3333` bps = `33.33% < 1/3`).
pub const MAX_BFT_CORRUPT_WEIGHT_BPS: u16 = 3_333;

/// Errors returned by `THREAT-1` validation and canonical codec routines.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Threat1Error {
    /// Quantitative model parameter violated BFT, retention, quorum, or scope invariants.
    InvalidModel(&'static str),
    /// Property claim map was incomplete, out of class order, or carried an unquantified slot.
    InvalidClaimMap(&'static str),
    /// Canonical wire buffer had wrong length or non-canonical boolean/field bytes.
    InvalidWire(&'static str),
}

impl fmt::Display for Threat1Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}

impl std::error::Error for Threat1Error {}

/// Quantitative v1 adversary model (`THREAT-1`) bound to a `PROFILE-SCOPE-D0` digest.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ThreatModelV1 {
    /// Schema version (`1`).
    pub schema_version: u16,
    /// Pre-ID `ProfileScopeV1` digest governed by this quantitative adversary model.
    pub profile_scope_digest: [u8; 32],
    /// Maximum Byzantine stake weight in basis points (`1..=3333`, strictly `< 1/3`).
    pub max_corrupt_weight_bps: u16,
    /// Target block interval in milliseconds (`2_000` for conservative consumer-node v1).
    pub target_block_time_ms: u32,
    /// Partial-synchrony Global Stabilization Time (GST) message-delay bound in milliseconds.
    pub gst_max_delay_ms: u32,
    /// Minimum adaptive key-corruption delay in epochs (`>= 1`).
    pub adaptive_corruption_delay_epochs: u64,
    /// Validator stake unbonding delay in epochs (`21` matching `VALIDATOR-ECON-D1`).
    pub unbonding_delay_epochs: u64,
    /// Rolling full-node block/evidence retention horizon in epochs (`>= unbonding_delay_epochs`).
    pub rolling_retention_epochs: u64,
    /// Minimum honest peer connections required to resist eclipse isolation (`>= 4`).
    pub min_honest_peers: u16,
    /// Independent reproducible release-builder threshold `m` (`m > n / 2` and `m <= n`).
    pub release_builder_threshold_m: u8,
    /// Independent reproducible release-builder total committee `n` (`>= 3`).
    pub release_builder_total_n: u8,
    /// Must be `false`: `SEQUENCER-1` carries an explicit zero-censorship-guarantee non-claim.
    pub sequencer_censorship_guaranteed: bool,
    /// `CONSENSUS-1` bounded inclusion factor (`<= factor * active_validators` blocks under GST).
    pub consensus_max_inclusion_window_factor: u16,
    /// Classical cryptographic security level in bits (`128` for RFC 8032 Ed25519 + SHA-256).
    pub suite_classical_security_bits: u16,
    /// Must be `false`: external bridges and oracles remain excluded in v1.
    pub external_bridges_admitted: bool,
}

impl ThreatModelV1 {
    /// Fixed canonical big-endian wire length (`78` bytes).
    pub const WIRE_LEN: usize = 78;

    /// Constructs the approved v1 conservative consumer-node BFT adversary model for `scope`.
    pub fn v1_conservative_consumer(scope: &ProfileScopeV1) -> Result<Self, Threat1Error> {
        let scope_digest = scope
            .tag()
            .map_err(|_| Threat1Error::InvalidModel("invalid ProfileScopeV1"))?
            .digest();
        let econ = ValidatorEconPolicyV1::v1_pos_default();
        let cons = ConsensusPolicyDescriptorV1::v1_bft_pos_default();
        let model = Self {
            schema_version: 1,
            profile_scope_digest: scope_digest,
            max_corrupt_weight_bps: MAX_BFT_CORRUPT_WEIGHT_BPS,
            target_block_time_ms: 2_000,
            gst_max_delay_ms: 10_000,
            adaptive_corruption_delay_epochs: 1,
            unbonding_delay_epochs: econ.unbonding_delay_epochs,
            rolling_retention_epochs: cons.rolling_retention_epochs,
            min_honest_peers: 8,
            release_builder_threshold_m: 3,
            release_builder_total_n: 5,
            sequencer_censorship_guaranteed: false,
            consensus_max_inclusion_window_factor: 3,
            suite_classical_security_bits: 128,
            external_bridges_admitted: cons.external_bridges_admitted,
        };
        model.validate()?;
        Ok(model)
    }

    /// Validates all quantitative thresholds, BFT safety margins, and v1 exclusions fail-closed.
    pub fn validate(&self) -> Result<(), Threat1Error> {
        if self.schema_version != 1 {
            return Err(Threat1Error::InvalidModel("schema_version must be 1"));
        }
        if self.profile_scope_digest == [0u8; 32] {
            return Err(Threat1Error::InvalidModel(
                "profile_scope_digest must be non-zero",
            ));
        }
        if self.max_corrupt_weight_bps == 0
            || self.max_corrupt_weight_bps > MAX_BFT_CORRUPT_WEIGHT_BPS
        {
            return Err(Threat1Error::InvalidModel(
                "max_corrupt_weight_bps must lie in 1..=3333 (< 1/3)",
            ));
        }
        if self.target_block_time_ms == 0 || self.gst_max_delay_ms < self.target_block_time_ms {
            return Err(Threat1Error::InvalidModel(
                "gst_max_delay_ms must be >= target_block_time_ms > 0",
            ));
        }
        if self.adaptive_corruption_delay_epochs == 0 {
            return Err(Threat1Error::InvalidModel(
                "adaptive_corruption_delay_epochs must be >= 1",
            ));
        }
        if self.unbonding_delay_epochs == 0
            || self.rolling_retention_epochs < self.unbonding_delay_epochs
        {
            return Err(Threat1Error::InvalidModel(
                "rolling_retention_epochs must be >= unbonding_delay_epochs > 0",
            ));
        }
        if self.min_honest_peers < 4 {
            return Err(Threat1Error::InvalidModel("min_honest_peers must be >= 4"));
        }
        if self.release_builder_total_n < 3
            || self.release_builder_threshold_m > self.release_builder_total_n
            || u16::from(self.release_builder_threshold_m) * 2
                <= u16::from(self.release_builder_total_n)
        {
            return Err(Threat1Error::InvalidModel(
                "release_builder quorum must satisfy m > n/2 and n >= 3",
            ));
        }
        if self.sequencer_censorship_guaranteed {
            return Err(Threat1Error::InvalidModel(
                "SEQUENCER-1 must carry explicit zero-censorship-guarantee non-claim",
            ));
        }
        if self.consensus_max_inclusion_window_factor == 0 {
            return Err(Threat1Error::InvalidModel(
                "consensus_max_inclusion_window_factor must be >= 1",
            ));
        }
        if self.suite_classical_security_bits < 128 {
            return Err(Threat1Error::InvalidModel(
                "suite_classical_security_bits must be >= 128",
            ));
        }
        if self.external_bridges_admitted {
            return Err(Threat1Error::InvalidModel(
                "external_bridges_admitted must be false in v1",
            ));
        }
        Ok(())
    }

    /// Computes the maximum `CONSENSUS-1` inclusion window in blocks for `active_validators`.
    pub fn consensus_inclusion_window_blocks(&self, active_validators: u16) -> Option<u32> {
        if active_validators == 0 {
            return None;
        }
        u32::from(self.consensus_max_inclusion_window_factor)
            .checked_mul(u32::from(active_validators))
    }

    /// Encodes the threat model into 78 canonical big-endian wire bytes.
    pub fn encode_canonical(&self) -> Result<[u8; Self::WIRE_LEN], Threat1Error> {
        self.validate()?;
        let mut out = [0u8; Self::WIRE_LEN];
        out[0..2].copy_from_slice(&self.schema_version.to_be_bytes());
        out[2..34].copy_from_slice(&self.profile_scope_digest);
        out[34..36].copy_from_slice(&self.max_corrupt_weight_bps.to_be_bytes());
        out[36..40].copy_from_slice(&self.target_block_time_ms.to_be_bytes());
        out[40..44].copy_from_slice(&self.gst_max_delay_ms.to_be_bytes());
        out[44..52].copy_from_slice(&self.adaptive_corruption_delay_epochs.to_be_bytes());
        out[52..60].copy_from_slice(&self.unbonding_delay_epochs.to_be_bytes());
        out[60..68].copy_from_slice(&self.rolling_retention_epochs.to_be_bytes());
        out[68..70].copy_from_slice(&self.min_honest_peers.to_be_bytes());
        out[70] = self.release_builder_threshold_m;
        out[71] = self.release_builder_total_n;
        out[72] = u8::from(self.sequencer_censorship_guaranteed);
        out[73..75].copy_from_slice(&self.consensus_max_inclusion_window_factor.to_be_bytes());
        out[75..77].copy_from_slice(&self.suite_classical_security_bits.to_be_bytes());
        out[77] = u8::from(self.external_bridges_admitted);
        Ok(out)
    }

    /// Decodes and validates a 78-byte canonical `ThreatModelV1`.
    pub fn decode_canonical(bytes: &[u8]) -> Result<Self, Threat1Error> {
        if bytes.len() != Self::WIRE_LEN {
            return Err(Threat1Error::InvalidWire("wrong ThreatModelV1 wire length"));
        }
        let bool_at = |idx: usize| -> Result<bool, Threat1Error> {
            match bytes[idx] {
                0 => Ok(false),
                1 => Ok(true),
                _ => Err(Threat1Error::InvalidWire("non-canonical boolean byte")),
            }
        };
        let mut scope = [0u8; 32];
        scope.copy_from_slice(&bytes[2..34]);
        let model = Self {
            schema_version: u16::from_be_bytes([bytes[0], bytes[1]]),
            profile_scope_digest: scope,
            max_corrupt_weight_bps: u16::from_be_bytes([bytes[34], bytes[35]]),
            target_block_time_ms: u32::from_be_bytes(bytes[36..40].try_into().expect("4")),
            gst_max_delay_ms: u32::from_be_bytes(bytes[40..44].try_into().expect("4")),
            adaptive_corruption_delay_epochs: u64::from_be_bytes(
                bytes[44..52].try_into().expect("8"),
            ),
            unbonding_delay_epochs: u64::from_be_bytes(bytes[52..60].try_into().expect("8")),
            rolling_retention_epochs: u64::from_be_bytes(bytes[60..68].try_into().expect("8")),
            min_honest_peers: u16::from_be_bytes([bytes[68], bytes[69]]),
            release_builder_threshold_m: bytes[70],
            release_builder_total_n: bytes[71],
            sequencer_censorship_guaranteed: bool_at(72)?,
            consensus_max_inclusion_window_factor: u16::from_be_bytes([bytes[73], bytes[74]]),
            suite_classical_security_bits: u16::from_be_bytes([bytes[75], bytes[76]]),
            external_bridges_admitted: bool_at(77)?,
        };
        model.validate()?;
        Ok(model)
    }

    /// Computes the canonical `Policy` `ContentTag` for this adversary model.
    pub fn tag(&self) -> Result<ContentTag, Threat1Error> {
        let bytes = self.encode_canonical()?;
        Ok(hash_canonical(
            TagKind::Policy,
            THREAT_1_MODEL_SCHEMA,
            bytes.len() as u64,
            |w| w(&bytes),
        )
        .expect("static schema and fixed length"))
    }
}

/// One quantified row in [`ThreatClaimMapV1`] mapping a `THREAT-D0` attacker-power class (`1..=11`)
/// to its primary quantitative threshold, loss condition, detection/recovery code, and exclusion bit.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ThreatPropertyEntryV1 {
    /// Attacker-power class ordinal (`1..=11` matching `docs/THREAT_D0_ENVELOPE.md`).
    pub class_id: u8,
    /// Primary quantified numeric threshold for this class (`> 0`).
    pub primary_threshold: u64,
    /// Canonical loss-condition identifier (`1..=11`).
    pub loss_condition_code: u16,
    /// Canonical detection / safe-halt / mitigation gate identifier (`1..=11`).
    pub mitigation_code: u16,
    /// `true` when portions of this class remain explicitly excluded in v1 (e.g., physical power
    /// loss, external bridges/oracles, or `SEQUENCER-1` censorship resistance).
    pub has_explicit_v1_exclusion: bool,
}

impl ThreatPropertyEntryV1 {
    /// Fixed canonical wire byte length per entry (`14` bytes).
    pub const WIRE_LEN: usize = 14;
}

/// Complete 11-class quantified property, loss-condition, and mitigation receipt (`THREAT-1`).
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct ThreatClaimMapV1 {
    /// Schema version (`1`).
    pub schema_version: u16,
    /// Digest of the owning [`ThreatModelV1`].
    pub threat_model_digest: [u8; 32],
    /// Exact ordered 11-class table (`class_id = 1..=11`).
    pub entries: [ThreatPropertyEntryV1; THREAT_1_CLASS_COUNT],
}

impl ThreatClaimMapV1 {
    /// Fixed canonical big-endian wire length (`188` bytes).
    pub const WIRE_LEN: usize = 2 + 32 + THREAT_1_CLASS_COUNT * ThreatPropertyEntryV1::WIRE_LEN;

    /// Builds the canonical 11-class quantified claim map derived directly from `model`.
    pub fn from_model(model: &ThreatModelV1) -> Result<Self, Threat1Error> {
        let model_digest = model.tag()?.digest();
        let entries = [
            // 1. Static & adaptive validator corruption
            ThreatPropertyEntryV1 {
                class_id: 1,
                primary_threshold: u64::from(model.max_corrupt_weight_bps),
                loss_condition_code: 101,
                mitigation_code: 201,
                has_explicit_v1_exclusion: false,
            },
            // 2. Network scheduling, partition & eclipse
            ThreatPropertyEntryV1 {
                class_id: 2,
                primary_threshold: u64::from(model.gst_max_delay_ms),
                loss_condition_code: 102,
                mitigation_code: 202,
                has_explicit_v1_exclusion: false,
            },
            // 3. Key compromise (actor, validator, release, governance)
            ThreatPropertyEntryV1 {
                class_id: 3,
                primary_threshold: model.adaptive_corruption_delay_epochs,
                loss_condition_code: 103,
                mitigation_code: 203,
                has_explicit_v1_exclusion: false,
            },
            // 4. Cryptographic primitive / hash break
            ThreatPropertyEntryV1 {
                class_id: 4,
                primary_threshold: u64::from(model.suite_classical_security_bits),
                loss_condition_code: 104,
                mitigation_code: 204,
                has_explicit_v1_exclusion: false,
            },
            // 5. Parser, codec & proof-verification DoS (2 MiB block cap -> BOUNDS-1)
            ThreatPropertyEntryV1 {
                class_id: 5,
                primary_threshold: 2_097_152,
                loss_condition_code: 105,
                mitigation_code: 205,
                has_explicit_v1_exclusion: false,
            },
            // 6. State-growth & history spam (4,096 writes/block ceiling -> BOUNDS-1)
            ThreatPropertyEntryV1 {
                class_id: 6,
                primary_threshold: 4_096,
                loss_condition_code: 106,
                mitigation_code: 206,
                has_explicit_v1_exclusion: false,
            },
            // 7. Censorship & ordering manipulation (SEQUENCER-1 excluded; CONSENSUS-1 <= 3*N)
            ThreatPropertyEntryV1 {
                class_id: 7,
                primary_threshold: u64::from(model.consensus_max_inclusion_window_factor),
                loss_condition_code: 107,
                mitigation_code: 207,
                has_explicit_v1_exclusion: true,
            },
            // 8. Data availability & archival withholding (rolling_retention_epochs)
            ThreatPropertyEntryV1 {
                class_id: 8,
                primary_threshold: model.rolling_retention_epochs,
                loss_condition_code: 108,
                mitigation_code: 208,
                has_explicit_v1_exclusion: false,
            },
            // 9. Supply-chain, compiler & build substitution (3-of-5 builders)
            ThreatPropertyEntryV1 {
                class_id: 9,
                primary_threshold: u64::from(model.release_builder_threshold_m),
                loss_condition_code: 109,
                mitigation_code: 209,
                has_explicit_v1_exclusion: false,
            },
            // 10. Bootstrap, genesis & upgrade spoofing (min_honest_peers + unbonding timelock)
            ThreatPropertyEntryV1 {
                class_id: 10,
                primary_threshold: u64::from(model.min_honest_peers),
                loss_condition_code: 110,
                mitigation_code: 210,
                has_explicit_v1_exclusion: false,
            },
            // 11. Host, local storage, display & bridge/oracle boundaries (explicit v1 exclusions)
            ThreatPropertyEntryV1 {
                class_id: 11,
                primary_threshold: model.unbonding_delay_epochs,
                loss_condition_code: 111,
                mitigation_code: 211,
                has_explicit_v1_exclusion: true,
            },
        ];
        let map = Self {
            schema_version: 1,
            threat_model_digest: model_digest,
            entries,
        };
        map.validate()?;
        Ok(map)
    }

    /// Validates canonical class ordering (`1..=11`), non-zero quantified thresholds, and exclusions.
    pub fn validate(&self) -> Result<(), Threat1Error> {
        if self.schema_version != 1 {
            return Err(Threat1Error::InvalidClaimMap("schema_version must be 1"));
        }
        if self.threat_model_digest == [0u8; 32] {
            return Err(Threat1Error::InvalidClaimMap(
                "threat_model_digest must be non-zero",
            ));
        }
        for (idx, entry) in self.entries.iter().enumerate() {
            if entry.class_id != (idx as u8) + 1 {
                return Err(Threat1Error::InvalidClaimMap(
                    "entries must appear in exact class_id order 1..=11",
                ));
            }
            if entry.primary_threshold == 0
                || entry.loss_condition_code == 0
                || entry.mitigation_code == 0
            {
                return Err(Threat1Error::InvalidClaimMap(
                    "every class must have non-zero threshold, loss code, and mitigation code",
                ));
            }
        }
        if !self.entries[6].has_explicit_v1_exclusion || !self.entries[10].has_explicit_v1_exclusion
        {
            return Err(Threat1Error::InvalidClaimMap(
                "classes 7 (SEQUENCER-1) and 11 (hardware/display/bridge) must record explicit v1 exclusions",
            ));
        }
        Ok(())
    }

    /// Encodes the claim map into 188 canonical big-endian wire bytes.
    pub fn encode_canonical(&self) -> Result<[u8; Self::WIRE_LEN], Threat1Error> {
        self.validate()?;
        let mut out = [0u8; Self::WIRE_LEN];
        out[0..2].copy_from_slice(&self.schema_version.to_be_bytes());
        out[2..34].copy_from_slice(&self.threat_model_digest);
        let mut cursor = 34;
        for entry in &self.entries {
            out[cursor] = entry.class_id;
            out[cursor + 1..cursor + 9].copy_from_slice(&entry.primary_threshold.to_be_bytes());
            out[cursor + 9..cursor + 11].copy_from_slice(&entry.loss_condition_code.to_be_bytes());
            out[cursor + 11..cursor + 13].copy_from_slice(&entry.mitigation_code.to_be_bytes());
            out[cursor + 13] = u8::from(entry.has_explicit_v1_exclusion);
            cursor += ThreatPropertyEntryV1::WIRE_LEN;
        }
        Ok(out)
    }

    /// Decodes and validates a 188-byte canonical `ThreatClaimMapV1`.
    pub fn decode_canonical(bytes: &[u8]) -> Result<Self, Threat1Error> {
        if bytes.len() != Self::WIRE_LEN {
            return Err(Threat1Error::InvalidWire(
                "wrong ThreatClaimMapV1 wire length",
            ));
        }
        let mut digest = [0u8; 32];
        digest.copy_from_slice(&bytes[2..34]);
        let mut entries = [ThreatPropertyEntryV1 {
            class_id: 1,
            primary_threshold: 1,
            loss_condition_code: 1,
            mitigation_code: 1,
            has_explicit_v1_exclusion: false,
        }; THREAT_1_CLASS_COUNT];
        let mut cursor = 34;
        for slot in &mut entries {
            let excl = match bytes[cursor + 13] {
                0 => false,
                1 => true,
                _ => return Err(Threat1Error::InvalidWire("non-canonical exclusion byte")),
            };
            *slot = ThreatPropertyEntryV1 {
                class_id: bytes[cursor],
                primary_threshold: u64::from_be_bytes(
                    bytes[cursor + 1..cursor + 9].try_into().expect("8"),
                ),
                loss_condition_code: u16::from_be_bytes([bytes[cursor + 9], bytes[cursor + 10]]),
                mitigation_code: u16::from_be_bytes([bytes[cursor + 11], bytes[cursor + 12]]),
                has_explicit_v1_exclusion: excl,
            };
            cursor += ThreatPropertyEntryV1::WIRE_LEN;
        }
        let map = Self {
            schema_version: u16::from_be_bytes([bytes[0], bytes[1]]),
            threat_model_digest: digest,
            entries,
        };
        map.validate()?;
        Ok(map)
    }

    /// Computes the canonical `Receipt` `ContentTag` for this 11-class claim map.
    pub fn tag(&self) -> Result<ContentTag, Threat1Error> {
        let bytes = self.encode_canonical()?;
        Ok(hash_canonical(
            TagKind::Receipt,
            THREAT_1_CLAIM_MAP_SCHEMA,
            bytes.len() as u64,
            |w| w(&bytes),
        )
        .expect("static schema and fixed length"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::profile_bootstrap::ProposedLimitVectorV1;

    #[test]
    fn threat_1_model_and_claim_map_round_trip_and_reject_unsafe_assumptions() {
        let scope = ProfileScopeV1 {
            schema_version: 1,
            base_transition_spec_digest: [1u8; 32],
            domain_registry_digest: [2u8; 32],
            crypto_suite_digest: [3u8; 32],
            asset_policy_digest: [4u8; 32],
            validator_econ_digest: [5u8; 32],
            consensus_policy_digest: [6u8; 32],
            governance_policy_digest: [7u8; 32],
            proposed_limits: ProposedLimitVectorV1::v1_default(),
            activated_predicates: Vec::new(),
        };
        let model = ThreatModelV1::v1_conservative_consumer(&scope).unwrap();
        let raw_m = model.encode_canonical().unwrap();
        assert_eq!(ThreatModelV1::decode_canonical(&raw_m).unwrap(), model);
        assert_eq!(model.consensus_inclusion_window_blocks(64), Some(192));

        let map = ThreatClaimMapV1::from_model(&model).unwrap();
        let raw_c = map.encode_canonical().unwrap();
        assert_eq!(ThreatClaimMapV1::decode_canonical(&raw_c).unwrap(), map);

        // Negative controls: >= 1/3 corrupt weight, sequencer censorship claim, bridges enabled.
        let mut bad = model;
        bad.max_corrupt_weight_bps = 3_334;
        assert!(bad.validate().is_err());
        bad = model;
        bad.sequencer_censorship_guaranteed = true;
        assert!(bad.validate().is_err());
        bad = model;
        bad.external_bridges_admitted = true;
        assert!(bad.validate().is_err());
    }
}
