//! Command-line startup modes, bounded file reads, and display-journal CLI verification.

use super::*;

pub(super) enum StartupMode {
    Observe(Program),
    Simulate(InteractionTape),
    #[cfg(all(windows, feature = "protocol-research"))]
    VerifyDisplayJournal {
        path: PathBuf,
        required_ancestor: ContentTag,
    },
}

#[cfg(all(windows, feature = "protocol-research"))]
pub(super) fn configure_display_artifact_journal(app: &mut App) -> Result<(), Box<dyn Error>> {
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
pub(super) fn configure_display_artifact_journal(_app: &mut App) -> Result<(), Box<dyn Error>> {
    if std::env::var_os(DISPLAY_JOURNAL_ENV).is_some() {
        return Err(io::Error::new(
            io::ErrorKind::Unsupported,
            format!("{DISPLAY_JOURNAL_ENV} requires Windows and --features protocol-research"),
        )
        .into());
    }
    Ok(())
}

pub(super) fn load_startup_mode() -> Result<StartupMode, Box<dyn Error>> {
    let mut arguments = std::env::args_os().skip(1);
    let Some(first) = arguments.next() else {
        return Ok(StartupMode::Observe(Program::new()));
    };

    if first == "--verify-display-journal" {
        #[cfg(all(windows, feature = "protocol-research"))]
        {
            let Some(path) = arguments.next().map(PathBuf::from) else {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "usage: bitblock --verify-display-journal <display-session.bbj> <#bb.storage-record.sha256...>",
                )
                .into());
            };
            let Some(required_text) = arguments.next() else {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "usage: bitblock --verify-display-journal <display-session.bbj> <#bb.storage-record.sha256...>",
                )
                .into());
            };
            if arguments.next().is_some() {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "usage: bitblock --verify-display-journal <display-session.bbj> <#bb.storage-record.sha256...>",
                )
                .into());
            }
            let required_text = required_text.to_str().ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "the required storage-record tag must be UTF-8",
                )
            })?;
            let required_ancestor = ContentTag::parse_full(required_text).map_err(|error| {
                io::Error::new(
                    io::ErrorKind::InvalidInput,
                    format!("invalid required storage-record tag: {error}"),
                )
            })?;
            if required_ancestor.kind() != TagKind::StorageRecord {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    format!(
                        "required journal anchor must be a storage-record tag, received {}",
                        required_ancestor.kind().name()
                    ),
                )
                .into());
            }
            return Ok(StartupMode::VerifyDisplayJournal {
                path,
                required_ancestor,
            });
        }
        #[cfg(not(all(windows, feature = "protocol-research")))]
        {
            return Err(io::Error::new(
                io::ErrorKind::Unsupported,
                "--verify-display-journal requires Windows and --features protocol-research",
            )
            .into());
        }
    }

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
                "usage: bitblock [program.bb] | bitblock --simulate-actions <interaction.tape> | bitblock --verify-display-journal <display-session.bbj> <#bb.storage-record.sha256...>",
        )
        .into());
    }

    let source = read_bounded_utf8(&path, HEADLESS_SIMULATION_MAX_SOURCE_BYTES)?;
    // A startup program's edits are all submitted to the first tick, so a program
    // larger than one tick can account for is refused here rather than panicking
    // later inside the fold.
    let program = Program::parse(&source).map_err(|error| -> Box<dyn Error> {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("{}: {error}", path.display()),
        )
        .into()
    })?;
    if program.edits().len() > MAX_ACTIONS_PER_TICK as usize {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!(
                "{}: {} edits exceed the {} one tick can account for",
                path.display(),
                program.edits().len(),
                MAX_ACTIONS_PER_TICK
            ),
        )
        .into());
    }
    Ok(StartupMode::Observe(program))
}

pub(super) fn read_bounded_utf8(
    path: &std::path::Path,
    maximum: usize,
) -> Result<String, io::Error> {
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

#[cfg(all(windows, feature = "protocol-research"))]
pub(super) fn verify_display_journal(
    path: &std::path::Path,
    required_ancestor: ContentTag,
) -> Result<(), Box<dyn Error>> {
    let bytes = read_bounded_display_journal(path, StorageLimits::DEFAULT.max_stream_bytes())?;
    let inspection = inspect_display_artifact_bytes(
        &bytes,
        StorageLimits::DEFAULT,
        AnchorPolicy::RequireAncestor(required_ancestor),
    );
    match inspection {
        Ok(ReadOnlyDisplayArtifactInspection::Clean { carrier, state }) => {
            print_display_journal_verification(&carrier, &state, required_ancestor);
            Ok(())
        }
        Ok(ReadOnlyDisplayArtifactInspection::NeedsRecovery { carrier, state }) => {
            print_display_journal_recovery(&carrier, &state, required_ancestor);
            Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "display journal has an unverified suffix; no complete verification claim was emitted",
            )
            .into())
        }
        Err(DisplayArtifactStoreError::Journal(
            bitblock::journal::JournalError::RequiredAncestorMissing { observed_head, .. },
        )) => {
            println!(
                "display-journal-verification\tstatus=missing-required-ancestor\tscope=typed-local-replay\tcausal_ancestry=references-only\trequired_ancestor={}\tobserved_head={}",
                required_ancestor,
                optional_tag(observed_head),
            );
            Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "the supplied required storage-record ancestor is absent from the admitted journal prefix",
            )
            .into())
        }
        Err(error) => Err(error.into()),
    }
}

#[cfg(all(windows, feature = "protocol-research"))]
pub(super) fn read_bounded_display_journal(
    path: &std::path::Path,
    maximum: usize,
) -> Result<Vec<u8>, io::Error> {
    let mut file = std::fs::File::open(path)?;
    let before = file.metadata()?;
    if !before.is_file() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "display journal input must be a regular file",
        ));
    }
    let before_len = before.len();
    let maximum_u64 = u64::try_from(maximum).expect("storage byte cap fits u64");
    if before_len > maximum_u64 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("display journal exceeds the bounded {maximum}-byte input policy"),
        ));
    }
    let expected_len = usize::try_from(before_len).map_err(|_| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            "display journal length is not addressable on this host",
        )
    })?;
    let mut bytes = Vec::new();
    bytes.try_reserve_exact(expected_len).map_err(|_| {
        io::Error::new(
            io::ErrorKind::OutOfMemory,
            format!("could not reserve {expected_len} display-journal bytes"),
        )
    })?;
    let limit = maximum_u64
        .checked_add(1)
        .expect("storage byte cap has one-byte headroom");
    (&mut file).take(limit).read_to_end(&mut bytes)?;
    if bytes.len() > maximum {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("display journal exceeds the bounded {maximum}-byte input policy"),
        ));
    }
    let after_len = file.metadata()?.len();
    if after_len != before_len || bytes.len() as u64 != before_len {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!(
                "display journal length changed during read: before {before_len}, bytes {}, after {after_len}",
                bytes.len()
            ),
        ));
    }
    Ok(bytes)
}

#[cfg(all(windows, feature = "protocol-research"))]
pub(super) fn print_display_journal_verification(
    carrier: &bitblock::journal::ScanReport,
    state: &bitblock::display_store::ReplayedDisplayArtifacts,
    required_ancestor: ContentTag,
) {
    println!("{}", display_journal_verification_header(required_ancestor));
    println!(
        "display-journal-inventory\tfile_bytes={}\tvalid_bytes={}\tobjects={}\tlogical_payload_bytes={}\tphysical_record_bytes={}\tstorage_head={}\tdisplay_head={}\tartifact_head={}\tstored_object_head={}",
        carrier.file_bytes,
        carrier.valid_prefix_bytes,
        carrier.valid_record_count,
        state.logical_payload_bytes(),
        state.physical_record_bytes(),
        optional_tag(state.storage_head()),
        optional_tag(state.display_receipt_head()),
        optional_tag(state.artifact_head()),
        optional_tag(state.stored_object_head()),
    );
    for entry in state.entries() {
        println!(
            "display-journal-evidence\tstorage={}\tobject={}\tartifact={}\tdisplay_receipt={}\tframe_receipt={}\ttick_outcome={}\tframe_input={}\texpected_frame={}\tcaptured_frame={}\tpolicy={}\tcapture_outcome={:?}\tcapture_stage={:?}\tmismatches={}\tfirst_mismatch={}\tcaptured_form={:?}\ttiming={}\ttopology_before={}\ttopology_after={}\troute_stability={:?}\tcolor_before={}\tcolor_after={}\treported_color_state={:?}",
            entry.storage_record,
            entry.stored_object,
            entry.artifact,
            entry.display_receipt,
            entry.frame_receipt,
            entry.tick_outcome,
            entry.frame_input,
            entry.expected_frame,
            optional_tag(entry.captured_frame),
            entry.policy,
            entry.capture_outcome,
            entry.capture_stage,
            entry.mismatch_count,
            optional_u64(entry.first_mismatch),
            entry.captured_form,
            optional_tag(entry.timing_artifact),
            optional_tag(entry.topology_before),
            optional_tag(entry.topology_after),
            entry.route_stability,
            optional_tag(entry.color_before),
            optional_tag(entry.color_after),
            entry.reported_color_state,
        );
    }
}

#[cfg(all(windows, feature = "protocol-research"))]
pub(super) fn display_journal_verification_header(required_ancestor: ContentTag) -> String {
    format!(
        "display-journal-verification\tstatus=clean\tscope=typed-local-replay\tpixel_evidence=retained-capture-claim-per-entry\tcausal_ancestry=references-only\tscanout=unobserved\tphysical_light=unobserved\tcustody=local-only\tconsensus=none\trequired_ancestor={required_ancestor}"
    )
}

#[cfg(all(windows, feature = "protocol-research"))]
pub(super) fn print_display_journal_recovery(
    carrier: &bitblock::journal::RecoveryReport,
    state: &bitblock::display_store::ReplayedDisplayArtifacts,
    required_ancestor: ContentTag,
) {
    println!(
        "display-journal-verification\tstatus=needs-recovery\tscope=typed-valid-prefix-only\tpixel_evidence=unverified-suffix-excluded\tcausal_ancestry=references-only\trequired_ancestor={}\tvalid_bytes={}\trejected_tail_bytes={}\trejected_tail={}\trejection_class={:?}\tvalid_head={}",
        required_ancestor,
        carrier.valid_prefix_bytes,
        carrier.rejected_tail_bytes,
        carrier.rejected_tail_tag,
        carrier.rejection_class,
        optional_tag(carrier.valid_head),
    );
    println!(
        "display-journal-prefix\tobjects={}\tstorage_head={}\tdisplay_head={}\tartifact_head={}\tstored_object_head={}",
        state.len(),
        optional_tag(state.storage_head()),
        optional_tag(state.display_receipt_head()),
        optional_tag(state.artifact_head()),
        optional_tag(state.stored_object_head()),
    );
}

#[cfg(all(windows, feature = "protocol-research"))]
pub(super) fn optional_u64(value: Option<u64>) -> String {
    value.map_or_else(|| "none".to_owned(), |value| value.to_string())
}
