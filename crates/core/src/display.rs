//! Opt-in observation of the OS-exposed desktop raster.
//!
//! A display receipt binds one screen-capture attempt to a successfully submitted CPU-frame
//! receipt. A matching GDI screen-DC sample observes the visible desktop rectangle, including
//! occlusion. It does not prove compositor scanout, monitor electronics, or emitted photons.

use std::fmt;

#[cfg(any(windows, test))]
use crate::frame::FrameRef;
use crate::frame::{FrameFacts, FrameObservation};
use crate::frame_v2::{FrameReceiptV2, FrameReceiptV2Error, FrameStream};
use crate::protocol::{ContentTag, TagKind, hash_canonical};

const DISPLAY_POLICY_SCHEMA: &[u8] = b"bitblock/display-capture-policy/gdi-screen-dc/v1";
const DISPLAY_RECEIPT_SCHEMA: &[u8] = b"bitblock/display-receipt/v1";
const DISPLAY_RECEIPT_V2_SCHEMA: &[u8] = b"bitblock/display-receipt/v2";
const DISPLAY_POLICY_PAYLOAD_LEN: u64 = 13;
const DISPLAY_RECEIPT_PAYLOAD_LEN: u64 = 312;
pub const DISPLAY_RECEIPT_V2_PAYLOAD_LEN: usize = 378;
#[cfg(any(windows, test))]
const RGB_MASK: u32 = 0x00ff_ffff;
const SRCCOPY_CAPTUREBLT: u32 = 0x40cc_0020;

/// Declared semantics of the Windows GDI screen-DC audit path.
pub fn display_capture_policy_tag() -> ContentTag {
    hash_canonical(
        TagKind::Policy,
        DISPLAY_POLICY_SCHEMA,
        DISPLAY_POLICY_PAYLOAD_LEN,
        |sink| {
            sink(&1_u16.to_be_bytes()); // policy revision
            sink(&[0]); // GDI screen DC, not a window rerender
            sink(&[0]); // top-down 32-bit BI_RGB DIB
            sink(&SRCCOPY_CAPTUREBLT.to_be_bytes());
            sink(&[0]); // mask the unused high byte to canonical XRGB
            sink(&[0]); // compare complete slices before interpreting tags
            sink(&[0]); // require stable client geometry wholly on one monitor
            sink(&[0]); // QueryPerformanceCounter interval
            sink(&[0]); // DwmFlush composition barrier before screen readback
        },
    )
    .expect("display capture policy encoding has a fixed length")
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CaptureRect {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
}

impl CaptureRect {
    pub fn width(self) -> Option<u32> {
        u32::try_from(self.right.checked_sub(self.left)?).ok()
    }

    pub fn height(self) -> Option<u32> {
        u32::try_from(self.bottom.checked_sub(self.top)?).ok()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum DisplayOutcome {
    Match = 0,
    Mismatch = 1,
    Unavailable = 2,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum CaptureStage {
    Complete = 0,
    WindowHandle = 1,
    FlushBefore = 2,
    Minimized = 3,
    GeometryBefore = 4,
    MonitorBoundary = 5,
    Dpi = 6,
    Shape = 7,
    PerformanceClock = 8,
    ScreenDc = 9,
    MemoryDc = 10,
    DibSection = 11,
    SelectBitmap = 12,
    BitBlt = 13,
    FlushAfter = 14,
    GeometryAfter = 15,
    CompositorFlush = 16,
}

/// Bound facts from one capture attempt. Failure stages are first-class results and are never
/// retried away by this layer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DisplayCaptureFacts {
    pub outcome: DisplayOutcome,
    pub stage: CaptureStage,
    /// Win32 last-error snapshot, or HRESULT bits for an HRESULT-returning failing boundary.
    pub error_code: u32,
    pub flush_before: bool,
    pub compositor_flush: bool,
    pub flush_after: bool,
    pub rect_before: CaptureRect,
    pub rect_after: CaptureRect,
    pub dpi: u32,
    pub qpc_start: i64,
    pub qpc_end: i64,
    pub qpc_frequency: i64,
    pub captured: Option<FrameObservation>,
    pub mismatch_count: u64,
    pub first_mismatch: Option<u64>,
}

impl DisplayCaptureFacts {
    pub const fn unavailable(stage: CaptureStage, error_code: u32) -> Self {
        Self {
            outcome: DisplayOutcome::Unavailable,
            stage,
            error_code,
            flush_before: false,
            compositor_flush: false,
            flush_after: false,
            rect_before: CaptureRect {
                left: 0,
                top: 0,
                right: 0,
                bottom: 0,
            },
            rect_after: CaptureRect {
                left: 0,
                top: 0,
                right: 0,
                bottom: 0,
            },
            dpi: 0,
            qpc_start: 0,
            qpc_end: 0,
            qpc_frequency: 0,
            captured: None,
            mismatch_count: 0,
            first_mismatch: None,
        }
    }
}

/// Transient captured artifact plus the facts committed by its receipt.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DisplayCapture {
    pub facts: DisplayCaptureFacts,
    pub pixels: Option<Vec<u32>>,
}

impl DisplayCapture {
    pub fn unavailable(stage: CaptureStage, error_code: u32) -> Self {
        Self {
            facts: DisplayCaptureFacts::unavailable(stage, error_code),
            pixels: None,
        }
    }
}

#[cfg(any(windows, test))]
/// One owned desktop readback after its pixels have been canonicalized and observed exactly once.
///
/// Keeping the pixels and their observation together prevents successful capture finalization
/// from normalizing and hashing the same framebuffer a second time. Later failure stages retain
/// this exact observation alongside the canonical pixels they already produced.
struct CanonicalCapture {
    pixels: Vec<u32>,
    observation: FrameObservation,
}

#[cfg(any(windows, test))]
/// Compares one already-canonicalized desktop readback against the expected XRGB frame.
fn finalize_capture(
    expected: FrameRef<'_>,
    capture: CanonicalCapture,
    mut facts: DisplayCaptureFacts,
) -> DisplayCapture {
    let CanonicalCapture {
        pixels,
        observation,
    } = capture;
    let mut mismatch_count = 0_u64;
    let mut first_mismatch = None;
    for (index, (&wanted, &seen)) in expected.pixels().iter().zip(&pixels).enumerate() {
        if wanted != seen {
            mismatch_count += 1;
            first_mismatch.get_or_insert(index as u64);
        }
    }
    facts.outcome = if mismatch_count == 0 {
        DisplayOutcome::Match
    } else {
        DisplayOutcome::Mismatch
    };
    facts.stage = CaptureStage::Complete;
    facts.captured = Some(observation);
    facts.mismatch_count = mismatch_count;
    facts.first_mismatch = first_mismatch;
    DisplayCapture {
        facts,
        pixels: Some(pixels),
    }
}

#[cfg(any(windows, test))]
fn canonicalize_captured(expected: FrameRef<'_>, mut pixels: Vec<u32>) -> CanonicalCapture {
    for word in pixels.iter_mut() {
        *word &= RGB_MASK;
    }
    let observation = FrameRef::new(expected.width(), expected.height(), &pixels)
        .expect("capture allocation was dimension checked")
        .observe();
    CanonicalCapture {
        pixels,
        observation,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DisplayReceipt {
    tag: ContentTag,
    parent: Option<ContentTag>,
    expected_receipt: ContentTag,
    expected_frame: ContentTag,
    expected_width: u32,
    expected_height: u32,
    policy: ContentTag,
    facts: DisplayCaptureFacts,
}

impl DisplayReceipt {
    pub const fn tag(self) -> ContentTag {
        self.tag
    }

    pub const fn parent(self) -> Option<ContentTag> {
        self.parent
    }

    pub const fn expected_receipt(self) -> ContentTag {
        self.expected_receipt
    }

    pub const fn expected_frame(self) -> ContentTag {
        self.expected_frame
    }

    pub const fn expected_viewport(self) -> (u32, u32) {
        (self.expected_width, self.expected_height)
    }

    pub const fn policy(self) -> ContentTag {
        self.policy
    }

    pub const fn facts(self) -> DisplayCaptureFacts {
        self.facts
    }

    fn recompute_tag(self) -> ContentTag {
        display_receipt_tag(
            self.parent,
            self.expected_receipt,
            self.expected_frame,
            self.expected_width,
            self.expected_height,
            self.policy,
            self.facts,
        )
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DisplayLedger {
    receipts: Vec<DisplayReceipt>,
}

impl DisplayLedger {
    pub fn receipts(&self) -> &[DisplayReceipt] {
        &self.receipts
    }

    pub fn len(&self) -> usize {
        self.receipts.len()
    }

    pub fn is_empty(&self) -> bool {
        self.receipts.is_empty()
    }

    pub fn head(&self) -> Option<ContentTag> {
        self.receipts.last().map(|receipt| receipt.tag)
    }

    pub fn append(
        &mut self,
        expected_receipt: ContentTag,
        expected_frame: ContentTag,
        expected_width: u32,
        expected_height: u32,
        facts: DisplayCaptureFacts,
    ) -> Result<&DisplayReceipt, DisplayProtocolError> {
        let receipt = DisplayReceipt {
            tag: display_receipt_tag(
                self.head(),
                expected_receipt,
                expected_frame,
                expected_width,
                expected_height,
                display_capture_policy_tag(),
                facts,
            ),
            parent: self.head(),
            expected_receipt,
            expected_frame,
            expected_width,
            expected_height,
            policy: display_capture_policy_tag(),
            facts,
        };
        self.append_existing(receipt)
    }

    pub fn verify(&self) -> Result<(), DisplayProtocolError> {
        let mut rebuilt = Self::default();
        for &receipt in &self.receipts {
            rebuilt.append_existing(receipt)?;
        }
        Ok(())
    }

    fn append_existing(
        &mut self,
        receipt: DisplayReceipt,
    ) -> Result<&DisplayReceipt, DisplayProtocolError> {
        require_kind("display parent", receipt.parent, TagKind::DisplayReceipt)?;
        require_kind(
            "expected frame receipt",
            Some(receipt.expected_receipt),
            TagKind::FrameReceipt,
        )?;
        require_kind(
            "expected frame",
            Some(receipt.expected_frame),
            TagKind::Frame,
        )?;
        require_kind("display policy", Some(receipt.policy), TagKind::Policy)?;
        if receipt.parent != self.head() {
            return Err(DisplayProtocolError::ParentMismatch {
                expected: self.head(),
                actual: receipt.parent,
            });
        }
        validate_facts(
            receipt.expected_frame,
            receipt.expected_width,
            receipt.expected_height,
            receipt.facts,
        )?;
        if receipt.tag != receipt.recompute_tag() {
            return Err(DisplayProtocolError::ReceiptTagMismatch);
        }
        self.receipts.push(receipt);
        Ok(self
            .receipts
            .last()
            .expect("a just-appended display receipt must exist"))
    }
}

/// Version-distinct display receipt whose expected frame provenance is a typed composite
/// [`FrameReceiptV2`], rather than an unqualified frame-receipt tag.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DisplayReceiptV2 {
    tag: ContentTag,
    parent: Option<ContentTag>,
    expected_receipt: FrameReceiptV2,
    expected_width: u32,
    expected_height: u32,
    policy: ContentTag,
    facts: DisplayCaptureFacts,
}

impl DisplayReceiptV2 {
    fn derived(
        parent: Option<ContentTag>,
        expected_receipt: FrameReceiptV2,
        expected_width: u32,
        expected_height: u32,
        facts: DisplayCaptureFacts,
    ) -> Self {
        Self {
            tag: display_receipt_v2_tag(
                parent,
                expected_receipt,
                expected_width,
                expected_height,
                display_capture_policy_tag(),
                facts,
            ),
            parent,
            expected_receipt,
            expected_width,
            expected_height,
            policy: display_capture_policy_tag(),
            facts,
        }
    }

    #[cfg(any(test, feature = "protocol-research"))]
    /// Stage a display capture for a store to admit before it is committed.
    pub fn prepare(
        parent: Option<ContentTag>,
        expected_receipt: FrameReceiptV2,
        expected_width: u32,
        expected_height: u32,
        facts: DisplayCaptureFacts,
    ) -> Result<Self, DisplayProtocolError> {
        let receipt = Self::derived(
            parent,
            expected_receipt,
            expected_width,
            expected_height,
            facts,
        );
        receipt.verify()?;
        Ok(receipt)
    }

    pub const fn tag(self) -> ContentTag {
        self.tag
    }

    pub const fn parent(self) -> Option<ContentTag> {
        self.parent
    }

    pub const fn expected_receipt(self) -> FrameReceiptV2 {
        self.expected_receipt
    }

    pub const fn expected_frame(self) -> ContentTag {
        self.expected_receipt.frame()
    }

    pub const fn expected_viewport(self) -> (u32, u32) {
        (self.expected_width, self.expected_height)
    }

    pub const fn policy(self) -> ContentTag {
        self.policy
    }

    pub const fn facts(self) -> DisplayCaptureFacts {
        self.facts
    }

    /// Returns the exact payload committed by the v2 display-receipt tag.
    #[cfg(any(test, feature = "protocol-research"))]
    pub fn encode_canonical(self) -> [u8; DISPLAY_RECEIPT_V2_PAYLOAD_LEN] {
        let mut encoded = Vec::with_capacity(DISPLAY_RECEIPT_V2_PAYLOAD_LEN);
        encode_optional_tag(self.parent, &mut |bytes| encoded.extend_from_slice(bytes));
        encoded.extend_from_slice(&self.expected_receipt.encode_canonical());
        encoded.extend_from_slice(&self.expected_width.to_be_bytes());
        encoded.extend_from_slice(&self.expected_height.to_be_bytes());
        encoded.extend_from_slice(&self.policy.digest());
        encode_display_facts(self.facts, &mut |bytes| encoded.extend_from_slice(bytes));
        encoded
            .try_into()
            .expect("display-receipt-v2 payload has a fixed canonical length")
    }

    /// Strictly decodes one complete canonical v2 receipt payload.
    #[cfg(any(test, feature = "protocol-research"))]
    pub fn decode_canonical(bytes: &[u8]) -> Result<Self, DisplayProtocolError> {
        if bytes.len() != DISPLAY_RECEIPT_V2_PAYLOAD_LEN {
            return Err(DisplayProtocolError::WrongPayloadLength {
                expected: DISPLAY_RECEIPT_V2_PAYLOAD_LEN,
                actual: bytes.len(),
            });
        }
        let parent =
            decode_optional_digest("display parent", &bytes[0..33], TagKind::DisplayReceipt)?;
        let expected_receipt = FrameReceiptV2::decode_canonical(&bytes[33..163])
            .map_err(DisplayProtocolError::InvalidFrameReceiptV2)?;
        let expected_width = u32::from_be_bytes(bytes[163..167].try_into().unwrap());
        let expected_height = u32::from_be_bytes(bytes[167..171].try_into().unwrap());
        let policy = ContentTag::from_digest(
            TagKind::Policy,
            bytes[171..203]
                .try_into()
                .expect("fixed policy digest lane"),
        );
        let facts = decode_display_facts(&bytes[203..])?;
        let receipt = Self {
            tag: display_receipt_v2_tag(
                parent,
                expected_receipt,
                expected_width,
                expected_height,
                policy,
                facts,
            ),
            parent,
            expected_receipt,
            expected_width,
            expected_height,
            policy,
            facts,
        };
        receipt.verify()?;
        if receipt.encode_canonical().as_slice() != bytes {
            return Err(DisplayProtocolError::NonCanonicalEncoding);
        }
        Ok(receipt)
    }

    pub fn verify(self) -> Result<(), DisplayProtocolError> {
        require_kind("display-v2 parent", self.parent, TagKind::DisplayReceipt)?;
        self.expected_receipt
            .verify()
            .map_err(DisplayProtocolError::InvalidFrameReceiptV2)?;
        if self.expected_receipt.stream() != FrameStream::Composite {
            return Err(DisplayProtocolError::WrongFrameStream {
                expected: FrameStream::Composite,
                actual: self.expected_receipt.stream(),
            });
        }
        require_kind("display-v2 policy", Some(self.policy), TagKind::Policy)?;
        validate_facts(
            self.expected_receipt.frame(),
            self.expected_width,
            self.expected_height,
            self.facts,
        )?;
        if self.tag != self.recompute_tag() {
            return Err(DisplayProtocolError::ReceiptTagMismatch);
        }
        Ok(())
    }

    fn recompute_tag(self) -> ContentTag {
        display_receipt_v2_tag(
            self.parent,
            self.expected_receipt,
            self.expected_width,
            self.expected_height,
            self.policy,
            self.facts,
        )
    }
}

/// A separate display-receipt epoch that refuses v1/bare-tag provenance at its API boundary.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DisplayLedgerV2 {
    receipts: Vec<DisplayReceiptV2>,
}

impl DisplayLedgerV2 {
    pub fn receipts(&self) -> &[DisplayReceiptV2] {
        &self.receipts
    }

    pub fn len(&self) -> usize {
        self.receipts.len()
    }

    pub fn is_empty(&self) -> bool {
        self.receipts.is_empty()
    }

    pub fn head(&self) -> Option<ContentTag> {
        self.receipts.last().map(|receipt| receipt.tag)
    }

    pub fn append(
        &mut self,
        expected_receipt: FrameReceiptV2,
        expected_width: u32,
        expected_height: u32,
        facts: DisplayCaptureFacts,
    ) -> Result<&DisplayReceiptV2, DisplayProtocolError> {
        let receipt = DisplayReceiptV2::derived(
            self.head(),
            expected_receipt,
            expected_width,
            expected_height,
            facts,
        );
        self.append_existing(receipt)
    }

    pub fn verify(&self) -> Result<(), DisplayProtocolError> {
        let mut rebuilt = Self::default();
        for &receipt in &self.receipts {
            rebuilt.append_existing(receipt)?;
        }
        Ok(())
    }

    fn append_existing(
        &mut self,
        receipt: DisplayReceiptV2,
    ) -> Result<&DisplayReceiptV2, DisplayProtocolError> {
        receipt.verify()?;
        if receipt.parent != self.head() {
            return Err(DisplayProtocolError::ParentMismatch {
                expected: self.head(),
                actual: receipt.parent,
            });
        }
        self.receipts.push(receipt);
        Ok(self
            .receipts
            .last()
            .expect("a just-appended display-v2 receipt must exist"))
    }
}

fn validate_facts(
    expected_frame: ContentTag,
    width: u32,
    height: u32,
    facts: DisplayCaptureFacts,
) -> Result<(), DisplayProtocolError> {
    if let Some(captured) = facts.captured
        && (captured.tag.kind() != TagKind::Frame
            || captured.facts.pixel_count != u64::from(width) * u64::from(height)
            || captured.facts.invalid_high_byte != 0)
    {
        return Err(DisplayProtocolError::InconsistentFacts);
    }

    let complete_geometry = facts.rect_before == facts.rect_after
        && facts.rect_before.width() == Some(width)
        && facts.rect_before.height() == Some(height);
    let valid = match facts.outcome {
        DisplayOutcome::Match => {
            facts.stage == CaptureStage::Complete
                && facts.error_code == 0
                && width > 0
                && height > 0
                && facts.dpi > 0
                && facts
                    .captured
                    .is_some_and(|captured| captured.tag == expected_frame)
                && facts.mismatch_count == 0
                && facts.first_mismatch.is_none()
                && complete_geometry
                && facts.flush_before
                && facts.compositor_flush
                && facts.flush_after
                && facts.qpc_frequency > 0
                && facts.qpc_end >= facts.qpc_start
        }
        DisplayOutcome::Mismatch => {
            facts.stage == CaptureStage::Complete
                && facts.error_code == 0
                && width > 0
                && height > 0
                && facts.dpi > 0
                && facts.captured.is_some()
                && facts.mismatch_count > 0
                && facts
                    .first_mismatch
                    .is_some_and(|index| index < u64::from(width) * u64::from(height))
                && complete_geometry
                && facts.flush_before
                && facts.compositor_flush
                && facts.flush_after
                && facts.qpc_frequency > 0
                && facts.qpc_end >= facts.qpc_start
        }
        DisplayOutcome::Unavailable => validate_unavailable_facts(width, height, facts),
    };
    valid
        .then_some(())
        .ok_or(DisplayProtocolError::InconsistentFacts)
}

fn validate_unavailable_facts(width: u32, height: u32, facts: DisplayCaptureFacts) -> bool {
    if facts.stage == CaptureStage::Complete
        || facts.mismatch_count != 0
        || facts.first_mismatch.is_some()
    {
        return false;
    }

    let no_flush = !facts.flush_before && !facts.compositor_flush && !facts.flush_after;
    let before_readback = facts.flush_before && facts.compositor_flush && !facts.flush_after;
    let after_readback = facts.flush_before && facts.compositor_flush && facts.flush_after;
    let untouched_geometry = facts.rect_before == CaptureRect::default()
        && facts.rect_after == CaptureRect::default()
        && facts.dpi == 0;
    let before_shape_clock = facts.qpc_start == 0 && facts.qpc_end == 0 && facts.qpc_frequency == 0;
    let expected_geometry = width > 0
        && height > 0
        && facts.rect_before.width() == Some(width)
        && facts.rect_before.height() == Some(height)
        && facts.dpi > 0;

    match (facts.stage, facts.captured) {
        (CaptureStage::WindowHandle | CaptureStage::FlushBefore, None) => {
            no_flush && untouched_geometry && before_shape_clock
        }
        (CaptureStage::CompositorFlush, None) => {
            facts.flush_before
                && !facts.compositor_flush
                && !facts.flush_after
                && untouched_geometry
                && before_shape_clock
        }
        (CaptureStage::Minimized | CaptureStage::GeometryBefore, None) => {
            before_readback && untouched_geometry && before_shape_clock
        }
        (CaptureStage::MonitorBoundary | CaptureStage::Dpi, None) => {
            before_readback
                && facts.rect_after == CaptureRect::default()
                && facts.dpi == 0
                && before_shape_clock
        }
        (CaptureStage::Shape, None) => {
            before_readback
                && facts.rect_after == CaptureRect::default()
                && facts.dpi > 0
                && before_shape_clock
        }
        (CaptureStage::PerformanceClock, None) => {
            before_readback && expected_geometry && facts.rect_after == CaptureRect::default()
        }
        (
            CaptureStage::ScreenDc
            | CaptureStage::MemoryDc
            | CaptureStage::DibSection
            | CaptureStage::SelectBitmap
            | CaptureStage::BitBlt
            | CaptureStage::FlushAfter,
            None,
        ) => {
            before_readback
                && expected_geometry
                && facts.rect_after == CaptureRect::default()
                && facts.qpc_frequency > 0
        }
        (CaptureStage::PerformanceClock, Some(_)) => {
            after_readback
                && expected_geometry
                && facts.rect_after == CaptureRect::default()
                && facts.qpc_frequency > 0
        }
        (CaptureStage::GeometryAfter, Some(_)) => {
            after_readback
                && expected_geometry
                && facts.qpc_frequency > 0
                && facts.qpc_end >= facts.qpc_start
        }
        (CaptureStage::MonitorBoundary, Some(_)) => {
            after_readback
                && expected_geometry
                && facts.rect_after == facts.rect_before
                && facts.qpc_frequency > 0
                && facts.qpc_end >= facts.qpc_start
        }
        _ => false,
    }
}

fn display_receipt_tag(
    parent: Option<ContentTag>,
    expected_receipt: ContentTag,
    expected_frame: ContentTag,
    expected_width: u32,
    expected_height: u32,
    policy: ContentTag,
    facts: DisplayCaptureFacts,
) -> ContentTag {
    hash_canonical(
        TagKind::DisplayReceipt,
        DISPLAY_RECEIPT_SCHEMA,
        DISPLAY_RECEIPT_PAYLOAD_LEN,
        |sink| {
            encode_optional_tag(parent, sink);
            sink(&expected_receipt.digest());
            sink(&expected_frame.digest());
            sink(&expected_width.to_be_bytes());
            sink(&expected_height.to_be_bytes());
            sink(&policy.digest());
            encode_display_facts(facts, sink);
        },
    )
    .expect("display receipt encoding has a fixed length")
}

fn display_receipt_v2_tag(
    parent: Option<ContentTag>,
    expected_receipt: FrameReceiptV2,
    expected_width: u32,
    expected_height: u32,
    policy: ContentTag,
    facts: DisplayCaptureFacts,
) -> ContentTag {
    hash_canonical(
        TagKind::DisplayReceipt,
        DISPLAY_RECEIPT_V2_SCHEMA,
        DISPLAY_RECEIPT_V2_PAYLOAD_LEN as u64,
        |sink| {
            encode_optional_tag(parent, sink);
            sink(&expected_receipt.encode_canonical());
            sink(&expected_width.to_be_bytes());
            sink(&expected_height.to_be_bytes());
            sink(&policy.digest());
            encode_display_facts(facts, sink);
        },
    )
    .expect("display receipt v2 encoding has a fixed length")
}

fn encode_display_facts(facts: DisplayCaptureFacts, sink: &mut dyn FnMut(&[u8])) {
    sink(&[facts.outcome as u8]);
    sink(&[facts.stage as u8]);
    sink(&facts.error_code.to_be_bytes());
    sink(&[u8::from(facts.flush_before)]);
    sink(&[u8::from(facts.compositor_flush)]);
    sink(&[u8::from(facts.flush_after)]);
    encode_rect(facts.rect_before, sink);
    encode_rect(facts.rect_after, sink);
    sink(&facts.dpi.to_be_bytes());
    sink(&facts.qpc_start.to_be_bytes());
    sink(&facts.qpc_end.to_be_bytes());
    sink(&facts.qpc_frequency.to_be_bytes());
    match facts.captured {
        Some(captured) => {
            sink(&[1]);
            sink(&captured.tag.digest());
            encode_frame_facts(captured.facts, sink);
        }
        None => {
            sink(&[0]);
            sink(&[0; 32]);
            encode_frame_facts(FrameFacts::default(), sink);
        }
    }
    sink(&facts.mismatch_count.to_be_bytes());
    match facts.first_mismatch {
        Some(index) => {
            sink(&[1]);
            sink(&index.to_be_bytes());
        }
        None => {
            sink(&[0]);
            sink(&0_u64.to_be_bytes());
        }
    }
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

fn encode_rect(rect: CaptureRect, sink: &mut dyn FnMut(&[u8])) {
    sink(&rect.left.to_be_bytes());
    sink(&rect.top.to_be_bytes());
    sink(&rect.right.to_be_bytes());
    sink(&rect.bottom.to_be_bytes());
}

fn encode_frame_facts(facts: FrameFacts, sink: &mut dyn FnMut(&[u8])) {
    for value in [
        facts.pixel_count,
        facts.non_background,
        facts.invalid_high_byte,
        facts.rgb_one_bits,
        facts.red_sum,
        facts.green_sum,
        facts.blue_sum,
    ] {
        sink(&value.to_be_bytes());
    }
}

#[cfg(any(test, feature = "protocol-research"))]
fn decode_optional_digest(
    field: &'static str,
    bytes: &[u8],
    kind: TagKind,
) -> Result<Option<ContentTag>, DisplayProtocolError> {
    debug_assert_eq!(bytes.len(), 33);
    match bytes[0] {
        0 => {
            if bytes[1..] != [0; 32] {
                return Err(DisplayProtocolError::NonZeroAbsentPadding { field });
            }
            Ok(None)
        }
        1 => Ok(Some(ContentTag::from_digest(
            kind,
            bytes[1..].try_into().expect("fixed optional digest lane"),
        ))),
        marker => Err(DisplayProtocolError::UnknownMarker { field, marker }),
    }
}

#[cfg(any(test, feature = "protocol-research"))]
fn decode_display_facts(bytes: &[u8]) -> Result<DisplayCaptureFacts, DisplayProtocolError> {
    debug_assert_eq!(bytes.len(), 175);
    let outcome = match bytes[0] {
        0 => DisplayOutcome::Match,
        1 => DisplayOutcome::Mismatch,
        2 => DisplayOutcome::Unavailable,
        code => return Err(DisplayProtocolError::UnknownOutcomeCode { code }),
    };
    let stage = match bytes[1] {
        0 => CaptureStage::Complete,
        1 => CaptureStage::WindowHandle,
        2 => CaptureStage::FlushBefore,
        3 => CaptureStage::Minimized,
        4 => CaptureStage::GeometryBefore,
        5 => CaptureStage::MonitorBoundary,
        6 => CaptureStage::Dpi,
        7 => CaptureStage::Shape,
        8 => CaptureStage::PerformanceClock,
        9 => CaptureStage::ScreenDc,
        10 => CaptureStage::MemoryDc,
        11 => CaptureStage::DibSection,
        12 => CaptureStage::SelectBitmap,
        13 => CaptureStage::BitBlt,
        14 => CaptureStage::FlushAfter,
        15 => CaptureStage::GeometryAfter,
        16 => CaptureStage::CompositorFlush,
        code => return Err(DisplayProtocolError::UnknownCaptureStageCode { code }),
    };
    let captured = match bytes[69] {
        0 => {
            if bytes[70..158] != [0; 88] {
                return Err(DisplayProtocolError::NonZeroAbsentPadding {
                    field: "captured frame",
                });
            }
            None
        }
        1 => Some(FrameObservation {
            tag: ContentTag::from_digest(
                TagKind::Frame,
                bytes[70..102]
                    .try_into()
                    .expect("fixed captured-frame digest lane"),
            ),
            facts: decode_frame_facts(&bytes[102..158]),
        }),
        marker => {
            return Err(DisplayProtocolError::UnknownMarker {
                field: "captured frame",
                marker,
            });
        }
    };
    let first_mismatch = match bytes[166] {
        0 => {
            if bytes[167..175] != [0; 8] {
                return Err(DisplayProtocolError::NonZeroAbsentPadding {
                    field: "first mismatch",
                });
            }
            None
        }
        1 => Some(u64::from_be_bytes(bytes[167..175].try_into().unwrap())),
        marker => {
            return Err(DisplayProtocolError::UnknownMarker {
                field: "first mismatch",
                marker,
            });
        }
    };
    Ok(DisplayCaptureFacts {
        outcome,
        stage,
        error_code: u32::from_be_bytes(bytes[2..6].try_into().unwrap()),
        flush_before: decode_bool("flush before", bytes[6])?,
        compositor_flush: decode_bool("compositor flush", bytes[7])?,
        flush_after: decode_bool("flush after", bytes[8])?,
        rect_before: decode_rect(&bytes[9..25]),
        rect_after: decode_rect(&bytes[25..41]),
        dpi: u32::from_be_bytes(bytes[41..45].try_into().unwrap()),
        qpc_start: i64::from_be_bytes(bytes[45..53].try_into().unwrap()),
        qpc_end: i64::from_be_bytes(bytes[53..61].try_into().unwrap()),
        qpc_frequency: i64::from_be_bytes(bytes[61..69].try_into().unwrap()),
        captured,
        mismatch_count: u64::from_be_bytes(bytes[158..166].try_into().unwrap()),
        first_mismatch,
    })
}

#[cfg(any(test, feature = "protocol-research"))]
fn decode_bool(field: &'static str, value: u8) -> Result<bool, DisplayProtocolError> {
    match value {
        0 => Ok(false),
        1 => Ok(true),
        actual => Err(DisplayProtocolError::UnknownBoolean {
            field,
            value: actual,
        }),
    }
}

#[cfg(any(test, feature = "protocol-research"))]
fn decode_rect(bytes: &[u8]) -> CaptureRect {
    debug_assert_eq!(bytes.len(), 16);
    CaptureRect {
        left: i32::from_be_bytes(bytes[0..4].try_into().unwrap()),
        top: i32::from_be_bytes(bytes[4..8].try_into().unwrap()),
        right: i32::from_be_bytes(bytes[8..12].try_into().unwrap()),
        bottom: i32::from_be_bytes(bytes[12..16].try_into().unwrap()),
    }
}

#[cfg(any(test, feature = "protocol-research"))]
fn decode_frame_facts(bytes: &[u8]) -> FrameFacts {
    debug_assert_eq!(bytes.len(), 56);
    let mut values = [0_u64; 7];
    for (value, chunk) in values.iter_mut().zip(bytes.chunks_exact(8)) {
        *value = u64::from_be_bytes(chunk.try_into().unwrap());
    }
    FrameFacts {
        pixel_count: values[0],
        non_background: values[1],
        invalid_high_byte: values[2],
        rgb_one_bits: values[3],
        red_sum: values[4],
        green_sum: values[5],
        blue_sum: values[6],
    }
}

fn require_kind(
    field: &'static str,
    tag: Option<ContentTag>,
    expected: TagKind,
) -> Result<(), DisplayProtocolError> {
    if let Some(tag) = tag
        && tag.kind() != expected
    {
        return Err(DisplayProtocolError::WrongKind {
            field,
            expected,
            actual: tag.kind(),
        });
    }
    Ok(())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DisplayProtocolError {
    WrongKind {
        field: &'static str,
        expected: TagKind,
        actual: TagKind,
    },
    ParentMismatch {
        expected: Option<ContentTag>,
        actual: Option<ContentTag>,
    },
    InvalidFrameReceiptV2(FrameReceiptV2Error),
    WrongFrameStream {
        expected: FrameStream,
        actual: FrameStream,
    },
    WrongPayloadLength {
        expected: usize,
        actual: usize,
    },
    UnknownMarker {
        field: &'static str,
        marker: u8,
    },
    NonZeroAbsentPadding {
        field: &'static str,
    },
    UnknownOutcomeCode {
        code: u8,
    },
    UnknownCaptureStageCode {
        code: u8,
    },
    UnknownBoolean {
        field: &'static str,
        value: u8,
    },
    NonCanonicalEncoding,
    InconsistentFacts,
    ReceiptTagMismatch,
}

impl fmt::Display for DisplayProtocolError {
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
                "display receipt parent mismatch: expected {expected:?}, received {actual:?}"
            ),
            Self::InvalidFrameReceiptV2(ref source) => {
                write!(
                    formatter,
                    "display receipt v2 references an invalid frame receipt: {source}"
                )
            }
            Self::WrongFrameStream { expected, actual } => write!(
                formatter,
                "display receipt v2 requires {expected:?} frame provenance, received {actual:?}"
            ),
            Self::WrongPayloadLength { expected, actual } => write!(
                formatter,
                "display receipt v2 requires {expected} bytes, found {actual}"
            ),
            Self::UnknownMarker { field, marker } => {
                write!(formatter, "display receipt {field} uses marker {marker}")
            }
            Self::NonZeroAbsentPadding { field } => {
                write!(
                    formatter,
                    "absent display receipt {field} has nonzero padding"
                )
            }
            Self::UnknownOutcomeCode { code } => {
                write!(formatter, "display receipt uses outcome code {code}")
            }
            Self::UnknownCaptureStageCode { code } => {
                write!(formatter, "display receipt uses capture-stage code {code}")
            }
            Self::UnknownBoolean { field, value } => {
                write!(
                    formatter,
                    "display receipt {field} uses boolean value {value}"
                )
            }
            Self::NonCanonicalEncoding => {
                formatter.write_str("display receipt payload is not canonical")
            }
            Self::InconsistentFacts => {
                formatter.write_str("display receipt facts are internally inconsistent")
            }
            Self::ReceiptTagMismatch => {
                formatter.write_str("display receipt tag does not match its canonical fields")
            }
        }
    }
}

impl std::error::Error for DisplayProtocolError {}

#[cfg(windows)]
pub mod windows {
    use std::ffi::c_void;
    use std::mem::size_of;
    use std::ptr::null_mut;

    use windows_sys::Win32::Foundation::{GetLastError, HWND, POINT, RECT};
    use windows_sys::Win32::Graphics::Dwm::DwmFlush;
    use windows_sys::Win32::Graphics::Gdi::{
        BI_RGB, BITMAPINFO, CAPTUREBLT, CreateCompatibleDC, CreateDIBSection, DIB_RGB_COLORS,
        DeleteDC, DeleteObject, GDI_ERROR, GdiFlush, GetDC, GetMonitorInfoW, HBITMAP, HDC, HGDIOBJ,
        MONITOR_DEFAULTTONULL, MONITORINFO, MonitorFromRect, ReleaseDC, SRCCOPY, SelectObject,
    };
    use windows_sys::Win32::Graphics::Gdi::{BitBlt, ClientToScreen};
    use windows_sys::Win32::System::Performance::{
        QueryPerformanceCounter, QueryPerformanceFrequency,
    };
    use windows_sys::Win32::UI::HiDpi::GetDpiForWindow;
    use windows_sys::Win32::UI::WindowsAndMessaging::{GetClientRect, IsIconic};

    use super::{
        CaptureRect, CaptureStage, DisplayCapture, DisplayCaptureFacts, DisplayOutcome,
        canonicalize_captured, finalize_capture,
    };
    use crate::frame::FrameRef;

    /// Samples the visible desktop rectangle occupied by one valid Win32 client area.
    ///
    /// # Safety
    ///
    /// `hwnd` must remain a valid window handle owned by the calling thread for the duration of
    /// this call.
    pub unsafe fn capture_client(hwnd: HWND, expected: FrameRef<'_>) -> DisplayCapture {
        capture_client_impl(hwnd, expected)
    }

    fn capture_client_impl(hwnd: HWND, expected: FrameRef<'_>) -> DisplayCapture {
        let mut facts = DisplayCaptureFacts::unavailable(CaptureStage::WindowHandle, 0);
        if hwnd.is_null() {
            return failed(facts, CaptureStage::WindowHandle, 0, None);
        }

        if unsafe { GdiFlush() } == 0 {
            return failed(facts, CaptureStage::FlushBefore, last_error(), None);
        }
        facts.flush_before = true;
        let compositor_result = unsafe { DwmFlush() };
        if compositor_result < 0 {
            return failed(
                facts,
                CaptureStage::CompositorFlush,
                compositor_result as u32,
                None,
            );
        }
        facts.compositor_flush = true;

        if unsafe { IsIconic(hwnd) } != 0 {
            return failed(facts, CaptureStage::Minimized, 0, None);
        }

        let rect_before = match client_screen_rect(hwnd) {
            Ok(rect) => rect,
            Err(error) => return failed(facts, CaptureStage::GeometryBefore, error, None),
        };
        facts.rect_before = rect_before;
        if let Err(error) = require_single_monitor(rect_before) {
            return failed(facts, CaptureStage::MonitorBoundary, error, None);
        }

        let dpi = unsafe { GetDpiForWindow(hwnd) };
        if dpi == 0 {
            return failed(facts, CaptureStage::Dpi, last_error(), None);
        }
        facts.dpi = dpi;

        let Some(width) = rect_before.width() else {
            return failed(facts, CaptureStage::Shape, 0, None);
        };
        let Some(height) = rect_before.height() else {
            return failed(facts, CaptureStage::Shape, 0, None);
        };
        if width != expected.width() || height != expected.height() {
            return failed(facts, CaptureStage::Shape, 0, None);
        }
        let Ok(width_i32) = i32::try_from(width) else {
            return failed(facts, CaptureStage::Shape, 0, None);
        };
        let Ok(height_i32) = i32::try_from(height) else {
            return failed(facts, CaptureStage::Shape, 0, None);
        };
        let Some(pixel_count) = (width as usize).checked_mul(height as usize) else {
            return failed(facts, CaptureStage::Shape, 0, None);
        };
        let Some(byte_count) = pixel_count.checked_mul(size_of::<u32>()) else {
            return failed(facts, CaptureStage::Shape, 0, None);
        };
        let Ok(byte_count_u32) = u32::try_from(byte_count) else {
            return failed(facts, CaptureStage::Shape, 0, None);
        };

        if unsafe { QueryPerformanceFrequency(&mut facts.qpc_frequency) } == 0
            || facts.qpc_frequency <= 0
            || unsafe { QueryPerformanceCounter(&mut facts.qpc_start) } == 0
        {
            return failed(facts, CaptureStage::PerformanceClock, last_error(), None);
        }

        let Some(screen_dc) = ScreenDc::acquire() else {
            return failed(facts, CaptureStage::ScreenDc, last_error(), None);
        };
        let Some(memory_dc) = MemoryDc::create(screen_dc.handle) else {
            return failed(facts, CaptureStage::MemoryDc, last_error(), None);
        };

        let mut bitmap_info = BITMAPINFO::default();
        bitmap_info.bmiHeader.biSize =
            size_of::<windows_sys::Win32::Graphics::Gdi::BITMAPINFOHEADER>() as u32;
        bitmap_info.bmiHeader.biWidth = width_i32;
        bitmap_info.bmiHeader.biHeight = -height_i32;
        bitmap_info.bmiHeader.biPlanes = 1;
        bitmap_info.bmiHeader.biBitCount = 32;
        bitmap_info.bmiHeader.biCompression = BI_RGB;
        bitmap_info.bmiHeader.biSizeImage = byte_count_u32;

        let mut bits: *mut c_void = null_mut();
        let bitmap_handle = unsafe {
            CreateDIBSection(
                memory_dc.handle,
                &bitmap_info,
                DIB_RGB_COLORS,
                &mut bits,
                null_mut(),
                0,
            )
        };
        let Some(bitmap) = Bitmap::new(bitmap_handle) else {
            return failed(facts, CaptureStage::DibSection, last_error(), None);
        };
        if bits.is_null() {
            return failed(facts, CaptureStage::DibSection, last_error(), None);
        }

        let old = unsafe { SelectObject(memory_dc.handle, bitmap.handle.cast()) };
        if old.is_null() || old as isize == GDI_ERROR as isize {
            return failed(facts, CaptureStage::SelectBitmap, last_error(), None);
        }
        let _selection = Selection {
            dc: memory_dc.handle,
            old,
        };

        let copied = unsafe {
            BitBlt(
                memory_dc.handle,
                0,
                0,
                width_i32,
                height_i32,
                screen_dc.handle,
                rect_before.left,
                rect_before.top,
                SRCCOPY | CAPTUREBLT,
            )
        };
        if copied == 0 {
            return failed(facts, CaptureStage::BitBlt, last_error(), None);
        }
        if unsafe { GdiFlush() } == 0 {
            return failed(facts, CaptureStage::FlushAfter, last_error(), None);
        }
        facts.flush_after = true;

        let capture = canonicalize_captured(
            expected,
            unsafe { std::slice::from_raw_parts(bits.cast::<u32>(), pixel_count) }.to_vec(),
        );
        facts.captured = Some(capture.observation);

        if unsafe { QueryPerformanceCounter(&mut facts.qpc_end) } == 0 {
            return failed(
                facts,
                CaptureStage::PerformanceClock,
                last_error(),
                Some(capture.pixels),
            );
        }

        let rect_after = match client_screen_rect(hwnd) {
            Ok(rect) => rect,
            Err(error) => {
                return failed(
                    facts,
                    CaptureStage::GeometryAfter,
                    error,
                    Some(capture.pixels),
                );
            }
        };
        facts.rect_after = rect_after;
        let dpi_after = unsafe { GetDpiForWindow(hwnd) };
        if rect_after != rect_before || dpi_after != dpi {
            return failed(facts, CaptureStage::GeometryAfter, 0, Some(capture.pixels));
        }
        if let Err(error) = require_single_monitor(rect_after) {
            return failed(
                facts,
                CaptureStage::MonitorBoundary,
                error,
                Some(capture.pixels),
            );
        }

        facts.error_code = 0;
        finalize_capture(expected, capture, facts)
    }

    fn failed(
        mut facts: DisplayCaptureFacts,
        stage: CaptureStage,
        error_code: u32,
        pixels: Option<Vec<u32>>,
    ) -> DisplayCapture {
        facts.outcome = DisplayOutcome::Unavailable;
        facts.stage = stage;
        facts.error_code = error_code;
        facts.mismatch_count = 0;
        facts.first_mismatch = None;
        DisplayCapture { facts, pixels }
    }

    fn client_screen_rect(hwnd: HWND) -> Result<CaptureRect, u32> {
        let mut client = RECT::default();
        if unsafe { GetClientRect(hwnd, &mut client) } == 0 {
            return Err(last_error());
        }
        let Some(width) = client
            .right
            .checked_sub(client.left)
            .filter(|&value| value > 0)
        else {
            return Err(0);
        };
        let Some(height) = client
            .bottom
            .checked_sub(client.top)
            .filter(|&value| value > 0)
        else {
            return Err(0);
        };
        let mut origin = POINT {
            x: client.left,
            y: client.top,
        };
        if unsafe { ClientToScreen(hwnd, &mut origin) } == 0 {
            return Err(last_error());
        }
        let Some(right) = origin.x.checked_add(width) else {
            return Err(0);
        };
        let Some(bottom) = origin.y.checked_add(height) else {
            return Err(0);
        };
        Ok(CaptureRect {
            left: origin.x,
            top: origin.y,
            right,
            bottom,
        })
    }

    fn require_single_monitor(rect: CaptureRect) -> Result<(), u32> {
        let native = RECT {
            left: rect.left,
            top: rect.top,
            right: rect.right,
            bottom: rect.bottom,
        };
        let monitor = unsafe { MonitorFromRect(&native, MONITOR_DEFAULTTONULL) };
        if monitor.is_null() {
            return Err(last_error());
        }
        let mut info = MONITORINFO {
            cbSize: size_of::<MONITORINFO>() as u32,
            ..MONITORINFO::default()
        };
        if unsafe { GetMonitorInfoW(monitor, &mut info) } == 0 {
            return Err(last_error());
        }
        let contained = rect.left >= info.rcMonitor.left
            && rect.top >= info.rcMonitor.top
            && rect.right <= info.rcMonitor.right
            && rect.bottom <= info.rcMonitor.bottom;
        contained.then_some(()).ok_or(0)
    }

    fn last_error() -> u32 {
        unsafe { GetLastError() }
    }

    struct ScreenDc {
        handle: HDC,
    }

    impl ScreenDc {
        fn acquire() -> Option<Self> {
            let handle = unsafe { GetDC(null_mut()) };
            (!handle.is_null()).then_some(Self { handle })
        }
    }

    impl Drop for ScreenDc {
        fn drop(&mut self) {
            unsafe {
                ReleaseDC(null_mut(), self.handle);
            }
        }
    }

    struct MemoryDc {
        handle: HDC,
    }

    impl MemoryDc {
        fn create(compatible_with: HDC) -> Option<Self> {
            let handle = unsafe { CreateCompatibleDC(compatible_with) };
            (!handle.is_null()).then_some(Self { handle })
        }
    }

    impl Drop for MemoryDc {
        fn drop(&mut self) {
            unsafe {
                DeleteDC(self.handle);
            }
        }
    }

    struct Bitmap {
        handle: HBITMAP,
    }

    impl Bitmap {
        fn new(handle: HBITMAP) -> Option<Self> {
            (!handle.is_null()).then_some(Self { handle })
        }
    }

    impl Drop for Bitmap {
        fn drop(&mut self) {
            unsafe {
                DeleteObject(self.handle.cast());
            }
        }
    }

    struct Selection {
        dc: HDC,
        old: HGDIOBJ,
    }

    impl Drop for Selection {
        fn drop(&mut self) {
            unsafe {
                SelectObject(self.dc, self.old);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sha2::Digest;

    fn tag(kind: TagKind, label: &[u8]) -> ContentTag {
        hash_canonical(
            kind,
            b"bitblock/display-test/v1",
            label.len() as u64,
            |sink| sink(label),
        )
        .unwrap()
    }

    fn capture(expected: FrameRef<'_>, pixels: Vec<u32>) -> DisplayCapture {
        let rect = CaptureRect {
            left: -20,
            top: 30,
            right: -18,
            bottom: 31,
        };
        let mut facts = DisplayCaptureFacts::unavailable(CaptureStage::Shape, 0);
        facts.flush_before = true;
        facts.compositor_flush = true;
        facts.flush_after = true;
        facts.rect_before = rect;
        facts.rect_after = rect;
        facts.dpi = 144;
        facts.qpc_start = 100;
        facts.qpc_end = 120;
        facts.qpc_frequency = 10_000_000;
        finalize_capture(expected, canonicalize_captured(expected, pixels), facts)
    }

    fn frame_receipt_v2(stream: FrameStream, frame: ContentTag) -> FrameReceiptV2 {
        FrameReceiptV2::new(
            None,
            stream,
            tag(TagKind::TickOutcome, b"outcome"),
            tag(TagKind::FrameInput, b"input"),
            frame,
        )
        .unwrap()
    }

    #[test]
    fn complete_slice_comparison_normalizes_only_the_unused_high_byte() {
        let expected_words = [0x0011_2233, 0x0044_5566];
        let expected = FrameRef::new(2, 1, &expected_words).unwrap();
        let matched = capture(expected, vec![0xff11_2233, 0x0044_5566]);
        assert_eq!(matched.facts.outcome, DisplayOutcome::Match);
        assert_eq!(matched.facts.mismatch_count, 0);
        assert_eq!(matched.pixels.as_deref(), Some(expected_words.as_slice()));

        let mismatched = capture(expected, vec![0x0011_2233, 0x0044_5567]);
        assert_eq!(mismatched.facts.outcome, DisplayOutcome::Mismatch);
        assert_eq!(mismatched.facts.mismatch_count, 1);
        assert_eq!(mismatched.facts.first_mismatch, Some(1));
    }

    #[test]
    fn finalization_reuses_the_single_canonical_capture_observation() {
        let expected_words = [0x0011_2233, 0x0044_5566];
        let expected = FrameRef::new(2, 1, &expected_words).unwrap();
        let canonical = canonicalize_captured(expected, vec![0xff11_2233, 0x0044_5566]);
        let expected_observation = canonical.observation;
        let finalized = finalize_capture(
            expected,
            canonical,
            DisplayCaptureFacts::unavailable(CaptureStage::Shape, 0),
        );

        assert_eq!(finalized.pixels.as_deref(), Some(expected_words.as_slice()));
        assert_eq!(finalized.facts.captured, Some(expected_observation));
        assert_eq!(finalized.facts.outcome, DisplayOutcome::Match);
    }

    #[test]
    fn display_policy_and_receipt_match_independent_python_sha256_vectors() {
        assert_eq!(
            display_capture_policy_tag().to_string(),
            "#bb.policy.sha256.vwu2yb5l6tcqw3foxladgwir4umvpzwdec7dim4at6eethztrksa"
        );
        let expected_words = [0x0011_2233, 0x0044_5566];
        let expected = FrameRef::new(2, 1, &expected_words).unwrap();
        assert_eq!(
            expected.content_tag().to_string(),
            "#bb.frame.sha256.wg6ev5ighrfukd3xzyrx3ejpn55kd2ol4p5sio7au3oww2oac54a"
        );
        let mut ledger = DisplayLedger::default();
        let receipt = ledger
            .append(
                tag(TagKind::FrameReceipt, b"submitted"),
                expected.content_tag(),
                2,
                1,
                capture(expected, expected_words.to_vec()).facts,
            )
            .unwrap();
        assert_eq!(
            receipt.tag().to_string(),
            "#bb.display-receipt.sha256.rsbrsj6aui5mk57qc63uadfylkrj3aritdziqhlhvhxwclj4hz7q"
        );
    }

    /// The v1 receipt's whole point is that a refused or disagreeing capture is a first-class
    /// result rather than an absent one. All three outcomes below are the same schema and the
    /// same 312 bytes; only the fact lanes separate them. `tools/display_receipt_v1_vectors.py`
    /// derives each literal without running any Rust.
    #[test]
    fn display_receipt_v1_outcomes_match_independent_python_vectors() {
        let expected_words = [0x0011_2233, 0x0044_5566];
        let expected = FrameRef::new(2, 1, &expected_words).unwrap();
        let submitted = tag(TagKind::FrameReceipt, b"submitted");

        let mut ledger = DisplayLedger::default();
        let matched = *ledger
            .append(
                submitted,
                expected.content_tag(),
                2,
                1,
                capture(expected, expected_words.to_vec()).facts,
            )
            .unwrap();
        assert_eq!(matched.facts().outcome, DisplayOutcome::Match);
        assert_eq!(
            matched.tag().to_string(),
            "#bb.display-receipt.sha256.rsbrsj6aui5mk57qc63uadfylkrj3aritdziqhlhvhxwclj4hz7q"
        );

        let mismatched = *ledger
            .append(
                submitted,
                expected.content_tag(),
                2,
                1,
                capture(expected, vec![0x0011_2233, 0x0044_5567]).facts,
            )
            .unwrap();
        assert_eq!(mismatched.parent(), Some(matched.tag()));
        assert_eq!(mismatched.facts().outcome, DisplayOutcome::Mismatch);
        assert_eq!(mismatched.facts().first_mismatch, Some(1));
        assert_eq!(
            mismatched.tag().to_string(),
            "#bb.display-receipt.sha256.ro5bwfyygtmzp5hqeiripljfpia67fc5mcheebnqg27elugpq7fq"
        );
        assert!(ledger.verify().is_ok());

        let mut refused = DisplayLedger::default();
        let unavailable = *refused
            .append(
                submitted,
                expected.content_tag(),
                2,
                1,
                DisplayCaptureFacts::unavailable(CaptureStage::WindowHandle, 5),
            )
            .unwrap();
        assert_eq!(unavailable.parent(), None);
        assert_eq!(unavailable.facts().captured, None);
        assert_eq!(
            unavailable.tag().to_string(),
            "#bb.display-receipt.sha256.e7db43nscarqcmnbzec6w44vxm4fepcevsb2yws3th6okh6ihnea"
        );
        assert!(refused.verify().is_ok());
    }

    #[test]
    fn display_receipt_v2_matches_independent_python_sha256_vector() {
        let expected_words = [0x0011_2233, 0x0044_5566];
        let expected = FrameRef::new(2, 1, &expected_words).unwrap();
        let expected_receipt = frame_receipt_v2(FrameStream::Composite, expected.content_tag());
        let mut ledger = DisplayLedgerV2::default();
        let receipt = ledger
            .append(
                expected_receipt,
                2,
                1,
                capture(expected, expected_words.to_vec()).facts,
            )
            .unwrap();

        assert_eq!(receipt.parent(), None);
        assert_eq!(receipt.expected_receipt(), expected_receipt);
        assert_eq!(receipt.expected_frame(), expected.content_tag());
        assert_eq!(receipt.expected_viewport(), (2, 1));
        assert_eq!(receipt.policy(), display_capture_policy_tag());
        assert_eq!(
            receipt.tag().to_string(),
            "#bb.display-receipt.sha256.6y74whydlo2cdhs5pl4fuwp66iwpqobgivpkrpeln2ntpni545pq"
        );
        let encoded = receipt.encode_canonical();
        let payload_digest: [u8; 32] = sha2::Sha256::digest(encoded).into();
        assert_eq!(
            payload_digest,
            [
                0x56, 0x75, 0x7b, 0xc0, 0x8e, 0x43, 0x02, 0x2b, 0x5c, 0x22, 0x25, 0x38, 0x19, 0xba,
                0x15, 0x7b, 0x20, 0x79, 0x31, 0xbd, 0x75, 0x91, 0xaa, 0x32, 0xd0, 0xbe, 0x28, 0x30,
                0xbe, 0x59, 0x30, 0xcc,
            ]
        );
        assert_eq!(DisplayReceiptV2::decode_canonical(&encoded), Ok(*receipt));
        assert!(ledger.verify().is_ok());
    }

    #[test]
    fn display_receipt_v2_codec_round_trips_outcomes_and_refuses_noncanonical_bytes() {
        let expected_words = [0x0011_2233, 0x0044_5566];
        let expected = FrameRef::new(2, 1, &expected_words).unwrap();
        let expected_receipt = frame_receipt_v2(FrameStream::Composite, expected.content_tag());
        let mut ledger = DisplayLedgerV2::default();
        let matched = *ledger
            .append(
                expected_receipt,
                2,
                1,
                capture(expected, expected_words.to_vec()).facts,
            )
            .unwrap();
        let mismatched = *ledger
            .append(
                expected_receipt,
                2,
                1,
                capture(expected, vec![0x0011_2232, 0x0044_5566]).facts,
            )
            .unwrap();
        let unavailable = *ledger
            .append(
                expected_receipt,
                2,
                1,
                DisplayCaptureFacts::unavailable(CaptureStage::FlushBefore, 5),
            )
            .unwrap();
        for receipt in [matched, mismatched, unavailable] {
            let encoded = receipt.encode_canonical();
            assert_eq!(DisplayReceiptV2::decode_canonical(&encoded), Ok(receipt));
        }

        let encoded = matched.encode_canonical();
        for end in 0..encoded.len() {
            assert_eq!(
                DisplayReceiptV2::decode_canonical(&encoded[..end]),
                Err(DisplayProtocolError::WrongPayloadLength {
                    expected: DISPLAY_RECEIPT_V2_PAYLOAD_LEN,
                    actual: end,
                })
            );
        }
        let mut overlong = encoded.to_vec();
        overlong.push(0);
        assert_eq!(
            DisplayReceiptV2::decode_canonical(&overlong),
            Err(DisplayProtocolError::WrongPayloadLength {
                expected: DISPLAY_RECEIPT_V2_PAYLOAD_LEN,
                actual: DISPLAY_RECEIPT_V2_PAYLOAD_LEN + 1,
            })
        );

        let mut mutant = encoded;
        mutant[0] = 2;
        assert!(matches!(
            DisplayReceiptV2::decode_canonical(&mutant),
            Err(DisplayProtocolError::UnknownMarker {
                field: "display parent",
                marker: 2,
            })
        ));
        mutant = encoded;
        mutant[1] = 1;
        assert_eq!(
            DisplayReceiptV2::decode_canonical(&mutant),
            Err(DisplayProtocolError::NonZeroAbsentPadding {
                field: "display parent",
            })
        );
        mutant = encoded;
        mutant[66] = FrameStream::Game as u8;
        assert_eq!(
            DisplayReceiptV2::decode_canonical(&mutant),
            Err(DisplayProtocolError::WrongFrameStream {
                expected: FrameStream::Composite,
                actual: FrameStream::Game,
            })
        );
        mutant = encoded;
        mutant[203] = 3;
        assert_eq!(
            DisplayReceiptV2::decode_canonical(&mutant),
            Err(DisplayProtocolError::UnknownOutcomeCode { code: 3 })
        );
        mutant = encoded;
        mutant[204] = 17;
        assert_eq!(
            DisplayReceiptV2::decode_canonical(&mutant),
            Err(DisplayProtocolError::UnknownCaptureStageCode { code: 17 })
        );
        mutant = encoded;
        mutant[209] = 2;
        assert_eq!(
            DisplayReceiptV2::decode_canonical(&mutant),
            Err(DisplayProtocolError::UnknownBoolean {
                field: "flush before",
                value: 2,
            })
        );
        mutant = encoded;
        mutant[272] = 2;
        assert!(matches!(
            DisplayReceiptV2::decode_canonical(&mutant),
            Err(DisplayProtocolError::UnknownMarker {
                field: "captured frame",
                marker: 2,
            })
        ));
        mutant = encoded;
        mutant[369] = 2;
        assert!(matches!(
            DisplayReceiptV2::decode_canonical(&mutant),
            Err(DisplayProtocolError::UnknownMarker {
                field: "first mismatch",
                marker: 2,
            })
        ));

        let unavailable = unavailable.encode_canonical();
        mutant = unavailable;
        mutant[273] = 1;
        assert_eq!(
            DisplayReceiptV2::decode_canonical(&mutant),
            Err(DisplayProtocolError::NonZeroAbsentPadding {
                field: "captured frame",
            })
        );
        mutant = encoded;
        mutant[370] = 1;
        assert_eq!(
            DisplayReceiptV2::decode_canonical(&mutant),
            Err(DisplayProtocolError::NonZeroAbsentPadding {
                field: "first mismatch",
            })
        );
    }

    #[test]
    fn display_ledger_v2_refuses_wrong_stream_and_free_frame_mismatch_atomically() {
        let expected_words = [0x0011_2233, 0x0044_5566];
        let expected = FrameRef::new(2, 1, &expected_words).unwrap();
        let facts = capture(expected, expected_words.to_vec()).facts;
        let mut ledger = DisplayLedgerV2::default();

        let game = frame_receipt_v2(FrameStream::Game, expected.content_tag());
        assert_eq!(
            ledger.append(game, 2, 1, facts),
            Err(DisplayProtocolError::WrongFrameStream {
                expected: FrameStream::Composite,
                actual: FrameStream::Game,
            })
        );
        assert!(ledger.is_empty());

        let other_words = [0x0011_2232, 0x0044_5566];
        let other = FrameRef::new(2, 1, &other_words).unwrap();
        let wrong_frame = frame_receipt_v2(FrameStream::Composite, other.content_tag());
        assert_eq!(
            ledger.append(wrong_frame, 2, 1, facts),
            Err(DisplayProtocolError::InconsistentFacts)
        );
        assert!(ledger.is_empty());
    }

    #[test]
    fn display_receipts_reject_production_impossible_capture_transcripts() {
        let expected_words = [0x0011_2233, 0x0044_5566];
        let expected = FrameRef::new(2, 1, &expected_words).unwrap();
        let expected_v1 = tag(TagKind::FrameReceipt, b"submitted");
        let expected_v2 = frame_receipt_v2(FrameStream::Composite, expected.content_tag());
        let valid_complete = capture(expected, expected_words.to_vec()).facts;

        for impossible in [
            DisplayCaptureFacts {
                error_code: 1,
                ..valid_complete
            },
            DisplayCaptureFacts {
                dpi: 0,
                ..valid_complete
            },
        ] {
            assert_eq!(
                DisplayLedger::default().append(
                    expected_v1,
                    expected.content_tag(),
                    2,
                    1,
                    impossible,
                ),
                Err(DisplayProtocolError::InconsistentFacts)
            );
            assert_eq!(
                DisplayLedgerV2::default().append(expected_v2, 2, 1, impossible),
                Err(DisplayProtocolError::InconsistentFacts)
            );
        }

        let empty_words = [];
        let empty = FrameRef::new(0, 0, &empty_words).unwrap();
        let empty_receipt = frame_receipt_v2(FrameStream::Composite, empty.content_tag());
        let empty_complete = DisplayCaptureFacts {
            outcome: DisplayOutcome::Match,
            stage: CaptureStage::Complete,
            error_code: 0,
            flush_before: true,
            compositor_flush: true,
            flush_after: true,
            rect_before: CaptureRect::default(),
            rect_after: CaptureRect::default(),
            dpi: 144,
            qpc_start: 100,
            qpc_end: 120,
            qpc_frequency: 10_000_000,
            captured: Some(empty.observe()),
            mismatch_count: 0,
            first_mismatch: None,
        };
        assert_eq!(
            DisplayLedgerV2::default().append(empty_receipt, 0, 0, empty_complete),
            Err(DisplayProtocolError::InconsistentFacts)
        );

        let mut impossible_pixels = valid_complete;
        impossible_pixels.outcome = DisplayOutcome::Unavailable;
        impossible_pixels.stage = CaptureStage::BitBlt;
        impossible_pixels.error_code = 5;
        impossible_pixels.mismatch_count = 0;
        impossible_pixels.first_mismatch = None;
        assert_eq!(
            DisplayLedgerV2::default().append(expected_v2, 2, 1, impossible_pixels),
            Err(DisplayProtocolError::InconsistentFacts)
        );

        assert_eq!(
            DisplayLedgerV2::default().append(
                expected_v2,
                2,
                1,
                DisplayCaptureFacts::unavailable(CaptureStage::ScreenDc, 5),
            ),
            Err(DisplayProtocolError::InconsistentFacts)
        );

        let mut post_readback_failure = valid_complete;
        post_readback_failure.outcome = DisplayOutcome::Unavailable;
        post_readback_failure.stage = CaptureStage::GeometryAfter;
        post_readback_failure.error_code = 5;
        post_readback_failure.rect_after = CaptureRect::default();
        post_readback_failure.mismatch_count = 0;
        post_readback_failure.first_mismatch = None;
        let mut admitted = DisplayLedgerV2::default();
        assert!(
            admitted
                .append(expected_v2, 2, 1, post_readback_failure)
                .is_ok()
        );
        assert!(admitted.verify().is_ok());
    }

    #[test]
    fn display_ledger_chains_matches_mismatches_and_failures_without_erasure() {
        let expected_words = [0x0011_2233, 0x0044_5566];
        let expected = FrameRef::new(2, 1, &expected_words).unwrap();
        let expected_tag = expected.content_tag();
        let frame_receipt = tag(TagKind::FrameReceipt, b"submitted");
        let mut ledger = DisplayLedger::default();

        let matched = capture(expected, expected_words.to_vec());
        let first = *ledger
            .append(frame_receipt, expected_tag, 2, 1, matched.facts)
            .unwrap();
        assert_eq!(first.parent(), None);

        let mismatched = capture(expected, vec![0x0011_2232, 0x0044_5566]);
        let second = *ledger
            .append(frame_receipt, expected_tag, 2, 1, mismatched.facts)
            .unwrap();
        assert_eq!(second.parent(), Some(first.tag()));

        let failed = DisplayCaptureFacts::unavailable(CaptureStage::FlushBefore, 5);
        let third = *ledger
            .append(frame_receipt, expected_tag, 2, 1, failed)
            .unwrap();
        assert_eq!(third.parent(), Some(second.tag()));
        assert_eq!(ledger.len(), 3);
        assert!(ledger.verify().is_ok());
    }

    #[test]
    fn outcome_consistency_and_tampering_fail_closed() {
        let expected_words = [0x0011_2233, 0x0044_5566];
        let expected = FrameRef::new(2, 1, &expected_words).unwrap();
        let frame_receipt = tag(TagKind::FrameReceipt, b"submitted");
        let mut impossible = capture(expected, expected_words.to_vec()).facts;
        impossible.mismatch_count = 1;
        assert_eq!(
            DisplayLedger::default().append(
                frame_receipt,
                expected.content_tag(),
                2,
                1,
                impossible,
            ),
            Err(DisplayProtocolError::InconsistentFacts)
        );

        let valid = capture(expected, expected_words.to_vec()).facts;
        let mut ledger = DisplayLedger::default();
        let mut receipt = *ledger
            .append(frame_receipt, expected.content_tag(), 2, 1, valid)
            .unwrap();
        receipt.tag = tag(TagKind::DisplayReceipt, b"tampered");
        let mut forged = DisplayLedger::default();
        assert_eq!(
            forged.append_existing(receipt),
            Err(DisplayProtocolError::ReceiptTagMismatch)
        );
    }
}
