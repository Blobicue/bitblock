//! `ASSET-D1` and `VALIDATOR-ECON-D1`: dual-domain asset semantics (non-fungible occupancy
//! episode tokens + separate fungible native fee/deposit/stake unit) and bonded PoS conservation.
//!
//! Freezes the pre-genesis asset and validator-economic contracts selected under `ASSET-D1` and
//! `VALIDATOR-ECON-D1`:
//! - **Dual-domain separation:** each live mutable occupied coordinate `(coord, mint_id)` maps
//!   to one transferable occupancy-episode owner, while a strictly separate fungible native unit
//!   pays burned execution fees, refundable per-block storage deposits, and bonded validator stake.
//! - **Bedrock exclusion:** origin `(0, 0, 0)` has no economic token and cannot lock a deposit.
//! - **No forced rent expiry:** anti-spam combines burned base fees (charged even on admitted
//!   failures) with refundable per-block deposits locked on mint and refunded on burn.
//! - **Bonded PoS economics (`VALIDATOR-ECON-D1`):** explicit stake bounds, delayed epoch
//!   activation, unbonding delay, equivocation/inactivity slashing, reward caps, concentration
//!   bounds, and closed supply/weight conservation across genesis, bond, unbond, slash, and exit.

use std::collections::BTreeMap;
use std::fmt;

use crate::identity_d1::ActorIdV1;
use crate::protocol::{ContentTag, TagKind, hash_canonical};
use crate::world::{BEDROCK_COORD, Coord};

/// Canonical schema string for an [`AssetPolicyDescriptorV1`] policy object.
pub const ASSET_D1_POLICY_SCHEMA: &[u8] = b"bitblock/asset-d1/policy/v1";
/// Canonical schema string for a [`ValidatorEconPolicyV1`] pre-genesis PoS policy object.
pub const VALIDATOR_ECON_D1_POLICY_SCHEMA: &[u8] = b"bitblock/validator-econ-d1/policy/v1";

/// Basis points denominator (`10,000 = 100%`).
pub const BPS_DENOMINATOR: u16 = 10_000;

/// Errors returned by `ASSET-D1` or `VALIDATOR-ECON-D1` validation and conservation checks.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AssetEconError {
    /// Policy invariant was violated.
    InvalidPolicy(&'static str),
    /// Wire length or discriminant was malformed.
    InvalidWire(&'static str),
    /// Attempted to mint, burn, or attach economic state to immutable bedrock `(0, 0, 0)`.
    BedrockHasNoEconomicToken,
    /// Coordinate is already occupied by a live episode token.
    CoordinateAlreadyOccupied(Coord),
    /// Coordinate has no live occupancy-episode token.
    CoordinateNotOccupied(Coord),
    /// Caller does not own the live occupancy-episode token at this coordinate.
    NotEpisodeOwner,
    /// Actor lacked sufficient liquid native balance for fee, deposit, or stake bond.
    InsufficientBalance { required: u128, available: u128 },
    /// Arithmetic overflow or supply-cap violation.
    SupplyOrOverflowViolation(&'static str),
    /// Validator stake violated minimum bond, concentration cap, or unbonding delay.
    ValidatorStakeViolation(&'static str),
}

impl fmt::Display for AssetEconError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}

impl std::error::Error for AssetEconError {}

/// Canonical pre-genesis asset policy descriptor frozen by `ASSET-D1`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct AssetPolicyDescriptorV1 {
    /// Schema version (`1`).
    pub schema_version: u16,
    /// Asset model code (`1` = Dual-domain occupancy-episode token + separate fungible native unit).
    pub asset_model_code: u8,
    /// Must be `false`: bedrock `(0, 0, 0)` has no economic token representation.
    pub bedrock_has_economic_token: bool,
    /// Must be `true`: admitted failed transactions still pay `base_fee_burn`.
    pub charge_admitted_failure: bool,
    /// Must be `false`: no forced expiry or state-rent auto-burn.
    pub state_rent_enabled: bool,
    /// Refundable native deposit locked per occupied mutable block (`> 0`).
    pub deposit_per_occupied_block: u128,
    /// Burned base execution/history fee per transaction (`> 0`).
    pub base_fee_burn: u128,
    /// Maximum native fungible supply ceiling (`>= deposit_per_occupied_block`).
    pub max_native_supply: u128,
}

impl AssetPolicyDescriptorV1 {
    /// Fixed canonical wire byte length (`54` bytes).
    pub const WIRE_LEN: usize = 54;

    /// Returns the default v1 dual-domain asset policy selected under `ASSET-D1`.
    pub const fn v1_dual_domain_default() -> Self {
        Self {
            schema_version: 1,
            asset_model_code: 1,
            bedrock_has_economic_token: false,
            charge_admitted_failure: true,
            state_rent_enabled: false,
            deposit_per_occupied_block: 1_000_000,
            base_fee_burn: 1_000,
            max_native_supply: 1_000_000_000_000_000_000,
        }
    }

    /// Validates all dual-domain, bedrock, deposit, and fee invariants.
    pub fn validate(&self) -> Result<(), AssetEconError> {
        if self.schema_version != 1 {
            return Err(AssetEconError::InvalidPolicy("schema_version must be 1"));
        }
        if self.asset_model_code != 1 {
            return Err(AssetEconError::InvalidPolicy(
                "asset_model_code must be 1 (DualDomain)",
            ));
        }
        if self.bedrock_has_economic_token {
            return Err(AssetEconError::InvalidPolicy(
                "bedrock must have no economic token",
            ));
        }
        if !self.charge_admitted_failure {
            return Err(AssetEconError::InvalidPolicy(
                "admitted failures must still pay base_fee_burn",
            ));
        }
        if self.state_rent_enabled {
            return Err(AssetEconError::InvalidPolicy(
                "state_rent_enabled must be false under refundable-deposit policy",
            ));
        }
        if self.deposit_per_occupied_block == 0 || self.base_fee_burn == 0 {
            return Err(AssetEconError::InvalidPolicy(
                "deposit_per_occupied_block and base_fee_burn must be > 0",
            ));
        }
        if self.max_native_supply < self.deposit_per_occupied_block {
            return Err(AssetEconError::InvalidPolicy(
                "max_native_supply below single block deposit",
            ));
        }
        Ok(())
    }

    /// Encodes the asset policy into exact canonical big-endian wire bytes.
    pub fn encode_canonical(&self) -> Result<[u8; Self::WIRE_LEN], AssetEconError> {
        self.validate()?;
        let mut out = [0u8; Self::WIRE_LEN];
        out[0..2].copy_from_slice(&self.schema_version.to_be_bytes());
        out[2] = self.asset_model_code;
        out[3] = u8::from(self.bedrock_has_economic_token);
        out[4] = u8::from(self.charge_admitted_failure);
        out[5] = u8::from(self.state_rent_enabled);
        out[6..22].copy_from_slice(&self.deposit_per_occupied_block.to_be_bytes());
        out[22..38].copy_from_slice(&self.base_fee_burn.to_be_bytes());
        out[38..54].copy_from_slice(&self.max_native_supply.to_be_bytes());
        Ok(out)
    }

    /// Decodes and validates a canonical `AssetPolicyDescriptorV1`.
    pub fn decode_canonical(bytes: &[u8]) -> Result<Self, AssetEconError> {
        if bytes.len() != Self::WIRE_LEN {
            return Err(AssetEconError::InvalidWire("invalid asset policy length"));
        }
        let b = |idx: usize| -> Result<bool, AssetEconError> {
            match bytes[idx] {
                0 => Ok(false),
                1 => Ok(true),
                _ => Err(AssetEconError::InvalidWire("non-boolean flag byte")),
            }
        };
        let policy = Self {
            schema_version: u16::from_be_bytes([bytes[0], bytes[1]]),
            asset_model_code: bytes[2],
            bedrock_has_economic_token: b(3)?,
            charge_admitted_failure: b(4)?,
            state_rent_enabled: b(5)?,
            deposit_per_occupied_block: u128::from_be_bytes(
                bytes[6..22].try_into().expect("16 bytes"),
            ),
            base_fee_burn: u128::from_be_bytes(bytes[22..38].try_into().expect("16 bytes")),
            max_native_supply: u128::from_be_bytes(bytes[38..54].try_into().expect("16 bytes")),
        };
        policy.validate()?;
        Ok(policy)
    }

    /// Computes the canonical `ContentTag` for this asset policy descriptor.
    pub fn tag(&self) -> Result<ContentTag, AssetEconError> {
        let bytes = self.encode_canonical()?;
        Ok(hash_canonical(
            TagKind::Policy,
            ASSET_D1_POLICY_SCHEMA,
            bytes.len() as u64,
            |w| w(&bytes),
        )
        .expect("static schema and fixed length"))
    }
}

/// Live transferable occupancy-episode ownership record in Domain 1.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct OccupancyTokenRecordV1 {
    /// Current owner controlling transfer and burn authority.
    pub owner: ActorIdV1,
    /// Immutable mint episode identifier from `LEDGER-1` (never rewritten by transfers).
    pub mint_episode_id: [u8; 32],
    /// Native storage deposit locked at this coordinate and refunded upon burn.
    pub locked_deposit: u128,
}

/// Deterministic dual-domain conservation ledger verifying exact supply and ownership equations
/// across mint, transfer, burn, admitted failure, bedrock attempt, and compensating mint events.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AssetConservationLedgerV1 {
    policy: AssetPolicyDescriptorV1,
    balances: BTreeMap<ActorIdV1, u128>,
    episodes: BTreeMap<Coord, OccupancyTokenRecordV1>,
    total_issued_supply: u128,
    total_burned_fees: u128,
}

impl AssetConservationLedgerV1 {
    /// Initializes a conservation ledger with genesis native balances under `policy`.
    pub fn from_genesis(
        policy: AssetPolicyDescriptorV1,
        genesis_balances: &[(ActorIdV1, u128)],
    ) -> Result<Self, AssetEconError> {
        policy.validate()?;
        let mut balances = BTreeMap::new();
        let mut total_issued_supply = 0u128;
        for &(actor, amount) in genesis_balances {
            let entry = balances.entry(actor).or_insert(0u128);
            *entry =
                (*entry)
                    .checked_add(amount)
                    .ok_or(AssetEconError::SupplyOrOverflowViolation(
                        "balance overflow",
                    ))?;
            total_issued_supply = total_issued_supply
                .checked_add(amount)
                .ok_or(AssetEconError::SupplyOrOverflowViolation("supply overflow"))?;
        }
        if total_issued_supply > policy.max_native_supply {
            return Err(AssetEconError::SupplyOrOverflowViolation(
                "genesis exceeds max_native_supply",
            ));
        }
        let ledger = Self {
            policy,
            balances,
            episodes: BTreeMap::new(),
            total_issued_supply,
            total_burned_fees: 0,
        };
        ledger.verify_conservation()?;
        Ok(ledger)
    }

    /// Returns the liquid native balance of `actor`.
    pub fn balance_of(&self, actor: &ActorIdV1) -> u128 {
        self.balances.get(actor).copied().unwrap_or(0)
    }

    /// Returns the live occupancy token record at `coord`, if occupied.
    pub fn episode_at(&self, coord: Coord) -> Option<OccupancyTokenRecordV1> {
        self.episodes.get(&coord).copied()
    }

    /// Returns total burned execution fees.
    pub const fn total_burned_fees(&self) -> u128 {
        self.total_burned_fees
    }

    /// Returns live mutable occupancy-episode token count.
    pub fn active_episode_count(&self) -> usize {
        self.episodes.len()
    }

    /// Atomically mints one occupancy-episode token: burns `base_fee_burn`, locks
    /// `deposit_per_occupied_block`, and binds `(coord, mint_episode_id)` to `actor`.
    pub fn apply_mint(
        &mut self,
        actor: ActorIdV1,
        coord: Coord,
        mint_episode_id: [u8; 32],
    ) -> Result<(), AssetEconError> {
        if coord == BEDROCK_COORD {
            return Err(AssetEconError::BedrockHasNoEconomicToken);
        }
        if self.episodes.contains_key(&coord) {
            return Err(AssetEconError::CoordinateAlreadyOccupied(coord));
        }
        let required = self
            .policy
            .base_fee_burn
            .checked_add(self.policy.deposit_per_occupied_block)
            .ok_or(AssetEconError::SupplyOrOverflowViolation("cost overflow"))?;
        let available = self.balance_of(&actor);
        if available < required {
            return Err(AssetEconError::InsufficientBalance {
                required,
                available,
            });
        }
        self.balances.insert(actor, available - required);
        self.total_burned_fees = self
            .total_burned_fees
            .checked_add(self.policy.base_fee_burn)
            .ok_or(AssetEconError::SupplyOrOverflowViolation(
                "burned fee overflow",
            ))?;
        self.episodes.insert(
            coord,
            OccupancyTokenRecordV1 {
                owner: actor,
                mint_episode_id,
                locked_deposit: self.policy.deposit_per_occupied_block,
            },
        );
        self.verify_conservation()
    }

    /// Transfers ownership of a live occupancy-episode token without mutating `mint_episode_id`
    /// or world occupancy, burning `base_fee_burn`.
    pub fn apply_transfer_episode(
        &mut self,
        from: ActorIdV1,
        to: ActorIdV1,
        coord: Coord,
    ) -> Result<(), AssetEconError> {
        let Some(rec) = self.episodes.get(&coord).copied() else {
            return Err(AssetEconError::CoordinateNotOccupied(coord));
        };
        if rec.owner != from {
            return Err(AssetEconError::NotEpisodeOwner);
        }
        let available = self.balance_of(&from);
        if available < self.policy.base_fee_burn {
            return Err(AssetEconError::InsufficientBalance {
                required: self.policy.base_fee_burn,
                available,
            });
        }
        self.balances
            .insert(from, available - self.policy.base_fee_burn);
        self.total_burned_fees += self.policy.base_fee_burn;
        self.episodes.insert(
            coord,
            OccupancyTokenRecordV1 {
                owner: to,
                mint_episode_id: rec.mint_episode_id,
                locked_deposit: rec.locked_deposit,
            },
        );
        self.verify_conservation()
    }

    /// Burns a live occupancy-episode token owned by `actor`, burning `base_fee_burn` and
    /// refunding `locked_deposit` to `actor`.
    pub fn apply_burn(&mut self, actor: ActorIdV1, coord: Coord) -> Result<(), AssetEconError> {
        if coord == BEDROCK_COORD {
            return Err(AssetEconError::BedrockHasNoEconomicToken);
        }
        let Some(rec) = self.episodes.get(&coord).copied() else {
            return Err(AssetEconError::CoordinateNotOccupied(coord));
        };
        if rec.owner != actor {
            return Err(AssetEconError::NotEpisodeOwner);
        }
        let available = self.balance_of(&actor);
        if available < self.policy.base_fee_burn {
            return Err(AssetEconError::InsufficientBalance {
                required: self.policy.base_fee_burn,
                available,
            });
        }
        let next_bal = (available - self.policy.base_fee_burn)
            .checked_add(rec.locked_deposit)
            .ok_or(AssetEconError::SupplyOrOverflowViolation("refund overflow"))?;
        self.balances.insert(actor, next_bal);
        self.total_burned_fees += self.policy.base_fee_burn;
        self.episodes.remove(&coord);
        self.verify_conservation()
    }

    /// Charges `base_fee_burn` for an admitted transaction whose operation failed atomically
    /// without mutating world occupancy or episode ownership.
    pub fn apply_admitted_failure(&mut self, actor: ActorIdV1) -> Result<(), AssetEconError> {
        let available = self.balance_of(&actor);
        if available < self.policy.base_fee_burn {
            return Err(AssetEconError::InsufficientBalance {
                required: self.policy.base_fee_burn,
                available,
            });
        }
        self.balances
            .insert(actor, available - self.policy.base_fee_burn);
        self.total_burned_fees += self.policy.base_fee_burn;
        self.verify_conservation()
    }

    /// Executes an explicit forward governance-authorized compensating mint (never rewriting any
    /// finalized prior burn), bounded by `max_native_supply`.
    pub fn apply_forward_compensating_mint(
        &mut self,
        recipient: ActorIdV1,
        amount: u128,
    ) -> Result<(), AssetEconError> {
        let next_issued = self
            .total_issued_supply
            .checked_add(amount)
            .ok_or(AssetEconError::SupplyOrOverflowViolation("supply overflow"))?;
        if next_issued > self.policy.max_native_supply {
            return Err(AssetEconError::SupplyOrOverflowViolation(
                "compensating mint exceeds max_native_supply",
            ));
        }
        let entry = self.balances.entry(recipient).or_insert(0u128);
        *entry = (*entry)
            .checked_add(amount)
            .ok_or(AssetEconError::SupplyOrOverflowViolation(
                "balance overflow",
            ))?;
        self.total_issued_supply = next_issued;
        self.verify_conservation()
    }

    /// Verifies exact conservation: `sum(balances) + sum(locked_deposits) + burned_fees == issued`
    /// and confirms bedrock `(0, 0, 0)` holds zero economic record.
    pub fn verify_conservation(&self) -> Result<(), AssetEconError> {
        if self.episodes.contains_key(&BEDROCK_COORD) {
            return Err(AssetEconError::BedrockHasNoEconomicToken);
        }
        let mut liquid_sum = 0u128;
        for &bal in self.balances.values() {
            liquid_sum = liquid_sum
                .checked_add(bal)
                .ok_or(AssetEconError::SupplyOrOverflowViolation("liquid overflow"))?;
        }
        let mut locked_sum = 0u128;
        for rec in self.episodes.values() {
            locked_sum = locked_sum
                .checked_add(rec.locked_deposit)
                .ok_or(AssetEconError::SupplyOrOverflowViolation("locked overflow"))?;
        }
        let accounted = liquid_sum
            .checked_add(locked_sum)
            .and_then(|v| v.checked_add(self.total_burned_fees))
            .ok_or(AssetEconError::SupplyOrOverflowViolation(
                "accounting overflow",
            ))?;
        if accounted != self.total_issued_supply {
            return Err(AssetEconError::SupplyOrOverflowViolation(
                "supply conservation equation mismatch",
            ));
        }
        Ok(())
    }
}

/// Canonical pre-genesis bonded PoS validator-economic descriptor (`VALIDATOR-ECON-D1`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ValidatorEconPolicyV1 {
    /// Schema version (`1`).
    pub schema_version: u16,
    /// Minimum self-bonded native stake required for active validator eligibility (`> 0`).
    pub min_self_stake: u128,
    /// Epochs bonded stake remains locked and slashable after unbond request (`>= 2`).
    pub unbonding_delay_epochs: u64,
    /// Delayed epoch activation horizon for stake/validator-set changes (`>= 2`).
    pub activation_delay_epochs: u64,
    /// Weak-subjectivity checkpoint trust horizon in epochs (`<= unbonding_delay_epochs`).
    pub weak_subjectivity_horizon_epochs: u64,
    /// Slash penalty in basis points for proven equivocation (`1..=10_000`).
    pub equivocation_slash_bps: u16,
    /// Slash penalty in basis points for sustained inactivity (`0..=equivocation_slash_bps`).
    pub inactivity_slash_bps: u16,
    /// Single-validator concentration ceiling in basis points (`1..=3_333` for BFT safety).
    pub max_single_validator_bps: u16,
    /// Maximum native reward issuance per epoch (`>= 0`).
    pub max_epoch_reward_issuance: u128,
}

impl ValidatorEconPolicyV1 {
    /// Fixed canonical wire byte length (`64` bytes).
    pub const WIRE_LEN: usize = 64;

    /// Returns the default v1 bonded PoS validator-economic policy.
    pub const fn v1_pos_default() -> Self {
        Self {
            schema_version: 1,
            min_self_stake: 100_000_000_000,
            unbonding_delay_epochs: 21,
            activation_delay_epochs: 2,
            weak_subjectivity_horizon_epochs: 21,
            equivocation_slash_bps: 5_000,
            inactivity_slash_bps: 100,
            max_single_validator_bps: 2_500,
            max_epoch_reward_issuance: 1_000_000_000,
        }
    }

    /// Validates all bonded PoS timing, slashing, and concentration bounds.
    pub fn validate(&self) -> Result<(), AssetEconError> {
        if self.schema_version != 1 {
            return Err(AssetEconError::InvalidPolicy("schema_version must be 1"));
        }
        if self.min_self_stake == 0 {
            return Err(AssetEconError::InvalidPolicy("min_self_stake must be > 0"));
        }
        if self.activation_delay_epochs < 2
            || self.unbonding_delay_epochs < self.activation_delay_epochs
        {
            return Err(AssetEconError::InvalidPolicy(
                "requires 2 <= activation_delay_epochs <= unbonding_delay_epochs",
            ));
        }
        if self.weak_subjectivity_horizon_epochs == 0
            || self.weak_subjectivity_horizon_epochs > self.unbonding_delay_epochs
        {
            return Err(AssetEconError::InvalidPolicy(
                "weak_subjectivity_horizon_epochs must lie in 1..=unbonding_delay_epochs",
            ));
        }
        if self.equivocation_slash_bps == 0
            || self.equivocation_slash_bps > BPS_DENOMINATOR
            || self.inactivity_slash_bps > self.equivocation_slash_bps
        {
            return Err(AssetEconError::InvalidPolicy(
                "invalid equivocation/inactivity slash basis points",
            ));
        }
        if self.max_single_validator_bps == 0 || self.max_single_validator_bps > 3_333 {
            return Err(AssetEconError::InvalidPolicy(
                "max_single_validator_bps must be in 1..=3333 (< 1/3 BFT safety cap)",
            ));
        }
        Ok(())
    }

    /// Encodes the validator-economic policy into exact canonical big-endian wire bytes.
    pub fn encode_canonical(&self) -> Result<[u8; Self::WIRE_LEN], AssetEconError> {
        self.validate()?;
        let mut out = [0u8; Self::WIRE_LEN];
        out[0..2].copy_from_slice(&self.schema_version.to_be_bytes());
        out[2..18].copy_from_slice(&self.min_self_stake.to_be_bytes());
        out[18..26].copy_from_slice(&self.unbonding_delay_epochs.to_be_bytes());
        out[26..34].copy_from_slice(&self.activation_delay_epochs.to_be_bytes());
        out[34..42].copy_from_slice(&self.weak_subjectivity_horizon_epochs.to_be_bytes());
        out[42..44].copy_from_slice(&self.equivocation_slash_bps.to_be_bytes());
        out[44..46].copy_from_slice(&self.inactivity_slash_bps.to_be_bytes());
        out[46..48].copy_from_slice(&self.max_single_validator_bps.to_be_bytes());
        out[48..64].copy_from_slice(&self.max_epoch_reward_issuance.to_be_bytes());
        Ok(out)
    }

    /// Decodes and validates a canonical `ValidatorEconPolicyV1`.
    pub fn decode_canonical(bytes: &[u8]) -> Result<Self, AssetEconError> {
        if bytes.len() != Self::WIRE_LEN {
            return Err(AssetEconError::InvalidWire(
                "invalid validator econ policy length",
            ));
        }
        let policy = Self {
            schema_version: u16::from_be_bytes([bytes[0], bytes[1]]),
            min_self_stake: u128::from_be_bytes(bytes[2..18].try_into().expect("16 bytes")),
            unbonding_delay_epochs: u64::from_be_bytes(bytes[18..26].try_into().expect("8 bytes")),
            activation_delay_epochs: u64::from_be_bytes(bytes[26..34].try_into().expect("8 bytes")),
            weak_subjectivity_horizon_epochs: u64::from_be_bytes(
                bytes[34..42].try_into().expect("8 bytes"),
            ),
            equivocation_slash_bps: u16::from_be_bytes([bytes[42], bytes[43]]),
            inactivity_slash_bps: u16::from_be_bytes([bytes[44], bytes[45]]),
            max_single_validator_bps: u16::from_be_bytes([bytes[46], bytes[47]]),
            max_epoch_reward_issuance: u128::from_be_bytes(
                bytes[48..64].try_into().expect("16 bytes"),
            ),
        };
        policy.validate()?;
        Ok(policy)
    }

    /// Computes the canonical `ContentTag` for this validator-economic policy.
    pub fn tag(&self) -> Result<ContentTag, AssetEconError> {
        let bytes = self.encode_canonical()?;
        Ok(hash_canonical(
            TagKind::Policy,
            VALIDATOR_ECON_D1_POLICY_SCHEMA,
            bytes.len() as u64,
            |w| w(&bytes),
        )
        .expect("static schema and fixed length"))
    }
}

/// Deterministic bonded PoS stake/unbonding/slashing/reward state verifying supply & weight closure.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ValidatorStakeBookV1 {
    policy: ValidatorEconPolicyV1,
    bonded: BTreeMap<ActorIdV1, u128>,
    unbonding: BTreeMap<ActorIdV1, (u128, u64)>,
    total_slashed_burned: u128,
    total_rewards_issued: u128,
    total_withdrawn: u128,
    initial_total_stake: u128,
}

impl ValidatorStakeBookV1 {
    /// Initializes genesis validator stakes, enforcing `min_self_stake` and concentration ceiling.
    pub fn from_genesis(
        policy: ValidatorEconPolicyV1,
        genesis_stakes: &[(ActorIdV1, u128)],
    ) -> Result<Self, AssetEconError> {
        policy.validate()?;
        if genesis_stakes.len() < 4 {
            return Err(AssetEconError::ValidatorStakeViolation(
                "BFT PoS genesis requires at least 4 distinct validators",
            ));
        }
        let mut bonded = BTreeMap::new();
        let mut total = 0u128;
        for &(vid, stake) in genesis_stakes {
            if stake < policy.min_self_stake {
                return Err(AssetEconError::ValidatorStakeViolation(
                    "genesis stake below min_self_stake",
                ));
            }
            if bonded.insert(vid, stake).is_some() {
                return Err(AssetEconError::ValidatorStakeViolation(
                    "duplicate genesis validator",
                ));
            }
            total = total
                .checked_add(stake)
                .ok_or(AssetEconError::SupplyOrOverflowViolation("stake overflow"))?;
        }
        for &stake in bonded.values() {
            let lhs = stake
                .checked_mul(u128::from(BPS_DENOMINATOR))
                .ok_or(AssetEconError::SupplyOrOverflowViolation("mul overflow"))?;
            let rhs = total
                .checked_mul(u128::from(policy.max_single_validator_bps))
                .ok_or(AssetEconError::SupplyOrOverflowViolation("mul overflow"))?;
            if lhs > rhs {
                return Err(AssetEconError::ValidatorStakeViolation(
                    "genesis validator exceeds max_single_validator_bps",
                ));
            }
        }
        let book = Self {
            policy,
            bonded,
            unbonding: BTreeMap::new(),
            total_slashed_burned: 0,
            total_rewards_issued: 0,
            total_withdrawn: 0,
            initial_total_stake: total,
        };
        book.verify_conservation()?;
        Ok(book)
    }

    /// Initiates unbonding of `amount` for `validator` at `current_epoch`, locking funds for
    /// `unbonding_delay_epochs`.
    pub fn begin_unbond(
        &mut self,
        validator: ActorIdV1,
        amount: u128,
        current_epoch: u64,
    ) -> Result<u64, AssetEconError> {
        let Some(&cur) = self.bonded.get(&validator) else {
            return Err(AssetEconError::ValidatorStakeViolation("unknown validator"));
        };
        if amount == 0 || amount > cur {
            return Err(AssetEconError::ValidatorStakeViolation(
                "invalid unbond amount",
            ));
        }
        let remaining = cur - amount;
        if remaining != 0 && remaining < self.policy.min_self_stake {
            return Err(AssetEconError::ValidatorStakeViolation(
                "remaining stake must be 0 (full exit) or >= min_self_stake",
            ));
        }
        let unlock_epoch = current_epoch
            .checked_add(self.policy.unbonding_delay_epochs)
            .ok_or(AssetEconError::SupplyOrOverflowViolation("epoch overflow"))?;
        if remaining == 0 {
            self.bonded.remove(&validator);
        } else {
            self.bonded.insert(validator, remaining);
        }
        let entry = self.unbonding.entry(validator).or_insert((0, unlock_epoch));
        entry.0 = entry
            .0
            .checked_add(amount)
            .ok_or(AssetEconError::SupplyOrOverflowViolation("unbond overflow"))?;
        entry.1 = entry.1.max(unlock_epoch);
        self.verify_conservation()?;
        Ok(unlock_epoch)
    }

    /// Applies equivocation slashing to both bonded and unbonding stake of `validator`.
    pub fn slash_equivocation(&mut self, validator: ActorIdV1) -> Result<u128, AssetEconError> {
        let mut slashed = 0u128;
        if let Some(st) = self.bonded.get_mut(&validator) {
            let cut = (*st * u128::from(self.policy.equivocation_slash_bps))
                / u128::from(BPS_DENOMINATOR);
            *st -= cut;
            slashed += cut;
        }
        if let Some((unb, _)) = self.unbonding.get_mut(&validator) {
            let cut = (*unb * u128::from(self.policy.equivocation_slash_bps))
                / u128::from(BPS_DENOMINATOR);
            *unb -= cut;
            slashed += cut;
        }
        self.total_slashed_burned += slashed;
        self.verify_conservation()?;
        Ok(slashed)
    }

    /// Credits bounded epoch reward issuance to an active bonded validator.
    pub fn credit_epoch_reward(
        &mut self,
        validator: ActorIdV1,
        amount: u128,
    ) -> Result<(), AssetEconError> {
        if amount > self.policy.max_epoch_reward_issuance {
            return Err(AssetEconError::ValidatorStakeViolation(
                "reward exceeds max_epoch_reward_issuance",
            ));
        }
        let Some(st) = self.bonded.get_mut(&validator) else {
            return Err(AssetEconError::ValidatorStakeViolation(
                "reward target is not an active bonded validator",
            ));
        };
        *st = st
            .checked_add(amount)
            .ok_or(AssetEconError::SupplyOrOverflowViolation("reward overflow"))?;
        self.total_rewards_issued = self.total_rewards_issued.checked_add(amount).ok_or(
            AssetEconError::SupplyOrOverflowViolation("reward total overflow"),
        )?;
        self.verify_conservation()
    }

    /// Completes withdrawal of unbonded stake only once `current_epoch >= unlock_epoch`.
    pub fn claim_unbonded(
        &mut self,
        validator: ActorIdV1,
        current_epoch: u64,
    ) -> Result<u128, AssetEconError> {
        let Some(&(amt, unlock_epoch)) = self.unbonding.get(&validator) else {
            return Err(AssetEconError::ValidatorStakeViolation(
                "no unbonding stake",
            ));
        };
        if current_epoch < unlock_epoch {
            return Err(AssetEconError::ValidatorStakeViolation(
                "unbonding delay has not elapsed",
            ));
        }
        self.unbonding.remove(&validator);
        self.total_withdrawn += amt;
        self.verify_conservation()?;
        Ok(amt)
    }

    /// Verifies exact stake conservation: `bonded + unbonding + slashed + withdrawn == initial + rewards`.
    pub fn verify_conservation(&self) -> Result<(), AssetEconError> {
        let bonded_sum: u128 = self.bonded.values().copied().sum();
        let unbonding_sum: u128 = self.unbonding.values().map(|(a, _)| *a).sum();
        let lhs = bonded_sum + unbonding_sum + self.total_slashed_burned + self.total_withdrawn;
        let rhs = self.initial_total_stake + self.total_rewards_issued;
        if lhs != rhs {
            return Err(AssetEconError::SupplyOrOverflowViolation(
                "validator stake book conservation mismatch",
            ));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn asset_and_validator_econ_policies_roundtrip_and_conserve() {
        let asset_pol = AssetPolicyDescriptorV1::v1_dual_domain_default();
        let raw_a = asset_pol.encode_canonical().unwrap();
        assert_eq!(
            AssetPolicyDescriptorV1::decode_canonical(&raw_a).unwrap(),
            asset_pol
        );
        let econ_pol = ValidatorEconPolicyV1::v1_pos_default();
        let raw_e = econ_pol.encode_canonical().unwrap();
        assert_eq!(
            ValidatorEconPolicyV1::decode_canonical(&raw_e).unwrap(),
            econ_pol
        );

        let alice = ActorIdV1::from_bytes([1; 32]);
        let bob = ActorIdV1::from_bytes([2; 32]);
        let mut ledger = AssetConservationLedgerV1::from_genesis(
            asset_pol,
            &[(alice, 10_000_000), (bob, 5_000)],
        )
        .unwrap();
        assert_eq!(
            ledger.apply_mint(alice, BEDROCK_COORD, [9; 32]),
            Err(AssetEconError::BedrockHasNoEconomicToken)
        );
        let c = Coord::new(1, 0, 0);
        ledger.apply_mint(alice, c, [0xab; 32]).unwrap();
        assert_eq!(ledger.active_episode_count(), 1);
        ledger.apply_transfer_episode(alice, bob, c).unwrap();
        assert_eq!(ledger.episode_at(c).unwrap().mint_episode_id, [0xab; 32]);
        assert_eq!(
            ledger.apply_burn(alice, c),
            Err(AssetEconError::NotEpisodeOwner)
        );
        ledger.apply_burn(bob, c).unwrap();
        assert_eq!(ledger.active_episode_count(), 0);
        ledger.apply_admitted_failure(alice).unwrap();
        ledger.apply_forward_compensating_mint(bob, 50_000).unwrap();
        ledger.verify_conservation().unwrap();

        let vals: Vec<(ActorIdV1, u128)> = (1u8..=4)
            .map(|i| (ActorIdV1::from_bytes([i; 32]), 200_000_000_000))
            .collect();
        let mut book = ValidatorStakeBookV1::from_genesis(econ_pol, &vals).unwrap();
        book.credit_epoch_reward(vals[0].0, 500_000).unwrap();
        let unlock = book.begin_unbond(vals[1].0, 200_000_000_000, 10).unwrap();
        assert_eq!(unlock, 31);
        assert!(book.claim_unbonded(vals[1].0, 30).is_err());
        let slashed = book.slash_equivocation(vals[1].0).unwrap();
        assert_eq!(slashed, 100_000_000_000);
        let withdrawn = book.claim_unbonded(vals[1].0, 31).unwrap();
        assert_eq!(withdrawn, 100_000_000_000);
        book.verify_conservation().unwrap();
    }
}
