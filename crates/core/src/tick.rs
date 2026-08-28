//! Canonical outcome of one deterministic fixed simulation tick.
//!
//! A [`TickOutcome`] commits the exact consumed control batch, the ordered disposition of every
//! edit attempt, the pre/post world and coordinate-history heads, the post-tick camera state, and
//! any game/composite frame pair produced by that tick. It deliberately excludes presentation
//! receipts and display capture: those occur later and may point back to this outcome without
//! forming a dependency cycle.
//!
//! The tag proves byte identity under SHA-256. Full validity additionally requires resolving the
//! parent, replaying the declared `bitblock/tick-outcome/v3` transition, checking the fixed genesis
//! camera for tick zero, and independently verifying referenced frame objects.

use std::f64::consts::PI;
use std::fmt;

use crate::camera::{
    Camera, MAX_ABS_PITCH_RADIANS, MAX_OBSERVATION_DISTANCE, MIN_OBSERVATION_DISTANCE,
};
use crate::frame::{FrameInput, FrameProtocolError};
use crate::program::{EDIT_CANONICAL_LEN, Edit, EditCodecError, Operation};
use crate::protocol::{ContentTag, TagKind, hash_canonical};

pub const TICK_OUTCOME_SCHEMA: &[u8] = b"bitblock/tick-outcome/v3";

/// Bytes present in every v3 payload, excluding its ordered edit-attempt records.
///
/// v3 adds the 40-byte accepted-action ledger to v2's 429.
pub const TICK_OUTCOME_FIXED_PAYLOAD_LEN: usize = 469;

/// One counter per accepted-action kind.
pub const ACTION_LANE_COUNT: usize = 10;

/// Lane ordinals. These are deliberately the interaction-tape opcodes, so a tape
/// record and the tick outcome that consumes it count the same actions into the
/// same slots and can be reconciled without a translation table.
pub const LANE_MOVE_EDGE: usize = 0;
pub const LANE_LOOK_DELTA: usize = 1;
pub const LANE_WHEEL_LINES: usize = 2;
pub const LANE_AIM_DELETE: usize = 3;
pub const LANE_AIM_PLACE: usize = 4;
pub const LANE_EXACT_EDIT: usize = 5;
pub const LANE_FIT_WORLD: usize = 6;
pub const LANE_TOGGLE_SIDEBAR: usize = 7;
pub const LANE_RESIZE: usize = 8;
pub const LANE_CLEAR_MOTION: usize = 9;

/// Most accepted actions one tick may consume, matching the interaction tape's
/// per-record bound. A tape that parses can therefore always be accounted for.
pub const MAX_ACTIONS_PER_TICK: u32 = 65_536;

/// Why one tick could not account for an accepted action.
///
/// Deliberately separate from [`TickProtocolError`], which carries decoder
/// sources and so cannot be `Copy`. Callers refuse an action on this value in
/// their hot path, before any mutation, so it stays cheap.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TickActionError {
    UnknownLane { lane: usize },
    Overflow { limit: u32 },
}

impl fmt::Display for TickActionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {
            Self::UnknownLane { lane } => {
                write!(
                    formatter,
                    "accepted action counted into unknown lane {lane}"
                )
            }
            Self::Overflow { limit } => write!(
                formatter,
                "one tick cannot account for more than {limit} accepted actions"
            ),
        }
    }
}

impl std::error::Error for TickActionError {}

impl From<TickActionError> for TickProtocolError {
    fn from(error: TickActionError) -> Self {
        match error {
            TickActionError::UnknownLane { lane } => Self::UnknownActionLane { lane },
            TickActionError::Overflow { limit } => Self::TickActionOverflow { limit },
        }
    }
}

/// How many accepted actions of each kind one tick consumed.
///
/// The point of this record is that an accepted action cannot vanish. Before v3
/// three actions -- toggling the sidebar, resizing, and clearing motion -- were
/// applied the instant they were accepted and appeared in no tick outcome at all.
/// Their *effects* were committed, through the sidebar flag and the viewport
/// dimensions in [`TickInputState`], but nothing recorded that the action had been
/// submitted, so no count could be reconciled against a tape and no replay could
/// be shown to have consumed the same actions as the live run.
///
/// Counting is deliberately all this commits. Ordered per-event records would put
/// mouse-motion granularity into a permanent payload; per-lane totals reconcile
/// exactly against a tape record while staying a fixed 40 bytes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TickActionLedger {
    lanes: [u32; ACTION_LANE_COUNT],
}

impl TickActionLedger {
    pub const fn empty() -> Self {
        Self {
            lanes: [0; ACTION_LANE_COUNT],
        }
    }

    /// The minimal ledger consistent with `attempts`: every edit attempted was
    /// submitted, and nothing else was. Fixtures that exercise no control action
    /// use this; it is not a default, it is a claim about what the tick consumed.
    pub fn for_edit_attempts(attempts: &[TickAttempt]) -> Self {
        let mut ledger = Self::empty();
        for attempt in attempts {
            let lane = match attempt {
                TickAttempt::AimDeleteUnresolved | TickAttempt::AimDeleteExecuted(_) => {
                    LANE_AIM_DELETE
                }
                TickAttempt::AimPlaceUnresolved
                | TickAttempt::AimPlaceRejectedObserver(_)
                | TickAttempt::AimPlaceExecuted(_) => LANE_AIM_PLACE,
                TickAttempt::ExactRejectedObserver(_) | TickAttempt::ExactExecuted(_) => {
                    LANE_EXACT_EDIT
                }
            };
            ledger.lanes[lane] += 1;
        }
        ledger
    }

    /// Counts one accepted action, or fails without recording it.
    ///
    /// Fail-closed is the whole contract: callers must record *before* mutating,
    /// so a tick that cannot account for an action never applies it either.
    pub fn record(&mut self, lane: usize) -> Result<(), TickActionError> {
        if lane >= ACTION_LANE_COUNT {
            return Err(TickActionError::UnknownLane { lane });
        }
        if self.submitted() >= u64::from(MAX_ACTIONS_PER_TICK) {
            return Err(TickActionError::Overflow {
                limit: MAX_ACTIONS_PER_TICK,
            });
        }
        self.lanes[lane] += 1;
        Ok(())
    }

    pub const fn lane(&self, lane: usize) -> u32 {
        self.lanes[lane]
    }

    /// Every accepted action this tick consumed, across all lanes.
    pub const fn submitted(&self) -> u64 {
        let mut total = 0_u64;
        let mut index = 0;
        while index < ACTION_LANE_COUNT {
            total += self.lanes[index] as u64;
            index += 1;
        }
        total
    }

    /// Actions whose disposition is an ordered [`TickAttempt`].
    pub const fn edit_lane_total(&self) -> u32 {
        self.lanes[LANE_AIM_DELETE] + self.lanes[LANE_AIM_PLACE] + self.lanes[LANE_EXACT_EDIT]
    }

    /// Actions applied outside the edit fold: the lanes v2 could not see.
    pub const fn control_lane_total(&self) -> u32 {
        self.lanes[LANE_TOGGLE_SIDEBAR] + self.lanes[LANE_RESIZE] + self.lanes[LANE_CLEAR_MOTION]
    }

    /// Actions that only steer the observer, committed through the input state.
    pub const fn camera_lane_total(&self) -> u32 {
        self.lanes[LANE_MOVE_EDGE]
            + self.lanes[LANE_LOOK_DELTA]
            + self.lanes[LANE_WHEEL_LINES]
            + self.lanes[LANE_FIT_WORLD]
    }

    fn validate(&self, attempts: &[TickAttempt]) -> Result<(), TickProtocolError> {
        if self.submitted() > u64::from(MAX_ACTIONS_PER_TICK) {
            return Err(TickProtocolError::TickActionOverflow {
                limit: MAX_ACTIONS_PER_TICK,
            });
        }
        // The reconciliation the gate asks for: every submitted edit has exactly
        // one ordered disposition, and every disposition had a submission.
        //
        // Compared lane by lane, not by total. Attempts carry their kind, so a
        // ledger claiming two aim-deletes against one aim-delete and one
        // aim-place has a correct sum and a false split; checking only the sum
        // would admit it. A guard that can be checked and is not is the same
        // shape as a guard that agrees with its own premise.
        let expected = Self::for_edit_attempts(attempts);
        for lane in [LANE_AIM_DELETE, LANE_AIM_PLACE, LANE_EXACT_EDIT] {
            if self.lanes[lane] != expected.lanes[lane] {
                return Err(TickProtocolError::ActionAccountingMismatch {
                    lane,
                    submitted: self.lanes[lane],
                    dispositions: expected.lanes[lane],
                });
            }
        }
        Ok(())
    }

    fn encode_with(&self, sink: &mut dyn FnMut(&[u8])) {
        for lane in self.lanes {
            sink(&lane.to_be_bytes());
        }
    }
}

const INPUT_FLAG_MASK: u16 = (1 << 11) - 1;
#[cfg(any(test, feature = "protocol-research"))]
const PREFIX_BEFORE_ATTEMPTS_LEN: usize = 195;
#[cfg(any(test, feature = "protocol-research"))]
const SUFFIX_AFTER_ATTEMPTS_LEN: usize = 274;
const CAMERA_LANES: usize = 10;
#[cfg(any(test, feature = "protocol-research"))]
const OPTIONAL_FRAME_LEN: usize = 1 + 32 + 32;

/// Local resource limits for decoding an untrusted canonical tick payload.
/// These do not change which byte strings are canonical; exceeding them is a bounded-verifier
/// refusal rather than a corruption verdict.
#[cfg(any(test, feature = "protocol-research"))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TickDecodeLimits {
    pub max_payload_bytes: usize,
    pub max_attempts: usize,
}

#[cfg(any(test, feature = "protocol-research"))]
impl Default for TickDecodeLimits {
    fn default() -> Self {
        Self {
            max_payload_bytes: 16 * 1024 * 1024,
            max_attempts: 1_000_000,
        }
    }
}

/// The ten Boolean inputs and entry-state switches consumed by one fixed tick.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TickInputFlags {
    bits: u16,
}

impl TickInputFlags {
    pub fn from_bits(bits: u16) -> Result<Self, TickProtocolError> {
        if bits & !INPUT_FLAG_MASK != 0 {
            return Err(TickProtocolError::ReservedInputFlags { bits });
        }
        Ok(Self { bits })
    }

    #[allow(clippy::too_many_arguments)]
    pub const fn new(
        forward: bool,
        backward: bool,
        left: bool,
        right: bool,
        up: bool,
        down: bool,
        fit_world: bool,
        game_dirty: bool,
        sidebar_dirty: bool,
        sidebar_visible: bool,
        viewport_refit: bool,
    ) -> Self {
        Self {
            bits: (forward as u16)
                | ((backward as u16) << 1)
                | ((left as u16) << 2)
                | ((right as u16) << 3)
                | ((up as u16) << 4)
                | ((down as u16) << 5)
                | ((fit_world as u16) << 6)
                | ((game_dirty as u16) << 7)
                | ((sidebar_dirty as u16) << 8)
                | ((sidebar_visible as u16) << 9)
                | ((viewport_refit as u16) << 10),
        }
    }

    pub const fn bits(self) -> u16 {
        self.bits
    }

    pub const fn forward(self) -> bool {
        self.bits & (1 << 0) != 0
    }

    pub const fn backward(self) -> bool {
        self.bits & (1 << 1) != 0
    }

    pub const fn left(self) -> bool {
        self.bits & (1 << 2) != 0
    }

    pub const fn right(self) -> bool {
        self.bits & (1 << 3) != 0
    }

    pub const fn up(self) -> bool {
        self.bits & (1 << 4) != 0
    }

    pub const fn down(self) -> bool {
        self.bits & (1 << 5) != 0
    }

    pub const fn fit_world(self) -> bool {
        self.bits & (1 << 6) != 0
    }

    pub const fn game_dirty(self) -> bool {
        self.bits & (1 << 7) != 0
    }

    pub const fn sidebar_dirty(self) -> bool {
        self.bits & (1 << 8) != 0
    }

    pub const fn sidebar_visible(self) -> bool {
        self.bits & (1 << 9) != 0
    }

    pub const fn viewport_refit(self) -> bool {
        self.bits & (1 << 10) != 0
    }
}

/// Exact non-edit controls and viewport state consumed at tick entry.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TickInputState {
    flags: TickInputFlags,
    mouse_x_bits: u64,
    mouse_y_bits: u64,
    wheel_bits: u64,
    simulation_width: u32,
    simulation_height: u32,
    output_width: u32,
    output_height: u32,
}

impl TickInputState {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        flags: TickInputFlags,
        mouse_x: f64,
        mouse_y: f64,
        wheel: f64,
        simulation_width: u32,
        simulation_height: u32,
        output_width: u32,
        output_height: u32,
    ) -> Result<Self, TickProtocolError> {
        Self::from_bits(
            flags,
            mouse_x.to_bits(),
            mouse_y.to_bits(),
            wheel.to_bits(),
            simulation_width,
            simulation_height,
            output_width,
            output_height,
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn from_bits(
        flags: TickInputFlags,
        mouse_x_bits: u64,
        mouse_y_bits: u64,
        wheel_bits: u64,
        simulation_width: u32,
        simulation_height: u32,
        output_width: u32,
        output_height: u32,
    ) -> Result<Self, TickProtocolError> {
        for (field, bits) in [
            ("mouse_x", mouse_x_bits),
            ("mouse_y", mouse_y_bits),
            ("wheel", wheel_bits),
        ] {
            if !f64::from_bits(bits).is_finite() {
                return Err(TickProtocolError::NonFiniteFloat { field, bits });
            }
        }
        for (field, value) in [
            ("simulation_width", simulation_width),
            ("simulation_height", simulation_height),
            ("output_width", output_width),
            ("output_height", output_height),
        ] {
            if value == 0 {
                return Err(TickProtocolError::ZeroDimension { field });
            }
        }
        if output_width < simulation_width {
            return Err(TickProtocolError::OutputNarrowerThanSimulation {
                simulation_width,
                output_width,
            });
        }
        if output_height != simulation_height {
            return Err(TickProtocolError::OutputHeightMismatch {
                simulation_height,
                output_height,
            });
        }
        Ok(Self {
            flags,
            mouse_x_bits,
            mouse_y_bits,
            wheel_bits,
            simulation_width,
            simulation_height,
            output_width,
            output_height,
        })
    }

    pub const fn flags(self) -> TickInputFlags {
        self.flags
    }

    pub fn mouse_x(self) -> f64 {
        f64::from_bits(self.mouse_x_bits)
    }

    pub fn mouse_y(self) -> f64 {
        f64::from_bits(self.mouse_y_bits)
    }

    pub fn wheel(self) -> f64 {
        f64::from_bits(self.wheel_bits)
    }

    pub const fn mouse_bits(self) -> [u64; 3] {
        [self.mouse_x_bits, self.mouse_y_bits, self.wheel_bits]
    }

    pub const fn dimensions(self) -> [u32; 4] {
        [
            self.simulation_width,
            self.simulation_height,
            self.output_width,
            self.output_height,
        ]
    }
}

/// Bit-exact post-tick camera state in the same order used by `FrameInput`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TickCameraState([u64; CAMERA_LANES]);

impl TickCameraState {
    pub fn capture(camera: &Camera) -> Self {
        let position = camera.position();
        let anchor = camera.observation_anchor();
        Self([
            position.x.to_bits(),
            position.y.to_bits(),
            position.z.to_bits(),
            camera.yaw_radians().to_bits(),
            camera.pitch_radians().to_bits(),
            anchor.x.to_bits(),
            anchor.y.to_bits(),
            anchor.z.to_bits(),
            camera.observation_distance().to_bits(),
            camera.vertical_fov_radians().to_bits(),
        ])
    }

    pub fn from_bits(bits: [u64; CAMERA_LANES]) -> Result<Self, TickProtocolError> {
        const FIELDS: [&str; CAMERA_LANES] = [
            "camera_position_x",
            "camera_position_y",
            "camera_position_z",
            "camera_yaw",
            "camera_pitch",
            "camera_anchor_x",
            "camera_anchor_y",
            "camera_anchor_z",
            "camera_observation_distance",
            "camera_vertical_fov",
        ];
        for (field, value_bits) in FIELDS.into_iter().zip(bits) {
            if !f64::from_bits(value_bits).is_finite() {
                return Err(TickProtocolError::NonFiniteFloat {
                    field,
                    bits: value_bits,
                });
            }
        }
        let distance = f64::from_bits(bits[8]);
        if !(MIN_OBSERVATION_DISTANCE..=MAX_OBSERVATION_DISTANCE).contains(&distance) {
            return Err(TickProtocolError::InvalidObservationDistance { bits: bits[8] });
        }
        let yaw = f64::from_bits(bits[3]);
        if !(-PI..PI).contains(&yaw) {
            return Err(TickProtocolError::InvalidYaw { bits: bits[3] });
        }
        let pitch = f64::from_bits(bits[4]);
        if !(-MAX_ABS_PITCH_RADIANS..=MAX_ABS_PITCH_RADIANS).contains(&pitch) {
            return Err(TickProtocolError::InvalidPitch { bits: bits[4] });
        }
        let fov = f64::from_bits(bits[9]);
        if !(0.0 < fov && fov < PI) {
            return Err(TickProtocolError::InvalidVerticalFov { bits: bits[9] });
        }
        Ok(Self(bits))
    }

    pub const fn bits(self) -> [u64; CAMERA_LANES] {
        self.0
    }
}

/// One raw edit input and its deterministic disposition during the tick.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TickAttempt {
    AimDeleteUnresolved,
    AimPlaceUnresolved,
    AimDeleteExecuted(Edit),
    AimPlaceRejectedObserver(Edit),
    AimPlaceExecuted(Edit),
    ExactRejectedObserver(Edit),
    ExactExecuted(Edit),
}

impl TickAttempt {
    pub const fn canonical_code(self) -> u8 {
        match self {
            Self::AimDeleteUnresolved => 0,
            Self::AimPlaceUnresolved => 1,
            Self::AimDeleteExecuted(_) => 2,
            Self::AimPlaceRejectedObserver(_) => 3,
            Self::AimPlaceExecuted(_) => 4,
            Self::ExactRejectedObserver(_) => 5,
            Self::ExactExecuted(_) => 6,
        }
    }

    pub const fn edit(self) -> Option<Edit> {
        match self {
            Self::AimDeleteUnresolved | Self::AimPlaceUnresolved => None,
            Self::AimDeleteExecuted(edit)
            | Self::AimPlaceRejectedObserver(edit)
            | Self::AimPlaceExecuted(edit)
            | Self::ExactRejectedObserver(edit)
            | Self::ExactExecuted(edit) => Some(edit),
        }
    }

    pub const fn executed_edit(self) -> Option<Edit> {
        match self {
            Self::AimDeleteExecuted(edit)
            | Self::AimPlaceExecuted(edit)
            | Self::ExactExecuted(edit) => Some(edit),
            Self::AimDeleteUnresolved
            | Self::AimPlaceUnresolved
            | Self::AimPlaceRejectedObserver(_)
            | Self::ExactRejectedObserver(_) => None,
        }
    }

    const fn encoded_len(self) -> usize {
        if self.edit().is_some() {
            1 + EDIT_CANONICAL_LEN
        } else {
            1
        }
    }

    fn validate(self, index: usize) -> Result<(), TickProtocolError> {
        let expected = match self {
            Self::AimDeleteExecuted(_) => Some(Operation::Off),
            Self::AimPlaceRejectedObserver(_) | Self::AimPlaceExecuted(_) => Some(Operation::On),
            _ => None,
        };
        if let (Some(expected), Some(edit)) = (expected, self.edit())
            && edit.operation != expected
        {
            return Err(TickProtocolError::AimOperationMismatch {
                index,
                code: self.canonical_code(),
                expected,
                actual: edit.operation,
            });
        }
        if let Self::ExactRejectedObserver(edit) = self
            && edit.operation == Operation::Off
        {
            return Err(TickProtocolError::ObserverRejectionImpossible {
                index,
                operation: edit.operation,
            });
        }
        Ok(())
    }

    fn encode_with(self, sink: &mut dyn FnMut(&[u8])) {
        sink(&[self.canonical_code()]);
        if let Some(edit) = self.edit() {
            sink(&edit.encode_canonical());
        }
    }
}

/// Exact renderer input/output tag pair produced in one lane of a tick.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TickFrameCommitment {
    input: ContentTag,
    frame: ContentTag,
}

impl TickFrameCommitment {
    pub fn new(input: ContentTag, frame: ContentTag) -> Result<Self, TickProtocolError> {
        require_kind("frame commitment input", input, TagKind::FrameInput)?;
        require_kind("frame commitment frame", frame, TagKind::Frame)?;
        Ok(Self { input, frame })
    }

    pub const fn input(self) -> ContentTag {
        self.input
    }

    pub const fn frame(self) -> ContentTag {
        self.frame
    }
}

/// Canonical, content-addressed result of one fixed simulation tick.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TickOutcome {
    tag: ContentTag,
    parent: Option<ContentTag>,
    tick: u64,
    pre_world: ContentTag,
    pre_history: ContentTag,
    input: TickInputState,
    actions: TickActionLedger,
    attempts: Vec<TickAttempt>,
    post_world: ContentTag,
    post_history: ContentTag,
    post_camera: TickCameraState,
    game: Option<TickFrameCommitment>,
    composite: Option<TickFrameCommitment>,
}

impl TickOutcome {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        parent: Option<ContentTag>,
        tick: u64,
        pre_world: ContentTag,
        pre_history: ContentTag,
        input: TickInputState,
        actions: TickActionLedger,
        attempts: impl IntoIterator<Item = TickAttempt>,
        post_world: ContentTag,
        post_history: ContentTag,
        post_camera: TickCameraState,
        game: Option<TickFrameCommitment>,
        composite: Option<TickFrameCommitment>,
    ) -> Result<Self, TickProtocolError> {
        if parent.is_none() != (tick == 0) {
            return Err(TickProtocolError::ParentTickMismatch {
                tick,
                has_parent: parent.is_some(),
            });
        }
        if let Some(parent) = parent {
            require_kind("tick parent", parent, TagKind::TickOutcome)?;
        }
        require_kind("pre-tick world", pre_world, TagKind::World)?;
        require_kind("pre-tick history", pre_history, TagKind::Receipt)?;
        require_kind("post-tick world", post_world, TagKind::World)?;
        require_kind("post-tick history", post_history, TagKind::Receipt)?;
        let attempts: Vec<_> = attempts.into_iter().collect();
        for (index, attempt) in attempts.iter().copied().enumerate() {
            attempt.validate(index)?;
        }
        canonical_payload_len(&attempts)?;
        actions.validate(&attempts)?;
        validate_frame_presence(input, game, composite)?;
        validate_world_frame_presence(pre_world, post_world, game)?;
        validate_history_execution(pre_history, post_history, &attempts)?;
        validate_world_history_progress(pre_world, post_world, pre_history, post_history)?;
        let mut outcome = Self {
            tag: ContentTag::from_digest(TagKind::TickOutcome, [0; 32]),
            parent,
            tick,
            pre_world,
            pre_history,
            input,
            actions,
            attempts,
            post_world,
            post_history,
            post_camera,
            game,
            composite,
        };
        outcome.tag = outcome.recompute_tag();
        Ok(outcome)
    }

    /// What this tick consumed, by lane.
    pub const fn actions(&self) -> TickActionLedger {
        self.actions
    }

    /// Builds an outcome whose ledger is the minimal one consistent with its
    /// attempts: exactly the edits attempted were submitted, and no camera or
    /// control action was.
    ///
    /// This is a claim, not a default. Anything exercising the control lanes --
    /// a sidebar toggle, a resize, a motion clear -- must state its ledger to
    /// [`TickOutcome::new`], because those are precisely the actions v2 lost.
    #[allow(clippy::too_many_arguments)]
    pub fn with_edit_attempts_only(
        parent: Option<ContentTag>,
        tick: u64,
        pre_world: ContentTag,
        pre_history: ContentTag,
        input: TickInputState,
        attempts: impl IntoIterator<Item = TickAttempt>,
        post_world: ContentTag,
        post_history: ContentTag,
        post_camera: TickCameraState,
        game: Option<TickFrameCommitment>,
        composite: Option<TickFrameCommitment>,
    ) -> Result<Self, TickProtocolError> {
        let attempts: Vec<_> = attempts.into_iter().collect();
        let actions = TickActionLedger::for_edit_attempts(&attempts);
        Self::new(
            parent,
            tick,
            pre_world,
            pre_history,
            input,
            actions,
            attempts,
            post_world,
            post_history,
            post_camera,
            game,
            composite,
        )
    }

    pub const fn tag(&self) -> ContentTag {
        self.tag
    }

    pub const fn parent(&self) -> Option<ContentTag> {
        self.parent
    }

    pub const fn tick(&self) -> u64 {
        self.tick
    }

    pub const fn pre_world(&self) -> ContentTag {
        self.pre_world
    }

    pub const fn pre_history(&self) -> ContentTag {
        self.pre_history
    }

    pub const fn input(&self) -> TickInputState {
        self.input
    }

    pub fn attempts(&self) -> &[TickAttempt] {
        &self.attempts
    }

    pub const fn post_world(&self) -> ContentTag {
        self.post_world
    }

    pub const fn post_history(&self) -> ContentTag {
        self.post_history
    }

    pub const fn post_camera(&self) -> TickCameraState {
        self.post_camera
    }

    pub const fn game(&self) -> Option<TickFrameCommitment> {
        self.game
    }

    pub const fn composite(&self) -> Option<TickFrameCommitment> {
        self.composite
    }

    pub const fn rendered(&self) -> bool {
        self.composite.is_some()
    }

    pub fn executed_edits(&self) -> impl Iterator<Item = Edit> + '_ {
        self.attempts
            .iter()
            .filter_map(|attempt| attempt.executed_edit())
    }

    /// Verifies the game-frame input digest relative to supplied renderer and overlay policies.
    ///
    /// Those policy tags are contextual rather than serialized in TickOutcome v3, so the generic
    /// byte codec deliberately does not choose them. The live application supplies the exact
    /// policies used for its renderer invocation before publishing the new outcome head.
    pub fn verify_game_input_binding(
        &self,
        renderer_profile: ContentTag,
        overlay_input: ContentTag,
    ) -> Result<(), TickProtocolError> {
        self.verify()?;
        let Some(game) = self.game else {
            return Ok(());
        };
        let [width, height, _, _] = self.input.dimensions();
        let expected = FrameInput::from_camera_bit_lanes(
            self.post_world,
            self.post_camera.bits(),
            width,
            height,
            renderer_profile,
            overlay_input,
        )
        .map_err(|source| TickProtocolError::FrameInputContext { source })?
        .tag();
        if game.input() != expected {
            return Err(TickProtocolError::GameFrameInputMismatch {
                expected,
                actual: game.input(),
            });
        }
        Ok(())
    }

    pub fn encoded_len(&self) -> usize {
        canonical_payload_len(&self.attempts)
            .expect("an admitted tick outcome has a representable payload length")
    }

    pub fn encode_canonical(&self) -> Vec<u8> {
        let expected = self.encoded_len();
        let mut bytes = Vec::with_capacity(expected);
        self.encode_canonical_into(|chunk| bytes.extend_from_slice(chunk));
        debug_assert_eq!(bytes.len(), expected);
        bytes
    }

    /// Streams the exact canonical outcome bytes without materializing a second payload.
    pub fn encode_canonical_into(&self, mut sink: impl FnMut(&[u8])) {
        encode_fields(self, &mut sink);
    }

    #[cfg(any(test, feature = "protocol-research"))]
    pub fn decode_canonical(bytes: &[u8]) -> Result<Self, TickProtocolError> {
        Self::decode_canonical_with_limits(bytes, TickDecodeLimits::default())
    }

    #[cfg(any(test, feature = "protocol-research"))]
    pub fn decode_canonical_with_limits(
        bytes: &[u8],
        limits: TickDecodeLimits,
    ) -> Result<Self, TickProtocolError> {
        if bytes.len() > limits.max_payload_bytes {
            return Err(TickProtocolError::DecodeLimitExceeded {
                resource: "payload bytes",
                limit: limits.max_payload_bytes,
                actual: bytes.len(),
            });
        }
        if bytes.len() < TICK_OUTCOME_FIXED_PAYLOAD_LEN {
            return Err(TickProtocolError::PayloadTooShort {
                minimum: TICK_OUTCOME_FIXED_PAYLOAD_LEN,
                actual: bytes.len(),
            });
        }
        let parent =
            decode_optional_tag(&bytes[0..33], "tick parent", TagKind::TickOutcome, "parent")?;
        let tick = u64::from_be_bytes(bytes[33..41].try_into().expect("fixed tick lane"));
        let pre_world = digest_tag(TagKind::World, &bytes[41..73]);
        let pre_history = digest_tag(TagKind::Receipt, &bytes[73..105]);
        let flags = TickInputFlags::from_bits(u16::from_be_bytes(
            bytes[105..107].try_into().expect("fixed flags lane"),
        ))?;
        let mouse_x_bits = u64::from_be_bytes(bytes[107..115].try_into().expect("fixed f64 lane"));
        let mouse_y_bits = u64::from_be_bytes(bytes[115..123].try_into().expect("fixed f64 lane"));
        let wheel_bits = u64::from_be_bytes(bytes[123..131].try_into().expect("fixed f64 lane"));
        let simulation_width =
            u32::from_be_bytes(bytes[131..135].try_into().expect("fixed dimension lane"));
        let simulation_height =
            u32::from_be_bytes(bytes[135..139].try_into().expect("fixed dimension lane"));
        let output_width =
            u32::from_be_bytes(bytes[139..143].try_into().expect("fixed dimension lane"));
        let output_height =
            u32::from_be_bytes(bytes[143..147].try_into().expect("fixed dimension lane"));
        let input = TickInputState::from_bits(
            flags,
            mouse_x_bits,
            mouse_y_bits,
            wheel_bits,
            simulation_width,
            simulation_height,
            output_width,
            output_height,
        )?;
        let mut ledger = TickActionLedger::empty();
        for (index, lane) in ledger.lanes.iter_mut().enumerate() {
            let start = 147 + index * 4;
            *lane = u32::from_be_bytes(
                bytes[start..start + 4]
                    .try_into()
                    .expect("fixed action lane"),
            );
        }
        let attempt_count_u64 =
            u64::from_be_bytes(bytes[187..195].try_into().expect("fixed count lane"));
        let attempt_count = usize::try_from(attempt_count_u64)
            .map_err(|_| TickProtocolError::PayloadLengthOverflow)?;
        if attempt_count > limits.max_attempts {
            return Err(TickProtocolError::DecodeLimitExceeded {
                resource: "attempts",
                limit: limits.max_attempts,
                actual: attempt_count,
            });
        }
        let maximum_count = bytes
            .len()
            .saturating_sub(PREFIX_BEFORE_ATTEMPTS_LEN + SUFFIX_AFTER_ATTEMPTS_LEN);
        if attempt_count > maximum_count {
            return Err(TickProtocolError::AttemptCountExceedsPayload {
                declared: attempt_count_u64,
                maximum: maximum_count,
            });
        }

        let mut attempts = Vec::new();
        attempts.try_reserve_exact(attempt_count).map_err(|_| {
            TickProtocolError::AttemptAllocationFailed {
                requested: attempt_count,
            }
        })?;
        let mut cursor = PREFIX_BEFORE_ATTEMPTS_LEN;
        for index in 0..attempt_count {
            let code = bytes[cursor];
            cursor += 1;
            let attempt = match code {
                0 => TickAttempt::AimDeleteUnresolved,
                1 => TickAttempt::AimPlaceUnresolved,
                2..=6 => {
                    let end = cursor
                        .checked_add(EDIT_CANONICAL_LEN)
                        .ok_or(TickProtocolError::PayloadLengthOverflow)?;
                    if end > bytes.len().saturating_sub(SUFFIX_AFTER_ATTEMPTS_LEN) {
                        return Err(TickProtocolError::TruncatedAttempt { index, code });
                    }
                    let edit = Edit::decode_canonical(&bytes[cursor..end])
                        .map_err(|source| TickProtocolError::Edit { index, source })?;
                    cursor = end;
                    match code {
                        2 => TickAttempt::AimDeleteExecuted(edit),
                        3 => TickAttempt::AimPlaceRejectedObserver(edit),
                        4 => TickAttempt::AimPlaceExecuted(edit),
                        5 => TickAttempt::ExactRejectedObserver(edit),
                        6 => TickAttempt::ExactExecuted(edit),
                        _ => unreachable!("range narrowed above"),
                    }
                }
                _ => return Err(TickProtocolError::UnknownAttemptCode { index, code }),
            };
            attempt.validate(index)?;
            attempts.push(attempt);
        }
        let expected = cursor
            .checked_add(SUFFIX_AFTER_ATTEMPTS_LEN)
            .ok_or(TickProtocolError::PayloadLengthOverflow)?;
        if bytes.len() != expected {
            return Err(TickProtocolError::WrongPayloadLength {
                expected,
                actual: bytes.len(),
            });
        }

        let post_world = digest_tag(TagKind::World, &bytes[cursor..cursor + 32]);
        cursor += 32;
        let post_history = digest_tag(TagKind::Receipt, &bytes[cursor..cursor + 32]);
        cursor += 32;
        let mut camera_bits = [0_u64; CAMERA_LANES];
        for lane in &mut camera_bits {
            *lane = u64::from_be_bytes(
                bytes[cursor..cursor + 8]
                    .try_into()
                    .expect("camera lane length checked"),
            );
            cursor += 8;
        }
        let post_camera = TickCameraState::from_bits(camera_bits)?;
        let game = decode_optional_frame(&bytes[cursor..cursor + OPTIONAL_FRAME_LEN], "game")?;
        cursor += OPTIONAL_FRAME_LEN;
        let composite =
            decode_optional_frame(&bytes[cursor..cursor + OPTIONAL_FRAME_LEN], "composite")?;

        let outcome = Self::new(
            parent,
            tick,
            pre_world,
            pre_history,
            input,
            ledger,
            attempts,
            post_world,
            post_history,
            post_camera,
            game,
            composite,
        )?;
        if !outcome.encoding_matches(bytes) {
            return Err(TickProtocolError::NonCanonicalEncoding);
        }
        Ok(outcome)
    }

    pub fn verify(&self) -> Result<(), TickProtocolError> {
        if self.parent.is_none() != (self.tick == 0) {
            return Err(TickProtocolError::ParentTickMismatch {
                tick: self.tick,
                has_parent: self.parent.is_some(),
            });
        }
        if let Some(parent) = self.parent {
            require_kind("tick parent", parent, TagKind::TickOutcome)?;
        }
        require_kind("pre-tick world", self.pre_world, TagKind::World)?;
        require_kind("pre-tick history", self.pre_history, TagKind::Receipt)?;
        require_kind("post-tick world", self.post_world, TagKind::World)?;
        require_kind("post-tick history", self.post_history, TagKind::Receipt)?;
        for (index, attempt) in self.attempts.iter().copied().enumerate() {
            attempt.validate(index)?;
        }
        canonical_payload_len(&self.attempts)?;
        self.actions.validate(&self.attempts)?;
        validate_frame_presence(self.input, self.game, self.composite)?;
        validate_world_frame_presence(self.pre_world, self.post_world, self.game)?;
        validate_history_execution(self.pre_history, self.post_history, &self.attempts)?;
        validate_world_history_progress(
            self.pre_world,
            self.post_world,
            self.pre_history,
            self.post_history,
        )?;
        if self.tag != self.recompute_tag() {
            return Err(TickProtocolError::OutcomeTagMismatch);
        }
        Ok(())
    }

    fn recompute_tag(&self) -> ContentTag {
        hash_canonical(
            TagKind::TickOutcome,
            TICK_OUTCOME_SCHEMA,
            self.encoded_len() as u64,
            |sink| encode_fields(self, sink),
        )
        .expect("an admitted tick outcome emits its declared length")
    }

    #[cfg(any(test, feature = "protocol-research"))]
    fn encoding_matches(&self, expected: &[u8]) -> bool {
        let mut offset = 0_usize;
        let mut matches = true;
        encode_fields(self, &mut |chunk| {
            let Some(end) = offset.checked_add(chunk.len()) else {
                matches = false;
                return;
            };
            if end > expected.len() || expected[offset..end] != *chunk {
                matches = false;
            }
            offset = end;
        });
        matches && offset == expected.len()
    }
}

/// Verifies the content-chain invariants visible without replaying the transition implementation.
/// Tick zero still requires the separately fixed genesis camera/context for semantic admission.
pub fn verify_successor(
    previous: Option<&TickOutcome>,
    next: &TickOutcome,
) -> Result<(), TickProtocolError> {
    next.verify()?;
    match previous {
        None => {
            if next.parent.is_some() || next.tick != 0 {
                return Err(TickProtocolError::ParentTickMismatch {
                    tick: next.tick,
                    has_parent: next.parent.is_some(),
                });
            }
        }
        Some(previous) => {
            previous.verify()?;
            if next.parent != Some(previous.tag) {
                return Err(TickProtocolError::ParentMismatch {
                    expected: Some(previous.tag),
                    actual: next.parent,
                });
            }
            let expected = previous
                .tick
                .checked_add(1)
                .ok_or(TickProtocolError::TickOverflow)?;
            if next.tick != expected {
                return Err(TickProtocolError::NonContiguousTick {
                    previous: previous.tick,
                    next: next.tick,
                });
            }
            if next.pre_world != previous.post_world {
                return Err(TickProtocolError::WorldDiscontinuity {
                    expected: previous.post_world,
                    actual: next.pre_world,
                });
            }
            if next.pre_history != previous.post_history {
                return Err(TickProtocolError::HistoryDiscontinuity {
                    expected: previous.post_history,
                    actual: next.pre_history,
                });
            }
            if next.post_camera != previous.post_camera && next.game.is_none() {
                return Err(TickProtocolError::CameraChangeWithoutGameFrame);
            }
        }
    }
    Ok(())
}

fn canonical_payload_len(attempts: &[TickAttempt]) -> Result<usize, TickProtocolError> {
    attempts
        .iter()
        .try_fold(TICK_OUTCOME_FIXED_PAYLOAD_LEN, |length, attempt| {
            length
                .checked_add(attempt.encoded_len())
                .ok_or(TickProtocolError::PayloadLengthOverflow)
        })
}

fn encode_fields(outcome: &TickOutcome, sink: &mut dyn FnMut(&[u8])) {
    encode_optional_tag(outcome.parent, sink);
    sink(&outcome.tick.to_be_bytes());
    sink(&outcome.pre_world.digest());
    sink(&outcome.pre_history.digest());
    sink(&outcome.input.flags.bits().to_be_bytes());
    for bits in outcome.input.mouse_bits() {
        sink(&bits.to_be_bytes());
    }
    for dimension in outcome.input.dimensions() {
        sink(&dimension.to_be_bytes());
    }
    outcome.actions.encode_with(sink);
    sink(&(outcome.attempts.len() as u64).to_be_bytes());
    for attempt in outcome.attempts.iter().copied() {
        attempt.encode_with(sink);
    }
    sink(&outcome.post_world.digest());
    sink(&outcome.post_history.digest());
    for bits in outcome.post_camera.bits() {
        sink(&bits.to_be_bytes());
    }
    encode_optional_frame(outcome.game, sink);
    encode_optional_frame(outcome.composite, sink);
}

fn encode_optional_tag(tag: Option<ContentTag>, sink: &mut dyn FnMut(&[u8])) {
    match tag {
        Some(tag) => {
            sink(&[1]);
            sink(&tag.digest());
        }
        None => {
            sink(&[0]);
            sink(&[0; 32]);
        }
    }
}

fn encode_optional_frame(frame: Option<TickFrameCommitment>, sink: &mut dyn FnMut(&[u8])) {
    match frame {
        Some(frame) => {
            sink(&[1]);
            sink(&frame.input.digest());
            sink(&frame.frame.digest());
        }
        None => {
            sink(&[0]);
            sink(&[0; 32]);
            sink(&[0; 32]);
        }
    }
}

#[cfg(any(test, feature = "protocol-research"))]
fn digest_tag(kind: TagKind, bytes: &[u8]) -> ContentTag {
    ContentTag::from_digest(
        kind,
        bytes.try_into().expect("digest slice has fixed length"),
    )
}

#[cfg(any(test, feature = "protocol-research"))]
fn decode_optional_tag(
    bytes: &[u8],
    field: &'static str,
    kind: TagKind,
    padding_field: &'static str,
) -> Result<Option<ContentTag>, TickProtocolError> {
    match bytes[0] {
        0 => {
            if bytes[1..] != [0; 32] {
                return Err(TickProtocolError::NonZeroAbsentPadding {
                    field: padding_field,
                });
            }
            Ok(None)
        }
        1 => {
            let tag = digest_tag(kind, &bytes[1..]);
            require_kind(field, tag, kind)?;
            Ok(Some(tag))
        }
        marker => Err(TickProtocolError::UnknownOptionMarker {
            field: padding_field,
            marker,
        }),
    }
}

#[cfg(any(test, feature = "protocol-research"))]
fn decode_optional_frame(
    bytes: &[u8],
    field: &'static str,
) -> Result<Option<TickFrameCommitment>, TickProtocolError> {
    match bytes[0] {
        0 => {
            if bytes[1..] != [0; 64] {
                return Err(TickProtocolError::NonZeroAbsentPadding { field });
            }
            Ok(None)
        }
        1 => TickFrameCommitment::new(
            digest_tag(TagKind::FrameInput, &bytes[1..33]),
            digest_tag(TagKind::Frame, &bytes[33..65]),
        )
        .map(Some),
        marker => Err(TickProtocolError::UnknownOptionMarker { field, marker }),
    }
}

fn require_kind(
    field: &'static str,
    tag: ContentTag,
    expected: TagKind,
) -> Result<(), TickProtocolError> {
    if tag.kind() != expected {
        return Err(TickProtocolError::WrongKind {
            field,
            expected,
            actual: tag.kind(),
        });
    }
    Ok(())
}

fn validate_frame_presence(
    input: TickInputState,
    game: Option<TickFrameCommitment>,
    composite: Option<TickFrameCommitment>,
) -> Result<(), TickProtocolError> {
    let flags = input.flags();
    if game.is_none()
        && (flags.game_dirty()
            || flags.fit_world()
            || flags.viewport_refit()
            || input.mouse_x() != 0.0
            || input.mouse_y() != 0.0)
    {
        return Err(TickProtocolError::MissingRequiredGameFrame);
    }
    if game.is_some() && composite.is_none() {
        return Err(TickProtocolError::GameFrameWithoutComposite);
    }
    if flags.sidebar_dirty() && composite.is_none() {
        return Err(TickProtocolError::MissingRequiredCompositeFrame);
    }
    if composite.is_some() && game.is_none() && !flags.sidebar_dirty() {
        return Err(TickProtocolError::UnexpectedCompositeFrame);
    }
    Ok(())
}

fn validate_world_frame_presence(
    pre_world: ContentTag,
    post_world: ContentTag,
    game: Option<TickFrameCommitment>,
) -> Result<(), TickProtocolError> {
    if pre_world != post_world && game.is_none() {
        return Err(TickProtocolError::WorldChangeWithoutGameFrame);
    }
    Ok(())
}

fn validate_history_execution(
    pre_history: ContentTag,
    post_history: ContentTag,
    attempts: &[TickAttempt],
) -> Result<(), TickProtocolError> {
    let has_executed_attempt = attempts
        .iter()
        .any(|attempt| attempt.executed_edit().is_some());
    let history_advanced = pre_history != post_history;
    if has_executed_attempt != history_advanced {
        return Err(TickProtocolError::HistoryExecutionMismatch {
            has_executed_attempt,
            history_advanced,
        });
    }
    Ok(())
}

fn validate_world_history_progress(
    pre_world: ContentTag,
    post_world: ContentTag,
    pre_history: ContentTag,
    post_history: ContentTag,
) -> Result<(), TickProtocolError> {
    if pre_world != post_world && pre_history == post_history {
        return Err(TickProtocolError::WorldChangeWithoutHistoryAdvance);
    }
    Ok(())
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TickProtocolError {
    /// An action was counted into a lane that does not exist.
    UnknownActionLane {
        lane: usize,
    },
    /// One tick was offered more accepted actions than it can account for.
    TickActionOverflow {
        limit: u32,
    },
    /// Submitted edits and their ordered dispositions did not reconcile.
    ActionAccountingMismatch {
        lane: usize,
        submitted: u32,
        dispositions: u32,
    },
    WrongKind {
        field: &'static str,
        expected: TagKind,
        actual: TagKind,
    },
    ParentTickMismatch {
        tick: u64,
        has_parent: bool,
    },
    ReservedInputFlags {
        bits: u16,
    },
    NonFiniteFloat {
        field: &'static str,
        bits: u64,
    },
    InvalidObservationDistance {
        bits: u64,
    },
    InvalidYaw {
        bits: u64,
    },
    InvalidPitch {
        bits: u64,
    },
    InvalidVerticalFov {
        bits: u64,
    },
    ZeroDimension {
        field: &'static str,
    },
    OutputNarrowerThanSimulation {
        simulation_width: u32,
        output_width: u32,
    },
    OutputHeightMismatch {
        simulation_height: u32,
        output_height: u32,
    },
    AimOperationMismatch {
        index: usize,
        code: u8,
        expected: Operation,
        actual: Operation,
    },
    ObserverRejectionImpossible {
        index: usize,
        operation: Operation,
    },
    MissingRequiredGameFrame,
    WorldChangeWithoutGameFrame,
    HistoryExecutionMismatch {
        has_executed_attempt: bool,
        history_advanced: bool,
    },
    WorldChangeWithoutHistoryAdvance,
    GameFrameWithoutComposite,
    MissingRequiredCompositeFrame,
    UnexpectedCompositeFrame,
    PayloadLengthOverflow,
    DecodeLimitExceeded {
        resource: &'static str,
        limit: usize,
        actual: usize,
    },
    AttemptAllocationFailed {
        requested: usize,
    },
    PayloadTooShort {
        minimum: usize,
        actual: usize,
    },
    AttemptCountExceedsPayload {
        declared: u64,
        maximum: usize,
    },
    TruncatedAttempt {
        index: usize,
        code: u8,
    },
    UnknownAttemptCode {
        index: usize,
        code: u8,
    },
    Edit {
        index: usize,
        source: EditCodecError,
    },
    WrongPayloadLength {
        expected: usize,
        actual: usize,
    },
    UnknownOptionMarker {
        field: &'static str,
        marker: u8,
    },
    NonZeroAbsentPadding {
        field: &'static str,
    },
    NonCanonicalEncoding,
    OutcomeTagMismatch,
    ParentMismatch {
        expected: Option<ContentTag>,
        actual: Option<ContentTag>,
    },
    TickOverflow,
    NonContiguousTick {
        previous: u64,
        next: u64,
    },
    WorldDiscontinuity {
        expected: ContentTag,
        actual: ContentTag,
    },
    HistoryDiscontinuity {
        expected: ContentTag,
        actual: ContentTag,
    },
    CameraChangeWithoutGameFrame,
    FrameInputContext {
        source: FrameProtocolError,
    },
    GameFrameInputMismatch {
        expected: ContentTag,
        actual: ContentTag,
    },
}

impl fmt::Display for TickProtocolError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownActionLane { lane } => {
                write!(
                    formatter,
                    "accepted action counted into unknown lane {lane}"
                )
            }
            Self::TickActionOverflow { limit } => write!(
                formatter,
                "one tick cannot account for more than {limit} accepted actions"
            ),
            Self::ActionAccountingMismatch {
                lane,
                submitted,
                dispositions,
            } => write!(
                formatter,
                "lane {lane} counted {submitted} submitted edits against {dispositions} ordered dispositions"
            ),
            Self::WrongKind {
                field,
                expected,
                actual,
            } => write!(
                formatter,
                "{field} must be {}, received {}",
                expected.name(),
                actual.name()
            ),
            Self::ParentTickMismatch { tick, has_parent } => write!(
                formatter,
                "tick {tick} has_parent={has_parent}; only tick zero may omit its parent"
            ),
            Self::ReservedInputFlags { bits } => {
                write!(formatter, "tick input flags use reserved bits: {bits:#06x}")
            }
            Self::NonFiniteFloat { field, bits } => {
                write!(formatter, "{field} has non-finite f64 bits {bits:#018x}")
            }
            Self::InvalidObservationDistance { bits } => write!(
                formatter,
                "camera observation distance is outside the supported range: {bits:#018x}"
            ),
            Self::InvalidYaw { bits } => {
                write!(
                    formatter,
                    "camera yaw is not wrapped to [-pi, pi): {bits:#018x}"
                )
            }
            Self::InvalidPitch { bits } => write!(
                formatter,
                "camera pitch is outside its clamped range: {bits:#018x}"
            ),
            Self::InvalidVerticalFov { bits } => write!(
                formatter,
                "camera vertical FOV must be strictly between zero and pi: {bits:#018x}"
            ),
            Self::ZeroDimension { field } => write!(formatter, "{field} cannot be zero"),
            Self::OutputNarrowerThanSimulation {
                simulation_width,
                output_width,
            } => write!(
                formatter,
                "output width {output_width} is narrower than simulation width {simulation_width}"
            ),
            Self::OutputHeightMismatch {
                simulation_height,
                output_height,
            } => write!(
                formatter,
                "output height {output_height} differs from simulation height {simulation_height}"
            ),
            Self::AimOperationMismatch {
                index,
                code,
                expected,
                actual,
            } => write!(
                formatter,
                "attempt {index} code {code} requires {expected}, received {actual}"
            ),
            Self::ObserverRejectionImpossible { index, operation } => write!(
                formatter,
                "attempt {index} cannot reject {operation} for enclosing the observer"
            ),
            Self::MissingRequiredGameFrame => formatter.write_str(
                "tick inputs force a game render but the game frame commitment is absent",
            ),
            Self::WorldChangeWithoutGameFrame => formatter
                .write_str("a changed world requires a game frame commitment from the same tick"),
            Self::HistoryExecutionMismatch {
                has_executed_attempt,
                history_advanced,
            } => write!(
                formatter,
                "executed-attempt presence ({has_executed_attempt}) does not match history advancement ({history_advanced})"
            ),
            Self::WorldChangeWithoutHistoryAdvance => formatter
                .write_str("a changed world requires the coordinate-history head to advance"),
            Self::GameFrameWithoutComposite => {
                formatter.write_str("a game frame requires a composite frame from the same tick")
            }
            Self::MissingRequiredCompositeFrame => {
                formatter.write_str("sidebar dirty state requires a composite frame commitment")
            }
            Self::UnexpectedCompositeFrame => formatter
                .write_str("a composite frame requires a game frame or sidebar dirty state"),
            Self::PayloadLengthOverflow => {
                formatter.write_str("tick outcome payload length is not representable")
            }
            Self::DecodeLimitExceeded {
                resource,
                limit,
                actual,
            } => write!(
                formatter,
                "tick outcome {resource} exceeds local decode limit {limit}: {actual}"
            ),
            Self::AttemptAllocationFailed { requested } => write!(
                formatter,
                "could not allocate storage for {requested} decoded attempts"
            ),
            Self::PayloadTooShort { minimum, actual } => write!(
                formatter,
                "tick outcome requires at least {minimum} bytes, found {actual}"
            ),
            Self::AttemptCountExceedsPayload { declared, maximum } => write!(
                formatter,
                "tick outcome declares {declared} attempts but at most {maximum} fit"
            ),
            Self::TruncatedAttempt { index, code } => {
                write!(
                    formatter,
                    "attempt {index} code {code} has a truncated edit"
                )
            }
            Self::UnknownAttemptCode { index, code } => {
                write!(formatter, "attempt {index} uses undefined code {code}")
            }
            Self::Edit { index, source } => {
                write!(
                    formatter,
                    "attempt {index} contains an invalid edit: {source}"
                )
            }
            Self::WrongPayloadLength { expected, actual } => write!(
                formatter,
                "tick outcome requires {expected} bytes, found {actual}"
            ),
            Self::UnknownOptionMarker { field, marker } => {
                write!(formatter, "{field} uses undefined option marker {marker}")
            }
            Self::NonZeroAbsentPadding { field } => {
                write!(formatter, "absent {field} has nonzero canonical padding")
            }
            Self::NonCanonicalEncoding => {
                formatter.write_str("tick outcome does not re-encode canonically")
            }
            Self::OutcomeTagMismatch => {
                formatter.write_str("tick outcome tag does not match its canonical fields")
            }
            Self::ParentMismatch { expected, actual } => {
                write!(
                    formatter,
                    "tick outcome parent mismatch: expected {expected:?}, received {actual:?}"
                )
            }
            Self::TickOverflow => formatter.write_str("tick outcome ordinal overflowed u64"),
            Self::NonContiguousTick { previous, next } => write!(
                formatter,
                "tick outcome ordinal is not contiguous: {previous} followed by {next}"
            ),
            Self::WorldDiscontinuity { expected, actual } => write!(
                formatter,
                "tick pre-world {actual} does not equal predecessor post-world {expected}"
            ),
            Self::HistoryDiscontinuity { expected, actual } => write!(
                formatter,
                "tick pre-history {actual} does not equal predecessor post-history {expected}"
            ),
            Self::CameraChangeWithoutGameFrame => formatter.write_str(
                "a changed post-camera requires a game frame commitment from the same tick",
            ),
            Self::FrameInputContext { source } => {
                write!(formatter, "invalid game-frame input context: {source}")
            }
            Self::GameFrameInputMismatch { expected, actual } => write!(
                formatter,
                "game-frame input mismatch: expected {expected}, received {actual}"
            ),
        }
    }
}

impl std::error::Error for TickProtocolError {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::encode_canonical_preimage;
    use crate::world::Coord;
    use sha2::{Digest, Sha256};

    fn tag(kind: TagKind, byte: u8) -> ContentTag {
        ContentTag::from_digest(kind, [byte; 32])
    }

    fn sequence_tag(kind: TagKind, start: u8) -> ContentTag {
        ContentTag::from_digest(
            kind,
            std::array::from_fn(|index| start.wrapping_add(index as u8)),
        )
    }

    fn sequence_frame(input_start: u8, frame_start: u8) -> TickFrameCommitment {
        TickFrameCommitment::new(
            sequence_tag(TagKind::FrameInput, input_start),
            sequence_tag(TagKind::Frame, frame_start),
        )
        .unwrap()
    }

    fn input(flags: u16) -> TickInputState {
        TickInputState::new(
            TickInputFlags::from_bits(flags).unwrap(),
            0.0,
            -0.0,
            1.5,
            801,
            801,
            1201,
            801,
        )
        .unwrap()
    }

    fn camera(seed: f64) -> TickCameraState {
        TickCameraState::from_bits([
            seed.to_bits(),
            (-seed).to_bits(),
            (seed + 1.0).to_bits(),
            0.25_f64.to_bits(),
            (-0.125_f64).to_bits(),
            (seed + 2.0).to_bits(),
            (seed + 3.0).to_bits(),
            (seed + 4.0).to_bits(),
            2.0_f64.to_bits(),
            1.0_f64.to_bits(),
        ])
        .unwrap()
    }

    fn frame(seed: u8) -> TickFrameCommitment {
        TickFrameCommitment::new(
            tag(TagKind::FrameInput, seed),
            tag(TagKind::Frame, seed.wrapping_add(1)),
        )
        .unwrap()
    }

    /// The gap v3 exists to close.
    ///
    /// Toggling the sidebar, resizing, and clearing motion are accepted actions
    /// applied outside the edit fold. Under v2 a tick that consumed all three
    /// hashed identically to a tick where nothing was submitted at all, because
    /// nothing in the payload counted them. They must now separate.
    #[test]
    fn control_actions_are_visible_in_the_outcome_tag() {
        let idle = outcome(None, 0, 0x10, 0x20, 0x10, 0x20, Vec::new());

        let mut ledger = TickActionLedger::empty();
        ledger.record(LANE_TOGGLE_SIDEBAR).unwrap();
        ledger.record(LANE_RESIZE).unwrap();
        ledger.record(LANE_CLEAR_MOTION).unwrap();
        let controlled = TickOutcome::new(
            None,
            0,
            tag(TagKind::World, 0x10),
            tag(TagKind::Receipt, 0x20),
            input(0),
            ledger,
            Vec::new(),
            tag(TagKind::World, 0x10),
            tag(TagKind::Receipt, 0x20),
            camera(1.0),
            None,
            None,
        )
        .unwrap();

        assert_eq!(controlled.actions().control_lane_total(), 3);
        assert_eq!(controlled.actions().submitted(), 3);
        assert_eq!(idle.actions().submitted(), 0);
        assert_ne!(
            idle.tag(),
            controlled.tag(),
            "three consumed control actions must not hash as an idle tick"
        );
        assert_eq!(
            TickOutcome::decode_canonical(&controlled.encode_canonical()),
            Ok(controlled)
        );
    }

    /// Every submitted edit has one ordered disposition, and every disposition
    /// had a submission. A ledger that disagrees is refused at construction and
    /// again at decode, so it cannot be reached by either route.
    #[test]
    fn submitted_edits_must_reconcile_with_their_dispositions() {
        let attempts = vec![TickAttempt::AimDeleteUnresolved];
        let mut short = TickActionLedger::empty();
        short.record(LANE_TOGGLE_SIDEBAR).unwrap(); // a control action, not an edit
        assert_eq!(
            TickOutcome::new(
                None,
                0,
                tag(TagKind::World, 0x10),
                tag(TagKind::Receipt, 0x20),
                input(0),
                short,
                attempts,
                tag(TagKind::World, 0x10),
                tag(TagKind::Receipt, 0x20),
                camera(1.0),
                None,
                None,
            ),
            Err(TickProtocolError::ActionAccountingMismatch {
                lane: LANE_AIM_DELETE,
                submitted: 0,
                dispositions: 1,
            })
        );
    }

    /// A correct total is not a correct account.
    ///
    /// One aim-delete and one aim-place, against a ledger claiming two
    /// aim-deletes: the sums agree and the split is a lie. Checking only
    /// `edit_lane_total()` would admit this, which is why the guard is per-lane.
    #[test]
    fn a_matching_total_cannot_cover_a_wrong_split() {
        let on = Edit::new(Operation::On, Coord::new(1, 2, 3));
        let off = Edit::new(Operation::Off, Coord::new(1, 2, 3));
        let attempts = vec![
            TickAttempt::AimDeleteExecuted(off),
            TickAttempt::AimPlaceExecuted(on),
        ];

        let mut lying = TickActionLedger::empty();
        lying.record(LANE_AIM_DELETE).unwrap();
        lying.record(LANE_AIM_DELETE).unwrap();
        assert_eq!(
            lying.edit_lane_total() as usize,
            attempts.len(),
            "the total matches, which is the whole point of this control"
        );

        assert_eq!(
            TickOutcome::new(
                None,
                0,
                tag(TagKind::World, 0x10),
                tag(TagKind::Receipt, 0x20),
                input(0),
                lying,
                attempts.clone(),
                tag(TagKind::World, 0x30),
                tag(TagKind::Receipt, 0x40),
                camera(1.0),
                Some(frame(0xe0)),
                Some(frame(0xf0)),
            ),
            Err(TickProtocolError::ActionAccountingMismatch {
                lane: LANE_AIM_DELETE,
                submitted: 2,
                dispositions: 1,
            })
        );

        // The honest ledger for the same dispositions is admitted.
        let honest = TickActionLedger::for_edit_attempts(&attempts);
        assert_eq!(honest.lane(LANE_AIM_DELETE), 1);
        assert_eq!(honest.lane(LANE_AIM_PLACE), 1);
    }

    /// Over-limit fails closed: the counter is refused and nothing is recorded.
    #[test]
    fn one_tick_cannot_account_for_more_actions_than_its_limit() {
        let mut ledger = TickActionLedger::empty();
        for _ in 0..MAX_ACTIONS_PER_TICK {
            ledger.record(LANE_MOVE_EDGE).unwrap();
        }
        assert_eq!(ledger.submitted(), u64::from(MAX_ACTIONS_PER_TICK));
        assert_eq!(
            ledger.record(LANE_MOVE_EDGE),
            Err(TickActionError::Overflow {
                limit: MAX_ACTIONS_PER_TICK
            })
        );
        assert_eq!(
            ledger.submitted(),
            u64::from(MAX_ACTIONS_PER_TICK),
            "a refused action must not be counted"
        );
        assert_eq!(
            ledger.record(ACTION_LANE_COUNT),
            Err(TickActionError::UnknownLane {
                lane: ACTION_LANE_COUNT
            })
        );
    }

    fn outcome(
        parent: Option<ContentTag>,
        tick: u64,
        pre_world: u8,
        pre_history: u8,
        post_world: u8,
        post_history: u8,
        attempts: Vec<TickAttempt>,
    ) -> TickOutcome {
        let changed_world = pre_world != post_world;
        TickOutcome::with_edit_attempts_only(
            parent,
            tick,
            tag(TagKind::World, pre_world),
            tag(TagKind::Receipt, pre_history),
            input(0),
            attempts,
            tag(TagKind::World, post_world),
            tag(TagKind::Receipt, post_history),
            camera(1.0),
            changed_world.then(|| frame(0xe0)),
            changed_world.then(|| frame(0xf0)),
        )
        .unwrap()
    }

    #[test]
    fn every_valid_input_flag_combination_round_trips() {
        for bits in 0..=INPUT_FLAG_MASK {
            let flags = TickInputFlags::from_bits(bits).unwrap();
            assert_eq!(flags.bits(), bits);
            assert_eq!(flags.forward(), bits & (1 << 0) != 0);
            assert_eq!(flags.backward(), bits & (1 << 1) != 0);
            assert_eq!(flags.left(), bits & (1 << 2) != 0);
            assert_eq!(flags.right(), bits & (1 << 3) != 0);
            assert_eq!(flags.up(), bits & (1 << 4) != 0);
            assert_eq!(flags.down(), bits & (1 << 5) != 0);
            assert_eq!(flags.fit_world(), bits & (1 << 6) != 0);
            assert_eq!(flags.game_dirty(), bits & (1 << 7) != 0);
            assert_eq!(flags.sidebar_dirty(), bits & (1 << 8) != 0);
            assert_eq!(flags.sidebar_visible(), bits & (1 << 9) != 0);
            assert_eq!(flags.viewport_refit(), bits & (1 << 10) != 0);
        }
        assert_eq!(
            TickInputFlags::from_bits(1 << 11),
            Err(TickProtocolError::ReservedInputFlags { bits: 1 << 11 })
        );
    }

    #[test]
    fn all_seven_attempt_variants_round_trip_in_order() {
        let off = Edit::new(Operation::Off, Coord::new(-1, 2, i64::MIN));
        let on = Edit::new(Operation::On, Coord::new(3, -4, i64::MAX));
        let toggle = Edit::new(Operation::Toggle, Coord::new(5, 6, -7));
        let attempts = vec![
            TickAttempt::AimDeleteUnresolved,
            TickAttempt::AimPlaceUnresolved,
            TickAttempt::AimDeleteExecuted(off),
            TickAttempt::AimPlaceRejectedObserver(on),
            TickAttempt::AimPlaceExecuted(on),
            TickAttempt::ExactRejectedObserver(toggle),
            TickAttempt::ExactExecuted(off),
        ];
        let outcome = outcome(None, 0, 0x10, 0x20, 0x30, 0x40, attempts.clone());
        assert_eq!(outcome.encoded_len(), 601);
        assert_eq!(outcome.attempts(), attempts);
        assert_eq!(
            outcome.executed_edits().collect::<Vec<_>>(),
            vec![off, on, off]
        );
        let bytes = outcome.encode_canonical();
        assert_eq!(TickOutcome::decode_canonical(&bytes), Ok(outcome));
    }

    #[test]
    fn aim_attempts_refuse_operations_the_resolver_cannot_emit() {
        let on = Edit::new(Operation::On, Coord::new(1, 2, 3));
        let off = Edit::new(Operation::Off, Coord::new(1, 2, 3));
        let error = TickOutcome::with_edit_attempts_only(
            None,
            0,
            tag(TagKind::World, 1),
            tag(TagKind::Receipt, 2),
            input(0),
            [TickAttempt::AimDeleteExecuted(on)],
            tag(TagKind::World, 3),
            tag(TagKind::Receipt, 4),
            camera(1.0),
            None,
            None,
        )
        .unwrap_err();
        assert!(matches!(
            error,
            TickProtocolError::AimOperationMismatch {
                code: 2,
                expected: Operation::Off,
                actual: Operation::On,
                ..
            }
        ));
        assert!(
            TickOutcome::with_edit_attempts_only(
                None,
                0,
                tag(TagKind::World, 1),
                tag(TagKind::Receipt, 2),
                input(0),
                [TickAttempt::AimPlaceExecuted(off)],
                tag(TagKind::World, 3),
                tag(TagKind::Receipt, 4),
                camera(1.0),
                None,
                None,
            )
            .is_err()
        );
        assert_eq!(
            TickOutcome::with_edit_attempts_only(
                None,
                0,
                tag(TagKind::World, 1),
                tag(TagKind::Receipt, 2),
                input(0),
                [TickAttempt::ExactRejectedObserver(off)],
                tag(TagKind::World, 3),
                tag(TagKind::Receipt, 4),
                camera(1.0),
                None,
                None,
            ),
            Err(TickProtocolError::ObserverRejectionImpossible {
                index: 0,
                operation: Operation::Off,
            })
        );
    }

    #[test]
    fn option_padding_unknown_codes_counts_and_lengths_fail_closed() {
        let base = outcome(None, 0, 1, 2, 1, 2, Vec::new()).encode_canonical();

        let mut bad_parent_padding = base.clone();
        bad_parent_padding[1] = 1;
        assert_eq!(
            TickOutcome::decode_canonical(&bad_parent_padding),
            Err(TickProtocolError::NonZeroAbsentPadding { field: "parent" })
        );

        let mut bad_parent_marker = base.clone();
        bad_parent_marker[0] = 2;
        assert_eq!(
            TickOutcome::decode_canonical(&bad_parent_marker),
            Err(TickProtocolError::UnknownOptionMarker {
                field: "parent",
                marker: 2,
            })
        );

        let mut impossible_count = base.clone();
        impossible_count[187..195].copy_from_slice(&1_u64.to_be_bytes());
        assert!(matches!(
            TickOutcome::decode_canonical(&impossible_count),
            Err(TickProtocolError::AttemptCountExceedsPayload { .. })
        ));

        let mut unknown_code = base.clone();
        unknown_code[187..195].copy_from_slice(&1_u64.to_be_bytes());
        unknown_code.insert(PREFIX_BEFORE_ATTEMPTS_LEN, 7);
        assert_eq!(
            TickOutcome::decode_canonical(&unknown_code),
            Err(TickProtocolError::UnknownAttemptCode { index: 0, code: 7 })
        );

        let mut trailing = base.clone();
        trailing.push(0);
        assert!(matches!(
            TickOutcome::decode_canonical(&trailing),
            Err(TickProtocolError::WrongPayloadLength { .. })
        ));
    }

    #[test]
    fn decoded_action_totals_above_the_tick_limit_fail_closed() {
        let base = outcome(None, 0, 1, 2, 1, 2, Vec::new()).encode_canonical();

        let mut single_over_limit = base.clone();
        single_over_limit[147..151].copy_from_slice(&(MAX_ACTIONS_PER_TICK + 1).to_be_bytes());
        assert_eq!(
            TickOutcome::decode_canonical(&single_over_limit),
            Err(TickProtocolError::TickActionOverflow {
                limit: MAX_ACTIONS_PER_TICK,
            })
        );

        // A mathematical total of 2^32 must not panic in debug or wrap to zero
        // in release before the protocol limit is checked.
        let mut wrapping_total = base;
        wrapping_total[147..151].copy_from_slice(&u32::MAX.to_be_bytes());
        wrapping_total[151..155].copy_from_slice(&1_u32.to_be_bytes());
        assert_eq!(
            TickOutcome::decode_canonical(&wrapping_total),
            Err(TickProtocolError::TickActionOverflow {
                limit: MAX_ACTIONS_PER_TICK,
            })
        );
    }

    #[test]
    fn absent_frame_padding_and_game_without_composite_fail_closed() {
        let base = outcome(None, 0, 1, 2, 1, 2, Vec::new());
        let mut bytes = base.encode_canonical();
        let game_offset = bytes.len() - 2 * OPTIONAL_FRAME_LEN;
        bytes[game_offset + 1] = 1;
        assert_eq!(
            TickOutcome::decode_canonical(&bytes),
            Err(TickProtocolError::NonZeroAbsentPadding { field: "game" })
        );

        assert_eq!(
            TickOutcome::with_edit_attempts_only(
                None,
                0,
                tag(TagKind::World, 1),
                tag(TagKind::Receipt, 2),
                input(0),
                [],
                tag(TagKind::World, 3),
                tag(TagKind::Receipt, 4),
                camera(1.0),
                Some(frame(5)),
                None,
            ),
            Err(TickProtocolError::GameFrameWithoutComposite)
        );
    }

    #[test]
    fn input_render_causes_and_frame_presence_are_consistent() {
        for bits in [1 << 6, 1 << 7, 1 << 10] {
            assert_eq!(
                TickOutcome::with_edit_attempts_only(
                    None,
                    0,
                    tag(TagKind::World, 1),
                    tag(TagKind::Receipt, 2),
                    input(bits),
                    [],
                    tag(TagKind::World, 3),
                    tag(TagKind::Receipt, 4),
                    camera(1.0),
                    None,
                    None,
                ),
                Err(TickProtocolError::MissingRequiredGameFrame)
            );
        }

        let mouse_input = TickInputState::new(
            TickInputFlags::default(),
            1.0,
            0.0,
            0.0,
            801,
            801,
            1201,
            801,
        )
        .unwrap();
        assert_eq!(
            TickOutcome::with_edit_attempts_only(
                None,
                0,
                tag(TagKind::World, 1),
                tag(TagKind::Receipt, 2),
                mouse_input,
                [],
                tag(TagKind::World, 3),
                tag(TagKind::Receipt, 4),
                camera(1.0),
                None,
                None,
            ),
            Err(TickProtocolError::MissingRequiredGameFrame)
        );

        assert_eq!(
            TickOutcome::with_edit_attempts_only(
                None,
                0,
                tag(TagKind::World, 1),
                tag(TagKind::Receipt, 2),
                input(1 << 8),
                [],
                tag(TagKind::World, 3),
                tag(TagKind::Receipt, 4),
                camera(1.0),
                None,
                None,
            ),
            Err(TickProtocolError::MissingRequiredCompositeFrame)
        );

        assert_eq!(
            TickOutcome::with_edit_attempts_only(
                None,
                0,
                tag(TagKind::World, 1),
                tag(TagKind::Receipt, 2),
                input(0),
                [],
                tag(TagKind::World, 3),
                tag(TagKind::Receipt, 4),
                camera(1.0),
                None,
                Some(frame(5)),
            ),
            Err(TickProtocolError::UnexpectedCompositeFrame)
        );

        assert_eq!(
            TickOutcome::with_edit_attempts_only(
                None,
                0,
                tag(TagKind::World, 1),
                tag(TagKind::Receipt, 2),
                input(0),
                [],
                tag(TagKind::World, 3),
                tag(TagKind::Receipt, 2),
                camera(1.0),
                None,
                None,
            ),
            Err(TickProtocolError::WorldChangeWithoutGameFrame)
        );
    }

    #[test]
    fn history_advances_if_and_only_if_an_attempt_executes() {
        assert_eq!(
            TickOutcome::with_edit_attempts_only(
                None,
                0,
                tag(TagKind::World, 1),
                tag(TagKind::Receipt, 2),
                input(0),
                [],
                tag(TagKind::World, 1),
                tag(TagKind::Receipt, 3),
                camera(1.0),
                None,
                None,
            ),
            Err(TickProtocolError::HistoryExecutionMismatch {
                has_executed_attempt: false,
                history_advanced: true,
            })
        );

        assert_eq!(
            TickOutcome::with_edit_attempts_only(
                None,
                0,
                tag(TagKind::World, 1),
                tag(TagKind::Receipt, 2),
                input(0),
                [TickAttempt::ExactExecuted(Edit::new(
                    Operation::Off,
                    Coord::new(0, 0, 0),
                ))],
                tag(TagKind::World, 1),
                tag(TagKind::Receipt, 2),
                camera(1.0),
                None,
                None,
            ),
            Err(TickProtocolError::HistoryExecutionMismatch {
                has_executed_attempt: true,
                history_advanced: false,
            })
        );
    }

    #[test]
    fn world_change_requires_history_advance() {
        assert_eq!(
            TickOutcome::with_edit_attempts_only(
                None,
                0,
                tag(TagKind::World, 1),
                tag(TagKind::Receipt, 2),
                input(0),
                [],
                tag(TagKind::World, 3),
                tag(TagKind::Receipt, 2),
                camera(1.0),
                Some(frame(5)),
                Some(frame(7)),
            ),
            Err(TickProtocolError::WorldChangeWithoutHistoryAdvance)
        );
    }

    #[test]
    fn nonfinite_controls_and_camera_are_refused_without_normalizing_bits() {
        assert!(matches!(
            TickInputState::new(TickInputFlags::default(), f64::NAN, 0.0, 0.0, 1, 1, 1, 1,),
            Err(TickProtocolError::NonFiniteFloat {
                field: "mouse_x",
                ..
            })
        ));
        let mut bits = camera(1.0).bits();
        bits[3] = f64::INFINITY.to_bits();
        assert!(matches!(
            TickCameraState::from_bits(bits),
            Err(TickProtocolError::NonFiniteFloat {
                field: "camera_yaw",
                ..
            })
        ));

        let mut bits = camera(1.0).bits();
        bits[8] = (MIN_OBSERVATION_DISTANCE / 2.0).to_bits();
        assert!(matches!(
            TickCameraState::from_bits(bits),
            Err(TickProtocolError::InvalidObservationDistance { .. })
        ));

        let mut bits = camera(1.0).bits();
        bits[3] = PI.to_bits();
        assert!(matches!(
            TickCameraState::from_bits(bits),
            Err(TickProtocolError::InvalidYaw { .. })
        ));

        let mut bits = camera(1.0).bits();
        bits[4] = (MAX_ABS_PITCH_RADIANS + 1.0e-9).to_bits();
        assert!(matches!(
            TickCameraState::from_bits(bits),
            Err(TickProtocolError::InvalidPitch { .. })
        ));
    }

    #[test]
    fn decoder_applies_explicit_local_resource_limits_before_allocation() {
        let empty = outcome(None, 0, 1, 2, 1, 2, Vec::new()).encode_canonical();
        assert_eq!(
            TickOutcome::decode_canonical_with_limits(
                &empty,
                TickDecodeLimits {
                    max_payload_bytes: empty.len() - 1,
                    max_attempts: usize::MAX,
                },
            ),
            Err(TickProtocolError::DecodeLimitExceeded {
                resource: "payload bytes",
                limit: empty.len() - 1,
                actual: empty.len(),
            })
        );

        let one_attempt =
            outcome(None, 0, 1, 2, 1, 2, vec![TickAttempt::AimDeleteUnresolved]).encode_canonical();
        assert_eq!(
            TickOutcome::decode_canonical_with_limits(
                &one_attempt,
                TickDecodeLimits {
                    max_payload_bytes: one_attempt.len(),
                    max_attempts: 0,
                },
            ),
            Err(TickProtocolError::DecodeLimitExceeded {
                resource: "attempts",
                limit: 0,
                actual: 1,
            })
        );
    }

    #[test]
    fn content_link_requires_exact_parent_ordinal_world_and_history() {
        let first = outcome(None, 0, 1, 2, 1, 2, Vec::new());
        let second = outcome(Some(first.tag()), 1, 1, 2, 1, 2, Vec::new());
        verify_successor(None, &first).unwrap();
        verify_successor(Some(&first), &second).unwrap();

        let wrong_parent = outcome(
            Some(tag(TagKind::TickOutcome, 99)),
            1,
            1,
            2,
            1,
            2,
            Vec::new(),
        );
        assert!(matches!(
            verify_successor(Some(&first), &wrong_parent),
            Err(TickProtocolError::ParentMismatch { .. })
        ));
        let wrong_world = outcome(Some(first.tag()), 1, 9, 2, 9, 2, Vec::new());
        assert!(matches!(
            verify_successor(Some(&first), &wrong_world),
            Err(TickProtocolError::WorldDiscontinuity { .. })
        ));
        let wrong_history = outcome(Some(first.tag()), 1, 1, 9, 1, 9, Vec::new());
        assert!(matches!(
            verify_successor(Some(&first), &wrong_history),
            Err(TickProtocolError::HistoryDiscontinuity { .. })
        ));
    }

    #[test]
    fn camera_change_requires_a_game_frame_in_the_successor() {
        let first = outcome(None, 0, 7, 8, 7, 8, Vec::new());
        let second = TickOutcome::with_edit_attempts_only(
            Some(first.tag()),
            1,
            tag(TagKind::World, 7),
            tag(TagKind::Receipt, 8),
            input(0),
            [],
            tag(TagKind::World, 7),
            tag(TagKind::Receipt, 8),
            camera(2.0),
            None,
            None,
        )
        .unwrap();

        assert_eq!(
            verify_successor(Some(&first), &second),
            Err(TickProtocolError::CameraChangeWithoutGameFrame)
        );
    }

    #[test]
    fn game_frame_input_binding_is_contextual_and_exact() {
        let world = tag(TagKind::World, 0x21);
        let history = tag(TagKind::Receipt, 0x22);
        let camera_state = camera(1.0);
        let tick_input = TickInputState::new(
            TickInputFlags::default(),
            0.0,
            0.0,
            0.0,
            801,
            603,
            1201,
            603,
        )
        .unwrap();
        let renderer_profile = tag(TagKind::Policy, 0x23);
        let overlay_input = tag(TagKind::Policy, 0x24);
        let make_input = |world, camera, width, height, renderer_profile, overlay_input| {
            FrameInput::from_camera_bit_lanes(
                world,
                camera,
                width,
                height,
                renderer_profile,
                overlay_input,
            )
            .unwrap()
            .tag()
        };
        let expected = make_input(
            world,
            camera_state.bits(),
            801,
            603,
            renderer_profile,
            overlay_input,
        );
        let make_outcome = |game_input| {
            TickOutcome::with_edit_attempts_only(
                None,
                0,
                world,
                history,
                tick_input,
                [],
                world,
                history,
                camera_state,
                Some(TickFrameCommitment::new(game_input, tag(TagKind::Frame, 0x25)).unwrap()),
                Some(frame(0x26)),
            )
            .unwrap()
        };

        let valid = make_outcome(expected);
        assert_eq!(
            valid.verify_game_input_binding(renderer_profile, overlay_input),
            Ok(())
        );

        let mut variants = vec![
            make_input(
                tag(TagKind::World, 0x31),
                camera_state.bits(),
                801,
                603,
                renderer_profile,
                overlay_input,
            ),
            make_input(
                world,
                camera_state.bits(),
                802,
                603,
                renderer_profile,
                overlay_input,
            ),
            make_input(
                world,
                camera_state.bits(),
                801,
                604,
                renderer_profile,
                overlay_input,
            ),
            make_input(
                world,
                camera_state.bits(),
                801,
                603,
                tag(TagKind::Policy, 0x32),
                overlay_input,
            ),
            make_input(
                world,
                camera_state.bits(),
                801,
                603,
                renderer_profile,
                tag(TagKind::Policy, 0x33),
            ),
        ];
        for lane in 0..10 {
            let mut camera = camera_state.bits();
            camera[lane] ^= 1;
            variants.push(make_input(
                world,
                camera,
                801,
                603,
                renderer_profile,
                overlay_input,
            ));
        }
        for variant in variants {
            assert!(matches!(
                make_outcome(variant).verify_game_input_binding(renderer_profile, overlay_input),
                Err(TickProtocolError::GameFrameInputMismatch { .. })
            ));
        }

        assert!(matches!(
            valid.verify_game_input_binding(tag(TagKind::World, 0x34), overlay_input),
            Err(TickProtocolError::FrameInputContext {
                source: FrameProtocolError::WrongKind {
                    expected: TagKind::Policy,
                    actual: TagKind::World,
                    ..
                }
            })
        ));

        let no_game = outcome(None, 0, 1, 2, 1, 2, Vec::new());
        assert_eq!(
            no_game
                .verify_game_input_binding(tag(TagKind::World, 0x35), tag(TagKind::Frame, 0x36),),
            Ok(())
        );
    }

    #[test]
    fn canonical_wrapper_length_matches_the_streamed_tag() {
        let outcome = TickOutcome::with_edit_attempts_only(
            None,
            0,
            tag(TagKind::World, 0x10),
            tag(TagKind::Receipt, 0x20),
            input(INPUT_FLAG_MASK),
            [TickAttempt::ExactExecuted(Edit::new(
                Operation::Toggle,
                Coord::new(i64::MIN, 0, i64::MAX),
            ))],
            tag(TagKind::World, 0x30),
            tag(TagKind::Receipt, 0x40),
            camera(1.0),
            Some(frame(0x50)),
            Some(frame(0x60)),
        )
        .unwrap();
        let payload = outcome.encode_canonical();
        let preimage = encode_canonical_preimage(
            TagKind::TickOutcome,
            TICK_OUTCOME_SCHEMA,
            payload.len() as u64,
            |sink| sink(&payload),
        )
        .unwrap();
        assert_eq!(payload.len(), TICK_OUTCOME_FIXED_PAYLOAD_LEN + 26);
        assert_eq!(
            crate::protocol::hash_canonical(
                TagKind::TickOutcome,
                TICK_OUTCOME_SCHEMA,
                payload.len() as u64,
                |sink| sink(&payload),
            )
            .unwrap(),
            outcome.tag()
        );
        assert!(preimage.ends_with(&payload));
    }

    #[test]
    fn independent_python_known_answers_freeze_all_option_shapes() {
        let codec_camera = TickCameraState::from_bits([
            0.0_f64.to_bits(),
            (-0.0_f64).to_bits(),
            (-1.0_f64).to_bits(),
            0.0_f64.to_bits(),
            (-0.0_f64).to_bits(),
            0.0_f64.to_bits(),
            0.0_f64.to_bits(),
            0.0_f64.to_bits(),
            1.0_f64.to_bits(),
            1.0_f64.to_bits(),
        ])
        .unwrap();
        let genesis_input = TickInputState::from_bits(
            TickInputFlags::default(),
            0.0_f64.to_bits(),
            (-0.0_f64).to_bits(),
            0.0_f64.to_bits(),
            801,
            801,
            1201,
            801,
        )
        .unwrap();
        let genesis = TickOutcome::with_edit_attempts_only(
            None,
            0,
            sequence_tag(TagKind::World, 0x00),
            sequence_tag(TagKind::Receipt, 0x20),
            genesis_input,
            [],
            sequence_tag(TagKind::World, 0x00),
            sequence_tag(TagKind::Receipt, 0x20),
            codec_camera,
            None,
            None,
        )
        .unwrap();
        assert_vector(
            &genesis,
            469,
            550,
            "1671e2c245695f10e592de025895140eca84f9936031a7fc28995ab07a3b7c7c",
            "#bb.tick-outcome.sha256.mdrfmhsgskam6cmt4n3zncrclswc26fjfowva6e7itavrs7zx2tq",
        );

        // Python independently constructs the same otherwise-idle outcome with
        // four accepted control occurrences across three kinds: toggle twice,
        // resize once, and clear-motion once. The unequal 2/1/1 split detects a
        // control-lane permutation involving ToggleSidebar.
        let mut control_actions = TickActionLedger::empty();
        for lane in [
            LANE_TOGGLE_SIDEBAR,
            LANE_TOGGLE_SIDEBAR,
            LANE_RESIZE,
            LANE_CLEAR_MOTION,
        ] {
            control_actions.record(lane).unwrap();
        }
        let control_lanes_only = TickOutcome::new(
            None,
            0,
            sequence_tag(TagKind::World, 0x00),
            sequence_tag(TagKind::Receipt, 0x20),
            genesis_input,
            control_actions,
            [],
            sequence_tag(TagKind::World, 0x00),
            sequence_tag(TagKind::Receipt, 0x20),
            codec_camera,
            None,
            None,
        )
        .unwrap();
        assert_vector(
            &control_lanes_only,
            469,
            550,
            "42968ac648afd01427128e9c9551a2bee9579a7bf107909c6c3f97e7cbb41973",
            "#bb.tick-outcome.sha256.ckeafgihrlyc57iiyef4mjvkqrxsyfbiucrrb4zndfvp57h45xhq",
        );

        let mixed_input = TickInputState::from_bits(
            TickInputFlags::from_bits(INPUT_FLAG_MASK).unwrap(),
            0x8000_0000_0000_0000,
            0x0000_0000_0000_0001,
            0x7fef_ffff_ffff_ffff,
            0x0102_0304,
            0x1112_1314,
            0x2122_2324,
            0x1112_1314,
        )
        .unwrap();
        let mixed_camera = TickCameraState::from_bits([
            0x0000_0000_0000_0000,
            0x8000_0000_0000_0000,
            0x0000_0000_0000_0001,
            0x8000_0000_0000_0001,
            0x3ff0_0000_0000_0000,
            0xbff0_0000_0000_0000,
            0x7fef_ffff_ffff_ffff,
            0xffef_ffff_ffff_ffff,
            2.0_f64.to_bits(),
            1.0_f64.to_bits(),
        ])
        .unwrap();
        let mixed = TickOutcome::with_edit_attempts_only(
            Some(sequence_tag(TagKind::TickOutcome, 0x00)),
            0x0102_0304_0506_0708,
            sequence_tag(TagKind::World, 0x20),
            sequence_tag(TagKind::Receipt, 0x40),
            mixed_input,
            [
                TickAttempt::AimDeleteUnresolved,
                TickAttempt::AimPlaceUnresolved,
                TickAttempt::AimDeleteExecuted(Edit::new(
                    Operation::Off,
                    Coord::new(i64::MIN, -2, i64::MAX),
                )),
                TickAttempt::AimPlaceRejectedObserver(Edit::new(
                    Operation::On,
                    Coord::new(-1, 0, 1),
                )),
                TickAttempt::AimPlaceExecuted(Edit::new(Operation::On, Coord::new(2, -3, 4))),
                TickAttempt::ExactRejectedObserver(Edit::new(
                    Operation::Toggle,
                    Coord::new(-5, 6, -7),
                )),
                TickAttempt::ExactExecuted(Edit::new(Operation::Off, Coord::new(8, -9, 10))),
            ],
            sequence_tag(TagKind::World, 0x60),
            sequence_tag(TagKind::Receipt, 0x80),
            mixed_camera,
            Some(sequence_frame(0xa0, 0xc0)),
            Some(sequence_frame(0xe0, 0x10)),
        )
        .unwrap();
        assert_vector(
            &mixed,
            601,
            682,
            "5983dd2fc0cd797a9bd6fb70fdeb19ae316c587a4c1ea406f766000718ea68d8",
            "#bb.tick-outcome.sha256.nkazbofarz6mjgss625grz4cxx62itnqhbplgcwwjyrr3657mqda",
        );

        let sidebar_input = TickInputState::from_bits(
            TickInputFlags::from_bits((1 << 8) | (1 << 9)).unwrap(),
            0.0_f64.to_bits(),
            0.0_f64.to_bits(),
            (-0.0_f64).to_bits(),
            801,
            801,
            1201,
            801,
        )
        .unwrap();
        let sidebar = TickOutcome::with_edit_attempts_only(
            Some(sequence_tag(TagKind::TickOutcome, 0x11)),
            1,
            sequence_tag(TagKind::World, 0x31),
            sequence_tag(TagKind::Receipt, 0x51),
            sidebar_input,
            [],
            sequence_tag(TagKind::World, 0x31),
            sequence_tag(TagKind::Receipt, 0x51),
            codec_camera,
            None,
            Some(sequence_frame(0x71, 0x91)),
        )
        .unwrap();
        assert_vector(
            &sidebar,
            469,
            550,
            "6cf9f6274bd78be3420ee5b568e651288d8c8ea6c9917e5af5f9a7c2130ed0b2",
            "#bb.tick-outcome.sha256.bzqggwzw2eygclvjcidemqtrhdpdozhioimfzrl44mbphbgoqvna",
        );
    }

    fn assert_vector(
        outcome: &TickOutcome,
        payload_len: usize,
        preimage_len: usize,
        payload_hash: &str,
        tag: &str,
    ) {
        let payload = outcome.encode_canonical();
        let preimage = encode_canonical_preimage(
            TagKind::TickOutcome,
            TICK_OUTCOME_SCHEMA,
            payload.len() as u64,
            |sink| sink(&payload),
        )
        .unwrap();
        assert_eq!(payload.len(), payload_len);
        assert_eq!(preimage.len(), preimage_len);
        assert_eq!(format!("{:x}", Sha256::digest(&payload)), payload_hash);
        assert_eq!(outcome.tag().to_string(), tag);
        assert_eq!(TickOutcome::decode_canonical(&payload), Ok(outcome.clone()));
    }
}
