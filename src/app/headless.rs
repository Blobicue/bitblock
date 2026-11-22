//! Headless tape preflight, deterministic simulation, executable fingerprinting, and summaries.

use super::*;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct HeadlessTickSummary {
    pub(super) ordinal: u64,
    pub(super) outcome: ContentTag,
    pub(super) post_world: ContentTag,
    pub(super) post_history: ContentTag,
    pub(super) camera_bits: [u64; 10],
    pub(super) game_input: Option<ContentTag>,
    pub(super) game_frame: Option<ContentTag>,
    pub(super) composite_input: Option<ContentTag>,
    pub(super) composite_frame: Option<ContentTag>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct HeadlessSimulationSummary {
    pub(super) tape: ContentTag,
    pub(super) action_count: usize,
    pub(super) ticks: Vec<HeadlessTickSummary>,
    pub(super) final_world: ContentTag,
    pub(super) final_history: ContentTag,
    pub(super) final_camera_bits: [u64; 10],
    pub(super) on_bits: usize,
    pub(super) coordinate_receipts: usize,
    pub(super) game_admissions: usize,
    pub(super) composite_admissions: usize,
    pub(super) game_receipt_head: Option<ContentTag>,
    pub(super) composite_receipt_head: Option<ContentTag>,
    #[cfg(feature = "protocol-research")]
    pub(super) local_replay_endpoint: LocalReplayEndpoint,
}
pub(super) fn headless_pixel_shape(width: u32, height: u32) -> Result<(u128, u128), io::Error> {
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

pub(super) fn checked_headless_charge(
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

pub(super) fn headless_possible_world_bits(
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

pub(super) type HeadlessChunkKey = (i64, i64, i64);

pub(super) fn headless_chunk_key(coord: Coord) -> HeadlessChunkKey {
    (
        coord.x.div_euclid(CHUNK_SIDE),
        coord.y.div_euclid(CHUNK_SIDE),
        coord.z.div_euclid(CHUNK_SIDE),
    )
}

pub(super) fn headless_insert_possible_exact(
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

pub(super) fn headless_remove_possible_exact(
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

pub(super) fn headless_possible_chunk_count(
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

pub(super) fn headless_chunk_word_units(possible_chunks: u128) -> Result<u128, io::Error> {
    possible_chunks
        .checked_mul(CHUNK_WORDS as u128)
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "chunk-word charge overflow"))
}

pub(super) fn headless_world_ordering_units(population: u128) -> Result<u128, io::Error> {
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
pub(super) fn preflight_headless_tape(tape: &InteractionTape) -> Result<(), io::Error> {
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
/// The fingerprint of the running executable, computed once for this process.
///
/// The fingerprint names the bytes of the executable that emitted a carrier. Within one
/// process that is a constant, so re-reading and re-hashing the file per call cannot make
/// the evidence more accurate. It can only make one run's evidence inconsistent with itself
/// if the file is replaced mid-run, so computing it once is the stronger reading of what the
/// fingerprint claims rather than a weaker one.
///
/// The mid-read integrity check in [`fingerprint_running_executable`] is unchanged and still
/// runs: the length is compared before and after hashing, so a file replaced *during* the
/// read is still refused. What is no longer detected is a replacement between two separate
/// verifications inside one process, which the evidence never claimed to cover.
///
/// The cost this removes: `verify_local_replay_evidence` streams the whole executable, and a
/// test calling it six times hashed 27MB six times -- 6.45s of an 8s verification run.
#[cfg(feature = "protocol-research")]
pub(super) fn headless_local_replay_build() -> Result<LocalReplayBuild, io::Error> {
    // io::Error is not Clone, so the kind and message are stored and rebuilt. A failure is
    // remembered like a success: an executable this process cannot fingerprint now is one it
    // cannot fingerprint later either, and retrying would re-read 27MB only to fail again.
    static FINGERPRINT: OnceLock<Result<LocalReplayBuild, (io::ErrorKind, String)>> =
        OnceLock::new();
    match FINGERPRINT.get_or_init(|| {
        fingerprint_running_executable().map_err(|error| (error.kind(), error.to_string()))
    }) {
        Ok(build) => Ok(*build),
        Err((kind, message)) => Err(io::Error::new(*kind, message.clone())),
    }
}

#[cfg(feature = "protocol-research")]
pub(super) fn fingerprint_running_executable() -> Result<LocalReplayBuild, io::Error> {
    let executable = std::env::current_exe()?;
    let mut file = std::fs::File::open(&executable)?;
    let before = file.metadata()?;
    if !before.is_file() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!(
                "current local replay executable is not a regular file: {}",
                executable.display()
            ),
        ));
    }
    let build = LocalReplayBuild::fingerprint_reader(
        &mut file,
        before.len(),
        HEADLESS_LOCAL_REPLAY_MAX_EXECUTABLE_BYTES,
    )
    .map_err(|error| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("could not fingerprint local replay executable: {error}"),
        )
    })?;
    let after = file.metadata()?;
    if !after.is_file() || after.len() != before.len() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!(
                "local replay executable changed while fingerprinting: {}",
                executable.display()
            ),
        ));
    }
    Ok(build)
}

#[cfg(feature = "protocol-research")]
pub(super) fn headless_local_replay_context() -> Result<LocalReplayContext, io::Error> {
    LocalReplayContext::new(
        headless_local_replay_build()?,
        headless_local_replay_genesis_policy_tag(),
        headless_local_replay_rules_policy_tag(),
        headless_local_replay_admission_policy_tag(),
    )
    .map_err(|error| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("could not construct local replay context: {error}"),
        )
    })
}

#[cfg(feature = "protocol-research")]
pub(super) fn simulate_interaction_tape_with_local_replay_evidence(
    tape: &InteractionTape,
) -> Result<(HeadlessSimulationSummary, LocalReplayEvidence), io::Error> {
    // Capture the build and declared policies before the fold. This is a local binding, not
    // source/build provenance or a claim that the executable cannot change concurrently.
    let context = headless_local_replay_context()?;
    let summary = simulate_interaction_tape(tape)?;
    let evidence = LocalReplayEvidence::new(context, tape.clone(), summary.local_replay_endpoint)
        .map_err(|error| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("could not construct local replay evidence: {error}"),
        )
    })?;
    evidence.verify().map_err(|error| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("constructed local replay evidence did not verify: {error}"),
        )
    })?;
    Ok((summary, evidence))
}

#[cfg(all(test, feature = "protocol-research"))]
pub(super) fn verify_local_replay_evidence(
    evidence: &LocalReplayEvidence,
) -> Result<HeadlessSimulationSummary, io::Error> {
    evidence.verify().map_err(|error| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("local replay evidence structural verification failed: {error}"),
        )
    })?;
    let current_context = headless_local_replay_context()?;
    if evidence.context() != current_context {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "local replay evidence context differs from this executable or its declared headless policies",
        ));
    }
    let summary = simulate_interaction_tape(evidence.tape())?;
    let expected = LocalReplayEvidence::new(
        current_context,
        evidence.tape().clone(),
        summary.local_replay_endpoint,
    )
    .map_err(|error| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("could not reconstruct local replay evidence: {error}"),
        )
    })?;
    if expected != *evidence {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "local replay evidence endpoint differs from replay of its embedded action tape",
        ));
    }
    Ok(summary)
}

pub(super) fn simulate_interaction_tape(
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
    #[cfg(feature = "protocol-research")]
    let mut latest_game = None;
    #[cfg(feature = "protocol-research")]
    let mut latest_composite = None;

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
        #[cfg(feature = "protocol-research")]
        if let Some(game) = game {
            latest_game = Some(
                ReplayFrameCommitment::new(game.input(), game.frame())
                    .expect("fixed tick game commitment has canonical nested kinds"),
            );
        }
        #[cfg(feature = "protocol-research")]
        if let Some(composite) = composite {
            latest_composite = Some(
                ReplayFrameCommitment::new(composite.input(), composite.frame())
                    .expect("fixed tick composite commitment has canonical nested kinds"),
            );
        }
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

    // `commit_tick` derives this root from an independent coordinate scan. No mutation follows
    // the fold, so the already-committed root is the canonical final-world identity for both
    // consumers below. The per-tick world_tag checks remain the divergence guard.
    let final_world = app.history.current_world();

    #[cfg(feature = "protocol-research")]
    let local_replay_endpoint = LocalReplayEndpoint::new(
        app.tick_outcomes.last().map(TickOutcome::tag),
        final_world,
        app.history.head(),
        TickCameraState::capture(&app.camera).bits(),
        app.renderer.width(),
        app.renderer.height(),
        app.compositor.width(),
        app.compositor.height(),
        app.sidebar_visible,
        latest_game,
        latest_composite,
    )
    .expect("headless fixed fold produces a structurally valid replay endpoint");

    Ok(HeadlessSimulationSummary {
        tape: tape.content_tag(),
        action_count: tape.total_actions(),
        ticks,
        final_world,
        final_history: app.history.head(),
        final_camera_bits: TickCameraState::capture(&app.camera).bits(),
        on_bits: app.world.len(),
        coordinate_receipts: app.history.receipts().len(),
        game_admissions,
        composite_admissions,
        game_receipt_head,
        composite_receipt_head,
        #[cfg(feature = "protocol-research")]
        local_replay_endpoint,
    })
}

pub(super) fn optional_tag(tag: Option<ContentTag>) -> String {
    tag.map_or_else(|| "none".to_owned(), |tag| tag.to_string())
}

pub(super) fn camera_bits_text(bits: [u64; 10]) -> String {
    bits.into_iter()
        .map(|bits| format!("{bits:016x}"))
        .collect::<Vec<_>>()
        .join(",")
}

pub(super) fn print_headless_summary(summary: &HeadlessSimulationSummary) {
    #[cfg(feature = "protocol-research")]
    println!(
        "interaction-simulation\ttape={}\tticks={}\tactions={}\tscope=fixed-tick-projection\trun_receipt=separate-local-replay-evidence-line",
        summary.tape,
        summary.ticks.len(),
        summary.action_count,
    );
    #[cfg(not(feature = "protocol-research"))]
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

/// Reports the in-memory carrier emitted for a research-enabled action simulation.
///
/// The line intentionally names only local replay bindings. It does not turn the carrier into
/// durable storage or attribute it to a display, clock, remote peer, owner, or consensus rule.
#[cfg(feature = "protocol-research")]
pub(super) fn print_local_replay_evidence(evidence: &LocalReplayEvidence) {
    let context = evidence.context();
    let endpoint = evidence.endpoint();
    println!(
        "interaction-local-replay-evidence\ttag={}\tcarrier_bytes={}\tcarrier=in-memory-only\tscope=canonical-local-replay\tbuild={}\tgenesis_policy={}\trules_policy={}\tadmission_policy={}\ttape={}\toutcome={}\tworld={}\thistory={}\tgame_frame={}\tcomposite_frame={}\tnot_claimed=persistence,custody,signature,ownership,consensus,clock,display,scanout,light",
        evidence.tag(),
        evidence.encoded_len(),
        context.build().tag(),
        context.genesis_policy(),
        context.rules_policy(),
        context.admission_policy(),
        evidence.tape().content_tag(),
        optional_tag(endpoint.outcome()),
        endpoint.world(),
        endpoint.history(),
        optional_tag(endpoint.game().map(ReplayFrameCommitment::frame)),
        optional_tag(endpoint.composite().map(ReplayFrameCommitment::frame)),
    );
}
