//! `PROGRAM-STATE-1`: deterministic canonical code identity, byte-availability references,
//! separately typed code and instance/storage Merkle roots, `TRANSITION-SPEC-1` profile-gated
//! activation (`program-active` wall on Launch, active on `CE-43` Successor), deterministic
//! `VM-1` / `VM-D1` execution with fail-closed atomic trap rollback, and cold restart parity
//! (`PS-048`, `wasm32`-clean, zero new Cargo dependencies).
//!
//! Bridges `VM-1` (`vm_1.rs`), `VM-D1` (`vm_d1.rs`), `LANGUAGE-R1` (`language_r1.rs`),
//! `BOUNDS-1` (`bounds_1.rs`), `PROFILE-D1` (`profile_d1.rs`), and `TRANSITION-SPEC-1`
//! (`transition_spec_1.rs`) without modifying any frozen crate or `node/`:
//! - **Canonical code identity & availability:** [`ProgramCodeRecordV1`] commits validated
//!   `SpatialOpcodeV1` byte sequences under `code_id = SHA-256("bb.prog1.code_id.v1" || ...)` and
//!   explicit `availability_digest = SHA-256("bb.prog1.avail.v1" || opcodes)`, rejecting empty,
//!   over-limit, malformed, stale-version, or unavailable code before root publication.
//! - **Separately typed roots:** [`ProgramCodeRootV1`] (`bb.prog1.code_tree.v1`) and
//!   [`ProgramInstanceRootV1`] (`bb.prog1.inst_tree.v1`) are domain-separated binary Merkle trees
//!   composed into [`ProgramStateRootsV1`] (`program_state_root` for `EXECUTION-ROOT-1` slot 6),
//!   with incremental-vs-cold full-rebuild parity and cross-domain proof rejection.
//! - **Profile-gated transition adapter:** inspects [`ResolvedProfileTransitionSpecV1`] band
//!   `80..=95` (`Program/VM`). On the Launch profile (`program-active = false`), every operation
//!   rejects fail-closed with zero state mutation; on the `CE-43` Successor profile
//!   (`program-active = true`), programs execute deterministically via `vm_1::step_opcode` with
//!   pre-execution gas charging and exact atomic rollback on any [`VmTrapV1`].
//!
//! Non-claim: committed program state does not grant shared consensus authority
//! (`PROGRAM-CONSENSUS` remains locked), prove useful contract semantics, establish network byte
//! availability, or make activation governance-safe.

use std::collections::BTreeMap;
use std::fmt;

use sha2::{Digest, Sha256};

use crate::asset_d1::AssetPolicyDescriptorV1;
use crate::bounds_1::{LocalVmBoundsV1, ProtocolBoundsProfileV1};
use crate::commit_state::DomainRegistryV1;
use crate::consensus_gov_d1::GovernancePolicyDescriptorV1;
use crate::crypto_d1::{CanonicalEd25519PublicKeyV1, CryptoSuiteDescriptorV1};
use crate::identity_d1::{CredentialRoleV1, ThresholdPolicyV1};
use crate::language_r1::{SPATIAL_GRAMMAR_VERSION, SpatialOpcodeV1};
use crate::local_bounds::LocalStorageProfile;
use crate::profile_bootstrap::ProfileScopeV1;
use crate::profile_d1::{BoundProfileManifestV1, CandidateProfileManifestV1};
use crate::protocol::{ContentTag, TagKind, hash_canonical};
use crate::resource_1::ResourcePolicyV1;
use crate::threat_1::ThreatModelV1;
use crate::transition_spec_1::ResolvedProfileTransitionSpecV1;
use crate::vm_1::step_opcode;
use crate::vm_d1::{VmSpecD1, VmTrapV1};

/// Canonical schema string for [`ProgramCodeRecordV1`].
pub const PROGRAM_STATE_1_CODE_RECORD_SCHEMA: &[u8] = b"bitblock/program-state-1/code-record/v1";
/// Canonical schema string for [`ProgramStateRootsV1`].
pub const PROGRAM_STATE_1_ROOTS_SCHEMA: &[u8] = b"bitblock/program-state-1/state-roots/v1";
/// Canonical schema string for [`ProgramExecReceiptV1`].
pub const PROGRAM_STATE_1_EXEC_RECEIPT_SCHEMA: &[u8] = b"bitblock/program-state-1/exec-receipt/v1";

/// Protocol version for `PROGRAM-STATE-1` canonical objects (`1`).
pub const PROGRAM_STATE_1_VERSION: u16 = 1;
/// Maximum canonical `SpatialOpcodeV1` instructions per registered code record (`1,024`).
pub const PROGRAM_STATE_1_MAX_OPCODES: usize = 1_024;
/// Maximum storage slots per program instance (`256`).
pub const PROGRAM_STATE_1_MAX_STORAGE_SLOTS: usize = 256;
/// Fixed gas charge for committing one persistent `(key, i64)` storage slot write (`20` gas).
pub const PROGRAM_STATE_1_STORAGE_WRITE_GAS: u64 = 20;
/// Canonical `TRANSITION-SPEC-1` operation tag for `PROGRAM-STATE-1` execution (`band 80..=95`).
pub const PROGRAM_STATE_1_EXEC_OP_TAG: u8 = 80;

const CODE_ID_PREFIX: &[u8] = b"bb.prog1.code_id.v1";
const CODE_AVAIL_PREFIX: &[u8] = b"bb.prog1.avail.v1";
const INSTANCE_RECORD_PREFIX: &[u8] = b"bb.prog1.instance.v1";
const STORAGE_MAP_PREFIX: &[u8] = b"bb.prog1.storage_map.v1";
const CODE_TREE_EMPTY_PREFIX: &[u8] = b"bb.prog1.code_tree.empty.v1";
const CODE_TREE_LEAF_PREFIX: &[u8] = b"bb.prog1.code_tree.leaf.v1";
const CODE_TREE_NODE_PREFIX: &[u8] = b"bb.prog1.code_tree.node.v1";
const INST_TREE_EMPTY_PREFIX: &[u8] = b"bb.prog1.inst_tree.empty.v1";
const INST_TREE_LEAF_PREFIX: &[u8] = b"bb.prog1.inst_tree.leaf.v1";
const INST_TREE_NODE_PREFIX: &[u8] = b"bb.prog1.inst_tree.node.v1";
const COMPOSED_STATE_ROOT_PREFIX: &[u8] = b"bb.prog1.state_root.v1";

/// Fail-closed error returned by `PROGRAM-STATE-1` codecs, proofs, and state transitions.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProgramState1Error {
    /// Canonical wire buffer had invalid length, version, opcode byte, or trailing bytes.
    InvalidWire(&'static str),
    /// Code record was empty, oversized, malformed, stale-versioned, or missing availability bytes.
    InvalidCode(&'static str),
    /// Active profile transition specification kept the `Program/VM` (`80..=95`) band inactive.
    ProfileProgramInactive,
    /// Target code record or program instance was not registered or already existed.
    MissingOrDuplicateEntity(&'static str),
    /// Gas limit was zero, exceeded `BOUNDS-1`, or failed to cover execution + storage write work.
    UnchargedOrOverBoundWork(&'static str),
    /// Proof domain tag did not match expected domain (`Code` vs `Instance`) or root failed.
    ProofVerificationFailed(&'static str),
    /// Incremental root disagreed with cold full-rebuild parity check.
    RebuildParityMismatch,
}

impl fmt::Display for ProgramState1Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}

impl std::error::Error for ProgramState1Error {}

/// Domain separator strictly isolating code commitments from program instance/storage commitments.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum ProgramTreeDomainV1 {
    /// Code registry Merkle tree (`ProgramCodeRootV1`).
    Code = 1,
    /// Program instance & storage Merkle tree (`ProgramInstanceRootV1`).
    Instance = 2,
}

impl ProgramTreeDomainV1 {
    /// Returns the canonical 1-byte domain discriminant.
    pub const fn code(self) -> u8 {
        self as u8
    }

    /// Decodes a canonical 1-byte domain discriminant.
    pub const fn from_code(code: u8) -> Result<Self, ProgramState1Error> {
        match code {
            1 => Ok(Self::Code),
            2 => Ok(Self::Instance),
            _ => Err(ProgramState1Error::InvalidWire(
                "unknown ProgramTreeDomainV1",
            )),
        }
    }
}

/// Deterministic execution disposition recorded inside [`ProgramExecReceiptV1`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum ProgramExecDispositionV1 {
    /// Program halted cleanly, charged full gas, and committed its storage write atomically.
    Admitted = 0,
    /// Program trapped deterministically (`VmTrapV1`) and rolled back storage writes atomically.
    TrappedAtomicRollback = 1,
    /// Refused before execution because the active profile has `program-active = false`.
    RejectedProfileInactive = 2,
}

impl ProgramExecDispositionV1 {
    /// Returns the canonical 1-byte wire discriminant.
    pub const fn code(self) -> u8 {
        self as u8
    }

    /// Decodes a canonical 1-byte disposition discriminant.
    pub const fn from_code(code: u8) -> Result<Self, ProgramState1Error> {
        match code {
            0 => Ok(Self::Admitted),
            1 => Ok(Self::TrappedAtomicRollback),
            2 => Ok(Self::RejectedProfileInactive),
            _ => Err(ProgramState1Error::InvalidWire(
                "unknown ProgramExecDispositionV1",
            )),
        }
    }
}

/// Decodes a raw byte into a canonical `LANGUAGE-R1` [`SpatialOpcodeV1`].
pub const fn opcode_from_byte(code: u8) -> Result<SpatialOpcodeV1, ProgramState1Error> {
    match code {
        0 => Ok(SpatialOpcodeV1::Nop),
        1 => Ok(SpatialOpcodeV1::PushOne),
        2 => Ok(SpatialOpcodeV1::PushZero),
        3 => Ok(SpatialOpcodeV1::Add),
        4 => Ok(SpatialOpcodeV1::Sub),
        5 => Ok(SpatialOpcodeV1::Dup),
        6 => Ok(SpatialOpcodeV1::Swap),
        7 => Ok(SpatialOpcodeV1::Halt),
        _ => Err(ProgramState1Error::InvalidCode(
            "malformed SpatialOpcodeV1 byte (> 7)",
        )),
    }
}

/// Computes the deterministic availability digest committing raw program opcode bytes.
pub fn compute_code_availability_digest(opcodes: &[u8]) -> [u8; 32] {
    let mut h = Sha256::new();
    h.update(CODE_AVAIL_PREFIX);
    h.update((opcodes.len() as u32).to_be_bytes());
    h.update(opcodes);
    h.finalize().into()
}

/// Computes the canonical `code_id` binding schema version, grammar version, and opcode bytes.
pub fn compute_code_id(version: u16, grammar_version: u16, opcodes: &[u8]) -> [u8; 32] {
    let avail = compute_code_availability_digest(opcodes);
    let mut h = Sha256::new();
    h.update(CODE_ID_PREFIX);
    h.update(version.to_be_bytes());
    h.update(grammar_version.to_be_bytes());
    h.update((opcodes.len() as u32).to_be_bytes());
    h.update(avail);
    h.finalize().into()
}

/// Canonical validated code artifact & byte-availability reference (`72 + N` wire bytes).
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct ProgramCodeRecordV1 {
    /// Schema version (`1`).
    pub version: u16,
    /// Bound `LANGUAGE-R1` grammar version (`1`).
    pub grammar_version: u16,
    /// Canonical `code_id` derived via [`compute_code_id`].
    pub code_id: [u8; 32],
    /// Explicit byte-availability digest derived via [`compute_code_availability_digest`].
    pub availability_digest: [u8; 32],
    /// Validated canonical `SpatialOpcodeV1` byte sequence (`1..=1,024` bytes).
    pub opcode_bytes: Vec<u8>,
}

impl ProgramCodeRecordV1 {
    /// Constructs and validates a canonical [`ProgramCodeRecordV1`] from opcode instructions.
    pub fn from_opcodes(opcodes: &[SpatialOpcodeV1]) -> Result<Self, ProgramState1Error> {
        let raw: Vec<u8> = opcodes.iter().map(|op| op.code()).collect();
        let rec = Self {
            version: PROGRAM_STATE_1_VERSION,
            grammar_version: SPATIAL_GRAMMAR_VERSION,
            code_id: compute_code_id(PROGRAM_STATE_1_VERSION, SPATIAL_GRAMMAR_VERSION, &raw),
            availability_digest: compute_code_availability_digest(&raw),
            opcode_bytes: raw,
        };
        rec.validate()?;
        Ok(rec)
    }

    /// Validates version, grammar version, bounds, opcode validity, availability digest, and `code_id`.
    pub fn validate(&self) -> Result<(), ProgramState1Error> {
        if self.version != PROGRAM_STATE_1_VERSION {
            return Err(ProgramState1Error::InvalidCode(
                "code record version must equal 1",
            ));
        }
        if self.grammar_version != SPATIAL_GRAMMAR_VERSION {
            return Err(ProgramState1Error::InvalidCode(
                "stale or unsupported grammar_version",
            ));
        }
        if self.opcode_bytes.is_empty() || self.opcode_bytes.len() > PROGRAM_STATE_1_MAX_OPCODES {
            return Err(ProgramState1Error::InvalidCode(
                "opcode_bytes must be in 1..=1024",
            ));
        }
        for &b in &self.opcode_bytes {
            let _ = opcode_from_byte(b)?;
        }
        let expected_avail = compute_code_availability_digest(&self.opcode_bytes);
        if self.availability_digest != expected_avail {
            return Err(ProgramState1Error::InvalidCode(
                "availability_digest mismatch (unavailable or tampered bytes)",
            ));
        }
        let expected_id = compute_code_id(self.version, self.grammar_version, &self.opcode_bytes);
        if self.code_id != expected_id {
            return Err(ProgramState1Error::InvalidCode("code_id mismatch"));
        }
        Ok(())
    }

    /// Decodes validated [`SpatialOpcodeV1`] instructions.
    pub fn parsed_opcodes(&self) -> Result<Vec<SpatialOpcodeV1>, ProgramState1Error> {
        self.validate()?;
        self.opcode_bytes
            .iter()
            .map(|&b| opcode_from_byte(b))
            .collect()
    }

    /// Encodes this code record into canonical big-endian wire bytes (`72 + opcode_bytes.len()`).
    pub fn encode_canonical(&self) -> Result<Vec<u8>, ProgramState1Error> {
        self.validate()?;
        let mut out = Vec::with_capacity(72 + self.opcode_bytes.len());
        out.extend_from_slice(&self.version.to_be_bytes());
        out.extend_from_slice(&self.grammar_version.to_be_bytes());
        out.extend_from_slice(&self.code_id);
        out.extend_from_slice(&self.availability_digest);
        out.extend_from_slice(&(self.opcode_bytes.len() as u32).to_be_bytes());
        out.extend_from_slice(&self.opcode_bytes);
        Ok(out)
    }

    /// Decodes and validates a canonical [`ProgramCodeRecordV1`].
    pub fn decode_canonical(bytes: &[u8]) -> Result<Self, ProgramState1Error> {
        if bytes.len() < 72 {
            return Err(ProgramState1Error::InvalidWire(
                "ProgramCodeRecordV1 truncated",
            ));
        }
        let version = u16::from_be_bytes([bytes[0], bytes[1]]);
        let grammar_version = u16::from_be_bytes([bytes[2], bytes[3]]);
        let code_id: [u8; 32] = bytes[4..36].try_into().expect("32");
        let availability_digest: [u8; 32] = bytes[36..68].try_into().expect("32");
        let len = u32::from_be_bytes(bytes[68..72].try_into().expect("4")) as usize;
        if bytes.len() != 72 + len {
            return Err(ProgramState1Error::InvalidWire(
                "ProgramCodeRecordV1 length mismatch or trailing bytes",
            ));
        }
        let rec = Self {
            version,
            grammar_version,
            code_id,
            availability_digest,
            opcode_bytes: bytes[72..].to_vec(),
        };
        rec.validate()?;
        Ok(rec)
    }

    /// Computes the canonical `Policy` [`ContentTag`] for this code record.
    pub fn tag(&self) -> Result<ContentTag, ProgramState1Error> {
        let bytes = self.encode_canonical()?;
        Ok(hash_canonical(
            TagKind::Policy,
            PROGRAM_STATE_1_CODE_RECORD_SCHEMA,
            bytes.len() as u64,
            |w| w(&bytes),
        )
        .expect("static schema and bounded length"))
    }
}

/// Deterministic program instance and its authenticated key-value `i64` storage state.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProgramInstanceV1 {
    /// Unique 32-byte program instance identifier.
    pub instance_id: [u8; 32],
    /// Referenced canonical `code_id` in the code registry.
    pub code_id: [u8; 32],
    /// Expected `LANGUAGE-R1` grammar version pinned at instantiation.
    pub grammar_version: u16,
    /// Owner `ActorIdV1` bytes.
    pub owner_actor: [u8; 32],
    /// Total admitted invocation counter.
    pub invocation_count: u64,
    /// Deterministic sorted persistent key-value storage (`<= 256` slots).
    pub storage: BTreeMap<[u8; 32], i64>,
}

impl ProgramInstanceV1 {
    /// Computes the deterministic SHA-256 storage-map commitment over sorted `(key, i64)` entries.
    pub fn storage_root(&self) -> [u8; 32] {
        let mut h = Sha256::new();
        h.update(STORAGE_MAP_PREFIX);
        h.update((self.storage.len() as u32).to_be_bytes());
        for (k, v) in &self.storage {
            h.update(k);
            h.update(v.to_be_bytes());
        }
        h.finalize().into()
    }

    /// Computes the deterministic 32-byte digest committed into `instance_root`.
    pub fn record_digest(&self) -> [u8; 32] {
        let sroot = self.storage_root();
        let mut h = Sha256::new();
        h.update(INSTANCE_RECORD_PREFIX);
        h.update(self.instance_id);
        h.update(self.code_id);
        h.update(self.grammar_version.to_be_bytes());
        h.update(self.owner_actor);
        h.update(self.invocation_count.to_be_bytes());
        h.update(sroot);
        h.finalize().into()
    }
}

/// Separately typed code root, instance root, and composed Domain-6 `program_state_root` (`106` B).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ProgramStateRootsV1 {
    /// Schema version (`1`).
    pub version: u16,
    /// Domain-separated Merkle root of registered [`ProgramCodeRecordV1`] entries.
    pub code_root: [u8; 32],
    /// Domain-separated Merkle root of [`ProgramInstanceV1`] records & storage roots.
    pub instance_root: [u8; 32],
    /// Composed Domain-6 `ProgramState` root ready for `EXECUTION-ROOT-1` slot 6.
    pub program_state_root: [u8; 32],
    /// Number of registered canonical code records.
    pub code_count: u32,
    /// Number of registered program instances.
    pub instance_count: u32,
}

impl ProgramStateRootsV1 {
    /// Exact canonical big-endian wire byte length (`106` bytes).
    pub const WIRE_LEN: usize = 2 + 32 + 32 + 32 + 4 + 4;

    /// Computes the composed `program_state_root` from `(code_root, instance_root, counts)`.
    pub fn compose_root(
        code_root: [u8; 32],
        instance_root: [u8; 32],
        code_count: u32,
        instance_count: u32,
    ) -> [u8; 32] {
        let mut h = Sha256::new();
        h.update(COMPOSED_STATE_ROOT_PREFIX);
        h.update(code_root);
        h.update(instance_root);
        h.update(code_count.to_be_bytes());
        h.update(instance_count.to_be_bytes());
        h.finalize().into()
    }

    /// Encodes this root triple into 106 canonical big-endian bytes.
    pub fn encode_canonical(&self) -> Result<[u8; Self::WIRE_LEN], ProgramState1Error> {
        if self.version != PROGRAM_STATE_1_VERSION {
            return Err(ProgramState1Error::InvalidWire(
                "roots version must equal 1",
            ));
        }
        let expected = Self::compose_root(
            self.code_root,
            self.instance_root,
            self.code_count,
            self.instance_count,
        );
        if self.program_state_root != expected {
            return Err(ProgramState1Error::InvalidWire(
                "program_state_root composition mismatch",
            ));
        }
        let mut out = [0u8; Self::WIRE_LEN];
        out[0..2].copy_from_slice(&self.version.to_be_bytes());
        out[2..34].copy_from_slice(&self.code_root);
        out[34..66].copy_from_slice(&self.instance_root);
        out[66..98].copy_from_slice(&self.program_state_root);
        out[98..102].copy_from_slice(&self.code_count.to_be_bytes());
        out[102..106].copy_from_slice(&self.instance_count.to_be_bytes());
        Ok(out)
    }

    /// Decodes and validates a 106-byte canonical [`ProgramStateRootsV1`].
    pub fn decode_canonical(bytes: &[u8]) -> Result<Self, ProgramState1Error> {
        if bytes.len() != Self::WIRE_LEN {
            return Err(ProgramState1Error::InvalidWire(
                "ProgramStateRootsV1 must be 106 bytes",
            ));
        }
        let rec = Self {
            version: u16::from_be_bytes([bytes[0], bytes[1]]),
            code_root: bytes[2..34].try_into().expect("32"),
            instance_root: bytes[34..66].try_into().expect("32"),
            program_state_root: bytes[66..98].try_into().expect("32"),
            code_count: u32::from_be_bytes(bytes[98..102].try_into().expect("4")),
            instance_count: u32::from_be_bytes(bytes[102..106].try_into().expect("4")),
        };
        let _ = rec.encode_canonical()?;
        Ok(rec)
    }

    /// Computes the canonical `Receipt` [`ContentTag`] for this roots record.
    pub fn tag(&self) -> Result<ContentTag, ProgramState1Error> {
        let bytes = self.encode_canonical()?;
        Ok(hash_canonical(
            TagKind::Receipt,
            PROGRAM_STATE_1_ROOTS_SCHEMA,
            bytes.len() as u64,
            |w| w(&bytes),
        )
        .expect("static schema and fixed length"))
    }
}

/// Deterministic activation/execution/trap receipt emitted per program invocation (`156` bytes).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ProgramExecReceiptV1 {
    /// Schema version (`1`).
    pub version: u16,
    /// Execution disposition (`Admitted`, `TrappedAtomicRollback`, `RejectedProfileInactive`).
    pub disposition: ProgramExecDispositionV1,
    /// `VM-D1` trap status code (`0` when admitted or profile-rejected; `1..=7` on trap).
    pub trap_status_code: u8,
    /// Target program `instance_id`.
    pub instance_id: [u8; 32],
    /// Executed canonical `code_id`.
    pub code_id: [u8; 32],
    /// Storage slot key targeted by this invocation.
    pub storage_key: [u8; 32],
    /// Written `i64` top-of-stack value (`0` when rolled back or profile-rejected).
    pub written_value: i64,
    /// Deterministic metered gas consumed (opcodes + storage write when admitted).
    pub gas_used: u64,
    /// Gas limit enforced for this invocation.
    pub gas_limit: u64,
    /// Pre-transition composed `program_state_root`.
    pub pre_state_root: [u8; 32],
    /// Post-transition composed `program_state_root` (equals `pre_state_root` on rollback/reject).
    pub post_state_root: [u8; 32],
}

impl ProgramExecReceiptV1 {
    /// Exact canonical big-endian wire byte length (`188` bytes).
    pub const WIRE_LEN: usize = 2 + 1 + 1 + 32 + 32 + 32 + 8 + 8 + 8 + 32 + 32;

    /// Encodes this execution receipt into 188 canonical big-endian bytes.
    pub fn encode_canonical(&self) -> Result<[u8; Self::WIRE_LEN], ProgramState1Error> {
        if self.version != PROGRAM_STATE_1_VERSION {
            return Err(ProgramState1Error::InvalidWire(
                "exec receipt version must equal 1",
            ));
        }
        let mut out = [0u8; Self::WIRE_LEN];
        out[0..2].copy_from_slice(&self.version.to_be_bytes());
        out[2] = self.disposition.code();
        out[3] = self.trap_status_code;
        out[4..36].copy_from_slice(&self.instance_id);
        out[36..68].copy_from_slice(&self.code_id);
        out[68..100].copy_from_slice(&self.storage_key);
        out[100..108].copy_from_slice(&self.written_value.to_be_bytes());
        out[108..116].copy_from_slice(&self.gas_used.to_be_bytes());
        out[116..124].copy_from_slice(&self.gas_limit.to_be_bytes());
        out[124..156].copy_from_slice(&self.pre_state_root);
        out[156..188].copy_from_slice(&self.post_state_root);
        Ok(out)
    }

    /// Decodes a 188-byte canonical [`ProgramExecReceiptV1`].
    pub fn decode_canonical(bytes: &[u8]) -> Result<Self, ProgramState1Error> {
        if bytes.len() != Self::WIRE_LEN {
            return Err(ProgramState1Error::InvalidWire(
                "ProgramExecReceiptV1 must be 188 bytes",
            ));
        }
        let version = u16::from_be_bytes([bytes[0], bytes[1]]);
        if version != PROGRAM_STATE_1_VERSION {
            return Err(ProgramState1Error::InvalidWire(
                "exec receipt version must equal 1",
            ));
        }
        Ok(Self {
            version,
            disposition: ProgramExecDispositionV1::from_code(bytes[2])?,
            trap_status_code: bytes[3],
            instance_id: bytes[4..36].try_into().expect("32"),
            code_id: bytes[36..68].try_into().expect("32"),
            storage_key: bytes[68..100].try_into().expect("32"),
            written_value: i64::from_be_bytes(bytes[100..108].try_into().expect("8")),
            gas_used: u64::from_be_bytes(bytes[108..116].try_into().expect("8")),
            gas_limit: u64::from_be_bytes(bytes[116..124].try_into().expect("8")),
            pre_state_root: bytes[124..156].try_into().expect("32"),
            post_state_root: bytes[156..188].try_into().expect("32"),
        })
    }

    /// Computes the canonical `Receipt` [`ContentTag`] for this execution receipt.
    pub fn tag(&self) -> Result<ContentTag, ProgramState1Error> {
        let bytes = self.encode_canonical()?;
        Ok(hash_canonical(
            TagKind::Receipt,
            PROGRAM_STATE_1_EXEC_RECEIPT_SCHEMA,
            bytes.len() as u64,
            |w| w(&bytes),
        )
        .expect("static schema and fixed length"))
    }
}

/// Domain-separated binary Merkle inclusion proof for either `Code` or `Instance` roots.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProgramMerkleProofV1 {
    /// Strict domain tag (`Code` vs `Instance`).
    pub domain: ProgramTreeDomainV1,
    /// Proven 32-byte leaf key (`code_id` or `instance_id`).
    pub key: [u8; 32],
    /// Proven 32-byte value digest (`code_record_tag_digest` or `instance_record_digest`).
    pub value_digest: [u8; 32],
    /// Bottom-up sibling hashes (`(sibling_hash, sibling_is_right)`).
    pub siblings: Vec<([u8; 32], bool)>,
}

impl ProgramMerkleProofV1 {
    /// Verifies this proof strictly against `expected_domain` and `expected_root`.
    pub fn verify(
        &self,
        expected_domain: ProgramTreeDomainV1,
        expected_root: [u8; 32],
    ) -> Result<(), ProgramState1Error> {
        if self.domain != expected_domain {
            return Err(ProgramState1Error::ProofVerificationFailed(
                "cross-domain program proof rejected",
            ));
        }
        let mut acc = hash_tree_leaf(expected_domain, self.key, self.value_digest);
        for &(sib, sib_is_right) in &self.siblings {
            acc = if sib_is_right {
                hash_tree_node(expected_domain, acc, sib)
            } else {
                hash_tree_node(expected_domain, sib, acc)
            };
        }
        if acc != expected_root {
            return Err(ProgramState1Error::ProofVerificationFailed(
                "merkle root mismatch",
            ));
        }
        Ok(())
    }
}

fn hash_tree_empty(domain: ProgramTreeDomainV1) -> [u8; 32] {
    let mut h = Sha256::new();
    match domain {
        ProgramTreeDomainV1::Code => h.update(CODE_TREE_EMPTY_PREFIX),
        ProgramTreeDomainV1::Instance => h.update(INST_TREE_EMPTY_PREFIX),
    }
    h.finalize().into()
}

fn hash_tree_leaf(domain: ProgramTreeDomainV1, key: [u8; 32], val_digest: [u8; 32]) -> [u8; 32] {
    let mut h = Sha256::new();
    match domain {
        ProgramTreeDomainV1::Code => h.update(CODE_TREE_LEAF_PREFIX),
        ProgramTreeDomainV1::Instance => h.update(INST_TREE_LEAF_PREFIX),
    }
    h.update(key);
    h.update(val_digest);
    h.finalize().into()
}

fn hash_tree_node(domain: ProgramTreeDomainV1, left: [u8; 32], right: [u8; 32]) -> [u8; 32] {
    let mut h = Sha256::new();
    match domain {
        ProgramTreeDomainV1::Code => h.update(CODE_TREE_NODE_PREFIX),
        ProgramTreeDomainV1::Instance => h.update(INST_TREE_NODE_PREFIX),
    }
    h.update(left);
    h.update(right);
    h.finalize().into()
}

fn next_merkle_level(domain: ProgramTreeDomainV1, level: &[[u8; 32]]) -> Vec<[u8; 32]> {
    let mut next = Vec::with_capacity(level.len().div_ceil(2));
    let mut i = 0;
    while i < level.len() {
        let left = level[i];
        let right = if i + 1 < level.len() {
            level[i + 1]
        } else {
            left
        };
        next.push(hash_tree_node(domain, left, right));
        i += 2;
    }
    next
}

fn build_sorted_merkle_root(
    domain: ProgramTreeDomainV1,
    entries: &[([u8; 32], [u8; 32])],
) -> [u8; 32] {
    if entries.is_empty() {
        return hash_tree_empty(domain);
    }
    let mut level: Vec<[u8; 32]> = entries
        .iter()
        .map(|&(k, v)| hash_tree_leaf(domain, k, v))
        .collect();
    while level.len() > 1 {
        level = next_merkle_level(domain, &level);
    }
    level[0]
}

fn prove_sorted_merkle(
    domain: ProgramTreeDomainV1,
    entries: &[([u8; 32], [u8; 32])],
    target_key: [u8; 32],
) -> Result<ProgramMerkleProofV1, ProgramState1Error> {
    let mut idx = entries.iter().position(|&(k, _)| k == target_key).ok_or(
        ProgramState1Error::MissingOrDuplicateEntity("key absent from program tree"),
    )?;
    let value_digest = entries[idx].1;
    let mut level: Vec<[u8; 32]> = entries
        .iter()
        .map(|&(k, v)| hash_tree_leaf(domain, k, v))
        .collect();
    let mut siblings = Vec::new();
    while level.len() > 1 {
        let sib_idx = if idx % 2 == 0 {
            if idx + 1 < level.len() { idx + 1 } else { idx }
        } else {
            idx - 1
        };
        let sib_is_right = idx % 2 == 0;
        siblings.push((level[sib_idx], sib_is_right));
        level = next_merkle_level(domain, &level);
        idx /= 2;
    }
    Ok(ProgramMerkleProofV1 {
        domain,
        key: target_key,
        value_digest,
        siblings,
    })
}

/// Checks whether `spec`'s `Program/VM` band (`80..=95`) is active in the resolved profile.
pub fn profile_permits_program_state(spec: &ResolvedProfileTransitionSpecV1) -> bool {
    spec.program_slot_active
        && spec.bands.iter().any(|b| {
            b.op_tag_min <= PROGRAM_STATE_1_EXEC_OP_TAG
                && PROGRAM_STATE_1_EXEC_OP_TAG <= b.op_tag_max
        })
}

/// Builds the canonical `(launch_spec, successor_spec)` pair over `PROFILE-D1` and
/// `TRANSITION-SPEC-1` (`launch_spec` keeps `program_slot_active == false`; `successor_spec`
/// enables `CE-43` `program_slot_active == true`).
pub fn canonical_program_state_profile_specs() -> Result<
    (
        ResolvedProfileTransitionSpecV1,
        ResolvedProfileTransitionSpecV1,
    ),
    ProgramState1Error,
> {
    let mut council_keys = Vec::with_capacity(3);
    for s in [31u8, 32, 33] {
        council_keys.push(
            CanonicalEd25519PublicKeyV1::from_secret_seed(&[s; 32])
                .map_err(|_| ProgramState1Error::InvalidCode("council key"))?,
        );
    }
    council_keys.sort();
    let emergency_council = ThresholdPolicyV1 {
        role: CredentialRoleV1::EmergencyPauseSigner,
        threshold: 2,
        keys: council_keys,
    };
    let gov_desc = GovernancePolicyDescriptorV1 {
        emergency_council,
        schema_version: 1,
        upgrade_quorum_numerator: 2,
        upgrade_quorum_denominator: 3,
        mandatory_exit_timelock_epochs: 21,
        max_emergency_pause_epochs: 3,
        automatic_safe_halt_on_proven_fault: true,
        emergency_council_pause_only: true,
        forbid_history_rewrite: true,
    };
    let active_preds = vec![
        "asset-active".to_string(),
        "economic-charging".to_string(),
        "stake-consensus".to_string(),
        "validator-economics".to_string(),
    ];
    let scope = ProfileScopeV1::from_wave1_contracts(
        &DomainRegistryV1::local_v1(),
        &gov_desc,
        active_preds,
    )
    .map_err(|_| ProgramState1Error::InvalidCode("scope"))?;
    let threat = ThreatModelV1::v1_conservative_consumer(&scope)
        .map_err(|_| ProgramState1Error::InvalidCode("threat"))?;
    let bounds =
        ProtocolBoundsProfileV1::v1_consumer_default(&threat, &LocalStorageProfile::DEFAULT)
            .map_err(|_| ProgramState1Error::InvalidCode("bounds"))?;
    let resource =
        ResourcePolicyV1::v1_default(&bounds, &AssetPolicyDescriptorV1::v1_dual_domain_default())
            .map_err(|_| ProgramState1Error::InvalidCode("resource"))?;
    let crypto_d = CryptoSuiteDescriptorV1::v1_ed25519_sha256()
        .tag()
        .map_err(|_| ProgramState1Error::InvalidCode("crypto"))?
        .digest();
    let genesis_d = ContentTag::parse_full(
        "#bb.policy.sha256.jowjx6wyiayipiaypxgmjc2xgynt335d7mrx3qfx2e6pxegyshgq",
    )
    .map_err(|_| ProgramState1Error::InvalidCode("genesis"))?
    .digest();
    let launch_cand = CandidateProfileManifestV1::canonical_launch_v1(
        genesis_d,
        [0x21; 32],
        [0x31; 32],
        bounds
            .tag()
            .map_err(|_| ProgramState1Error::InvalidCode("bounds tag"))?
            .digest(),
        crypto_d,
        resource
            .tag()
            .map_err(|_| ProgramState1Error::InvalidCode("res tag"))?
            .digest(),
    )
    .map_err(|_| ProgramState1Error::InvalidCode("launch cand"))?;
    let launch_bound = BoundProfileManifestV1::seal(launch_cand)
        .map_err(|_| ProgramState1Error::InvalidCode("launch seal"))?;
    let succ_cand =
        CandidateProfileManifestV1::canonical_successor_program_v1(&launch_bound, [0x71; 32])
            .map_err(|_| ProgramState1Error::InvalidCode("succ cand"))?;
    let succ_bound = BoundProfileManifestV1::seal(succ_cand)
        .map_err(|_| ProgramState1Error::InvalidCode("succ seal"))?;
    let launch_spec =
        ResolvedProfileTransitionSpecV1::resolve_from_profile(&launch_bound, &bounds, &resource)
            .map_err(|_| ProgramState1Error::InvalidCode("launch spec"))?;
    let succ_spec =
        ResolvedProfileTransitionSpecV1::resolve_from_profile(&succ_bound, &bounds, &resource)
            .map_err(|_| ProgramState1Error::InvalidCode("succ spec"))?;
    Ok((launch_spec, succ_spec))
}

/// Deterministic authenticated `PROGRAM-STATE-1` engine maintaining separately typed code and
/// instance/storage roots with full-rebuild parity and fail-closed `VM-1` atomic execution.
#[derive(Clone, Debug)]
pub struct ProgramStateEngineV1 {
    vm_spec: VmSpecD1,
    vm_bounds: LocalVmBoundsV1,
    codes: BTreeMap<[u8; 32], ProgramCodeRecordV1>,
    instances: BTreeMap<[u8; 32], ProgramInstanceV1>,
    cached_roots: ProgramStateRootsV1,
}

impl ProgramStateEngineV1 {
    /// Creates an empty `PROGRAM-STATE-1` state machine under `vm_spec` and `vm_bounds`.
    pub fn empty(
        vm_spec: VmSpecD1,
        vm_bounds: LocalVmBoundsV1,
    ) -> Result<Self, ProgramState1Error> {
        vm_spec
            .validate()
            .map_err(|_| ProgramState1Error::InvalidCode("invalid VmSpecD1"))?;
        vm_bounds
            .validate(Some(&vm_spec))
            .map_err(|_| ProgramState1Error::UnchargedOrOverBoundWork("invalid LocalVmBoundsV1"))?;
        let code_root = hash_tree_empty(ProgramTreeDomainV1::Code);
        let instance_root = hash_tree_empty(ProgramTreeDomainV1::Instance);
        let cached_roots = ProgramStateRootsV1 {
            version: PROGRAM_STATE_1_VERSION,
            code_root,
            instance_root,
            program_state_root: ProgramStateRootsV1::compose_root(code_root, instance_root, 0, 0),
            code_count: 0,
            instance_count: 0,
        };
        Ok(Self {
            vm_spec,
            vm_bounds,
            codes: BTreeMap::new(),
            instances: BTreeMap::new(),
            cached_roots,
        })
    }

    /// Returns the current committed [`ProgramStateRootsV1`].
    pub const fn roots(&self) -> ProgramStateRootsV1 {
        self.cached_roots
    }

    /// Returns a reference to a registered [`ProgramInstanceV1`], if present.
    pub fn instance(&self, instance_id: &[u8; 32]) -> Option<&ProgramInstanceV1> {
        self.instances.get(instance_id)
    }

    /// Recomputes `code_root`, `instance_root`, and `program_state_root` cold from scratch and
    /// asserts bit-for-bit equality with the incrementally maintained roots.
    pub fn verify_rebuild_parity(&self) -> Result<ProgramStateRootsV1, ProgramState1Error> {
        let rebuilt = self.compute_roots_cold()?;
        if rebuilt != self.cached_roots {
            return Err(ProgramState1Error::RebuildParityMismatch);
        }
        Ok(rebuilt)
    }

    fn compute_roots_cold(&self) -> Result<ProgramStateRootsV1, ProgramState1Error> {
        let mut code_entries = Vec::with_capacity(self.codes.len());
        for (&id, rec) in &self.codes {
            code_entries.push((id, rec.tag()?.digest()));
        }
        let inst_entries: Vec<([u8; 32], [u8; 32])> = self
            .instances
            .iter()
            .map(|(&id, inst)| (id, inst.record_digest()))
            .collect();
        let code_root = build_sorted_merkle_root(ProgramTreeDomainV1::Code, &code_entries);
        let instance_root = build_sorted_merkle_root(ProgramTreeDomainV1::Instance, &inst_entries);
        let code_count = self.codes.len() as u32;
        let instance_count = self.instances.len() as u32;
        Ok(ProgramStateRootsV1 {
            version: PROGRAM_STATE_1_VERSION,
            code_root,
            instance_root,
            program_state_root: ProgramStateRootsV1::compose_root(
                code_root,
                instance_root,
                code_count,
                instance_count,
            ),
            code_count,
            instance_count,
        })
    }

    /// Registers a validated [`ProgramCodeRecordV1`] gated by `profile_spec`.
    pub fn register_code(
        &mut self,
        profile_spec: &ResolvedProfileTransitionSpecV1,
        record: ProgramCodeRecordV1,
    ) -> Result<[u8; 32], ProgramState1Error> {
        if !profile_permits_program_state(profile_spec) {
            return Err(ProgramState1Error::ProfileProgramInactive);
        }
        record.validate()?;
        if self.codes.contains_key(&record.code_id) {
            return Err(ProgramState1Error::MissingOrDuplicateEntity(
                "code_id already registered",
            ));
        }
        let id = record.code_id;
        self.codes.insert(id, record);
        self.cached_roots = self.compute_roots_cold()?;
        Ok(id)
    }

    /// Instantiates a registered `code_id` under `instance_id` and `owner_actor`, gated by `profile_spec`.
    pub fn instantiate_program(
        &mut self,
        profile_spec: &ResolvedProfileTransitionSpecV1,
        instance_id: [u8; 32],
        code_id: [u8; 32],
        owner_actor: [u8; 32],
    ) -> Result<ProgramStateRootsV1, ProgramState1Error> {
        if !profile_permits_program_state(profile_spec) {
            return Err(ProgramState1Error::ProfileProgramInactive);
        }
        let code = self
            .codes
            .get(&code_id)
            .ok_or(ProgramState1Error::MissingOrDuplicateEntity(
                "code_id not registered",
            ))?;
        if self.instances.contains_key(&instance_id) {
            return Err(ProgramState1Error::MissingOrDuplicateEntity(
                "instance_id already exists",
            ));
        }
        self.instances.insert(
            instance_id,
            ProgramInstanceV1 {
                instance_id,
                code_id,
                grammar_version: code.grammar_version,
                owner_actor,
                invocation_count: 0,
                storage: BTreeMap::new(),
            },
        );
        self.cached_roots = self.compute_roots_cold()?;
        Ok(self.cached_roots)
    }

    /// Executes one registered program instance against `storage_key` under `gas_limit`, gated by
    /// `profile_spec`. Rolls back storage atomically on any `VmTrapV1` or profile refusal.
    pub fn execute_instance(
        &mut self,
        profile_spec: &ResolvedProfileTransitionSpecV1,
        instance_id: [u8; 32],
        storage_key: [u8; 32],
        gas_limit: u64,
    ) -> Result<ProgramExecReceiptV1, ProgramState1Error> {
        let pre_state_root = self.cached_roots.program_state_root;
        if !profile_permits_program_state(profile_spec) {
            return Ok(ProgramExecReceiptV1 {
                version: PROGRAM_STATE_1_VERSION,
                disposition: ProgramExecDispositionV1::RejectedProfileInactive,
                trap_status_code: 0,
                instance_id,
                code_id: [0u8; 32],
                storage_key,
                written_value: 0,
                gas_used: 0,
                gas_limit,
                pre_state_root,
                post_state_root: pre_state_root,
            });
        }
        if gas_limit == 0 || gas_limit > self.vm_bounds.max_gas_per_program {
            return Err(ProgramState1Error::UnchargedOrOverBoundWork(
                "gas_limit must be in 1..=max_gas_per_program",
            ));
        }
        let inst_snapshot = self
            .instances
            .get(&instance_id)
            .ok_or(ProgramState1Error::MissingOrDuplicateEntity(
                "instance_id not registered",
            ))?
            .clone();
        let code = self
            .codes
            .get(&inst_snapshot.code_id)
            .ok_or(ProgramState1Error::MissingOrDuplicateEntity(
                "code_id missing for instance",
            ))?
            .clone();
        let opcodes = code.parsed_opcodes()?;
        if code.grammar_version != inst_snapshot.grammar_version {
            return Err(ProgramState1Error::InvalidCode(
                "instance grammar_version stale vs code record",
            ));
        }

        let max_depth = usize::from(
            self.vm_spec
                .max_stack_depth
                .min(self.vm_bounds.max_stack_depth),
        );
        let mut stack: Vec<i64> = Vec::new();
        let mut gas_used = 0u64;
        let mut halted_ok = false;
        let mut trap_opt: Option<VmTrapV1> = None;

        for &op in &opcodes {
            let step_gas = self.vm_spec.gas_for_opcode(op);
            let next_gas = gas_used.saturating_add(step_gas);
            if next_gas > gas_limit {
                trap_opt = Some(VmTrapV1::GasExhausted);
                break;
            }
            gas_used = next_gas;
            match step_opcode(&mut stack, op, max_depth) {
                Ok(true) => {
                    halted_ok = true;
                    break;
                }
                Ok(false) => {}
                Err(trap) => {
                    trap_opt = Some(trap);
                    break;
                }
            }
        }
        if trap_opt.is_none() && !halted_ok {
            trap_opt = Some(VmTrapV1::MissingHalt);
        }

        // Charge explicit storage-write gas before committing; trap and roll back if insufficient.
        let mut written_value = 0i64;
        if trap_opt.is_none() {
            let Some(&top) = stack.last() else {
                trap_opt = Some(VmTrapV1::StackUnderflow);
                return Ok(ProgramExecReceiptV1 {
                    version: PROGRAM_STATE_1_VERSION,
                    disposition: ProgramExecDispositionV1::TrappedAtomicRollback,
                    trap_status_code: trap_opt.expect("some").status_code(),
                    instance_id,
                    code_id: code.code_id,
                    storage_key,
                    written_value: 0,
                    gas_used,
                    gas_limit,
                    pre_state_root,
                    post_state_root: pre_state_root,
                });
            };
            let next_gas = gas_used.saturating_add(PROGRAM_STATE_1_STORAGE_WRITE_GAS);
            let slots_full = !inst_snapshot.storage.contains_key(&storage_key)
                && inst_snapshot.storage.len() >= PROGRAM_STATE_1_MAX_STORAGE_SLOTS;
            if next_gas > gas_limit || slots_full {
                trap_opt = Some(VmTrapV1::GasExhausted);
            } else {
                gas_used = next_gas;
                written_value = top;
            }
        }

        if let Some(trap) = trap_opt {
            // Atomic rollback: zero storage or root mutations on any trap.
            return Ok(ProgramExecReceiptV1 {
                version: PROGRAM_STATE_1_VERSION,
                disposition: ProgramExecDispositionV1::TrappedAtomicRollback,
                trap_status_code: trap.status_code(),
                instance_id,
                code_id: code.code_id,
                storage_key,
                written_value: 0,
                gas_used,
                gas_limit,
                pre_state_root,
                post_state_root: pre_state_root,
            });
        }

        // Commit storage write + invocation counter atomically.
        let mut next_inst = inst_snapshot;
        next_inst.storage.insert(storage_key, written_value);
        next_inst.invocation_count = next_inst.invocation_count.saturating_add(1);
        self.instances.insert(instance_id, next_inst);
        self.cached_roots = self.compute_roots_cold()?;

        Ok(ProgramExecReceiptV1 {
            version: PROGRAM_STATE_1_VERSION,
            disposition: ProgramExecDispositionV1::Admitted,
            trap_status_code: 0,
            instance_id,
            code_id: code.code_id,
            storage_key,
            written_value,
            gas_used,
            gas_limit,
            pre_state_root,
            post_state_root: self.cached_roots.program_state_root,
        })
    }

    /// Produces a domain-separated inclusion proof for a registered `code_id`.
    pub fn prove_code(
        &self,
        code_id: [u8; 32],
    ) -> Result<ProgramMerkleProofV1, ProgramState1Error> {
        let mut entries = Vec::with_capacity(self.codes.len());
        for (&id, rec) in &self.codes {
            entries.push((id, rec.tag()?.digest()));
        }
        prove_sorted_merkle(ProgramTreeDomainV1::Code, &entries, code_id)
    }

    /// Produces a domain-separated inclusion proof for a registered `instance_id`.
    pub fn prove_instance(
        &self,
        instance_id: [u8; 32],
    ) -> Result<ProgramMerkleProofV1, ProgramState1Error> {
        let entries: Vec<([u8; 32], [u8; 32])> = self
            .instances
            .iter()
            .map(|(&id, inst)| (id, inst.record_digest()))
            .collect();
        prove_sorted_merkle(ProgramTreeDomainV1::Instance, &entries, instance_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn program_state_1_canonical_roundtrip_and_golden_tags() {
        let vm_spec = VmSpecD1::v1_default();
        let vm_bounds = LocalVmBoundsV1::v1_default(&vm_spec).expect("bounds");
        let (launch_spec, succ_spec) = canonical_program_state_profile_specs().expect("specs");

        assert!(!profile_permits_program_state(&launch_spec));
        assert!(profile_permits_program_state(&succ_spec));

        // Program: PushOne, Dup, Add, Dup, Add, Halt => (1+1)+(1+1) = 4
        let code = ProgramCodeRecordV1::from_opcodes(&[
            SpatialOpcodeV1::PushOne,
            SpatialOpcodeV1::Dup,
            SpatialOpcodeV1::Add,
            SpatialOpcodeV1::Dup,
            SpatialOpcodeV1::Add,
            SpatialOpcodeV1::Halt,
        ])
        .expect("code");
        let code_wire = code.encode_canonical().expect("enc");
        assert_eq!(
            ProgramCodeRecordV1::decode_canonical(&code_wire).expect("dec"),
            code
        );

        let mut eng = ProgramStateEngineV1::empty(vm_spec, vm_bounds).expect("empty");
        let code_id = eng.register_code(&succ_spec, code.clone()).expect("reg");
        let inst_id = [0x11u8; 32];
        let owner = [0x22u8; 32];
        let slot_key = [0x33u8; 32];
        eng.instantiate_program(&succ_spec, inst_id, code_id, owner)
            .expect("inst");
        let receipt = eng
            .execute_instance(&succ_spec, inst_id, slot_key, 200)
            .expect("exec");
        assert_eq!(receipt.disposition, ProgramExecDispositionV1::Admitted);
        assert_eq!(receipt.written_value, 4);
        assert_eq!(receipt.gas_used, 33); // 2+2+3+2+3+1 = 13 opcode gas + 20 storage write = 33
        assert_eq!(eng.verify_rebuild_parity().expect("parity"), eng.roots());

        let roots_wire = eng.roots().encode_canonical().expect("roots enc");
        assert_eq!(
            ProgramStateRootsV1::decode_canonical(&roots_wire).expect("roots dec"),
            eng.roots()
        );
        let rec_wire = receipt.encode_canonical().expect("rec enc");
        assert_eq!(
            ProgramExecReceiptV1::decode_canonical(&rec_wire).expect("rec dec"),
            receipt
        );

        assert_eq!(
            code.tag().expect("ctag").to_string(),
            "#bb.policy.sha256.qmmm3akwhwhy42jblombezj6chppxzdfxzrtualsrmone6i3xtfa"
        );
        assert_eq!(
            eng.roots().tag().expect("rtag").to_string(),
            "#bb.receipt.sha256.gbzdt7lyv2ej5ewweyqw3xf6bce4pghgs5vqzs4x5h4piln2lj4q"
        );
        assert_eq!(
            receipt.tag().expect("etag").to_string(),
            "#bb.receipt.sha256.4pvmwu57ahsryqvsiyox5qwgsfnljtdcjx752cynhpag4y2zbqla"
        );
    }
}
