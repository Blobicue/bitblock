//! Windows display-artifact archive, presentation-output capture, and Win32 window helpers.

use super::*;

#[cfg(all(windows, feature = "protocol-research"))]
pub(super) enum PendingPresentationOutput {
    #[cfg(test)]
    V1(Box<PresentationOutputArtifact>),
    V2(Box<PresentationOutputArtifactV2>),
}

#[cfg(all(windows, feature = "protocol-research"))]
impl PendingPresentationOutput {
    pub(super) const fn tag(&self) -> ContentTag {
        match self {
            #[cfg(test)]
            Self::V1(presentation) => presentation.tag(),
            Self::V2(presentation) => presentation.tag(),
        }
    }
}

#[cfg(all(windows, feature = "protocol-research"))]
pub(super) struct LiveDisplayArtifactArchive {
    pub(super) journal: DisplayArtifactJournal,
    pub(super) pending_artifact: Option<DisplayCaptureArtifact>,
    pub(super) pending_presentation_output: Option<PendingPresentationOutput>,
    pub(super) faulted: bool,
}

#[cfg(all(windows, feature = "protocol-research"))]
pub(super) struct LiveDisplayArtifactFailure {
    pub(super) error: Box<DisplayArtifactStoreError>,
    pub(super) artifact: Option<bitblock::protocol::ContentTag>,
    pub(super) poisoned: bool,
}

#[cfg(all(windows, feature = "protocol-research"))]
impl LiveDisplayArtifactArchive {
    pub(super) fn new(journal: DisplayArtifactJournal) -> Self {
        Self {
            journal,
            pending_artifact: None,
            pending_presentation_output: None,
            faulted: false,
        }
    }

    pub(super) fn can_capture_new(&self) -> bool {
        !self.faulted
            && self.pending_artifact.is_none()
            && self.pending_presentation_output.is_none()
    }

    pub(super) fn pending_artifact(&self) -> Option<bitblock::protocol::ContentTag> {
        self.pending_artifact
            .as_ref()
            .map(DisplayCaptureArtifact::tag)
    }

    pub(super) fn pending_presentation_output(&self) -> Option<bitblock::protocol::ContentTag> {
        self.pending_presentation_output
            .as_ref()
            .map(PendingPresentationOutput::tag)
    }

    pub(super) fn prepare_and_append(
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

    pub(super) fn append_pending(
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

    #[cfg(test)]
    pub(super) fn prepare_and_append_presentation_output(
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
        self.pending_presentation_output =
            Some(PendingPresentationOutput::V1(Box::new(presentation)));
        self.append_pending_presentation_output()
    }

    pub(super) fn prepare_and_append_presentation_output_v2(
        &mut self,
        expected_receipt: FrameReceiptV2,
        expected: FrameRef<'_>,
        capture: &DisplayCapture,
        timing: CompositionTimingEnvelope,
        pre_output: ActiveOutputWithColor,
        post_output: ActiveOutputWithColor,
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
        let v1 = match PresentationOutputArtifact::new(
            display,
            timing,
            pre_output.topology().clone(),
            post_output.topology().clone(),
        ) {
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
        let presentation = match PresentationOutputArtifactV2::new(
            v1,
            pre_output.color().clone(),
            post_output.color().clone(),
        ) {
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
        self.pending_presentation_output =
            Some(PendingPresentationOutput::V2(Box::new(presentation)));
        self.append_pending_presentation_output()
    }

    pub(super) fn append_pending_presentation_output(
        &mut self,
    ) -> Result<PresentationOutputAppendReceipt, LiveDisplayArtifactFailure> {
        let presentation = self
            .pending_presentation_output
            .as_ref()
            .expect("an explicit F5 retry requires one retained presentation-output artifact");
        let presentation_tag = presentation.tag();
        let append = match presentation {
            #[cfg(test)]
            PendingPresentationOutput::V1(presentation) => {
                self.journal.append_presentation_output(presentation)
            }
            PendingPresentationOutput::V2(presentation) => {
                self.journal.append_presentation_output_v2(presentation)
            }
        };
        match append {
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
#[cfg(all(windows, feature = "protocol-research"))]
pub(super) fn service_display_artifact_request(
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
pub(super) fn report_display_artifact_result(
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
pub(super) enum LivePresentationOutputFailure {
    MissingTiming,
    Timing(PresentationProtocolError),
    TimingReceiptMismatch {
        presented: bitblock::protocol::ContentTag,
        timing: bitblock::protocol::ContentTag,
    },
    MissingWindowHandle,
    #[cfg(test)]
    Topology {
        stage: &'static str,
        error: OutputTopologyError,
    },
    OutputObservation {
        stage: &'static str,
        error: ActiveOutputObservationError,
    },
    Store(LiveDisplayArtifactFailure),
}

#[cfg(all(windows, feature = "protocol-research"))]
#[cfg(test)]
pub(super) fn attempt_presentation_output_observation<ObserveTopology, Capture>(
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

/// Captures one F5 candidate from paired topology/color observations collected immediately
/// before and after the GDI capture. A retained storage retry reuses the complete v2 wrapper and
/// never samples a later Windows configuration.
#[cfg(all(windows, feature = "protocol-research"))]
pub(super) fn attempt_presentation_output_v2_observation<ObserveOutput, Capture>(
    archive: &mut LiveDisplayArtifactArchive,
    presented: PresentedFrameObservation,
    expected: FrameRef<'_>,
    timing: CompositionTimingEnvelope,
    mut observe_output: ObserveOutput,
    capture: Capture,
) -> Result<PresentationOutputAppendReceipt, LivePresentationOutputFailure>
where
    ObserveOutput:
        FnMut(&'static str) -> Result<ActiveOutputWithColor, LivePresentationOutputFailure>,
    Capture: FnOnce() -> DisplayCapture,
{
    if timing.receipt() != presented.receipt_v2 {
        return Err(LivePresentationOutputFailure::TimingReceiptMismatch {
            presented: presented.receipt_v2.tag(),
            timing: timing.receipt().tag(),
        });
    }
    let pre_output = observe_output("before-capture")?;
    let capture = capture();
    let post_output = observe_output("after-capture")?;
    archive
        .prepare_and_append_presentation_output_v2(
            presented.receipt_v2,
            expected,
            &capture,
            timing,
            pre_output,
            post_output,
        )
        .map_err(LivePresentationOutputFailure::Store)
}

#[cfg(all(windows, feature = "protocol-research"))]
pub(super) fn service_presentation_output_request_with<'a, Attempt>(
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
    // the complete wrapper and performs no timing, topology/color, capture, or pixel calls.
    *requested = false;
    Some(attempt(archive, presented, expected))
}

#[cfg(all(windows, feature = "protocol-research"))]
pub(super) fn service_presentation_output_request(
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
            attempt_presentation_output_v2_observation(
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
                    unsafe { observe_active_output_with_color(hwnd) }.map_err(|error| {
                        LivePresentationOutputFailure::OutputObservation { stage, error }
                    })
                },
                || capture_visible_client(window, expected),
            )
        },
    )
}

#[cfg(all(windows, feature = "protocol-research"))]
pub(super) fn report_presentation_output_result(
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
                "presentation-output-stored\toutput={}\tdisplay={}\ttiming={}\ttopology_before={}\ttopology_after={}\tstability={:?}\tcolor_before={}\tcolor_after={}\treported_color_state={:?}\trecord={}\trange=[{}, {})\tcount={}",
                receipt.presentation_output,
                receipt.artifact,
                receipt.timing_artifact,
                receipt.topology_before,
                receipt.topology_after,
                receipt.route_stability,
                optional_tag(receipt.color_before),
                optional_tag(receipt.color_after),
                receipt.reported_color_state,
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
        #[cfg(test)]
        Err(LivePresentationOutputFailure::Topology { stage, error }) => {
            window.set_title("bitblock — output topology refused");
            eprintln!(
                "presentation-output-refused\tdisposition=topology-admission-fault\tstage={stage}\t{error}"
            );
        }
        Err(LivePresentationOutputFailure::OutputObservation { stage, error }) => {
            window.set_title("bitblock — output configuration refused");
            eprintln!(
                "presentation-output-refused\tdisposition=output-configuration-admission-fault\tstage={stage}\t{error}"
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
#[cfg(windows)]
pub(super) fn display_audit_enabled() -> bool {
    std::env::var("BITBLOCK_DISPLAY_AUDIT").is_ok_and(|value| {
        value == "1" || value.eq_ignore_ascii_case("true") || value.eq_ignore_ascii_case("on")
    })
}

#[cfg(all(windows, feature = "protocol-research"))]
pub(super) fn presentation_audit_enabled() -> bool {
    std::env::var(PRESENTATION_AUDIT_ENV).is_ok_and(|value| {
        value == "1" || value.eq_ignore_ascii_case("true") || value.eq_ignore_ascii_case("on")
    })
}

#[cfg(windows)]
pub(super) fn capture_visible_client(window: &Window, expected: FrameRef<'_>) -> DisplayCapture {
    let Some(hwnd) = win32_hwnd(window) else {
        return DisplayCapture::unavailable(CaptureStage::WindowHandle, 0);
    };
    // SAFETY: winit owns this HWND on the current event-loop thread, and the borrowed window
    // outlives the synchronous capture call.
    unsafe { bitblock::display::windows::capture_client(hwnd, expected) }
}

#[cfg(windows)]
pub(super) fn win32_hwnd(window: &Window) -> Option<HWND> {
    let handle = window.window_handle().ok()?;
    let RawWindowHandle::Win32(handle) = handle.as_raw() else {
        return None;
    };
    Some(handle.hwnd.get() as HWND)
}

#[cfg(windows)]
pub(super) fn disable_window_corner_clipping(window: &Window) -> Result<(), i32> {
    let Some(hwnd) = win32_hwnd(window) else {
        return Err(-1);
    };
    let preference = DWMWCP_DONOTROUND;
    // SAFETY: `hwnd` is owned by the live `window` reference and `preference` is a valid stack
    // `i32` whose exact byte size is passed to `DwmSetWindowAttribute`.
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

pub(super) fn top_align_window_on_monitor(window: &Window) {
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
