//! `RESOURCE-1`: deterministic EIP-1559 fee burn, bounded proposer tip, and refundable voxel
//! storage deposit policy calibrated to `BOUNDS-1` and `ASSET-D1`.
//!
//! Resolves `PS-039:RESOURCE-1@base` (and conditional edges `CE-08` / `CE-14`) by pairing:
//! - **`BOUNDS-1`-calibrated integer gas weights:** explicitly pricing Ed25519 signature checks
//!   (`95%` of worst-case measured block CPU), Patricia state writes (`4,096`/block disk-growth
//!   driver), payload/proof bytes, and occupancy operations under a unified block gas ceiling
//!   (`target = 50_000_000`, `max = 100_000_000`).
//! - **Unified integer EIP-1559 dynamic base-fee controller:** adjusts `base_fee_per_gas` by up to
//!   `±12.5%` per block (`denominator = 8`) around `50%` target utilization using pure checked
//!   `u128` arithmetic (zero floats), strictly clamped to `[min_base_fee_per_gas, max_base_fee_per_gas]`.
//! - **100% base-fee burn + bounded proposer tip:** base fees are permanently burned even on
//!   admitted transaction failures (`charge_admitted_failure = true`), while optional priority tips
//!   are strictly capped by `max_priority_tip_per_gas` and credited to the block proposer.
//! - **Refundable per-voxel deposits (no forced rent):** mutable voxel mints lock exact
//!   `deposit_per_occupied_block`, burns refund the locked deposit to the episode owner, and
//!   bedrock `(0, 0, 0)` never holds a deposit (`state_rent_enabled = false`).
//!
//! Resource pricing does not guarantee liveness, fair inclusion, price stability, or market value.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use crate::asset_d1::AssetPolicyDescriptorV1;
use crate::bounds_1::ProtocolBoundsProfileV1;
use crate::identity_d1::ActorIdV1;
use crate::protocol::{ContentTag, TagKind, hash_canonical};
use crate::world::{BEDROCK_COORD, Coord};

/// Canonical schema string for a [`ResourcePolicyV1`] policy object (`RESOURCE-1`).
pub const RESOURCE_1_POLICY_SCHEMA: &[u8] = b"bitblock/resource-1/fee-policy/v1";
/// Canonical schema string for a [`BlockResourceSettlementV1`] settlement receipt (`RESOURCE-1`).
pub const RESOURCE_1_SETTLEMENT_SCHEMA: &[u8] = b"bitblock/resource-1/block-settlement/v1";

/// Default base gas per admitted transaction.
pub const DEFAULT_GAS_PER_TX_BASE: u32 = 21_000;
/// Default gas per encoded transaction payload byte.
pub const DEFAULT_GAS_PER_TX_BYTE: u32 = 16;
/// Default gas per strict RFC 8032 Ed25519 signature verification (`BOUNDS-1` dominant CPU cost).
pub const DEFAULT_GAS_PER_ED25519_SIG: u32 = 35_000;
/// Default gas per committed Patricia state write (`BOUNDS-1` storage-growth driver).
pub const DEFAULT_GAS_PER_STATE_WRITE: u32 = 12_000;
/// Default gas per encoded authenticated proof byte.
pub const DEFAULT_GAS_PER_PROOF_BYTE: u32 = 32;
/// Default gas per occupancy edit operation.
pub const DEFAULT_GAS_PER_OCCUPANCY_OP: u32 = 2_000;
/// Default EIP-1559 target gas per block (`50%` utilization).
pub const DEFAULT_TARGET_BLOCK_GAS: u64 = 50_000_000;
/// Default EIP-1559 hard ceiling gas per block (`2x` target).
pub const DEFAULT_MAX_BLOCK_GAS: u64 = 100_000_000;
/// Default EIP-1559 integer change denominator (`8` => `±12.5%` max adjustment per block).
pub const DEFAULT_BASE_FEE_CHANGE_DENOMINATOR: u16 = 8;
/// Default minimum burned base fee per gas unit.
pub const DEFAULT_MIN_BASE_FEE_PER_GAS: u128 = 1;
/// Default maximum burned base fee per gas unit.
pub const DEFAULT_MAX_BASE_FEE_PER_GAS: u128 = 1_000_000_000;
/// Default hard cap on proposer priority tip per gas unit.
pub const DEFAULT_MAX_PRIORITY_TIP_PER_GAS: u128 = 100_000_000;

/// Errors returned by `RESOURCE-1` policy validation, metering, and block settlement.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Resource1Error {
    /// Policy parameter violated positivity, bounds, or `ASSET-D1` / `BOUNDS-1` invariants.
    InvalidPolicy(&'static str),
    /// Canonical wire buffer had wrong length, reserved flag bits, or trailing bytes.
    InvalidWire(&'static str),
    /// Checked integer arithmetic overflowed during metering or settlement.
    ArithmeticOverflow(&'static str),
    /// Block exceeded `max_block_gas` or a transaction exceeded `BOUNDS-1` single-tx bounds.
    BlockGasLimitExceeded {
        /// Cumulative gas requested.
        requested: u64,
        /// Hard policy block gas ceiling.
        limit: u64,
    },
    /// Transaction `max_fee_per_gas` was strictly below the active block `base_fee_per_gas`.
    MaxFeeBelowBaseFee {
        /// Transaction maximum fee per gas.
        max_fee_per_gas: u128,
        /// Active block base fee per gas.
        base_fee_per_gas: u128,
    },
    /// Payer lacked sufficient liquid native units to cover fees, tips, or refundable deposits.
    InsufficientLiquidBalance {
        /// Total native units required.
        required: u128,
        /// Available liquid native units.
        available: u128,
    },
    /// Attempted to attach or refund a storage deposit on immutable bedrock `(0, 0, 0)`.
    BedrockDepositForbidden,
    /// Coordinate already held a locked deposit, or burn lacked a matching owner deposit.
    DepositStateViolation(&'static str),
    /// Closed supply conservation invariant was violated.
    ConservationInvariantBroken,
}

impl fmt::Display for Resource1Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}

impl std::error::Error for Resource1Error {}

/// Metered resource work vector for one admitted transaction (`RESOURCE-1`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct TxResourceWorkV1 {
    /// Encoded transaction payload bytes.
    pub tx_bytes: u32,
    /// Number of strict Ed25519 signature verifications required.
    pub ed25519_signatures: u32,
    /// Number of committed state writes.
    pub state_writes: u32,
    /// Encoded authenticated proof bytes carried or verified.
    pub proof_bytes: u32,
    /// Number of voxel occupancy operations.
    pub occupancy_ops: u32,
}

/// Canonical `RESOURCE-1` fee, EIP-1559 congestion, and refundable-deposit policy.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ResourcePolicyV1 {
    /// Schema version (`1`).
    pub schema_version: u16,
    /// Digest of the governing `ProtocolBoundsProfileV1` (`BOUNDS-1`).
    pub bounds_profile_digest: [u8; 32],
    /// Digest of the governing `AssetPolicyDescriptorV1` (`ASSET-D1`).
    pub asset_policy_digest: [u8; 32],
    /// Base gas per admitted transaction (`> 0`).
    pub gas_per_tx_base: u32,
    /// Gas per transaction payload byte (`> 0`).
    pub gas_per_tx_byte: u32,
    /// Gas per strict RFC 8032 Ed25519 verification (`> 0`).
    pub gas_per_ed25519_sig: u32,
    /// Gas per Patricia state write (`> 0`).
    pub gas_per_state_write: u32,
    /// Gas per authenticated proof byte (`> 0`).
    pub gas_per_proof_byte: u32,
    /// Gas per voxel occupancy edit operation (`> 0`).
    pub gas_per_occupancy_op: u32,
    /// EIP-1559 target block gas (`> 0`).
    pub target_block_gas: u64,
    /// Hard maximum block gas (`> target_block_gas`).
    pub max_block_gas: u64,
    /// Integer base-fee change denominator (`>= 2`; `8` = `±12.5%` max step).
    pub base_fee_change_denominator: u16,
    /// Lower clamp for dynamic burned base fee per gas (`>= 1`).
    pub min_base_fee_per_gas: u128,
    /// Upper clamp for dynamic burned base fee per gas (`>= min_base_fee_per_gas`).
    pub max_base_fee_per_gas: u128,
    /// Hard ceiling on optional proposer priority tip per gas (`> 0`, `<= max_base_fee_per_gas`).
    pub max_priority_tip_per_gas: u128,
    /// Refundable native storage deposit locked per occupied mutable voxel (`> 0`).
    pub deposit_per_occupied_block: u128,
    /// Must be `true`: 100% of dynamic base fee is permanently burned.
    pub burn_base_fee: bool,
    /// Must be `true`: admitted failed transactions still pay burned base fee and capped tip.
    pub charge_admitted_failure: bool,
    /// Must be `false`: no forced state-rent expiry under refundable deposit policy.
    pub state_rent_enabled: bool,
}

impl ResourcePolicyV1 {
    /// Fixed canonical big-endian wire length (`173` bytes).
    pub const WIRE_LEN: usize = 173;

    /// Builds the default `RESOURCE-1` v1 policy bound to `bounds` and `asset`.
    pub fn v1_default(
        bounds: &ProtocolBoundsProfileV1,
        asset: &AssetPolicyDescriptorV1,
    ) -> Result<Self, Resource1Error> {
        let bounds_digest = bounds
            .tag()
            .map_err(|_| Resource1Error::InvalidPolicy("invalid ProtocolBoundsProfileV1"))?
            .digest();
        let asset_digest = asset
            .tag()
            .map_err(|_| Resource1Error::InvalidPolicy("invalid AssetPolicyDescriptorV1"))?
            .digest();
        let policy = Self {
            schema_version: 1,
            bounds_profile_digest: bounds_digest,
            asset_policy_digest: asset_digest,
            gas_per_tx_base: DEFAULT_GAS_PER_TX_BASE,
            gas_per_tx_byte: DEFAULT_GAS_PER_TX_BYTE,
            gas_per_ed25519_sig: DEFAULT_GAS_PER_ED25519_SIG,
            gas_per_state_write: DEFAULT_GAS_PER_STATE_WRITE,
            gas_per_proof_byte: DEFAULT_GAS_PER_PROOF_BYTE,
            gas_per_occupancy_op: DEFAULT_GAS_PER_OCCUPANCY_OP,
            target_block_gas: DEFAULT_TARGET_BLOCK_GAS,
            max_block_gas: DEFAULT_MAX_BLOCK_GAS,
            base_fee_change_denominator: DEFAULT_BASE_FEE_CHANGE_DENOMINATOR,
            min_base_fee_per_gas: DEFAULT_MIN_BASE_FEE_PER_GAS,
            max_base_fee_per_gas: DEFAULT_MAX_BASE_FEE_PER_GAS,
            max_priority_tip_per_gas: DEFAULT_MAX_PRIORITY_TIP_PER_GAS,
            deposit_per_occupied_block: asset.deposit_per_occupied_block,
            burn_base_fee: true,
            charge_admitted_failure: true,
            state_rent_enabled: false,
        };
        policy.validate(Some(bounds), Some(asset))?;
        Ok(policy)
    }

    /// Validates all internal policy invariants and optional `BOUNDS-1` / `ASSET-D1` bindings.
    pub fn validate(
        &self,
        bounds: Option<&ProtocolBoundsProfileV1>,
        asset: Option<&AssetPolicyDescriptorV1>,
    ) -> Result<(), Resource1Error> {
        if self.schema_version != 1 {
            return Err(Resource1Error::InvalidPolicy("schema_version must be 1"));
        }
        if self.bounds_profile_digest == [0u8; 32] || self.asset_policy_digest == [0u8; 32] {
            return Err(Resource1Error::InvalidPolicy(
                "bounds_profile_digest and asset_policy_digest must be non-zero",
            ));
        }
        let weights = [
            self.gas_per_tx_base,
            self.gas_per_tx_byte,
            self.gas_per_ed25519_sig,
            self.gas_per_state_write,
            self.gas_per_proof_byte,
            self.gas_per_occupancy_op,
        ];
        if weights.contains(&0) {
            return Err(Resource1Error::InvalidPolicy(
                "all resource gas weights must be > 0",
            ));
        }
        if self.target_block_gas == 0 || self.max_block_gas <= self.target_block_gas {
            return Err(Resource1Error::InvalidPolicy(
                "must satisfy 0 < target_block_gas < max_block_gas",
            ));
        }
        if self.base_fee_change_denominator < 2 {
            return Err(Resource1Error::InvalidPolicy(
                "base_fee_change_denominator must be >= 2",
            ));
        }
        if self.min_base_fee_per_gas == 0 || self.max_base_fee_per_gas < self.min_base_fee_per_gas {
            return Err(Resource1Error::InvalidPolicy(
                "must satisfy 0 < min_base_fee_per_gas <= max_base_fee_per_gas",
            ));
        }
        if self.max_priority_tip_per_gas == 0
            || self.max_priority_tip_per_gas > self.max_base_fee_per_gas
        {
            return Err(Resource1Error::InvalidPolicy(
                "max_priority_tip_per_gas must lie in 1..=max_base_fee_per_gas",
            ));
        }
        if self.deposit_per_occupied_block == 0 {
            return Err(Resource1Error::InvalidPolicy(
                "deposit_per_occupied_block must be > 0",
            ));
        }
        if !self.burn_base_fee || !self.charge_admitted_failure || self.state_rent_enabled {
            return Err(Resource1Error::InvalidPolicy(
                "burn_base_fee and charge_admitted_failure must be true; state_rent_enabled false",
            ));
        }
        if let Some(b) = bounds {
            let expected = b
                .tag()
                .map_err(|_| Resource1Error::InvalidPolicy("invalid bounds profile"))?
                .digest();
            if self.bounds_profile_digest != expected {
                return Err(Resource1Error::InvalidPolicy(
                    "bounds_profile_digest mismatch",
                ));
            }
            let min_tx_gas = u64::from(self.gas_per_tx_base);
            if self.max_block_gas < min_tx_gas {
                return Err(Resource1Error::InvalidPolicy(
                    "max_block_gas below single transaction base gas",
                ));
            }
        }
        if let Some(a) = asset {
            let expected = a
                .tag()
                .map_err(|_| Resource1Error::InvalidPolicy("invalid asset policy"))?
                .digest();
            if self.asset_policy_digest != expected {
                return Err(Resource1Error::InvalidPolicy(
                    "asset_policy_digest mismatch",
                ));
            }
            if self.deposit_per_occupied_block != a.deposit_per_occupied_block {
                return Err(Resource1Error::InvalidPolicy(
                    "deposit_per_occupied_block must match ASSET-D1",
                ));
            }
            let min_tx_burn = self
                .min_base_fee_per_gas
                .checked_mul(u128::from(self.gas_per_tx_base))
                .ok_or(Resource1Error::ArithmeticOverflow("min_tx_burn overflow"))?;
            if min_tx_burn < a.base_fee_burn {
                return Err(Resource1Error::InvalidPolicy(
                    "minimum base tx fee burn falls below ASSET-D1 base_fee_burn",
                ));
            }
        }
        Ok(())
    }

    /// Computes deterministic integer gas required for one transaction's metered work vector.
    pub fn meter_work(&self, work: &TxResourceWorkV1) -> Result<u64, Resource1Error> {
        let mut total = u64::from(self.gas_per_tx_base);
        let terms = [
            (work.tx_bytes, self.gas_per_tx_byte),
            (work.ed25519_signatures, self.gas_per_ed25519_sig),
            (work.state_writes, self.gas_per_state_write),
            (work.proof_bytes, self.gas_per_proof_byte),
            (work.occupancy_ops, self.gas_per_occupancy_op),
        ];
        for (qty, weight) in terms {
            let prod = u64::from(qty)
                .checked_mul(u64::from(weight))
                .ok_or(Resource1Error::ArithmeticOverflow("work gas product"))?;
            total = total
                .checked_add(prod)
                .ok_or(Resource1Error::ArithmeticOverflow("work gas sum"))?;
        }
        Ok(total)
    }

    /// Pure integer EIP-1559 next-block base fee transition clamped to `[min, max]`.
    pub fn next_base_fee(
        &self,
        prev_base_fee: u128,
        block_gas_used: u64,
    ) -> Result<u128, Resource1Error> {
        self.validate(None, None)?;
        if block_gas_used > self.max_block_gas {
            return Err(Resource1Error::BlockGasLimitExceeded {
                requested: block_gas_used,
                limit: self.max_block_gas,
            });
        }
        let clamped_prev =
            prev_base_fee.clamp(self.min_base_fee_per_gas, self.max_base_fee_per_gas);
        let target = u128::from(self.target_block_gas);
        let denom = u128::from(self.base_fee_change_denominator);
        if block_gas_used == self.target_block_gas {
            return Ok(clamped_prev);
        }
        if block_gas_used > self.target_block_gas {
            let delta_gas = u128::from(block_gas_used - self.target_block_gas);
            let step = clamped_prev
                .checked_mul(delta_gas)
                .ok_or(Resource1Error::ArithmeticOverflow("eip1559 up mul"))?
                / target
                / denom;
            let inc = step.max(1);
            Ok(clamped_prev
                .saturating_add(inc)
                .min(self.max_base_fee_per_gas))
        } else {
            let delta_gas = u128::from(self.target_block_gas - block_gas_used);
            let step = clamped_prev
                .checked_mul(delta_gas)
                .ok_or(Resource1Error::ArithmeticOverflow("eip1559 down mul"))?
                / target
                / denom;
            Ok(clamped_prev
                .saturating_sub(step)
                .max(self.min_base_fee_per_gas))
        }
    }

    /// Encodes the resource policy into 173 canonical big-endian wire bytes.
    pub fn encode_canonical(&self) -> Result<[u8; Self::WIRE_LEN], Resource1Error> {
        self.validate(None, None)?;
        let mut out = [0u8; Self::WIRE_LEN];
        out[0..2].copy_from_slice(&self.schema_version.to_be_bytes());
        out[2..34].copy_from_slice(&self.bounds_profile_digest);
        out[34..66].copy_from_slice(&self.asset_policy_digest);
        out[66..70].copy_from_slice(&self.gas_per_tx_base.to_be_bytes());
        out[70..74].copy_from_slice(&self.gas_per_tx_byte.to_be_bytes());
        out[74..78].copy_from_slice(&self.gas_per_ed25519_sig.to_be_bytes());
        out[78..82].copy_from_slice(&self.gas_per_state_write.to_be_bytes());
        out[82..86].copy_from_slice(&self.gas_per_proof_byte.to_be_bytes());
        out[86..90].copy_from_slice(&self.gas_per_occupancy_op.to_be_bytes());
        out[90..98].copy_from_slice(&self.target_block_gas.to_be_bytes());
        out[98..106].copy_from_slice(&self.max_block_gas.to_be_bytes());
        out[106..108].copy_from_slice(&self.base_fee_change_denominator.to_be_bytes());
        out[108..124].copy_from_slice(&self.min_base_fee_per_gas.to_be_bytes());
        out[124..140].copy_from_slice(&self.max_base_fee_per_gas.to_be_bytes());
        out[140..156].copy_from_slice(&self.max_priority_tip_per_gas.to_be_bytes());
        out[156..172].copy_from_slice(&self.deposit_per_occupied_block.to_be_bytes());
        out[172] = u8::from(self.burn_base_fee)
            | (u8::from(self.charge_admitted_failure) << 1)
            | (u8::from(self.state_rent_enabled) << 2);
        Ok(out)
    }

    /// Decodes and validates a 173-byte canonical [`ResourcePolicyV1`].
    pub fn decode_canonical(bytes: &[u8]) -> Result<Self, Resource1Error> {
        if bytes.len() != Self::WIRE_LEN {
            return Err(Resource1Error::InvalidWire(
                "wrong ResourcePolicyV1 wire length",
            ));
        }
        let flags = bytes[172];
        if (flags & !0x07) != 0 {
            return Err(Resource1Error::InvalidWire(
                "reserved ResourcePolicyV1 flag bits set",
            ));
        }
        let r32 = |o: usize| -> [u8; 32] { bytes[o..o + 32].try_into().expect("32") };
        let ru32 = |o: usize| -> u32 { u32::from_be_bytes(bytes[o..o + 4].try_into().expect("4")) };
        let ru64 = |o: usize| -> u64 { u64::from_be_bytes(bytes[o..o + 8].try_into().expect("8")) };
        let ru128 =
            |o: usize| -> u128 { u128::from_be_bytes(bytes[o..o + 16].try_into().expect("16")) };
        let policy = Self {
            schema_version: u16::from_be_bytes([bytes[0], bytes[1]]),
            bounds_profile_digest: r32(2),
            asset_policy_digest: r32(34),
            gas_per_tx_base: ru32(66),
            gas_per_tx_byte: ru32(70),
            gas_per_ed25519_sig: ru32(74),
            gas_per_state_write: ru32(78),
            gas_per_proof_byte: ru32(82),
            gas_per_occupancy_op: ru32(86),
            target_block_gas: ru64(90),
            max_block_gas: ru64(98),
            base_fee_change_denominator: u16::from_be_bytes([bytes[106], bytes[107]]),
            min_base_fee_per_gas: ru128(108),
            max_base_fee_per_gas: ru128(124),
            max_priority_tip_per_gas: ru128(140),
            deposit_per_occupied_block: ru128(156),
            burn_base_fee: (flags & 1) != 0,
            charge_admitted_failure: (flags & 2) != 0,
            state_rent_enabled: (flags & 4) != 0,
        };
        policy.validate(None, None)?;
        Ok(policy)
    }

    /// Computes the canonical `Policy` `ContentTag` for this resource policy.
    pub fn tag(&self) -> Result<ContentTag, Resource1Error> {
        let bytes = self.encode_canonical()?;
        Ok(hash_canonical(
            TagKind::Policy,
            RESOURCE_1_POLICY_SCHEMA,
            bytes.len() as u64,
            |w| w(&bytes),
        )
        .expect("static schema and fixed length"))
    }
}

/// One admitted transaction submitted for deterministic resource charging & deposit settlement.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TxResourceRequestV1 {
    /// Payer actor identity.
    pub payer: ActorIdV1,
    /// Maximum total fee per gas unit authorized by the payer.
    pub max_fee_per_gas: u128,
    /// Maximum optional priority tip per gas unit authorized by the payer.
    pub max_priority_tip_per_gas: u128,
    /// Metered work profile of this transaction.
    pub work: TxResourceWorkV1,
    /// `true` if transaction execution succeeded; `false` if it was an admitted execution failure.
    pub execution_succeeded: bool,
    /// Mutable coordinates newly occupied (minted) on success.
    pub minted_coords: Vec<Coord>,
    /// Mutable coordinates vacated (burned) on success (must be owned by `payer`).
    pub burned_coords: Vec<Coord>,
}

/// Deterministic per-transaction resource charge entry inside [`BlockResourceSettlementV1`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct TxResourceChargeV1 {
    /// Payer actor identity.
    pub payer: ActorIdV1,
    /// `true` if execution succeeded; `false` if admitted failure (fee still burned).
    pub admitted_succeeded: bool,
    /// Exact integer gas metered and billed.
    pub gas_used: u64,
    /// Burned dynamic base fee (`base_fee_per_gas * gas_used`).
    pub burned_fee: u128,
    /// Capped effective priority tip credited to the block proposer.
    pub proposer_tip: u128,
    /// Refundable native deposit locked on minted mutable coordinates (`0` on admitted failure).
    pub deposit_locked: u128,
    /// Refundable native deposit returned on burned mutable coordinates (`0` on admitted failure).
    pub deposit_refunded: u128,
}

impl TxResourceChargeV1 {
    /// Fixed canonical wire length per charge entry (`105` bytes).
    pub const WIRE_LEN: usize = 32 + 1 + 8 + 16 * 4;
}

/// Canonical per-block resource settlement & EIP-1559 base-fee transition receipt.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct BlockResourceSettlementV1 {
    /// Digest of the governing [`ResourcePolicyV1`].
    pub policy_digest: [u8; 32],
    /// Monotonic block height (`>= 1`).
    pub block_height: u64,
    /// Block proposer receiving capped priority tips.
    pub proposer: ActorIdV1,
    /// Active burned base fee per gas during this block.
    pub base_fee_per_gas: u128,
    /// Deterministic EIP-1559 burned base fee per gas for `block_height + 1`.
    pub next_base_fee_per_gas: u128,
    /// Total metered gas consumed across all admitted transactions in the block.
    pub block_gas_used: u64,
    /// Total native units permanently burned in this block.
    pub total_burned_fees: u128,
    /// Total capped priority tips credited to `proposer` in this block.
    pub total_proposer_tips: u128,
    /// Signed net change in locked refundable deposits (`+locked - refunded`).
    pub net_locked_deposit_delta: i128,
    /// Ordered per-transaction charge receipts.
    pub charges: Vec<TxResourceChargeV1>,
}

impl BlockResourceSettlementV1 {
    /// Encodes the block settlement receipt into canonical big-endian wire bytes.
    pub fn encode_canonical(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(164 + self.charges.len() * TxResourceChargeV1::WIRE_LEN);
        out.extend_from_slice(&self.policy_digest);
        out.extend_from_slice(&self.block_height.to_be_bytes());
        out.extend_from_slice(self.proposer.as_bytes());
        out.extend_from_slice(&self.base_fee_per_gas.to_be_bytes());
        out.extend_from_slice(&self.next_base_fee_per_gas.to_be_bytes());
        out.extend_from_slice(&self.block_gas_used.to_be_bytes());
        out.extend_from_slice(&self.total_burned_fees.to_be_bytes());
        out.extend_from_slice(&self.total_proposer_tips.to_be_bytes());
        out.extend_from_slice(&self.net_locked_deposit_delta.to_be_bytes());
        out.extend_from_slice(&(self.charges.len() as u32).to_be_bytes());
        for c in &self.charges {
            out.extend_from_slice(c.payer.as_bytes());
            out.push(u8::from(c.admitted_succeeded));
            out.extend_from_slice(&c.gas_used.to_be_bytes());
            out.extend_from_slice(&c.burned_fee.to_be_bytes());
            out.extend_from_slice(&c.proposer_tip.to_be_bytes());
            out.extend_from_slice(&c.deposit_locked.to_be_bytes());
            out.extend_from_slice(&c.deposit_refunded.to_be_bytes());
        }
        out
    }

    /// Computes the canonical `Receipt` `ContentTag` for this block settlement.
    pub fn tag(&self) -> ContentTag {
        let bytes = self.encode_canonical();
        hash_canonical(
            TagKind::Receipt,
            RESOURCE_1_SETTLEMENT_SCHEMA,
            bytes.len() as u64,
            |w| w(&bytes),
        )
        .expect("static schema and bounded length")
    }
}

/// Deterministic conservation-checked native fee, tip, and refundable-deposit ledger.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResourceAccountingLedgerV1 {
    /// Liquid native balance per actor.
    pub balances: BTreeMap<ActorIdV1, u128>,
    /// Locked refundable deposit `(owner, amount)` per occupied mutable coordinate.
    pub locked_deposits: BTreeMap<Coord, (ActorIdV1, u128)>,
    /// Cumulative permanently burned base fees.
    pub total_burned_fees: u128,
    /// Fixed initial issued supply conserved across all liquid, locked, and burned buckets.
    pub total_issued_supply: u128,
    /// Active burned base fee per gas for the next settled block.
    pub current_base_fee: u128,
}

impl ResourceAccountingLedgerV1 {
    /// Creates a new ledger from initial actor balances and starting `base_fee_per_gas`.
    pub fn new(
        initial_balances: BTreeMap<ActorIdV1, u128>,
        initial_base_fee: u128,
    ) -> Result<Self, Resource1Error> {
        let mut total = 0u128;
        for &bal in initial_balances.values() {
            total = total
                .checked_add(bal)
                .ok_or(Resource1Error::ArithmeticOverflow("initial supply sum"))?;
        }
        Ok(Self {
            balances: initial_balances,
            locked_deposits: BTreeMap::new(),
            total_burned_fees: 0,
            total_issued_supply: total,
            current_base_fee: initial_base_fee,
        })
    }

    /// Verifies exact closed conservation: `liquid + locked_deposits + burned == issued`.
    pub fn verify_conservation(&self) -> Result<(), Resource1Error> {
        if self.locked_deposits.contains_key(&BEDROCK_COORD) {
            return Err(Resource1Error::BedrockDepositForbidden);
        }
        let mut sum = self.total_burned_fees;
        for &bal in self.balances.values() {
            sum = sum
                .checked_add(bal)
                .ok_or(Resource1Error::ArithmeticOverflow("liquid sum"))?;
        }
        for &(_, dep) in self.locked_deposits.values() {
            sum = sum
                .checked_add(dep)
                .ok_or(Resource1Error::ArithmeticOverflow("deposit sum"))?;
        }
        if sum != self.total_issued_supply {
            return Err(Resource1Error::ConservationInvariantBroken);
        }
        Ok(())
    }

    /// Atomically settles one block of admitted transactions under `policy`, advancing the
    /// EIP-1559 base fee and enforcing strict supply conservation.
    pub fn settle_block(
        &mut self,
        policy: &ResourcePolicyV1,
        block_height: u64,
        proposer: ActorIdV1,
        requests: &[TxResourceRequestV1],
    ) -> Result<BlockResourceSettlementV1, Resource1Error> {
        let policy_digest = policy.tag()?.digest();
        let active_base_fee = self
            .current_base_fee
            .clamp(policy.min_base_fee_per_gas, policy.max_base_fee_per_gas);
        let mut next_ledger = self.clone();
        let mut block_gas_used = 0u64;
        let mut total_burned = 0u128;
        let mut total_tips = 0u128;
        let mut total_locked = 0u128;
        let mut total_refunded = 0u128;
        let mut charges = Vec::with_capacity(requests.len());

        for req in requests {
            let gas_used = policy.meter_work(&req.work)?;
            let next_block_gas = block_gas_used
                .checked_add(gas_used)
                .ok_or(Resource1Error::ArithmeticOverflow("block gas sum"))?;
            if next_block_gas > policy.max_block_gas {
                return Err(Resource1Error::BlockGasLimitExceeded {
                    requested: next_block_gas,
                    limit: policy.max_block_gas,
                });
            }
            if req.max_fee_per_gas < active_base_fee {
                return Err(Resource1Error::MaxFeeBelowBaseFee {
                    max_fee_per_gas: req.max_fee_per_gas,
                    base_fee_per_gas: active_base_fee,
                });
            }
            let headroom = req.max_fee_per_gas - active_base_fee;
            let eff_tip_per_gas = req
                .max_priority_tip_per_gas
                .min(headroom)
                .min(policy.max_priority_tip_per_gas);
            let burned_fee = active_base_fee
                .checked_mul(u128::from(gas_used))
                .ok_or(Resource1Error::ArithmeticOverflow("burned_fee mul"))?;
            let proposer_tip = eff_tip_per_gas
                .checked_mul(u128::from(gas_used))
                .ok_or(Resource1Error::ArithmeticOverflow("proposer_tip mul"))?;
            let fee_total = burned_fee
                .checked_add(proposer_tip)
                .ok_or(Resource1Error::ArithmeticOverflow("fee_total add"))?;

            let mut deposit_locked = 0u128;
            let mut deposit_refunded = 0u128;
            let mut staged_mints = BTreeSet::new();
            let mut staged_burns = BTreeSet::new();

            if req.execution_succeeded {
                for &coord in &req.minted_coords {
                    if coord == BEDROCK_COORD {
                        return Err(Resource1Error::BedrockDepositForbidden);
                    }
                    if !staged_mints.insert(coord)
                        || next_ledger.locked_deposits.contains_key(&coord)
                    {
                        return Err(Resource1Error::DepositStateViolation(
                            "duplicate or already-occupied mint coordinate",
                        ));
                    }
                    deposit_locked = deposit_locked
                        .checked_add(policy.deposit_per_occupied_block)
                        .ok_or(Resource1Error::ArithmeticOverflow("deposit_locked add"))?;
                }
                for &coord in &req.burned_coords {
                    if coord == BEDROCK_COORD {
                        return Err(Resource1Error::BedrockDepositForbidden);
                    }
                    if !staged_burns.insert(coord) {
                        return Err(Resource1Error::DepositStateViolation(
                            "duplicate burn coordinate",
                        ));
                    }
                    match next_ledger.locked_deposits.get(&coord) {
                        Some(&(owner, amount)) if owner == req.payer => {
                            deposit_refunded = deposit_refunded
                                .checked_add(amount)
                                .ok_or(Resource1Error::ArithmeticOverflow("deposit_refunded"))?;
                        }
                        _ => {
                            return Err(Resource1Error::DepositStateViolation(
                                "burn coordinate missing or not owned by payer",
                            ));
                        }
                    }
                }
            }

            let available = *next_ledger.balances.get(&req.payer).unwrap_or(&0);
            let debit = fee_total
                .checked_add(deposit_locked)
                .ok_or(Resource1Error::ArithmeticOverflow("debit add"))?;
            if available < debit {
                return Err(Resource1Error::InsufficientLiquidBalance {
                    required: debit,
                    available,
                });
            }
            let after_debit = available - debit;
            let final_payer_bal = after_debit
                .checked_add(deposit_refunded)
                .ok_or(Resource1Error::ArithmeticOverflow("refund credit"))?;
            next_ledger.balances.insert(req.payer, final_payer_bal);
            let prop_bal = *next_ledger.balances.get(&proposer).unwrap_or(&0);
            next_ledger.balances.insert(
                proposer,
                prop_bal
                    .checked_add(proposer_tip)
                    .ok_or(Resource1Error::ArithmeticOverflow("proposer credit"))?,
            );
            next_ledger.total_burned_fees =
                next_ledger
                    .total_burned_fees
                    .checked_add(burned_fee)
                    .ok_or(Resource1Error::ArithmeticOverflow("total_burned add"))?;

            if req.execution_succeeded {
                for coord in staged_mints {
                    next_ledger
                        .locked_deposits
                        .insert(coord, (req.payer, policy.deposit_per_occupied_block));
                }
                for coord in staged_burns {
                    next_ledger.locked_deposits.remove(&coord);
                }
            }

            block_gas_used = next_block_gas;
            total_burned = total_burned
                .checked_add(burned_fee)
                .ok_or(Resource1Error::ArithmeticOverflow("block burned"))?;
            total_tips = total_tips
                .checked_add(proposer_tip)
                .ok_or(Resource1Error::ArithmeticOverflow("block tips"))?;
            total_locked = total_locked
                .checked_add(deposit_locked)
                .ok_or(Resource1Error::ArithmeticOverflow("block locked"))?;
            total_refunded = total_refunded
                .checked_add(deposit_refunded)
                .ok_or(Resource1Error::ArithmeticOverflow("block refunded"))?;
            charges.push(TxResourceChargeV1 {
                payer: req.payer,
                admitted_succeeded: req.execution_succeeded,
                gas_used,
                burned_fee,
                proposer_tip,
                deposit_locked,
                deposit_refunded,
            });
        }

        let next_base_fee = policy.next_base_fee(active_base_fee, block_gas_used)?;
        next_ledger.current_base_fee = next_base_fee;
        next_ledger.verify_conservation()?;
        *self = next_ledger;

        let locked_i = i128::try_from(total_locked)
            .map_err(|_| Resource1Error::ArithmeticOverflow("locked i128"))?;
        let refunded_i = i128::try_from(total_refunded)
            .map_err(|_| Resource1Error::ArithmeticOverflow("refunded i128"))?;
        Ok(BlockResourceSettlementV1 {
            policy_digest,
            block_height,
            proposer,
            base_fee_per_gas: active_base_fee,
            next_base_fee_per_gas: next_base_fee,
            block_gas_used,
            total_burned_fees: total_burned,
            total_proposer_tips: total_tips,
            net_locked_deposit_delta: locked_i - refunded_i,
            charges,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::local_bounds::LocalStorageProfile;
    use crate::profile_bootstrap::{ProfileScopeV1, ProposedLimitVectorV1};
    use crate::threat_1::ThreatModelV1;

    #[test]
    fn resource_1_policy_eip1559_and_deposit_conservation() {
        let scope = ProfileScopeV1 {
            schema_version: 1,
            base_transition_spec_digest: [0x11; 32],
            domain_registry_digest: [0x22; 32],
            crypto_suite_digest: [0x33; 32],
            asset_policy_digest: [0x44; 32],
            validator_econ_digest: [0x55; 32],
            consensus_policy_digest: [0x66; 32],
            governance_policy_digest: [0x77; 32],
            proposed_limits: ProposedLimitVectorV1::v1_default(),
            activated_predicates: vec!["asset-active".to_string()],
        };
        let threat = ThreatModelV1::v1_conservative_consumer(&scope).unwrap();
        let bounds =
            ProtocolBoundsProfileV1::v1_consumer_default(&threat, &LocalStorageProfile::DEFAULT)
                .unwrap();
        let asset = AssetPolicyDescriptorV1::v1_dual_domain_default();
        let policy = ResourcePolicyV1::v1_default(&bounds, &asset).unwrap();
        let raw = policy.encode_canonical().unwrap();
        assert_eq!(ResourcePolicyV1::decode_canonical(&raw).unwrap(), policy);

        // 100% full block pushes base fee up by >= +1; empty block decays by 12.5%.
        assert_eq!(policy.next_base_fee(1, 100_000_000).unwrap(), 2);
        assert_eq!(policy.next_base_fee(100, 100_000_000).unwrap(), 112);
        assert_eq!(policy.next_base_fee(100, 0).unwrap(), 88);

        let alice = ActorIdV1::from_bytes([0x11; 32]);
        let proposer = ActorIdV1::from_bytes([0x22; 32]);
        let mut ledger = ResourceAccountingLedgerV1::new(
            BTreeMap::from([(alice, 100_000_000_000u128), (proposer, 0u128)]),
            10,
        )
        .unwrap();
        let work = TxResourceWorkV1 {
            tx_bytes: 256,
            ed25519_signatures: 1,
            state_writes: 2,
            proof_bytes: 256,
            occupancy_ops: 2,
        };
        let s1 = ledger
            .settle_block(
                &policy,
                1,
                proposer,
                &[
                    TxResourceRequestV1 {
                        payer: alice,
                        max_fee_per_gas: 50,
                        max_priority_tip_per_gas: 5,
                        work,
                        execution_succeeded: true,
                        minted_coords: vec![Coord::new(1, 0, 0), Coord::new(2, 0, 0)],
                        burned_coords: vec![],
                    },
                    TxResourceRequestV1 {
                        payer: alice,
                        max_fee_per_gas: 50,
                        max_priority_tip_per_gas: 5,
                        work,
                        execution_succeeded: false,
                        minted_coords: vec![Coord::new(3, 0, 0)],
                        burned_coords: vec![],
                    },
                ],
            )
            .unwrap();
        assert_eq!(s1.charges.len(), 2);
        assert_eq!(s1.charges[0].deposit_locked, 2_000_000);
        assert_eq!(s1.charges[1].deposit_locked, 0);
        assert!(s1.charges[1].burned_fee > 0);
        assert_eq!(ledger.locked_deposits.len(), 2);
        ledger.verify_conservation().unwrap();
    }
}
