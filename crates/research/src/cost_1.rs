//! `COST-1`: canonical browser-game-platform resource budgets, deterministic cost evaluation,
//! and measurement receipts covering Bitblock L1 + `BITS-ECONOMY-1` (`wasm32`-clean, zero new
//! Cargo dependencies, strictly testnet/devnet research boundary).
//!
//! Extends the local-reference budgets in `docs/COST_1_BUDGET.md` (`PS-063:COST-1@budget=BOUNDED`)
//! and `docs/COST_1_COMMIT_1_MEASUREMENTS.md` to the activated Bits creator-economy surface
//! (`Gameladder` + `Bitblock Studio` + `bitblock-light-wasm`), fixing explicit attackable ceilings
//! before optimization and enforcing powered negative controls:
//! - Browser light verifier budget (`bitblock-light-wasm <= 256,000 B` gzipped),
//! - Daily mobile/browser light-client session budget (`<= 1,048,576 B` = `1 MiB`),
//! - `AUTH-COMMIT-1` Domain-3 Patricia wallet proof budget (`<= 12,288 B` per `BOUNDS-1`),
//! - Canonical signed `BitsPurchaseSplitV1` tx wire (`<= 512 B`) and `BitsStateReceiptV1` (`<= 256 B`),
//! - Per-tx `PurchaseSplit` execution (`<= 1,000 us`) and wallet proof verification (`<= 500 us`),
//! - Exact integer annual raw payload growth at `10 TPS` (`<= 256 GiB/yr` = `262,144 MiB/yr`).
//!
//! Non-claim: a single host, finite account count, or testnet workload does not prove universal
//! cheapness, quiet-host Windows x86_64 p99 latency, or production readiness (`PROD-OPS-1`,
//! `L1-PRODUCTION`, and `VALUE-1` remain excluded).

use std::fmt;

use crate::protocol::{ContentTag, TagKind, hash_canonical};

/// Canonical schema string for [`Cost1PlatformBudgetProfileV1`].
pub const COST_1_PLATFORM_BUDGET_PROFILE_SCHEMA: &[u8] =
    b"bitblock/cost-1/platform-budget-profile/v1";
/// Canonical schema string for [`Cost1MeasurementReceiptV1`].
pub const COST_1_MEASUREMENT_RECEIPT_SCHEMA: &[u8] = b"bitblock/cost-1/measurement-receipt/v1";

/// Protocol schema version for `COST-1` canonical objects (`1`).
pub const COST_1_VERSION: u16 = 1;
/// Seconds in one 365-day year (`31,536,000`), used for exact integer storage projections.
pub const SECONDS_PER_YEAR: u64 = 365 * 24 * 60 * 60;
/// Bytes per mebibyte (`1,048,576`).
pub const BYTES_PER_MIB: u64 = 1_048_576;
/// Bitmask requiring all 6 powered negative controls (`bits 0..=5`) before `Met` is allowed.
pub const COST_1_ALL_CONTROLS_MASK: u32 = 0x3F;

/// Powered control bit `0`: 1-byte-tighter wallet proof cap trips fail-closed.
pub const COST_1_CTRL_WALLET_PROOF_CAP: u32 = 1 << 0;
/// Powered control bit `1`: 1-byte-over `bitblock-light-wasm` gzip cap trips fail-closed.
pub const COST_1_CTRL_WASM_GZIP_CAP: u32 = 1 << 1;
/// Powered control bit `2`: 1-byte-over signed `PurchaseSplit` wire cap trips fail-closed.
pub const COST_1_CTRL_TX_WIRE_CAP: u32 = 1 << 2;
/// Powered control bit `3`: 1-microsecond-tighter proof-verify latency cap trips `Miss`.
pub const COST_1_CTRL_VERIFY_LATENCY_CAP: u32 = 1 << 3;
/// Powered control bit `4`: 1-byte-over daily phone session cap trips `Miss`.
pub const COST_1_CTRL_PHONE_BUDGET_CAP: u32 = 1 << 4;
/// Powered control bit `5`: unpowered control mask (`!= 0x3F`) trips `Unmeasured`.
pub const COST_1_CTRL_UNPOWERED_GUARD: u32 = 1 << 5;

/// Fail-closed error returned by `COST-1` codecs, projections, and budget evaluators.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Cost1Error {
    /// Canonical wire buffer had invalid length, version, discriminant, or trailing bytes.
    InvalidWire(&'static str),
    /// Fixed budget profile contained a zero or inconsistent ceiling.
    InvalidProfile(&'static str),
    /// Integer storage or latency calculation overflowed `u64`/`u128`.
    ArithmeticOverflow(&'static str),
}

impl fmt::Display for Cost1Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}

impl std::error::Error for Cost1Error {}

/// Canonical disposition vocabulary matching `docs/COST_1_BUDGET.md`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum Cost1DispositionV1 {
    /// All measured budgets within fixed ceilings and all 6 powered controls verified.
    Met = 0,
    /// At least one measured observation crossed an already-fixed ceiling.
    Miss = 1,
    /// Budget is fixed, but required counters, timings, or powered controls are incomplete.
    Unmeasured = 2,
    /// Surface not activated in the evaluated profile.
    Na = 3,
    /// Historical context only; ineligible to pass a current budget.
    Historical = 4,
}

impl Cost1DispositionV1 {
    /// Returns the canonical 1-byte wire discriminant.
    pub const fn code(self) -> u8 {
        self as u8
    }

    /// Decodes a canonical 1-byte disposition discriminant.
    pub const fn from_code(code: u8) -> Result<Self, Cost1Error> {
        match code {
            0 => Ok(Self::Met),
            1 => Ok(Self::Miss),
            2 => Ok(Self::Unmeasured),
            3 => Ok(Self::Na),
            4 => Ok(Self::Historical),
            _ => Err(Cost1Error::InvalidWire("unknown Cost1DispositionV1")),
        }
    }
}

/// Fixed browser-game-platform cost budget profile (`46` canonical big-endian wire bytes).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Cost1PlatformBudgetProfileV1 {
    /// Schema version (`1`).
    pub version: u16,
    /// Maximum gzipped `bitblock-light-wasm` download size in bytes (`256,000` = `250 KiB`).
    pub max_light_wasm_gzip_bytes: u32,
    /// Maximum daily phone/browser light-client proof-door download in bytes (`1,048,576` = `1 MiB`).
    pub max_phone_session_bytes: u32,
    /// Maximum `AUTH-COMMIT-1` Domain-3 wallet Patricia proof bytes (`12,288` = `12 KiB`).
    pub max_wallet_proof_bytes: u32,
    /// Maximum canonical signed `BitsPurchaseSplitV1` transaction wire bytes (`512` B).
    pub max_purchase_tx_wire_bytes: u32,
    /// Maximum canonical `BitsStateReceiptV1` wire bytes (`256` B).
    pub max_receipt_wire_bytes: u32,
    /// Maximum per-wallet Patricia proof verification latency in microseconds (`500` us).
    pub max_verify_wallet_proof_us: u64,
    /// Maximum per-tx `BitsPurchaseSplitV1` execution latency in microseconds (`1,000` us).
    pub max_apply_purchase_split_us: u64,
    /// Maximum full-block verification budget in milliseconds (`400` ms per `BOUNDS-1`).
    pub max_block_verify_budget_ms: u32,
    /// Maximum projected annual raw tx+receipt payload growth at `10 TPS` in MiB (`262,144` = `256 GiB`).
    pub max_annual_mib_at_10tps: u32,
}

impl Cost1PlatformBudgetProfileV1 {
    /// Exact canonical big-endian wire length (`46` bytes).
    pub const WIRE_LEN: usize = 2 + 4 * 5 + 8 * 2 + 4 * 2;

    /// Returns the fixed canonical browser-game-platform testnet budget profile.
    pub const fn canonical_platform_default() -> Self {
        Self {
            version: COST_1_VERSION,
            max_light_wasm_gzip_bytes: 256_000,
            max_phone_session_bytes: 1_048_576,
            max_wallet_proof_bytes: 12_288,
            max_purchase_tx_wire_bytes: 512,
            max_receipt_wire_bytes: 256,
            max_verify_wallet_proof_us: 500,
            max_apply_purchase_split_us: 1_000,
            max_block_verify_budget_ms: 400,
            max_annual_mib_at_10tps: 262_144,
        }
    }

    /// Validates that every fixed ceiling is strictly positive.
    pub const fn validate(&self) -> Result<(), Cost1Error> {
        if self.version != COST_1_VERSION {
            return Err(Cost1Error::InvalidProfile("version must equal 1"));
        }
        if self.max_light_wasm_gzip_bytes == 0
            || self.max_phone_session_bytes == 0
            || self.max_wallet_proof_bytes == 0
            || self.max_purchase_tx_wire_bytes == 0
            || self.max_receipt_wire_bytes == 0
            || self.max_verify_wallet_proof_us == 0
            || self.max_apply_purchase_split_us == 0
            || self.max_block_verify_budget_ms == 0
            || self.max_annual_mib_at_10tps == 0
        {
            return Err(Cost1Error::InvalidProfile(
                "all cost budget ceilings must be strictly positive",
            ));
        }
        Ok(())
    }

    /// Encodes this budget profile into 46 canonical big-endian bytes.
    pub fn encode_canonical(&self) -> Result<[u8; Self::WIRE_LEN], Cost1Error> {
        self.validate()?;
        let mut out = [0u8; Self::WIRE_LEN];
        out[0..2].copy_from_slice(&self.version.to_be_bytes());
        out[2..6].copy_from_slice(&self.max_light_wasm_gzip_bytes.to_be_bytes());
        out[6..10].copy_from_slice(&self.max_phone_session_bytes.to_be_bytes());
        out[10..14].copy_from_slice(&self.max_wallet_proof_bytes.to_be_bytes());
        out[14..18].copy_from_slice(&self.max_purchase_tx_wire_bytes.to_be_bytes());
        out[18..22].copy_from_slice(&self.max_receipt_wire_bytes.to_be_bytes());
        out[22..30].copy_from_slice(&self.max_verify_wallet_proof_us.to_be_bytes());
        out[30..38].copy_from_slice(&self.max_apply_purchase_split_us.to_be_bytes());
        out[38..42].copy_from_slice(&self.max_block_verify_budget_ms.to_be_bytes());
        out[42..46].copy_from_slice(&self.max_annual_mib_at_10tps.to_be_bytes());
        Ok(out)
    }

    /// Decodes and validates a 46-byte canonical [`Cost1PlatformBudgetProfileV1`].
    pub fn decode_canonical(bytes: &[u8]) -> Result<Self, Cost1Error> {
        if bytes.len() != Self::WIRE_LEN {
            return Err(Cost1Error::InvalidWire(
                "Cost1PlatformBudgetProfileV1 must be 46 bytes",
            ));
        }
        let prof = Self {
            version: u16::from_be_bytes([bytes[0], bytes[1]]),
            max_light_wasm_gzip_bytes: u32::from_be_bytes(bytes[2..6].try_into().expect("4")),
            max_phone_session_bytes: u32::from_be_bytes(bytes[6..10].try_into().expect("4")),
            max_wallet_proof_bytes: u32::from_be_bytes(bytes[10..14].try_into().expect("4")),
            max_purchase_tx_wire_bytes: u32::from_be_bytes(bytes[14..18].try_into().expect("4")),
            max_receipt_wire_bytes: u32::from_be_bytes(bytes[18..22].try_into().expect("4")),
            max_verify_wallet_proof_us: u64::from_be_bytes(bytes[22..30].try_into().expect("8")),
            max_apply_purchase_split_us: u64::from_be_bytes(bytes[30..38].try_into().expect("8")),
            max_block_verify_budget_ms: u32::from_be_bytes(bytes[38..42].try_into().expect("4")),
            max_annual_mib_at_10tps: u32::from_be_bytes(bytes[42..46].try_into().expect("4")),
        };
        prof.validate()?;
        Ok(prof)
    }

    /// Computes the canonical `Policy` [`ContentTag`] for this budget profile.
    pub fn tag(&self) -> Result<ContentTag, Cost1Error> {
        let bytes = self.encode_canonical()?;
        Ok(hash_canonical(
            TagKind::Policy,
            COST_1_PLATFORM_BUDGET_PROFILE_SCHEMA,
            bytes.len() as u64,
            |w| w(&bytes),
        )
        .expect("static schema and fixed length"))
    }
}

/// Raw deterministic counters and host timings fed into [`evaluate_cost_1_platform_envelope`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Cost1PlatformObservationV1 {
    /// Observed gzipped `bitblock-light-wasm` size in bytes (`149,181` B on `main`).
    pub light_wasm_gzip_bytes: u32,
    /// Observed canonical signed `BitsPurchaseSplitV1` wire size in bytes (`367` B).
    pub purchase_tx_wire_bytes: u32,
    /// Observed canonical `BitsStateReceiptV1` wire size in bytes (`236` B).
    pub receipt_wire_bytes: u32,
    /// Observed Domain-3 Patricia inclusion proof bytes.
    pub wallet_inclusion_proof_bytes: u32,
    /// Observed Domain-3 Patricia non-membership proof bytes.
    pub wallet_absence_proof_bytes: u32,
    /// Observed `/l1.html` session proof-door fetch bytes (e.g., `30,720` B).
    pub phone_session_door_bytes: u32,
    /// Number of active accounts sampled in the measured Bits ledger.
    pub active_accounts_sampled: u32,
    /// Median `BitsPurchaseSplitV1` apply latency in microseconds (`0` = unmeasured).
    pub apply_purchase_split_p50_us: u64,
    /// Max `BitsPurchaseSplitV1` apply latency in microseconds (`0` = unmeasured).
    pub apply_purchase_split_max_us: u64,
    /// Median `verify_bits_wallet_proof` latency in microseconds (`0` = unmeasured).
    pub verify_wallet_proof_p50_us: u64,
    /// Max `verify_bits_wallet_proof` latency in microseconds (`0` = unmeasured).
    pub verify_wallet_proof_max_us: u64,
    /// Median `BitsTestnetGrantV1` apply latency in microseconds (`0` = unmeasured).
    pub apply_grant_p50_us: u64,
    /// Bitmask of powered negative controls that fired during the measurement run.
    pub powered_controls_mask: u32,
}

/// Canonical `COST-1` measurement receipt (`111` big-endian wire bytes).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Cost1MeasurementReceiptV1 {
    /// Schema version (`1`).
    pub version: u16,
    /// 32-byte digest of the [`Cost1PlatformBudgetProfileV1`] evaluated against.
    pub profile_digest: [u8; 32],
    /// Deterministic overall disposition (`Met`, `Miss`, or `Unmeasured`).
    pub disposition: Cost1DispositionV1,
    /// Observed gzipped `bitblock-light-wasm` size in bytes.
    pub light_wasm_gzip_bytes: u32,
    /// Observed canonical signed `BitsPurchaseSplitV1` wire bytes.
    pub purchase_tx_wire_bytes: u32,
    /// Observed canonical `BitsStateReceiptV1` wire bytes.
    pub receipt_wire_bytes: u32,
    /// Observed Domain-3 Patricia inclusion proof bytes.
    pub wallet_inclusion_proof_bytes: u32,
    /// Observed Domain-3 Patricia non-membership proof bytes.
    pub wallet_absence_proof_bytes: u32,
    /// Observed `/l1.html` session door fetch bytes.
    pub phone_session_door_bytes: u32,
    /// Number of active accounts sampled.
    pub active_accounts_sampled: u32,
    /// Exact integer projected annual MiB of raw tx+receipt bytes at sustained `10 TPS`.
    pub projected_annual_mib_at_10tps: u32,
    /// Median `BitsPurchaseSplitV1` apply latency in microseconds.
    pub apply_purchase_split_p50_us: u64,
    /// Max `BitsPurchaseSplitV1` apply latency in microseconds.
    pub apply_purchase_split_max_us: u64,
    /// Median `verify_bits_wallet_proof` latency in microseconds.
    pub verify_wallet_proof_p50_us: u64,
    /// Max `verify_bits_wallet_proof` latency in microseconds.
    pub verify_wallet_proof_max_us: u64,
    /// Median `BitsTestnetGrantV1` apply latency in microseconds.
    pub apply_grant_p50_us: u64,
    /// Bitmask of powered negative controls verified (`0x3F` required for `Met`).
    pub powered_controls_mask: u32,
}

impl Cost1MeasurementReceiptV1 {
    /// Exact canonical big-endian wire length (`111` bytes).
    pub const WIRE_LEN: usize = 2 + 32 + 1 + 4 * 8 + 8 * 5 + 4;

    /// Encodes this measurement receipt into 111 canonical big-endian bytes.
    pub fn encode_canonical(&self) -> Result<[u8; Self::WIRE_LEN], Cost1Error> {
        if self.version != COST_1_VERSION {
            return Err(Cost1Error::InvalidWire("receipt version must equal 1"));
        }
        let mut out = [0u8; Self::WIRE_LEN];
        out[0..2].copy_from_slice(&self.version.to_be_bytes());
        out[2..34].copy_from_slice(&self.profile_digest);
        out[34] = self.disposition.code();
        out[35..39].copy_from_slice(&self.light_wasm_gzip_bytes.to_be_bytes());
        out[39..43].copy_from_slice(&self.purchase_tx_wire_bytes.to_be_bytes());
        out[43..47].copy_from_slice(&self.receipt_wire_bytes.to_be_bytes());
        out[47..51].copy_from_slice(&self.wallet_inclusion_proof_bytes.to_be_bytes());
        out[51..55].copy_from_slice(&self.wallet_absence_proof_bytes.to_be_bytes());
        out[55..59].copy_from_slice(&self.phone_session_door_bytes.to_be_bytes());
        out[59..63].copy_from_slice(&self.active_accounts_sampled.to_be_bytes());
        out[63..67].copy_from_slice(&self.projected_annual_mib_at_10tps.to_be_bytes());
        out[67..75].copy_from_slice(&self.apply_purchase_split_p50_us.to_be_bytes());
        out[75..83].copy_from_slice(&self.apply_purchase_split_max_us.to_be_bytes());
        out[83..91].copy_from_slice(&self.verify_wallet_proof_p50_us.to_be_bytes());
        out[91..99].copy_from_slice(&self.verify_wallet_proof_max_us.to_be_bytes());
        out[99..107].copy_from_slice(&self.apply_grant_p50_us.to_be_bytes());
        out[107..111].copy_from_slice(&self.powered_controls_mask.to_be_bytes());
        Ok(out)
    }

    /// Decodes a 111-byte canonical [`Cost1MeasurementReceiptV1`].
    pub fn decode_canonical(bytes: &[u8]) -> Result<Self, Cost1Error> {
        if bytes.len() != Self::WIRE_LEN {
            return Err(Cost1Error::InvalidWire(
                "Cost1MeasurementReceiptV1 must be 111 bytes",
            ));
        }
        let version = u16::from_be_bytes([bytes[0], bytes[1]]);
        if version != COST_1_VERSION {
            return Err(Cost1Error::InvalidWire("receipt version must equal 1"));
        }
        Ok(Self {
            version,
            profile_digest: bytes[2..34].try_into().expect("32"),
            disposition: Cost1DispositionV1::from_code(bytes[34])?,
            light_wasm_gzip_bytes: u32::from_be_bytes(bytes[35..39].try_into().expect("4")),
            purchase_tx_wire_bytes: u32::from_be_bytes(bytes[39..43].try_into().expect("4")),
            receipt_wire_bytes: u32::from_be_bytes(bytes[43..47].try_into().expect("4")),
            wallet_inclusion_proof_bytes: u32::from_be_bytes(bytes[47..51].try_into().expect("4")),
            wallet_absence_proof_bytes: u32::from_be_bytes(bytes[51..55].try_into().expect("4")),
            phone_session_door_bytes: u32::from_be_bytes(bytes[55..59].try_into().expect("4")),
            active_accounts_sampled: u32::from_be_bytes(bytes[59..63].try_into().expect("4")),
            projected_annual_mib_at_10tps: u32::from_be_bytes(bytes[63..67].try_into().expect("4")),
            apply_purchase_split_p50_us: u64::from_be_bytes(bytes[67..75].try_into().expect("8")),
            apply_purchase_split_max_us: u64::from_be_bytes(bytes[75..83].try_into().expect("8")),
            verify_wallet_proof_p50_us: u64::from_be_bytes(bytes[83..91].try_into().expect("8")),
            verify_wallet_proof_max_us: u64::from_be_bytes(bytes[91..99].try_into().expect("8")),
            apply_grant_p50_us: u64::from_be_bytes(bytes[99..107].try_into().expect("8")),
            powered_controls_mask: u32::from_be_bytes(bytes[107..111].try_into().expect("4")),
        })
    }

    /// Computes the canonical `Receipt` [`ContentTag`] for this measurement receipt.
    pub fn tag(&self) -> Result<ContentTag, Cost1Error> {
        let bytes = self.encode_canonical()?;
        Ok(hash_canonical(
            TagKind::Receipt,
            COST_1_MEASUREMENT_RECEIPT_SCHEMA,
            bytes.len() as u64,
            |w| w(&bytes),
        )
        .expect("static schema and fixed length"))
    }
}

/// Computes the exact integer ceiling MiB/year for a constant `bytes_per_tx` at sustained `tps`
/// (`zero` floating-point arithmetic).
pub fn annual_mib_at_tps(bytes_per_tx: u64, tps: u64) -> Result<u32, Cost1Error> {
    let total_bytes = u128::from(bytes_per_tx)
        .checked_mul(u128::from(tps))
        .and_then(|v| v.checked_mul(u128::from(SECONDS_PER_YEAR)))
        .ok_or(Cost1Error::ArithmeticOverflow("annual byte count overflow"))?;
    let mib_u128 = total_bytes
        .checked_add(u128::from(BYTES_PER_MIB) - 1)
        .ok_or(Cost1Error::ArithmeticOverflow("annual MiB ceil overflow"))?
        / u128::from(BYTES_PER_MIB);
    u32::try_from(mib_u128).map_err(|_| Cost1Error::ArithmeticOverflow("annual MiB exceeds u32"))
}

/// Evaluates an observation against a fixed [`Cost1PlatformBudgetProfileV1`] and produces a
/// canonical [`Cost1MeasurementReceiptV1`].
///
/// Disposition precedence (fail-closed):
/// 1. Any measured byte counter or timing exceeding its fixed ceiling -> [`Cost1DispositionV1::Miss`].
/// 2. Any required counter/timing zero (`0`) or missing powered control bit (`!= 0x3F`) ->
///    [`Cost1DispositionV1::Unmeasured`].
/// 3. Otherwise -> [`Cost1DispositionV1::Met`].
pub fn evaluate_cost_1_platform_envelope(
    profile: &Cost1PlatformBudgetProfileV1,
    obs: &Cost1PlatformObservationV1,
) -> Result<Cost1MeasurementReceiptV1, Cost1Error> {
    profile.validate()?;
    let profile_digest = profile.tag()?.digest();
    let per_tx_bytes = u64::from(obs.purchase_tx_wire_bytes) + u64::from(obs.receipt_wire_bytes);
    let projected_annual_mib_at_10tps = annual_mib_at_tps(per_tx_bytes, 10)?;

    let any_miss = obs.light_wasm_gzip_bytes > profile.max_light_wasm_gzip_bytes
        || obs.phone_session_door_bytes > profile.max_phone_session_bytes
        || obs.wallet_inclusion_proof_bytes > profile.max_wallet_proof_bytes
        || obs.wallet_absence_proof_bytes > profile.max_wallet_proof_bytes
        || obs.purchase_tx_wire_bytes > profile.max_purchase_tx_wire_bytes
        || obs.receipt_wire_bytes > profile.max_receipt_wire_bytes
        || obs.verify_wallet_proof_p50_us > profile.max_verify_wallet_proof_us
        || obs.apply_purchase_split_p50_us > profile.max_apply_purchase_split_us
        || projected_annual_mib_at_10tps > profile.max_annual_mib_at_10tps;

    let any_unmeasured = obs.light_wasm_gzip_bytes == 0
        || obs.purchase_tx_wire_bytes == 0
        || obs.receipt_wire_bytes == 0
        || obs.wallet_inclusion_proof_bytes == 0
        || obs.wallet_absence_proof_bytes == 0
        || obs.phone_session_door_bytes == 0
        || obs.active_accounts_sampled == 0
        || obs.apply_purchase_split_p50_us == 0
        || obs.apply_purchase_split_max_us == 0
        || obs.verify_wallet_proof_p50_us == 0
        || obs.verify_wallet_proof_max_us == 0
        || obs.apply_grant_p50_us == 0
        || (obs.powered_controls_mask & COST_1_ALL_CONTROLS_MASK) != COST_1_ALL_CONTROLS_MASK;

    let disposition = if any_miss {
        Cost1DispositionV1::Miss
    } else if any_unmeasured {
        Cost1DispositionV1::Unmeasured
    } else {
        Cost1DispositionV1::Met
    };

    Ok(Cost1MeasurementReceiptV1 {
        version: COST_1_VERSION,
        profile_digest,
        disposition,
        light_wasm_gzip_bytes: obs.light_wasm_gzip_bytes,
        purchase_tx_wire_bytes: obs.purchase_tx_wire_bytes,
        receipt_wire_bytes: obs.receipt_wire_bytes,
        wallet_inclusion_proof_bytes: obs.wallet_inclusion_proof_bytes,
        wallet_absence_proof_bytes: obs.wallet_absence_proof_bytes,
        phone_session_door_bytes: obs.phone_session_door_bytes,
        active_accounts_sampled: obs.active_accounts_sampled,
        projected_annual_mib_at_10tps,
        apply_purchase_split_p50_us: obs.apply_purchase_split_p50_us,
        apply_purchase_split_max_us: obs.apply_purchase_split_max_us,
        verify_wallet_proof_p50_us: obs.verify_wallet_proof_p50_us,
        verify_wallet_proof_max_us: obs.verify_wallet_proof_max_us,
        apply_grant_p50_us: obs.apply_grant_p50_us,
        powered_controls_mask: obs.powered_controls_mask,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cost_1_canonical_roundtrip_and_dispositions() {
        let profile = Cost1PlatformBudgetProfileV1::canonical_platform_default();
        let prof_wire = profile.encode_canonical().expect("encode profile");
        assert_eq!(
            Cost1PlatformBudgetProfileV1::decode_canonical(&prof_wire).expect("decode"),
            profile
        );

        let obs = Cost1PlatformObservationV1 {
            light_wasm_gzip_bytes: 149_181,
            purchase_tx_wire_bytes: 367,
            receipt_wire_bytes: 236,
            wallet_inclusion_proof_bytes: 460,
            wallet_absence_proof_bytes: 380,
            phone_session_door_bytes: 30_720,
            active_accounts_sampled: 64,
            apply_purchase_split_p50_us: 120,
            apply_purchase_split_max_us: 210,
            verify_wallet_proof_p50_us: 15,
            verify_wallet_proof_max_us: 32,
            apply_grant_p50_us: 95,
            powered_controls_mask: COST_1_ALL_CONTROLS_MASK,
        };
        let rec = evaluate_cost_1_platform_envelope(&profile, &obs).expect("eval");
        assert_eq!(rec.disposition, Cost1DispositionV1::Met);
        assert_eq!(rec.projected_annual_mib_at_10tps, 181_353);
        let rec_wire = rec.encode_canonical().expect("encode receipt");
        assert_eq!(
            Cost1MeasurementReceiptV1::decode_canonical(&rec_wire).expect("decode receipt"),
            rec
        );

        assert_eq!(
            profile.tag().expect("ptag").to_string(),
            "#bb.policy.sha256.2lyhmz5uypgxkhwjmvlho7psyiaoc7glzrhtcx4gyikqqgtohrkq"
        );
        assert_eq!(
            rec.tag().expect("rtag").to_string(),
            "#bb.receipt.sha256.us2uwb6mpt6tvt4yd6it7es6jhnkhwqgw23xloik2krw6iiv3cuq"
        );
    }
}
