//! Interactive observer `App` state, fixed-tick loop, sidebar snapshot, and `winit` handler.

use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum AcceptedActionApplyError {
    NonFiniteLane {
        field: &'static str,
        bits: u64,
    },
    NonFiniteAccumulation {
        field: &'static str,
        prior_bits: u64,
        delta_bits: u64,
    },
    Unaccountable(TickActionError),
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
            Self::Unaccountable(error) => {
                write!(formatter, "tick cannot account for the action: {error}")
            }
        }
    }
}

impl std::error::Error for AcceptedActionApplyError {}

/// The ledger lane one accepted action counts into.
///
/// The interaction-tape opcode is the lane ordinal. `canonical_code` is already
/// exhaustive over `AcceptedAction`, so there is no second variant-to-lane table
/// that can drift from the canonical accepted-input codec.
pub(super) const fn action_lane(action: AcceptedAction) -> usize {
    action.canonical_code() as usize
}

pub(super) fn checked_accepted_accumulation(
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
pub(super) enum AppMode {
    Live,
    Headless,
}

pub(super) struct App {
    pub(super) display: Option<Display>,
    pub(super) world: World,
    pub(super) history: History,
    pub(super) verified_world_revision: u64,
    pub(super) verified_world_tag: ContentTag,
    pub(super) camera: Camera,
    pub(super) renderer: Renderer,
    pub(super) compositor: FrameCompositor,
    pub(super) inputs: Inputs,
    pub(super) mouse_captured: bool,
    pub(super) cursor_position: Option<PhysicalPosition<f64>>,
    pub(super) sidebar_visible: bool,
    pub(super) display_predecessor_override: Option<DisplayStatus>,
    pub(super) last_frame: Instant,
    pub(super) next_tick: Instant,
    pub(super) accumulator: Duration,
    pub(super) needs_render: bool,
    pub(super) needs_sidebar_render: bool,
    pub(super) needs_present: bool,
    pub(super) tick: u64,
    pub(super) last_tick_batch: Option<TickBatch>,
    pub(super) tick_outcomes: Vec<TickOutcome>,
    pub(super) frame_ledger: FrameLedger,
    pub(super) output_frame_ledger: FrameLedger,
    pub(super) frame_ledgers_v2: Option<LiveFrameLedgersV2>,
    pub(super) pending_frame: Option<PendingRenderedFrame>,
    pub(super) pending_output_frame: Option<PendingRenderedFrame>,
    pub(super) pending_sidebar_facts: Option<SidebarRasterFacts>,
    pub(super) last_presented_frame: Option<PresentedFrameObservation>,
    pub(super) last_presented_output: Option<PresentedFrameObservation>,
    pub(super) last_presented_sidebar_facts: Option<SidebarRasterFacts>,
    #[cfg(windows)]
    pub(super) display_ledger: Option<DisplayLedgerV2>,
    #[cfg(all(windows, feature = "protocol-research"))]
    pub(super) display_artifact_archive: Option<LiveDisplayArtifactArchive>,
    #[cfg(all(windows, feature = "protocol-research"))]
    pub(super) display_artifact_capture_requested: bool,
    #[cfg(all(windows, feature = "protocol-research"))]
    pub(super) presentation_output_capture_requested: bool,
    #[cfg(all(windows, feature = "protocol-research"))]
    pub(super) presentation_audit_enabled: bool,
}
impl App {
    #[cfg(test)]
    pub(super) fn new() -> Self {
        Self::with_program(Program::new())
    }

    pub(super) fn with_program(program: Program) -> Self {
        Self::with_program_mode(program, AppMode::Live)
    }

    pub(super) fn for_headless_simulation() -> Self {
        Self::with_program_mode(Program::new(), AppMode::Headless)
    }

    pub(super) fn with_program_mode(program: Program, mode: AppMode) -> Self {
        let now = Instant::now();
        let mut inputs = Inputs::default();
        // Startup-program edits reach the first tick's fold and get ordered
        // dispositions there, so they are submissions and must be counted as
        // such. `load_startup_mode` has already refused an over-large program.
        for edit in program {
            inputs
                .actions
                .record(LANE_EXACT_EDIT)
                .expect("startup program size is bounded before the app is built");
            inputs.edits.push(PendingEdit::Exact(edit));
        }
        let world = World::new();
        let history = History::new(&world);
        let verified_world_revision = world.revision();
        let verified_world_tag = history.current_world();
        Self {
            display: None,
            world,
            history,
            verified_world_revision,
            verified_world_tag,
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
    pub(super) fn apply_accepted_action(
        &mut self,
        action: AcceptedAction,
    ) -> Result<(), AcceptedActionApplyError> {
        // Account first, apply second. The reservation is taken on a copy, so an
        // action this tick has no room for is refused before anything mutates,
        // and an action the body refuses as malformed is never counted either.
        let mut accounted = self.inputs.actions;
        accounted
            .record(action_lane(action))
            .map_err(AcceptedActionApplyError::Unaccountable)?;

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
        self.inputs.actions = accounted;
        Ok(())
    }

    /// Applies an action whose only failure modes are refusals this caller has
    /// already ruled out.
    ///
    /// Finite aggregation cannot fail for a non-analog action. Accounting can,
    /// once a tick has been offered `MAX_ACTIONS_PER_TICK` actions -- 65,536
    /// between two ticks, which no hand reaches and no adapter here produces.
    /// Panicking is the fail-closed answer if it ever does: refusing to apply
    /// an action nothing can account for is the point of the limit.
    pub(super) fn apply_infallible_accepted_action(&mut self, action: AcceptedAction) {
        self.apply_accepted_action(action)
            .expect("a non-analog action fails only when one tick is over its action limit");
    }

    pub(super) fn capture_mouse(&mut self, capture: bool) {
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

    pub(super) fn pointer_in_sidebar(&self) -> bool {
        self.cursor_position.is_some_and(|position| {
            position.x >= f64::from(self.compositor.simulation_width())
                && position.x < f64::from(self.compositor.width())
                && position.y >= 0.0
                && position.y < f64::from(self.compositor.height())
        })
    }

    pub(super) fn queue_wheel(&mut self, lines: f64) {
        if !self.mouse_captured && self.pointer_in_sidebar() {
            return;
        }
        if let Err(error) = self.apply_accepted_action(AcceptedAction::WheelLines(lines)) {
            eprintln!("accepted-action-refused\t{error}");
        }
    }

    pub(super) fn resize(&mut self, size: PhysicalSize<u32>) {
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
            display.surface_sized = true;
        }
        let sidebar_width = SIDEBAR_WIDTH.min(size.width.saturating_sub(1));
        let simulation_width = size.width - sidebar_width;
        self.renderer.resize(simulation_width, size.height);
        self.compositor
            .resize(size.width, size.height, simulation_width);
        self.needs_render = true;

        self.inputs.viewport_refit |= was_top;
    }

    pub(super) fn fit_world(&mut self) {
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

    pub(super) fn translate_observer(&mut self, local_delta: Vec3) -> bool {
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

    pub(super) fn dolly_observer(
        &mut self,
        wheel: f64,
        min_distance: f64,
        max_distance: f64,
    ) -> bool {
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

    pub(super) fn sidebar_snapshot(&self, source_tick: u64) -> SidebarSnapshot {
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
    pub(super) fn sidebar_display_predecessor(
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
    pub(super) fn sidebar_display_predecessor(
        &self,
    ) -> (Option<bitblock::protocol::ContentTag>, u64, DisplayStatus) {
        if let Some(status) = self.display_predecessor_override {
            return (None, 0, status);
        }
        (None, 0, DisplayStatus::Unsupported)
    }

    pub(super) fn fixed_tick(&mut self) -> bool {
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
        if self.world.revision() != self.verified_world_revision
            || pre_world != self.verified_world_tag
        {
            assert_eq!(
                world_tag(&self.world),
                pre_world,
                "live world diverged from coordinate-history identity before the fixed tick"
            );
            self.verified_world_revision = self.world.revision();
            self.verified_world_tag = pre_world;
        }
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

        let consumed_actions = std::mem::take(&mut self.inputs.actions);
        let pending_edits = std::mem::take(&mut self.inputs.edits);
        let attempts = pending_edits.len();
        let mut outcome_attempts = Vec::with_capacity(attempts);
        let mut transaction = self.history.begin_tick(self.tick);
        for pending in pending_edits {
            let attempt = admit_pending_edit(&self.world, &self.camera, pending);
            if let Some(edit) = attempt.executed_edit() {
                self.needs_render |= transaction.apply_edit(&self.history, &mut self.world, edit);
            }
            outcome_attempts.push(attempt);
        }

        let batch = TickBatch::new(self.tick, attempts, transaction.edits().iter().copied());
        let post_world = match self.history.commit_tick(transaction, &self.world) {
            Some(receipt) => receipt.post_world(),
            None => pre_world,
        };
        self.verified_world_revision = self.world.revision();
        self.verified_world_tag = post_world;
        self.last_tick_batch = Some(batch);
        debug_assert_eq!(
            world_tag(&self.world),
            post_world,
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
            let (game_frame, palette_audit) = self.renderer.observe_final_game_frame();
            assert!(
                palette_audit.is_valid(),
                "renderer produced {} illegal final RGB colors; first={:?}",
                palette_audit.illegal_rgb_count(),
                palette_audit.first_illegal(),
            );
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
            let sidebar_facts = if game_rendered {
                self.compositor
                    .compose(self.renderer.pixels(), game.1.tag, &snapshot)
            } else {
                self.compositor.compose_sidebar_only(game.1.tag, &snapshot)
            };
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
            consumed_actions,
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

    pub(super) fn advance_ticks(&mut self, now: Instant) -> bool {
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

    pub(super) fn present(&mut self) {
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
        if !display.surface_sized {
            return;
        }
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
        let submitted = match submit_live_frame_pair(
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
        ) {
            Ok(pair) => pair,
            Err(error) => {
                eprintln!("live-present-failed\t{error:?}");
                return;
            }
        };
        #[cfg(not(windows))]
        let submitted = match submit_live_frame_pair(
            &self.tick_outcomes,
            &mut self.frame_ledger,
            &mut self.output_frame_ledger,
            self.frame_ledgers_v2
                .as_mut()
                .expect("v2 frame ledgers cannot disappear during one presentation"),
            game_pending,
            output_pending,
            |_| buffer.present(),
        ) {
            Ok(pair) => pair,
            Err(error) => {
                eprintln!("live-present-failed\t{error:?}");
                return;
            }
        };
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

    pub(super) fn request_display_artifact_capture(&mut self) {
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

    pub(super) fn request_presentation_output_capture(&mut self) {
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

    pub(super) fn key(&mut self, code: KeyCode, pressed: bool) {
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
            self.display = Some(Display {
                surface,
                window,
                surface_sized: false,
            });
        }
        #[cfg(not(windows))]
        {
            self.display = Some(Display {
                surface,
                _context: context,
                window,
                surface_sized: false,
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

pub(super) fn resolve_aim_edit(world: &World, camera: &Camera, edit: AimEdit) -> Option<WorldEdit> {
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

/// Resolves observer-local policy to one disposition before the world reducer sees it.
///
/// Camera and viewport state stop at this boundary. Only [`TickAttempt::executed_edit`]
/// may cross into [`bitblock::history::TickTransaction::apply_edit`], whose input is one
/// exact integer-coordinate edit.
pub(super) fn admit_pending_edit(
    world: &World,
    camera: &Camera,
    pending: PendingEdit,
) -> TickAttempt {
    match pending {
        PendingEdit::Aim(AimEdit::Delete) => {
            let Some(edit) = resolve_aim_edit(world, camera, AimEdit::Delete) else {
                return TickAttempt::AimDeleteUnresolved;
            };
            assert!(
                !edit_would_enclose_observer(world, camera, edit),
                "an aimed Off edit cannot enclose the observer"
            );
            TickAttempt::AimDeleteExecuted(edit)
        }
        PendingEdit::Aim(AimEdit::Place) => {
            let Some(edit) = resolve_aim_edit(world, camera, AimEdit::Place) else {
                return TickAttempt::AimPlaceUnresolved;
            };
            if edit_would_enclose_observer(world, camera, edit) {
                TickAttempt::AimPlaceRejectedObserver(edit)
            } else {
                TickAttempt::AimPlaceExecuted(edit)
            }
        }
        PendingEdit::Exact(edit) => {
            if edit_would_enclose_observer(world, camera, edit) {
                TickAttempt::ExactRejectedObserver(edit)
            } else {
                TickAttempt::ExactExecuted(edit)
            }
        }
    }
}

pub(super) fn collision_limited_delta(world: &World, start: Vec3, requested: Vec3) -> Vec3 {
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

pub(super) fn observer_is_clear(world: &World, position: Vec3) -> bool {
    sweep_point(world, position, Vec3::ZERO).is_none()
}

pub(super) fn edit_would_enclose_observer(world: &World, camera: &Camera, edit: WorldEdit) -> bool {
    let turns_on = match edit.operation {
        Operation::On => !world.is_on(edit.coord),
        Operation::Toggle => !world.is_on(edit.coord),
        Operation::Off => false,
    };
    turns_on && coord_containing(camera.position()) == Some(edit.coord)
}

#[cfg(test)]
pub(super) fn apply_aim_edit(world: &mut World, camera: &Camera, edit: AimEdit) -> bool {
    resolve_aim_edit(world, camera, edit)
        .filter(|edit| !edit_would_enclose_observer(world, camera, *edit))
        .is_some_and(|edit| edit.apply(world))
}

pub(super) fn axis(positive: bool, negative: bool) -> f64 {
    f64::from(positive as u8) - f64::from(negative as u8)
}
