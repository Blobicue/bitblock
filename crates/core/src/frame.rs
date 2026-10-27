//! Canonical observation of the final CPU-side framebuffer.
//!
//! The renderer stores one row-major `u32` per physical pixel. Only the low 24 bits are RGB;
//! the high byte must remain zero. This module gives that slice an explicit, testable protocol
//! boundary without claiming that submission to the window system proves monitor output.

use std::fmt;

use crate::camera::Camera;
use crate::protocol::{ContentTag, TagKind, hash_canonical};

/// Domain prefix for the canonical frame byte stream.
///
/// The stream is: two-byte big-endian domain length, these domain bytes, big-endian width,
/// height and pixel count, followed by every complete XRGB word in row-major big-endian order.
pub const CANONICAL_FRAME_DOMAIN: &[u8] = b"bitblock/frame/xrgb8888-row-major/v1";
/// Canonical schema identifier for `OverlayInput` policy and dynamic payload bindings.
pub const OVERLAY_INPUT_SCHEMA: &[u8] = b"bitblock/overlay-input/v1";
/// Canonical schema identifier for `FrameInput` renderer invocation commitments.
pub const FRAME_INPUT_SCHEMA: &[u8] = b"bitblock/frame-input/v1";
/// Canonical schema identifier for v1 `FrameReceipt` submitted-frame provenance records.
pub const FRAME_RECEIPT_SCHEMA: &[u8] = b"bitblock/frame-receipt/v1";

const RGB_MASK: u32 = 0x00ff_ffff;
const ENCODED_PIXEL_BATCH: usize = 1024;
const OVERLAY_INPUT_PAYLOAD_LEN: u64 = 65;
const FRAME_INPUT_PAYLOAD_LEN: u64 = 184;
const FRAME_RECEIPT_PAYLOAD_LEN: u64 = 105;

/// A dimension-checked view of a complete CPU framebuffer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FrameRef<'a> {
    width: u32,
    height: u32,
    xrgb: &'a [u32],
}

impl<'a> FrameRef<'a> {
    /// Constructs a validated `FrameRef` after checking that `xrgb.len()` equals `width * height`.
    pub fn new(width: u32, height: u32, xrgb: &'a [u32]) -> Result<Self, FrameShapeError> {
        let expected = u64::from(width) * u64::from(height);
        if expected != xrgb.len() as u64 {
            return Err(FrameShapeError::PixelCount {
                width,
                height,
                expected,
                actual: xrgb.len(),
            });
        }
        Ok(Self {
            width,
            height,
            xrgb,
        })
    }

    /// Returns the horizontal width in pixels of this validated CPU framebuffer view.
    pub const fn width(self) -> u32 {
        self.width
    }

    /// Returns the vertical height in pixels of this validated CPU framebuffer view.
    pub const fn height(self) -> u32 {
        self.height
    }

    /// Returns the underlying row-major XRGB8888 pixel slice of this framebuffer view.
    pub const fn pixels(self) -> &'a [u32] {
        self.xrgb
    }

    /// Computes RGB channel sums, set-bit counts, and high-byte validity facts across `self.pixels()`.
    pub fn facts(self) -> FrameFacts {
        let mut facts = FrameFacts {
            pixel_count: self.xrgb.len() as u64,
            ..FrameFacts::default()
        };

        for &word in self.xrgb {
            let rgb = word & RGB_MASK;
            facts.invalid_high_byte += u64::from(word & !RGB_MASK != 0);
            facts.non_background += u64::from(rgb != 0);
            facts.rgb_one_bits += u64::from(rgb.count_ones());
            facts.red_sum += u64::from((rgb >> 16) & 0xff);
            facts.green_sum += u64::from((rgb >> 8) & 0xff);
            facts.blue_sum += u64::from(rgb & 0xff);
        }

        facts
    }

    /// Feeds the canonical byte stream to `sink` without allocating a second frame.
    pub fn encode_canonical(self, mut sink: impl FnMut(&[u8])) {
        self.encode_with(&mut sink, |_| {});
    }

    fn encode_with(self, sink: &mut dyn FnMut(&[u8]), mut inspect: impl FnMut(u32)) {
        let domain_len = u16::try_from(CANONICAL_FRAME_DOMAIN.len())
            .expect("canonical frame domain length exceeds u16");
        sink(&domain_len.to_be_bytes());
        sink(CANONICAL_FRAME_DOMAIN);
        sink(&self.width.to_be_bytes());
        sink(&self.height.to_be_bytes());
        sink(&(self.xrgb.len() as u64).to_be_bytes());
        let mut encoded = [0_u8; ENCODED_PIXEL_BATCH * 4];
        for words in self.xrgb.chunks(ENCODED_PIXEL_BATCH) {
            for (bytes, &word) in encoded.chunks_exact_mut(4).zip(words) {
                inspect(word);
                bytes.copy_from_slice(&word.to_be_bytes());
            }
            sink(&encoded[..words.len() * 4]);
        }
    }

    /// Returns the exact byte length of the canonical `CANONICAL_FRAME_DOMAIN` stream for `self`.
    pub fn canonical_len(self) -> usize {
        2 + CANONICAL_FRAME_DOMAIN.len() + 4 + 4 + 8 + self.xrgb.len() * 4
    }

    /// Commits the complete canonical frame under the standard authority suite.
    pub fn content_tag(self) -> ContentTag {
        self.observe().tag
    }

    /// Scans and tags every final pixel in one pass.
    pub fn observe(self) -> FrameObservation {
        self.observe_with(|_, _| {})
    }

    /// Scans and tags every final pixel in one pass while exposing its canonical row-major words
    /// to a caller-owned derived audit.
    ///
    /// The callback is a projection hook only: it cannot alter the hashed bytes, frame facts, or
    /// tag. This avoids a second framebuffer walk for a renderer-specific metric while preserving
    /// the ordinary [`Self::observe`] result exactly.
    pub fn observe_with(self, mut inspect: impl FnMut(usize, u32)) -> FrameObservation {
        let mut facts = FrameFacts {
            pixel_count: self.xrgb.len() as u64,
            ..FrameFacts::default()
        };
        let mut index = 0_usize;
        let tag = hash_canonical(
            TagKind::Frame,
            CANONICAL_FRAME_DOMAIN,
            self.canonical_len() as u64,
            |sink| {
                self.encode_with(sink, |word| {
                    let rgb = word & RGB_MASK;
                    facts.invalid_high_byte += u64::from(word & !RGB_MASK != 0);
                    facts.non_background += u64::from(rgb != 0);
                    facts.rgb_one_bits += u64::from(rgb.count_ones());
                    facts.red_sum += u64::from((rgb >> 16) & 0xff);
                    facts.green_sum += u64::from((rgb >> 8) & 0xff);
                    facts.blue_sum += u64::from(rgb & 0xff);
                    inspect(index, word);
                    index += 1;
                });
            },
        )
        .expect("validated frame canonical length must match its encoder");
        debug_assert_eq!(index, self.xrgb.len());
        FrameObservation { facts, tag }
    }
}

/// Combined single-pass `FrameFacts` summary and canonical `Frame` content tag for a framebuffer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FrameObservation {
    pub facts: FrameFacts,
    pub tag: ContentTag,
}

/// The exact overlay rule and optional dynamic payload used to compose a frame.
///
/// The current crosshair has no dynamic payload. A later sidebar can bind its complete rendered
/// data here, including only the predecessor receipt rather than the receipt being constructed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct OverlayInput {
    policy: ContentTag,
    payload: Option<ContentTag>,
    tag: ContentTag,
}

impl OverlayInput {
    /// Constructs a canonical `OverlayInput` commitment from a `Policy` tag and optional payload tag.
    pub fn new(
        policy: ContentTag,
        payload: Option<ContentTag>,
    ) -> Result<Self, FrameProtocolError> {
        require_kind("overlay policy", policy, TagKind::Policy)?;
        let tag = overlay_input_tag(policy, payload);
        Ok(Self {
            policy,
            payload,
            tag,
        })
    }

    /// Returns the `Policy` content tag governing the overlay rasterization rules.
    pub const fn policy(self) -> ContentTag {
        self.policy
    }

    /// Returns the optional dynamic payload content tag bound to this overlay input.
    pub const fn payload(self) -> Option<ContentTag> {
        self.payload
    }

    /// Returns the canonical `Policy` content tag representing this `OverlayInput` commitment.
    pub const fn tag(self) -> ContentTag {
        self.tag
    }

    /// Verifies that `self.policy` has kind `Policy` and `self.tag` matches the canonical encoding.
    pub fn verify(self) -> Result<(), FrameProtocolError> {
        require_kind("overlay policy", self.policy, TagKind::Policy)?;
        if self.tag != self.recompute_tag() {
            return Err(FrameProtocolError::OverlayInputTagMismatch);
        }
        Ok(())
    }

    fn recompute_tag(self) -> ContentTag {
        overlay_input_tag(self.policy, self.payload)
    }
}

fn overlay_input_tag(policy: ContentTag, payload: Option<ContentTag>) -> ContentTag {
    hash_canonical(
        TagKind::Policy,
        OVERLAY_INPUT_SCHEMA,
        OVERLAY_INPUT_PAYLOAD_LEN,
        |sink| {
            sink(&policy.digest());
            match payload {
                Some(payload) => {
                    sink(&[1]);
                    sink(&payload.digest());
                }
                None => {
                    sink(&[0]);
                    sink(&[0; 32]);
                }
            }
        },
    )
    .expect("overlay input encoding has a fixed length")
}

/// Bit-exact snapshot of every stored camera field that can affect or contextualize a frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct CameraBits([u64; 10]);

impl CameraBits {
    fn capture(camera: &Camera) -> Self {
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
}

/// Canonical input commitment captured beside one renderer invocation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FrameInput {
    world: ContentTag,
    camera: CameraBits,
    width: u32,
    height: u32,
    renderer_profile: ContentTag,
    overlay_input: ContentTag,
    tag: ContentTag,
}

impl FrameInput {
    /// Captures a canonical `FrameInput` commitment from `world`, `camera`, viewport dimensions, and policies.
    pub fn capture(
        world: ContentTag,
        camera: &Camera,
        width: u32,
        height: u32,
        renderer_profile: ContentTag,
        overlay_input: ContentTag,
    ) -> Result<Self, FrameProtocolError> {
        Self::from_camera_bits(
            world,
            CameraBits::capture(camera),
            width,
            height,
            renderer_profile,
            overlay_input,
        )
    }

    /// Reconstructs the canonical input commitment from already captured camera bit lanes.
    ///
    /// The lane order is position xyz, yaw, pitch, anchor xyz, observation distance, and
    /// vertical FOV. This is the semantic-verifier seam for protocols that already commit those
    /// exact bits; it uses the same v1 encoder as [`FrameInput::capture`].
    pub fn from_camera_bit_lanes(
        world: ContentTag,
        camera: [u64; 10],
        width: u32,
        height: u32,
        renderer_profile: ContentTag,
        overlay_input: ContentTag,
    ) -> Result<Self, FrameProtocolError> {
        Self::from_camera_bits(
            world,
            CameraBits(camera),
            width,
            height,
            renderer_profile,
            overlay_input,
        )
    }

    fn from_camera_bits(
        world: ContentTag,
        camera: CameraBits,
        width: u32,
        height: u32,
        renderer_profile: ContentTag,
        overlay_input: ContentTag,
    ) -> Result<Self, FrameProtocolError> {
        require_kind("frame world", world, TagKind::World)?;
        require_kind("renderer profile", renderer_profile, TagKind::Policy)?;
        require_kind("overlay input", overlay_input, TagKind::Policy)?;
        let tag = frame_input_tag(
            world,
            camera,
            width,
            height,
            renderer_profile,
            overlay_input,
        );
        Ok(Self {
            world,
            camera,
            width,
            height,
            renderer_profile,
            overlay_input,
            tag,
        })
    }

    /// Returns the `World` content tag committed inside this `FrameInput`.
    pub const fn world(self) -> ContentTag {
        self.world
    }

    /// Returns the `(width, height)` physical viewport dimensions in pixels committed by `self`.
    pub const fn viewport(self) -> (u32, u32) {
        (self.width, self.height)
    }

    /// Returns the `Policy` content tag identifying the renderer profile for this frame input.
    pub const fn renderer_profile(self) -> ContentTag {
        self.renderer_profile
    }

    /// Returns the `Policy` content tag identifying the `OverlayInput` commitment for this frame.
    pub const fn overlay_input(self) -> ContentTag {
        self.overlay_input
    }

    /// Returns the canonical `FrameInput` content tag computed over all committed input fields.
    pub const fn tag(self) -> ContentTag {
        self.tag
    }

    /// Verifies that all bound tags have their required kinds and `self.tag` matches the canonical digest.
    pub fn verify(self) -> Result<(), FrameProtocolError> {
        require_kind("frame world", self.world, TagKind::World)?;
        require_kind("renderer profile", self.renderer_profile, TagKind::Policy)?;
        require_kind("overlay input", self.overlay_input, TagKind::Policy)?;
        if self.tag != self.recompute_tag() {
            return Err(FrameProtocolError::FrameInputTagMismatch);
        }
        Ok(())
    }

    fn recompute_tag(self) -> ContentTag {
        frame_input_tag(
            self.world,
            self.camera,
            self.width,
            self.height,
            self.renderer_profile,
            self.overlay_input,
        )
    }
}

fn frame_input_tag(
    world: ContentTag,
    camera: CameraBits,
    width: u32,
    height: u32,
    renderer_profile: ContentTag,
    overlay_input: ContentTag,
) -> ContentTag {
    hash_canonical(
        TagKind::FrameInput,
        FRAME_INPUT_SCHEMA,
        FRAME_INPUT_PAYLOAD_LEN,
        |sink| {
            sink(&world.digest());
            for bits in camera.0 {
                sink(&bits.to_be_bytes());
            }
            sink(&width.to_be_bytes());
            sink(&height.to_be_bytes());
            sink(&renderer_profile.digest());
            sink(&overlay_input.digest());
        },
    )
    .expect("frame input encoding has a fixed length")
}

/// A commitment to one changed CPU frame after its presentation call returned successfully.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FrameReceipt {
    tag: ContentTag,
    parent: Option<ContentTag>,
    source_tick: u64,
    input: ContentTag,
    frame: ContentTag,
}

impl FrameReceipt {
    /// Constructs a canonical v1 `FrameReceipt` binding `parent`, `source_tick`, `input`, and `frame`.
    pub fn new(
        parent: Option<ContentTag>,
        source_tick: u64,
        input: ContentTag,
        frame: ContentTag,
    ) -> Result<Self, FrameProtocolError> {
        if let Some(parent) = parent {
            require_kind("frame receipt parent", parent, TagKind::FrameReceipt)?;
        }
        require_kind("frame receipt input", input, TagKind::FrameInput)?;
        require_kind("frame receipt frame", frame, TagKind::Frame)?;
        Ok(Self {
            tag: frame_receipt_tag(parent, source_tick, input, frame),
            parent,
            source_tick,
            input,
            frame,
        })
    }

    /// Returns the canonical `FrameReceipt` content tag for this v1 submitted-frame receipt.
    pub const fn tag(self) -> ContentTag {
        self.tag
    }

    /// Returns the optional parent `FrameReceipt` content tag in the v1 frame ledger chain.
    pub const fn parent(self) -> Option<ContentTag> {
        self.parent
    }

    /// Returns the monotonically increasing simulation `source_tick` index for this v1 receipt.
    pub const fn source_tick(self) -> u64 {
        self.source_tick
    }

    /// Returns the `FrameInput` content tag committed by this v1 frame receipt.
    pub const fn input(self) -> ContentTag {
        self.input
    }

    /// Returns the canonical XRGB8888 `Frame` content tag committed by this v1 frame receipt.
    pub const fn frame(self) -> ContentTag {
        self.frame
    }

    fn recompute_tag(self) -> ContentTag {
        frame_receipt_tag(self.parent, self.source_tick, self.input, self.frame)
    }
}

fn frame_receipt_tag(
    parent: Option<ContentTag>,
    source_tick: u64,
    input: ContentTag,
    frame: ContentTag,
) -> ContentTag {
    hash_canonical(
        TagKind::FrameReceipt,
        FRAME_RECEIPT_SCHEMA,
        FRAME_RECEIPT_PAYLOAD_LEN,
        |sink| {
            match parent {
                Some(parent) => {
                    sink(&[1]);
                    sink(&parent.digest());
                }
                None => {
                    sink(&[0]);
                    sink(&[0; 32]);
                }
            }
            sink(&source_tick.to_be_bytes());
            sink(&input.digest());
            sink(&frame.digest());
        },
    )
    .expect("frame receipt encoding has a fixed length")
}

/// Append-only local chain of successfully submitted changed CPU frames.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FrameLedger {
    receipts: Vec<FrameReceipt>,
}

impl FrameLedger {
    /// Borrows the ordered slice of committed v1 `FrameReceipt` entries in this frame ledger.
    pub fn receipts(&self) -> &[FrameReceipt] {
        &self.receipts
    }

    /// Returns `true` when this v1 frame ledger contains no committed frame receipts.
    pub fn is_empty(&self) -> bool {
        self.receipts.is_empty()
    }

    /// Returns the number of v1 `FrameReceipt` entries currently recorded in this frame ledger.
    pub fn len(&self) -> usize {
        self.receipts.len()
    }

    /// Returns the `FrameReceipt` content tag at the tip of this v1 ledger, or `None` if empty.
    pub fn head(&self) -> Option<ContentTag> {
        self.receipts.last().map(|receipt| receipt.tag)
    }

    /// Verifies parent linkage, tick monotonicity, and digest integrity before appending `receipt`.
    pub fn append(&mut self, receipt: FrameReceipt) -> Result<&FrameReceipt, FrameProtocolError> {
        let expected_parent = self.head();
        if receipt.parent != expected_parent {
            return Err(FrameProtocolError::ParentMismatch {
                expected: expected_parent,
                actual: receipt.parent,
            });
        }
        if let Some(previous) = self.receipts.last()
            && receipt.source_tick <= previous.source_tick
        {
            return Err(FrameProtocolError::NonIncreasingTick {
                previous: previous.source_tick,
                next: receipt.source_tick,
            });
        }
        if receipt.tag != receipt.recompute_tag() {
            return Err(FrameProtocolError::ReceiptTagMismatch);
        }
        self.receipts.push(receipt);
        Ok(self
            .receipts
            .last()
            .expect("a just-appended frame receipt must exist"))
    }

    /// Re-verifies parent linkage, strictly increasing `source_tick` order, and tags across all receipts.
    pub fn verify(&self) -> Result<(), FrameProtocolError> {
        let mut rebuilt = Self::default();
        for &receipt in &self.receipts {
            rebuilt.append(receipt)?;
        }
        Ok(())
    }
}

/// Protocol error returned when constructing or verifying `OverlayInput`, `FrameInput`, or `FrameReceipt`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FrameProtocolError {
    WrongKind {
        field: &'static str,
        expected: TagKind,
        actual: TagKind,
    },
    ParentMismatch {
        expected: Option<ContentTag>,
        actual: Option<ContentTag>,
    },
    NonIncreasingTick {
        previous: u64,
        next: u64,
    },
    OverlayInputTagMismatch,
    FrameInputTagMismatch,
    ReceiptTagMismatch,
}

impl fmt::Display for FrameProtocolError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {
            Self::WrongKind {
                field,
                expected,
                actual,
            } => write!(
                formatter,
                "{field} requires a {} tag, received {}",
                expected.name(),
                actual.name()
            ),
            Self::ParentMismatch { expected, actual } => write!(
                formatter,
                "frame receipt parent mismatch: expected {expected:?}, received {actual:?}"
            ),
            Self::NonIncreasingTick { previous, next } => write!(
                formatter,
                "frame receipt tick must increase: previous {previous}, next {next}"
            ),
            Self::OverlayInputTagMismatch => {
                formatter.write_str("overlay input tag does not match its canonical fields")
            }
            Self::FrameInputTagMismatch => {
                formatter.write_str("frame input tag does not match its canonical fields")
            }
            Self::ReceiptTagMismatch => {
                formatter.write_str("frame receipt tag does not match its canonical fields")
            }
        }
    }
}

impl std::error::Error for FrameProtocolError {}

fn require_kind(
    field: &'static str,
    tag: ContentTag,
    expected: TagKind,
) -> Result<(), FrameProtocolError> {
    if tag.kind() != expected {
        return Err(FrameProtocolError::WrongKind {
            field,
            expected,
            actual: tag.kind(),
        });
    }
    Ok(())
}

/// Deterministic pixel, RGB channel-sum, and high-byte validity statistics for an XRGB8888 framebuffer.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct FrameFacts {
    /// Complete physical-pixel count, including background and overlay pixels.
    pub pixel_count: u64,
    /// Pixels whose RGB payload differs from black.
    pub non_background: u64,
    /// Words with any set bit outside the declared 24-bit RGB payload.
    pub invalid_high_byte: u64,
    /// Total set bits across all 24-bit RGB payloads.
    pub rgb_one_bits: u64,
    /// Exact sums of the three eight-bit channels over the complete frame.
    pub red_sum: u64,
    pub green_sum: u64,
    pub blue_sum: u64,
}

/// Dimension or slice-length mismatch returned when constructing or copying a `FrameRef`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FrameShapeError {
    PixelCount {
        width: u32,
        height: u32,
        expected: u64,
        actual: usize,
    },
    DestinationLength {
        expected: usize,
        actual: usize,
    },
}

impl fmt::Display for FrameShapeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {
            Self::PixelCount {
                width,
                height,
                expected,
                actual,
            } => write!(
                formatter,
                "{width}x{height} frame requires {expected} pixels, received {actual}"
            ),
            Self::DestinationLength { expected, actual } => write!(
                formatter,
                "frame copy requires {expected} destination pixels, received {actual}"
            ),
        }
    }
}

impl std::error::Error for FrameShapeError {}

/// Copies the complete validated CPU frame to another XRGB surface.
///
/// Equality immediately after this function is exact. A later backend `present` call is a
/// separate boundary and cannot establish what an OS compositor or physical display emitted.
pub fn copy_exact(destination: &mut [u32], frame: FrameRef<'_>) -> Result<(), FrameShapeError> {
    if destination.len() != frame.pixels().len() {
        return Err(FrameShapeError::DestinationLength {
            expected: frame.pixels().len(),
            actual: destination.len(),
        });
    }
    destination.copy_from_slice(frame.pixels());
    Ok(())
}

/// The camera the independent frame vectors capture, in the declared lane order: position xyz,
/// yaw, pitch, anchor xyz, observation distance, vertical FOV.
///
/// Lane 1 is negative zero. It compares equal to lane 0 under every f64 comparison and differs
/// only in its bits, which is the property `FrameInput`'s bit-lane encoding exists to preserve.
/// `crates/core/src/sidebar.rs` names the same lanes so its composite vector contains exactly the
/// inner game input `tools/frame_chain_vectors.py` derives.
#[cfg(test)]
pub(crate) const VECTOR_CAMERA_LANES: [f64; 10] =
    [0.0, -0.0, 3.0, 0.25, -0.5, 6.0, 7.0, 8.0, 9.0, 1.0];

#[cfg(test)]
mod tests {
    use super::*;

    fn tag(kind: TagKind, label: &[u8]) -> ContentTag {
        hash_canonical(
            kind,
            b"bitblock/test-object/v1",
            label.len() as u64,
            |sink| sink(label),
        )
        .unwrap()
    }

    fn sample_input() -> FrameInput {
        let overlay = OverlayInput::new(tag(TagKind::Policy, b"overlay"), None).unwrap();
        FrameInput::from_camera_bits(
            tag(TagKind::World, b"world"),
            CameraBits([
                0.0_f64.to_bits(),
                (-0.0_f64).to_bits(),
                3.0_f64.to_bits(),
                0.25_f64.to_bits(),
                (-0.5_f64).to_bits(),
                6.0_f64.to_bits(),
                7.0_f64.to_bits(),
                8.0_f64.to_bits(),
                9.0_f64.to_bits(),
                1.0_f64.to_bits(),
            ]),
            801,
            603,
            tag(TagKind::Policy, b"renderer"),
            overlay.tag(),
        )
        .unwrap()
    }

    fn canonical_bytes(frame: FrameRef<'_>) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(frame.canonical_len());
        frame.encode_canonical(|part| bytes.extend_from_slice(part));
        bytes
    }

    #[test]
    fn shape_requires_exact_dimension_product() {
        let pixels = [0_u32; 3];
        assert_eq!(
            FrameRef::new(2, 2, &pixels),
            Err(FrameShapeError::PixelCount {
                width: 2,
                height: 2,
                expected: 4,
                actual: 3,
            })
        );
        assert!(FrameRef::new(3, 1, &pixels).is_ok());
    }

    #[test]
    fn facts_scan_every_complete_xrgb_word() {
        let pixels = [0x0000_0000, 0x0012_3456, 0xff01_0204, 0x00ff_ffff];
        let facts = FrameRef::new(2, 2, &pixels).unwrap().facts();

        assert_eq!(facts.pixel_count, 4);
        assert_eq!(facts.non_background, 3);
        assert_eq!(facts.invalid_high_byte, 1);
        assert_eq!(facts.rgb_one_bits, 9 + 3 + 24);
        assert_eq!(facts.red_sum, 0x12 + 0x01 + 0xff);
        assert_eq!(facts.green_sum, 0x34 + 0x02 + 0xff);
        assert_eq!(facts.blue_sum, 0x56 + 0x04 + 0xff);
    }

    #[test]
    fn callback_observation_is_row_major_and_keeps_the_standard_frame_result() {
        let pixels = [0x0000_0000, 0x0011_2233, 0x0044_5566, 0x0077_8899];
        let frame = FrameRef::new(2, 2, &pixels).unwrap();
        let mut seen = Vec::new();
        let observed = frame.observe_with(|index, word| seen.push((index, word)));

        assert_eq!(
            seen,
            [
                (0, 0x0000_0000),
                (1, 0x0011_2233),
                (2, 0x0044_5566),
                (3, 0x0077_8899),
            ]
        );
        assert_eq!(observed, frame.observe());
    }

    #[test]
    fn canonical_two_by_two_vector_is_fixed_and_big_endian() {
        let pixels = [0x0000_0000, 0x0011_2233, 0x0044_5566, 0x0077_8899];
        let frame = FrameRef::new(2, 2, &pixels).unwrap();
        let mut expected = Vec::new();
        expected.extend_from_slice(&(CANONICAL_FRAME_DOMAIN.len() as u16).to_be_bytes());
        expected.extend_from_slice(CANONICAL_FRAME_DOMAIN);
        expected.extend_from_slice(&2_u32.to_be_bytes());
        expected.extend_from_slice(&2_u32.to_be_bytes());
        expected.extend_from_slice(&4_u64.to_be_bytes());
        expected.extend_from_slice(&[
            0x00, 0x00, 0x00, 0x00, 0x00, 0x11, 0x22, 0x33, 0x00, 0x44, 0x55, 0x66, 0x00, 0x77,
            0x88, 0x99,
        ]);

        let actual = canonical_bytes(frame);
        assert_eq!(actual, expected);
        assert_eq!(actual.len(), frame.canonical_len());
    }

    #[test]
    fn canonical_stream_commits_position_and_high_byte() {
        let left = [0x0011_2233, 0xff44_5566];
        let reversed = [left[1], left[0]];
        let high_byte_cleared = [left[0], left[1] & RGB_MASK];

        let encode = |pixels: &[u32]| canonical_bytes(FrameRef::new(2, 1, pixels).unwrap());
        assert_ne!(encode(&left), encode(&reversed));
        assert_ne!(encode(&left), encode(&high_byte_cleared));
    }

    #[test]
    fn frame_tag_commits_dimensions_position_and_every_word() {
        let pixels = [0x0011_2233, 0x0044_5566];
        let reversed = [pixels[1], pixels[0]];
        let base = FrameRef::new(2, 1, &pixels).unwrap().content_tag();

        assert_eq!(base.kind(), TagKind::Frame);
        assert_eq!(base, FrameRef::new(2, 1, &pixels).unwrap().content_tag());
        assert_ne!(base, FrameRef::new(1, 2, &pixels).unwrap().content_tag());
        assert_ne!(base, FrameRef::new(2, 1, &reversed).unwrap().content_tag());
    }

    #[test]
    fn observation_fuses_the_same_facts_and_tag_into_one_pixel_pass() {
        let pixels = [0x0000_0000, 0x0012_3456, 0xff01_0204, 0x00ff_ffff];
        let frame = FrameRef::new(2, 2, &pixels).unwrap();
        let observation = frame.observe();

        assert_eq!(observation.facts, frame.facts());
        assert_eq!(observation.tag, frame.content_tag());
    }

    #[test]
    fn exact_copy_rejects_wrong_shape_and_preserves_every_word() {
        let pixels = [0x0011_2233, 0xff44_5566];
        let frame = FrameRef::new(2, 1, &pixels).unwrap();
        let mut too_short = [0_u32; 1];
        assert_eq!(
            copy_exact(&mut too_short, frame),
            Err(FrameShapeError::DestinationLength {
                expected: 2,
                actual: 1,
            })
        );

        let mut destination = [0_u32; 2];
        copy_exact(&mut destination, frame).unwrap();
        assert_eq!(destination, pixels);
    }

    #[test]
    fn overlay_and_frame_input_commit_every_declared_field() {
        let overlay_policy = tag(TagKind::Policy, b"overlay");
        let empty_overlay = OverlayInput::new(overlay_policy, None).unwrap();
        let payload_overlay =
            OverlayInput::new(overlay_policy, Some(tag(TagKind::FrameReceipt, b"prior"))).unwrap();
        assert_eq!(empty_overlay.tag().kind(), TagKind::Policy);
        assert_ne!(empty_overlay.tag(), payload_overlay.tag());
        assert!(empty_overlay.verify().is_ok());

        let base = sample_input();
        assert_eq!(base.tag().kind(), TagKind::FrameInput);
        assert!(base.verify().is_ok());

        for index in 0..base.camera.0.len() {
            let mut camera = base.camera;
            camera.0[index] ^= 1;
            let changed = FrameInput::from_camera_bits(
                base.world,
                camera,
                base.width,
                base.height,
                base.renderer_profile,
                base.overlay_input,
            )
            .unwrap();
            assert_ne!(base.tag(), changed.tag(), "camera bit field {index}");
        }

        let variants = [
            FrameInput::from_camera_bits(
                tag(TagKind::World, b"other-world"),
                base.camera,
                base.width,
                base.height,
                base.renderer_profile,
                base.overlay_input,
            )
            .unwrap(),
            FrameInput::from_camera_bits(
                base.world,
                base.camera,
                base.width + 1,
                base.height,
                base.renderer_profile,
                base.overlay_input,
            )
            .unwrap(),
            FrameInput::from_camera_bits(
                base.world,
                base.camera,
                base.width,
                base.height + 1,
                base.renderer_profile,
                base.overlay_input,
            )
            .unwrap(),
            FrameInput::from_camera_bits(
                base.world,
                base.camera,
                base.width,
                base.height,
                tag(TagKind::Policy, b"other-renderer"),
                base.overlay_input,
            )
            .unwrap(),
            FrameInput::from_camera_bits(
                base.world,
                base.camera,
                base.width,
                base.height,
                base.renderer_profile,
                payload_overlay.tag(),
            )
            .unwrap(),
        ];
        for variant in variants {
            assert_ne!(base.tag(), variant.tag());
        }
    }

    #[test]
    fn camera_signed_zero_is_preserved_by_frame_input() {
        let base = sample_input();
        let mut positive = base.camera;
        let mut negative = base.camera;
        positive.0[0] = 0.0_f64.to_bits();
        negative.0[0] = (-0.0_f64).to_bits();
        let make = |camera| {
            FrameInput::from_camera_bits(
                base.world,
                camera,
                base.width,
                base.height,
                base.renderer_profile,
                base.overlay_input,
            )
            .unwrap()
        };
        assert_ne!(make(positive).tag(), make(negative).tag());
    }

    #[test]
    fn public_camera_bit_lanes_use_the_exact_existing_frame_input_encoder() {
        let base = sample_input();
        let reconstructed = FrameInput::from_camera_bit_lanes(
            base.world,
            base.camera.0,
            base.width,
            base.height,
            base.renderer_profile,
            base.overlay_input,
        )
        .unwrap();
        assert_eq!(reconstructed, base);

        for lane in 0..10 {
            let mut camera = base.camera.0;
            camera[lane] ^= 1;
            let changed = FrameInput::from_camera_bit_lanes(
                base.world,
                camera,
                base.width,
                base.height,
                base.renderer_profile,
                base.overlay_input,
            )
            .unwrap();
            assert_ne!(
                changed.tag(),
                base.tag(),
                "camera lane {lane} was not bound"
            );
        }
    }

    #[test]
    fn nested_tag_kinds_fail_closed() {
        let policy = tag(TagKind::Policy, b"policy");
        let world = tag(TagKind::World, b"world");
        let overlay = OverlayInput::new(policy, None).unwrap();
        assert!(matches!(
            OverlayInput::new(world, None),
            Err(FrameProtocolError::WrongKind {
                expected: TagKind::Policy,
                actual: TagKind::World,
                ..
            })
        ));
        assert!(matches!(
            FrameInput::from_camera_bits(policy, CameraBits([0; 10]), 1, 1, policy, overlay.tag(),),
            Err(FrameProtocolError::WrongKind {
                expected: TagKind::World,
                actual: TagKind::Policy,
                ..
            })
        ));
        assert!(matches!(
            FrameReceipt::new(None, 0, world, tag(TagKind::Frame, b"frame")),
            Err(FrameProtocolError::WrongKind {
                expected: TagKind::FrameInput,
                actual: TagKind::World,
                ..
            })
        ));
    }

    #[test]
    fn frame_ledger_enforces_parent_tick_and_canonical_receipt() {
        let input = sample_input();
        let frame = FrameRef::new(1, 1, &[0x0012_3456]).unwrap().content_tag();
        let first = FrameReceipt::new(None, 4, input.tag(), frame).unwrap();
        let mut ledger = FrameLedger::default();
        ledger.append(first).unwrap();
        let second = FrameReceipt::new(ledger.head(), 9, input.tag(), frame).unwrap();
        ledger.append(second).unwrap();
        assert_eq!(ledger.len(), 2);
        assert_eq!(ledger.head(), Some(second.tag()));
        assert!(ledger.verify().is_ok());

        let wrong_parent = FrameReceipt::new(None, 10, input.tag(), frame).unwrap();
        assert!(matches!(
            ledger.append(wrong_parent),
            Err(FrameProtocolError::ParentMismatch { .. })
        ));
        let stale_tick = FrameReceipt::new(ledger.head(), 9, input.tag(), frame).unwrap();
        assert_eq!(
            ledger.append(stale_tick),
            Err(FrameProtocolError::NonIncreasingTick {
                previous: 9,
                next: 9,
            })
        );

        let mut altered = FrameReceipt::new(None, 4, input.tag(), frame).unwrap();
        altered.tag = FrameReceipt::new(None, 5, input.tag(), frame)
            .unwrap()
            .tag();
        assert_eq!(
            FrameLedger::default().append(altered),
            Err(FrameProtocolError::ReceiptTagMismatch)
        );
    }

    #[test]
    fn one_pixel_mutation_changes_the_complete_frame_and_receipt_tags() {
        let input = sample_input();
        let original = FrameRef::new(2, 1, &[0x0011_2233, 0x0044_5566])
            .unwrap()
            .content_tag();
        let mutated = FrameRef::new(2, 1, &[0x0011_2233, 0x0044_5567])
            .unwrap()
            .content_tag();
        assert_ne!(original, mutated);
        let original_receipt = FrameReceipt::new(None, 7, input.tag(), original).unwrap();
        let mutated_receipt = FrameReceipt::new(None, 7, input.tag(), mutated).unwrap();
        assert_ne!(original_receipt.tag(), mutated_receipt.tag());
    }

    #[test]
    fn frame_chain_matches_independent_python_sha256_vectors() {
        use crate::render::{crosshair_overlay_input, renderer_profile_tag};

        let input = FrameInput::from_camera_bits(
            tag(TagKind::World, b"world"),
            sample_input().camera,
            801,
            603,
            renderer_profile_tag(),
            crosshair_overlay_input().tag(),
        )
        .unwrap();
        assert_eq!(
            input.tag().to_string(),
            "#bb.frame-input.sha256.dwurbimkh2gdrpgrbk5ypce25mjlyrju5k6uufmg4ckshbtd3u6q"
        );

        let frame = FrameRef::new(1, 1, &[0x0012_3456]).unwrap().content_tag();
        assert_eq!(
            frame.to_string(),
            "#bb.frame.sha256.j7t5627svcsb3jhjgr65f7tfvhi6qbur2mcpt6bgptkrnaxozaca"
        );
        let first = FrameReceipt::new(None, 4, input.tag(), frame).unwrap();
        assert_eq!(
            first.tag().to_string(),
            "#bb.frame-receipt.sha256.v4wy4jdf6mensssn2nj34dze7oq5aqitfzv6hkpja5gcw7voneoq"
        );
        let second = FrameReceipt::new(Some(first.tag()), 9, input.tag(), frame).unwrap();
        assert_eq!(
            second.tag().to_string(),
            "#bb.frame-receipt.sha256.iod4acu4trlpx5tomtblxq7ahq66cpmfyfhqsqwguqj4ezxjxg7q"
        );
    }

    /// The literals below are derived by `tools/frame_chain_vectors.py`, which rebuilds both
    /// payloads from the schema and applies the content-tag wrapper without running any Rust.
    /// Every nested object is one some independent suite already reconstructs: the renderer
    /// profile and crosshair overlay from `tools/render_policy_vectors.py`, the framebuffer under
    /// the canonical frame domain. The world is a fully declared digest so the vector has no
    /// input this side alone could move.
    #[test]
    fn frame_input_and_receipt_v1_match_independent_python_vectors() {
        use crate::render::{crosshair_overlay_input, renderer_profile_tag};

        let world =
            ContentTag::from_digest(TagKind::World, std::array::from_fn(|index| index as u8));
        let input = FrameInput::from_camera_bit_lanes(
            world,
            VECTOR_CAMERA_LANES.map(f64::to_bits),
            801,
            801,
            renderer_profile_tag(),
            crosshair_overlay_input().tag(),
        )
        .unwrap();
        assert_eq!(
            input.tag().to_string(),
            "#bb.frame-input.sha256.oiwdnnaxuumavgducowfx7n4uc4gh4zoawgxj56zn5aaux2vzuvq"
        );

        let frame = FrameRef::new(1, 1, &[0x0012_3456]).unwrap().content_tag();
        let root = FrameReceipt::new(None, 4, input.tag(), frame).unwrap();
        assert_eq!(
            root.tag().to_string(),
            "#bb.frame-receipt.sha256.b765k2pnxuxwlwxbbp4w6tn6fdw7fa676e2fpgg5jnhvcfo42yxq"
        );
        let linked = FrameReceipt::new(Some(root.tag()), 9, input.tag(), frame).unwrap();
        assert_eq!(
            linked.tag().to_string(),
            "#bb.frame-receipt.sha256.jiwqu7gg62732shlykypv4q3xzaisywfaijtgjs6a5yzfo2ur5ga"
        );

        // An absent parent pads with 32 zero bytes, so a receipt naming an all-zero parent
        // differs from a root receipt only in the present marker.
        let zero_parent = FrameReceipt::new(
            Some(ContentTag::from_digest(TagKind::FrameReceipt, [0; 32])),
            4,
            input.tag(),
            frame,
        )
        .unwrap();
        assert_ne!(root.tag(), zero_parent.tag());

        // The renderer-profile and overlay-input lanes are both checked against
        // `TagKind::Policy`, which is the same check, so the constructor accepts either tag in
        // either position. Only their order in the canonical payload separates them, and that
        // is what this control pins.
        let swapped = FrameInput::from_camera_bit_lanes(
            world,
            VECTOR_CAMERA_LANES.map(f64::to_bits),
            801,
            801,
            crosshair_overlay_input().tag(),
            renderer_profile_tag(),
        )
        .expect("both lanes are typed as policy tags, so a swap is not refused");
        assert_ne!(input.tag(), swapped.tag());
    }
}
