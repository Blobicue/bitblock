//! The `bitblock` binary: the interactive observer window and the headless CLI.
//!
//! With no arguments it opens the observer window (the GDI/DWM presentation and audit paths
//! are Windows-only). `--simulate-actions <tape>` replays an interaction tape without a window
//! and prints one content-tagged line per tick. The protocol logic lives in `bitblock-core`
//! and `bitblock-research`; this file is wiring, argument parsing and platform presentation.

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
#[cfg(feature = "protocol-research")]
use std::sync::OnceLock;
use std::time::{Duration, Instant};

use bitblock::camera::{Camera, DEFAULT_DOLLY_LOG_STEP, DEFAULT_MOUSE_SENSITIVITY, Vec3};
#[cfg(windows)]
use bitblock::display::{CaptureStage, DisplayCapture, DisplayLedgerV2, DisplayOutcome};
#[cfg(all(windows, feature = "protocol-research"))]
use bitblock::display_store::{
    DisplayArtifactAppendReceipt, DisplayArtifactJournal, DisplayArtifactStoreError,
    PresentationOutputAppendReceipt, ReadOnlyDisplayArtifactInspection,
    inspect_display_artifact_bytes,
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
#[cfg(feature = "protocol-research")]
use bitblock::interaction::{
    MAX_INTERACTION_ACTIONS_PER_RECORD, MAX_INTERACTION_TAPE_RECORDS, MAX_INTERACTION_TOTAL_ACTIONS,
};
#[cfg(all(windows, feature = "protocol-research"))]
use bitblock::journal::AnchorPolicy;
#[cfg(all(test, windows, feature = "protocol-research"))]
use bitblock::output_topology::{OutputTopologyError, OutputTopologyObservation};
use bitblock::program::{Edit as WorldEdit, Operation, Program, TickBatch};
use bitblock::protocol::ContentTag;
#[cfg(feature = "protocol-research")]
use bitblock::protocol::{TagKind, hash_canonical};
#[cfg(feature = "protocol-research")]
use bitblock::render::crosshair_policy_tag;
use bitblock::render::{
    Renderer, bounds_aabb, coord_containing, crosshair_overlay_input, raycast,
    renderer_profile_tag, sweep_point,
};
#[cfg(feature = "protocol-research")]
use bitblock::replay_evidence::{
    LocalReplayBuild, LocalReplayContext, LocalReplayEndpoint, LocalReplayEvidence,
    ReplayFrameCommitment,
};
#[cfg(feature = "protocol-research")]
use bitblock::sidebar::composite_policy_tag;
use bitblock::sidebar::{
    CompositeFrameInput, DisplayStatus, FrameCompositor, SIDEBAR_WIDTH, SidebarRasterFacts,
    SidebarSnapshot, composite_overlay_input,
};
#[cfg(test)]
use bitblock::tick::{
    LANE_CLEAR_MOTION, LANE_LOOK_DELTA, LANE_MOVE_EDGE, LANE_RESIZE, LANE_TOGGLE_SIDEBAR,
};
use bitblock::tick::{
    LANE_EXACT_EDIT, MAX_ACTIONS_PER_TICK, TickActionError, TickActionLedger, TickAttempt,
    TickCameraState, TickFrameCommitment, TickInputFlags, TickInputState, TickOutcome,
    verify_successor,
};
#[cfg(windows)]
use bitblock::windows_present::WindowsPixelSurface;
#[cfg(all(windows, feature = "protocol-research"))]
use bitblock::windows_timing::observe_checked_present;
#[cfg(all(windows, feature = "protocol-research"))]
use bitblock::windows_topology::{
    ActiveOutputObservationError, ActiveOutputWithColor, observe_active_output_with_color,
};
use bitblock::world::{BEDROCK_COORD, CHUNK_SIDE, CHUNK_WORDS, Coord, World};
#[cfg(all(windows, feature = "protocol-research"))]
use bitblock::{display_artifact::DisplayCaptureArtifact, storage::StorageLimits};
#[cfg(all(windows, feature = "protocol-research"))]
use bitblock::{
    presentation::{CompositionTimingEnvelope, PresentationProtocolError},
    presentation_output::PresentationOutputArtifact,
    presentation_output_v2::PresentationOutputArtifactV2,
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
#[cfg(feature = "protocol-research")]
const HEADLESS_LOCAL_REPLAY_MAX_EXECUTABLE_BYTES: u64 = 64 * 1024 * 1024;
#[cfg(feature = "protocol-research")]
const HEADLESS_LOCAL_REPLAY_GENESIS_SCHEMA: &[u8] = b"bitblock/local-replay-genesis/headless-v1";
#[cfg(feature = "protocol-research")]
const HEADLESS_LOCAL_REPLAY_RULES_SCHEMA: &[u8] = b"bitblock/local-replay-rules/headless-v1";
#[cfg(feature = "protocol-research")]
const HEADLESS_LOCAL_REPLAY_ADMISSION_SCHEMA: &[u8] =
    b"bitblock/local-replay-admission-policy/headless-v1";
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
            #[cfg(feature = "protocol-research")]
            {
                let (summary, evidence) =
                    simulate_interaction_tape_with_local_replay_evidence(&tape)?;
                print_headless_summary(&summary);
                print_local_replay_evidence(&evidence);
            }
            #[cfg(not(feature = "protocol-research"))]
            {
                let summary = simulate_interaction_tape(&tape)?;
                print_headless_summary(&summary);
            }
        }
        #[cfg(all(windows, feature = "protocol-research"))]
        StartupMode::VerifyDisplayJournal {
            path,
            required_ancestor,
        } => verify_display_journal(&path, required_ancestor)?,
    }
    Ok(())
}

#[path = "app/audit.rs"]
mod audit;
#[path = "app/cli.rs"]
mod cli;
#[path = "app/frame_sync.rs"]
mod frame_sync;
#[path = "app/headless.rs"]
mod headless;
#[path = "app/live.rs"]
mod live;

use self::audit::*;
use self::cli::*;
use self::frame_sync::*;
use self::headless::*;
use self::live::*;

#[cfg(feature = "protocol-research")]
fn local_replay_policy_tag(schema: &[u8], payload: &[u8]) -> ContentTag {
    hash_canonical(TagKind::Policy, schema, payload.len() as u64, |sink| {
        sink(payload)
    })
    .expect("local replay policy payload length is measured from its exact encoding")
}

/// Declares the deterministic initial state of the fixed headless fold.
///
/// The executable fingerprint in the surrounding context is still required: this policy tag is
/// a transparent state declaration, not a hash of the implementation that obeyed it.
#[cfg(feature = "protocol-research")]
fn headless_local_replay_genesis_policy_tag() -> ContentTag {
    let app = App::for_headless_simulation();
    let mut payload = Vec::with_capacity(192);
    payload.extend_from_slice(&1_u16.to_be_bytes());
    payload.extend_from_slice(&world_tag(&app.world).digest());
    payload.extend_from_slice(&app.history.genesis().digest());
    for bits in TickCameraState::capture(&app.camera).bits() {
        payload.extend_from_slice(&bits.to_be_bytes());
    }
    payload.extend_from_slice(&app.renderer.width().to_be_bytes());
    payload.extend_from_slice(&app.renderer.height().to_be_bytes());
    payload.extend_from_slice(&app.compositor.width().to_be_bytes());
    payload.extend_from_slice(&app.compositor.height().to_be_bytes());
    payload.extend_from_slice(&app.compositor.simulation_width().to_be_bytes());
    payload.extend_from_slice(&app.tick.to_be_bytes());
    payload.push(u8::from(app.sidebar_visible));
    payload.push(u8::from(app.needs_render));
    payload.push(u8::from(app.needs_sidebar_render));
    payload.push(u8::from(app.needs_present));
    payload.push(u8::from(app.inputs.edits.is_empty()));
    payload.push(
        app.display_predecessor_override
            .expect("headless simulation has an explicit display-audit-disabled predecessor")
            as u8,
    );
    local_replay_policy_tag(HEADLESS_LOCAL_REPLAY_GENESIS_SCHEMA, &payload)
}

/// Declares the fixed-tick and renderer/compositor policies used by one headless replay.
#[cfg(feature = "protocol-research")]
fn headless_local_replay_rules_policy_tag() -> ContentTag {
    let mut payload = Vec::with_capacity(192);
    payload.extend_from_slice(&1_u16.to_be_bytes());
    payload.extend_from_slice(&TICK_RATE.to_bits().to_be_bytes());
    payload.extend_from_slice(&(TICK_DURATION.as_nanos()).to_be_bytes());
    payload.extend_from_slice(&DEFAULT_DOLLY_LOG_STEP.to_bits().to_be_bytes());
    payload.extend_from_slice(&DEFAULT_MOUSE_SENSITIVITY.to_bits().to_be_bytes());
    payload.extend_from_slice(&BLOCK_FILL_DISTANCE.to_bits().to_be_bytes());
    payload.extend_from_slice(&OBSERVER_COLLISION_CLEARANCE.to_bits().to_be_bytes());
    payload.extend_from_slice(&renderer_profile_tag().digest());
    payload.extend_from_slice(&crosshair_policy_tag().digest());
    payload.extend_from_slice(&composite_policy_tag().digest());
    local_replay_policy_tag(HEADLESS_LOCAL_REPLAY_RULES_SCHEMA, &payload)
}

/// Declares the bounded input and work-admission policy before a tape is replayed.
#[cfg(feature = "protocol-research")]
fn headless_local_replay_admission_policy_tag() -> ContentTag {
    let mut payload = Vec::with_capacity(256);
    payload.extend_from_slice(&1_u16.to_be_bytes());
    for value in [
        MAX_INTERACTION_TAPE_PAYLOAD_BYTES as u64,
        MAX_INTERACTION_TAPE_RECORDS as u64,
        MAX_INTERACTION_ACTIONS_PER_RECORD as u64,
        MAX_INTERACTION_TOTAL_ACTIONS as u64,
        HEADLESS_SIMULATION_MAX_SOURCE_BYTES as u64,
        HEADLESS_LOCAL_REPLAY_MAX_EXECUTABLE_BYTES,
        HEADLESS_SIMULATION_MAX_OUTPUT_PIXELS,
        HEADLESS_RENDER_TRIANGLES_PER_BIT_BOUND as u64,
    ] {
        payload.extend_from_slice(&value.to_be_bytes());
    }
    for value in [
        HEADLESS_SIMULATION_MAX_RESIZE_PIXEL_WRITES,
        HEADLESS_SIMULATION_MAX_OUTPUT_PIXEL_TICKS,
        HEADLESS_SIMULATION_MAX_RASTER_SAMPLE_TESTS,
        HEADLESS_SIMULATION_MAX_WORLD_COORDINATE_TESTS,
        HEADLESS_SIMULATION_MAX_EDIT_COORDINATE_TESTS,
        HEADLESS_SIMULATION_MAX_CHUNK_WORD_TESTS,
        HEADLESS_SIMULATION_MAX_WORLD_ORDERING_UNITS,
        HEADLESS_SIMULATION_MAX_POSSIBLE_CHUNKS,
    ] {
        payload.extend_from_slice(&value.to_be_bytes());
    }
    local_replay_policy_tag(HEADLESS_LOCAL_REPLAY_ADMISSION_SCHEMA, &payload)
}

#[cfg(test)]
mod tests {
    /// The three headless local-replay policy tags, pinned.
    ///
    /// These declare the deterministic initial state, the fold rules, and the admission
    /// policy of the headless replay. They are the only canonical encoders in this file,
    /// and until this test existed nothing asserted their values: a byte-order flip across
    /// all eighteen encoding sites in main.rs left the whole suite green. Every other
    /// module in the crate failed that same mutation.
    ///
    /// A change here is not necessarily wrong, but it is never incidental -- it means the
    /// declared headless starting state or its rules moved, and every replay taken under
    /// the old tags describes a different fold.
    ///
    /// Until `tools/headless_replay_policy_vectors.py` existed this was still only half a
    /// guard: the literals below came from the same Rust encoders they check, so a change
    /// consistent across both would have passed. That tool now rebuilds all three payloads
    /// from the declared constants -- one bedrock coordinate, the observer-spawn camera, and
    /// the renderer/crosshair/sidebar policies recomputed from their own constants -- and
    /// reaches these exact three tags without compiling any Rust. A disagreement now means
    /// the two implementations parted, and the tool's controls say on which field.
    #[cfg(feature = "protocol-research")]
    #[test]
    fn headless_local_replay_policy_tags_are_pinned() {
        assert_eq!(
            headless_local_replay_genesis_policy_tag().to_string(),
            "#bb.policy.sha256.sgtpi7mozzgscwu7naqghepq37pko4lrz4ka63kdhw5qmty3fnrq"
        );
        assert_eq!(
            headless_local_replay_rules_policy_tag().to_string(),
            "#bb.policy.sha256.abg53isb3svtuxvxo2yq6mymbwnkv4rkwfzpg2euenjj3ug57yxq"
        );
        assert_eq!(
            headless_local_replay_admission_policy_tag().to_string(),
            "#bb.policy.sha256.f5mg62hs64szq5f4xumd76tpjd5dcmtxbqufwn2jnoxsmylzbrga"
        );
    }

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
    fn test_output_with_color(profile: u32) -> ActiveOutputWithColor {
        let topology = test_unavailable_topology(profile);
        let color = bitblock::output_color::OutputColorObservation::new_unavailable(
            &topology,
            0x4354_4f50,
            1168,
        )
        .unwrap();
        ActiveOutputWithColor::new(topology, color).unwrap()
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

    fn exact_world_run(
        simulation_width: u32,
        simulation_height: u32,
        edits: [WorldEdit; 3],
    ) -> App {
        let mut app = App::for_headless_simulation();
        app.apply_infallible_accepted_action(AcceptedAction::Resize(
            simulation_width + SIDEBAR_WIDTH,
            simulation_height,
        ));
        for edit in edits {
            app.apply_infallible_accepted_action(AcceptedAction::ExactEdit(edit));
        }
        app.fixed_tick();
        app
    }

    #[test]
    fn same_admitted_exact_edits_preserve_world_history_across_resize_contexts() {
        let edits = [
            WorldEdit::new(Operation::On, Coord::new(7, -8, 9)),
            WorldEdit::new(Operation::On, Coord::new(-4, 5, 6)),
            WorldEdit::new(Operation::Toggle, Coord::new(2, 3, -11)),
        ];
        let small = exact_world_run(640, 480, edits);
        let large = exact_world_run(1280, 960, edits);

        assert_ne!(
            small.tick_outcomes[0].input().dimensions(),
            large.tick_outcomes[0].input().dimensions(),
            "the two interaction contexts must actually use different Resize payloads"
        );
        for app in [&small, &large] {
            assert_eq!(
                app.tick_outcomes[0].attempts(),
                edits.map(TickAttempt::ExactExecuted)
            );
            let batch = app.last_tick_batch.as_ref().unwrap();
            assert_eq!(batch.tick(), 0);
            assert_eq!(batch.resolved_edits(), edits);
        }

        assert_eq!(small.history.receipts(), large.history.receipts());
        assert_eq!(small.history.current_world(), large.history.current_world());
        assert_eq!(small.history.head(), large.history.head());
    }

    #[test]
    fn moving_one_executed_on_edit_changes_this_world_and_history() {
        let baseline_edits = [
            WorldEdit::new(Operation::On, Coord::new(7, -8, 9)),
            WorldEdit::new(Operation::On, Coord::new(-4, 5, 6)),
            WorldEdit::new(Operation::Toggle, Coord::new(2, 3, -11)),
        ];
        let moved_edits = [
            WorldEdit::new(Operation::On, Coord::new(7, -8, 10)),
            baseline_edits[1],
            baseline_edits[2],
        ];
        let baseline = exact_world_run(801, 801, baseline_edits);
        let moved = exact_world_run(801, 801, moved_edits);

        for (app, edits) in [(&baseline, baseline_edits), (&moved, moved_edits)] {
            assert_eq!(
                app.tick_outcomes[0].attempts(),
                edits.map(TickAttempt::ExactExecuted)
            );
            assert_eq!(
                app.last_tick_batch.as_ref().unwrap().resolved_edits(),
                edits
            );
        }
        assert_ne!(
            baseline.history.current_world(),
            moved.history.current_world(),
            "moving this admitted On edit must change the resulting world tag"
        );
        assert_ne!(
            baseline.history.head(),
            moved.history.head(),
            "moving this admitted On edit must change the history receipt tag"
        );
    }

    const VIEWPORT_CHARACTERIZATION_DEEP_MARK: WorldEdit =
        WorldEdit::new(Operation::On, Coord::new(0, 0, -80_000));

    fn resized_aim_right_run(simulation_width: u32, simulation_height: u32) -> App {
        let mut app = App::for_headless_simulation();
        app.apply_infallible_accepted_action(AcceptedAction::Resize(
            simulation_width + SIDEBAR_WIDTH,
            simulation_height,
        ));
        app.apply_infallible_accepted_action(AcceptedAction::ExactEdit(
            VIEWPORT_CHARACTERIZATION_DEEP_MARK,
        ));
        app.fixed_tick();

        app.apply_infallible_accepted_action(AcceptedAction::FitWorld);
        app.fixed_tick();

        app.apply_infallible_accepted_action(AcceptedAction::MoveEdge(MoveControl::Right, true));
        app.apply_infallible_accepted_action(AcceptedAction::AimPlace);
        app.fixed_tick();
        app
    }

    #[test]
    fn different_resize_inputs_can_change_local_aim_resolution() {
        let small = resized_aim_right_run(640, 480);
        let large = resized_aim_right_run(1280, 960);
        let small_edit = WorldEdit::new(Operation::On, Coord::new(505, 0, -40_000));
        let large_edit = WorldEdit::new(Operation::On, Coord::new(510, 0, -40_000));

        assert_eq!(
            small.tick_outcomes[0].input().dimensions(),
            [640, 480, 640 + SIDEBAR_WIDTH, 480]
        );
        assert_eq!(
            large.tick_outcomes[0].input().dimensions(),
            [1280, 960, 1280 + SIDEBAR_WIDTH, 960]
        );
        assert_eq!(
            small.tick_outcomes[2].attempts(),
            [TickAttempt::AimPlaceExecuted(small_edit)]
        );
        assert_eq!(
            large.tick_outcomes[2].attempts(),
            [TickAttempt::AimPlaceExecuted(large_edit)]
        );
        assert_eq!(
            small.last_tick_batch.as_ref().unwrap().resolved_edits(),
            [small_edit]
        );
        assert_eq!(
            large.last_tick_batch.as_ref().unwrap().resolved_edits(),
            [large_edit]
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
    fn fixed_tick_reuses_the_history_world_root_for_idle_and_edit_ticks() {
        let mut app = App::new();

        assert!(app.fixed_tick());
        let idle = app.tick_outcomes.last().cloned().unwrap();
        assert_eq!(idle.pre_world(), idle.post_world());
        assert_eq!(idle.post_world(), world_tag(&app.world));
        assert_eq!(idle.post_world(), app.history.current_world());

        let placed = WorldEdit::new(Operation::On, Coord::new(7, -8, 9));
        app.apply_infallible_accepted_action(AcceptedAction::ExactEdit(placed));
        assert!(app.fixed_tick());

        let edited = app.tick_outcomes.last().cloned().unwrap();
        assert_eq!(edited.pre_world(), idle.post_world());
        assert_ne!(edited.post_world(), edited.pre_world());
        assert_eq!(edited.post_world(), world_tag(&app.world));
        assert_eq!(edited.post_world(), app.history.current_world());
        assert_eq!(
            app.history.receipts().last().unwrap().post_world(),
            edited.post_world()
        );
        let replayed = app.history.verify(&World::new()).unwrap();
        assert_eq!(world_tag(&replayed), world_tag(&app.world));
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

    /// The three actions ACCOUNT-1 was opened for.
    ///
    /// `ToggleSidebar`, `Resize` and `ClearMotion` apply the moment they are
    /// accepted, which is why they used to leave no trace in the tick: their
    /// effects reached the input state but nothing recorded that an action had
    /// been submitted. They are counted now, and the tick commits the count.
    #[test]
    fn control_actions_reach_the_tick_ledger() {
        let mut app = App::new();
        app.apply_infallible_accepted_action(AcceptedAction::ToggleSidebar);
        app.apply_infallible_accepted_action(AcceptedAction::Resize(640, 480));
        app.apply_infallible_accepted_action(AcceptedAction::ClearMotion);
        app.fixed_tick();

        let actions = app.tick_outcomes.last().unwrap().actions();
        assert_eq!(actions.lane(LANE_TOGGLE_SIDEBAR), 1);
        assert_eq!(actions.lane(LANE_RESIZE), 1);
        assert_eq!(actions.lane(LANE_CLEAR_MOTION), 1);
        assert_eq!(actions.control_lane_total(), 3);
        assert_eq!(actions.submitted(), 3);
        assert_eq!(
            actions.edit_lane_total(),
            0,
            "no edit was submitted, so the ordered dispositions stay empty"
        );

        // The next tick starts from zero: the ledger is consumed, not cumulative.
        app.fixed_tick();
        assert_eq!(app.tick_outcomes.last().unwrap().actions().submitted(), 0);
    }

    /// The replay half of the gate: the same accepted actions, arriving from a
    /// tape rather than from winit, must produce the same account.
    ///
    /// The expectation is derived from the tape record's canonical opcodes.
    /// Opcode identity is pinned independently by the interaction-tape vectors;
    /// the runtime deliberately uses that same codec authority as its lane.
    #[test]
    fn a_replayed_tape_accounts_for_its_control_actions() {
        let tape = InteractionTape::parse(
            "tick
sidebar
resize 640 480
clear-motion
",
        )
        .unwrap();
        let record = &tape.records()[0];

        let mut app = App::for_headless_simulation();
        for &action in record.actions() {
            app.apply_accepted_action(action).unwrap();
        }
        app.fixed_tick();

        let mut expected = TickActionLedger::empty();
        for &action in record.actions() {
            expected.record(action_lane(action)).unwrap();
        }

        let actual = app.tick_outcomes.last().unwrap().actions();
        for lane in 0..bitblock::tick::ACTION_LANE_COUNT {
            assert_eq!(
                actual.lane(lane),
                expected.lane(lane),
                "replayed lane {lane} diverged from the tape record"
            );
        }
        assert_eq!(actual.control_lane_total(), 3);
        assert_eq!(actual.submitted() as usize, record.actions().len());
    }

    /// A refused action is neither applied nor counted.
    #[test]
    fn a_malformed_action_leaves_no_trace_in_the_ledger() {
        let mut app = App::new();
        app.apply_infallible_accepted_action(AcceptedAction::MoveEdge(MoveControl::Forward, true));
        assert!(
            app.apply_accepted_action(AcceptedAction::LookDelta(f64::NAN, 0.0))
                .is_err()
        );
        app.fixed_tick();

        let actions = app.tick_outcomes.last().unwrap().actions();
        assert_eq!(actions.lane(LANE_MOVE_EDGE), 1);
        assert_eq!(
            actions.lane(LANE_LOOK_DELTA),
            0,
            "the refused look delta was never applied, so it is never counted"
        );
        assert_eq!(actions.submitted(), 1);
    }

    /// Startup-program edits are submissions too: they reach the first tick's
    /// fold and get ordered dispositions there.
    #[test]
    fn startup_program_edits_are_counted_as_submitted() {
        let program = Program::parse(
            "off 0 0 0
on 7 -8 9",
        )
        .unwrap();
        let mut app = App::with_program(program);
        app.fixed_tick();

        let outcome = app.tick_outcomes.last().unwrap();
        assert_eq!(outcome.actions().lane(LANE_EXACT_EDIT), 2);
        assert_eq!(
            outcome.actions().edit_lane_total() as usize,
            outcome.attempts().len(),
            "submitted edits reconcile with their dispositions"
        );
    }

    #[test]
    fn live_aim_inputs_map_to_named_outcome_dispositions() {
        let mut app = App::new();
        app.apply_infallible_accepted_action(AcceptedAction::AimPlace);
        app.apply_infallible_accepted_action(AcceptedAction::AimDelete);
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
        unresolved.apply_infallible_accepted_action(AcceptedAction::AimDelete);
        unresolved.fixed_tick();
        assert_eq!(
            unresolved.tick_outcomes.last().unwrap().attempts(),
            [TickAttempt::AimDeleteUnresolved]
        );

        let mut placed = App::new();
        placed.apply_infallible_accepted_action(AcceptedAction::WheelLines(-10.0));
        placed.apply_infallible_accepted_action(AcceptedAction::AimPlace);
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
        let repeated_outcome = TickOutcome::with_edit_attempts_only(
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
    fn f5_v2_pairs_color_with_each_topology_before_and_after_capture() {
        let path = TempDisplayJournal::new("f5-v2-paired-order");
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

        let mismatch_events = std::cell::RefCell::new(Vec::new());
        let mismatch = attempt_presentation_output_v2_observation(
            &mut archive,
            presented,
            expected,
            test_timing(receipt),
            |stage| {
                mismatch_events.borrow_mut().push(stage);
                Err(LivePresentationOutputFailure::OutputObservation {
                    stage,
                    error: ActiveOutputObservationError::Topology(
                        OutputTopologyError::DuplicateRoute,
                    ),
                })
            },
            || {
                mismatch_events.borrow_mut().push("capture");
                test_complete_capture(expected)
            },
        );
        assert!(matches!(
            mismatch,
            Err(LivePresentationOutputFailure::OutputObservation {
                stage: "before-capture",
                ..
            })
        ));
        assert_eq!(*mismatch_events.borrow(), ["before-capture"]);

        let events = std::cell::RefCell::new(Vec::new());
        let calls = std::cell::Cell::new(0_u32);
        let stored = match attempt_presentation_output_v2_observation(
            &mut archive,
            presented,
            expected,
            test_timing(receipt),
            |stage| {
                events.borrow_mut().push(stage);
                let call = calls.get();
                calls.set(call + 1);
                Ok(test_output_with_color(call + 1))
            },
            || {
                events.borrow_mut().push("capture");
                test_complete_capture(expected)
            },
        ) {
            Ok(stored) => stored,
            Err(_) => panic!("complete paired output observation must store"),
        };
        assert_eq!(
            *events.borrow(),
            ["before-capture", "capture", "after-capture"]
        );
        assert!(stored.color_before.is_some());
        assert!(stored.color_after.is_some());
        assert_eq!(
            stored.reported_color_state,
            Some(bitblock::presentation_output_v2::ReportedColorState::Unavailable)
        );
        assert_eq!(stored.artifact_count, 1);
        let entry = &archive.journal.state().entries()[0];
        assert_eq!(entry.color_before, stored.color_before);
        assert_eq!(entry.color_after, stored.color_after);
        assert_eq!(entry.reported_color_state, stored.reported_color_state);
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
            .prepare_and_append_presentation_output_v2(
                receipt,
                expected,
                &capture,
                test_timing(receipt),
                test_output_with_color(1),
                test_output_with_color(2),
            )
            .unwrap_err();
        assert!(!first_failure.poisoned);
        assert_eq!(
            first_failure.artifact,
            archive.pending_presentation_output()
        );
        assert_eq!(archive.pending_artifact(), None);
        assert!(matches!(
            archive.pending_presentation_output.as_ref(),
            Some(PendingPresentationOutput::V2(_))
        ));
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
        assert_eq!(app.compositor.last_game_prefix_copy_words(), 0);
        let simulation_width = app.renderer.width() as usize;
        let output_width = app.compositor.width() as usize;
        for row in 0..app.renderer.height() as usize {
            assert_eq!(
                &app.compositor.pixels()[row * output_width..row * output_width + simulation_width],
                &app.renderer.pixels()[row * simulation_width..(row + 1) * simulation_width],
                "sidebar-only recomposition changed the cached game prefix in row {row}"
            );
        }
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
        #[cfg(feature = "protocol-research")]
        assert_eq!(
            summary.local_replay_endpoint.world(),
            world_tag(&direct.world)
        );
        assert_eq!(summary.final_history, direct.history.head());
        assert_eq!(
            summary.final_camera_bits,
            TickCameraState::capture(&direct.camera).bits()
        );
    }

    #[test]
    fn ui_reachable_place_delete_example_returns_to_bedrock_only() {
        let tape =
            InteractionTape::parse(include_str!("../examples/observer_place_delete.tape")).unwrap();
        assert_eq!(tape.len(), 2);
        assert_eq!(
            tape.records()[0].actions(),
            &[AcceptedAction::WheelLines(-10.0), AcceptedAction::AimPlace]
        );
        assert_eq!(tape.records()[1].actions(), &[AcceptedAction::AimDelete]);

        let summary = simulate_interaction_tape(&tape).unwrap();
        let origin_only = App::for_headless_simulation();
        assert_eq!(summary.on_bits, 1);
        assert_eq!(summary.coordinate_receipts, 2);
        assert_eq!(summary.final_world, world_tag(&origin_only.world));
    }

    #[cfg(feature = "protocol-research")]
    #[test]
    fn local_replay_evidence_replays_the_embedded_tape_and_refuses_context_or_endpoint_swaps() {
        let tape =
            InteractionTape::parse(include_str!("../examples/observer_place_delete.tape")).unwrap();
        let (summary, evidence) =
            simulate_interaction_tape_with_local_replay_evidence(&tape).unwrap();
        assert_eq!(verify_local_replay_evidence(&evidence).unwrap(), summary);

        let different_tape = InteractionTape::parse("tick\nwheel 1\n").unwrap();
        let wrong_endpoint =
            LocalReplayEvidence::new(evidence.context(), different_tape, evidence.endpoint())
                .unwrap();
        let endpoint_error = verify_local_replay_evidence(&wrong_endpoint).unwrap_err();
        assert_eq!(endpoint_error.kind(), io::ErrorKind::InvalidData);
        assert!(endpoint_error.to_string().contains("endpoint differs"));

        let mut changed_build = evidence.encode_canonical();
        changed_build[size_of::<u64>()] ^= 1;
        let wrong_context = LocalReplayEvidence::decode_canonical(&changed_build).unwrap();
        let context_error = verify_local_replay_evidence(&wrong_context).unwrap_err();
        assert_eq!(context_error.kind(), io::ErrorKind::InvalidData);
        assert!(context_error.to_string().contains("context differs"));

        let context = evidence.context();
        let changed_policy = |policy: ContentTag| {
            let digest = policy.digest();
            hash_canonical(
                TagKind::Policy,
                b"bitblock/test/local-replay-policy-context-swap/v1",
                digest.len() as u64,
                |sink| sink(&digest),
            )
            .unwrap()
        };
        for (name, changed_context) in [
            (
                "genesis",
                LocalReplayContext::new(
                    context.build(),
                    changed_policy(context.genesis_policy()),
                    context.rules_policy(),
                    context.admission_policy(),
                )
                .unwrap(),
            ),
            (
                "rules",
                LocalReplayContext::new(
                    context.build(),
                    context.genesis_policy(),
                    changed_policy(context.rules_policy()),
                    context.admission_policy(),
                )
                .unwrap(),
            ),
            (
                "admission",
                LocalReplayContext::new(
                    context.build(),
                    context.genesis_policy(),
                    context.rules_policy(),
                    changed_policy(context.admission_policy()),
                )
                .unwrap(),
            ),
        ] {
            let structurally_valid = LocalReplayEvidence::new(
                changed_context,
                evidence.tape().clone(),
                evidence.endpoint(),
            )
            .unwrap();
            let decoded =
                LocalReplayEvidence::decode_canonical(&structurally_valid.encode_canonical())
                    .unwrap();
            decoded.verify().unwrap();
            let error = verify_local_replay_evidence(&decoded).unwrap_err();
            assert_eq!(error.kind(), io::ErrorKind::InvalidData, "{name}");
            assert!(
                error.to_string().contains("context differs"),
                "{name}: {error}"
            );
        }
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

    #[cfg(all(windows, feature = "protocol-research"))]
    #[test]
    fn clean_display_journal_report_does_not_promote_capture_or_causal_references() {
        let required = ContentTag::parse_full(
            "#bb.storage-record.sha256.aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        )
        .unwrap();
        let report = display_journal_verification_header(required);

        assert!(report.contains("status=clean"));
        assert!(report.contains("pixel_evidence=retained-capture-claim-per-entry"));
        assert!(report.contains("causal_ancestry=references-only"));
        assert!(!report.contains("os-desktop-raster-sample"));
        assert!(report.contains(&format!("required_ancestor={required}")));
    }
}
