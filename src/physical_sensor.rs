//! Bounded, local records for RGB samples claimed to come from an external display sensor.
//!
//! This module deliberately stops short of a physical-display proof. A carrier can state only
//! that no sensor applied, that a sensor was unavailable, or that integer RGB samples were
//! reported. [`PhysicalSensorObservation::verify_against`] then binds reported samples to one
//! supplied [`PresentationOutputArtifact`]. A returned [`BoundSampleMatch`] means only that the
//! supplied numbers lie within declared integer-code tolerances for named pixels of an already
//! matching OS-raster artifact. It does not establish instrument identity, calibration custody,
//! camera-to-panel geometry, timing, scanout, or emitted light.

use std::cmp::{Ordering, Reverse};
use std::collections::BinaryHeap;
use std::fmt;

use crate::display::{CaptureStage, DisplayOutcome};
use crate::presentation_output::PresentationOutputArtifact;
use crate::protocol::{ContentTag, TagError, TagKind, hash_canonical};
use crate::storage::MAX_STORAGE_PAYLOAD_BYTES;

/// Schema bound into the fixed policy tag for this local comparison protocol.
pub const PHYSICAL_SENSOR_POLICY_SCHEMA: &[u8] = b"bitblock/physical-sensor-policy/v1";
/// Schema bound into one declared external-sensor profile tag.
pub const DECLARED_SENSOR_PROFILE_SCHEMA: &[u8] = b"bitblock/declared-sensor-profile/v1";
/// Schema bound into one ordered RGB challenge tag.
pub const SENSOR_CHALLENGE_SCHEMA: &[u8] = b"bitblock/sensor-rgb-challenge/v1";
/// Schema bound into the deterministic RGB-transition sampling policy.
pub const RGB_GRADIENT_CHALLENGE_POLICY_SCHEMA: &[u8] =
    b"bitblock/sensor-rgb-gradient-challenge-policy/v1";
/// Schema bound into a physical-sensor observation carrier.
pub const PHYSICAL_SENSOR_OBSERVATION_SCHEMA: &[u8] = b"bitblock/physical-sensor-observation/v1";

/// The outer storage cap applies even though this carrier is normally far smaller.
pub const MAX_PHYSICAL_SENSOR_OBSERVATION_BYTES: usize = MAX_STORAGE_PAYLOAD_BYTES;
/// Maximum ordered framebuffer positions in one challenge or reported-sample batch.
pub const MAX_SENSOR_CHALLENGE_POINTS: usize = 4_096;
/// Maximum opaque source bytes retained as an audit attachment, never as a success condition.
pub const MAX_SENSOR_RAW_BYTES: usize = 64 * 1024;

const TAG_BYTES: usize = 32;
const PROFILE_BYTES: usize = 8;
const CHALLENGE_POINT_BYTES: usize = 12;
const SAMPLE_BYTES: usize = 6;
const U16_BYTES: usize = 2;
const U32_BYTES: usize = 4;
const OPTIONAL_MARKER_NONE: u8 = 0;
const OPTIONAL_MARKER_SOME: u8 = 1;
const RGB_GRADIENT_CHALLENGE_POLICY_PAYLOAD: &[u8] = concat!(
    "parent=verified-presentation-output-with-exact-desktop-raster;",
    "pixels=expected-xrgb-row-major;",
    "adjacency=right-and-down-existing-neighbours;",
    "edge-score=sum-abs-rgb8-channel-deltas;",
    "point-score=sum-existing-right-down-edge-scores;",
    "rank=point-score-descending-then-y-ascending-then-x-ascending;",
    "retain=top-requested-count;output=row-major;",
    "purpose=diagnostic-sensor-challenge-selection-not-physical-proof"
)
.as_bytes();

/// Returns the full typed hash tag for the fixed comparison policy.
///
/// It names this protocol's integer-code comparison rules. It is not an instrument credential.
pub fn physical_sensor_policy_tag() -> ContentTag {
    hash_canonical(TagKind::Policy, PHYSICAL_SENSOR_POLICY_SCHEMA, 0, |_| {})
        .expect("the fixed physical-sensor policy payload has zero bytes")
}

/// Returns the full typed hash tag for the fixed RGB-transition challenge-selection policy.
///
/// This names only a deterministic way to choose diagnostic framebuffer positions. The source
/// framebuffer remains fully named by its presentation artifact, and neither this policy nor its
/// chosen points establishes a physical camera/panel mapping, depth, scanout, or emitted light.
pub fn rgb_gradient_challenge_policy_tag() -> ContentTag {
    hash_canonical(
        TagKind::Policy,
        RGB_GRADIENT_CHALLENGE_POLICY_SCHEMA,
        RGB_GRADIENT_CHALLENGE_POLICY_PAYLOAD.len() as u64,
        |sink| sink(RGB_GRADIENT_CHALLENGE_POLICY_PAYLOAD),
    )
    .expect("the fixed RGB-gradient challenge policy payload length must match")
}

/// The only states a canonical carrier is allowed to serialize.
///
/// Success is intentionally absent. A bounded match exists only as the result of binding an
/// observation to a supplied checked presentation artifact.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum SensorEvidenceState {
    NotApplicable = 0,
    Unavailable = 1,
    ReportedSamples = 2,
}

impl SensorEvidenceState {
    const fn code(self) -> u8 {
        self as u8
    }

    fn decode(code: u8) -> Result<Self, PhysicalSensorObservationError> {
        match code {
            0 => Ok(Self::NotApplicable),
            1 => Ok(Self::Unavailable),
            2 => Ok(Self::ReportedSamples),
            _ => Err(PhysicalSensorObservationError::UnknownEvidenceState { code }),
        }
    }
}

/// A descriptive kind declared by the reporting process, not an authenticated device identity.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum DeclaredSensorKind {
    Camera = 1,
    Colorimeter = 2,
    OtherExternalSensor = 3,
}

impl DeclaredSensorKind {
    const fn code(self) -> u8 {
        self as u8
    }

    fn decode(code: u8) -> Result<Self, PhysicalSensorObservationError> {
        match code {
            1 => Ok(Self::Camera),
            2 => Ok(Self::Colorimeter),
            3 => Ok(Self::OtherExternalSensor),
            _ => Err(PhysicalSensorObservationError::UnknownSensorKind { code }),
        }
    }
}

/// The one code space v1 compares, without assuming ICC, HDR, luminance, or panel transforms.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum SensorCodeSpace {
    /// Each expected XRGB8 channel is expanded to a `u16` with `channel * 257`.
    Xrgb8ExpandedRgb16 = 1,
}

impl SensorCodeSpace {
    const fn code(self) -> u8 {
        self as u8
    }

    fn decode(code: u8) -> Result<Self, PhysicalSensorObservationError> {
        match code {
            1 => Ok(Self::Xrgb8ExpandedRgb16),
            _ => Err(PhysicalSensorObservationError::UnknownCodeSpace { code }),
        }
    }
}

/// A self-declared comparison profile.
///
/// The profile describes code-space and per-channel limits. Its tag is a content name for those
/// declarations; it is not evidence that a device is calibrated or held by an independent party.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DeclaredSensorProfile {
    kind: DeclaredSensorKind,
    code_space: SensorCodeSpace,
    red_tolerance: u16,
    green_tolerance: u16,
    blue_tolerance: u16,
}

impl DeclaredSensorProfile {
    pub const fn new(
        kind: DeclaredSensorKind,
        red_tolerance: u16,
        green_tolerance: u16,
        blue_tolerance: u16,
    ) -> Self {
        Self {
            kind,
            code_space: SensorCodeSpace::Xrgb8ExpandedRgb16,
            red_tolerance,
            green_tolerance,
            blue_tolerance,
        }
    }

    pub const fn kind(self) -> DeclaredSensorKind {
        self.kind
    }

    pub const fn code_space(self) -> SensorCodeSpace {
        self.code_space
    }

    pub const fn tolerances(self) -> [u16; 3] {
        [
            self.red_tolerance,
            self.green_tolerance,
            self.blue_tolerance,
        ]
    }

    /// Full content tag for this declared profile.
    pub fn tag(self) -> ContentTag {
        let bytes = self.encode();
        hash_canonical(
            TagKind::Policy,
            DECLARED_SENSOR_PROFILE_SCHEMA,
            bytes.len() as u64,
            |sink| sink(&bytes),
        )
        .expect("a fixed-width declared sensor profile always hashes")
    }

    fn encode(self) -> [u8; PROFILE_BYTES] {
        let mut bytes = [0_u8; PROFILE_BYTES];
        bytes[0] = self.kind.code();
        bytes[1] = self.code_space.code();
        bytes[2..4].copy_from_slice(&self.red_tolerance.to_be_bytes());
        bytes[4..6].copy_from_slice(&self.green_tolerance.to_be_bytes());
        bytes[6..8].copy_from_slice(&self.blue_tolerance.to_be_bytes());
        bytes
    }

    fn decode(bytes: &[u8]) -> Result<Self, PhysicalSensorObservationError> {
        let [
            kind,
            code_space,
            red_hi,
            red_lo,
            green_hi,
            green_lo,
            blue_hi,
            blue_lo,
        ] = bytes
            .try_into()
            .map_err(|_| PhysicalSensorObservationError::ProfileLength {
                actual: bytes.len(),
            })?;
        Ok(Self {
            kind: DeclaredSensorKind::decode(kind)?,
            code_space: SensorCodeSpace::decode(code_space)?,
            red_tolerance: u16::from_be_bytes([red_hi, red_lo]),
            green_tolerance: u16::from_be_bytes([green_hi, green_lo]),
            blue_tolerance: u16::from_be_bytes([blue_hi, blue_lo]),
        })
    }
}

/// One ordered framebuffer position and the XRGB word the challenge expects there.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SensorChallengePoint {
    x: u32,
    y: u32,
    expected_xrgb: u32,
}

impl SensorChallengePoint {
    pub const fn new(x: u32, y: u32, expected_xrgb: u32) -> Self {
        Self {
            x,
            y,
            expected_xrgb,
        }
    }

    pub const fn x(self) -> u32 {
        self.x
    }

    pub const fn y(self) -> u32 {
        self.y
    }

    pub const fn expected_xrgb(self) -> u32 {
        self.expected_xrgb
    }
}

/// A canonical, ordered challenge. Expected colors are independently checked against the
/// supplied presentation artifact before samples can match.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SensorChallenge {
    points: Vec<SensorChallengePoint>,
}

impl SensorChallenge {
    pub fn new(points: Vec<SensorChallengePoint>) -> Result<Self, PhysicalSensorObservationError> {
        let challenge = Self { points };
        challenge.validate()?;
        Ok(challenge)
    }

    pub fn points(&self) -> &[SensorChallengePoint] {
        &self.points
    }

    pub fn tag(&self) -> ContentTag {
        let payload_len = U16_BYTES + self.points.len() * CHALLENGE_POINT_BYTES;
        hash_canonical(
            TagKind::Policy,
            SENSOR_CHALLENGE_SCHEMA,
            payload_len as u64,
            |sink| {
                sink(&(self.points.len() as u16).to_be_bytes());
                for point in &self.points {
                    sink(&point.x.to_be_bytes());
                    sink(&point.y.to_be_bytes());
                    sink(&point.expected_xrgb.to_be_bytes());
                }
            },
        )
        .expect("a validated bounded sensor challenge always hashes")
    }

    fn validate(&self) -> Result<(), PhysicalSensorObservationError> {
        if self.points.is_empty() {
            return Err(PhysicalSensorObservationError::EmptyChallenge);
        }
        if self.points.len() > MAX_SENSOR_CHALLENGE_POINTS {
            return Err(PhysicalSensorObservationError::ChallengePointLimit {
                actual: self.points.len(),
                maximum: MAX_SENSOR_CHALLENGE_POINTS,
            });
        }

        let mut previous = None;
        for (index, point) in self.points.iter().enumerate() {
            if point.expected_xrgb & 0xff00_0000 != 0 {
                return Err(PhysicalSensorObservationError::InvalidChallengeXrgb {
                    index,
                    value: point.expected_xrgb,
                });
            }
            let coordinate = (point.y, point.x);
            if previous.is_some_and(|last| coordinate <= last) {
                return Err(PhysicalSensorObservationError::NonCanonicalChallengeOrder { index });
            }
            previous = Some(coordinate);
        }
        Ok(())
    }
}

/// Exact RGB-transition facts computed while selecting a diagnostic challenge.
///
/// They are projections of the already named complete XRGB slice, not an additional display
/// validity claim. Every adjacency is counted once: from each pixel to its existing right and
/// lower neighbours.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RgbGradientFacts {
    inspected_pixels: u64,
    horizontal_adjacencies: u64,
    vertical_adjacencies: u64,
    nonzero_adjacencies: u64,
    total_rgb_delta: u64,
    maximum_adjacency_delta: u16,
    maximum_point_score: u16,
}

impl RgbGradientFacts {
    pub const fn inspected_pixels(self) -> u64 {
        self.inspected_pixels
    }

    pub const fn horizontal_adjacencies(self) -> u64 {
        self.horizontal_adjacencies
    }

    pub const fn vertical_adjacencies(self) -> u64 {
        self.vertical_adjacencies
    }

    pub const fn nonzero_adjacencies(self) -> u64 {
        self.nonzero_adjacencies
    }

    pub const fn total_rgb_delta(self) -> u64 {
        self.total_rgb_delta
    }

    pub const fn maximum_adjacency_delta(self) -> u16 {
        self.maximum_adjacency_delta
    }

    pub const fn maximum_point_score(self) -> u16 {
        self.maximum_point_score
    }

    fn record_adjacency(&mut self, delta: u16) {
        self.nonzero_adjacencies += u64::from(delta != 0);
        self.total_rgb_delta += u64::from(delta);
        self.maximum_adjacency_delta = self.maximum_adjacency_delta.max(delta);
    }
}

/// A deterministic set of the most RGB-discriminating expected pixels in one exact presentation.
///
/// The selected points are suitable for asking a future external instrument a more informative
/// finite question than arbitrary coordinates. They do not test the instrument themselves and do
/// not replace the complete CPU/desktop-raster comparison that admits the parent artifact.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RgbGradientChallenge {
    parent: ContentTag,
    point_budget: usize,
    challenge: SensorChallenge,
    facts: RgbGradientFacts,
}

impl RgbGradientChallenge {
    /// Derives up to `point_budget` challenge points from an already matching desktop-raster
    /// presentation artifact.
    ///
    /// Each pixel receives the sum of its right and lower RGB8 edge deltas. The strongest pixels
    /// win; equal scores use row-major coordinates as a stable tie break. The returned challenge
    /// itself is row-major because that is the canonical sensor-record ordering.
    pub fn from_exact_presentation(
        parent: &PresentationOutputArtifact,
        point_budget: usize,
    ) -> Result<Self, RgbGradientChallengeError> {
        if point_budget == 0 || point_budget > MAX_SENSOR_CHALLENGE_POINTS {
            return Err(RgbGradientChallengeError::PointBudget {
                actual: point_budget,
                maximum: MAX_SENSOR_CHALLENGE_POINTS,
            });
        }
        if parent.verify().is_err() {
            return Err(RgbGradientChallengeError::ParentInvalid);
        }
        let capture = parent.display().receipt().facts();
        if capture.outcome != DisplayOutcome::Match || capture.stage != CaptureStage::Complete {
            return Err(RgbGradientChallengeError::NoExactDesktopRaster);
        }

        let display = parent.display();
        let (width, height) = display.receipt().expected_viewport();
        let pixels = display.expected_pixels();
        if pixels.is_empty() {
            return Err(RgbGradientChallengeError::EmptyExpectedFrame);
        }

        let width = usize::try_from(width).expect("u32 width always fits usize");
        let height = usize::try_from(height).expect("u32 height always fits usize");
        debug_assert_eq!(pixels.len(), width * height);
        let capacity = point_budget.min(pixels.len());
        let mut selected = BinaryHeap::<Reverse<GradientCandidate>>::new();
        selected.try_reserve(capacity).map_err(|_| {
            RgbGradientChallengeError::AllocationFailed {
                requested_points: capacity,
            }
        })?;

        let mut facts = RgbGradientFacts {
            inspected_pixels: pixels.len() as u64,
            ..RgbGradientFacts::default()
        };
        for y in 0..height {
            for x in 0..width {
                let index = y * width + x;
                let xrgb = pixels[index];
                if xrgb & 0xff00_0000 != 0 {
                    return Err(RgbGradientChallengeError::ParentXrgbHasHighByte {
                        x: u32::try_from(x).expect("frame width is a u32"),
                        y: u32::try_from(y).expect("frame height is a u32"),
                        value: xrgb,
                    });
                }

                let mut score = 0_u16;
                if x + 1 < width {
                    let delta = rgb_delta(xrgb, pixels[index + 1]);
                    facts.horizontal_adjacencies += 1;
                    facts.record_adjacency(delta);
                    score += delta;
                }
                if y + 1 < height {
                    let delta = rgb_delta(xrgb, pixels[index + width]);
                    facts.vertical_adjacencies += 1;
                    facts.record_adjacency(delta);
                    score += delta;
                }
                facts.maximum_point_score = facts.maximum_point_score.max(score);

                retain_gradient_candidate(
                    &mut selected,
                    GradientCandidate {
                        score,
                        x: u32::try_from(x).expect("frame width is a u32"),
                        y: u32::try_from(y).expect("frame height is a u32"),
                        xrgb,
                    },
                    point_budget,
                );
            }
        }

        let mut candidates = Vec::new();
        candidates.try_reserve_exact(selected.len()).map_err(|_| {
            RgbGradientChallengeError::AllocationFailed {
                requested_points: selected.len(),
            }
        })?;
        candidates.extend(selected.into_iter().map(|Reverse(candidate)| candidate));
        candidates.sort_unstable_by_key(|candidate| (candidate.y, candidate.x));

        let mut points = Vec::new();
        points.try_reserve_exact(candidates.len()).map_err(|_| {
            RgbGradientChallengeError::AllocationFailed {
                requested_points: candidates.len(),
            }
        })?;
        points.extend(
            candidates.into_iter().map(|candidate| {
                SensorChallengePoint::new(candidate.x, candidate.y, candidate.xrgb)
            }),
        );

        Ok(Self {
            parent: parent.tag(),
            point_budget,
            challenge: SensorChallenge::new(points)
                .expect("selected exact XRGB points are nonempty and canonical"),
            facts,
        })
    }

    pub const fn parent_tag(&self) -> ContentTag {
        self.parent
    }

    pub fn policy_tag(&self) -> ContentTag {
        rgb_gradient_challenge_policy_tag()
    }

    pub const fn point_budget(&self) -> usize {
        self.point_budget
    }

    pub fn challenge(&self) -> &SensorChallenge {
        &self.challenge
    }

    pub fn into_challenge(self) -> SensorChallenge {
        self.challenge
    }

    pub const fn facts(&self) -> RgbGradientFacts {
        self.facts
    }
}

/// Why a deterministic RGB-transition challenge could not be derived.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RgbGradientChallengeError {
    PointBudget { actual: usize, maximum: usize },
    ParentInvalid,
    NoExactDesktopRaster,
    EmptyExpectedFrame,
    ParentXrgbHasHighByte { x: u32, y: u32, value: u32 },
    AllocationFailed { requested_points: usize },
}

impl fmt::Display for RgbGradientChallengeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {
            Self::PointBudget { actual, maximum } => write!(
                formatter,
                "RGB-gradient challenge requests {actual} points outside 1..={maximum}"
            ),
            Self::ParentInvalid => formatter.write_str(
                "RGB-gradient challenge parent does not verify as a presentation-output artifact",
            ),
            Self::NoExactDesktopRaster => formatter.write_str(
                "RGB-gradient challenge requires a complete matching desktop-raster parent",
            ),
            Self::EmptyExpectedFrame => {
                formatter.write_str("RGB-gradient challenge parent has no expected pixels")
            }
            Self::ParentXrgbHasHighByte { x, y, value } => write!(
                formatter,
                "RGB-gradient challenge parent pixel ({x}, {y}) has non-XRGB word {value:#010x}"
            ),
            Self::AllocationFailed { requested_points } => write!(
                formatter,
                "could not allocate RGB-gradient challenge storage for {requested_points} points"
            ),
        }
    }
}

impl std::error::Error for RgbGradientChallengeError {}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct GradientCandidate {
    score: u16,
    x: u32,
    y: u32,
    xrgb: u32,
}

impl Ord for GradientCandidate {
    fn cmp(&self, other: &Self) -> Ordering {
        self.score
            .cmp(&other.score)
            .then_with(|| other.y.cmp(&self.y))
            .then_with(|| other.x.cmp(&self.x))
    }
}

impl PartialOrd for GradientCandidate {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

fn retain_gradient_candidate(
    selected: &mut BinaryHeap<Reverse<GradientCandidate>>,
    candidate: GradientCandidate,
    point_budget: usize,
) {
    if selected.len() < point_budget {
        selected.push(Reverse(candidate));
    } else if candidate > selected.peek().expect("nonempty full selection heap").0 {
        selected.pop();
        selected.push(Reverse(candidate));
    }
}

fn rgb_delta(left: u32, right: u32) -> u16 {
    let red = ((left >> 16) & 0xff).abs_diff((right >> 16) & 0xff);
    let green = ((left >> 8) & 0xff).abs_diff((right >> 8) & 0xff);
    let blue = (left & 0xff).abs_diff(right & 0xff);
    u16::try_from(red + green + blue).expect("three eight-bit RGB deltas fit u16")
}

/// One RGB code triplet supplied for a declared external-sensor report.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SensorRgbSample {
    pub red: u16,
    pub green: u16,
    pub blue: u16,
}

impl SensorRgbSample {
    pub const fn new(red: u16, green: u16, blue: u16) -> Self {
        Self { red, green, blue }
    }

    pub const fn channels(self) -> [u16; 3] {
        [self.red, self.green, self.blue]
    }
}

/// One self-contained, bounded local report from the physical-sensor boundary.
///
/// The parent is a full typed presentation-output tag, not a numeric device ID. It is resolved
/// only by [`Self::verify_against`], which prevents a local payload from promoting itself to a
/// physical-display assertion.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PhysicalSensorObservation {
    tag: ContentTag,
    encoded_len: usize,
    state: SensorEvidenceState,
    parent_presentation: Option<ContentTag>,
    profile: Option<DeclaredSensorProfile>,
    challenge: Option<SensorChallenge>,
    samples: Vec<SensorRgbSample>,
    raw_measurement: Vec<u8>,
}

impl PhysicalSensorObservation {
    /// A canonical declaration that no physical-sensor observation applies.
    pub fn not_applicable() -> Self {
        Self::build(
            SensorEvidenceState::NotApplicable,
            None,
            None,
            None,
            Vec::new(),
            Vec::new(),
        )
        .expect("the fixed not-applicable sensor observation is valid")
    }

    /// Records a claimed external-sensor attempt that did not supply comparable samples.
    pub fn unavailable(
        parent_presentation: ContentTag,
        profile: DeclaredSensorProfile,
        challenge: SensorChallenge,
        raw_measurement: Vec<u8>,
    ) -> Result<Self, PhysicalSensorObservationError> {
        Self::build(
            SensorEvidenceState::Unavailable,
            Some(parent_presentation),
            Some(profile),
            Some(challenge),
            Vec::new(),
            raw_measurement,
        )
    }

    /// Records caller-supplied numeric samples claimed for an external-sensor report.
    /// This constructor never decides whether they match.
    pub fn reported_samples(
        parent_presentation: ContentTag,
        profile: DeclaredSensorProfile,
        challenge: SensorChallenge,
        samples: Vec<SensorRgbSample>,
        raw_measurement: Vec<u8>,
    ) -> Result<Self, PhysicalSensorObservationError> {
        Self::build(
            SensorEvidenceState::ReportedSamples,
            Some(parent_presentation),
            Some(profile),
            Some(challenge),
            samples,
            raw_measurement,
        )
    }

    fn build(
        state: SensorEvidenceState,
        parent_presentation: Option<ContentTag>,
        profile: Option<DeclaredSensorProfile>,
        challenge: Option<SensorChallenge>,
        samples: Vec<SensorRgbSample>,
        raw_measurement: Vec<u8>,
    ) -> Result<Self, PhysicalSensorObservationError> {
        let mut observation = Self {
            tag: ContentTag::from_digest(TagKind::PhysicalSensorObservation, [0; TAG_BYTES]),
            encoded_len: 0,
            state,
            parent_presentation,
            profile,
            challenge,
            samples,
            raw_measurement,
        };
        let encoded_len = observation.validate_and_len()?;
        observation.tag = observation.recompute_tag(encoded_len)?;
        observation.encoded_len = encoded_len;
        Ok(observation)
    }

    pub const fn tag(&self) -> ContentTag {
        self.tag
    }

    pub const fn state(&self) -> SensorEvidenceState {
        self.state
    }

    pub const fn parent_presentation_tag(&self) -> Option<ContentTag> {
        self.parent_presentation
    }

    pub const fn profile(&self) -> Option<DeclaredSensorProfile> {
        self.profile
    }

    pub fn profile_tag(&self) -> Option<ContentTag> {
        self.profile.map(DeclaredSensorProfile::tag)
    }

    pub fn challenge(&self) -> Option<&SensorChallenge> {
        self.challenge.as_ref()
    }

    pub fn challenge_tag(&self) -> Option<ContentTag> {
        self.challenge.as_ref().map(SensorChallenge::tag)
    }

    pub fn samples(&self) -> &[SensorRgbSample] {
        &self.samples
    }

    /// Bounded source attachment retained for audit only; it never affects a match outcome.
    pub fn raw_measurement(&self) -> &[u8] {
        &self.raw_measurement
    }

    pub const fn encoded_len(&self) -> usize {
        self.encoded_len
    }

    pub fn encode_canonical(&self) -> Result<Vec<u8>, PhysicalSensorObservationError> {
        let encoded_len = self.validate_and_len()?;
        let mut bytes = Vec::new();
        bytes.try_reserve_exact(encoded_len).map_err(|_| {
            PhysicalSensorObservationError::AllocationFailed {
                requested: encoded_len,
            }
        })?;
        self.feed_payload(&mut |chunk| bytes.extend_from_slice(chunk));
        debug_assert_eq!(bytes.len(), encoded_len);
        Ok(bytes)
    }

    /// Strictly decodes a complete canonical observation and recomputes its full tag.
    pub fn decode_canonical(bytes: &[u8]) -> Result<Self, PhysicalSensorObservationError> {
        if bytes.len() > MAX_PHYSICAL_SENSOR_OBSERVATION_BYTES {
            return Err(PhysicalSensorObservationError::PayloadTooLarge {
                actual: bytes.len(),
                maximum: MAX_PHYSICAL_SENSOR_OBSERVATION_BYTES,
            });
        }

        let mut reader = Reader::new(bytes);
        let state = SensorEvidenceState::decode(reader.byte("evidence state")?)?;
        let policy_digest = reader.fixed("physical-sensor policy digest", TAG_BYTES)?;
        if policy_digest != physical_sensor_policy_tag().digest() {
            return Err(PhysicalSensorObservationError::PolicyTagMismatch);
        }

        let parent_presentation = match reader.byte("parent presentation marker")? {
            OPTIONAL_MARKER_NONE => None,
            OPTIONAL_MARKER_SOME => Some(ContentTag::from_digest(
                TagKind::PresentationOutputArtifact,
                reader
                    .fixed("parent presentation digest", TAG_BYTES)?
                    .try_into()
                    .expect("fixed digest length"),
            )),
            marker => {
                return Err(PhysicalSensorObservationError::InvalidOptionalMarker {
                    field: "parent presentation",
                    marker,
                });
            }
        };

        let profile = match reader.byte("declared sensor profile marker")? {
            OPTIONAL_MARKER_NONE => None,
            OPTIONAL_MARKER_SOME => Some(DeclaredSensorProfile::decode(
                reader.fixed("declared sensor profile", PROFILE_BYTES)?,
            )?),
            marker => {
                return Err(PhysicalSensorObservationError::InvalidOptionalMarker {
                    field: "declared sensor profile",
                    marker,
                });
            }
        };

        let challenge = match reader.byte("sensor challenge marker")? {
            OPTIONAL_MARKER_NONE => None,
            OPTIONAL_MARKER_SOME => {
                let count = usize::from(reader.u16("sensor challenge count")?);
                if count > MAX_SENSOR_CHALLENGE_POINTS {
                    return Err(PhysicalSensorObservationError::ChallengePointLimit {
                        actual: count,
                        maximum: MAX_SENSOR_CHALLENGE_POINTS,
                    });
                }
                let mut points = Vec::new();
                points.try_reserve_exact(count).map_err(|_| {
                    PhysicalSensorObservationError::AllocationFailed { requested: count }
                })?;
                for _ in 0..count {
                    points.push(SensorChallengePoint::new(
                        reader.u32("challenge x")?,
                        reader.u32("challenge y")?,
                        reader.u32("challenge expected XRGB")?,
                    ));
                }
                Some(SensorChallenge::new(points)?)
            }
            marker => {
                return Err(PhysicalSensorObservationError::InvalidOptionalMarker {
                    field: "sensor challenge",
                    marker,
                });
            }
        };

        let sample_count = usize::from(reader.u16("sensor sample count")?);
        if sample_count > MAX_SENSOR_CHALLENGE_POINTS {
            return Err(PhysicalSensorObservationError::SampleLimit {
                actual: sample_count,
                maximum: MAX_SENSOR_CHALLENGE_POINTS,
            });
        }
        let mut samples = Vec::new();
        samples.try_reserve_exact(sample_count).map_err(|_| {
            PhysicalSensorObservationError::AllocationFailed {
                requested: sample_count,
            }
        })?;
        for _ in 0..sample_count {
            samples.push(SensorRgbSample::new(
                reader.u16("sample red")?,
                reader.u16("sample green")?,
                reader.u16("sample blue")?,
            ));
        }

        let raw_len =
            usize::try_from(reader.u32("raw measurement length")?).expect("u32 fits usize");
        if raw_len > MAX_SENSOR_RAW_BYTES {
            return Err(PhysicalSensorObservationError::RawMeasurementTooLarge {
                actual: raw_len,
                maximum: MAX_SENSOR_RAW_BYTES,
            });
        }
        let raw_slice = reader.take("raw measurement", raw_len)?;
        let mut raw_measurement = Vec::new();
        raw_measurement
            .try_reserve_exact(raw_len)
            .map_err(|_| PhysicalSensorObservationError::AllocationFailed { requested: raw_len })?;
        raw_measurement.extend_from_slice(raw_slice);
        reader.finish()?;

        let observation = Self::build(
            state,
            parent_presentation,
            profile,
            challenge,
            samples,
            raw_measurement,
        )?;
        if !observation.payload_matches(bytes)? {
            return Err(PhysicalSensorObservationError::NonCanonicalEncoding);
        }
        Ok(observation)
    }

    /// Recomputes structural invariants and the full observation tag.
    pub fn verify(&self) -> Result<(), PhysicalSensorObservationError> {
        let encoded_len = self.validate_and_len()?;
        if self.tag.kind() != TagKind::PhysicalSensorObservation {
            return Err(PhysicalSensorObservationError::WrongTagKind {
                actual: self.tag.kind(),
            });
        }
        if self.tag != self.recompute_tag(encoded_len)? {
            return Err(PhysicalSensorObservationError::ObservationTagMismatch);
        }
        if self.encoded_len != encoded_len {
            return Err(PhysicalSensorObservationError::NonCanonicalEncoding);
        }
        Ok(())
    }

    /// Binds this local report to one supplied presentation-output artifact.
    ///
    /// A successful return says only that all caller-supplied RGB16 values are within the declared
    /// integer tolerances of the named XRGB8 pixels after the supplied artifact has independently
    /// verified an exact OS-raster comparison. It never proves an external sensor made the report.
    pub fn verify_against(
        &self,
        supplied_parent: Option<&PresentationOutputArtifact>,
    ) -> Result<PhysicalSensorBinding, PhysicalSensorObservationError> {
        self.verify()?;
        if self.state == SensorEvidenceState::NotApplicable {
            return Ok(PhysicalSensorBinding::NotApplicable);
        }

        let expected_parent = self.parent_presentation.expect(
            "validated unavailable/reported-sample observations always name a presentation parent",
        );
        let Some(parent) = supplied_parent else {
            return Ok(PhysicalSensorBinding::Unbound(
                PhysicalSensorUnboundReason::ParentUnavailable,
            ));
        };
        if parent.tag() != expected_parent {
            return Ok(PhysicalSensorBinding::Unbound(
                PhysicalSensorUnboundReason::ParentTagMismatch {
                    expected: expected_parent,
                    actual: parent.tag(),
                },
            ));
        }
        if parent.verify().is_err() {
            return Ok(PhysicalSensorBinding::Unbound(
                PhysicalSensorUnboundReason::ParentInvalid,
            ));
        }
        let capture = parent.display().receipt().facts();
        if capture.outcome != DisplayOutcome::Match || capture.stage != CaptureStage::Complete {
            return Ok(PhysicalSensorBinding::Unbound(
                PhysicalSensorUnboundReason::NoExactDesktopRaster,
            ));
        }
        if self.state == SensorEvidenceState::Unavailable {
            return Ok(PhysicalSensorBinding::Unavailable);
        }

        let profile = self.profile.expect(
            "validated reported-sample observations always retain a declared sensor profile",
        );
        let challenge = self
            .challenge
            .as_ref()
            .expect("validated reported-sample observations always retain a sensor challenge");
        let display = parent.display();
        let (width, height) = display.receipt().expected_viewport();
        let pixels = display.expected_pixels();
        for (point_index, point) in challenge.points().iter().enumerate() {
            let Some(index) = point
                .y
                .checked_mul(width)
                .and_then(|value| value.checked_add(point.x))
                .and_then(|value| usize::try_from(value).ok())
            else {
                return Ok(PhysicalSensorBinding::ChallengeMismatch(
                    SensorChallengeMismatch::CoordinateOutOfBounds {
                        point_index,
                        x: point.x,
                        y: point.y,
                        width,
                        height,
                    },
                ));
            };
            if point.x >= width || point.y >= height || index >= pixels.len() {
                return Ok(PhysicalSensorBinding::ChallengeMismatch(
                    SensorChallengeMismatch::CoordinateOutOfBounds {
                        point_index,
                        x: point.x,
                        y: point.y,
                        width,
                        height,
                    },
                ));
            }
            let observed_xrgb = pixels[index];
            if observed_xrgb & 0xff00_0000 != 0 {
                return Ok(PhysicalSensorBinding::ChallengeMismatch(
                    SensorChallengeMismatch::ParentXrgbHasHighByte {
                        point_index,
                        value: observed_xrgb,
                    },
                ));
            }
            if observed_xrgb != point.expected_xrgb {
                return Ok(PhysicalSensorBinding::ChallengeMismatch(
                    SensorChallengeMismatch::ExpectedXrgbMismatch {
                        point_index,
                        challenge: point.expected_xrgb,
                        parent: observed_xrgb,
                    },
                ));
            }
        }

        if self.samples.len() != challenge.points().len() {
            return Ok(PhysicalSensorBinding::SampleMismatch(
                SensorSampleMismatch::Count {
                    expected: challenge.points().len(),
                    actual: self.samples.len(),
                },
            ));
        }
        let tolerances = profile.tolerances();
        for (point_index, (point, sample)) in
            challenge.points().iter().zip(&self.samples).enumerate()
        {
            let expected = xrgb_to_rgb16(point.expected_xrgb);
            let actual = sample.channels();
            let residual = [
                expected[0].abs_diff(actual[0]),
                expected[1].abs_diff(actual[1]),
                expected[2].abs_diff(actual[2]),
            ];
            if residual[0] > tolerances[0]
                || residual[1] > tolerances[1]
                || residual[2] > tolerances[2]
            {
                return Ok(PhysicalSensorBinding::SampleMismatch(
                    SensorSampleMismatch::Residual {
                        point_index,
                        expected,
                        observed: *sample,
                        residual,
                        tolerances,
                    },
                ));
            }
        }

        Ok(PhysicalSensorBinding::BoundSampleMatch(BoundSampleMatch {
            observation: self.tag,
            presentation: expected_parent,
            profile: profile.tag(),
            challenge: challenge.tag(),
            sample_count: self.samples.len(),
        }))
    }

    fn validate_and_len(&self) -> Result<usize, PhysicalSensorObservationError> {
        if let Some(parent) = self.parent_presentation
            && parent.kind() != TagKind::PresentationOutputArtifact
        {
            return Err(PhysicalSensorObservationError::WrongParentKind {
                actual: parent.kind(),
            });
        }
        if self.raw_measurement.len() > MAX_SENSOR_RAW_BYTES {
            return Err(PhysicalSensorObservationError::RawMeasurementTooLarge {
                actual: self.raw_measurement.len(),
                maximum: MAX_SENSOR_RAW_BYTES,
            });
        }
        if self.samples.len() > MAX_SENSOR_CHALLENGE_POINTS {
            return Err(PhysicalSensorObservationError::SampleLimit {
                actual: self.samples.len(),
                maximum: MAX_SENSOR_CHALLENGE_POINTS,
            });
        }
        if let Some(challenge) = &self.challenge {
            challenge.validate()?;
        }

        match self.state {
            SensorEvidenceState::NotApplicable => {
                if self.parent_presentation.is_some()
                    || self.profile.is_some()
                    || self.challenge.is_some()
                    || !self.samples.is_empty()
                    || !self.raw_measurement.is_empty()
                {
                    return Err(PhysicalSensorObservationError::InvalidStateShape {
                        state: self.state,
                        requirement: "not-applicable records retain no parent, profile, challenge, samples, or raw bytes",
                    });
                }
            }
            SensorEvidenceState::Unavailable => {
                if self.parent_presentation.is_none()
                    || self.profile.is_none()
                    || self.challenge.is_none()
                    || !self.samples.is_empty()
                {
                    return Err(PhysicalSensorObservationError::InvalidStateShape {
                        state: self.state,
                        requirement: "unavailable records need parent/profile/challenge and no samples",
                    });
                }
            }
            SensorEvidenceState::ReportedSamples => {
                let challenge_len = self
                    .challenge
                    .as_ref()
                    .map_or(0, |value| value.points.len());
                if self.parent_presentation.is_none()
                    || self.profile.is_none()
                    || self.challenge.is_none()
                    || self.samples.len() != challenge_len
                {
                    return Err(PhysicalSensorObservationError::InvalidStateShape {
                        state: self.state,
                        requirement: "reported-sample records need parent/profile/challenge and exactly one sample per challenge point",
                    });
                }
            }
        }

        let parent_len = 1 + usize::from(self.parent_presentation.is_some()) * TAG_BYTES;
        let profile_len = 1 + usize::from(self.profile.is_some()) * PROFILE_BYTES;
        let challenge_len = match &self.challenge {
            None => 1,
            Some(challenge) => {
                1 + U16_BYTES + checked_mul(challenge.points.len(), CHALLENGE_POINT_BYTES)?
            }
        };
        let samples_len = U16_BYTES + checked_mul(self.samples.len(), SAMPLE_BYTES)?;
        let total = 1_usize
            .checked_add(TAG_BYTES)
            .and_then(|value| value.checked_add(parent_len))
            .and_then(|value| value.checked_add(profile_len))
            .and_then(|value| value.checked_add(challenge_len))
            .and_then(|value| value.checked_add(samples_len))
            .and_then(|value| value.checked_add(U32_BYTES))
            .and_then(|value| value.checked_add(self.raw_measurement.len()))
            .ok_or(PhysicalSensorObservationError::LengthOverflow)?;
        if total > MAX_PHYSICAL_SENSOR_OBSERVATION_BYTES {
            return Err(PhysicalSensorObservationError::PayloadTooLarge {
                actual: total,
                maximum: MAX_PHYSICAL_SENSOR_OBSERVATION_BYTES,
            });
        }
        Ok(total)
    }

    fn recompute_tag(
        &self,
        encoded_len: usize,
    ) -> Result<ContentTag, PhysicalSensorObservationError> {
        hash_canonical(
            TagKind::PhysicalSensorObservation,
            PHYSICAL_SENSOR_OBSERVATION_SCHEMA,
            encoded_len as u64,
            |sink| self.feed_payload(sink),
        )
        .map_err(PhysicalSensorObservationError::Hash)
    }

    fn feed_payload(&self, sink: &mut dyn FnMut(&[u8])) {
        sink(&[self.state.code()]);
        sink(&physical_sensor_policy_tag().digest());
        match self.parent_presentation {
            None => sink(&[OPTIONAL_MARKER_NONE]),
            Some(parent) => {
                sink(&[OPTIONAL_MARKER_SOME]);
                sink(&parent.digest());
            }
        }
        match self.profile {
            None => sink(&[OPTIONAL_MARKER_NONE]),
            Some(profile) => {
                sink(&[OPTIONAL_MARKER_SOME]);
                sink(&profile.encode());
            }
        }
        match &self.challenge {
            None => sink(&[OPTIONAL_MARKER_NONE]),
            Some(challenge) => {
                sink(&[OPTIONAL_MARKER_SOME]);
                sink(&(challenge.points.len() as u16).to_be_bytes());
                for point in &challenge.points {
                    sink(&point.x.to_be_bytes());
                    sink(&point.y.to_be_bytes());
                    sink(&point.expected_xrgb.to_be_bytes());
                }
            }
        }
        sink(&(self.samples.len() as u16).to_be_bytes());
        for sample in &self.samples {
            sink(&sample.red.to_be_bytes());
            sink(&sample.green.to_be_bytes());
            sink(&sample.blue.to_be_bytes());
        }
        sink(&(self.raw_measurement.len() as u32).to_be_bytes());
        sink(&self.raw_measurement);
    }

    fn payload_matches(&self, expected: &[u8]) -> Result<bool, PhysicalSensorObservationError> {
        let actual = self.encode_canonical()?;
        Ok(actual == expected)
    }
}

/// The result of binding a local report to a supplied presentation artifact.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PhysicalSensorBinding {
    NotApplicable,
    Unavailable,
    Unbound(PhysicalSensorUnboundReason),
    ChallengeMismatch(SensorChallengeMismatch),
    SampleMismatch(SensorSampleMismatch),
    BoundSampleMatch(BoundSampleMatch),
}

/// Why a report could not be bound to an exact OS-raster presentation artifact.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PhysicalSensorUnboundReason {
    ParentUnavailable,
    ParentTagMismatch {
        expected: ContentTag,
        actual: ContentTag,
    },
    ParentInvalid,
    NoExactDesktopRaster,
}

/// Why the declared challenge does not name the supplied parent framebuffer exactly.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SensorChallengeMismatch {
    CoordinateOutOfBounds {
        point_index: usize,
        x: u32,
        y: u32,
        width: u32,
        height: u32,
    },
    ParentXrgbHasHighByte {
        point_index: usize,
        value: u32,
    },
    ExpectedXrgbMismatch {
        point_index: usize,
        challenge: u32,
        parent: u32,
    },
}

/// Why a reported RGB code triplet lies outside the declared integer-code tolerance.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SensorSampleMismatch {
    Count {
        expected: usize,
        actual: usize,
    },
    Residual {
        point_index: usize,
        expected: [u16; 3],
        observed: SensorRgbSample,
        residual: [u16; 3],
        tolerances: [u16; 3],
    },
}

/// A narrow successful result returned only after an explicit parent binding.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BoundSampleMatch {
    observation: ContentTag,
    presentation: ContentTag,
    profile: ContentTag,
    challenge: ContentTag,
    sample_count: usize,
}

impl BoundSampleMatch {
    pub const fn observation_tag(self) -> ContentTag {
        self.observation
    }

    pub const fn presentation_tag(self) -> ContentTag {
        self.presentation
    }

    pub const fn profile_tag(self) -> ContentTag {
        self.profile
    }

    pub const fn challenge_tag(self) -> ContentTag {
        self.challenge
    }

    pub const fn sample_count(self) -> usize {
        self.sample_count
    }
}

fn checked_mul(left: usize, right: usize) -> Result<usize, PhysicalSensorObservationError> {
    left.checked_mul(right)
        .ok_or(PhysicalSensorObservationError::LengthOverflow)
}

fn xrgb_to_rgb16(xrgb: u32) -> [u16; 3] {
    [
        (((xrgb >> 16) & 0xff) as u16) * 257,
        (((xrgb >> 8) & 0xff) as u16) * 257,
        ((xrgb & 0xff) as u16) * 257,
    ]
}

struct Reader<'a> {
    bytes: &'a [u8],
    cursor: usize,
}

impl<'a> Reader<'a> {
    const fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, cursor: 0 }
    }

    fn byte(&mut self, field: &'static str) -> Result<u8, PhysicalSensorObservationError> {
        Ok(self.take(field, 1)?[0])
    }

    fn u16(&mut self, field: &'static str) -> Result<u16, PhysicalSensorObservationError> {
        Ok(u16::from_be_bytes(
            self.fixed(field, U16_BYTES)?
                .try_into()
                .expect("fixed u16 length"),
        ))
    }

    fn u32(&mut self, field: &'static str) -> Result<u32, PhysicalSensorObservationError> {
        Ok(u32::from_be_bytes(
            self.fixed(field, U32_BYTES)?
                .try_into()
                .expect("fixed u32 length"),
        ))
    }

    fn fixed(
        &mut self,
        field: &'static str,
        length: usize,
    ) -> Result<&'a [u8], PhysicalSensorObservationError> {
        self.take(field, length)
    }

    fn take(
        &mut self,
        field: &'static str,
        length: usize,
    ) -> Result<&'a [u8], PhysicalSensorObservationError> {
        let remaining = self.bytes.len().saturating_sub(self.cursor);
        if length > remaining {
            return Err(PhysicalSensorObservationError::Truncated {
                field,
                needed: length,
                remaining,
            });
        }
        let end = self
            .cursor
            .checked_add(length)
            .ok_or(PhysicalSensorObservationError::LengthOverflow)?;
        let result = &self.bytes[self.cursor..end];
        self.cursor = end;
        Ok(result)
    }

    fn finish(self) -> Result<(), PhysicalSensorObservationError> {
        if self.cursor == self.bytes.len() {
            Ok(())
        } else {
            Err(PhysicalSensorObservationError::TrailingBytes {
                count: self.bytes.len() - self.cursor,
            })
        }
    }
}

/// Why a canonical physical-sensor observation was refused.
#[derive(Debug)]
pub enum PhysicalSensorObservationError {
    WrongParentKind {
        actual: TagKind,
    },
    EmptyChallenge,
    ChallengePointLimit {
        actual: usize,
        maximum: usize,
    },
    SampleLimit {
        actual: usize,
        maximum: usize,
    },
    InvalidChallengeXrgb {
        index: usize,
        value: u32,
    },
    NonCanonicalChallengeOrder {
        index: usize,
    },
    RawMeasurementTooLarge {
        actual: usize,
        maximum: usize,
    },
    InvalidStateShape {
        state: SensorEvidenceState,
        requirement: &'static str,
    },
    PayloadTooLarge {
        actual: usize,
        maximum: usize,
    },
    LengthOverflow,
    AllocationFailed {
        requested: usize,
    },
    UnknownEvidenceState {
        code: u8,
    },
    UnknownSensorKind {
        code: u8,
    },
    UnknownCodeSpace {
        code: u8,
    },
    ProfileLength {
        actual: usize,
    },
    PolicyTagMismatch,
    InvalidOptionalMarker {
        field: &'static str,
        marker: u8,
    },
    WrongTagKind {
        actual: TagKind,
    },
    ObservationTagMismatch,
    Truncated {
        field: &'static str,
        needed: usize,
        remaining: usize,
    },
    TrailingBytes {
        count: usize,
    },
    NonCanonicalEncoding,
    Hash(TagError),
}

impl fmt::Display for PhysicalSensorObservationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::WrongParentKind { actual } => write!(
                formatter,
                "physical-sensor parent has {} kind instead of presentation-output-artifact",
                actual.name()
            ),
            Self::EmptyChallenge => formatter.write_str("sensor challenge has no points"),
            Self::ChallengePointLimit { actual, maximum } => write!(
                formatter,
                "sensor challenge has {actual} points, exceeding the {maximum}-point limit"
            ),
            Self::SampleLimit { actual, maximum } => write!(
                formatter,
                "sensor report has {actual} samples, exceeding the {maximum}-sample limit"
            ),
            Self::InvalidChallengeXrgb { index, value } => write!(
                formatter,
                "sensor challenge point {index} has non-XRGB word {value:#010x}"
            ),
            Self::NonCanonicalChallengeOrder { index } => write!(
                formatter,
                "sensor challenge point {index} is duplicate or out of row-major order"
            ),
            Self::RawMeasurementTooLarge { actual, maximum } => write!(
                formatter,
                "raw sensor attachment has {actual} bytes, exceeding the {maximum}-byte limit"
            ),
            Self::InvalidStateShape { state, requirement } => write!(
                formatter,
                "physical-sensor {:?} record violates required shape: {requirement}",
                state
            ),
            Self::PayloadTooLarge { actual, maximum } => write!(
                formatter,
                "physical-sensor payload has {actual} bytes, exceeding the {maximum}-byte limit"
            ),
            Self::LengthOverflow => {
                formatter.write_str("physical-sensor payload length overflowed")
            }
            Self::AllocationFailed { requested } => {
                write!(
                    formatter,
                    "could not allocate {requested} physical-sensor bytes"
                )
            }
            Self::UnknownEvidenceState { code } => {
                write!(formatter, "unknown physical-sensor evidence state {code}")
            }
            Self::UnknownSensorKind { code } => {
                write!(formatter, "unknown declared sensor kind {code}")
            }
            Self::UnknownCodeSpace { code } => {
                write!(formatter, "unknown declared sensor code space {code}")
            }
            Self::ProfileLength { actual } => {
                write!(
                    formatter,
                    "declared sensor profile has {actual} bytes instead of {PROFILE_BYTES}"
                )
            }
            Self::PolicyTagMismatch => {
                formatter.write_str("physical-sensor record names the wrong fixed policy tag")
            }
            Self::InvalidOptionalMarker { field, marker } => {
                write!(formatter, "{field} has invalid optional marker {marker}")
            }
            Self::WrongTagKind { actual } => write!(
                formatter,
                "physical-sensor record has {} tag kind instead of physical-sensor-observation",
                actual.name()
            ),
            Self::ObservationTagMismatch => formatter
                .write_str("physical-sensor observation tag does not match canonical bytes"),
            Self::Truncated {
                field,
                needed,
                remaining,
            } => write!(
                formatter,
                "physical-sensor {field} needs {needed} bytes but only {remaining} remain"
            ),
            Self::TrailingBytes { count } => {
                write!(
                    formatter,
                    "physical-sensor record has {count} trailing bytes"
                )
            }
            Self::NonCanonicalEncoding => {
                formatter.write_str("physical-sensor bytes are not canonical")
            }
            Self::Hash(source) => write!(formatter, "physical-sensor hash failed: {source}"),
        }
    }
}

impl std::error::Error for PhysicalSensorObservationError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Hash(source) => Some(source),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::display::{CaptureRect, DisplayCaptureFacts, DisplayLedgerV2};
    use crate::display_artifact::DisplayCaptureArtifact;
    use crate::frame::FrameRef;
    use crate::frame_v2::{FrameReceiptV2, FrameStream};
    use crate::output_topology::{AcquisitionAttempt, AcquisitionFacts, OutputTopologyObservation};
    use crate::presentation::{
        CompositionTimingEnvelope, DwmFlushCall, DwmTimingCall, GdiFlushCall, PresenterEvidence,
        QpcBracket, QpcFrequency, QpcRead,
    };
    use sha2::{Digest, Sha256};

    const PIXELS: [u32; 2] = [0x0011_2233, 0x0044_5566];

    fn test_tag(kind: TagKind, lane: u8) -> ContentTag {
        hash_canonical(kind, b"bitblock/physical-sensor-test/v1", 1, |sink| {
            sink(&[lane]);
        })
        .unwrap()
    }

    fn frame_receipt(frame: FrameRef<'_>) -> FrameReceiptV2 {
        FrameReceiptV2::new(
            None,
            FrameStream::Composite,
            test_tag(TagKind::TickOutcome, 1),
            test_tag(TagKind::FrameInput, 2),
            frame.content_tag(),
        )
        .unwrap()
    }

    fn qpc(value: i64) -> QpcRead {
        QpcRead {
            succeeded: true,
            value,
            error_code: 0,
        }
    }

    fn bracket(before: i64, after: i64) -> QpcBracket {
        QpcBracket {
            before: qpc(before),
            after: qpc(after),
        }
    }

    fn timing(receipt: FrameReceiptV2, scanlines: u32) -> CompositionTimingEnvelope {
        CompositionTimingEnvelope::new(
            receipt,
            PresenterEvidence {
                policy: test_tag(TagKind::Policy, 3),
                requested_scanlines: scanlines,
                returned_scanlines: i32::try_from(scanlines).unwrap(),
                call_error_code: 0,
                release_result: 1,
                release_error_code: 0,
            },
            QpcFrequency {
                succeeded: true,
                counts_per_second: 10_000_000,
                error_code: 0,
            },
            DwmTimingCall {
                bracket: bracket(10, 11),
                hresult: -1,
                snapshot: None,
            },
            bracket(12, 13),
            GdiFlushCall {
                bracket: bracket(14, 15),
                succeeded: true,
            },
            DwmFlushCall {
                bracket: bracket(16, 17),
                hresult: 0,
            },
            DwmTimingCall {
                bracket: bracket(18, 19),
                hresult: -1,
                snapshot: None,
            },
        )
        .unwrap()
    }

    fn topology(seed: u32) -> OutputTopologyObservation {
        OutputTopologyObservation::new_unavailable(
            AcquisitionFacts {
                profile_code_raw: seed,
                request_flags_raw: seed.rotate_left(3),
                attempts: vec![AcquisitionAttempt {
                    retry_ordinal: 0,
                    outcome_code_raw: seed,
                    ..AcquisitionAttempt::default()
                }],
            },
            11,
            87,
        )
        .unwrap()
    }

    fn presentation(exact_capture: bool) -> PresentationOutputArtifact {
        presentation_with_pixels(2, 1, &PIXELS, exact_capture)
    }

    fn presentation_with_pixels(
        width: u32,
        height: u32,
        pixels: &[u32],
        exact_capture: bool,
    ) -> PresentationOutputArtifact {
        let expected = FrameRef::new(width, height, pixels).unwrap();
        let receipt = frame_receipt(expected);
        let rect = CaptureRect {
            left: 0,
            top: 0,
            right: i32::try_from(width).unwrap(),
            bottom: i32::try_from(height).unwrap(),
        };
        let facts = if exact_capture {
            DisplayCaptureFacts {
                outcome: DisplayOutcome::Match,
                stage: CaptureStage::Complete,
                error_code: 0,
                flush_before: true,
                compositor_flush: true,
                flush_after: true,
                rect_before: rect,
                rect_after: rect,
                dpi: 96,
                qpc_start: 100,
                qpc_end: 101,
                qpc_frequency: 10_000_000,
                captured: Some(expected.observe()),
                mismatch_count: 0,
                first_mismatch: None,
            }
        } else {
            DisplayCaptureFacts::unavailable(CaptureStage::FlushBefore, 5)
        };
        let mut ledger = DisplayLedgerV2::default();
        let display_receipt = *ledger.append(receipt, width, height, facts).unwrap();
        let display = if exact_capture {
            DisplayCaptureArtifact::new(display_receipt, expected, Some(expected)).unwrap()
        } else {
            DisplayCaptureArtifact::new(display_receipt, expected, None).unwrap()
        };
        PresentationOutputArtifact::new(display, timing(receipt, height), topology(1), topology(2))
            .unwrap()
    }

    fn profile(tolerance: u16) -> DeclaredSensorProfile {
        DeclaredSensorProfile::new(
            DeclaredSensorKind::Colorimeter,
            tolerance,
            tolerance,
            tolerance,
        )
    }

    fn challenge() -> SensorChallenge {
        SensorChallenge::new(vec![
            SensorChallengePoint::new(0, 0, PIXELS[0]),
            SensorChallengePoint::new(1, 0, PIXELS[1]),
        ])
        .unwrap()
    }

    fn exact_samples() -> Vec<SensorRgbSample> {
        PIXELS
            .into_iter()
            .map(|pixel| {
                let [red, green, blue] = xrgb_to_rgb16(pixel);
                SensorRgbSample::new(red, green, blue)
            })
            .collect()
    }

    #[test]
    fn reported_samples_need_explicit_exact_parent_binding_before_they_match() {
        let parent = presentation(true);
        let observation = PhysicalSensorObservation::reported_samples(
            parent.tag(),
            profile(0),
            challenge(),
            exact_samples(),
            vec![0xde, 0xad],
        )
        .unwrap();
        assert_eq!(observation.state(), SensorEvidenceState::ReportedSamples);
        assert!(matches!(
            observation.verify_against(None).unwrap(),
            PhysicalSensorBinding::Unbound(PhysicalSensorUnboundReason::ParentUnavailable)
        ));
        let binding = observation.verify_against(Some(&parent)).unwrap();
        let PhysicalSensorBinding::BoundSampleMatch(match_result) = binding else {
            panic!("synthetic exact integer samples should satisfy the codec relation")
        };
        assert_eq!(match_result.observation_tag(), observation.tag());
        assert_eq!(match_result.presentation_tag(), parent.tag());
        assert_eq!(match_result.sample_count(), 2);

        // This is deliberately an in-memory codec fixture, not a physical instrument reading.
        let bytes = observation.encode_canonical().unwrap();
        let decoded = PhysicalSensorObservation::decode_canonical(&bytes).unwrap();
        assert_eq!(decoded, observation);
        assert_eq!(decoded.tag().kind(), TagKind::PhysicalSensorObservation);
        decoded.verify().unwrap();
    }

    #[test]
    fn no_sensor_and_unavailable_sensor_never_promote_to_a_match() {
        let not_applicable = PhysicalSensorObservation::not_applicable();
        assert!(matches!(
            not_applicable.verify_against(None).unwrap(),
            PhysicalSensorBinding::NotApplicable
        ));

        let parent = presentation(true);
        let unavailable = PhysicalSensorObservation::unavailable(
            parent.tag(),
            profile(0),
            challenge(),
            Vec::new(),
        )
        .unwrap();
        assert!(matches!(
            unavailable.verify_against(Some(&parent)).unwrap(),
            PhysicalSensorBinding::Unavailable
        ));
        assert!(matches!(
            unavailable.verify_against(None).unwrap(),
            PhysicalSensorBinding::Unbound(PhysicalSensorUnboundReason::ParentUnavailable)
        ));
    }

    #[test]
    fn wrong_parent_capture_challenge_and_residual_each_refuse_to_match() {
        let exact_parent = presentation(true);
        let unavailable_parent = presentation(false);
        let base = PhysicalSensorObservation::reported_samples(
            exact_parent.tag(),
            profile(0),
            challenge(),
            exact_samples(),
            Vec::new(),
        )
        .unwrap();
        assert!(matches!(
            base.verify_against(Some(&unavailable_parent)).unwrap(),
            PhysicalSensorBinding::Unbound(PhysicalSensorUnboundReason::ParentTagMismatch { .. })
        ));

        let unobserved = PhysicalSensorObservation::reported_samples(
            unavailable_parent.tag(),
            profile(0),
            challenge(),
            exact_samples(),
            Vec::new(),
        )
        .unwrap();
        assert!(matches!(
            unobserved
                .verify_against(Some(&unavailable_parent))
                .unwrap(),
            PhysicalSensorBinding::Unbound(PhysicalSensorUnboundReason::NoExactDesktopRaster)
        ));

        let wrong_challenge = SensorChallenge::new(vec![
            SensorChallengePoint::new(0, 0, PIXELS[0] ^ 1),
            SensorChallengePoint::new(1, 0, PIXELS[1]),
        ])
        .unwrap();
        let challenge_mismatch = PhysicalSensorObservation::reported_samples(
            exact_parent.tag(),
            profile(0),
            wrong_challenge,
            exact_samples(),
            Vec::new(),
        )
        .unwrap();
        assert!(matches!(
            challenge_mismatch
                .verify_against(Some(&exact_parent))
                .unwrap(),
            PhysicalSensorBinding::ChallengeMismatch(
                SensorChallengeMismatch::ExpectedXrgbMismatch { point_index: 0, .. }
            )
        ));

        let mut samples = exact_samples();
        samples[1].blue = samples[1].blue.saturating_add(1);
        let sample_mismatch = PhysicalSensorObservation::reported_samples(
            exact_parent.tag(),
            profile(0),
            challenge(),
            samples,
            Vec::new(),
        )
        .unwrap();
        assert!(matches!(
            sample_mismatch.verify_against(Some(&exact_parent)).unwrap(),
            PhysicalSensorBinding::SampleMismatch(SensorSampleMismatch::Residual {
                point_index: 1,
                ..
            })
        ));
    }

    #[test]
    fn tolerance_boundary_is_inclusive_and_one_code_excess_refuses() {
        let parent = presentation(true);
        let mut boundary = exact_samples();
        boundary[0].red = boundary[0].red.saturating_add(2);
        let admitted = PhysicalSensorObservation::reported_samples(
            parent.tag(),
            profile(2),
            challenge(),
            boundary,
            Vec::new(),
        )
        .unwrap();
        assert!(matches!(
            admitted.verify_against(Some(&parent)).unwrap(),
            PhysicalSensorBinding::BoundSampleMatch(_)
        ));

        let mut excess = exact_samples();
        excess[0].red = excess[0].red.saturating_add(3);
        let refused = PhysicalSensorObservation::reported_samples(
            parent.tag(),
            profile(2),
            challenge(),
            excess,
            Vec::new(),
        )
        .unwrap();
        assert!(matches!(
            refused.verify_against(Some(&parent)).unwrap(),
            PhysicalSensorBinding::SampleMismatch(SensorSampleMismatch::Residual {
                point_index: 0,
                ..
            })
        ));
    }

    #[test]
    fn canonical_decoder_rejects_prefixes_trailing_bytes_and_structural_mutations() {
        let parent = presentation(true);
        let observation = PhysicalSensorObservation::reported_samples(
            parent.tag(),
            profile(0),
            challenge(),
            exact_samples(),
            Vec::new(),
        )
        .unwrap();
        let bytes = observation.encode_canonical().unwrap();
        for end in 0..bytes.len() {
            assert!(
                PhysicalSensorObservation::decode_canonical(&bytes[..end]).is_err(),
                "accepted proper prefix ending at {end}"
            );
        }
        let mut trailing = bytes.clone();
        trailing.push(0);
        assert!(matches!(
            PhysicalSensorObservation::decode_canonical(&trailing),
            Err(PhysicalSensorObservationError::TrailingBytes { count: 1 })
        ));

        let mut unknown_state = bytes.clone();
        unknown_state[0] = 0xff;
        assert!(matches!(
            PhysicalSensorObservation::decode_canonical(&unknown_state),
            Err(PhysicalSensorObservationError::UnknownEvidenceState { code: 0xff })
        ));

        let mut policy_mutation = bytes;
        policy_mutation[1] ^= 1;
        assert!(matches!(
            PhysicalSensorObservation::decode_canonical(&policy_mutation),
            Err(PhysicalSensorObservationError::PolicyTagMismatch)
        ));
    }

    #[test]
    fn challenge_rejects_duplicate_out_of_order_and_non_xrgb_coordinates() {
        assert!(matches!(
            SensorChallenge::new(vec![SensorChallengePoint::new(0, 0, 0xff00_0000)]),
            Err(PhysicalSensorObservationError::InvalidChallengeXrgb { .. })
        ));
        assert!(matches!(
            SensorChallenge::new(vec![
                SensorChallengePoint::new(1, 0, PIXELS[1]),
                SensorChallengePoint::new(0, 0, PIXELS[0]),
            ]),
            Err(PhysicalSensorObservationError::NonCanonicalChallengeOrder { index: 1 })
        ));
        assert!(matches!(
            SensorChallenge::new(vec![
                SensorChallengePoint::new(0, 0, PIXELS[0]),
                SensorChallengePoint::new(0, 0, PIXELS[0]),
            ]),
            Err(PhysicalSensorObservationError::NonCanonicalChallengeOrder { index: 1 })
        ));
    }

    #[test]
    fn rgb_transition_challenge_selects_canonical_highest_edges_and_requires_exact_parent() {
        let pixels = [0x0000_0000, 0x0000_0000, 0x0000_0000, 0x00ff_ffff];
        let parent = presentation_with_pixels(2, 2, &pixels, true);
        let selected = RgbGradientChallenge::from_exact_presentation(&parent, 2).unwrap();

        assert_eq!(selected.parent_tag(), parent.tag());
        assert_eq!(
            selected.policy_tag().to_string(),
            "#bb.policy.sha256.3fhlsd4o5bd75gk2mdfuf2aerog44kqj3z6e5w6kzr24emxryltq"
        );
        assert_eq!(selected.point_budget(), 2);
        assert_eq!(
            selected.challenge().points(),
            [
                SensorChallengePoint::new(1, 0, pixels[1]),
                SensorChallengePoint::new(0, 1, pixels[2]),
            ]
        );
        assert_eq!(
            selected.facts(),
            RgbGradientFacts {
                inspected_pixels: 4,
                horizontal_adjacencies: 2,
                vertical_adjacencies: 2,
                nonzero_adjacencies: 2,
                total_rgb_delta: 1_530,
                maximum_adjacency_delta: 765,
                maximum_point_score: 765,
            }
        );
        assert_eq!(
            selected.challenge().tag().to_string(),
            "#bb.policy.sha256.n2v2w67sapkkqtrkuwgt5p4ksuug6rinjyklrimetjjxk3pptruq"
        );

        // Equal edge scores choose the earlier row before the earlier column, then output is
        // canonical row-major order. A color or luminance-only ranking would not satisfy this.
        let one = RgbGradientChallenge::from_exact_presentation(&parent, 1).unwrap();
        assert_eq!(
            one.challenge().points(),
            [SensorChallengePoint::new(1, 0, pixels[1])]
        );
        assert_eq!(
            one.challenge().tag().to_string(),
            "#bb.policy.sha256.3spwnsqwiesp2d3zjl2el34r2mbcdroqdvdwebkwxpfxcyiqsgpq"
        );
        assert!(matches!(
            RgbGradientChallenge::from_exact_presentation(&parent, 0),
            Err(RgbGradientChallengeError::PointBudget { actual: 0, .. })
        ));
        assert!(matches!(
            RgbGradientChallenge::from_exact_presentation(&presentation(false), 1),
            Err(RgbGradientChallengeError::NoExactDesktopRaster)
        ));
    }

    #[test]
    fn independent_python_known_answer_tags_match_the_rust_codec() {
        assert_eq!(
            physical_sensor_policy_tag().to_string(),
            "#bb.policy.sha256.o5lo24retmb5tnqnsjxh2defm2za6txgcntog5tpzxeannjzd4dq"
        );
        assert_eq!(
            PhysicalSensorObservation::not_applicable()
                .tag()
                .to_string(),
            "#bb.physical-sensor-observation.sha256.ojrmsd5622e42zrvqwlcelaia5kzrvl3v4b6yglg2s27dmzm4uvq"
        );

        let profile = DeclaredSensorProfile::new(DeclaredSensorKind::Colorimeter, 0, 2, 4);
        assert_eq!(
            profile.tag().to_string(),
            "#bb.policy.sha256.wkxyvnyqua22wdm6cbv4nkt3lttnazsbwrq4jse5zrgnyynqob5q"
        );
        let challenge = challenge();
        assert_eq!(
            challenge.tag().to_string(),
            "#bb.policy.sha256.5a5ldpfnswugam6677eqhtmu7a34cawh5fhh52chadkudd5apnpq"
        );
        let parent = ContentTag::from_digest(
            TagKind::PresentationOutputArtifact,
            Sha256::digest(b"bitblock/physical-sensor-vector-parent/v1").into(),
        );
        let observation = PhysicalSensorObservation::reported_samples(
            parent,
            profile,
            challenge,
            vec![
                SensorRgbSample::new(0x1111, 0x2222, 0x3333),
                SensorRgbSample::new(0x4444, 0x5555, 0x6666),
            ],
            b"vector-raw".to_vec(),
        )
        .unwrap();
        assert_eq!(
            observation.tag().to_string(),
            "#bb.physical-sensor-observation.sha256.cbwsp5lz3nl4ux3spzakh57ag46takzn4m6yv42tjdmtl45nv2ta"
        );
    }

    #[test]
    fn out_of_bounds_challenge_and_wrong_sample_count_are_refused_before_a_match() {
        let parent = presentation(true);
        let out_of_bounds = SensorChallenge::new(vec![
            SensorChallengePoint::new(0, 0, PIXELS[0]),
            SensorChallengePoint::new(2, 0, 0),
        ])
        .unwrap();
        let coordinate_mismatch = PhysicalSensorObservation::reported_samples(
            parent.tag(),
            profile(0),
            out_of_bounds,
            exact_samples(),
            Vec::new(),
        )
        .unwrap();
        assert!(matches!(
            coordinate_mismatch.verify_against(Some(&parent)).unwrap(),
            PhysicalSensorBinding::ChallengeMismatch(
                SensorChallengeMismatch::CoordinateOutOfBounds { point_index: 1, .. }
            )
        ));

        assert!(matches!(
            PhysicalSensorObservation::reported_samples(
                parent.tag(),
                profile(0),
                challenge(),
                vec![SensorRgbSample::new(0, 0, 0)],
                Vec::new(),
            ),
            Err(PhysicalSensorObservationError::InvalidStateShape {
                state: SensorEvidenceState::ReportedSamples,
                ..
            })
        ));
    }

    #[test]
    fn not_applicable_cannot_be_reencoded_as_reported_sensor_data() {
        let mut bytes = PhysicalSensorObservation::not_applicable()
            .encode_canonical()
            .unwrap();
        // state | policy digest | parent marker | profile marker | challenge marker |
        // sample count | raw length. Insert one sample before the retained zero raw-length lane.
        const SAMPLE_COUNT_OFFSET: usize = 1 + TAG_BYTES + 3;
        const RAW_LENGTH_OFFSET: usize = SAMPLE_COUNT_OFFSET + U16_BYTES;
        bytes[SAMPLE_COUNT_OFFSET..RAW_LENGTH_OFFSET].copy_from_slice(&1_u16.to_be_bytes());
        bytes.splice(RAW_LENGTH_OFFSET..RAW_LENGTH_OFFSET, [0_u8; SAMPLE_BYTES]);
        assert!(matches!(
            PhysicalSensorObservation::decode_canonical(&bytes),
            Err(PhysicalSensorObservationError::InvalidStateShape {
                state: SensorEvidenceState::NotApplicable,
                ..
            })
        ));
    }
}
