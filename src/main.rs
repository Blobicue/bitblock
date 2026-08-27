#![cfg_attr(
    all(not(debug_assertions), not(feature = "headless-console")),
    windows_subsystem = "windows"
)]

use std::collections::{HashMap, HashSet};
use std::error::Error;
use std::io::{self, Read};
use std::num::NonZeroU32;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use bitblock::camera::{Camera, DEFAULT_DOLLY_LOG_STEP, DEFAULT_MOUSE_SENSITIVITY, Vec3};
#[cfg(windows)]
use bitblock::display::{CaptureStage, DisplayCapture, DisplayLedgerV2, DisplayOutcome};
#[cfg(all(windows, feature = "protocol-research"))]
use bitblock::display_store::{
    DisplayArtifactAppendReceipt, DisplayArtifactJournal, DisplayArtifactStoreError,
    PresentationOutputAppendReceipt,
};
use bitblock::frame::{
    FrameInput, FrameLedger, FrameObservation, FrameProtocolError, FrameReceipt, FrameRef,
    copy_exact,
};
use bitblock::frame_v2::{
    FrameLedgerV2, FramePairAdmissionV2Error, FrameReceiptPairV2, FrameReceiptV2,
    FrameReceiptV2AdmissionError, FrameReceiptV2Error, FrameStream, admit_frame_pair_derived,
};
use bitblock::history::{History, world_tag};
use bitblock::interaction::{
    AcceptedAction, InteractionTape, MAX_INTERACTION_TAPE_PAYLOAD_BYTES, MoveControl,
};
use bitblock::program::{Edit as WorldEdit, Operation, Program, TickBatch};
use bitblock::protocol::ContentTag;
use bitblock::render::{
    Renderer, bounds_aabb, coord_containing, crosshair_overlay_input, raycast,
    renderer_profile_tag, sweep_point,
};
use bitblock::sidebar::{
    CompositeFrameInput, DisplayStatus, FrameCompositor, SIDEBAR_WIDTH, SidebarRasterFacts,
    SidebarSnapshot, composite_overlay_input,
};
use bitblock::tick::{
    TickAttempt, TickCameraState, TickFrameCommitment, TickInputFlags, TickInputState, TickOutcome,
    verify_successor,
};
#[cfg(windows)]
use bitblock::windows_present::WindowsPixelSurface;
#[cfg(all(windows, feature = "protocol-research"))]
use bitblock::windows_timing::observe_checked_present;
#[cfg(all(windows, feature = "protocol-research"))]
use bitblock::windows_topology::observe_active_output;
use bitblock::world::{BEDROCK_COORD, CHUNK_SIDE, CHUNK_WORDS, Coord, World};
#[cfg(all(windows, feature = "protocol-research"))]
use bitblock::{display_artifact::DisplayCaptureArtifact, storage::StorageLimits};
#[cfg(all(windows, feature = "protocol-research"))]
use bitblock::{
    output_topology::{OutputTopologyError, OutputTopologyObservation},
    presentation::{CompositionTimingEnvelope, PresentationProtocolError},
    presentation_output::PresentationOutputArtifact,
};
#[cfg(not(windows))]
use softbuffer::{Context, Surface};
#[cfg(windows)]
use windows_sys::Win32::Foundation::HWND;
#[cfg(windows)]
use windows_sys::Win32::Graphics::Dwm::{
    DWMWA_WINDOW_CORNER_PREFERENCE, DWMWCP_DONOTROUND, DwmSetWindowAttribute,
};
use winit::application::ApplicationHandler;
use winit::dpi::{PhysicalPosition, PhysicalSize};
use winit::event::{
    DeviceEvent, DeviceId, ElementState, MouseButton, MouseScrollDelta, WindowEvent,
};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::{KeyCode, PhysicalKey};
#[cfg(windows)]
use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};
use winit::window::{CursorGrabMode, Window, WindowId};

const INITIAL_VIEWPORT: u32 = 801;
const INITIAL_OUTPUT_WIDTH: u32 = INITIAL_VIEWPORT + SIDEBAR_WIDTH;
const TICK_RATE: f64 = 60.0;
const TICK_DURATION: Duration = Duration::from_nanos(1_000_000_000 / TICK_RATE as u64);
const MAX_FRAME_GAP: Duration = Duration::from_millis(250);
const MAX_TICKS_PER_FRAME: usize = 15;
const BLOCK_FILL_DISTANCE: f64 = 0.25;
const OBSERVER_COLLISION_CLEARANCE: f64 = 1.0e-6;
const HEADLESS_SIMULATION_MAX_OUTPUT_PIXELS: u64 = 16_777_216;
const HEADLESS_SIMULATION_MAX_RESIZE_PIXEL_WRITES: u128 = 268_435_456;
const HEADLESS_SIMULATION_MAX_OUTPUT_PIXEL_TICKS: u128 = 268_435_456;
const HEADLESS_SIMULATION_MAX_RASTER_SAMPLE_TESTS: u128 = 1_073_741_824;
const HEADLESS_SIMULATION_MAX_WORLD_COORDINATE_TESTS: u128 = 268_435_456;
const HEADLESS_SIMULATION_MAX_EDIT_COORDINATE_TESTS: u128 = 268_435_456;
const HEADLESS_SIMULATION_MAX_CHUNK_WORD_TESTS: u128 = 1_073_741_824;
const HEADLESS_SIMULATION_MAX_WORLD_ORDERING_UNITS: u128 = 1_073_741_824;
const HEADLESS_SIMULATION_MAX_POSSIBLE_CHUNKS: u128 = 65_536;
const HEADLESS_RENDER_TRIANGLES_PER_BIT_BOUND: u128 = 12;
const HEADLESS_SIMULATION_MAX_SOURCE_BYTES: usize = MAX_INTERACTION_TAPE_PAYLOAD_BYTES;
const DISPLAY_JOURNAL_ENV: &str = "BITBLOCK_DISPLAY_JOURNAL_NEW";
#[cfg(all(windows, feature = "protocol-research"))]
const PRESENTATION_AUDIT_ENV: &str = "BITBLOCK_PRESENTATION_AUDIT";
#[cfg(all(windows, feature = "protocol-research"))]
const RESEARCH_WINDOW_TITLE: &str = "bitblock — F3 metrics · F4 pixels · F5 presentation evidence";
#[cfg(all(windows, feature = "protocol-research"))]
const PRESENTATION_EVIDENCE_WAITING_TITLE: &str =
    "bitblock — F5 armed · waiting for next changed presentation";

fn main() -> Result<(), Box<dyn Error>> {
    match load_startup_mode()? {
        StartupMode::Observe(program) => {
            let mut app = App::with_program(program);
            configure_display_artifact_journal(&mut app)?;
            let event_loop = EventLoop::new()?;
            event_loop.set_control_flow(ControlFlow::Wait);
            event_loop.run_app(&mut app)?;
        }
        StartupMode::Simulate(tape) => {
            let summary = simulate_interaction_tape(&tape)?;
            print_headless_summary(&summary);
        }
    }
    Ok(())
}

enum StartupMode {
    Observe(Program),
    Simulate(InteractionTape),
}

#[cfg(all(windows, feature = "protocol-research"))]
fn configure_display_artifact_journal(app: &mut App) -> Result<(), Box<dyn Error>> {
    let Some(path) = std::env::var_os(DISPLAY_JOURNAL_ENV).map(PathBuf::from) else {
        return Ok(());
    };
    if path.as_os_str().is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("{DISPLAY_JOURNAL_ENV} must name a new journal file"),
        )
        .into());
    }
    if display_audit_enabled() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("{DISPLAY_JOURNAL_ENV} and BITBLOCK_DISPLAY_AUDIT are mutually exclusive"),
        )
        .into());
    }
    let (journal, created) = DisplayArtifactJournal::create_new(&path, StorageLimits::DEFAULT)
        .map_err(|error| {
            io::Error::other(format!(
                "could not create fresh display-artifact journal {}: {error}",
                path.display()
            ))
        })?;
    eprintln!(
        "display-journal-created\t{}\tbytes={}\t{:?}",
        path.display(),
        created.file_bytes,
        created.sync_evidence
    );
    app.display_artifact_archive = Some(LiveDisplayArtifactArchive::new(journal));
    Ok(())
}

#[cfg(not(all(windows, feature = "protocol-research")))]
fn configure_display_artifact_journal(_app: &mut App) -> Result<(), Box<dyn Error>> {
    if std::env::var_os(DISPLAY_JOURNAL_ENV).is_some() {
        return Err(io::Error::new(
            io::ErrorKind::Unsupported,
            format!("{DISPLAY_JOURNAL_ENV} requires Windows and --features protocol-research"),
        )
        .into());
    }
    Ok(())
}

fn load_startup_mode() -> Result<StartupMode, Box<dyn Error>> {
    let mut arguments = std::env::args_os().skip(1);
    let Some(first) = arguments.next() else {
        return Ok(StartupMode::Observe(Program::new()));
    };

    if first == "--simulate-actions" {
        let Some(path) = arguments.next().map(PathBuf::from) else {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "usage: bitblock --simulate-actions <interaction.tape>",
            )
            .into());
        };
        if arguments.next().is_some() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "usage: bitblock --simulate-actions <interaction.tape>",
            )
            .into());
        }
        let source = read_bounded_utf8(&path, HEADLESS_SIMULATION_MAX_SOURCE_BYTES)?;
        let tape = InteractionTape::parse(&source).map_err(|error| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                format!("{}: {error}", path.display()),
            )
        })?;
        return Ok(StartupMode::Simulate(tape));
    }

    let path = PathBuf::from(first);
    if arguments.next().is_some() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "usage: bitblock [program.bb] | bitblock --simulate-actions <interaction.tape>",
        )
        .into());
    }

    let source = std::fs::read_to_string(&path)?;
    Program::parse(&source)
        .map(StartupMode::Observe)
        .map_err(|error| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                format!("{}: {error}", path.display()),
            )
            .into()
        })
}

fn read_bounded_utf8(path: &std::path::Path, maximum: usize) -> Result<String, io::Error> {
    let file = std::fs::File::open(path)?;
    let limit = u64::try_from(maximum)
        .expect("the interaction source limit fits u64")
        .checked_add(1)
        .expect("the interaction source limit has one-byte headroom");
    let mut bytes = Vec::new();
    file.take(limit).read_to_end(&mut bytes)?;
    if bytes.len() > maximum {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("interaction source exceeds the bounded {maximum}-byte input policy"),
        ));
    }
    String::from_utf8(bytes).map_err(|error| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("interaction source is not UTF-8: {error}"),
        )
    })
}

#[cfg(windows)]
struct Display {
    surface: WindowsPixelSurface,
    window: Arc<Window>,
}

#[cfg(not(windows))]
struct Display {
    // Drop the surface before the context and window whose handles it owns.
    surface: Surface<Arc<Window>, Arc<Window>>,
    _context: Context<Arc<Window>>,
    window: Arc<Window>,
}

#[derive(Clone, Copy)]
enum AimEdit {
    Place,
    Delete,
}

#[derive(Clone, Copy)]
enum PendingEdit {
    Aim(AimEdit),
    Exact(WorldEdit),
}

#[derive(Default)]
struct Inputs {
    forward: bool,
    backward: bool,
    left: bool,
    right: bool,
    up: bool,
    down: bool,
    mouse_x: f64,
    mouse_y: f64,
    wheel: f64,
    edits: Vec<PendingEdit>,
    fit_world: bool,
    viewport_refit: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct LegacyPresentedFrameObservation {
    legacy_receipt: FrameReceipt,
    frame: FrameObservation,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct PresentedFrameObservation {
    legacy_receipt: FrameReceipt,
    receipt_v2: FrameReceiptV2,
    frame: FrameObservation,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct PendingRenderedFrame {
    parent: Option<bitblock::protocol::ContentTag>,
    source_tick: u64,
    source_outcome: bitblock::protocol::ContentTag,
    input: bitblock::protocol::ContentTag,
    frame: FrameObservation,
}

#[cfg(all(windows, feature = "protocol-research"))]
struct LiveDisplayArtifactArchive {
    journal: DisplayArtifactJournal,
    pending_artifact: Option<DisplayCaptureArtifact>,
    pending_presentation_output: Option<PresentationOutputArtifact>,
    faulted: bool,
}

#[cfg(all(windows, feature = "protocol-research"))]
struct LiveDisplayArtifactFailure {
    error: Box<DisplayArtifactStoreError>,
    artifact: Option<bitblock::protocol::ContentTag>,
    poisoned: bool,
}

#[cfg(all(windows, feature = "protocol-research"))]
impl LiveDisplayArtifactArchive {
    fn new(journal: DisplayArtifactJournal) -> Self {
        Self {
            journal,
            pending_artifact: None,
            pending_presentation_output: None,
            faulted: false,
        }
    }

    fn can_capture_new(&self) -> bool {
        !self.faulted
            && self.pending_artifact.is_none()
            && self.pending_presentation_output.is_none()
    }

    fn pending_artifact(&self) -> Option<bitblock::protocol::ContentTag> {
        self.pending_artifact
            .as_ref()
            .map(DisplayCaptureArtifact::tag)
    }

    fn pending_presentation_output(&self) -> Option<bitblock::protocol::ContentTag> {
        self.pending_presentation_output
            .as_ref()
            .map(PresentationOutputArtifact::tag)
    }

    fn prepare_and_append(
        &mut self,
        expected_receipt: FrameReceiptV2,
        expected: FrameRef<'_>,
        capture: &DisplayCapture,
    ) -> Result<DisplayArtifactAppendReceipt, LiveDisplayArtifactFailure> {
        debug_assert!(self.can_capture_new());
        let artifact =
            match self
                .journal
                .prepare_capture_artifact(expected_receipt, expected, capture)
            {
                Ok(artifact) => artifact,
                Err(error) => {
                    self.faulted = true;
                    return Err(LiveDisplayArtifactFailure {
                        error: Box::new(error),
                        artifact: None,
                        poisoned: self.journal.is_poisoned(),
                    });
                }
            };
        self.pending_artifact = Some(artifact);
        self.append_pending()
    }

    fn append_pending(
        &mut self,
    ) -> Result<DisplayArtifactAppendReceipt, LiveDisplayArtifactFailure> {
        let artifact = self
            .pending_artifact
            .as_ref()
            .expect("an explicit retry requires one retained display artifact");
        let artifact_tag = artifact.tag();
        match self.journal.append_artifact(artifact) {
            Ok(receipt) => {
                self.pending_artifact = None;
                Ok(receipt)
            }
            Err(error) => {
                let poisoned = self.journal.is_poisoned();
                self.faulted |= poisoned;
                Err(LiveDisplayArtifactFailure {
                    error: Box::new(error),
                    artifact: Some(artifact_tag),
                    poisoned,
                })
            }
        }
    }

    fn prepare_and_append_presentation_output(
        &mut self,
        expected_receipt: FrameReceiptV2,
        expected: FrameRef<'_>,
        capture: &DisplayCapture,
        timing: CompositionTimingEnvelope,
        pre_topology: bitblock::output_topology::OutputTopologyObservation,
        post_topology: bitblock::output_topology::OutputTopologyObservation,
    ) -> Result<PresentationOutputAppendReceipt, LiveDisplayArtifactFailure> {
        debug_assert!(self.can_capture_new());
        let display =
            match self
                .journal
                .prepare_capture_artifact(expected_receipt, expected, capture)
            {
                Ok(artifact) => artifact,
                Err(error) => {
                    self.faulted = true;
                    return Err(LiveDisplayArtifactFailure {
                        error: Box::new(error),
                        artifact: None,
                        poisoned: self.journal.is_poisoned(),
                    });
                }
            };
        let presentation =
            match PresentationOutputArtifact::new(display, timing, pre_topology, post_topology) {
                Ok(artifact) => artifact,
                Err(error) => {
                    self.faulted = true;
                    return Err(LiveDisplayArtifactFailure {
                        error: Box::new(DisplayArtifactStoreError::from(error)),
                        artifact: None,
                        poisoned: self.journal.is_poisoned(),
                    });
                }
            };
        self.pending_presentation_output = Some(presentation);
        self.append_pending_presentation_output()
    }

    fn append_pending_presentation_output(
        &mut self,
    ) -> Result<PresentationOutputAppendReceipt, LiveDisplayArtifactFailure> {
        let presentation = self
            .pending_presentation_output
            .as_ref()
            .expect("an explicit F5 retry requires one retained presentation-output artifact");
        let presentation_tag = presentation.tag();
        match self.journal.append_presentation_output(presentation) {
            Ok(receipt) => {
                self.pending_presentation_output = None;
                Ok(receipt)
            }
            Err(error) => {
                let poisoned = self.journal.is_poisoned();
                self.faulted |= poisoned;
                Err(LiveDisplayArtifactFailure {
                    error: Box::new(error),
                    artifact: Some(presentation_tag),
                    poisoned,
                })
            }
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct LiveFrameLedgersV2 {
    game: FrameLedgerV2,
    composite: FrameLedgerV2,
}

#[cfg(any(test, all(windows, feature = "protocol-research")))]
fn current_committed_composite_frame<'a>(
    ledger: &FrameLedgerV2,
    presented: PresentedFrameObservation,
    width: u32,
    height: u32,
    pixels: &'a [u32],
) -> Option<FrameRef<'a>> {
    let receipt = presented.receipt_v2;
    if ledger.stream() != FrameStream::Composite
        || receipt.stream() != FrameStream::Composite
        || ledger.head() != Some(receipt.tag())
        || receipt.verify().is_err()
    {
        return None;
    }
    let frame = FrameRef::new(width, height, pixels).ok()?;
    let observation = frame.observe();
    if observation != presented.frame || receipt.frame() != observation.tag {
        return None;
    }
    Some(frame)
}

#[cfg(all(windows, feature = "protocol-research"))]
fn service_display_artifact_request(
    requested: &mut bool,
    archive: &mut LiveDisplayArtifactArchive,
    composite_ledger: &FrameLedgerV2,
    window: &Window,
    presented: PresentedFrameObservation,
    expected: FrameRef<'_>,
) -> Option<Result<DisplayArtifactAppendReceipt, LiveDisplayArtifactFailure>> {
    if !*requested || !archive.can_capture_new() {
        return None;
    }
    current_committed_composite_frame(
        composite_ledger,
        presented,
        expected.width(),
        expected.height(),
        expected.pixels(),
    )?;

    // This one-shot now owns exactly one OS observation attempt. Any storage retry reuses the
    // prepared artifact and cannot silently sample a later presentation.
    *requested = false;
    let capture = capture_visible_client(window, expected);
    Some(archive.prepare_and_append(presented.receipt_v2, expected, &capture))
}

#[cfg(all(windows, feature = "protocol-research"))]
fn report_display_artifact_result(
    window: &Window,
    result: Result<DisplayArtifactAppendReceipt, LiveDisplayArtifactFailure>,
) {
    match result {
        Ok(receipt) => {
            window.set_title(&format!(
                "bitblock — display artifact stored {}",
                receipt.artifact_count
            ));
            eprintln!(
                "display-artifact-stored\tartifact={}\tdisplay={}\trecord={}\trange=[{}, {})\tcount={}",
                receipt.artifact,
                receipt.display_receipt,
                receipt.carrier_append.record_tag,
                receipt.carrier_append.start_offset,
                receipt.carrier_append.end_offset,
                receipt.artifact_count
            );
        }
        Err(failure) => {
            let disposition = if failure.poisoned {
                "reopen-required"
            } else if failure.artifact.is_some() {
                "retained-f4-retry"
            } else {
                "capture-admission-fault"
            };
            window.set_title(&format!("bitblock — display artifact {disposition}"));
            eprintln!(
                "display-artifact-refused\tdisposition={disposition}\tartifact={:?}\t{}",
                failure.artifact, failure.error
            );
        }
    }
}

#[cfg(all(windows, feature = "protocol-research"))]
enum LivePresentationOutputFailure {
    MissingTiming,
    Timing(PresentationProtocolError),
    TimingReceiptMismatch {
        presented: bitblock::protocol::ContentTag,
        timing: bitblock::protocol::ContentTag,
    },
    MissingWindowHandle,
    Topology {
        stage: &'static str,
        error: OutputTopologyError,
    },
    Store(LiveDisplayArtifactFailure),
}

#[cfg(all(windows, feature = "protocol-research"))]
fn attempt_presentation_output_observation<ObserveTopology, Capture>(
    archive: &mut LiveDisplayArtifactArchive,
    presented: PresentedFrameObservation,
    expected: FrameRef<'_>,
    timing: CompositionTimingEnvelope,
    mut observe_topology: ObserveTopology,
    capture: Capture,
) -> Result<PresentationOutputAppendReceipt, LivePresentationOutputFailure>
where
    ObserveTopology:
        FnMut(&'static str) -> Result<OutputTopologyObservation, LivePresentationOutputFailure>,
    Capture: FnOnce() -> DisplayCapture,
{
    if timing.receipt() != presented.receipt_v2 {
        return Err(LivePresentationOutputFailure::TimingReceiptMismatch {
            presented: presented.receipt_v2.tag(),
            timing: timing.receipt().tag(),
        });
    }
    let pre_topology = observe_topology("before-capture")?;
    let capture = capture();
    let post_topology = observe_topology("after-capture")?;
    archive
        .prepare_and_append_presentation_output(
            presented.receipt_v2,
            expected,
            &capture,
            timing,
            pre_topology,
            post_topology,
        )
        .map_err(LivePresentationOutputFailure::Store)
}

#[cfg(all(windows, feature = "protocol-research"))]
fn service_presentation_output_request_with<'a, Attempt>(
    requested: &mut bool,
    archive: &mut LiveDisplayArtifactArchive,
    composite_ledger: &FrameLedgerV2,
    presented: PresentedFrameObservation,
    expected: FrameRef<'a>,
    attempt: Attempt,
) -> Option<Result<PresentationOutputAppendReceipt, LivePresentationOutputFailure>>
where
    Attempt: FnOnce(
        &mut LiveDisplayArtifactArchive,
        PresentedFrameObservation,
        FrameRef<'a>,
    ) -> Result<PresentationOutputAppendReceipt, LivePresentationOutputFailure>,
{
    if !*requested || !archive.can_capture_new() {
        return None;
    }
    current_committed_composite_frame(
        composite_ledger,
        presented,
        expected.width(),
        expected.height(),
        expected.pixels(),
    )?;

    // F5 owns one exact post-presentation observation attempt. Any later storage retry retains
    // the complete wrapper and performs no timing, topology, capture, or pixel calls.
    *requested = false;
    Some(attempt(archive, presented, expected))
}

#[cfg(all(windows, feature = "protocol-research"))]
fn service_presentation_output_request(
    requested: &mut bool,
    archive: &mut LiveDisplayArtifactArchive,
    composite_ledger: &FrameLedgerV2,
    window: &Window,
    presented: PresentedFrameObservation,
    expected: FrameRef<'_>,
    timing: Option<Result<CompositionTimingEnvelope, PresentationProtocolError>>,
) -> Option<Result<PresentationOutputAppendReceipt, LivePresentationOutputFailure>> {
    service_presentation_output_request_with(
        requested,
        archive,
        composite_ledger,
        presented,
        expected,
        |archive, presented, expected| {
            let timing = match timing {
                Some(Ok(timing)) => timing,
                Some(Err(error)) => return Err(LivePresentationOutputFailure::Timing(error)),
                None => return Err(LivePresentationOutputFailure::MissingTiming),
            };
            let mut cached_hwnd = None;
            attempt_presentation_output_observation(
                archive,
                presented,
                expected,
                timing,
                |stage| {
                    let hwnd = match cached_hwnd {
                        Some(hwnd) => hwnd,
                        None => {
                            let Some(hwnd) = win32_hwnd(window) else {
                                return Err(LivePresentationOutputFailure::MissingWindowHandle);
                            };
                            cached_hwnd = Some(hwnd);
                            hwnd
                        }
                    };
                    // SAFETY: winit owns this HWND on the event-loop thread for the duration of
                    // each synchronous observation. The collector retains no handle or transient
                    // numeric join key.
                    unsafe { observe_active_output(hwnd) }
                        .map_err(|error| LivePresentationOutputFailure::Topology { stage, error })
                },
                || capture_visible_client(window, expected),
            )
        },
    )
}

#[cfg(all(windows, feature = "protocol-research"))]
fn report_presentation_output_result(
    window: &Window,
    result: Result<PresentationOutputAppendReceipt, LivePresentationOutputFailure>,
) {
    match result {
        Ok(receipt) => {
            window.set_title(&format!(
                "bitblock — presentation output stored {}",
                receipt.artifact_count
            ));
            eprintln!(
                "presentation-output-stored\toutput={}\tdisplay={}\ttiming={}\ttopology_before={}\ttopology_after={}\tstability={:?}\trecord={}\trange=[{}, {})\tcount={}",
                receipt.presentation_output,
                receipt.artifact,
                receipt.timing_artifact,
                receipt.topology_before,
                receipt.topology_after,
                receipt.route_stability,
                receipt.carrier_append.record_tag,
                receipt.carrier_append.start_offset,
                receipt.carrier_append.end_offset,
                receipt.artifact_count,
            );
        }
        Err(LivePresentationOutputFailure::Store(failure)) => {
            let disposition = if failure.poisoned {
                "reopen-required"
            } else if failure.artifact.is_some() {
                "retained-f5-retry"
            } else {
                "bundle-admission-fault"
            };
            window.set_title(&format!("bitblock — presentation output {disposition}"));
            eprintln!(
                "presentation-output-refused\tdisposition={disposition}\toutput={:?}\t{}",
                failure.artifact, failure.error
            );
        }
        Err(LivePresentationOutputFailure::Timing(error)) => {
            window.set_title("bitblock — presentation timing refused");
            eprintln!("presentation-output-refused\tdisposition=timing-admission-fault\t{error}");
        }
        Err(LivePresentationOutputFailure::TimingReceiptMismatch { presented, timing }) => {
            window.set_title("bitblock — presentation receipt mismatch");
            eprintln!(
                "presentation-output-refused\tdisposition=timing-receipt-mismatch\tpresented={}\ttiming={}",
                presented, timing,
            );
        }
        Err(LivePresentationOutputFailure::Topology { stage, error }) => {
            window.set_title("bitblock — output topology refused");
            eprintln!(
                "presentation-output-refused\tdisposition=topology-admission-fault\tstage={stage}\t{error}"
            );
        }
        Err(LivePresentationOutputFailure::MissingTiming) => {
            window.set_title("bitblock — presentation timing missing");
            eprintln!("presentation-output-refused\tdisposition=internal-missing-timing");
        }
        Err(LivePresentationOutputFailure::MissingWindowHandle) => {
            window.set_title("bitblock — output topology unavailable");
            eprintln!("presentation-output-refused\tdisposition=missing-window-handle");
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum PendingFrameProtocolError {
    MissingSourceTick {
        tick: u64,
    },
    SourceTickMismatch {
        expected: u64,
        actual: u64,
    },
    MissingLedgerOutcome {
        tick: u64,
    },
    LedgerOutcomeMismatch {
        tick: u64,
        expected: bitblock::protocol::ContentTag,
        actual: bitblock::protocol::ContentTag,
    },
    EndpointPrecedesLedgerHead {
        head_tick: u64,
        endpoint_tick: u64,
    },
    PathLimitExceeded {
        resource: &'static str,
        limit: usize,
        actual: usize,
    },
    PathAttemptCountOverflow,
    PathAllocationFailed,
    InvalidReceipt(FrameReceiptV2Error),
    InvalidOutcomeBinding(FrameReceiptV2AdmissionError),
}

fn verify_pending_frame(
    outcomes: &[TickOutcome],
    stream: FrameStream,
    pending: PendingRenderedFrame,
) -> Result<(), PendingFrameProtocolError> {
    let index = usize::try_from(pending.source_tick).map_err(|_| {
        PendingFrameProtocolError::MissingSourceTick {
            tick: pending.source_tick,
        }
    })?;
    let outcome = outcomes
        .get(index)
        .ok_or(PendingFrameProtocolError::MissingSourceTick {
            tick: pending.source_tick,
        })?;
    if outcome.tick() != pending.source_tick {
        return Err(PendingFrameProtocolError::SourceTickMismatch {
            expected: pending.source_tick,
            actual: outcome.tick(),
        });
    }
    FrameReceiptV2::new(
        None,
        stream,
        pending.source_outcome,
        pending.input,
        pending.frame.tag,
    )
    .map_err(PendingFrameProtocolError::InvalidReceipt)?
    .verify_selected_outcome(outcome)
    .map_err(PendingFrameProtocolError::InvalidOutcomeBinding)
}

fn retained_successors(
    outcomes: &[TickOutcome],
    ledger: &FrameLedgerV2,
    stream: FrameStream,
    pending: PendingRenderedFrame,
) -> Result<Box<[TickOutcome]>, PendingFrameProtocolError> {
    verify_pending_frame(outcomes, stream, pending)?;
    let head = ledger.outcome_head();
    let head_index = usize::try_from(head.tick())
        .map_err(|_| PendingFrameProtocolError::MissingLedgerOutcome { tick: head.tick() })?;
    let retained_head = outcomes
        .get(head_index)
        .ok_or(PendingFrameProtocolError::MissingLedgerOutcome { tick: head.tick() })?;
    if retained_head.tag() != head.tag() {
        return Err(PendingFrameProtocolError::LedgerOutcomeMismatch {
            tick: head.tick(),
            expected: head.tag(),
            actual: retained_head.tag(),
        });
    }
    if pending.source_tick < head.tick() {
        return Err(PendingFrameProtocolError::EndpointPrecedesLedgerHead {
            head_tick: head.tick(),
            endpoint_tick: pending.source_tick,
        });
    }
    if pending.source_tick == head.tick() {
        return Ok(Box::new([]));
    }

    let endpoint_index = usize::try_from(pending.source_tick).map_err(|_| {
        PendingFrameProtocolError::MissingSourceTick {
            tick: pending.source_tick,
        }
    })?;
    let successors = &outcomes[(head_index + 1)..=endpoint_index];
    let limits = ledger.limits();
    if successors.len() > limits.max_successor_outcomes {
        return Err(PendingFrameProtocolError::PathLimitExceeded {
            resource: "successor outcomes",
            limit: limits.max_successor_outcomes,
            actual: successors.len(),
        });
    }
    let attempts = successors.iter().try_fold(0_usize, |total, outcome| {
        total.checked_add(outcome.attempts().len())
    });
    let Some(attempts) = attempts else {
        return Err(PendingFrameProtocolError::PathAttemptCountOverflow);
    };
    if attempts > limits.max_successor_attempts {
        return Err(PendingFrameProtocolError::PathLimitExceeded {
            resource: "successor attempts",
            limit: limits.max_successor_attempts,
            actual: attempts,
        });
    }

    let mut retained = Vec::new();
    retained
        .try_reserve_exact(successors.len())
        .map_err(|_| PendingFrameProtocolError::PathAllocationFailed)?;
    retained.extend(successors.iter().cloned());
    Ok(retained.into_boxed_slice())
}

#[derive(Debug, PartialEq, Eq)]
enum SubmitFrameError<E> {
    Backend(E),
    Protocol(FrameProtocolError),
}

fn submit_and_record<E>(
    ledger: &mut FrameLedger,
    pending: PendingRenderedFrame,
    submit: impl FnOnce() -> Result<(), E>,
) -> Result<LegacyPresentedFrameObservation, SubmitFrameError<E>> {
    submit().map_err(SubmitFrameError::Backend)?;
    let receipt = FrameReceipt::new(
        pending.parent,
        pending.source_tick,
        pending.input,
        pending.frame.tag,
    )
    .map_err(SubmitFrameError::Protocol)?;
    let receipt = *ledger.append(receipt).map_err(SubmitFrameError::Protocol)?;
    Ok(LegacyPresentedFrameObservation {
        legacy_receipt: receipt,
        frame: pending.frame,
    })
}

fn submit_pair_and_record<E>(
    game_ledger: &mut FrameLedger,
    output_ledger: &mut FrameLedger,
    game: Option<PendingRenderedFrame>,
    output: PendingRenderedFrame,
    submit: impl FnOnce() -> Result<(), E>,
) -> Result<
    (
        Option<LegacyPresentedFrameObservation>,
        LegacyPresentedFrameObservation,
    ),
    SubmitFrameError<E>,
> {
    submit().map_err(SubmitFrameError::Backend)?;
    let game = game
        .map(|pending| submit_and_record(game_ledger, pending, || Ok::<(), E>(())))
        .transpose()?;
    let output = submit_and_record(output_ledger, output, || Ok::<(), E>(()))?;
    Ok((game, output))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct PresentedFramePair {
    game: Option<PresentedFrameObservation>,
    output: PresentedFrameObservation,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum LiveFrameSubmissionError<E> {
    Pending(PendingFrameProtocolError),
    Legacy(FrameProtocolError),
    V2(FramePairAdmissionV2Error<E>),
}

fn submit_live_frame_pair<E>(
    outcomes: &[TickOutcome],
    game_ledger: &mut FrameLedger,
    output_ledger: &mut FrameLedger,
    ledgers_v2: &mut LiveFrameLedgersV2,
    game: Option<PendingRenderedFrame>,
    output: PendingRenderedFrame,
    submit: impl FnOnce(FrameReceiptPairV2) -> Result<(), E>,
) -> Result<PresentedFramePair, LiveFrameSubmissionError<E>> {
    let game_successors = game
        .map(|pending| retained_successors(outcomes, &ledgers_v2.game, FrameStream::Game, pending))
        .transpose()
        .map_err(LiveFrameSubmissionError::Pending)?;
    let composite_successors = retained_successors(
        outcomes,
        &ledgers_v2.composite,
        FrameStream::Composite,
        output,
    )
    .map_err(LiveFrameSubmissionError::Pending)?;

    let mut next_game_ledger = game_ledger.clone();
    let mut next_output_ledger = output_ledger.clone();
    let (legacy_game, legacy_output) = submit_pair_and_record(
        &mut next_game_ledger,
        &mut next_output_ledger,
        game,
        output,
        || Ok::<(), std::convert::Infallible>(()),
    )
    .map_err(|error| match error {
        SubmitFrameError::Backend(never) => match never {},
        SubmitFrameError::Protocol(source) => LiveFrameSubmissionError::Legacy(source),
    })?;

    let LiveFrameLedgersV2 {
        game: game_v2,
        composite: composite_v2,
    } = ledgers_v2;
    let game_admission = game_successors.map(|successors| (game_v2, successors));
    let receipts_v2 = admit_frame_pair_derived(
        game_admission,
        (composite_v2, composite_successors),
        |receipts| {
            match (game, receipts.game()) {
                (Some(pending), Some(receipt)) => {
                    assert_eq!(receipt.outcome(), pending.source_outcome);
                    assert_eq!(receipt.input(), pending.input);
                    assert_eq!(receipt.frame(), pending.frame.tag);
                }
                (None, None) => {}
                _ => panic!("prepared game receipt presence diverged from the pending frame"),
            }
            assert_eq!(receipts.composite().outcome(), output.source_outcome);
            assert_eq!(receipts.composite().input(), output.input);
            assert_eq!(receipts.composite().frame(), output.frame.tag);
            submit(receipts)
        },
    )
    .map_err(LiveFrameSubmissionError::V2)?;

    *game_ledger = next_game_ledger;
    *output_ledger = next_output_ledger;
    let presented_game = match (legacy_game, receipts_v2.game()) {
        (Some(legacy), Some(receipt_v2)) => Some(PresentedFrameObservation {
            legacy_receipt: legacy.legacy_receipt,
            receipt_v2,
            frame: legacy.frame,
        }),
        (None, None) => None,
        _ => unreachable!("validated legacy and v2 game receipt presence must agree"),
    };
    let presented_output = PresentedFrameObservation {
        legacy_receipt: legacy_output.legacy_receipt,
        receipt_v2: receipts_v2.composite(),
        frame: legacy_output.frame,
    };
    Ok(PresentedFramePair {
        game: presented_game,
        output: presented_output,
    })
}

impl Inputs {
    fn clear_motion(&mut self) {
        self.forward = false;
        self.backward = false;
        self.left = false;
        self.right = false;
        self.up = false;
        self.down = false;
        self.mouse_x = 0.0;
        self.mouse_y = 0.0;
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum AcceptedActionApplyError {
    NonFiniteLane {
        field: &'static str,
        bits: u64,
    },
    NonFiniteAccumulation {
        field: &'static str,
        prior_bits: u64,
        delta_bits: u64,
    },
}

impl std::fmt::Display for AcceptedActionApplyError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match *self {
            Self::NonFiniteLane { field, bits } => {
                write!(
                    formatter,
                    "accepted {field} lane is nonfinite: {bits:#018x}"
                )
            }
            Self::NonFiniteAccumulation {
                field,
                prior_bits,
                delta_bits,
            } => write!(
                formatter,
                "accepted {field} accumulation is nonfinite: prior={prior_bits:#018x} delta={delta_bits:#018x}"
            ),
        }
    }
}

impl std::error::Error for AcceptedActionApplyError {}

fn checked_accepted_accumulation(
    field: &'static str,
    prior: f64,
    delta: f64,
) -> Result<f64, AcceptedActionApplyError> {
    if !delta.is_finite() {
        return Err(AcceptedActionApplyError::NonFiniteLane {
            field,
            bits: delta.to_bits(),
        });
    }
    let next = prior + delta;
    if !next.is_finite() {
        return Err(AcceptedActionApplyError::NonFiniteAccumulation {
            field,
            prior_bits: prior.to_bits(),
            delta_bits: delta.to_bits(),
        });
    }
    Ok(next)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum AppMode {
    Live,
    Headless,
}

struct App {
    display: Option<Display>,
    world: World,
    history: History,
    camera: Camera,
    renderer: Renderer,
    compositor: FrameCompositor,
    inputs: Inputs,
    mouse_captured: bool,
    cursor_position: Option<PhysicalPosition<f64>>,
    sidebar_visible: bool,
    display_predecessor_override: Option<DisplayStatus>,
    last_frame: Instant,
    next_tick: Instant,
    accumulator: Duration,
    needs_render: bool,
    needs_sidebar_render: bool,
    needs_present: bool,
    tick: u64,
    last_tick_batch: Option<TickBatch>,
    tick_outcomes: Vec<TickOutcome>,
    frame_ledger: FrameLedger,
    output_frame_ledger: FrameLedger,
    frame_ledgers_v2: Option<LiveFrameLedgersV2>,
    pending_frame: Option<PendingRenderedFrame>,
    pending_output_frame: Option<PendingRenderedFrame>,
    pending_sidebar_facts: Option<SidebarRasterFacts>,
    last_presented_frame: Option<PresentedFrameObservation>,
    last_presented_output: Option<PresentedFrameObservation>,
    last_presented_sidebar_facts: Option<SidebarRasterFacts>,
    #[cfg(windows)]
    display_ledger: Option<DisplayLedgerV2>,
    #[cfg(all(windows, feature = "protocol-research"))]
    display_artifact_archive: Option<LiveDisplayArtifactArchive>,
    #[cfg(all(windows, feature = "protocol-research"))]
    display_artifact_capture_requested: bool,
    #[cfg(all(windows, feature = "protocol-research"))]
    presentation_output_capture_requested: bool,
    #[cfg(all(windows, feature = "protocol-research"))]
    presentation_audit_enabled: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct HeadlessTickSummary {
    ordinal: u64,
    outcome: ContentTag,
    post_world: ContentTag,
    post_history: ContentTag,
    camera_bits: [u64; 10],
    game_input: Option<ContentTag>,
    game_frame: Option<ContentTag>,
    composite_input: Option<ContentTag>,
    composite_frame: Option<ContentTag>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct HeadlessSimulationSummary {
    tape: ContentTag,
    action_count: usize,
    ticks: Vec<HeadlessTickSummary>,
    final_world: ContentTag,
    final_history: ContentTag,
    final_camera_bits: [u64; 10],
    on_bits: usize,
    coordinate_receipts: usize,
    game_admissions: usize,
    composite_admissions: usize,
    game_receipt_head: Option<ContentTag>,
    composite_receipt_head: Option<ContentTag>,
}

impl App {
    #[cfg(test)]
    fn new() -> Self {
        Self::with_program(Program::new())
    }

    fn with_program(program: Program) -> Self {
        Self::with_program_mode(program, AppMode::Live)
    }

    fn for_headless_simulation() -> Self {
        Self::with_program_mode(Program::new(), AppMode::Headless)
    }

    fn with_program_mode(program: Program, mode: AppMode) -> Self {
        let now = Instant::now();
        let mut inputs = Inputs::default();
        inputs
            .edits
            .extend(program.into_iter().map(PendingEdit::Exact));
        let world = World::new();
        let history = History::new(&world);
        Self {
            display: None,
            world,
            history,
            camera: Camera::for_observer_spawn(),
            renderer: Renderer::new(INITIAL_VIEWPORT, INITIAL_VIEWPORT),
            compositor: FrameCompositor::new(
                INITIAL_OUTPUT_WIDTH,
                INITIAL_VIEWPORT,
                INITIAL_VIEWPORT,
            ),
            inputs,
            mouse_captured: false,
            cursor_position: None,
            sidebar_visible: true,
            display_predecessor_override: (mode == AppMode::Headless)
                .then_some(DisplayStatus::AuditDisabled),
            last_frame: now,
            next_tick: now,
            accumulator: Duration::ZERO,
            needs_render: true,
            needs_sidebar_render: false,
            needs_present: false,
            tick: 0,
            last_tick_batch: None,
            tick_outcomes: Vec::new(),
            frame_ledger: FrameLedger::default(),
            output_frame_ledger: FrameLedger::default(),
            frame_ledgers_v2: None,
            pending_frame: None,
            pending_output_frame: None,
            pending_sidebar_facts: None,
            last_presented_frame: None,
            last_presented_output: None,
            last_presented_sidebar_facts: None,
            #[cfg(windows)]
            display_ledger: (mode == AppMode::Live && display_audit_enabled())
                .then(DisplayLedgerV2::default),
            #[cfg(all(windows, feature = "protocol-research"))]
            display_artifact_archive: None,
            #[cfg(all(windows, feature = "protocol-research"))]
            display_artifact_capture_requested: false,
            #[cfg(all(windows, feature = "protocol-research"))]
            presentation_output_capture_requested: false,
            #[cfg(all(windows, feature = "protocol-research"))]
            presentation_audit_enabled: mode == AppMode::Live && presentation_audit_enabled(),
        }
    }

    /// Applies one normalized action that has already passed the platform adapter.
    ///
    /// Live winit input and headless tapes both enter through this method. Raw device routing,
    /// mouse-grab success, synthetic-event filtering, redraw, and all display observations stay
    /// outside this accepted-action boundary.
    fn apply_accepted_action(
        &mut self,
        action: AcceptedAction,
    ) -> Result<(), AcceptedActionApplyError> {
        match action {
            AcceptedAction::MoveEdge(control, pressed) => match control {
                MoveControl::Forward => self.inputs.forward = pressed,
                MoveControl::Backward => self.inputs.backward = pressed,
                MoveControl::Left => self.inputs.left = pressed,
                MoveControl::Right => self.inputs.right = pressed,
                MoveControl::Up => self.inputs.up = pressed,
                MoveControl::Down => self.inputs.down = pressed,
            },
            AcceptedAction::LookDelta(x, y) => {
                let next_x = checked_accepted_accumulation("look_x", self.inputs.mouse_x, x)?;
                let next_y = checked_accepted_accumulation("look_y", self.inputs.mouse_y, y)?;
                self.inputs.mouse_x = next_x;
                self.inputs.mouse_y = next_y;
            }
            AcceptedAction::WheelLines(lines) => {
                if !lines.is_finite() {
                    return Err(AcceptedActionApplyError::NonFiniteLane {
                        field: "wheel",
                        bits: lines.to_bits(),
                    });
                }
                self.inputs.wheel = (self.inputs.wheel + lines).clamp(-64.0, 64.0);
            }
            AcceptedAction::AimDelete => {
                self.inputs.edits.push(PendingEdit::Aim(AimEdit::Delete));
            }
            AcceptedAction::AimPlace => {
                self.inputs.edits.push(PendingEdit::Aim(AimEdit::Place));
            }
            AcceptedAction::ExactEdit(edit) => {
                self.inputs.edits.push(PendingEdit::Exact(edit));
            }
            AcceptedAction::FitWorld => self.inputs.fit_world = true,
            AcceptedAction::ToggleSidebar => {
                self.sidebar_visible = !self.sidebar_visible;
                self.needs_sidebar_render = true;
            }
            AcceptedAction::Resize(width, height) => {
                self.resize(PhysicalSize::new(width, height));
            }
            AcceptedAction::ClearMotion => self.inputs.clear_motion(),
        }
        Ok(())
    }

    fn apply_infallible_accepted_action(&mut self, action: AcceptedAction) {
        self.apply_accepted_action(action)
            .expect("a non-analog accepted action cannot fail finite aggregation");
    }

    fn capture_mouse(&mut self, capture: bool) {
        let Some(display) = self.display.as_ref() else {
            return;
        };

        if capture {
            let grabbed = display
                .window
                .set_cursor_grab(CursorGrabMode::Locked)
                .or_else(|_| display.window.set_cursor_grab(CursorGrabMode::Confined))
                .is_ok();
            display.window.set_cursor_visible(!grabbed);
            self.mouse_captured = grabbed;
        } else {
            let _ = display.window.set_cursor_grab(CursorGrabMode::None);
            display.window.set_cursor_visible(true);
            self.mouse_captured = false;
            self.apply_infallible_accepted_action(AcceptedAction::ClearMotion);
        }
    }

    fn pointer_in_sidebar(&self) -> bool {
        self.cursor_position.is_some_and(|position| {
            position.x >= f64::from(self.compositor.simulation_width())
                && position.x < f64::from(self.compositor.width())
                && position.y >= 0.0
                && position.y < f64::from(self.compositor.height())
        })
    }

    fn queue_wheel(&mut self, lines: f64) {
        if !self.mouse_captured && self.pointer_in_sidebar() {
            return;
        }
        if let Err(error) = self.apply_accepted_action(AcceptedAction::WheelLines(lines)) {
            eprintln!("accepted-action-refused\t{error}");
        }
    }

    fn resize(&mut self, size: PhysicalSize<u32>) {
        let (Some(width), Some(height)) =
            (NonZeroU32::new(size.width), NonZeroU32::new(size.height))
        else {
            return;
        };

        let was_top = self.world.bounds().is_some_and(|bounds| {
            let (min, max) = bounds_aabb(bounds);
            self.camera.bounds_fit_one_pixel(
                min,
                max,
                self.renderer.width(),
                self.renderer.height(),
            )
        });

        if let Some(display) = self.display.as_mut() {
            #[cfg(windows)]
            display
                .surface
                .resize(width.get(), height.get())
                .expect("failed to resize checked Win32 pixel surface");
            #[cfg(not(windows))]
            display
                .surface
                .resize(width, height)
                .expect("failed to resize physical-pixel surface");
        }
        let sidebar_width = SIDEBAR_WIDTH.min(size.width.saturating_sub(1));
        let simulation_width = size.width - sidebar_width;
        self.renderer.resize(simulation_width, size.height);
        self.compositor
            .resize(size.width, size.height, simulation_width);
        self.needs_render = true;

        self.inputs.viewport_refit |= was_top;
    }

    fn fit_world(&mut self) {
        let Some(bounds) = self.world.bounds() else {
            return;
        };
        if self.renderer.width() == 0 || self.renderer.height() == 0 {
            return;
        }
        let (min, max) = bounds_aabb(bounds);
        self.camera
            .fit_bounds(min, max, self.renderer.width(), self.renderer.height());
        debug_assert!(observer_is_clear(&self.world, self.camera.position()));
    }

    fn translate_observer(&mut self, local_delta: Vec3) -> bool {
        let start = self.camera.position();
        let mut candidate = self.camera.clone();
        candidate.translate(local_delta);
        let allowed = collision_limited_delta(&self.world, start, candidate.position() - start);
        if allowed.length_squared() == 0.0 {
            return false;
        }

        self.camera.translate_world(allowed);
        debug_assert!(observer_is_clear(&self.world, self.camera.position()));
        true
    }

    fn dolly_observer(&mut self, wheel: f64, min_distance: f64, max_distance: f64) -> bool {
        let start = self.camera.position();
        let anchor = self.camera.observation_anchor();
        let mut candidate = self.camera.clone();
        candidate.dolly_clamped(wheel, DEFAULT_DOLLY_LOG_STEP, min_distance, max_distance);
        let requested = candidate.position() - start;
        if requested.length_squared() == 0.0 {
            return false;
        }

        let allowed = collision_limited_delta(&self.world, start, requested);
        if allowed.length_squared() == 0.0 {
            return false;
        }
        if allowed == requested {
            self.camera = candidate;
        } else {
            let safe_position = start + allowed;
            self.camera
                .set_observation_distance((anchor - safe_position).length());
        }
        debug_assert!(observer_is_clear(&self.world, self.camera.position()));
        true
    }

    fn sidebar_snapshot(&self, source_tick: u64) -> SidebarSnapshot {
        let top_saturated = self.world.bounds().is_some_and(|bounds| {
            let (min, max) = bounds_aabb(bounds);
            self.camera.bounds_fit_one_pixel(
                min,
                max,
                self.renderer.width(),
                self.renderer.height(),
            )
        });
        let (prior_display_head, prior_display_len, display_status) =
            self.sidebar_display_predecessor();
        SidebarSnapshot {
            frame_source_tick: source_tick,
            tick_index: source_tick
                .checked_add(1)
                .expect("completed tick count overflow"),
            occupied_bits: self.world.len() as u64,
            mutable_bits: self.history.circulating_supply() as u64,
            bounds: self.world.bounds(),
            observer_position: self.camera.position(),
            observer_cell: coord_containing(self.camera.position()),
            yaw_radians: self.camera.yaw_radians(),
            pitch_radians: self.camera.pitch_radians(),
            observation_distance: self.camera.observation_distance(),
            origin_distance: self.camera.position().length(),
            simulation_width: self.renderer.width(),
            simulation_height: self.renderer.height(),
            output_width: self.compositor.width(),
            output_height: self.compositor.height(),
            sidebar_width: self.compositor.sidebar_width(),
            world_pixels: self.renderer.world_pixel_count(),
            top_saturated,
            world: self.history.current_world(),
            history_head: self.history.head(),
            history_len: self.history.receipts().len() as u64,
            prior_frame_head: self.output_frame_ledger.head(),
            prior_frame_len: self.output_frame_ledger.len() as u64,
            prior_display_head,
            prior_display_len,
            display_status,
            visible: self.sidebar_visible,
            previous_raster: self.last_presented_sidebar_facts,
        }
    }

    #[cfg(windows)]
    fn sidebar_display_predecessor(
        &self,
    ) -> (Option<bitblock::protocol::ContentTag>, u64, DisplayStatus) {
        if let Some(status) = self.display_predecessor_override {
            return (None, 0, status);
        }
        let Some(ledger) = self.display_ledger.as_ref() else {
            return (None, 0, DisplayStatus::AuditDisabled);
        };
        let status = ledger
            .receipts()
            .last()
            .map_or(DisplayStatus::NoReceipt, |receipt| {
                match receipt.facts().outcome {
                    DisplayOutcome::Match => DisplayStatus::Match,
                    DisplayOutcome::Mismatch => DisplayStatus::Mismatch,
                    DisplayOutcome::Unavailable => DisplayStatus::Unavailable,
                }
            });
        (ledger.head(), ledger.len() as u64, status)
    }

    #[cfg(not(windows))]
    fn sidebar_display_predecessor(
        &self,
    ) -> (Option<bitblock::protocol::ContentTag>, u64, DisplayStatus) {
        if let Some(status) = self.display_predecessor_override {
            return (None, 0, status);
        }
        (None, 0, DisplayStatus::Unsupported)
    }

    fn fixed_tick(&mut self) -> bool {
        if let Some(previous) = self.tick_outcomes.last() {
            assert_eq!(
                TickCameraState::capture(&self.camera),
                previous.post_camera(),
                "live camera diverged from predecessor tick outcome before the fixed tick"
            );
        }
        self.tick_outcomes
            .try_reserve(1)
            .expect("fixed tick must reserve its outcome-history slot before mutating state");
        let pre_world = self.history.current_world();
        assert_eq!(
            world_tag(&self.world),
            pre_world,
            "live world diverged from coordinate-history identity before the fixed tick"
        );
        let pre_history = self.history.head();
        let tick_input = TickInputState::new(
            TickInputFlags::new(
                self.inputs.forward,
                self.inputs.backward,
                self.inputs.left,
                self.inputs.right,
                self.inputs.up,
                self.inputs.down,
                self.inputs.fit_world,
                self.needs_render,
                self.needs_sidebar_render,
                self.sidebar_visible,
                self.inputs.viewport_refit,
            ),
            self.inputs.mouse_x,
            self.inputs.mouse_y,
            self.inputs.wheel,
            self.renderer.width(),
            self.renderer.height(),
            self.compositor.width(),
            self.compositor.height(),
        )
        .expect("fixed-tick input must be finite with a valid viewport");

        if self.inputs.viewport_refit {
            self.fit_world();
            self.inputs.viewport_refit = false;
            self.needs_render = true;
        }

        if self.inputs.mouse_x != 0.0 || self.inputs.mouse_y != 0.0 {
            self.camera.apply_mouse_delta(
                self.inputs.mouse_x,
                self.inputs.mouse_y,
                DEFAULT_MOUSE_SENSITIVITY,
            );
            self.inputs.mouse_x = 0.0;
            self.inputs.mouse_y = 0.0;
            self.needs_render = true;
        }

        let local = Vec3::new(
            axis(self.inputs.right, self.inputs.left),
            axis(self.inputs.up, self.inputs.down),
            axis(self.inputs.forward, self.inputs.backward),
        );
        if let Some(direction) = local.try_normalized() {
            // Movement remains useful at every depth: the farther the observation anchor,
            // the more world space corresponds to one physical screen pixel.
            let speed = (self.camera.observation_distance() * 0.75).max(1.0);
            self.needs_render |= self.translate_observer(direction * (speed / TICK_RATE));
        }

        if self.inputs.fit_world {
            self.fit_world();
            self.inputs.fit_world = false;
            self.needs_render = true;
        }

        if self.inputs.wheel != 0.0 {
            let wheel = self.inputs.wheel;
            self.inputs.wheel = 0.0;
            let max_distance = self
                .world
                .bounds()
                .map(|bounds| {
                    let (min, max) = bounds_aabb(bounds);
                    self.camera.fit_distance_for_bounds(
                        min,
                        max,
                        self.renderer.width().max(1),
                        self.renderer.height().max(1),
                    )
                })
                .unwrap_or(bitblock::camera::MAX_OBSERVATION_DISTANCE)
                .max(BLOCK_FILL_DISTANCE);
            self.needs_render |= self.dolly_observer(wheel, BLOCK_FILL_DISTANCE, max_distance);
        }

        let pending_edits = std::mem::take(&mut self.inputs.edits);
        let attempts = pending_edits.len();
        let mut outcome_attempts = Vec::with_capacity(attempts);
        let mut transaction = self.history.begin_tick(self.tick);
        for pending in pending_edits {
            match pending {
                PendingEdit::Aim(AimEdit::Delete) => {
                    let Some(edit) = resolve_aim_edit(&self.world, &self.camera, AimEdit::Delete)
                    else {
                        outcome_attempts.push(TickAttempt::AimDeleteUnresolved);
                        continue;
                    };
                    assert!(
                        !edit_would_enclose_observer(&self.world, &self.camera, edit),
                        "an aimed Off edit cannot enclose the observer"
                    );
                    self.needs_render |=
                        transaction.apply_edit(&self.history, &mut self.world, edit);
                    outcome_attempts.push(TickAttempt::AimDeleteExecuted(edit));
                }
                PendingEdit::Aim(AimEdit::Place) => {
                    let Some(edit) = resolve_aim_edit(&self.world, &self.camera, AimEdit::Place)
                    else {
                        outcome_attempts.push(TickAttempt::AimPlaceUnresolved);
                        continue;
                    };
                    if edit_would_enclose_observer(&self.world, &self.camera, edit) {
                        outcome_attempts.push(TickAttempt::AimPlaceRejectedObserver(edit));
                        continue;
                    }
                    self.needs_render |=
                        transaction.apply_edit(&self.history, &mut self.world, edit);
                    outcome_attempts.push(TickAttempt::AimPlaceExecuted(edit));
                }
                PendingEdit::Exact(edit) => {
                    if edit_would_enclose_observer(&self.world, &self.camera, edit) {
                        outcome_attempts.push(TickAttempt::ExactRejectedObserver(edit));
                        continue;
                    }
                    self.needs_render |=
                        transaction.apply_edit(&self.history, &mut self.world, edit);
                    outcome_attempts.push(TickAttempt::ExactExecuted(edit));
                }
            }
        }

        let batch = TickBatch::new(self.tick, attempts, transaction.edits().iter().copied());
        self.history.commit_tick(transaction, &self.world);
        self.last_tick_batch = Some(batch);
        let post_world = world_tag(&self.world);
        assert_eq!(
            post_world,
            self.history.current_world(),
            "live world diverged from coordinate-history identity after fixed-tick edits"
        );

        let game_rendered = std::mem::take(&mut self.needs_render);
        let sidebar_rendered = std::mem::take(&mut self.needs_sidebar_render);
        let rendered = game_rendered || sidebar_rendered;
        let mut produced_game = None;
        let mut produced_game_context = None;
        if game_rendered {
            debug_assert_eq!(
                self.history.current_world(),
                world_tag(&self.world),
                "world and coordinate history diverged before rendering"
            );
            let overlay = crosshair_overlay_input();
            let renderer_profile = renderer_profile_tag();
            let overlay_input = overlay.tag();
            let input = FrameInput::capture(
                post_world,
                &self.camera,
                self.renderer.width(),
                self.renderer.height(),
                renderer_profile,
                overlay_input,
            )
            .expect("renderer state must use canonical nested tag kinds");
            self.renderer.render(&self.world, &self.camera);
            self.renderer.draw_crosshair(overlay);
            let game_frame = FrameRef::new(
                self.renderer.width(),
                self.renderer.height(),
                self.renderer.pixels(),
            )
            .expect("renderer produced an invalid game-frame shape")
            .observe();
            produced_game = Some((input.tag(), game_frame));
            produced_game_context = Some((renderer_profile, overlay_input));
        }

        let mut produced_composite = None;
        let mut produced_sidebar_facts = None;
        if rendered {
            let game = produced_game
                .or_else(|| {
                    self.pending_frame
                        .map(|pending| (pending.input, pending.frame))
                })
                .or_else(|| {
                    self.last_presented_frame
                        .map(|presented| (presented.receipt_v2.input(), presented.frame))
                })
                .expect("a sidebar frame requires one rendered game frame");
            let snapshot = self.sidebar_snapshot(self.tick);
            let snapshot_tag = snapshot.tag();
            let composition_overlay = composite_overlay_input(snapshot_tag)
                .expect("sidebar composition policy and snapshot kinds are canonical");
            let composite_input = CompositeFrameInput::new(
                game.0,
                game.1.tag,
                composition_overlay,
                self.renderer.width(),
                self.renderer.height(),
                self.compositor.width(),
                self.compositor.height(),
            );
            let sidebar_facts = self.compositor.compose(self.renderer.pixels(), &snapshot);
            let output_frame = FrameRef::new(
                self.compositor.width(),
                self.compositor.height(),
                self.compositor.pixels(),
            )
            .expect("compositor produced an invalid full-client shape")
            .observe();
            produced_composite = Some((composite_input.tag(), output_frame));
            produced_sidebar_facts = Some(sidebar_facts);
        }

        let game_commitment = produced_game.map(|(input, frame)| {
            TickFrameCommitment::new(input, frame.tag)
                .expect("a rendered game frame has canonical nested tag kinds")
        });
        let composite_commitment = produced_composite.map(|(input, frame)| {
            TickFrameCommitment::new(input, frame.tag)
                .expect("a rendered composite frame has canonical nested tag kinds")
        });
        let outcome = TickOutcome::new(
            self.tick_outcomes.last().map(TickOutcome::tag),
            self.tick,
            pre_world,
            pre_history,
            tick_input,
            outcome_attempts,
            post_world,
            self.history.head(),
            TickCameraState::capture(&self.camera),
            game_commitment,
            composite_commitment,
        )
        .expect("fixed tick produced a structurally invalid outcome");
        if let Some((renderer_profile, overlay_input)) = produced_game_context {
            outcome
                .verify_game_input_binding(renderer_profile, overlay_input)
                .expect("fixed tick game-frame input diverged from its captured render context");
        }
        verify_successor(self.tick_outcomes.last(), &outcome)
            .expect("fixed tick broke its content-addressed outcome chain");
        if self.tick_outcomes.is_empty() {
            assert!(
                self.frame_ledgers_v2.is_none(),
                "v2 frame ledgers cannot precede the retained tick-zero outcome"
            );
            self.frame_ledgers_v2 = Some(LiveFrameLedgersV2 {
                game: FrameLedgerV2::new_structural_genesis(FrameStream::Game, outcome.clone())
                    .expect("tick zero must be a structural game-ledger genesis"),
                composite: FrameLedgerV2::new_structural_genesis(
                    FrameStream::Composite,
                    outcome.clone(),
                )
                .expect("tick zero must be a structural composite-ledger genesis"),
            });
        }
        let source_tick = outcome.tick();
        let source_outcome = outcome.tag();

        if let Some((input, frame)) = produced_game {
            self.pending_frame = Some(PendingRenderedFrame {
                parent: self.frame_ledger.head(),
                source_tick,
                source_outcome,
                input,
                frame,
            });
        }
        if let Some((input, frame)) = produced_composite {
            self.pending_output_frame = Some(PendingRenderedFrame {
                parent: self.output_frame_ledger.head(),
                source_tick,
                source_outcome,
                input,
                frame,
            });
            self.pending_sidebar_facts = produced_sidebar_facts;
        }
        self.tick_outcomes.push(outcome);
        self.tick = self.tick.checked_add(1).expect("tick counter overflow");
        rendered
    }

    fn advance_ticks(&mut self, now: Instant) -> bool {
        let elapsed = now.duration_since(self.last_frame).min(MAX_FRAME_GAP);
        self.last_frame = now;
        self.accumulator += elapsed;

        let mut steps = 0;
        let mut rendered = false;
        while self.accumulator >= TICK_DURATION && steps < MAX_TICKS_PER_FRAME {
            rendered |= self.fixed_tick();
            self.accumulator -= TICK_DURATION;
            steps += 1;
        }
        if steps == MAX_TICKS_PER_FRAME {
            self.accumulator = Duration::ZERO;
        }

        rendered
    }

    fn present(&mut self) {
        if !self.needs_present {
            #[cfg(all(windows, feature = "protocol-research"))]
            if self.display_artifact_capture_requested
                && let (Some(presented), Some(ledgers), Some(display), Some(archive)) = (
                    self.last_presented_output,
                    self.frame_ledgers_v2.as_ref(),
                    self.display.as_ref(),
                    self.display_artifact_archive.as_mut(),
                )
                && let Ok(frame) = FrameRef::new(
                    self.compositor.width(),
                    self.compositor.height(),
                    self.compositor.pixels(),
                )
                && let Some(result) = service_display_artifact_request(
                    &mut self.display_artifact_capture_requested,
                    archive,
                    &ledgers.composite,
                    &display.window,
                    presented,
                    frame,
                )
            {
                report_display_artifact_result(&display.window, result);
            }
            return;
        }

        let game_pending = self.pending_frame;
        let output_pending = self
            .pending_output_frame
            .expect("a requested presentation must have a pending composite frame");
        let sidebar_facts = self
            .pending_sidebar_facts
            .expect("a pending composite frame must retain its sidebar facts");

        let Some(display) = self.display.as_mut() else {
            return;
        };
        let frame = FrameRef::new(
            self.compositor.width(),
            self.compositor.height(),
            self.compositor.pixels(),
        )
        .expect("compositor produced an invalid full-client shape");
        debug_assert_eq!(frame.observe(), output_pending.frame);
        if let Some(game_pending) = game_pending {
            debug_assert_eq!(
                FrameRef::new(
                    self.renderer.width(),
                    self.renderer.height(),
                    self.renderer.pixels(),
                )
                .expect("renderer produced an invalid game-frame shape")
                .observe(),
                game_pending.frame
            );
        }
        assert_eq!(
            output_pending.frame.facts.invalid_high_byte, 0,
            "compositor emitted pixels outside canonical 0x00RRGGBB"
        );
        #[cfg(windows)]
        {
            let buffer = display
                .surface
                .pixels_mut()
                .expect("failed to acquire checked Win32 pixel buffer");
            copy_exact(buffer, frame).expect("surface does not match framebuffer shape");
        }
        #[cfg(not(windows))]
        let mut buffer = display
            .surface
            .buffer_mut()
            .expect("failed to acquire physical-pixel buffer");
        #[cfg(not(windows))]
        copy_exact(&mut buffer, frame).expect("surface does not match framebuffer shape");

        #[cfg(all(windows, feature = "protocol-research"))]
        let presentation_audit_enabled =
            self.presentation_audit_enabled || self.presentation_output_capture_requested;
        #[cfg(all(windows, feature = "protocol-research"))]
        let mut presentation_timing = None;
        #[cfg(windows)]
        let submitted = submit_live_frame_pair(
            &self.tick_outcomes,
            &mut self.frame_ledger,
            &mut self.output_frame_ledger,
            self.frame_ledgers_v2
                .as_mut()
                .expect("v2 frame ledgers cannot disappear during one presentation"),
            game_pending,
            output_pending,
            |_receipts| {
                #[cfg(feature = "protocol-research")]
                if presentation_audit_enabled {
                    let observation = observe_checked_present(|| display.surface.present())?;
                    if !observation.presenter.release_succeeded() {
                        eprintln!(
                            "windows-present-release\tresult={}\terror={}",
                            observation.presenter.release_result,
                            observation.presenter.release_error_code
                        );
                    }
                    presentation_timing = Some(observation.bind(_receipts.composite()));
                    return Ok(());
                }
                display.surface.present().map(|facts| {
                    if !facts.release_succeeded() {
                        eprintln!(
                            "windows-present-release\tresult={}\terror={}",
                            facts.release_result, facts.release_error_code
                        );
                    }
                })
            },
        )
        .unwrap_or_else(|error| panic!("failed to submit canonical v2 frame pair: {error:?}"));
        #[cfg(not(windows))]
        let submitted = submit_live_frame_pair(
            &self.tick_outcomes,
            &mut self.frame_ledger,
            &mut self.output_frame_ledger,
            self.frame_ledgers_v2
                .as_mut()
                .expect("v2 frame ledgers cannot disappear during one presentation"),
            game_pending,
            output_pending,
            |_| buffer.present(),
        )
        .unwrap_or_else(|error| panic!("failed to submit canonical v2 frame pair: {error:?}"));
        let PresentedFramePair {
            game: presented_game,
            output: presented_output,
        } = submitted;
        debug_assert_eq!(game_pending.is_some(), presented_game.is_some());
        debug_assert_eq!(presented_output.receipt_v2.stream(), FrameStream::Composite);
        #[cfg(all(windows, feature = "protocol-research"))]
        if let Some(timing) = presentation_timing {
            match timing {
                Ok(envelope) => {
                    debug_assert_eq!(envelope.receipt(), presented_output.receipt_v2);
                    eprintln!(
                        "composition-timing\tartifact={}\tpolicy={}\tprogress={:?}\tattribution={:?}\tqpf={}\tpre_hr={}\tgdi_flush={}\tdwm_flush_hr={}\tpost_hr={}",
                        envelope.tag(),
                        envelope.timing_policy(),
                        envelope.progress(),
                        envelope.frame_attribution(),
                        envelope.qpc_frequency().succeeded,
                        envelope.pre().hresult,
                        envelope.gdi_flush().succeeded,
                        envelope.dwm_flush().hresult,
                        envelope.post().hresult,
                    );
                }
                Err(error) => {
                    eprintln!("composition-timing-refused\t{error}");
                }
            }
        }

        // Presentation authority is already committed. Publish that state and clear the pending
        // submission before any observational capture or filesystem work can refuse afterward.
        if let Some(presented_game) = presented_game {
            self.last_presented_frame = Some(presented_game);
        }
        self.last_presented_output = Some(presented_output);
        self.last_presented_sidebar_facts = Some(sidebar_facts);
        self.pending_frame = None;
        self.pending_output_frame = None;
        self.pending_sidebar_facts = None;
        self.needs_present = false;

        #[cfg(all(windows, feature = "protocol-research"))]
        if self.presentation_output_capture_requested
            && let (Some(ledgers), Some(archive)) = (
                self.frame_ledgers_v2.as_ref(),
                self.display_artifact_archive.as_mut(),
            )
            && let Some(result) = service_presentation_output_request(
                &mut self.presentation_output_capture_requested,
                archive,
                &ledgers.composite,
                &display.window,
                presented_output,
                frame,
                presentation_timing,
            )
        {
            report_presentation_output_result(&display.window, result);
        }

        #[cfg(windows)]
        if let Some(display_ledger) = self.display_ledger.as_mut() {
            let capture = capture_visible_client(&display.window, frame);
            let receipt = *display_ledger
                .append(
                    presented_output.receipt_v2,
                    frame.width(),
                    frame.height(),
                    capture.facts,
                )
                .expect("display capture facts must satisfy their canonical protocol");
            display.window.set_title(&format!(
                "bitblock — display audit {:?} / {:?}",
                capture.facts.outcome, capture.facts.stage
            ));
            eprintln!(
                "display-audit\t{}\t{:?}\tmismatches={}\tfirst={:?}",
                receipt.tag(),
                capture.facts.stage,
                capture.facts.mismatch_count,
                capture.facts.first_mismatch
            );
            if display_ledger.len() == 1
                && capture.facts.mismatch_count != 0
                && let Some(captured) = capture.pixels.as_deref()
            {
                for (index, (&expected, &actual)) in frame
                    .pixels()
                    .iter()
                    .zip(captured)
                    .enumerate()
                    .filter(|(_, (expected, actual))| expected != actual)
                    .take(64)
                {
                    eprintln!(
                        "display-mismatch\tx={}\ty={}\texpected={expected:#08x}\tactual={actual:#08x}",
                        index % frame.width() as usize,
                        index / frame.width() as usize,
                    );
                }
            }
        }

        #[cfg(all(windows, feature = "protocol-research"))]
        if self.display_artifact_capture_requested
            && let (Some(ledgers), Some(archive)) = (
                self.frame_ledgers_v2.as_ref(),
                self.display_artifact_archive.as_mut(),
            )
            && let Some(result) = service_display_artifact_request(
                &mut self.display_artifact_capture_requested,
                archive,
                &ledgers.composite,
                &display.window,
                presented_output,
                frame,
            )
        {
            report_display_artifact_result(&display.window, result);
        }
    }

    fn request_display_artifact_capture(&mut self) {
        #[cfg(all(windows, feature = "protocol-research"))]
        {
            let Some(archive) = self.display_artifact_archive.as_mut() else {
                eprintln!(
                    "display-artifact-disabled\tset {DISPLAY_JOURNAL_ENV} to a fresh path before startup"
                );
                return;
            };
            if archive.faulted {
                eprintln!("display-artifact-disabled\tjournal requires explicit typed reopen");
                return;
            }
            if self.presentation_output_capture_requested
                || archive.pending_presentation_output().is_some()
            {
                eprintln!("display-artifact-request\tbusy=presentation-output");
                return;
            }
            if archive.pending_artifact().is_some() {
                let result = archive.append_pending();
                if let Some(display) = self.display.as_ref() {
                    report_display_artifact_result(&display.window, result);
                }
                return;
            }
            if self.display_artifact_capture_requested {
                eprintln!("display-artifact-request\tcoalesced");
                return;
            }
            self.display_artifact_capture_requested = true;
            eprintln!("display-artifact-request\tarmed");
            if let Some(display) = self.display.as_ref() {
                display.window.request_redraw();
            }
        }
        #[cfg(not(all(windows, feature = "protocol-research")))]
        eprintln!(
            "display-artifact-disabled\tF4 requires Windows and --features protocol-research"
        );
    }

    fn request_presentation_output_capture(&mut self) {
        #[cfg(all(windows, feature = "protocol-research"))]
        {
            let Some(archive) = self.display_artifact_archive.as_mut() else {
                eprintln!(
                    "presentation-output-disabled\tset {DISPLAY_JOURNAL_ENV} to a fresh path before startup"
                );
                return;
            };
            if archive.faulted {
                eprintln!("presentation-output-disabled\tjournal requires explicit typed reopen");
                return;
            }
            if self.display_artifact_capture_requested || archive.pending_artifact().is_some() {
                eprintln!("presentation-output-request\tbusy=display-artifact");
                return;
            }
            if archive.pending_presentation_output().is_some() {
                let result = archive.append_pending_presentation_output();
                if let Some(display) = self.display.as_ref() {
                    report_presentation_output_result(
                        &display.window,
                        result.map_err(LivePresentationOutputFailure::Store),
                    );
                }
                return;
            }
            if self.presentation_output_capture_requested {
                eprintln!("presentation-output-request\tcoalesced");
                return;
            }
            self.presentation_output_capture_requested = true;
            eprintln!("presentation-output-request\tarmed-next-presentation");
            if let Some(display) = self.display.as_ref() {
                // A redraw services UI state but does not forge a frame, tick, or resubmit the
                // current surface. The request stays armed until a normal presentation exists.
                display
                    .window
                    .set_title(PRESENTATION_EVIDENCE_WAITING_TITLE);
                display.window.request_redraw();
            }
        }
        #[cfg(not(all(windows, feature = "protocol-research")))]
        eprintln!(
            "presentation-output-disabled\tF5 requires Windows and --features protocol-research"
        );
    }

    fn key(&mut self, code: KeyCode, pressed: bool) {
        match code {
            KeyCode::KeyW => self.apply_infallible_accepted_action(AcceptedAction::MoveEdge(
                MoveControl::Forward,
                pressed,
            )),
            KeyCode::KeyS => self.apply_infallible_accepted_action(AcceptedAction::MoveEdge(
                MoveControl::Backward,
                pressed,
            )),
            KeyCode::KeyA => self.apply_infallible_accepted_action(AcceptedAction::MoveEdge(
                MoveControl::Left,
                pressed,
            )),
            KeyCode::KeyD => self.apply_infallible_accepted_action(AcceptedAction::MoveEdge(
                MoveControl::Right,
                pressed,
            )),
            KeyCode::Space => self.apply_infallible_accepted_action(AcceptedAction::MoveEdge(
                MoveControl::Up,
                pressed,
            )),
            KeyCode::ShiftLeft | KeyCode::ShiftRight => self.apply_infallible_accepted_action(
                AcceptedAction::MoveEdge(MoveControl::Down, pressed),
            ),
            KeyCode::KeyF if pressed => {
                self.apply_infallible_accepted_action(AcceptedAction::FitWorld);
            }
            KeyCode::F3 if pressed => {
                self.apply_infallible_accepted_action(AcceptedAction::ToggleSidebar);
            }
            KeyCode::F4 if pressed => self.request_display_artifact_capture(),
            KeyCode::F5 if pressed => self.request_presentation_output_capture(),
            KeyCode::Escape if pressed => self.capture_mouse(false),
            _ => {}
        }
    }
}

fn headless_pixel_shape(width: u32, height: u32) -> Result<(u128, u128), io::Error> {
    let output_pixels = u128::from(width)
        .checked_mul(u128::from(height))
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "pixel count overflow"))?;
    if output_pixels > u128::from(HEADLESS_SIMULATION_MAX_OUTPUT_PIXELS) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!(
                "headless resize {width}x{height} requires {output_pixels} output pixels; policy limit is {HEADLESS_SIMULATION_MAX_OUTPUT_PIXELS}"
            ),
        ));
    }
    let sidebar_width = SIDEBAR_WIDTH.min(width.saturating_sub(1));
    let simulation_pixels = u128::from(width - sidebar_width)
        .checked_mul(u128::from(height))
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                "simulation pixel count overflow",
            )
        })?;
    Ok((output_pixels, simulation_pixels))
}

fn checked_headless_charge(
    accumulated: &mut u128,
    addition: u128,
    limit: u128,
    lane: &str,
    tick: u64,
) -> Result<(), io::Error> {
    let next = accumulated.checked_add(addition).ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("headless {lane} overflow at tick {tick}"),
        )
    })?;
    if next > limit {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!(
                "headless {lane} policy exceeded at tick {tick}: required {next}, limit {limit}"
            ),
        ));
    }
    *accumulated = next;
    Ok(())
}

fn headless_possible_world_bits(
    possible_exact_bits: usize,
    unresolved_aim_place_bits: u128,
) -> Result<u128, io::Error> {
    (possible_exact_bits as u128)
        .checked_add(unresolved_aim_place_bits)
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                "possible world population overflow",
            )
        })
}

type HeadlessChunkKey = (i64, i64, i64);

fn headless_chunk_key(coord: Coord) -> HeadlessChunkKey {
    (
        coord.x.div_euclid(CHUNK_SIDE),
        coord.y.div_euclid(CHUNK_SIDE),
        coord.z.div_euclid(CHUNK_SIDE),
    )
}

fn headless_insert_possible_exact(
    possible_exact_bits: &mut HashSet<Coord>,
    possible_exact_chunk_counts: &mut HashMap<HeadlessChunkKey, usize>,
    coord: Coord,
) -> Result<(), io::Error> {
    if possible_exact_bits.insert(coord) {
        let count = possible_exact_chunk_counts
            .entry(headless_chunk_key(coord))
            .or_default();
        *count = count.checked_add(1).ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                "possible exact chunk population overflow",
            )
        })?;
    }
    Ok(())
}

fn headless_remove_possible_exact(
    possible_exact_bits: &mut HashSet<Coord>,
    possible_exact_chunk_counts: &mut HashMap<HeadlessChunkKey, usize>,
    coord: Coord,
) {
    if !possible_exact_bits.remove(&coord) {
        return;
    }
    let key = headless_chunk_key(coord);
    let remove_chunk = {
        let count = possible_exact_chunk_counts
            .get_mut(&key)
            .expect("a possible exact bit must retain its chunk count");
        *count = count
            .checked_sub(1)
            .expect("a possible exact chunk count must be positive");
        *count == 0
    };
    if remove_chunk {
        possible_exact_chunk_counts.remove(&key);
    }
}

fn headless_possible_chunk_count(
    possible_exact_chunks: usize,
    unresolved_aim_place_bits: u128,
) -> Result<u128, io::Error> {
    (possible_exact_chunks as u128)
        .checked_add(unresolved_aim_place_bits)
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                "possible chunk population overflow",
            )
        })
}

fn headless_chunk_word_units(possible_chunks: u128) -> Result<u128, io::Error> {
    possible_chunks
        .checked_mul(CHUNK_WORDS as u128)
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "chunk-word charge overflow"))
}

fn headless_world_ordering_units(population: u128) -> Result<u128, io::Error> {
    let levels = if population <= 1 {
        1
    } else {
        u128::BITS - (population - 1).leading_zeros()
    };
    population
        .checked_mul(u128::from(levels))
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "world-ordering charge overflow"))
}

/// Refuses an expensive tape before constructing or mutating the simulation.
///
/// The accounting follows a pure dirty-state shadow while conservatively assuming that requested
/// movement succeeds. Exact edits update possible/definite coordinate sets, unresolved aimed
/// placements only raise the possible population, and each voxel can cover the complete viewport
/// once per cube triangle. Every nonzero resize is charged separately because repeated grow/shrink
/// actions allocate and clear buffers even when the final dimensions are small.
fn preflight_headless_tape(tape: &InteractionTape) -> Result<(), io::Error> {
    let mut width = INITIAL_OUTPUT_WIDTH;
    let mut height = INITIAL_VIEWPORT;
    let mut possible_exact_bits = HashSet::from([BEDROCK_COORD]);
    let mut possible_exact_chunk_counts =
        HashMap::from([(headless_chunk_key(BEDROCK_COORD), 1_usize)]);
    let mut definite_exact_bits = HashSet::from([BEDROCK_COORD]);
    let mut unresolved_aim_place_bits = 0_u128;
    let mut movement = [false; 6];
    let mut resize_pixel_writes = 0_u128;
    let mut output_pixel_ticks = 0_u128;
    let mut raster_sample_tests = 0_u128;
    let mut world_coordinate_tests = 0_u128;
    let mut edit_coordinate_tests = 0_u128;
    let mut chunk_word_tests = 0_u128;
    let mut world_ordering_units = 0_u128;

    for record in tape.records() {
        let mut record_peak_possible_bits =
            headless_possible_world_bits(possible_exact_bits.len(), unresolved_aim_place_bits)?;
        let mut record_peak_possible_chunks = headless_possible_chunk_count(
            possible_exact_chunk_counts.len(),
            unresolved_aim_place_bits,
        )?;
        let mut aim_actions = 0_u128;
        let mut wheel = 0.0_f64;
        let mut look_x = 0.0_f64;
        let mut look_y = 0.0_f64;
        let mut has_edit_action = false;
        let mut has_resize = false;
        let mut fit_requested = false;
        let mut game_dirty = record.ordinal() == 0;
        let mut sidebar_dirty = false;
        for (action_index, &action) in record.actions().iter().enumerate() {
            match action {
                AcceptedAction::MoveEdge(control, pressed) => {
                    let index = match control {
                        MoveControl::Forward => 0,
                        MoveControl::Backward => 1,
                        MoveControl::Left => 2,
                        MoveControl::Right => 3,
                        MoveControl::Up => 4,
                        MoveControl::Down => 5,
                    };
                    movement[index] = pressed;
                }
                AcceptedAction::LookDelta(x, y) => {
                    let map_error = |error: AcceptedActionApplyError| {
                        io::Error::new(
                            io::ErrorKind::InvalidData,
                            format!(
                                "interaction tick {} action {action_index} was refused during preflight: {error}",
                                record.ordinal()
                            ),
                        )
                    };
                    let next_x =
                        checked_accepted_accumulation("look_x", look_x, x).map_err(map_error)?;
                    let next_y =
                        checked_accepted_accumulation("look_y", look_y, y).map_err(map_error)?;
                    look_x = next_x;
                    look_y = next_y;
                }
                AcceptedAction::WheelLines(lines) => {
                    wheel = (wheel + lines).clamp(-64.0, 64.0);
                }
                AcceptedAction::Resize(next_width, next_height)
                    if next_width != 0 && next_height != 0 =>
                {
                    let (output_pixels, simulation_pixels) =
                        headless_pixel_shape(next_width, next_height)?;
                    let resize_charge = simulation_pixels
                        .checked_mul(2)
                        .and_then(|simulation_buffers| {
                            output_pixels.checked_add(simulation_buffers)
                        })
                        .ok_or_else(|| {
                            io::Error::new(
                                io::ErrorKind::InvalidData,
                                "resize pixel-write charge overflow",
                            )
                        })?;
                    checked_headless_charge(
                        &mut resize_pixel_writes,
                        resize_charge,
                        HEADLESS_SIMULATION_MAX_RESIZE_PIXEL_WRITES,
                        "resize-pixel-write",
                        record.ordinal(),
                    )?;
                    width = next_width;
                    height = next_height;
                    has_resize = true;
                    game_dirty = true;
                }
                AcceptedAction::AimDelete => {
                    aim_actions = aim_actions.checked_add(1).ok_or_else(|| {
                        io::Error::new(io::ErrorKind::InvalidData, "aim-action count overflow")
                    })?;
                    has_edit_action = true;
                    game_dirty = true;
                }
                AcceptedAction::AimPlace => {
                    aim_actions = aim_actions.checked_add(1).ok_or_else(|| {
                        io::Error::new(io::ErrorKind::InvalidData, "aim-action count overflow")
                    })?;
                    unresolved_aim_place_bits =
                        unresolved_aim_place_bits.checked_add(1).ok_or_else(|| {
                            io::Error::new(
                                io::ErrorKind::InvalidData,
                                "possible aimed-placement population overflow",
                            )
                        })?;
                    has_edit_action = true;
                    game_dirty = true;
                }
                AcceptedAction::ExactEdit(edit) => {
                    let possible_before = headless_possible_world_bits(
                        possible_exact_bits.len(),
                        unresolved_aim_place_bits,
                    )?;
                    let possible_chunks_before = headless_possible_chunk_count(
                        possible_exact_chunk_counts.len(),
                        unresolved_aim_place_bits,
                    )?;
                    if matches!(edit.operation, Operation::Off | Operation::Toggle) {
                        checked_headless_charge(
                            &mut edit_coordinate_tests,
                            possible_before,
                            HEADLESS_SIMULATION_MAX_EDIT_COORDINATE_TESTS,
                            "edit-coordinate-test",
                            record.ordinal(),
                        )?;
                        checked_headless_charge(
                            &mut chunk_word_tests,
                            headless_chunk_word_units(possible_chunks_before)?,
                            HEADLESS_SIMULATION_MAX_CHUNK_WORD_TESTS,
                            "chunk-word-test",
                            record.ordinal(),
                        )?;
                    }
                    if edit.coord != BEDROCK_COORD {
                        match edit.operation {
                            Operation::On => {
                                headless_insert_possible_exact(
                                    &mut possible_exact_bits,
                                    &mut possible_exact_chunk_counts,
                                    edit.coord,
                                )?;
                            }
                            Operation::Off => {
                                headless_remove_possible_exact(
                                    &mut possible_exact_bits,
                                    &mut possible_exact_chunk_counts,
                                    edit.coord,
                                );
                                definite_exact_bits.remove(&edit.coord);
                            }
                            Operation::Toggle => {
                                if definite_exact_bits.remove(&edit.coord) {
                                    headless_remove_possible_exact(
                                        &mut possible_exact_bits,
                                        &mut possible_exact_chunk_counts,
                                        edit.coord,
                                    );
                                } else {
                                    headless_insert_possible_exact(
                                        &mut possible_exact_bits,
                                        &mut possible_exact_chunk_counts,
                                        edit.coord,
                                    )?;
                                }
                            }
                        }
                    }
                    has_edit_action = true;
                    game_dirty = true;
                }
                AcceptedAction::FitWorld => {
                    fit_requested = true;
                    game_dirty = true;
                }
                AcceptedAction::ToggleSidebar => sidebar_dirty = true,
                AcceptedAction::ClearMotion => {
                    movement.fill(false);
                    look_x = 0.0;
                    look_y = 0.0;
                }
                _ => {}
            }
            record_peak_possible_bits = record_peak_possible_bits.max(
                headless_possible_world_bits(possible_exact_bits.len(), unresolved_aim_place_bits)?,
            );
            let possible_chunks = headless_possible_chunk_count(
                possible_exact_chunk_counts.len(),
                unresolved_aim_place_bits,
            )?;
            if possible_chunks > HEADLESS_SIMULATION_MAX_POSSIBLE_CHUNKS {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!(
                        "headless possible-chunk-count policy exceeded at tick {}: required {possible_chunks}, limit {HEADLESS_SIMULATION_MAX_POSSIBLE_CHUNKS}",
                        record.ordinal()
                    ),
                ));
            }
            record_peak_possible_chunks = record_peak_possible_chunks.max(possible_chunks);
        }

        let possible_world_bits =
            headless_possible_world_bits(possible_exact_bits.len(), unresolved_aim_place_bits)?;
        let (output_pixels, simulation_pixels) = headless_pixel_shape(width, height)?;
        let movement_active =
            movement[0] != movement[1] || movement[2] != movement[3] || movement[4] != movement[5];
        game_dirty |= look_x != 0.0 || look_y != 0.0 || movement_active || wheel != 0.0;
        if game_dirty || sidebar_dirty {
            checked_headless_charge(
                &mut output_pixel_ticks,
                output_pixels,
                HEADLESS_SIMULATION_MAX_OUTPUT_PIXEL_TICKS,
                "output-pixel-tick",
                record.ordinal(),
            )?;
        }
        if game_dirty {
            let tick_raster_sample_tests = simulation_pixels
                .checked_mul(possible_world_bits)
                .and_then(|candidate_pixels| {
                    candidate_pixels.checked_mul(HEADLESS_RENDER_TRIANGLES_PER_BIT_BOUND)
                })
                .ok_or_else(|| {
                    io::Error::new(
                        io::ErrorKind::InvalidData,
                        "raster-sample test charge overflow",
                    )
                })?;
            checked_headless_charge(
                &mut raster_sample_tests,
                tick_raster_sample_tests,
                HEADLESS_SIMULATION_MAX_RASTER_SAMPLE_TESTS,
                "raster-sample-test",
                record.ordinal(),
            )?;
        }

        // Two baseline scans cover the pre/post world commitments. The remaining terms bound a
        // possible edit-receipt commitment, render traversal, raycasts, collision sweeps, and the
        // debug invariant sweeps performed by movement, wheel, resize-refit, and explicit fit.
        let world_scans = 2_u128
            .checked_add(u128::from(has_edit_action))
            .and_then(|scans| scans.checked_add(u128::from(game_dirty)))
            .and_then(|scans| scans.checked_add(u128::from(has_resize)))
            .and_then(|scans| scans.checked_add(u128::from(fit_requested)))
            .and_then(|scans| {
                aim_actions
                    .checked_mul(2)
                    .and_then(|aim_scans| scans.checked_add(aim_scans))
            })
            .and_then(|scans| scans.checked_add(if movement_active { 2 } else { 0 }))
            .and_then(|scans| scans.checked_add(if wheel != 0.0 { 2 } else { 0 }))
            .ok_or_else(|| {
                io::Error::new(io::ErrorKind::InvalidData, "world-scan count overflow")
            })?;
        let tick_world_coordinate_tests = world_scans
            .checked_mul(record_peak_possible_bits)
            .ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::InvalidData,
                    "world-coordinate test charge overflow",
                )
            })?;
        checked_headless_charge(
            &mut world_coordinate_tests,
            tick_world_coordinate_tests,
            HEADLESS_SIMULATION_MAX_WORLD_COORDINATE_TESTS,
            "world-coordinate-test",
            record.ordinal(),
        )?;
        let tick_chunk_word_tests = headless_chunk_word_units(record_peak_possible_chunks)?
            .checked_mul(world_scans)
            .ok_or_else(|| {
                io::Error::new(io::ErrorKind::InvalidData, "chunk-word charge overflow")
            })?;
        checked_headless_charge(
            &mut chunk_word_tests,
            tick_chunk_word_tests,
            HEADLESS_SIMULATION_MAX_CHUNK_WORD_TESTS,
            "chunk-word-test",
            record.ordinal(),
        )?;
        let world_tag_scans = 2_u128
            .checked_add(u128::from(has_edit_action))
            .and_then(|scans| scans.checked_add(u128::from(game_dirty)))
            .ok_or_else(|| {
                io::Error::new(io::ErrorKind::InvalidData, "world-tag scan count overflow")
            })?;
        let tick_world_ordering_units = headless_world_ordering_units(record_peak_possible_bits)?
            .checked_mul(world_tag_scans)
            .ok_or_else(|| {
                io::Error::new(io::ErrorKind::InvalidData, "world-ordering charge overflow")
            })?;
        checked_headless_charge(
            &mut world_ordering_units,
            tick_world_ordering_units,
            HEADLESS_SIMULATION_MAX_WORLD_ORDERING_UNITS,
            "world-ordering",
            record.ordinal(),
        )?;
    }
    let final_tick = tape.records().last().map_or(0, |record| record.ordinal());
    let possible_world_bits =
        headless_possible_world_bits(possible_exact_bits.len(), unresolved_aim_place_bits)?;
    let possible_chunks = headless_possible_chunk_count(
        possible_exact_chunk_counts.len(),
        unresolved_aim_place_bits,
    )?;
    checked_headless_charge(
        &mut world_coordinate_tests,
        possible_world_bits,
        HEADLESS_SIMULATION_MAX_WORLD_COORDINATE_TESTS,
        "world-coordinate-test",
        final_tick,
    )?;
    checked_headless_charge(
        &mut chunk_word_tests,
        headless_chunk_word_units(possible_chunks)?,
        HEADLESS_SIMULATION_MAX_CHUNK_WORD_TESTS,
        "chunk-word-test",
        final_tick,
    )?;
    checked_headless_charge(
        &mut world_ordering_units,
        headless_world_ordering_units(possible_world_bits)?,
        HEADLESS_SIMULATION_MAX_WORLD_ORDERING_UNITS,
        "world-ordering",
        final_tick,
    )?;
    Ok(())
}

fn simulate_interaction_tape(
    tape: &InteractionTape,
) -> Result<HeadlessSimulationSummary, io::Error> {
    preflight_headless_tape(tape)?;
    let mut app = App::for_headless_simulation();
    let mut ticks = Vec::new();
    ticks.try_reserve(tape.len()).map_err(|_| {
        io::Error::new(
            io::ErrorKind::OutOfMemory,
            "could not reserve headless tick summaries",
        )
    })?;

    for record in tape.records() {
        for (action_index, &action) in record.actions().iter().enumerate() {
            app.apply_accepted_action(action).map_err(|error| {
                io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!(
                        "interaction tick {} action {action_index} was refused: {error}",
                        record.ordinal()
                    ),
                )
            })?;
        }
        app.fixed_tick();
        let outcome = app
            .tick_outcomes
            .last()
            .expect("one interaction-tape record must emit exactly one fixed-tick outcome");
        assert_eq!(
            outcome.tick(),
            record.ordinal(),
            "interaction-tape ordinal diverged from the fixed-tick fold"
        );
        let game = outcome.game();
        let composite = outcome.composite();
        ticks.push(HeadlessTickSummary {
            ordinal: outcome.tick(),
            outcome: outcome.tag(),
            post_world: outcome.post_world(),
            post_history: outcome.post_history(),
            camera_bits: outcome.post_camera().bits(),
            game_input: game.map(TickFrameCommitment::input),
            game_frame: game.map(TickFrameCommitment::frame),
            composite_input: composite.map(TickFrameCommitment::input),
            composite_frame: composite.map(TickFrameCommitment::frame),
        });
    }

    let (game_admissions, composite_admissions, game_receipt_head, composite_receipt_head) = app
        .frame_ledgers_v2
        .as_ref()
        .map_or((0, 0, None, None), |ledgers| {
            (
                ledgers.game.admissions().len(),
                ledgers.composite.admissions().len(),
                ledgers.game.head(),
                ledgers.composite.head(),
            )
        });
    if game_admissions != 0
        || composite_admissions != 0
        || game_receipt_head.is_some()
        || composite_receipt_head.is_some()
        || !app.frame_ledger.is_empty()
        || !app.output_frame_ledger.is_empty()
    {
        return Err(io::Error::other(
            "headless interaction fold unexpectedly published a frame receipt",
        ));
    }

    Ok(HeadlessSimulationSummary {
        tape: tape.content_tag(),
        action_count: tape.total_actions(),
        ticks,
        final_world: world_tag(&app.world),
        final_history: app.history.head(),
        final_camera_bits: TickCameraState::capture(&app.camera).bits(),
        on_bits: app.world.len(),
        coordinate_receipts: app.history.receipts().len(),
        game_admissions,
        composite_admissions,
        game_receipt_head,
        composite_receipt_head,
    })
}

fn optional_tag(tag: Option<ContentTag>) -> String {
    tag.map_or_else(|| "none".to_owned(), |tag| tag.to_string())
}

fn camera_bits_text(bits: [u64; 10]) -> String {
    bits.into_iter()
        .map(|bits| format!("{bits:016x}"))
        .collect::<Vec<_>>()
        .join(",")
}

fn print_headless_summary(summary: &HeadlessSimulationSummary) {
    println!(
        "interaction-simulation\ttape={}\tticks={}\tactions={}\tscope=fixed-tick-projection\trun_receipt=unavailable",
        summary.tape,
        summary.ticks.len(),
        summary.action_count,
    );
    for tick in &summary.ticks {
        println!(
            "interaction-tick\tordinal={}\toutcome={}\tworld={}\thistory={}\tgame_input={}\tgame_frame={}\tcomposite_input={}\tcomposite_frame={}\tcamera_bits={}",
            tick.ordinal,
            tick.outcome,
            tick.post_world,
            tick.post_history,
            optional_tag(tick.game_input),
            optional_tag(tick.game_frame),
            optional_tag(tick.composite_input),
            optional_tag(tick.composite_frame),
            camera_bits_text(tick.camera_bits),
        );
    }
    println!(
        "interaction-final\tworld={}\thistory={}\ton_bits={}\tcoordinate_receipts={}\tcamera_bits={}",
        summary.final_world,
        summary.final_history,
        summary.on_bits,
        summary.coordinate_receipts,
        camera_bits_text(summary.final_camera_bits),
    );
    println!(
        "interaction-boundary\tgame_admissions={}\tcomposite_admissions={}\tgame_receipt_head={}\tcomposite_receipt_head={}\tdisplay_evidence=unavailable-without-window",
        summary.game_admissions,
        summary.composite_admissions,
        optional_tag(summary.game_receipt_head),
        optional_tag(summary.composite_receipt_head),
    );
}

#[cfg(windows)]
fn display_audit_enabled() -> bool {
    std::env::var("BITBLOCK_DISPLAY_AUDIT").is_ok_and(|value| {
        value == "1" || value.eq_ignore_ascii_case("true") || value.eq_ignore_ascii_case("on")
    })
}

#[cfg(all(windows, feature = "protocol-research"))]
fn presentation_audit_enabled() -> bool {
    std::env::var(PRESENTATION_AUDIT_ENV).is_ok_and(|value| {
        value == "1" || value.eq_ignore_ascii_case("true") || value.eq_ignore_ascii_case("on")
    })
}

#[cfg(windows)]
fn capture_visible_client(window: &Window, expected: FrameRef<'_>) -> DisplayCapture {
    let Some(hwnd) = win32_hwnd(window) else {
        return DisplayCapture::unavailable(CaptureStage::WindowHandle, 0);
    };
    // SAFETY: winit owns this HWND on the current event-loop thread, and the borrowed window
    // outlives the synchronous capture call.
    unsafe { bitblock::display::windows::capture_client(hwnd, expected) }
}

#[cfg(windows)]
fn win32_hwnd(window: &Window) -> Option<HWND> {
    let handle = window.window_handle().ok()?;
    let RawWindowHandle::Win32(handle) = handle.as_raw() else {
        return None;
    };
    Some(handle.hwnd.get() as HWND)
}

#[cfg(windows)]
fn disable_window_corner_clipping(window: &Window) -> Result<(), i32> {
    let Some(hwnd) = win32_hwnd(window) else {
        return Err(-1);
    };
    let preference = DWMWCP_DONOTROUND;
    let result = unsafe {
        DwmSetWindowAttribute(
            hwnd,
            DWMWA_WINDOW_CORNER_PREFERENCE as u32,
            (&preference as *const i32).cast(),
            std::mem::size_of_val(&preference) as u32,
        )
    };
    if result < 0 { Err(result) } else { Ok(()) }
}

fn top_align_window_on_monitor(window: &Window) {
    let Some(monitor) = window.current_monitor() else {
        return;
    };
    let monitor_position = monitor.position();
    let monitor_size = monitor.size();
    let outer_size = window.outer_size();
    let horizontal_margin = monitor_size.width.saturating_sub(outer_size.width) / 2;
    let x = i64::from(monitor_position.x) + i64::from(horizontal_margin);
    let x = x.clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32;
    window.set_outer_position(PhysicalPosition::new(x, monitor_position.y));
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.display.is_some() {
            return;
        }

        #[cfg(all(windows, feature = "protocol-research"))]
        let title = if self.display_artifact_archive.is_some() {
            RESEARCH_WINDOW_TITLE
        } else {
            "bitblock — F3 metrics"
        };
        #[cfg(not(all(windows, feature = "protocol-research")))]
        let title = "bitblock — F3 metrics";
        let attributes = Window::default_attributes()
            .with_title(title)
            .with_inner_size(PhysicalSize::new(INITIAL_OUTPUT_WIDTH, INITIAL_VIEWPORT))
            .with_min_inner_size(PhysicalSize::new(SIDEBAR_WIDTH + 1, 1));
        let window = Arc::new(
            event_loop
                .create_window(attributes)
                .expect("failed to create bitblock window"),
        );
        top_align_window_on_monitor(&window);
        #[cfg(windows)]
        if let Err(error) = disable_window_corner_clipping(&window) {
            eprintln!("window-corner-policy\tHRESULT={error:#010x}");
        }
        #[cfg(windows)]
        let surface = {
            let hwnd = win32_hwnd(&window).expect("winit did not expose its Win32 HWND");
            // SAFETY: the Arc<Window> stored alongside the surface owns this HWND and all calls
            // remain on the winit event-loop thread.
            unsafe { WindowsPixelSurface::new(hwnd) }
                .expect("failed to create checked Win32 pixel surface")
        };
        #[cfg(not(windows))]
        let context = Context::new(Arc::clone(&window)).expect("failed to create pixel context");
        #[cfg(not(windows))]
        let surface = Surface::new(&context, Arc::clone(&window))
            .expect("failed to create physical-pixel surface");
        let size = window.inner_size();
        #[cfg(windows)]
        {
            self.display = Some(Display { surface, window });
        }
        #[cfg(not(windows))]
        {
            self.display = Some(Display {
                surface,
                _context: context,
                window,
            });
        }
        self.apply_infallible_accepted_action(AcceptedAction::Resize(size.width, size.height));
        let now = Instant::now();
        self.last_frame = now;
        self.next_tick = now + TICK_DURATION;
        self.accumulator = TICK_DURATION;
        self.needs_present = self.advance_ticks(now);
        self.capture_mouse(true);
        event_loop.set_control_flow(ControlFlow::WaitUntil(self.next_tick));
        if self.needs_present
            && let Some(display) = self.display.as_ref()
        {
            display.window.request_redraw();
        }
    }

    fn suspended(&mut self, _event_loop: &ActiveEventLoop) {
        self.display = None;
        self.mouse_captured = false;
        self.needs_present = false;
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        window_id: WindowId,
        event: WindowEvent,
    ) {
        if self
            .display
            .as_ref()
            .is_none_or(|display| display.window.id() != window_id)
        {
            return;
        }

        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::Resized(size) => {
                self.apply_infallible_accepted_action(AcceptedAction::Resize(
                    size.width,
                    size.height,
                ));
            }
            WindowEvent::Focused(false) => self.capture_mouse(false),
            WindowEvent::KeyboardInput {
                event,
                is_synthetic: false,
                ..
            } => {
                if let PhysicalKey::Code(code) = event.physical_key
                    && !(event.repeat && matches!(code, KeyCode::F4 | KeyCode::F5))
                {
                    self.key(code, event.state == ElementState::Pressed);
                }
            }
            WindowEvent::CursorMoved { position, .. } => {
                self.cursor_position = Some(position);
            }
            WindowEvent::MouseWheel { delta, .. } => {
                let lines = match delta {
                    MouseScrollDelta::LineDelta(_, y) => y as f64,
                    MouseScrollDelta::PixelDelta(position) => position.y / 120.0,
                };
                self.queue_wheel(lines);
            }
            WindowEvent::MouseInput {
                state: ElementState::Pressed,
                button,
                ..
            } => {
                if !self.mouse_captured {
                    if !self.pointer_in_sidebar() {
                        self.capture_mouse(true);
                    }
                } else {
                    match button {
                        MouseButton::Left => {
                            self.apply_infallible_accepted_action(AcceptedAction::AimDelete)
                        }
                        MouseButton::Right => {
                            self.apply_infallible_accepted_action(AcceptedAction::AimPlace)
                        }
                        _ => {}
                    }
                }
            }
            WindowEvent::RedrawRequested => {
                self.present();
            }
            _ => {}
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        let now = Instant::now();
        if now >= self.next_tick {
            self.needs_present |= self.advance_ticks(now);
            if self.needs_present
                && let Some(display) = self.display.as_ref()
            {
                display.window.request_redraw();
            }
            self.next_tick = now + TICK_DURATION.saturating_sub(self.accumulator);
        }
        event_loop.set_control_flow(ControlFlow::WaitUntil(self.next_tick));
    }

    fn device_event(
        &mut self,
        _event_loop: &ActiveEventLoop,
        _device_id: DeviceId,
        event: DeviceEvent,
    ) {
        if !self.mouse_captured {
            return;
        }
        if let DeviceEvent::MouseMotion { delta: (x, y) } = event
            && let Err(error) = self.apply_accepted_action(AcceptedAction::LookDelta(x, y))
        {
            eprintln!("accepted-action-refused\t{error}");
        }
    }
}

fn resolve_aim_edit(world: &World, camera: &Camera, edit: AimEdit) -> Option<WorldEdit> {
    let hit = raycast(world, camera.center_ray());
    match edit {
        AimEdit::Delete => hit.map(|hit| WorldEdit::new(Operation::Off, hit.coord)),
        AimEdit::Place => {
            let target = hit
                .and_then(|hit| hit.adjacent)
                .or_else(|| coord_containing(camera.observation_anchor()));
            target.map(|target| WorldEdit::new(Operation::On, target))
        }
    }
}

fn collision_limited_delta(world: &World, start: Vec3, requested: Vec3) -> Vec3 {
    let distance = requested.length();
    if distance == 0.0 {
        return Vec3::ZERO;
    }
    let Some(hit) = sweep_point(world, start, requested) else {
        return requested;
    };

    let coordinate_scale = start
        .x
        .abs()
        .max(start.y.abs())
        .max(start.z.abs())
        .max(hit.distance)
        .max(1.0);
    let clearance = OBSERVER_COLLISION_CLEARANCE.max(coordinate_scale * f64::EPSILON * 16.0);
    let allowed_distance = (hit.distance - clearance).max(0.0);
    requested * (allowed_distance / distance)
}

fn observer_is_clear(world: &World, position: Vec3) -> bool {
    sweep_point(world, position, Vec3::ZERO).is_none()
}

fn edit_would_enclose_observer(world: &World, camera: &Camera, edit: WorldEdit) -> bool {
    let turns_on = match edit.operation {
        Operation::On => !world.is_on(edit.coord),
        Operation::Toggle => !world.is_on(edit.coord),
        Operation::Off => false,
    };
    turns_on && coord_containing(camera.position()) == Some(edit.coord)
}

#[cfg(test)]
fn apply_aim_edit(world: &mut World, camera: &Camera, edit: AimEdit) -> bool {
    resolve_aim_edit(world, camera, edit)
        .filter(|edit| !edit_would_enclose_observer(world, camera, *edit))
        .is_some_and(|edit| edit.apply(world))
}

fn axis(positive: bool, negative: bool) -> f64 {
    f64::from(positive as u8) - f64::from(negative as u8)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(all(windows, feature = "protocol-research"))]
    use std::fs;
    #[cfg(all(windows, feature = "protocol-research"))]
    use std::sync::atomic::{AtomicU64, Ordering};

    #[cfg(all(windows, feature = "protocol-research"))]
    static DISPLAY_PATH_SERIAL: AtomicU64 = AtomicU64::new(0);

    #[cfg(all(windows, feature = "protocol-research"))]
    struct TempDisplayJournal(PathBuf);

    #[cfg(all(windows, feature = "protocol-research"))]
    impl TempDisplayJournal {
        fn new(label: &str) -> Self {
            let serial = DISPLAY_PATH_SERIAL.fetch_add(1, Ordering::Relaxed);
            Self(std::env::temp_dir().join(format!(
                "bitblock-live-display-{label}-{}-{serial}.bbj",
                std::process::id()
            )))
        }
    }

    #[cfg(all(windows, feature = "protocol-research"))]
    impl Drop for TempDisplayJournal {
        fn drop(&mut self) {
            match fs::remove_file(&self.0) {
                Ok(()) => {}
                Err(error) if error.kind() == io::ErrorKind::NotFound => {}
                Err(error) => panic!("could not remove live display-journal test file: {error}"),
            }
        }
    }

    #[cfg(all(windows, feature = "protocol-research"))]
    fn test_qpc(value: i64) -> bitblock::presentation::QpcRead {
        bitblock::presentation::QpcRead {
            succeeded: true,
            value,
            error_code: 0,
        }
    }

    #[cfg(all(windows, feature = "protocol-research"))]
    fn test_qpc_bracket(before: i64, after: i64) -> bitblock::presentation::QpcBracket {
        bitblock::presentation::QpcBracket {
            before: test_qpc(before),
            after: test_qpc(after),
        }
    }

    #[cfg(all(windows, feature = "protocol-research"))]
    fn test_timing(receipt: FrameReceiptV2) -> CompositionTimingEnvelope {
        use bitblock::presentation::{
            DwmFlushCall, DwmTimingCall, GdiFlushCall, PresenterEvidence, QpcFrequency,
        };

        let policy = bitblock::protocol::hash_canonical(
            bitblock::protocol::TagKind::Policy,
            b"bitblock/test-presenter-policy/v1",
            0,
            |_| {},
        )
        .unwrap();
        CompositionTimingEnvelope::new(
            receipt,
            PresenterEvidence {
                policy,
                requested_scanlines: 1,
                returned_scanlines: 1,
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
                bracket: test_qpc_bracket(10, 11),
                hresult: -1,
                snapshot: None,
            },
            test_qpc_bracket(12, 13),
            GdiFlushCall {
                bracket: test_qpc_bracket(14, 15),
                succeeded: true,
            },
            DwmFlushCall {
                bracket: test_qpc_bracket(16, 17),
                hresult: 0,
            },
            DwmTimingCall {
                bracket: test_qpc_bracket(18, 19),
                hresult: -1,
                snapshot: None,
            },
        )
        .unwrap()
    }

    #[cfg(all(windows, feature = "protocol-research"))]
    fn test_unavailable_topology(
        profile: u32,
    ) -> bitblock::output_topology::OutputTopologyObservation {
        use bitblock::output_topology::{AcquisitionAttempt, AcquisitionFacts};

        bitblock::output_topology::OutputTopologyObservation::new_unavailable(
            AcquisitionFacts {
                profile_code_raw: profile,
                request_flags_raw: 0x52,
                attempts: vec![AcquisitionAttempt {
                    operation_code_raw: 1,
                    profile_code_raw: profile,
                    request_flags_raw: 0x52,
                    retry_ordinal: 0,
                    outcome_code_raw: 1,
                    error_code: 1168,
                    reported_path_capacity: 0,
                    reported_mode_capacity: 0,
                }],
            },
            1,
            1168,
        )
        .unwrap()
    }

    #[cfg(all(windows, feature = "protocol-research"))]
    fn test_complete_capture(expected: FrameRef<'_>) -> DisplayCapture {
        let rect = bitblock::display::CaptureRect {
            left: 0,
            top: 0,
            right: expected.width() as i32,
            bottom: expected.height() as i32,
        };
        DisplayCapture {
            facts: bitblock::display::DisplayCaptureFacts {
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
                qpc_end: 120,
                qpc_frequency: 10_000_000,
                captured: Some(expected.observe()),
                mismatch_count: 0,
                first_mismatch: None,
            },
            pixels: Some(expected.pixels().to_vec()),
        }
    }

    #[test]
    fn app_starts_in_the_cell_adjacent_to_bedrock() {
        let app = App::new();

        assert_eq!(app.camera.position(), Vec3::new(0.0, 0.0, -1.0));
        assert_eq!(app.camera.observation_anchor(), Vec3::ZERO);
        assert!(app.world.is_on(BEDROCK_COORD));
    }

    #[test]
    fn place_cannot_enclose_the_observer_at_spawn() {
        let camera = Camera::for_observer_spawn();
        let mut world = World::new();
        let target = raycast(&world, camera.center_ray())
            .and_then(|hit| hit.adjacent)
            .expect("the initial bit must have a placeable face");

        assert_eq!(target, bitblock::world::Coord::new(0, 0, -1));
        assert!(!world.is_on(target));
        assert!(!apply_aim_edit(&mut world, &camera, AimEdit::Place));

        assert!(!world.is_on(target));
        assert_eq!(world.len(), 1);
    }

    #[test]
    fn place_succeeds_when_the_target_is_not_the_observer_cell() {
        let camera = Camera::from_anchor(
            Vec3::ZERO,
            2.0,
            0.0,
            0.0,
            bitblock::camera::DEFAULT_VERTICAL_FOV_RADIANS,
        );
        let mut world = World::new();
        let target = bitblock::world::Coord::new(0, 0, -1);

        assert!(apply_aim_edit(&mut world, &camera, AimEdit::Place));

        assert!(world.is_on(target));
        assert_eq!(world.len(), 2);
    }

    #[test]
    fn delete_cannot_break_the_origin_bedrock() {
        let camera = Camera::for_observer_spawn();
        let mut world = World::new();

        assert!(!apply_aim_edit(&mut world, &camera, AimEdit::Delete));

        assert!(world.is_on(BEDROCK_COORD));
        assert_eq!(world.len(), 1);
    }

    #[test]
    fn visual_and_text_edits_share_the_exact_instruction_type() {
        let camera = Camera::for_observer_spawn();
        let world = World::new();
        let visual = resolve_aim_edit(&world, &camera, AimEdit::Delete).unwrap();
        let textual = Program::parse("off 0 0 0").unwrap().edits()[0];

        assert_eq!(visual, textual);
        assert_eq!(visual.to_string(), "off 0 0 0");
    }

    #[test]
    fn startup_program_is_applied_inside_the_first_tick() {
        let program = Program::parse("off 0 0 0\non 7 -8 9").unwrap();
        let mut app = App::with_program(program);

        assert!(app.world.is_on(bitblock::world::Coord::new(0, 0, 0)));
        assert!(app.fixed_tick());
        assert!(app.world.is_on(bitblock::world::Coord::new(0, 0, 0)));
        assert!(app.world.is_on(bitblock::world::Coord::new(7, -8, 9)));
    }

    #[test]
    fn startup_program_cannot_enclose_the_observer() {
        let spawn = bitblock::world::Coord::new(0, 0, -1);
        let program = Program::parse("on 0 0 -1\ntoggle 0 0 -1").unwrap();
        let mut app = App::with_program(program);

        app.fixed_tick();

        assert!(!app.world.is_on(spawn));
        assert!(observer_is_clear(&app.world, app.camera.position()));
        let batch = app.last_tick_batch.as_ref().unwrap();
        assert_eq!(batch.attempts(), 2);
        assert_eq!(batch.resolved_edits().len(), 0);
        assert_eq!(batch.misses(), 2);
    }

    #[test]
    fn retained_tick_batch_replays_the_exact_world_transition() {
        let program = Program::parse("off 0 0 0\non 7 -8 9\non 7 -8 9").unwrap();
        let mut app = App::with_program(program);
        app.fixed_tick();

        let batch = app.last_tick_batch.as_ref().unwrap();
        let mut replay = World::new();
        assert_eq!(batch.tick(), 0);
        assert_eq!(batch.attempts(), 3);
        assert_eq!(batch.resolved_edits().len(), 3);
        assert_eq!(batch.replay(&mut replay), 1);

        let mut actual = app.world.iter_on().collect::<Vec<_>>();
        let mut reproduced = replay.iter_on().collect::<Vec<_>>();
        actual.sort_unstable();
        reproduced.sort_unstable();
        assert_eq!(reproduced, actual);

        assert_eq!(app.history.receipts().len(), 1);
        assert_eq!(app.history.circulating_supply(), 1);
        let history_replay = app.history.verify(&World::new()).unwrap();
        assert_eq!(
            bitblock::history::world_tag(&history_replay),
            bitblock::history::world_tag(&app.world)
        );
    }

    #[test]
    fn every_fixed_tick_emits_one_contiguous_outcome_including_idle_ticks() {
        let mut app = App::new();
        let genesis_world = app.history.current_world();
        let genesis_history = app.history.head();

        assert!(app.fixed_tick());
        let first = app.tick_outcomes.last().cloned().unwrap();
        assert_eq!(first.tick(), 0);
        assert_eq!(first.parent(), None);
        assert_eq!(first.pre_world(), genesis_world);
        assert_eq!(first.pre_history(), genesis_history);
        assert_eq!(first.post_world(), app.history.current_world());
        assert_eq!(first.post_history(), app.history.head());
        assert_eq!(first.post_camera(), TickCameraState::capture(&app.camera));
        assert!(first.input().flags().game_dirty());
        assert_eq!(first.attempts(), []);
        let pending_game = app.pending_frame.unwrap();
        let pending_output = app.pending_output_frame.unwrap();
        assert_eq!(
            first.game(),
            Some(TickFrameCommitment::new(pending_game.input, pending_game.frame.tag).unwrap())
        );
        assert_eq!(
            first.composite(),
            Some(TickFrameCommitment::new(pending_output.input, pending_output.frame.tag).unwrap())
        );
        assert_eq!(pending_game.source_tick, first.tick());
        assert_eq!(pending_output.source_tick, first.tick());
        assert_eq!(pending_game.source_outcome, first.tag());
        assert_eq!(pending_output.source_outcome, first.tag());
        first
            .verify_game_input_binding(renderer_profile_tag(), crosshair_overlay_input().tag())
            .unwrap();

        assert!(!app.fixed_tick());
        let second = app.tick_outcomes.last().cloned().unwrap();
        assert_eq!(second.tick(), 1);
        assert_eq!(second.parent(), Some(first.tag()));
        assert_eq!(second.pre_world(), first.post_world());
        assert_eq!(second.pre_history(), first.post_history());
        assert_eq!(second.post_camera(), first.post_camera());
        assert_eq!(second.game(), None);
        assert_eq!(second.composite(), None);
        assert!(!second.rendered());
        second
            .verify_game_input_binding(renderer_profile_tag(), crosshair_overlay_input().tag())
            .unwrap();
        verify_successor(Some(&first), &second).unwrap();
        assert_eq!(app.tick_outcomes, [first, second]);
        assert_eq!(app.tick, 2);
    }

    #[test]
    fn outcome_preserves_rejections_noops_and_the_exact_tick_batch_projection() {
        let spawn = bitblock::world::Coord::new(0, 0, -1);
        let toggled = bitblock::world::Coord::new(7, -8, 9);
        let off_bedrock = WorldEdit::new(Operation::Off, BEDROCK_COORD);
        let enclose = WorldEdit::new(Operation::On, spawn);
        let toggle = WorldEdit::new(Operation::Toggle, toggled);
        let program = Program::from_edits([off_bedrock, enclose, toggle]);
        let mut app = App::with_program(program);

        app.fixed_tick();

        let outcome = app.tick_outcomes.last().unwrap();
        assert_eq!(
            outcome.attempts(),
            [
                TickAttempt::ExactExecuted(off_bedrock),
                TickAttempt::ExactRejectedObserver(enclose),
                TickAttempt::ExactExecuted(toggle),
            ]
        );
        let executed = outcome.executed_edits().collect::<Vec<_>>();
        assert_eq!(executed, [off_bedrock, toggle]);
        let batch = app.last_tick_batch.as_ref().unwrap();
        assert_eq!(batch.tick(), outcome.tick());
        assert_eq!(batch.attempts(), outcome.attempts().len());
        assert_eq!(batch.resolved_edits(), executed);
        assert_eq!(batch.misses(), 1);

        let mut replay = World::new();
        for edit in executed {
            edit.apply(&mut replay);
        }
        assert_eq!(world_tag(&replay), outcome.post_world());
        assert_eq!(outcome.post_world(), app.history.current_world());
        assert_eq!(outcome.post_history(), app.history.head());
        assert_eq!(app.history.receipts().len(), 1);
        assert_eq!(app.history.receipts()[0].edits(), [off_bedrock, toggle]);
        assert!(app.world.is_on(toggled));
        assert!(!app.world.is_on(spawn));
    }

    #[test]
    fn live_aim_inputs_map_to_named_outcome_dispositions() {
        let mut app = App::new();
        app.inputs.edits.extend([
            PendingEdit::Aim(AimEdit::Place),
            PendingEdit::Aim(AimEdit::Delete),
        ]);
        app.fixed_tick();
        assert_eq!(
            app.tick_outcomes.last().unwrap().attempts(),
            [
                TickAttempt::AimPlaceRejectedObserver(WorldEdit::new(
                    Operation::On,
                    bitblock::world::Coord::new(0, 0, -1),
                )),
                TickAttempt::AimDeleteExecuted(WorldEdit::new(Operation::Off, BEDROCK_COORD)),
            ]
        );

        let mut unresolved = App::new();
        unresolved.inputs.mouse_x = std::f64::consts::PI / DEFAULT_MOUSE_SENSITIVITY;
        unresolved
            .inputs
            .edits
            .push(PendingEdit::Aim(AimEdit::Delete));
        unresolved.fixed_tick();
        assert_eq!(
            unresolved.tick_outcomes.last().unwrap().attempts(),
            [TickAttempt::AimDeleteUnresolved]
        );

        let mut placed = App::new();
        placed.inputs.wheel = -10.0;
        placed.inputs.edits.push(PendingEdit::Aim(AimEdit::Place));
        placed.fixed_tick();
        assert!(matches!(
            placed.tick_outcomes.last().unwrap().attempts(),
            [TickAttempt::AimPlaceExecuted(_)]
        ));
    }

    #[test]
    fn resize_refit_is_consumed_inside_the_tick_and_preserves_camera_ancestry() {
        let mut app = App::new();
        app.inputs.fit_world = true;
        app.fixed_tick();
        let parent = app.tick_outcomes.last().cloned().unwrap();
        let before_resize = app.camera.clone();

        app.resize(PhysicalSize::new(SIDEBAR_WIDTH + 601, 601));

        assert_eq!(app.camera, before_resize);
        assert!(app.inputs.viewport_refit);
        assert!(app.fixed_tick());
        let child = app.tick_outcomes.last().unwrap();
        assert!(child.input().flags().viewport_refit());
        assert_eq!(child.parent(), Some(parent.tag()));
        assert_eq!(child.post_camera(), TickCameraState::capture(&app.camera));
        assert_ne!(child.post_camera(), parent.post_camera());
        verify_successor(Some(&parent), child).unwrap();
        assert!(!app.inputs.viewport_refit);
    }

    #[test]
    #[should_panic(
        expected = "live camera diverged from predecessor tick outcome before the fixed tick"
    )]
    fn fixed_tick_refuses_out_of_fold_camera_mutation() {
        let mut app = App::new();
        app.fixed_tick();

        app.camera.set_observation_distance(2.0);
        app.fixed_tick();
    }

    #[test]
    #[should_panic(
        expected = "live world diverged from coordinate-history identity before the fixed tick"
    )]
    fn fixed_tick_refuses_out_of_fold_world_mutation() {
        let mut app = App::new();
        app.fixed_tick();

        app.world.set_on(bitblock::world::Coord::new(31, -41, 59));
        app.fixed_tick();
    }

    #[test]
    fn maximum_depth_fills_every_pixel_after_camera_rotation() {
        let world = World::new_with_origin();
        let mut camera = Camera::for_single_pixel_voxel(INITIAL_VIEWPORT);
        camera.rotate(0.73, -0.41);
        let bounds = world.bounds().expect("the origin bit has bounds");
        let (min, max) = bounds_aabb(bounds);
        let top = camera.fit_bounds(min, max, INITIAL_VIEWPORT, INITIAL_VIEWPORT);
        camera.dolly_clamped(64.0, DEFAULT_DOLLY_LOG_STEP, BLOCK_FILL_DISTANCE, top);
        let mut renderer = Renderer::new(INITIAL_VIEWPORT, INITIAL_VIEWPORT);

        renderer.render(&world, &camera);

        assert!(renderer.pixels().iter().all(|&pixel| pixel != 0));
    }

    #[test]
    fn high_speed_movement_cannot_tunnel_through_a_block() {
        let obstacle = bitblock::world::Coord::new(0, 0, -995);
        let mut app = App::new();
        app.world.set_on(obstacle);
        app.history = History::new(&app.world);
        app.camera.set_observation_distance(1_000.0);
        app.inputs.forward = true;

        app.fixed_tick();

        assert!(app.camera.position().z < -995.5);
        assert!(observer_is_clear(&app.world, app.camera.position()));
    }

    #[test]
    fn inward_wheel_stops_before_bedrock() {
        let mut app = App::new();
        app.inputs.wheel = 100.0;

        app.fixed_tick();

        assert!(app.camera.position().z < -0.5);
        assert!(observer_is_clear(&app.world, app.camera.position()));
    }

    #[test]
    fn outward_wheel_stops_before_a_block_behind_the_observer() {
        let mut app = App::new();
        app.world.set_on(bitblock::world::Coord::new(0, 0, -2));
        app.history = History::new(&app.world);
        app.inputs.wheel = -100.0;

        app.fixed_tick();

        assert!(app.camera.position().z > -1.5);
        assert!(observer_is_clear(&app.world, app.camera.position()));
    }

    #[test]
    fn an_idle_tick_skips_an_unchanged_frame() {
        let mut app = App::new();

        assert!(app.fixed_tick());
        assert_eq!(
            app.pending_frame.map(|pending| pending.source_tick),
            Some(0)
        );
        assert!(!app.fixed_tick());
        assert_eq!(
            app.pending_frame.map(|pending| pending.source_tick),
            Some(0)
        );
        assert_eq!(app.tick, 2);
    }

    #[test]
    fn failed_submission_cannot_advance_the_frame_ledger() {
        let mut app = App::new();
        assert!(app.fixed_tick());
        let pending = app.pending_frame.unwrap();

        let failed = submit_and_record(&mut app.frame_ledger, pending, || Err("powered failure"));
        assert_eq!(failed, Err(SubmitFrameError::Backend("powered failure")));
        assert!(app.frame_ledger.is_empty());

        let presented =
            submit_and_record(&mut app.frame_ledger, pending, || Ok::<(), &str>(())).unwrap();
        assert_eq!(app.frame_ledger.len(), 1);
        assert_eq!(presented.legacy_receipt.source_tick(), 0);
        assert_eq!(presented.legacy_receipt.input(), pending.input);
        assert_eq!(presented.legacy_receipt.frame(), pending.frame.tag);
    }

    #[test]
    fn failed_composite_submission_advances_neither_frame_chain() {
        let mut app = App::new();
        assert!(app.fixed_tick());
        let game = app.pending_frame.unwrap();
        let output = app.pending_output_frame.unwrap();

        let failed = submit_pair_and_record(
            &mut app.frame_ledger,
            &mut app.output_frame_ledger,
            Some(game),
            output,
            || Err("powered failure"),
        );

        assert_eq!(failed, Err(SubmitFrameError::Backend("powered failure")));
        assert!(app.frame_ledger.is_empty());
        assert!(app.output_frame_ledger.is_empty());
        assert!(app.last_presented_sidebar_facts.is_none());
    }

    #[test]
    fn display_capture_source_requires_the_current_composite_occurrence_not_only_its_pixels() {
        let mut app = App::new();
        assert!(app.fixed_tick());
        let game = app.pending_frame.unwrap();
        let output = app.pending_output_frame.unwrap();
        let first = submit_live_frame_pair(
            &app.tick_outcomes,
            &mut app.frame_ledger,
            &mut app.output_frame_ledger,
            app.frame_ledgers_v2.as_mut().unwrap(),
            Some(game),
            output,
            |_| Ok::<(), ()>(()),
        )
        .unwrap();

        let genesis = app.tick_outcomes[0].clone();
        let [
            simulation_width,
            simulation_height,
            output_width,
            output_height,
        ] = genesis.input().dimensions();
        let repeated_input = TickInputState::new(
            TickInputFlags::new(
                false,
                false,
                false,
                false,
                false,
                false,
                false,
                false,
                true,
                genesis.input().flags().sidebar_visible(),
                false,
            ),
            0.0,
            0.0,
            0.0,
            simulation_width,
            simulation_height,
            output_width,
            output_height,
        )
        .unwrap();
        let repeated_outcome = TickOutcome::new(
            Some(genesis.tag()),
            1,
            genesis.post_world(),
            genesis.post_history(),
            repeated_input,
            [],
            genesis.post_world(),
            genesis.post_history(),
            genesis.post_camera(),
            None,
            genesis.composite(),
        )
        .unwrap();
        verify_successor(Some(&genesis), &repeated_outcome).unwrap();
        let repeated_pending = PendingRenderedFrame {
            parent: app.output_frame_ledger.head(),
            source_tick: repeated_outcome.tick(),
            source_outcome: repeated_outcome.tag(),
            input: output.input,
            frame: output.frame,
        };
        app.tick_outcomes.push(repeated_outcome);
        let second = submit_live_frame_pair(
            &app.tick_outcomes,
            &mut app.frame_ledger,
            &mut app.output_frame_ledger,
            app.frame_ledgers_v2.as_mut().unwrap(),
            None,
            repeated_pending,
            |_| Ok::<(), ()>(()),
        )
        .unwrap();

        assert_eq!(first.output.frame, second.output.frame);
        assert_eq!(
            first.output.receipt_v2.frame(),
            second.output.receipt_v2.frame()
        );
        assert_ne!(
            first.output.receipt_v2.tag(),
            second.output.receipt_v2.tag()
        );
        let ledger = &app.frame_ledgers_v2.as_ref().unwrap().composite;
        assert_eq!(ledger.head(), Some(second.output.receipt_v2.tag()));
        let pixels = app.compositor.pixels();
        assert!(
            current_committed_composite_frame(
                ledger,
                first.output,
                app.compositor.width(),
                app.compositor.height(),
                pixels,
            )
            .is_none()
        );
        assert!(
            current_committed_composite_frame(
                ledger,
                second.output,
                app.compositor.width(),
                app.compositor.height(),
                pixels,
            )
            .is_some()
        );
    }

    #[cfg(all(windows, feature = "protocol-research"))]
    #[test]
    fn f4_arms_only_observational_work_without_mutating_tick_authority() {
        let path = TempDisplayJournal::new("f4-authority");
        let (journal, _) =
            DisplayArtifactJournal::create_new(&path.0, StorageLimits::DEFAULT).unwrap();
        let mut app = App::new();
        assert!(app.fixed_tick());
        app.display_artifact_archive = Some(LiveDisplayArtifactArchive::new(journal));

        let before_world = world_tag(&app.world);
        let before_history = app.history.head();
        let before_history_len = app.history.receipts().len();
        let before_camera = app.camera.clone();
        let before_tick = app.tick;
        let before_outcomes = app.tick_outcomes.clone();
        let before_v1_game = app.frame_ledger.clone();
        let before_v1_output = app.output_frame_ledger.clone();
        let before_v2 = app.frame_ledgers_v2.clone();
        let before_pending_game = app.pending_frame;
        let before_pending_output = app.pending_output_frame;
        let before_pending_sidebar = app.pending_sidebar_facts;
        let before_render_flags = (
            app.needs_render,
            app.needs_sidebar_render,
            app.needs_present,
        );

        app.key(KeyCode::F4, true);

        assert!(app.display_artifact_capture_requested);
        assert_eq!(world_tag(&app.world), before_world);
        assert_eq!(app.history.head(), before_history);
        assert_eq!(app.history.receipts().len(), before_history_len);
        assert_eq!(app.camera, before_camera);
        assert_eq!(app.tick, before_tick);
        assert_eq!(app.tick_outcomes, before_outcomes);
        assert_eq!(app.frame_ledger, before_v1_game);
        assert_eq!(app.output_frame_ledger, before_v1_output);
        assert_eq!(app.frame_ledgers_v2, before_v2);
        assert_eq!(app.pending_frame, before_pending_game);
        assert_eq!(app.pending_output_frame, before_pending_output);
        assert_eq!(app.pending_sidebar_facts, before_pending_sidebar);
        assert_eq!(
            (
                app.needs_render,
                app.needs_sidebar_render,
                app.needs_present,
            ),
            before_render_flags
        );
        let archive = app.display_artifact_archive.as_ref().unwrap();
        assert!(archive.can_capture_new());
        assert_eq!(archive.pending_artifact(), None);
    }

    #[cfg(all(windows, feature = "protocol-research"))]
    #[test]
    fn f5_labels_wait_for_evidence_without_claiming_output_proof() {
        for title in [RESEARCH_WINDOW_TITLE, PRESENTATION_EVIDENCE_WAITING_TITLE] {
            let normalized = title.to_ascii_lowercase();
            assert!(!normalized.contains("proof"));
            assert!(!normalized.contains("scanout"));
            assert!(!normalized.contains("displayed"));
        }
        assert!(RESEARCH_WINDOW_TITLE.contains("presentation evidence"));
        assert!(PRESENTATION_EVIDENCE_WAITING_TITLE.contains("armed"));
        assert!(PRESENTATION_EVIDENCE_WAITING_TITLE.contains("next changed presentation"));
    }

    #[cfg(all(windows, feature = "protocol-research"))]
    #[test]
    fn f5_arms_the_next_presentation_without_forging_tick_or_render_authority() {
        let path = TempDisplayJournal::new("f5-authority");
        let (journal, _) =
            DisplayArtifactJournal::create_new(&path.0, StorageLimits::DEFAULT).unwrap();
        let mut app = App::new();
        assert!(app.fixed_tick());
        app.display_artifact_archive = Some(LiveDisplayArtifactArchive::new(journal));

        let before_world = world_tag(&app.world);
        let before_history = app.history.head();
        let before_history_len = app.history.receipts().len();
        let before_camera = app.camera.clone();
        let before_tick = app.tick;
        let before_outcomes = app.tick_outcomes.clone();
        let before_v1_game = app.frame_ledger.clone();
        let before_v1_output = app.output_frame_ledger.clone();
        let before_v2 = app.frame_ledgers_v2.clone();
        let before_pending_game = app.pending_frame;
        let before_pending_output = app.pending_output_frame;
        let before_pending_sidebar = app.pending_sidebar_facts;
        let before_render_flags = (
            app.needs_render,
            app.needs_sidebar_render,
            app.needs_present,
        );

        app.key(KeyCode::F5, true);

        assert!(app.presentation_output_capture_requested);
        assert!(!app.display_artifact_capture_requested);
        assert_eq!(world_tag(&app.world), before_world);
        assert_eq!(app.history.head(), before_history);
        assert_eq!(app.history.receipts().len(), before_history_len);
        assert_eq!(app.camera, before_camera);
        assert_eq!(app.tick, before_tick);
        assert_eq!(app.tick_outcomes, before_outcomes);
        assert_eq!(app.frame_ledger, before_v1_game);
        assert_eq!(app.output_frame_ledger, before_v1_output);
        assert_eq!(app.frame_ledgers_v2, before_v2);
        assert_eq!(app.pending_frame, before_pending_game);
        assert_eq!(app.pending_output_frame, before_pending_output);
        assert_eq!(app.pending_sidebar_facts, before_pending_sidebar);
        assert_eq!(
            (
                app.needs_render,
                app.needs_sidebar_render,
                app.needs_present,
            ),
            before_render_flags
        );
        let archive = app.display_artifact_archive.as_ref().unwrap();
        assert!(archive.can_capture_new());
        assert_eq!(archive.pending_artifact(), None);
        assert_eq!(archive.pending_presentation_output(), None);
    }

    #[cfg(all(windows, feature = "protocol-research"))]
    #[test]
    fn f5_service_gate_and_observation_order_are_explicit() {
        let path = TempDisplayJournal::new("f5-service-order");
        let (journal, _) =
            DisplayArtifactJournal::create_new(&path.0, StorageLimits::DEFAULT).unwrap();
        let mut archive = LiveDisplayArtifactArchive::new(journal);

        let mut app = App::new();
        app.resize(PhysicalSize::new(3, 2));
        assert!(app.fixed_tick());
        let submitted = submit_live_frame_pair(
            &app.tick_outcomes,
            &mut app.frame_ledger,
            &mut app.output_frame_ledger,
            app.frame_ledgers_v2.as_mut().unwrap(),
            Some(app.pending_frame.unwrap()),
            app.pending_output_frame.unwrap(),
            |_| Ok::<(), ()>(()),
        )
        .unwrap();
        let presented = submitted.output;
        let receipt = presented.receipt_v2;
        let expected = FrameRef::new(
            app.compositor.width(),
            app.compositor.height(),
            app.compositor.pixels(),
        )
        .unwrap();
        let composite_ledger = &app.frame_ledgers_v2.as_ref().unwrap().composite;

        let invalid_pixels = [0_u32];
        let invalid_frame = FrameRef::new(1, 1, &invalid_pixels).unwrap();
        let invalid_calls = std::cell::Cell::new(0_u32);
        let mut requested = true;
        let invalid = service_presentation_output_request_with(
            &mut requested,
            &mut archive,
            composite_ledger,
            presented,
            invalid_frame,
            |_, _, _| {
                invalid_calls.set(invalid_calls.get() + 1);
                Err(LivePresentationOutputFailure::MissingTiming)
            },
        );
        assert!(invalid.is_none());
        assert!(requested);
        assert_eq!(invalid_calls.get(), 0);

        let valid_calls = std::cell::Cell::new(0_u32);
        let consumed = service_presentation_output_request_with(
            &mut requested,
            &mut archive,
            composite_ledger,
            presented,
            expected,
            |_, _, _| {
                valid_calls.set(valid_calls.get() + 1);
                Err(LivePresentationOutputFailure::MissingTiming)
            },
        );
        assert!(matches!(
            consumed,
            Some(Err(LivePresentationOutputFailure::MissingTiming))
        ));
        assert!(!requested);
        assert_eq!(valid_calls.get(), 1);

        let alternate_receipt = FrameReceiptV2::new(
            Some(receipt.tag()),
            receipt.stream(),
            receipt.outcome(),
            receipt.input(),
            receipt.frame(),
        )
        .unwrap();
        let mismatch_events = std::cell::RefCell::new(Vec::new());
        let mismatch = attempt_presentation_output_observation(
            &mut archive,
            presented,
            expected,
            test_timing(alternate_receipt),
            |stage| {
                mismatch_events.borrow_mut().push(stage);
                Ok(test_unavailable_topology(1))
            },
            || {
                mismatch_events.borrow_mut().push("capture");
                test_complete_capture(expected)
            },
        );
        assert!(matches!(
            mismatch,
            Err(LivePresentationOutputFailure::TimingReceiptMismatch { .. })
        ));
        assert!(mismatch_events.borrow().is_empty());

        let pre_events = std::cell::RefCell::new(Vec::new());
        let pre_failure = attempt_presentation_output_observation(
            &mut archive,
            presented,
            expected,
            test_timing(receipt),
            |stage| {
                pre_events.borrow_mut().push(stage);
                Err(LivePresentationOutputFailure::Topology {
                    stage,
                    error: OutputTopologyError::DuplicateRoute,
                })
            },
            || {
                pre_events.borrow_mut().push("capture");
                test_complete_capture(expected)
            },
        );
        assert!(matches!(
            pre_failure,
            Err(LivePresentationOutputFailure::Topology {
                stage: "before-capture",
                ..
            })
        ));
        assert_eq!(*pre_events.borrow(), ["before-capture"]);

        let post_events = std::cell::RefCell::new(Vec::new());
        let post_calls = std::cell::Cell::new(0_u32);
        let post_failure = attempt_presentation_output_observation(
            &mut archive,
            presented,
            expected,
            test_timing(receipt),
            |stage| {
                post_events.borrow_mut().push(stage);
                let call = post_calls.get();
                post_calls.set(call + 1);
                if call == 0 {
                    Ok(test_unavailable_topology(1))
                } else {
                    Err(LivePresentationOutputFailure::Topology {
                        stage,
                        error: OutputTopologyError::DuplicateRoute,
                    })
                }
            },
            || {
                post_events.borrow_mut().push("capture");
                test_complete_capture(expected)
            },
        );
        assert!(matches!(
            post_failure,
            Err(LivePresentationOutputFailure::Topology {
                stage: "after-capture",
                ..
            })
        ));
        assert_eq!(
            *post_events.borrow(),
            ["before-capture", "capture", "after-capture"]
        );
        assert!(archive.journal.state().is_empty());

        let success_events = std::cell::RefCell::new(Vec::new());
        let success_calls = std::cell::Cell::new(0_u32);
        let stored = match attempt_presentation_output_observation(
            &mut archive,
            presented,
            expected,
            test_timing(receipt),
            |stage| {
                success_events.borrow_mut().push(stage);
                let call = success_calls.get();
                success_calls.set(call + 1);
                Ok(test_unavailable_topology(call + 1))
            },
            || {
                success_events.borrow_mut().push("capture");
                test_complete_capture(expected)
            },
        ) {
            Ok(stored) => stored,
            Err(_) => panic!("complete injected presentation observation must store"),
        };
        assert_eq!(
            *success_events.borrow(),
            ["before-capture", "capture", "after-capture"]
        );
        assert_eq!(stored.artifact_count, 1);
        assert_eq!(archive.journal.state().len(), 1);
        assert!(archive.can_capture_new());
    }

    #[cfg(all(windows, feature = "protocol-research"))]
    #[test]
    fn f4_and_f5_requests_are_mutually_exclusive_without_state_conversion() {
        let f5_path = TempDisplayJournal::new("f5-blocks-f4");
        let (f5_journal, _) =
            DisplayArtifactJournal::create_new(&f5_path.0, StorageLimits::DEFAULT).unwrap();
        let mut f5_first = App::new();
        f5_first.display_artifact_archive = Some(LiveDisplayArtifactArchive::new(f5_journal));
        f5_first.key(KeyCode::F5, true);
        f5_first.key(KeyCode::F4, true);
        assert!(f5_first.presentation_output_capture_requested);
        assert!(!f5_first.display_artifact_capture_requested);

        let f4_path = TempDisplayJournal::new("f4-blocks-f5");
        let (f4_journal, _) =
            DisplayArtifactJournal::create_new(&f4_path.0, StorageLimits::DEFAULT).unwrap();
        let mut f4_first = App::new();
        f4_first.display_artifact_archive = Some(LiveDisplayArtifactArchive::new(f4_journal));
        f4_first.key(KeyCode::F4, true);
        f4_first.key(KeyCode::F5, true);
        assert!(f4_first.display_artifact_capture_requested);
        assert!(!f4_first.presentation_output_capture_requested);
    }

    #[cfg(all(windows, feature = "protocol-research"))]
    #[test]
    fn refused_append_retains_one_exact_artifact_for_retry_without_recapture() {
        let path = TempDisplayJournal::new("exact-retry");
        let default_limits = StorageLimits::DEFAULT;
        let zero_record_limits = StorageLimits::new(
            default_limits.max_record_bytes(),
            default_limits.max_schema_bytes(),
            default_limits.max_payload_bytes(),
            0,
            default_limits.max_stream_bytes(),
        )
        .unwrap();
        let (journal, _) = DisplayArtifactJournal::create_new(&path.0, zero_record_limits).unwrap();
        let mut archive = LiveDisplayArtifactArchive::new(journal);

        let mut app = App::new();
        app.resize(PhysicalSize::new(3, 2));
        assert!(app.fixed_tick());
        let game = app.pending_frame.unwrap();
        let output = app.pending_output_frame.unwrap();
        let submitted = submit_live_frame_pair(
            &app.tick_outcomes,
            &mut app.frame_ledger,
            &mut app.output_frame_ledger,
            app.frame_ledgers_v2.as_mut().unwrap(),
            Some(game),
            output,
            |_| Ok::<(), ()>(()),
        )
        .unwrap();
        let expected = FrameRef::new(
            app.compositor.width(),
            app.compositor.height(),
            app.compositor.pixels(),
        )
        .unwrap();
        let rect = bitblock::display::CaptureRect {
            left: 0,
            top: 0,
            right: expected.width() as i32,
            bottom: expected.height() as i32,
        };
        let capture = DisplayCapture {
            facts: bitblock::display::DisplayCaptureFacts {
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
                qpc_end: 120,
                qpc_frequency: 10_000_000,
                captured: Some(expected.observe()),
                mismatch_count: 0,
                first_mismatch: None,
            },
            pixels: Some(expected.pixels().to_vec()),
        };

        let first_failure = archive
            .prepare_and_append(submitted.output.receipt_v2, expected, &capture)
            .unwrap_err();
        assert!(!first_failure.poisoned);
        assert_eq!(first_failure.artifact, archive.pending_artifact());
        let retained = archive.pending_artifact().unwrap();
        drop(capture);

        let retry_failure = archive.append_pending().unwrap_err();
        assert!(!retry_failure.poisoned);
        assert_eq!(retry_failure.artifact, Some(retained));
        assert_eq!(archive.pending_artifact(), Some(retained));
        assert!(!archive.faulted);
        assert_eq!(archive.journal.carrier_valid_bytes(), 0);
    }

    #[cfg(all(windows, feature = "protocol-research"))]
    #[test]
    fn refused_f5_append_retains_the_exact_full_wrapper_for_storage_only_retry() {
        let path = TempDisplayJournal::new("f5-exact-retry");
        let default_limits = StorageLimits::DEFAULT;
        let zero_record_limits = StorageLimits::new(
            default_limits.max_record_bytes(),
            default_limits.max_schema_bytes(),
            default_limits.max_payload_bytes(),
            0,
            default_limits.max_stream_bytes(),
        )
        .unwrap();
        let (journal, _) = DisplayArtifactJournal::create_new(&path.0, zero_record_limits).unwrap();
        let mut archive = LiveDisplayArtifactArchive::new(journal);

        let mut app = App::new();
        app.resize(PhysicalSize::new(3, 2));
        assert!(app.fixed_tick());
        let game = app.pending_frame.unwrap();
        let output = app.pending_output_frame.unwrap();
        let submitted = submit_live_frame_pair(
            &app.tick_outcomes,
            &mut app.frame_ledger,
            &mut app.output_frame_ledger,
            app.frame_ledgers_v2.as_mut().unwrap(),
            Some(game),
            output,
            |_| Ok::<(), ()>(()),
        )
        .unwrap();
        let expected = FrameRef::new(
            app.compositor.width(),
            app.compositor.height(),
            app.compositor.pixels(),
        )
        .unwrap();
        let rect = bitblock::display::CaptureRect {
            left: 0,
            top: 0,
            right: expected.width() as i32,
            bottom: expected.height() as i32,
        };
        let capture = DisplayCapture {
            facts: bitblock::display::DisplayCaptureFacts {
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
                qpc_end: 120,
                qpc_frequency: 10_000_000,
                captured: Some(expected.observe()),
                mismatch_count: 0,
                first_mismatch: None,
            },
            pixels: Some(expected.pixels().to_vec()),
        };
        let receipt = submitted.output.receipt_v2;
        let first_failure = archive
            .prepare_and_append_presentation_output(
                receipt,
                expected,
                &capture,
                test_timing(receipt),
                test_unavailable_topology(1),
                test_unavailable_topology(2),
            )
            .unwrap_err();
        assert!(!first_failure.poisoned);
        assert_eq!(
            first_failure.artifact,
            archive.pending_presentation_output()
        );
        assert_eq!(archive.pending_artifact(), None);
        let retained = archive.pending_presentation_output().unwrap();
        drop(capture);

        let retry_failure = archive.append_pending_presentation_output().unwrap_err();
        assert!(!retry_failure.poisoned);
        assert_eq!(retry_failure.artifact, Some(retained));
        assert_eq!(archive.pending_presentation_output(), Some(retained));
        assert!(!archive.faulted);
        assert_eq!(archive.journal.carrier_valid_bytes(), 0);
    }

    #[test]
    fn f3_recomposes_sidebar_without_forging_a_changed_game_frame() {
        let mut app = App::new();
        assert!(app.fixed_tick());
        let game = app.pending_frame.take().unwrap();
        let original_game_tag = game.frame.tag;
        let output = app.pending_output_frame.take().unwrap();
        let presented = submit_live_frame_pair(
            &app.tick_outcomes,
            &mut app.frame_ledger,
            &mut app.output_frame_ledger,
            app.frame_ledgers_v2.as_mut().unwrap(),
            Some(game),
            output,
            |_| Ok::<(), ()>(()),
        )
        .unwrap();
        app.last_presented_frame = presented.game;
        app.last_presented_output = Some(presented.output);

        app.key(KeyCode::F3, true);
        assert!(!app.needs_render);
        assert!(app.needs_sidebar_render);
        assert!(app.fixed_tick());

        assert!(app.pending_frame.is_none());
        let outcome = app.tick_outcomes.last().unwrap();
        assert!(outcome.input().flags().sidebar_dirty());
        assert_eq!(outcome.game(), None);
        let pending_output = app.pending_output_frame.unwrap();
        assert_eq!(
            outcome.composite(),
            Some(TickFrameCommitment::new(pending_output.input, pending_output.frame.tag).unwrap())
        );
        assert_eq!(
            FrameRef::new(
                app.renderer.width(),
                app.renderer.height(),
                app.renderer.pixels(),
            )
            .unwrap()
            .content_tag(),
            original_game_tag
        );
        let output = app.pending_output_frame.take().unwrap();
        submit_pair_and_record(
            &mut app.frame_ledger,
            &mut app.output_frame_ledger,
            None,
            output,
            || Ok::<(), ()>(()),
        )
        .unwrap();
        assert_eq!(app.frame_ledger.len(), 1);
        assert_eq!(app.output_frame_ledger.len(), 2);
    }

    #[test]
    fn pending_streams_resolve_their_exact_independent_outcome_endpoints() {
        let mut app = App::new();
        assert!(app.fixed_tick());
        let first_game = app.pending_frame.unwrap();
        let first_output = app.pending_output_frame.unwrap();
        assert_eq!(first_game.source_tick, 0);
        assert_eq!(first_output.source_tick, 0);
        verify_pending_frame(&app.tick_outcomes, FrameStream::Game, first_game).unwrap();
        verify_pending_frame(&app.tick_outcomes, FrameStream::Composite, first_output).unwrap();

        app.key(KeyCode::F3, true);
        assert!(app.fixed_tick());
        let retained_game = app.pending_frame.unwrap();
        let newer_output = app.pending_output_frame.unwrap();
        assert_eq!(retained_game, first_game);
        assert_eq!(retained_game.source_outcome, app.tick_outcomes[0].tag());
        assert_eq!(newer_output.source_tick, 1);
        assert_eq!(newer_output.source_outcome, app.tick_outcomes[1].tag());
        verify_pending_frame(&app.tick_outcomes, FrameStream::Game, retained_game).unwrap();
        verify_pending_frame(&app.tick_outcomes, FrameStream::Composite, newer_output).unwrap();

        let forged_outcome = PendingRenderedFrame {
            source_outcome: app.tick_outcomes[1].tag(),
            ..retained_game
        };
        assert!(matches!(
            verify_pending_frame(&app.tick_outcomes, FrameStream::Game, forged_outcome),
            Err(PendingFrameProtocolError::InvalidOutcomeBinding(
                FrameReceiptV2AdmissionError::OutcomeMismatch { .. }
            ))
        ));

        let missing_tick = PendingRenderedFrame {
            source_tick: 99,
            ..retained_game
        };
        assert_eq!(
            verify_pending_frame(&app.tick_outcomes, FrameStream::Game, missing_tick),
            Err(PendingFrameProtocolError::MissingSourceTick { tick: 99 })
        );
        assert!(
            verify_pending_frame(&app.tick_outcomes, FrameStream::Composite, retained_game,)
                .is_err()
        );
    }

    #[test]
    fn live_v2_pair_submission_is_atomic_across_backend_refusal_and_retry() {
        let mut app = App::new();
        assert!(app.fixed_tick());
        let game = app.pending_frame.unwrap();
        let output = app.pending_output_frame.unwrap();
        let before_game_v1 = app.frame_ledger.clone();
        let before_output_v1 = app.output_frame_ledger.clone();
        let before_v2 = app.frame_ledgers_v2.clone().unwrap();
        let before_outcomes = app.tick_outcomes.clone();
        let mut backend_calls = 0;

        let refused = submit_live_frame_pair(
            &app.tick_outcomes,
            &mut app.frame_ledger,
            &mut app.output_frame_ledger,
            app.frame_ledgers_v2.as_mut().unwrap(),
            Some(game),
            output,
            |_| {
                backend_calls += 1;
                Err("powered backend refusal")
            },
        );
        assert_eq!(
            refused,
            Err(LiveFrameSubmissionError::V2(
                FramePairAdmissionV2Error::Backend("powered backend refusal")
            ))
        );
        assert_eq!(backend_calls, 1);
        assert_eq!(app.frame_ledger, before_game_v1);
        assert_eq!(app.output_frame_ledger, before_output_v1);
        assert_eq!(app.frame_ledgers_v2.as_ref().unwrap(), &before_v2);
        assert_eq!(app.tick_outcomes, before_outcomes);
        assert_eq!(app.pending_frame, Some(game));
        assert_eq!(app.pending_output_frame, Some(output));

        let submitted = submit_live_frame_pair(
            &app.tick_outcomes,
            &mut app.frame_ledger,
            &mut app.output_frame_ledger,
            app.frame_ledgers_v2.as_mut().unwrap(),
            Some(game),
            output,
            |receipts| {
                backend_calls += 1;
                assert_eq!(receipts.game().unwrap().outcome(), game.source_outcome);
                assert_eq!(receipts.composite().outcome(), output.source_outcome);
                Ok::<(), &'static str>(())
            },
        )
        .unwrap();
        assert_eq!(backend_calls, 2);
        assert_eq!(app.frame_ledger.len(), 1);
        assert_eq!(app.output_frame_ledger.len(), 1);
        let ledgers_v2 = app.frame_ledgers_v2.as_ref().unwrap();
        assert_eq!(ledgers_v2.game.len(), 1);
        assert_eq!(ledgers_v2.composite.len(), 1);
        assert_eq!(
            ledgers_v2.game.head(),
            submitted.game.map(|presented| presented.receipt_v2.tag())
        );
        assert_eq!(
            ledgers_v2.composite.head(),
            Some(submitted.output.receipt_v2.tag())
        );
        assert_eq!(
            submitted.game.unwrap().legacy_receipt.tag(),
            app.frame_ledger.head().unwrap()
        );
        assert_eq!(
            submitted.output.legacy_receipt.tag(),
            app.output_frame_ledger.head().unwrap()
        );
        assert_eq!(ledgers_v2.game.verify(), Ok(()));
        assert_eq!(ledgers_v2.composite.verify(), Ok(()));
    }

    #[test]
    fn live_v2_streams_follow_independent_f3_endpoints_and_catch_up_paths() {
        let mut app = App::new();
        assert!(app.fixed_tick());
        app.key(KeyCode::F3, true);
        assert!(app.fixed_tick());
        let game_zero = app.pending_frame.unwrap();
        let composite_one = app.pending_output_frame.unwrap();
        assert_eq!(game_zero.source_tick, 0);
        assert_eq!(composite_one.source_tick, 1);

        let first_submission = submit_live_frame_pair(
            &app.tick_outcomes,
            &mut app.frame_ledger,
            &mut app.output_frame_ledger,
            app.frame_ledgers_v2.as_mut().unwrap(),
            Some(game_zero),
            composite_one,
            |_| Ok::<(), ()>(()),
        )
        .unwrap();
        assert_eq!(
            first_submission.game.unwrap().receipt_v2.outcome(),
            game_zero.source_outcome
        );
        assert_eq!(
            first_submission.output.receipt_v2.outcome(),
            composite_one.source_outcome
        );
        assert_eq!(
            first_submission.output.receipt_v2.stream(),
            FrameStream::Composite
        );
        let ledgers_v2 = app.frame_ledgers_v2.as_ref().unwrap();
        assert_eq!(ledgers_v2.game.outcome_head().tick(), 0);
        assert_eq!(ledgers_v2.composite.outcome_head().tick(), 1);
        assert!(ledgers_v2.game.admissions()[0].successors().is_empty());
        assert_eq!(ledgers_v2.composite.admissions()[0].successors().len(), 1);

        app.pending_frame = None;
        app.pending_output_frame = None;
        app.inputs.mouse_x = 3.0;
        assert!(app.fixed_tick());
        let game_two = app.pending_frame.unwrap();
        let composite_two = app.pending_output_frame.unwrap();
        assert_eq!(game_two.source_tick, 2);
        assert_eq!(composite_two.source_tick, 2);
        submit_live_frame_pair(
            &app.tick_outcomes,
            &mut app.frame_ledger,
            &mut app.output_frame_ledger,
            app.frame_ledgers_v2.as_mut().unwrap(),
            Some(game_two),
            composite_two,
            |_| Ok::<(), ()>(()),
        )
        .unwrap();

        let ledgers_v2 = app.frame_ledgers_v2.as_ref().unwrap();
        assert_eq!(ledgers_v2.game.outcome_head().tick(), 2);
        assert_eq!(ledgers_v2.composite.outcome_head().tick(), 2);
        assert_eq!(ledgers_v2.game.admissions()[1].successors().len(), 2);
        assert_eq!(ledgers_v2.composite.admissions()[1].successors().len(), 1);
        assert_eq!(ledgers_v2.game.verify(), Ok(()));
        assert_eq!(ledgers_v2.composite.verify(), Ok(()));
    }

    #[test]
    fn uncaptured_sidebar_wheel_isolated_from_observer_input() {
        let mut app = App::new();
        app.mouse_captured = false;
        app.cursor_position = Some(PhysicalPosition::new(
            f64::from(app.compositor.simulation_width()) + 10.0,
            20.0,
        ));
        assert!(app.pointer_in_sidebar());

        app.queue_wheel(7.0);

        assert_eq!(app.inputs.wheel, 0.0);
        app.cursor_position = Some(PhysicalPosition::new(20.0, 20.0));
        assert!(!app.pointer_in_sidebar());
        app.queue_wheel(7.0);
        assert_eq!(app.inputs.wheel, 7.0);
    }

    #[test]
    fn dirty_renders_coalesce_to_the_final_unsubmitted_frame() {
        let mut app = App::new();
        assert!(app.fixed_tick());
        let first = app.pending_frame.unwrap();
        app.inputs.mouse_x = 3.0;
        assert!(app.fixed_tick());
        let final_pending = app.pending_frame.unwrap();
        assert_ne!(first.input, final_pending.input);
        assert_eq!(final_pending.source_tick, 1);
        assert!(app.frame_ledger.is_empty());

        submit_and_record(&mut app.frame_ledger, final_pending, || Ok::<(), ()>(())).unwrap();
        assert_eq!(app.frame_ledger.len(), 1);
        assert_eq!(app.frame_ledger.receipts()[0].source_tick(), 1);
    }

    #[test]
    fn idle_gap_adds_no_receipts_and_next_dirty_frame_links_to_predecessor() {
        let mut app = App::new();
        assert!(app.fixed_tick());
        let first_pending = app.pending_frame.take().unwrap();
        let first =
            submit_and_record(&mut app.frame_ledger, first_pending, || Ok::<(), ()>(())).unwrap();

        for _ in 0..3 {
            assert!(!app.fixed_tick());
        }
        assert_eq!(app.frame_ledger.len(), 1);

        app.inputs.mouse_y = 2.0;
        assert!(app.fixed_tick());
        let second_pending = app.pending_frame.take().unwrap();
        assert_eq!(second_pending.parent, Some(first.legacy_receipt.tag()));
        assert_eq!(second_pending.source_tick, 4);
        let second =
            submit_and_record(&mut app.frame_ledger, second_pending, || Ok::<(), ()>(())).unwrap();
        assert_eq!(app.frame_ledger.len(), 2);
        assert_eq!(
            second.legacy_receipt.parent(),
            Some(first.legacy_receipt.tag())
        );
        assert_eq!(second.legacy_receipt.source_tick(), 4);
        assert!(app.frame_ledger.verify().is_ok());
        assert_eq!(app.tick_outcomes.len(), 5);
        verify_successor(None, &app.tick_outcomes[0]).unwrap();
        for pair in app.tick_outcomes.windows(2) {
            verify_successor(Some(&pair[0]), &pair[1]).unwrap();
        }
    }

    #[test]
    fn parsed_interaction_tape_drives_the_same_fixed_fold_as_accepted_actions() {
        let tape = InteractionTape::parse(
            "\
tick
move forward down
look 1.5 -0.25
wheel -3
tick
move forward up
place
sidebar
tick
exact on 7 -8 9
fit
",
        )
        .unwrap();
        let summary = simulate_interaction_tape(&tape).unwrap();

        let mut direct = App::for_headless_simulation();
        let mut direct_tags = Vec::new();
        for record in tape.records() {
            for &action in record.actions() {
                direct.apply_accepted_action(action).unwrap();
            }
            direct.fixed_tick();
            direct_tags.push(direct.tick_outcomes.last().unwrap().tag());
        }

        assert_eq!(
            summary
                .ticks
                .iter()
                .map(|tick| tick.outcome)
                .collect::<Vec<_>>(),
            direct_tags
        );
        assert_eq!(summary.final_world, world_tag(&direct.world));
        assert_eq!(summary.final_history, direct.history.head());
        assert_eq!(
            summary.final_camera_bits,
            TickCameraState::capture(&direct.camera).bits()
        );
    }

    #[test]
    fn physical_w_key_adapter_and_normalized_tape_map_to_the_same_action() {
        let mut live_adapter = App::for_headless_simulation();
        live_adapter.key(KeyCode::KeyW, true);
        live_adapter.fixed_tick();

        let tape = InteractionTape::parse("tick\nmove forward down\n").unwrap();
        let simulated = simulate_interaction_tape(&tape).unwrap();

        assert_eq!(simulated.ticks.len(), 1);
        assert_eq!(
            simulated.ticks[0].outcome,
            live_adapter.tick_outcomes[0].tag()
        );
        assert_eq!(
            simulated.final_camera_bits,
            TickCameraState::capture(&live_adapter.camera).bits()
        );
    }

    #[test]
    fn tape_preserves_per_event_wheel_clamping_instead_of_summing_first() {
        let ordered = InteractionTape::parse("tick\nwheel 100\nwheel -100\n").unwrap();
        let collapsed = InteractionTape::parse("tick\nwheel 0\n").unwrap();

        let ordered = simulate_interaction_tape(&ordered).unwrap();
        let collapsed = simulate_interaction_tape(&collapsed).unwrap();

        assert_ne!(ordered.ticks[0].outcome, collapsed.ticks[0].outcome);
        assert_ne!(ordered.final_camera_bits, collapsed.final_camera_bits);
    }

    #[test]
    fn headless_tape_emits_tick_and_frame_commitments_but_no_frame_receipts() {
        let tape = InteractionTape::parse("tick\ntick\nsidebar\n").unwrap();
        let summary = simulate_interaction_tape(&tape).unwrap();

        assert_eq!(summary.ticks.len(), 2);
        assert!(summary.ticks[0].game_frame.is_some());
        assert!(summary.ticks[0].composite_frame.is_some());
        assert!(summary.ticks[1].game_frame.is_none());
        assert!(summary.ticks[1].composite_frame.is_some());
        assert_eq!(summary.game_admissions, 0);
        assert_eq!(summary.composite_admissions, 0);
        assert_eq!(summary.game_receipt_head, None);
        assert_eq!(summary.composite_receipt_head, None);
    }

    #[test]
    fn headless_resize_policy_refuses_before_allocating_the_requested_surface() {
        let tape = InteractionTape::parse("tick\nresize 4294967295 4294967295\n").unwrap();
        let error = simulate_interaction_tape(&tape).unwrap_err();

        assert_eq!(error.kind(), io::ErrorKind::InvalidData);
        assert!(error.to_string().contains("policy limit"));
    }

    #[test]
    fn headless_display_predecessor_is_platform_neutral_and_overrides_live_audit_state() {
        let baseline = App::for_headless_simulation();
        assert_eq!(
            baseline.sidebar_display_predecessor(),
            (None, 0, DisplayStatus::AuditDisabled)
        );

        #[cfg(windows)]
        {
            let mut audit_enabled = App::for_headless_simulation();
            audit_enabled.display_ledger = Some(DisplayLedgerV2::default());
            assert_eq!(
                baseline.sidebar_snapshot(0).tag(),
                audit_enabled.sidebar_snapshot(0).tag()
            );
        }
    }

    #[test]
    fn headless_preflight_refuses_finite_look_values_whose_sum_is_nonfinite() {
        let tape = InteractionTape::parse(
            "tick\nlook 1.7976931348623157e308 0\nlook 1.7976931348623157e308 0\n",
        )
        .unwrap();
        let error = simulate_interaction_tape(&tape).unwrap_err();

        assert_eq!(error.kind(), io::ErrorKind::InvalidData);
        assert!(error.to_string().contains("action 1"));
        assert!(error.to_string().contains("accumulation is nonfinite"));
    }

    #[test]
    fn headless_preflight_preserves_the_full_idle_record_allowance() {
        let tape = InteractionTape::parse(&"tick\n".repeat(16_384)).unwrap();
        preflight_headless_tape(&tape).unwrap();

        let toggles = format!("tick\n{}", "exact toggle 1 0 0\n".repeat(140));
        let toggles = InteractionTape::parse(&toggles).unwrap();
        preflight_headless_tape(&toggles).unwrap();
    }

    #[test]
    fn headless_preflight_charges_each_resize_even_when_the_tape_shrinks_before_tick() {
        let source = format!("tick\n{}resize 1 1\n", "resize 4096 4096\n".repeat(6));
        let tape = InteractionTape::parse(&source).unwrap();
        let error = preflight_headless_tape(&tape).unwrap_err();

        assert!(error.to_string().contains("resize-pixel-write"));
    }

    #[test]
    fn headless_preflight_refuses_the_possible_chunk_boundary_directly() {
        let source = format!("tick\n{}", "place\n".repeat(65_536));
        let tape = InteractionTape::parse(&source).unwrap();
        let error = preflight_headless_tape(&tape).unwrap_err();

        assert!(error.to_string().contains("possible-chunk-count"));
        assert!(error.to_string().contains("required 65537, limit 65536"));
    }

    #[test]
    fn headless_preflight_bounds_composition_raster_and_world_scan_lanes() {
        let composition_source =
            format!("tick\nresize 4096 4096\n{}", "tick\nsidebar\n".repeat(16));
        let composition = InteractionTape::parse(&composition_source).unwrap();
        assert!(
            preflight_headless_tape(&composition)
                .unwrap_err()
                .to_string()
                .contains("output-pixel-tick")
        );

        let raster_source = "tick\nresize 4096 4096\nexact on 1 0 0\nexact on 2 0 0\nexact on 3 0 0\nexact on 4 0 0\nexact on 5 0 0\n";
        let raster = InteractionTape::parse(raster_source).unwrap();
        assert!(
            preflight_headless_tape(&raster)
                .unwrap_err()
                .to_string()
                .contains("raster-sample-test")
        );

        let exact_coordinates = (1..=5_000)
            .map(|x| format!("exact on {x} 0 0\n"))
            .collect::<Vec<_>>()
            .concat();
        let world_scan_source = format!(
            "tick\nresize 1 1\n{}{}",
            exact_coordinates,
            "delete\n".repeat(60_000)
        );
        let world_scan = InteractionTape::parse(&world_scan_source).unwrap();
        assert!(
            preflight_headless_tape(&world_scan)
                .unwrap_err()
                .to_string()
                .contains("world-coordinate-test")
        );

        let packed_coord = |index: i64| {
            let chunk = index.div_euclid(16 * 16 * 16);
            let local = index.rem_euclid(16 * 16 * 16);
            (
                chunk * CHUNK_SIDE + local.rem_euclid(CHUNK_SIDE),
                local.div_euclid(CHUNK_SIDE).rem_euclid(CHUNK_SIDE),
                local.div_euclid(CHUNK_SIDE * CHUNK_SIDE),
            )
        };
        let ascending = (1_i64..=25_000)
            .map(|index| {
                let (x, y, z) = packed_coord(index);
                format!("exact on {x} {y} {z}\n")
            })
            .collect::<Vec<_>>()
            .concat();
        let descending = (1_i64..=25_000)
            .rev()
            .map(|index| {
                let (x, y, z) = packed_coord(index);
                format!("exact off {x} {y} {z}\n")
            })
            .collect::<Vec<_>>()
            .concat();
        let edit_scan_source = format!("tick\nresize 1 1\n{ascending}{descending}");
        let edit_scan = InteractionTape::parse(&edit_scan_source).unwrap();
        assert!(
            preflight_headless_tape(&edit_scan)
                .unwrap_err()
                .to_string()
                .contains("edit-coordinate-test")
        );

        let ordering_coordinates = (1..=10_000)
            .map(|x| format!("exact on {x} 0 0\n"))
            .collect::<Vec<_>>()
            .concat();
        let ordering_source = format!(
            "tick\nresize 1 1\n{ordering_coordinates}{}",
            "tick\n".repeat(4_000)
        );
        let ordering = InteractionTape::parse(&ordering_source).unwrap();
        assert!(
            preflight_headless_tape(&ordering)
                .unwrap_err()
                .to_string()
                .contains("world-ordering")
        );

        let sparse_chunks = (1..=10_000)
            .map(|x| format!("exact on {} 0 0\n", i64::from(x) * CHUNK_SIDE))
            .collect::<Vec<_>>()
            .concat();
        let chunk_scan_source = format!(
            "tick\nresize 1 1\n{sparse_chunks}{}",
            "tick\n".repeat(1_000)
        );
        let chunk_scan = InteractionTape::parse(&chunk_scan_source).unwrap();
        assert!(
            preflight_headless_tape(&chunk_scan)
                .unwrap_err()
                .to_string()
                .contains("chunk-word-test")
        );
    }
}
