//! Public-function calls that the research tests reach only through private helpers.
//!
//! `small_artifact`, `v1`/`small`, and `fixture`/`execute_v2_fixture` stay private.
//! These functions call the same production constructors those helpers call.

use bitblock_core::camera::Camera;
use bitblock_core::display::{
    CaptureRect, CaptureStage, DisplayCaptureFacts, DisplayLedgerV2, DisplayOutcome,
};
use bitblock_core::frame::{FrameInput, FrameRef};
use bitblock_core::frame_v2::{
    FrameLedgerV2, FrameReceiptV2, FrameStream, admit_frame_pair_derived,
};
use bitblock_core::history::History;
use bitblock_core::history::world_tag;
use bitblock_core::program::{Edit, Operation};
use bitblock_core::protocol::{ContentTag, hash_canonical};
use bitblock_core::render::{crosshair_overlay_input, renderer_profile_tag};
use bitblock_core::sidebar::composite_overlay_input;
use bitblock_core::tick::{
    TickAttempt, TickCameraState, TickFrameCommitment, TickInputFlags, TickInputState, TickOutcome,
};
use bitblock_core::world::{BEDROCK_COORD, Coord, World};
use bitblock_research::display_artifact::DisplayCaptureArtifact;
use bitblock_research::optimization::{
    CompositeFrameArtifact, DeterministicSyntheticDisplay, OptimizationArtifact, OptimizationCase,
    OptimizationIdentity, assay_optimization_equivalence,
};
use bitblock_research::output_color::{AdvancedColorRouteFacts, OutputColorObservation};
use bitblock_research::output_topology::{
    AcquisitionAttempt, AcquisitionFacts, OutputTopologyObservation, RawRational, RectI32,
    SourceFacts, SourceModeFacts, TargetIdentityObservation, TargetRouteFacts, TargetSignalTiming,
    Utf16Text,
};
use bitblock_research::presentation::{
    CompositionTimingEnvelope, DwmFlushCall, DwmTimingCall, GdiFlushCall, PresenterEvidence,
    QpcBracket, QpcFrequency, QpcRead,
};
use bitblock_research::presentation_output::PresentationOutputArtifact;
use bitblock_research::presentation_output_v2::PresentationOutputArtifactV2;
use bitblock_research::protocol::TagKind;

const PIXELS: [u32; 2] = [0x0011_2233, 0x0044_5566];

fn presentation_tag(domain: &[u8], kind: TagKind, lane: u8) -> ContentTag {
    hash_canonical(kind, domain, 1, |sink| sink(&[lane])).expect("presentation test tag")
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

fn timing(domain: &[u8], receipt: FrameReceiptV2, policy_lane: u8) -> CompositionTimingEnvelope {
    CompositionTimingEnvelope::new(
        receipt,
        PresenterEvidence {
            policy: presentation_tag(domain, TagKind::Policy, policy_lane),
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
    .expect("composition timing envelope")
}

fn acquisition(seed: u32) -> AcquisitionFacts {
    AcquisitionFacts {
        profile_code_raw: seed,
        request_flags_raw: seed.rotate_left(3),
        attempts: vec![AcquisitionAttempt {
            retry_ordinal: 0,
            outcome_code_raw: seed,
            ..AcquisitionAttempt::default()
        }],
    }
}

fn v1_source() -> SourceFacts {
    SourceFacts {
        gdi_device_name: Utf16Text::from_units(&[b'D' as u16]).expect("device name"),
        client_rect: RectI32 {
            left: 0,
            top: 0,
            right: 2,
            bottom: 1,
        },
        monitor_rect: RectI32 {
            left: 0,
            top: 0,
            right: 1920,
            bottom: 1080,
        },
        work_rect: RectI32 {
            left: 0,
            top: 0,
            right: 1920,
            bottom: 1040,
        },
        monitor_flags_raw: 1,
        mode: SourceModeFacts {
            width: 1920,
            height: 1080,
            ..SourceModeFacts::default()
        },
        status_flags_raw: 0,
    }
}

fn v1_route(path_flags_raw: u32) -> TargetRouteFacts {
    TargetRouteFacts {
        identity: TargetIdentityObservation::Unavailable {
            reason_code_raw: 7,
            error_code: 5,
        },
        output_technology_raw: 1,
        target_available: true,
        status_flags_raw: 2,
        rotation_raw: 1,
        scaling_raw: 1,
        refresh_rate: RawRational {
            numerator: 60,
            denominator: 1,
        },
        path_flags_raw,
        signal: TargetSignalTiming::default(),
        desktop_image: None,
    }
}

fn unavailable_display(receipt: FrameReceiptV2) -> DisplayCaptureArtifact {
    let expected = FrameRef::new(2, 1, &PIXELS).expect("2x1 frame");
    let mut ledger = DisplayLedgerV2::default();
    let display_receipt = *ledger
        .append(
            receipt,
            2,
            1,
            DisplayCaptureFacts::unavailable(CaptureStage::FlushBefore, 5),
        )
        .expect("display ledger append");
    DisplayCaptureArtifact::new(display_receipt, expected, None).expect("display artifact")
}

/// `PresentationOutputArtifact::new`, the call inside private `small_artifact`.
pub fn research_presentation_output_status() -> String {
    const DOMAIN: &[u8] = b"bitblock/presentation-output-test/v1";
    let frame = FrameRef::new(2, 1, &PIXELS).expect("2x1 frame");
    let receipt = FrameReceiptV2::new(
        None,
        FrameStream::Composite,
        presentation_tag(DOMAIN, TagKind::TickOutcome, 1),
        presentation_tag(DOMAIN, TagKind::FrameInput, 2),
        frame.content_tag(),
    )
    .expect("frame receipt");
    let artifact = PresentationOutputArtifact::new(
        unavailable_display(receipt),
        timing(DOMAIN, receipt, 9),
        OutputTopologyObservation::new_available(acquisition(1), v1_source(), vec![v1_route(4)])
            .expect("pre topology"),
        OutputTopologyObservation::new_available(acquisition(2), v1_source(), vec![v1_route(4)])
            .expect("post topology"),
    )
    .expect("presentation output artifact");
    let encoded = artifact
        .encode_canonical()
        .expect("presentation output encodes");
    format!(
        "bitblock_browser_research ok module=presentation_output tag={} bytes={} stability={}",
        artifact.tag(),
        encoded.len(),
        match artifact.route_stability() {
            bitblock_research::presentation_output::RouteStability::Stable => "stable",
            bitblock_research::presentation_output::RouteStability::Changed => "changed",
            bitblock_research::presentation_output::RouteStability::Unavailable => "unavailable",
        },
    )
}

fn v2_topology(seed: u32) -> OutputTopologyObservation {
    OutputTopologyObservation::new_available(
        acquisition(seed),
        SourceFacts {
            gdi_device_name: Utf16Text::from_units(&[b'D' as u16]).expect("device name"),
            client_rect: RectI32 {
                right: 2,
                bottom: 1,
                ..RectI32::default()
            },
            monitor_rect: RectI32 {
                right: 1920,
                bottom: 1080,
                ..RectI32::default()
            },
            work_rect: RectI32 {
                right: 1920,
                bottom: 1040,
                ..RectI32::default()
            },
            monitor_flags_raw: 1,
            mode: SourceModeFacts {
                width: 1920,
                height: 1080,
                ..SourceModeFacts::default()
            },
            status_flags_raw: 0,
        },
        vec![TargetRouteFacts {
            identity: TargetIdentityObservation::Unavailable {
                reason_code_raw: 7,
                error_code: 5,
            },
            output_technology_raw: 1,
            target_available: true,
            status_flags_raw: 2,
            rotation_raw: 1,
            scaling_raw: 1,
            refresh_rate: RawRational {
                numerator: 60,
                denominator: 1,
            },
            path_flags_raw: 4,
            signal: TargetSignalTiming {
                pixel_rate: 1,
                h_sync: RawRational {
                    numerator: 1,
                    denominator: 1,
                },
                v_sync: RawRational {
                    numerator: 1,
                    denominator: 1,
                },
                active_size: bitblock_research::output_topology::PixelSize {
                    width: 1,
                    height: 1,
                },
                total_size: bitblock_research::output_topology::PixelSize {
                    width: 1,
                    height: 1,
                },
                video_standard_raw: 0,
                scanline_ordering_raw: 0,
            },
            desktop_image: None,
        }],
    )
    .expect("v2 topology")
}

/// `PresentationOutputArtifactV2::new`, the call inside private `small`.
pub fn research_presentation_output_v2_status() -> String {
    const DOMAIN: &[u8] = b"bitblock/presentation-output-v2-test/v1";
    let frame = FrameRef::new(2, 1, &PIXELS).expect("2x1 frame");
    let receipt = FrameReceiptV2::new(
        None,
        FrameStream::Composite,
        presentation_tag(DOMAIN, TagKind::TickOutcome, 1),
        presentation_tag(DOMAIN, TagKind::FrameInput, 2),
        frame.content_tag(),
    )
    .expect("frame receipt");
    let v1 = PresentationOutputArtifact::new(
        unavailable_display(receipt),
        timing(DOMAIN, receipt, 3),
        v2_topology(1),
        v2_topology(2),
    )
    .expect("v1 artifact");
    let color = |topology: &OutputTopologyObservation| {
        OutputColorObservation::new_available(
            topology,
            vec![AdvancedColorRouteFacts::Available {
                flags_raw: 1,
                color_encoding_raw: 0xffff_fffe,
                bits_per_color_channel: 10,
            }],
        )
        .expect("color observation")
    };
    let pre = color(v1.pre_topology());
    let post = color(v1.post_topology());
    let artifact =
        PresentationOutputArtifactV2::new(v1, pre, post).expect("presentation output v2");
    let encoded = artifact
        .encode_canonical()
        .expect("presentation output v2 encodes");
    format!(
        "bitblock_browser_research ok module=presentation_output_v2 tag={} bytes={} v1={} color={}",
        artifact.tag(),
        encoded.len(),
        artifact.v1().tag(),
        match artifact.reported_color_state() {
            bitblock_research::presentation_output_v2::ReportedColorState::Stable => "stable",
            bitblock_research::presentation_output_v2::ReportedColorState::Changed => "changed",
            bitblock_research::presentation_output_v2::ReportedColorState::Unavailable =>
                "unavailable",
        },
    )
}

fn opt_tag(kind: TagKind, bytes: &[u8]) -> ContentTag {
    hash_canonical(
        kind,
        b"bitblock/optimization-test/v1",
        bytes.len() as u64,
        |sink| sink(bytes),
    )
    .expect("optimization test tag")
}

fn opt_plans() -> [Vec<Edit>; 2] {
    const A: Coord = Coord::new(1, 0, 0);
    const B: Coord = Coord::new(2, 0, 0);
    const C: Coord = Coord::new(3, 0, 0);
    const D: Coord = Coord::new(4, 0, 0);
    [
        vec![
            Edit::new(Operation::On, A),
            Edit::new(Operation::On, A),
            Edit::new(Operation::Off, B),
            Edit::new(Operation::Off, BEDROCK_COORD),
            Edit::new(Operation::On, C),
            Edit::new(Operation::Off, C),
        ],
        vec![
            Edit::new(Operation::Off, A),
            Edit::new(Operation::On, A),
            Edit::new(Operation::On, D),
            Edit::new(Operation::Off, D),
        ],
    ]
}

fn synthetic_display(
    pixels: &[u32],
    width: u32,
    height: u32,
    frame_receipt: FrameReceiptV2,
) -> DeterministicSyntheticDisplay {
    let observation = FrameRef::new(width, height, pixels)
        .expect("composite frame")
        .observe();
    let rect = CaptureRect {
        left: 0,
        top: 0,
        right: width as i32,
        bottom: height as i32,
    };
    let facts = DisplayCaptureFacts {
        outcome: DisplayOutcome::Match,
        stage: CaptureStage::Complete,
        error_code: 0,
        flush_before: true,
        compositor_flush: true,
        flush_after: true,
        rect_before: rect,
        rect_after: rect,
        dpi: 96,
        qpc_start: 1,
        qpc_end: 2,
        qpc_frequency: 1,
        captured: Some(observation),
        mismatch_count: 0,
        first_mismatch: None,
    };
    let mut ledger = DisplayLedgerV2::default();
    ledger
        .append(frame_receipt, width, height, facts)
        .expect("synthetic display append");
    DeterministicSyntheticDisplay::new(ledger).expect("synthetic display")
}

/// `assay_optimization_equivalence`, the call inside private `fixture`.
pub fn research_optimization_status() -> String {
    let initial_world = World::new();
    let mut world = initial_world.clone();
    let mut history = History::new(&world);
    let camera = Camera::for_single_pixel_voxel(2);
    let camera_state = TickCameraState::capture(&camera);
    let idle_input = TickInputState::new(TickInputFlags::default(), 0.0, 0.0, 0.0, 2, 2, 3, 2)
        .expect("idle input");
    let initial_world_tag = world_tag(&world);
    let genesis = TickOutcome::with_edit_attempts_only(
        None,
        0,
        initial_world_tag,
        history.genesis(),
        idle_input,
        [],
        initial_world_tag,
        history.genesis(),
        camera_state,
        None,
        None,
    )
    .expect("genesis outcome");
    let rendered_input = TickInputState::new(
        TickInputFlags::new(
            false, false, false, false, false, false, false, true, true, true, false,
        ),
        0.0,
        0.0,
        0.0,
        2,
        2,
        3,
        2,
    )
    .expect("rendered input");
    let plans = opt_plans();
    let mut parent = genesis.tag();
    let mut successors = Vec::new();
    let mut final_game_pixels = Vec::new();
    let mut final_input = None;
    let mut final_game_observation = None;
    let mut final_composite_input = None;
    let mut final_composite_overlay = None;
    let mut final_composite_pixels = Vec::new();
    let mut final_composite_observation = None;
    for (index, edits) in plans.iter().enumerate() {
        let tick = index as u64 + 1;
        let pre_world = world_tag(&world);
        let pre_history = history.head();
        let mut transaction = history.begin_tick(tick);
        for &edit in edits {
            transaction.apply_edit(&history, &mut world, edit);
        }
        history.commit_tick(transaction, &world).expect("commit");
        let post_world = world_tag(&world);
        let post_history = history.head();
        let game_pixels = if index == 0 {
            vec![0x0000_0001, 0x0000_0002, 0x0000_0003, 0x0000_0004]
        } else {
            vec![0x0001_0203, 0x0004_0506, 0, 0x00ff_ffff]
        };
        let overlay = crosshair_overlay_input();
        let input = FrameInput::capture(
            post_world,
            &camera,
            2,
            2,
            renderer_profile_tag(),
            overlay.tag(),
        )
        .expect("frame input");
        let game_observation = FrameRef::new(2, 2, &game_pixels)
            .expect("game frame")
            .observe();
        let composite_overlay =
            composite_overlay_input(opt_tag(TagKind::MetricSnapshot, b"snapshot"))
                .expect("composite overlay");
        let composite_input = bitblock_core::sidebar::CompositeFrameInput::new(
            input.tag(),
            game_observation.tag,
            composite_overlay,
            2,
            2,
            3,
            2,
        );
        let composite_pixels = vec![
            game_pixels[0],
            game_pixels[1],
            7,
            game_pixels[2],
            game_pixels[3],
            0,
        ];
        let composite_observation = FrameRef::new(3, 2, &composite_pixels)
            .expect("composite frame")
            .observe();
        let game = TickFrameCommitment::new(input.tag(), game_observation.tag).expect("game");
        let composite = TickFrameCommitment::new(composite_input.tag(), composite_observation.tag)
            .expect("composite");
        let outcome = TickOutcome::with_edit_attempts_only(
            Some(parent),
            tick,
            pre_world,
            pre_history,
            rendered_input,
            edits.iter().copied().map(TickAttempt::ExactExecuted),
            post_world,
            post_history,
            camera_state,
            Some(game),
            Some(composite),
        )
        .expect("tick outcome");
        outcome
            .verify_game_input_binding(renderer_profile_tag(), crosshair_overlay_input().tag())
            .expect("game input binding");
        parent = outcome.tag();
        successors.push(outcome);
        final_game_pixels = game_pixels;
        final_input = Some(input);
        final_game_observation = Some(game_observation);
        final_composite_input = Some(composite_input);
        final_composite_overlay = Some(composite_overlay);
        final_composite_pixels = composite_pixels;
        final_composite_observation = Some(composite_observation);
    }
    let final_world_tag = world_tag(&world);
    let mut frame_ledger =
        FrameLedgerV2::new_structural_genesis(FrameStream::Game, genesis.clone())
            .expect("game ledger");
    let mut composite_frame_ledger =
        FrameLedgerV2::new_structural_genesis(FrameStream::Composite, genesis)
            .expect("composite ledger");
    let receipts = admit_frame_pair_derived(
        Some((&mut frame_ledger, successors.clone().into_boxed_slice())),
        (&mut composite_frame_ledger, successors.into_boxed_slice()),
        |_| Ok::<(), std::convert::Infallible>(()),
    )
    .expect("frame pair admission");
    let composite = CompositeFrameArtifact {
        width: 3,
        height: 2,
        pixels: final_composite_pixels.clone(),
        input: final_composite_input.expect("composite input"),
        overlay_input: final_composite_overlay.expect("overlay"),
        observation: final_composite_observation.expect("composite observation"),
        frame_ledger: composite_frame_ledger,
    };
    let synthetic_display = synthetic_display(&final_composite_pixels, 3, 2, receipts.composite());
    let reference = OptimizationArtifact {
        identity: OptimizationIdentity {
            implementation: Some(opt_tag(TagKind::Registry, b"reference")),
            policy: Some(opt_tag(TagKind::Policy, b"reference-policy")),
        },
        final_world: world,
        final_world_tag,
        history,
        frame_input: final_input.expect("frame input"),
        game_pixels: final_game_pixels,
        game_observation: final_game_observation.expect("game observation"),
        frame_ledger,
        composite: Some(composite),
        synthetic_display: Some(synthetic_display),
    };
    let mut candidate = reference.clone();
    candidate.identity = OptimizationIdentity {
        implementation: Some(opt_tag(TagKind::Registry, b"candidate")),
        policy: Some(opt_tag(TagKind::Policy, b"candidate-policy")),
    };
    let case = OptimizationCase {
        case_tag: Some(opt_tag(TagKind::Registry, b"two-tick-case")),
        initial_world,
        reference,
        candidate,
    };
    let receipt = assay_optimization_equivalence(&case).expect("optimization equivalence");
    format!(
        "bitblock_browser_research ok module=optimization receipt={} case={} output={}",
        receipt.tag(),
        receipt.case(),
        receipt.output(),
    )
}
