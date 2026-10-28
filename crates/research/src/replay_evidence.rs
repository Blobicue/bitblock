//! Canonical local evidence for a bounded, unsigned interaction replay.
//!
//! A [`LocalReplayEvidence`] carrier embeds the complete canonical [`InteractionTape`] rather
//! than merely naming it. It binds that accepted-action sequence, the declared fixed-headless
//! policies, the bytes of the executable that emitted the carrier, and one exact endpoint.
//!
//! It is deliberately **local replay evidence only**. It is not a signature, persistence or
//! custody record, proof of work/history/stake, consensus vote, ownership record, Layer 1, or
//! claim about wall-clock time, a window, presentation, scanout, or physical light.

use std::fmt;
use std::io::{self, Read};

use sha2::{Digest, Sha256};

use crate::interaction::{
    InteractionTape, InteractionTapeCodecError, MAX_INTERACTION_TAPE_PAYLOAD_BYTES,
};
use crate::protocol::{ContentTag, TagError, TagKind, hash_canonical};

/// Schema bound into one fingerprint of the locally executed artifact bytes.
pub const LOCAL_REPLAY_BUILD_SCHEMA: &[u8] = b"bitblock/local-replay-build/executable-sha256/v1";
/// Schema bound into every canonical local replay-evidence carrier.
pub const LOCAL_REPLAY_EVIDENCE_SCHEMA: &[u8] = b"bitblock/local-replay-evidence/v1";

const BUILD_CANONICAL_BYTES: usize = size_of::<u64>() + 32;
const TAG_BYTES: usize = 32;
const CAMERA_BITS_LEN: usize = 10;
const OPTIONAL_TAG_BYTES: usize = 1 + TAG_BYTES;
const OPTIONAL_FRAME_BYTES: usize = 1 + 2 * TAG_BYTES;
const POLICY_COUNT: usize = 3;
const TAPE_LEN_OFFSET: usize = BUILD_CANONICAL_BYTES + POLICY_COUNT * TAG_BYTES;

/// Bytes outside the embedded canonical interaction-tape payload.
///
/// The field order is executable fingerprint, three policy digests, tape byte length, optional
/// final tick outcome, final world/history, ten camera lanes, four dimensions, sidebar state,
/// then optional latest game and composite frame commitments.
pub const LOCAL_REPLAY_EVIDENCE_FIXED_BYTES: usize = BUILD_CANONICAL_BYTES
    + POLICY_COUNT * TAG_BYTES
    + size_of::<u64>()
    + OPTIONAL_TAG_BYTES
    + 2 * TAG_BYTES
    + CAMERA_BITS_LEN * size_of::<u64>()
    + 4 * size_of::<u32>()
    + 1
    + 2 * OPTIONAL_FRAME_BYTES;

/// A streamed SHA-256 fingerprint of the exact local executable bytes used for a replay.
///
/// This names bytes available to the local process. It does not identify source, compiler,
/// operating system, custody, a trusted build, or behavior on another host.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LocalReplayBuild {
    artifact_len: u64,
    sha256: [u8; TAG_BYTES],
}

impl LocalReplayBuild {
    /// Constructs a `LocalReplayBuild` from a byte length and SHA-256 digest.
    pub const fn from_parts(artifact_len: u64, sha256: [u8; TAG_BYTES]) -> Self {
        Self {
            artifact_len,
            sha256,
        }
    }

    /// Streams exactly `expected_len` artifact bytes and rejects a short or extended stream.
    ///
    /// Callers should obtain `expected_len` from a stable metadata observation and recheck that
    /// metadata after this function returns. The function itself refuses a change visible as a
    /// short/long byte stream, but does not claim filesystem custody or race-free identity.
    pub fn fingerprint_reader<R: Read>(
        mut reader: R,
        expected_len: u64,
        maximum_len: u64,
    ) -> Result<Self, LocalReplayBuildError> {
        if expected_len > maximum_len {
            return Err(LocalReplayBuildError::LengthLimit {
                actual: expected_len,
                maximum: maximum_len,
            });
        }

        let mut remaining = expected_len;
        let mut hasher = Sha256::new();
        let mut buffer = [0_u8; 64 * 1024];
        while remaining != 0 {
            let requested = usize::try_from(remaining.min(buffer.len() as u64))
                .expect("bounded read request fits usize");
            let read = reader
                .read(&mut buffer[..requested])
                .map_err(LocalReplayBuildError::Read)?;
            if read == 0 {
                return Err(LocalReplayBuildError::ShortRead {
                    expected: expected_len,
                    observed: expected_len - remaining,
                });
            }
            hasher.update(&buffer[..read]);
            remaining -= read as u64;
        }

        let mut extra = [0_u8; 1];
        if reader
            .read(&mut extra)
            .map_err(LocalReplayBuildError::Read)?
            != 0
        {
            return Err(LocalReplayBuildError::TrailingBytes {
                expected: expected_len,
            });
        }

        Ok(Self::from_parts(expected_len, hasher.finalize().into()))
    }

    /// Returns the declared byte length of the fingerprinted executable artifact.
    pub const fn artifact_len(self) -> u64 {
        self.artifact_len
    }

    /// Returns the 32-byte SHA-256 digest of the executable artifact.
    pub const fn sha256(self) -> [u8; TAG_BYTES] {
        self.sha256
    }

    /// Content tag for the artifact length and exact SHA-256 digest.
    pub fn tag(self) -> ContentTag {
        hash_canonical(
            TagKind::Build,
            LOCAL_REPLAY_BUILD_SCHEMA,
            BUILD_CANONICAL_BYTES as u64,
            |sink| {
                sink(&self.artifact_len.to_be_bytes());
                sink(&self.sha256);
            },
        )
        .expect("fixed local replay build encoding has its declared length")
    }
}

/// Why a streamed local executable fingerprint was refused.
#[derive(Debug)]
pub enum LocalReplayBuildError {
    LengthLimit { actual: u64, maximum: u64 },
    ShortRead { expected: u64, observed: u64 },
    TrailingBytes { expected: u64 },
    Read(io::Error),
}

impl fmt::Display for LocalReplayBuildError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::LengthLimit { actual, maximum } => write!(
                formatter,
                "local executable declares {actual} bytes, exceeding the {maximum}-byte policy"
            ),
            Self::ShortRead { expected, observed } => write!(
                formatter,
                "local executable ended after {observed} bytes, expected {expected}"
            ),
            Self::TrailingBytes { expected } => write!(
                formatter,
                "local executable exceeded the declared {expected}-byte length"
            ),
            Self::Read(error) => write!(formatter, "could not read local executable: {error}"),
        }
    }
}

impl std::error::Error for LocalReplayBuildError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Read(error) => Some(error),
            _ => None,
        }
    }
}

/// The declared local replay context, independent from user-supplied accepted actions.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LocalReplayContext {
    build: LocalReplayBuild,
    genesis_policy: ContentTag,
    rules_policy: ContentTag,
    admission_policy: ContentTag,
}

impl LocalReplayContext {
    /// Constructs and validates a `LocalReplayContext` from build and policy tags.
    pub fn new(
        build: LocalReplayBuild,
        genesis_policy: ContentTag,
        rules_policy: ContentTag,
        admission_policy: ContentTag,
    ) -> Result<Self, LocalReplayEvidenceError> {
        let context = Self {
            build,
            genesis_policy,
            rules_policy,
            admission_policy,
        };
        context.verify()?;
        Ok(context)
    }

    /// Returns the `LocalReplayBuild` executable fingerprint.
    pub const fn build(self) -> LocalReplayBuild {
        self.build
    }

    /// Returns the declared genesis policy content tag.
    pub const fn genesis_policy(self) -> ContentTag {
        self.genesis_policy
    }

    /// Returns the declared simulation rules policy content tag.
    pub const fn rules_policy(self) -> ContentTag {
        self.rules_policy
    }

    /// Returns the declared action admission policy content tag.
    pub const fn admission_policy(self) -> ContentTag {
        self.admission_policy
    }

    /// Verifies that all three context policy tags have `TagKind::Policy`.
    pub fn verify(self) -> Result<(), LocalReplayEvidenceError> {
        require_kind("genesis policy", self.genesis_policy, TagKind::Policy)?;
        require_kind("rules policy", self.rules_policy, TagKind::Policy)?;
        require_kind("admission policy", self.admission_policy, TagKind::Policy)
    }
}

/// A pair naming one captured frame input and the resulting complete CPU frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ReplayFrameCommitment {
    input: ContentTag,
    frame: ContentTag,
}

impl ReplayFrameCommitment {
    /// Constructs and validates a `ReplayFrameCommitment` from frame-input and frame tags.
    pub fn new(input: ContentTag, frame: ContentTag) -> Result<Self, LocalReplayEvidenceError> {
        let commitment = Self { input, frame };
        commitment.verify()?;
        Ok(commitment)
    }

    /// Returns the `TagKind::FrameInput` content tag.
    pub const fn input(self) -> ContentTag {
        self.input
    }

    /// Returns the `TagKind::Frame` content tag.
    pub const fn frame(self) -> ContentTag {
        self.frame
    }

    /// Verifies the tag kinds of the frame input and rendered frame.
    pub fn verify(self) -> Result<(), LocalReplayEvidenceError> {
        require_kind("replay frame input", self.input, TagKind::FrameInput)?;
        require_kind("replay frame", self.frame, TagKind::Frame)
    }
}

/// The exact semantic endpoint selected from a local fixed-tick replay.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LocalReplayEndpoint {
    outcome: Option<ContentTag>,
    world: ContentTag,
    history: ContentTag,
    camera_bits: [u64; CAMERA_BITS_LEN],
    simulation_width: u32,
    simulation_height: u32,
    output_width: u32,
    output_height: u32,
    sidebar_visible: bool,
    game: Option<ReplayFrameCommitment>,
    composite: Option<ReplayFrameCommitment>,
}

impl LocalReplayEndpoint {
    /// Constructs and validates a `LocalReplayEndpoint` from final replay state commitments.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        outcome: Option<ContentTag>,
        world: ContentTag,
        history: ContentTag,
        camera_bits: [u64; CAMERA_BITS_LEN],
        simulation_width: u32,
        simulation_height: u32,
        output_width: u32,
        output_height: u32,
        sidebar_visible: bool,
        game: Option<ReplayFrameCommitment>,
        composite: Option<ReplayFrameCommitment>,
    ) -> Result<Self, LocalReplayEvidenceError> {
        let endpoint = Self {
            outcome,
            world,
            history,
            camera_bits,
            simulation_width,
            simulation_height,
            output_width,
            output_height,
            sidebar_visible,
            game,
            composite,
        };
        endpoint.verify_structure()?;
        Ok(endpoint)
    }

    /// Returns the optional final `TagKind::TickOutcome` content tag.
    pub const fn outcome(self) -> Option<ContentTag> {
        self.outcome
    }

    /// Returns the final `TagKind::World` content tag.
    pub const fn world(self) -> ContentTag {
        self.world
    }

    /// Returns the final `TagKind::Receipt` history content tag.
    pub const fn history(self) -> ContentTag {
        self.history
    }

    /// Returns the ten canonical IEEE-754 `u64` camera state lanes.
    pub const fn camera_bits(self) -> [u64; CAMERA_BITS_LEN] {
        self.camera_bits
    }

    /// Returns the simulation viewport dimensions `(width, height)` in pixels.
    pub const fn simulation_dimensions(self) -> (u32, u32) {
        (self.simulation_width, self.simulation_height)
    }

    /// Returns the output surface dimensions `(width, height)` in pixels.
    pub const fn output_dimensions(self) -> (u32, u32) {
        (self.output_width, self.output_height)
    }

    /// Returns whether the inspector sidebar was visible at the replay endpoint.
    pub const fn sidebar_visible(self) -> bool {
        self.sidebar_visible
    }

    /// Returns the optional latest game-only `ReplayFrameCommitment`.
    pub const fn game(self) -> Option<ReplayFrameCommitment> {
        self.game
    }

    /// Returns the optional latest composite `ReplayFrameCommitment`.
    pub const fn composite(self) -> Option<ReplayFrameCommitment> {
        self.composite
    }

    fn verify_structure(self) -> Result<(), LocalReplayEvidenceError> {
        if let Some(outcome) = self.outcome {
            require_kind("replay outcome", outcome, TagKind::TickOutcome)?;
        }
        require_kind("replay endpoint world", self.world, TagKind::World)?;
        require_kind("replay endpoint history", self.history, TagKind::Receipt)?;
        if let Some(game) = self.game {
            game.verify()?;
        }
        if let Some(composite) = self.composite {
            composite.verify()?;
        }
        if self.simulation_width == 0
            || self.simulation_height == 0
            || self.output_width == 0
            || self.output_height == 0
        {
            return Err(LocalReplayEvidenceError::ZeroDimension);
        }
        if self.output_width < self.simulation_width || self.output_height != self.simulation_height
        {
            return Err(LocalReplayEvidenceError::DimensionRelation {
                simulation_width: self.simulation_width,
                simulation_height: self.simulation_height,
                output_width: self.output_width,
                output_height: self.output_height,
            });
        }
        Ok(())
    }
}

/// A self-validating, content-addressed local replay carrier.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LocalReplayEvidence {
    context: LocalReplayContext,
    tape: InteractionTape,
    endpoint: LocalReplayEndpoint,
    tag: ContentTag,
}

impl LocalReplayEvidence {
    /// Constructs and hashes a validated `LocalReplayEvidence` carrier.
    pub fn new(
        context: LocalReplayContext,
        tape: InteractionTape,
        endpoint: LocalReplayEndpoint,
    ) -> Result<Self, LocalReplayEvidenceError> {
        validate_evidence(context, &tape, endpoint)?;
        let tag = tag_for_evidence(context, &tape, endpoint)?;
        Ok(Self {
            context,
            tape,
            endpoint,
            tag,
        })
    }

    /// Strictly decodes one complete bounded canonical carrier and recomputes its content tag.
    pub fn decode_canonical(bytes: &[u8]) -> Result<Self, LocalReplayEvidenceError> {
        if bytes.len() < LOCAL_REPLAY_EVIDENCE_FIXED_BYTES {
            return Err(LocalReplayEvidenceError::PayloadTooShort {
                actual: bytes.len(),
                minimum: LOCAL_REPLAY_EVIDENCE_FIXED_BYTES,
            });
        }
        let maximum = LOCAL_REPLAY_EVIDENCE_FIXED_BYTES
            .checked_add(MAX_INTERACTION_TAPE_PAYLOAD_BYTES)
            .expect("fixed replay evidence cap fits usize");
        if bytes.len() > maximum {
            return Err(LocalReplayEvidenceError::PayloadTooLarge {
                actual: bytes.len(),
                maximum,
            });
        }

        let declared_tape_len = u64::from_be_bytes(
            bytes[TAPE_LEN_OFFSET..TAPE_LEN_OFFSET + size_of::<u64>()]
                .try_into()
                .expect("fixed evidence prefix holds the tape length"),
        );
        if declared_tape_len > MAX_INTERACTION_TAPE_PAYLOAD_BYTES as u64 {
            return Err(LocalReplayEvidenceError::TapeLengthLimit {
                declared: declared_tape_len,
                maximum: MAX_INTERACTION_TAPE_PAYLOAD_BYTES,
            });
        }
        let tape_len = usize::try_from(declared_tape_len)
            .expect("the bounded interaction tape cap fits every supported usize");
        let expected = LOCAL_REPLAY_EVIDENCE_FIXED_BYTES
            .checked_add(tape_len)
            .expect("bounded replay evidence length fits usize");
        if bytes.len() != expected {
            return Err(LocalReplayEvidenceError::PayloadLengthMismatch {
                actual: bytes.len(),
                expected,
            });
        }

        let mut decoder = Decoder::new(bytes);
        let build = LocalReplayBuild::from_parts(
            decoder.read_u64("artifact length")?,
            decoder.read_digest("artifact SHA-256")?,
        );
        let genesis_policy = decoder.read_tag(TagKind::Policy, "genesis policy")?;
        let rules_policy = decoder.read_tag(TagKind::Policy, "rules policy")?;
        let admission_policy = decoder.read_tag(TagKind::Policy, "admission policy")?;
        let tape_len_again = decoder.read_u64("interaction tape byte length")?;
        debug_assert_eq!(tape_len_again, declared_tape_len);
        let tape_bytes = decoder.read_bytes(tape_len, "interaction tape")?;
        let tape = InteractionTape::decode_canonical(tape_bytes)
            .map_err(LocalReplayEvidenceError::Tape)?;
        let outcome = decoder.read_optional_tag(TagKind::TickOutcome, "replay outcome")?;
        let world = decoder.read_tag(TagKind::World, "replay endpoint world")?;
        let history = decoder.read_tag(TagKind::Receipt, "replay endpoint history")?;
        let camera_bits = std::array::from_fn(|_| {
            decoder
                .read_u64("replay camera bit lane")
                .expect("length-checked replay endpoint contains all camera lanes")
        });
        let simulation_width = decoder.read_u32("simulation width")?;
        let simulation_height = decoder.read_u32("simulation height")?;
        let output_width = decoder.read_u32("output width")?;
        let output_height = decoder.read_u32("output height")?;
        let sidebar_visible = decoder.read_boolean("sidebar visible")?;
        let game = decoder.read_optional_frame("latest game frame")?;
        let composite = decoder.read_optional_frame("latest composite frame")?;
        debug_assert_eq!(decoder.remaining(), 0);

        let context =
            LocalReplayContext::new(build, genesis_policy, rules_policy, admission_policy)?;
        let endpoint = LocalReplayEndpoint::new(
            outcome,
            world,
            history,
            camera_bits,
            simulation_width,
            simulation_height,
            output_width,
            output_height,
            sidebar_visible,
            game,
            composite,
        )?;
        Self::new(context, tape, endpoint)
    }

    /// Recomputes all structural invariants and the complete local replay-evidence tag.
    pub fn verify(&self) -> Result<(), LocalReplayEvidenceError> {
        validate_evidence(self.context, &self.tape, self.endpoint)?;
        let expected = tag_for_evidence(self.context, &self.tape, self.endpoint)?;
        if self.tag != expected {
            return Err(LocalReplayEvidenceError::TagMismatch {
                expected,
                actual: self.tag,
            });
        }
        Ok(())
    }

    /// Returns the `LocalReplayContext` bound into this carrier.
    pub const fn context(&self) -> LocalReplayContext {
        self.context
    }

    /// Borrows the embedded `InteractionTape` bound into this carrier.
    pub fn tape(&self) -> &InteractionTape {
        &self.tape
    }

    /// Returns the `LocalReplayEndpoint` bound into this carrier.
    pub const fn endpoint(&self) -> LocalReplayEndpoint {
        self.endpoint
    }

    /// Returns the canonical content tag of this replay-evidence carrier.
    pub const fn tag(&self) -> ContentTag {
        self.tag
    }

    /// Total canonical carrier bytes, including the complete embedded tape payload.
    pub fn encoded_len(&self) -> usize {
        LOCAL_REPLAY_EVIDENCE_FIXED_BYTES + self.tape.encode_canonical().len()
    }

    /// Encodes the exact carrier bytes named by [`Self::tag`].
    pub fn encode_canonical(&self) -> Vec<u8> {
        let tape_bytes = self.tape.encode_canonical();
        let mut bytes = Vec::with_capacity(LOCAL_REPLAY_EVIDENCE_FIXED_BYTES + tape_bytes.len());
        encode_evidence_fields(self.context, &tape_bytes, self.endpoint, &mut |chunk| {
            bytes.extend_from_slice(chunk)
        });
        debug_assert_eq!(bytes.len(), self.encoded_len());
        bytes
    }
}

fn validate_evidence(
    context: LocalReplayContext,
    tape: &InteractionTape,
    endpoint: LocalReplayEndpoint,
) -> Result<(), LocalReplayEvidenceError> {
    context.verify()?;
    endpoint.verify_structure()?;
    let tape_bytes = tape.encode_canonical();
    if tape_bytes.len() > MAX_INTERACTION_TAPE_PAYLOAD_BYTES {
        return Err(LocalReplayEvidenceError::TapeLengthLimit {
            declared: tape_bytes.len() as u64,
            maximum: MAX_INTERACTION_TAPE_PAYLOAD_BYTES,
        });
    }

    match (
        tape.is_empty(),
        endpoint.outcome,
        endpoint.game,
        endpoint.composite,
    ) {
        (true, None, None, None) => Ok(()),
        (true, outcome, game, composite) => Err(LocalReplayEvidenceError::EmptyTapeEndpoint {
            has_outcome: outcome.is_some(),
            has_game: game.is_some(),
            has_composite: composite.is_some(),
        }),
        (false, Some(_), Some(_), Some(_)) => Ok(()),
        (false, outcome, game, composite) => {
            Err(LocalReplayEvidenceError::IncompleteReplayEndpoint {
                has_outcome: outcome.is_some(),
                has_game: game.is_some(),
                has_composite: composite.is_some(),
            })
        }
    }
}

fn tag_for_evidence(
    context: LocalReplayContext,
    tape: &InteractionTape,
    endpoint: LocalReplayEndpoint,
) -> Result<ContentTag, LocalReplayEvidenceError> {
    let tape_bytes = tape.encode_canonical();
    let payload_len = LOCAL_REPLAY_EVIDENCE_FIXED_BYTES
        .checked_add(tape_bytes.len())
        .expect("bounded local replay evidence length fits usize");
    hash_canonical(
        TagKind::ReplayEvidence,
        LOCAL_REPLAY_EVIDENCE_SCHEMA,
        payload_len as u64,
        |sink| encode_evidence_fields(context, &tape_bytes, endpoint, sink),
    )
    .map_err(LocalReplayEvidenceError::Tag)
}

fn encode_evidence_fields(
    context: LocalReplayContext,
    tape_bytes: &[u8],
    endpoint: LocalReplayEndpoint,
    sink: &mut dyn FnMut(&[u8]),
) {
    sink(&context.build.artifact_len.to_be_bytes());
    sink(&context.build.sha256);
    sink(&context.genesis_policy.digest());
    sink(&context.rules_policy.digest());
    sink(&context.admission_policy.digest());
    sink(&(tape_bytes.len() as u64).to_be_bytes());
    sink(tape_bytes);
    encode_optional_tag(endpoint.outcome, sink);
    sink(&endpoint.world.digest());
    sink(&endpoint.history.digest());
    for bits in endpoint.camera_bits {
        sink(&bits.to_be_bytes());
    }
    sink(&endpoint.simulation_width.to_be_bytes());
    sink(&endpoint.simulation_height.to_be_bytes());
    sink(&endpoint.output_width.to_be_bytes());
    sink(&endpoint.output_height.to_be_bytes());
    sink(&[u8::from(endpoint.sidebar_visible)]);
    encode_optional_frame(endpoint.game, sink);
    encode_optional_frame(endpoint.composite, sink);
}

fn encode_optional_tag(tag: Option<ContentTag>, sink: &mut dyn FnMut(&[u8])) {
    match tag {
        Some(tag) => {
            sink(&[1]);
            sink(&tag.digest());
        }
        None => sink(&[0; OPTIONAL_TAG_BYTES]),
    }
}

fn encode_optional_frame(commitment: Option<ReplayFrameCommitment>, sink: &mut dyn FnMut(&[u8])) {
    match commitment {
        Some(commitment) => {
            sink(&[1]);
            sink(&commitment.input.digest());
            sink(&commitment.frame.digest());
        }
        None => sink(&[0; OPTIONAL_FRAME_BYTES]),
    }
}

fn require_kind(
    field: &'static str,
    tag: ContentTag,
    expected: TagKind,
) -> Result<(), LocalReplayEvidenceError> {
    if tag.kind() == expected {
        Ok(())
    } else {
        Err(LocalReplayEvidenceError::WrongKind {
            field,
            expected,
            actual: tag.kind(),
        })
    }
}

struct Decoder<'a> {
    bytes: &'a [u8],
    cursor: usize,
}

impl<'a> Decoder<'a> {
    const fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, cursor: 0 }
    }

    fn remaining(&self) -> usize {
        self.bytes.len() - self.cursor
    }

    fn read_bytes(
        &mut self,
        count: usize,
        field: &'static str,
    ) -> Result<&'a [u8], LocalReplayEvidenceError> {
        let end = self
            .cursor
            .checked_add(count)
            .ok_or(LocalReplayEvidenceError::Truncated {
                field,
                offset: self.cursor,
                needed: count,
                remaining: self.remaining(),
            })?;
        if end > self.bytes.len() {
            return Err(LocalReplayEvidenceError::Truncated {
                field,
                offset: self.cursor,
                needed: count,
                remaining: self.remaining(),
            });
        }
        let result = &self.bytes[self.cursor..end];
        self.cursor = end;
        Ok(result)
    }

    fn read_u64(&mut self, field: &'static str) -> Result<u64, LocalReplayEvidenceError> {
        Ok(u64::from_be_bytes(
            self.read_bytes(size_of::<u64>(), field)?
                .try_into()
                .expect("the exact u64 field length was checked"),
        ))
    }

    fn read_u32(&mut self, field: &'static str) -> Result<u32, LocalReplayEvidenceError> {
        Ok(u32::from_be_bytes(
            self.read_bytes(size_of::<u32>(), field)?
                .try_into()
                .expect("the exact u32 field length was checked"),
        ))
    }

    fn read_digest(
        &mut self,
        field: &'static str,
    ) -> Result<[u8; TAG_BYTES], LocalReplayEvidenceError> {
        Ok(self
            .read_bytes(TAG_BYTES, field)?
            .try_into()
            .expect("the exact digest field length was checked"))
    }

    fn read_tag(
        &mut self,
        kind: TagKind,
        field: &'static str,
    ) -> Result<ContentTag, LocalReplayEvidenceError> {
        Ok(ContentTag::from_digest(kind, self.read_digest(field)?))
    }

    fn read_boolean(&mut self, field: &'static str) -> Result<bool, LocalReplayEvidenceError> {
        match self.read_bytes(1, field)?[0] {
            0 => Ok(false),
            1 => Ok(true),
            actual => Err(LocalReplayEvidenceError::Boolean { field, actual }),
        }
    }

    fn read_optional_tag(
        &mut self,
        kind: TagKind,
        field: &'static str,
    ) -> Result<Option<ContentTag>, LocalReplayEvidenceError> {
        let bytes = self.read_bytes(OPTIONAL_TAG_BYTES, field)?;
        match bytes[0] {
            0 if bytes[1..].iter().all(|&byte| byte == 0) => Ok(None),
            0 => Err(LocalReplayEvidenceError::NonCanonicalAbsent { field }),
            1 => Ok(Some(ContentTag::from_digest(
                kind,
                bytes[1..]
                    .try_into()
                    .expect("the optional tag digest has a fixed length"),
            ))),
            actual => Err(LocalReplayEvidenceError::OptionalMarker { field, actual }),
        }
    }

    fn read_optional_frame(
        &mut self,
        field: &'static str,
    ) -> Result<Option<ReplayFrameCommitment>, LocalReplayEvidenceError> {
        let bytes = self.read_bytes(OPTIONAL_FRAME_BYTES, field)?;
        match bytes[0] {
            0 if bytes[1..].iter().all(|&byte| byte == 0) => Ok(None),
            0 => Err(LocalReplayEvidenceError::NonCanonicalAbsent { field }),
            1 => ReplayFrameCommitment::new(
                ContentTag::from_digest(
                    TagKind::FrameInput,
                    bytes[1..1 + TAG_BYTES]
                        .try_into()
                        .expect("the optional frame input digest has a fixed length"),
                ),
                ContentTag::from_digest(
                    TagKind::Frame,
                    bytes[1 + TAG_BYTES..]
                        .try_into()
                        .expect("the optional frame digest has a fixed length"),
                ),
            )
            .map(Some),
            actual => Err(LocalReplayEvidenceError::OptionalMarker { field, actual }),
        }
    }
}

/// Why one local replay-evidence carrier was refused.
#[derive(Debug)]
pub enum LocalReplayEvidenceError {
    PayloadTooShort {
        actual: usize,
        minimum: usize,
    },
    PayloadTooLarge {
        actual: usize,
        maximum: usize,
    },
    PayloadLengthMismatch {
        actual: usize,
        expected: usize,
    },
    TapeLengthLimit {
        declared: u64,
        maximum: usize,
    },
    Truncated {
        field: &'static str,
        offset: usize,
        needed: usize,
        remaining: usize,
    },
    WrongKind {
        field: &'static str,
        expected: TagKind,
        actual: TagKind,
    },
    OptionalMarker {
        field: &'static str,
        actual: u8,
    },
    NonCanonicalAbsent {
        field: &'static str,
    },
    Boolean {
        field: &'static str,
        actual: u8,
    },
    ZeroDimension,
    DimensionRelation {
        simulation_width: u32,
        simulation_height: u32,
        output_width: u32,
        output_height: u32,
    },
    EmptyTapeEndpoint {
        has_outcome: bool,
        has_game: bool,
        has_composite: bool,
    },
    IncompleteReplayEndpoint {
        has_outcome: bool,
        has_game: bool,
        has_composite: bool,
    },
    Tape(InteractionTapeCodecError),
    Tag(TagError),
    TagMismatch {
        expected: ContentTag,
        actual: ContentTag,
    },
}

impl fmt::Display for LocalReplayEvidenceError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::PayloadTooShort { actual, minimum } => write!(
                formatter,
                "local replay evidence has {actual} bytes, fewer than fixed minimum {minimum}"
            ),
            Self::PayloadTooLarge { actual, maximum } => write!(
                formatter,
                "local replay evidence has {actual} bytes, exceeding bounded maximum {maximum}"
            ),
            Self::PayloadLengthMismatch { actual, expected } => write!(
                formatter,
                "local replay evidence has {actual} bytes, but its embedded tape length requires {expected}"
            ),
            Self::TapeLengthLimit { declared, maximum } => write!(
                formatter,
                "local replay evidence declares a {declared}-byte interaction tape, exceeding {maximum}"
            ),
            Self::Truncated {
                field,
                offset,
                needed,
                remaining,
            } => write!(
                formatter,
                "local replay evidence truncated {field} at byte {offset}: need {needed}, have {remaining}"
            ),
            Self::WrongKind {
                field,
                expected,
                actual,
            } => write!(
                formatter,
                "local replay evidence {field} must be {}, received {}",
                expected.name(),
                actual.name()
            ),
            Self::OptionalMarker { field, actual } => write!(
                formatter,
                "local replay evidence {field} marker {actual:#04x} is not canonical"
            ),
            Self::NonCanonicalAbsent { field } => write!(
                formatter,
                "local replay evidence absent {field} has nonzero padding"
            ),
            Self::Boolean { field, actual } => write!(
                formatter,
                "local replay evidence {field} boolean {actual:#04x} is not canonical"
            ),
            Self::ZeroDimension => {
                formatter.write_str("local replay evidence endpoint has a zero dimension")
            }
            Self::DimensionRelation {
                simulation_width,
                simulation_height,
                output_width,
                output_height,
            } => write!(
                formatter,
                "local replay evidence dimensions simulation={simulation_width}x{simulation_height} output={output_width}x{output_height} are invalid"
            ),
            Self::EmptyTapeEndpoint {
                has_outcome,
                has_game,
                has_composite,
            } => write!(
                formatter,
                "empty interaction tape must have no outcome/game/composite endpoint fields; got outcome={has_outcome} game={has_game} composite={has_composite}"
            ),
            Self::IncompleteReplayEndpoint {
                has_outcome,
                has_game,
                has_composite,
            } => write!(
                formatter,
                "nonempty interaction tape must bind outcome/game/composite fields; got outcome={has_outcome} game={has_game} composite={has_composite}"
            ),
            Self::Tape(error) => write!(formatter, "embedded interaction tape is invalid: {error}"),
            Self::Tag(error) => write!(formatter, "local replay evidence tag failed: {error}"),
            Self::TagMismatch { expected, actual } => write!(
                formatter,
                "local replay evidence tag mismatch: expected {expected}, received {actual}"
            ),
        }
    }
}

impl std::error::Error for LocalReplayEvidenceError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Tape(error) => Some(error),
            Self::Tag(error) => Some(error),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use super::*;

    fn tag(kind: TagKind, byte: u8) -> ContentTag {
        ContentTag::from_digest(kind, [byte; TAG_BYTES])
    }

    fn context() -> LocalReplayContext {
        LocalReplayContext::new(
            LocalReplayBuild::from_parts(123, [1; TAG_BYTES]),
            tag(TagKind::Policy, 2),
            tag(TagKind::Policy, 3),
            tag(TagKind::Policy, 4),
        )
        .unwrap()
    }

    fn frame(byte: u8) -> ReplayFrameCommitment {
        ReplayFrameCommitment::new(
            tag(TagKind::FrameInput, byte),
            tag(TagKind::Frame, byte + 1),
        )
        .unwrap()
    }

    /// The executable fingerprint, pinned against `tools/local_replay_build_vectors.py`.
    ///
    /// The build tag carries two fields, not one, and the second is why: a digest alone would
    /// be satisfied by a stream the reader stopped early on, so the declared length is inside
    /// the hashed domain too. The cases below hold one field while moving the other, which is
    /// the only way to show both are actually bound.
    ///
    /// This encoder was previously unpinned from either direction. The carrier vector above
    /// exercises the replay-evidence payload, which spells the build's two fields inline
    /// through a different encoder, so a byte-order flip in `LocalReplayBuild::tag` left it
    /// green. That mutation was run: it fails here and nowhere else.
    ///
    /// The literals were produced by the Python reconstruction and never by this module.
    #[test]
    fn independent_python_vectors_pin_the_local_replay_build_tag() {
        // The empty artifact: length zero and the SHA-256 of no bytes at all.
        let empty = LocalReplayBuild::fingerprint_reader(Cursor::new(Vec::new()), 0, 1 << 20)
            .expect("an empty artifact is a well-formed declaration");
        assert_eq!(
            empty.tag().to_string(),
            "#bb.build.sha256.finohoe777iojerrq7tckczn6jr65uzsvt42dvuyfxbsztdxazca"
        );

        let artifact: Vec<u8> = (0..4).flat_map(|_| 0..=255_u8).collect();
        assert_eq!(artifact.len(), 1024);
        let real =
            LocalReplayBuild::fingerprint_reader(Cursor::new(artifact.clone()), 1024, 1 << 20)
                .expect("a complete stream is admitted");
        assert_eq!(
            real.tag().to_string(),
            "#bb.build.sha256.7vr3dqbgdrme5n5n3kfzqbixtbwhtgkomjcfde6bckv5rnqa2hyq"
        );

        // Same digest, a length short by one: a truncated read cannot name the whole artifact.
        let lied = LocalReplayBuild::from_parts(1023, real.sha256());
        assert_eq!(
            lied.tag().to_string(),
            "#bb.build.sha256.6btonokjyyuciib5kq3saaib7gqfwap5irozq62b6endjgk6sbya"
        );
        assert_ne!(lied.tag(), real.tag());

        // Same length, one flipped artifact byte.
        let mut flipped_bytes = artifact;
        flipped_bytes[512] ^= 0x01;
        let flipped =
            LocalReplayBuild::fingerprint_reader(Cursor::new(flipped_bytes), 1024, 1 << 20)
                .expect("a complete stream is admitted");
        assert_eq!(
            flipped.tag().to_string(),
            "#bb.build.sha256.dnvcagmbmwfcs6rucq3f2xy3d7wrizhqrlqe2malwzp4edpxeqfq"
        );
        assert_ne!(flipped.tag(), real.tag());
        assert_eq!(flipped.artifact_len(), real.artifact_len());

        // The streamed fingerprint and the raw declaration name the same object.
        assert_eq!(
            LocalReplayBuild::from_parts(1024, flipped.sha256()).tag(),
            flipped.tag()
        );
        assert_eq!(flipped.tag().kind(), TagKind::Build);
    }

    fn endpoint(nonempty: bool) -> LocalReplayEndpoint {
        LocalReplayEndpoint::new(
            nonempty.then(|| tag(TagKind::TickOutcome, 5)),
            tag(TagKind::World, 6),
            tag(TagKind::Receipt, 7),
            std::array::from_fn(|index| index as u64 + 8),
            801,
            801,
            1061,
            801,
            true,
            nonempty.then(|| frame(18)),
            nonempty.then(|| frame(28)),
        )
        .unwrap()
    }

    fn nonempty_tape() -> InteractionTape {
        "tick\nwheel -1\nplace\ntick\nmove forward down\nlook 1 -0\nsidebar"
            .parse()
            .unwrap()
    }

    fn range_tag(kind: TagKind, start: u8) -> ContentTag {
        ContentTag::from_digest(
            kind,
            std::array::from_fn(|index| start.wrapping_add(index as u8)),
        )
    }

    fn independent_vector_evidence() -> LocalReplayEvidence {
        let mut tape_bytes = Vec::with_capacity(38);
        tape_bytes.extend_from_slice(&1_u64.to_be_bytes());
        tape_bytes.extend_from_slice(&2_u64.to_be_bytes());
        tape_bytes.extend_from_slice(&0_u64.to_be_bytes());
        tape_bytes.extend_from_slice(&2_u32.to_be_bytes());
        tape_bytes.push(2); // wheel lines
        tape_bytes.extend_from_slice(&(-0.0_f64).to_bits().to_be_bytes());
        tape_bytes.push(4); // aim place
        let tape = InteractionTape::decode_canonical(&tape_bytes).unwrap();
        let context = LocalReplayContext::new(
            LocalReplayBuild::from_parts(
                0x0102_0304_0506_0708,
                std::array::from_fn(|index| index as u8),
            ),
            range_tag(TagKind::Policy, 0x20),
            range_tag(TagKind::Policy, 0x40),
            range_tag(TagKind::Policy, 0x60),
        )
        .unwrap();
        let endpoint = LocalReplayEndpoint::new(
            Some(range_tag(TagKind::TickOutcome, 0x80)),
            range_tag(TagKind::World, 0xa0),
            range_tag(TagKind::Receipt, 0xc0),
            std::array::from_fn(|index| 0x0102_0304_0506_0708 + index as u64),
            801,
            601,
            1001,
            601,
            true,
            Some(
                ReplayFrameCommitment::new(
                    range_tag(TagKind::FrameInput, 0xe0),
                    range_tag(TagKind::Frame, 0x00),
                )
                .unwrap(),
            ),
            Some(
                ReplayFrameCommitment::new(
                    range_tag(TagKind::FrameInput, 0x20),
                    range_tag(TagKind::Frame, 0x40),
                )
                .unwrap(),
            ),
        )
        .unwrap();
        LocalReplayEvidence::new(context, tape, endpoint).unwrap()
    }

    #[test]
    fn build_fingerprint_streams_exactly_the_declared_bytes() {
        let build = LocalReplayBuild::fingerprint_reader(Cursor::new(b"abc"), 3, 3).unwrap();
        assert_eq!(build.artifact_len(), 3);
        assert_ne!(
            build.tag(),
            LocalReplayBuild::from_parts(3, [0; TAG_BYTES]).tag()
        );
        assert!(matches!(
            LocalReplayBuild::fingerprint_reader(Cursor::new(b"ab"), 3, 3),
            Err(LocalReplayBuildError::ShortRead { .. })
        ));
        assert!(matches!(
            LocalReplayBuild::fingerprint_reader(Cursor::new(b"abcd"), 3, 4),
            Err(LocalReplayBuildError::TrailingBytes { .. })
        ));
        assert!(matches!(
            LocalReplayBuild::fingerprint_reader(Cursor::new(b"abc"), 3, 2),
            Err(LocalReplayBuildError::LengthLimit { .. })
        ));
    }

    #[test]
    fn canonical_round_trip_embeds_the_complete_tape_and_endpoint() {
        let evidence =
            LocalReplayEvidence::new(context(), nonempty_tape(), endpoint(true)).unwrap();
        let encoded = evidence.encode_canonical();
        assert_eq!(encoded.len(), evidence.encoded_len());
        assert_eq!(
            encoded.len(),
            LOCAL_REPLAY_EVIDENCE_FIXED_BYTES + evidence.tape().encode_canonical().len()
        );
        let decoded = LocalReplayEvidence::decode_canonical(&encoded).unwrap();
        assert_eq!(decoded, evidence);
        assert_eq!(decoded.tape().content_tag(), evidence.tape().content_tag());
        decoded.verify().unwrap();
    }

    #[test]
    fn independent_python_known_answer_vector_matches_the_complete_carrier() {
        let evidence = independent_vector_evidence();
        assert_eq!(evidence.encoded_len(), 506);
        assert_eq!(
            evidence.tag().to_string(),
            "#bb.replay-evidence.sha256.r7ukwnl52rten5bwquqztobeuhfn362ywca7wrcudy5mvpache4q"
        );
        assert_eq!(
            LocalReplayEvidence::decode_canonical(&evidence.encode_canonical()).unwrap(),
            evidence
        );
    }

    #[test]
    fn every_proper_prefix_and_a_trailing_byte_refuse() {
        let evidence =
            LocalReplayEvidence::new(context(), nonempty_tape(), endpoint(true)).unwrap();
        let encoded = evidence.encode_canonical();
        for end in 0..encoded.len() {
            assert!(
                LocalReplayEvidence::decode_canonical(&encoded[..end]).is_err(),
                "proper prefix length {end} unexpectedly decoded"
            );
        }
        let mut trailing = encoded;
        trailing.push(0);
        assert!(matches!(
            LocalReplayEvidence::decode_canonical(&trailing),
            Err(LocalReplayEvidenceError::PayloadLengthMismatch { .. })
        ));
    }

    #[test]
    fn decodable_single_byte_mutations_change_the_outer_tag() {
        let evidence =
            LocalReplayEvidence::new(context(), nonempty_tape(), endpoint(true)).unwrap();
        let encoded = evidence.encode_canonical();
        for index in 0..encoded.len() {
            let mut changed = encoded.clone();
            changed[index] ^= 1;
            if let Ok(candidate) = LocalReplayEvidence::decode_canonical(&changed) {
                assert_ne!(candidate.tag(), evidence.tag(), "byte {index} did not bind");
            }
        }
    }

    #[test]
    fn malformed_boolean_and_absent_padding_refuse() {
        let evidence =
            LocalReplayEvidence::new(context(), nonempty_tape(), endpoint(true)).unwrap();
        let mut boolean = evidence.encode_canonical();
        let sidebar_offset = TAPE_LEN_OFFSET
            + size_of::<u64>()
            + evidence.tape().encode_canonical().len()
            + OPTIONAL_TAG_BYTES
            + 2 * TAG_BYTES
            + CAMERA_BITS_LEN * size_of::<u64>()
            + 4 * size_of::<u32>();
        boolean[sidebar_offset] = 2;
        assert!(matches!(
            LocalReplayEvidence::decode_canonical(&boolean),
            Err(LocalReplayEvidenceError::Boolean {
                field: "sidebar visible",
                actual: 2
            })
        ));

        let empty =
            LocalReplayEvidence::new(context(), InteractionTape::new(), endpoint(false)).unwrap();
        let mut absent = empty.encode_canonical();
        let game_offset = TAPE_LEN_OFFSET
            + size_of::<u64>()
            + empty.tape().encode_canonical().len()
            + OPTIONAL_TAG_BYTES
            + 2 * TAG_BYTES
            + CAMERA_BITS_LEN * size_of::<u64>()
            + 4 * size_of::<u32>()
            + 1;
        absent[game_offset + 1] = 1;
        assert!(matches!(
            LocalReplayEvidence::decode_canonical(&absent),
            Err(LocalReplayEvidenceError::NonCanonicalAbsent {
                field: "latest game frame"
            })
        ));
    }

    #[test]
    fn tape_and_endpoint_shapes_fail_closed() {
        let empty_with_outcome = LocalReplayEndpoint::new(
            Some(tag(TagKind::TickOutcome, 70)),
            tag(TagKind::World, 71),
            tag(TagKind::Receipt, 72),
            [0; CAMERA_BITS_LEN],
            1,
            1,
            1,
            1,
            false,
            None,
            None,
        )
        .unwrap();
        assert!(matches!(
            LocalReplayEvidence::new(context(), InteractionTape::new(), empty_with_outcome),
            Err(LocalReplayEvidenceError::EmptyTapeEndpoint { .. })
        ));

        let incomplete = LocalReplayEndpoint::new(
            Some(tag(TagKind::TickOutcome, 70)),
            tag(TagKind::World, 71),
            tag(TagKind::Receipt, 72),
            [0; CAMERA_BITS_LEN],
            1,
            1,
            1,
            1,
            false,
            Some(frame(73)),
            None,
        )
        .unwrap();
        assert!(matches!(
            LocalReplayEvidence::new(context(), nonempty_tape(), incomplete),
            Err(LocalReplayEvidenceError::IncompleteReplayEndpoint { .. })
        ));

        assert!(matches!(
            LocalReplayContext::new(
                LocalReplayBuild::from_parts(0, [0; TAG_BYTES]),
                tag(TagKind::World, 1),
                tag(TagKind::Policy, 2),
                tag(TagKind::Policy, 3),
            ),
            Err(LocalReplayEvidenceError::WrongKind {
                field: "genesis policy",
                ..
            })
        ));
    }
}
