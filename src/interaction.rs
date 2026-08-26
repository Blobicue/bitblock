//! Canonical, tick-indexed accepted interaction tapes.
//!
//! An [`InteractionTape`] records only actions accepted by the application, grouped by the
//! simulation tick that consumes them. Tick ordinals are implicit in the text form and must be
//! the contiguous sequence `0..record_count` in the canonical binary form. This makes one tape a
//! deterministic replay input rather than a transcript of platform-specific input events.

use crate::program::{EDIT_CANONICAL_LEN, Edit, EditCodecError, Operation};
use crate::protocol::{ContentTag, TagKind, hash_canonical};
use crate::world::Coord;
use std::fmt;
use std::str::FromStr;

/// Schema bound into every canonical interaction-tape tag.
pub const INTERACTION_TAPE_SCHEMA: &[u8] = b"bitblock/interaction-tape/v1";

/// Maximum canonical payload accepted by the bounded decoder.
pub const MAX_INTERACTION_TAPE_PAYLOAD_BYTES: usize = 16 * 1024 * 1024;
/// Maximum number of tick records in one tape.
pub const MAX_INTERACTION_TAPE_RECORDS: usize = 16_384;
/// Maximum number of accepted actions in one tick record.
pub const MAX_INTERACTION_ACTIONS_PER_RECORD: usize = 65_536;
/// Maximum number of accepted actions across one tape.
pub const MAX_INTERACTION_TOTAL_ACTIONS: usize = 262_144;

const PAYLOAD_HEADER_LEN: usize = 2 * size_of::<u64>();
const RECORD_HEADER_LEN: usize = size_of::<u64>() + size_of::<u32>();

const OPCODE_MOVE_EDGE: u8 = 0;
const OPCODE_LOOK_DELTA: u8 = 1;
const OPCODE_WHEEL_LINES: u8 = 2;
const OPCODE_AIM_DELETE: u8 = 3;
const OPCODE_AIM_PLACE: u8 = 4;
const OPCODE_EXACT_EDIT: u8 = 5;
const OPCODE_FIT_WORLD: u8 = 6;
const OPCODE_TOGGLE_SIDEBAR: u8 = 7;
const OPCODE_RESIZE: u8 = 8;
const OPCODE_CLEAR_MOTION: u8 = 9;

/// One edge-triggered movement control.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum MoveControl {
    Forward,
    Backward,
    Left,
    Right,
    Up,
    Down,
}

impl MoveControl {
    /// The canonical one-byte code embedded after a move-edge opcode.
    pub const fn canonical_code(self) -> u8 {
        match self {
            Self::Forward => 0,
            Self::Backward => 1,
            Self::Left => 2,
            Self::Right => 3,
            Self::Up => 4,
            Self::Down => 5,
        }
    }

    /// Decodes the complete canonical movement-control code set.
    pub const fn from_canonical_code(code: u8) -> Option<Self> {
        match code {
            0 => Some(Self::Forward),
            1 => Some(Self::Backward),
            2 => Some(Self::Left),
            3 => Some(Self::Right),
            4 => Some(Self::Up),
            5 => Some(Self::Down),
            _ => None,
        }
    }

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Forward => "forward",
            Self::Backward => "backward",
            Self::Left => "left",
            Self::Right => "right",
            Self::Up => "up",
            Self::Down => "down",
        }
    }

    fn parse(token: &str) -> Option<Self> {
        Some(match token {
            "forward" => Self::Forward,
            "backward" => Self::Backward,
            "left" => Self::Left,
            "right" => Self::Right,
            "up" => Self::Up,
            "down" => Self::Down,
            _ => return None,
        })
    }
}

impl fmt::Display for MoveControl {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

/// One action accepted by the application for execution at a tick boundary.
///
/// Floating-point lanes are compared by their exact IEEE-754 bits. In particular, `0.0` and
/// `-0.0` remain distinct replay inputs. Canonical tapes admit only finite values.
#[derive(Clone, Copy, Debug)]
pub enum AcceptedAction {
    MoveEdge(MoveControl, bool),
    LookDelta(f64, f64),
    WheelLines(f64),
    AimDelete,
    AimPlace,
    ExactEdit(Edit),
    FitWorld,
    ToggleSidebar,
    Resize(u32, u32),
    ClearMotion,
}

impl AcceptedAction {
    /// The canonical opcode that begins this action.
    pub const fn canonical_code(self) -> u8 {
        match self {
            Self::MoveEdge(..) => OPCODE_MOVE_EDGE,
            Self::LookDelta(..) => OPCODE_LOOK_DELTA,
            Self::WheelLines(..) => OPCODE_WHEEL_LINES,
            Self::AimDelete => OPCODE_AIM_DELETE,
            Self::AimPlace => OPCODE_AIM_PLACE,
            Self::ExactEdit(..) => OPCODE_EXACT_EDIT,
            Self::FitWorld => OPCODE_FIT_WORLD,
            Self::ToggleSidebar => OPCODE_TOGGLE_SIDEBAR,
            Self::Resize(..) => OPCODE_RESIZE,
            Self::ClearMotion => OPCODE_CLEAR_MOTION,
        }
    }

    const fn canonical_len(self) -> usize {
        1 + match self {
            Self::MoveEdge(..) => 2,
            Self::LookDelta(..) => 2 * size_of::<u64>(),
            Self::WheelLines(..) => size_of::<u64>(),
            Self::ExactEdit(..) => EDIT_CANONICAL_LEN,
            Self::Resize(..) => 2 * size_of::<u32>(),
            Self::AimDelete
            | Self::AimPlace
            | Self::FitWorld
            | Self::ToggleSidebar
            | Self::ClearMotion => 0,
        }
    }

    fn is_canonical(self) -> bool {
        match self {
            Self::LookDelta(horizontal, vertical) => horizontal.is_finite() && vertical.is_finite(),
            Self::WheelLines(lines) => lines.is_finite(),
            _ => true,
        }
    }

    fn encode_canonical_into(self, sink: &mut dyn FnMut(&[u8])) {
        sink(&[self.canonical_code()]);
        match self {
            Self::MoveEdge(control, down) => {
                sink(&[control.canonical_code()]);
                sink(&[u8::from(down)]);
            }
            Self::LookDelta(horizontal, vertical) => {
                sink(&horizontal.to_bits().to_be_bytes());
                sink(&vertical.to_bits().to_be_bytes());
            }
            Self::WheelLines(lines) => sink(&lines.to_bits().to_be_bytes()),
            Self::ExactEdit(edit) => sink(&edit.encode_canonical()),
            Self::Resize(width, height) => {
                sink(&width.to_be_bytes());
                sink(&height.to_be_bytes());
            }
            Self::AimDelete
            | Self::AimPlace
            | Self::FitWorld
            | Self::ToggleSidebar
            | Self::ClearMotion => {}
        }
    }
}

impl PartialEq for AcceptedAction {
    fn eq(&self, other: &Self) -> bool {
        match (*self, *other) {
            (Self::MoveEdge(a_control, a_down), Self::MoveEdge(b_control, b_down)) => {
                a_control == b_control && a_down == b_down
            }
            (Self::LookDelta(a_x, a_y), Self::LookDelta(b_x, b_y)) => {
                a_x.to_bits() == b_x.to_bits() && a_y.to_bits() == b_y.to_bits()
            }
            (Self::WheelLines(a), Self::WheelLines(b)) => a.to_bits() == b.to_bits(),
            (Self::AimDelete, Self::AimDelete)
            | (Self::AimPlace, Self::AimPlace)
            | (Self::FitWorld, Self::FitWorld)
            | (Self::ToggleSidebar, Self::ToggleSidebar)
            | (Self::ClearMotion, Self::ClearMotion) => true,
            (Self::ExactEdit(a), Self::ExactEdit(b)) => a == b,
            (Self::Resize(a_width, a_height), Self::Resize(b_width, b_height)) => {
                a_width == b_width && a_height == b_height
            }
            _ => false,
        }
    }
}

impl Eq for AcceptedAction {}

impl fmt::Display for AcceptedAction {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {
            Self::MoveEdge(control, down) => write!(
                formatter,
                "move {control} {}",
                if down { "down" } else { "up" }
            ),
            Self::LookDelta(horizontal, vertical) => {
                write!(formatter, "look {horizontal} {vertical}")
            }
            Self::WheelLines(lines) => write!(formatter, "wheel {lines}"),
            Self::AimDelete => formatter.write_str("delete"),
            Self::AimPlace => formatter.write_str("place"),
            Self::ExactEdit(edit) => write!(formatter, "exact {edit}"),
            Self::FitWorld => formatter.write_str("fit"),
            Self::ToggleSidebar => formatter.write_str("sidebar"),
            Self::Resize(width, height) => write!(formatter, "resize {width} {height}"),
            Self::ClearMotion => formatter.write_str("clear-motion"),
        }
    }
}

/// One contiguous, zero-based tick and the ordered actions accepted for it.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TickRecord {
    ordinal: u64,
    actions: Vec<AcceptedAction>,
}

impl TickRecord {
    /// The tick ordinal. A canonical tape always has `ordinal == record index`.
    pub const fn ordinal(&self) -> u64 {
        self.ordinal
    }

    /// Accepted actions in their exact replay order.
    pub fn actions(&self) -> &[AcceptedAction] {
        &self.actions
    }

    pub const fn len(&self) -> usize {
        self.actions.len()
    }

    pub const fn is_empty(&self) -> bool {
        self.actions.is_empty()
    }
}

/// A bounded sequence of contiguous tick records suitable for exact replay.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct InteractionTape {
    records: Vec<TickRecord>,
    total_actions: usize,
}

impl InteractionTape {
    pub const fn new() -> Self {
        Self {
            records: Vec::new(),
            total_actions: 0,
        }
    }

    /// Parses the line-oriented interaction language.
    pub fn parse(source: &str) -> Result<Self, InteractionTapeParseError> {
        source.parse()
    }

    pub fn records(&self) -> &[TickRecord] {
        &self.records
    }

    /// Number of tick records, including empty ticks.
    pub const fn len(&self) -> usize {
        self.records.len()
    }

    pub const fn is_empty(&self) -> bool {
        self.records.is_empty()
    }

    pub const fn total_actions(&self) -> usize {
        self.total_actions
    }

    /// Encodes the complete canonical payload using fixed-width big-endian fields.
    pub fn encode_canonical(&self) -> Vec<u8> {
        let payload_len = self.canonical_payload_len();
        let mut bytes = Vec::with_capacity(payload_len);
        self.encode_canonical_into(&mut |chunk| bytes.extend_from_slice(chunk));
        debug_assert_eq!(bytes.len(), payload_len);
        bytes
    }

    /// Strictly decodes one complete bounded canonical payload.
    pub fn decode_canonical(bytes: &[u8]) -> Result<Self, InteractionTapeCodecError> {
        if bytes.len() > MAX_INTERACTION_TAPE_PAYLOAD_BYTES {
            return Err(InteractionTapeCodecError::PayloadTooLarge {
                actual: bytes.len(),
                maximum: MAX_INTERACTION_TAPE_PAYLOAD_BYTES,
            });
        }

        let mut decoder = Decoder::new(bytes);
        let declared_record_count = decoder.read_u64("record count")?;
        let declared_total_actions = decoder.read_u64("total action count")?;

        if declared_record_count > MAX_INTERACTION_TAPE_RECORDS as u64 {
            return Err(InteractionTapeCodecError::RecordCountLimit {
                declared: declared_record_count,
                maximum: MAX_INTERACTION_TAPE_RECORDS,
            });
        }
        if declared_total_actions > MAX_INTERACTION_TOTAL_ACTIONS as u64 {
            return Err(InteractionTapeCodecError::TotalActionCountLimit {
                declared: declared_total_actions,
                maximum: MAX_INTERACTION_TOTAL_ACTIONS,
            });
        }

        let record_count = usize::try_from(declared_record_count)
            .expect("the protocol record cap fits every supported usize");
        let minimum_len = PAYLOAD_HEADER_LEN
            .checked_add(
                record_count
                    .checked_mul(RECORD_HEADER_LEN)
                    .expect("bounded record headers fit usize"),
            )
            .expect("bounded record headers fit usize");
        if minimum_len > bytes.len() {
            return Err(InteractionTapeCodecError::RecordHeadersCannotFit {
                declared: record_count,
                minimum_bytes: minimum_len,
                actual_bytes: bytes.len(),
            });
        }

        let mut records = Vec::new();
        records.try_reserve_exact(record_count).map_err(|_| {
            InteractionTapeCodecError::AllocationRefused {
                subject: "tick records",
                requested: record_count,
            }
        })?;

        let mut observed_total_actions = 0_usize;
        for record_index in 0..record_count {
            let ordinal = decoder.read_u64("tick ordinal")?;
            let expected_ordinal = record_index as u64;
            if ordinal != expected_ordinal {
                return Err(InteractionTapeCodecError::NonContiguousOrdinal {
                    record_index,
                    expected: expected_ordinal,
                    actual: ordinal,
                });
            }

            let declared_action_count = decoder.read_u32("record action count")? as usize;
            if declared_action_count > MAX_INTERACTION_ACTIONS_PER_RECORD {
                return Err(InteractionTapeCodecError::ActionCountLimit {
                    record_index,
                    declared: declared_action_count as u64,
                    maximum: MAX_INTERACTION_ACTIONS_PER_RECORD,
                });
            }
            observed_total_actions = observed_total_actions
                .checked_add(declared_action_count)
                .ok_or(InteractionTapeCodecError::TotalActionCountOverflow)?;
            if observed_total_actions > MAX_INTERACTION_TOTAL_ACTIONS {
                return Err(InteractionTapeCodecError::ObservedTotalActionCountLimit {
                    observed: observed_total_actions,
                    maximum: MAX_INTERACTION_TOTAL_ACTIONS,
                });
            }
            if observed_total_actions as u64 > declared_total_actions {
                return Err(InteractionTapeCodecError::TotalActionCountMismatch {
                    declared: declared_total_actions,
                    observed: observed_total_actions,
                });
            }

            // Every action needs at least its opcode. Refuse an impossible count before asking
            // the allocator for attacker-controlled capacity.
            if declared_action_count > decoder.remaining() {
                return Err(InteractionTapeCodecError::ActionBytesCannotFit {
                    record_index,
                    declared: declared_action_count,
                    remaining_bytes: decoder.remaining(),
                });
            }
            let mut actions = Vec::new();
            actions
                .try_reserve_exact(declared_action_count)
                .map_err(|_| InteractionTapeCodecError::AllocationRefused {
                    subject: "accepted actions",
                    requested: declared_action_count,
                })?;

            for action_index in 0..declared_action_count {
                actions.push(decode_action(&mut decoder, record_index, action_index)?);
            }
            records.push(TickRecord { ordinal, actions });
        }

        if observed_total_actions as u64 != declared_total_actions {
            return Err(InteractionTapeCodecError::TotalActionCountMismatch {
                declared: declared_total_actions,
                observed: observed_total_actions,
            });
        }
        if decoder.remaining() != 0 {
            return Err(InteractionTapeCodecError::TrailingBytes {
                count: decoder.remaining(),
            });
        }

        let tape = Self {
            records,
            total_actions: observed_total_actions,
        };
        if tape.encode_canonical().as_slice() != bytes {
            return Err(InteractionTapeCodecError::NonCanonicalEncoding);
        }
        Ok(tape)
    }

    /// Names the exact canonical payload under the intent kind and interaction-tape schema.
    pub fn content_tag(&self) -> ContentTag {
        let payload_len = self.canonical_payload_len();
        hash_canonical(
            TagKind::Intent,
            INTERACTION_TAPE_SCHEMA,
            payload_len as u64,
            |sink| self.encode_canonical_into(sink),
        )
        .expect("a bounded interaction tape emits its declared canonical length")
    }

    fn canonical_payload_len(&self) -> usize {
        PAYLOAD_HEADER_LEN
            + self
                .records
                .iter()
                .map(|record| {
                    RECORD_HEADER_LEN
                        + record
                            .actions
                            .iter()
                            .copied()
                            .map(AcceptedAction::canonical_len)
                            .sum::<usize>()
                })
                .sum::<usize>()
    }

    fn encode_canonical_into(&self, sink: &mut dyn FnMut(&[u8])) {
        sink(&(self.records.len() as u64).to_be_bytes());
        sink(&(self.total_actions as u64).to_be_bytes());
        for record in &self.records {
            sink(&record.ordinal.to_be_bytes());
            sink(&(record.actions.len() as u32).to_be_bytes());
            for action in &record.actions {
                action.encode_canonical_into(sink);
            }
        }
    }
}

impl fmt::Display for InteractionTape {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut first_line = true;
        for record in &self.records {
            if !first_line {
                formatter.write_str("\n")?;
            }
            formatter.write_str("tick")?;
            first_line = false;
            for action in &record.actions {
                formatter.write_str("\n")?;
                action.fmt(formatter)?;
            }
        }
        Ok(())
    }
}

impl FromStr for InteractionTape {
    type Err = InteractionTapeParseError;

    fn from_str(source: &str) -> Result<Self, Self::Err> {
        let mut records = Vec::new();
        let mut current_actions: Option<Vec<AcceptedAction>> = None;
        let mut total_actions = 0_usize;

        for (line_index, source_line) in source.lines().enumerate() {
            let line = line_index + 1;
            let instruction = source_line
                .split_once('#')
                .map_or(source_line, |(before_comment, _)| before_comment)
                .trim();
            if instruction.is_empty() {
                continue;
            }

            let command = instruction
                .split_whitespace()
                .next()
                .expect("a nonempty instruction has a command");
            if command == "tick" {
                require_field_count(instruction, line, command, 1)?;
                let started_records = records.len() + usize::from(current_actions.is_some());
                if started_records >= MAX_INTERACTION_TAPE_RECORDS {
                    return Err(InteractionTapeParseError::new(
                        line,
                        InteractionTapeParseErrorKind::RecordCountLimit {
                            maximum: MAX_INTERACTION_TAPE_RECORDS,
                        },
                    ));
                }
                if let Some(actions) = current_actions.take() {
                    push_parsed_record(&mut records, actions, line)?;
                }
                current_actions = Some(Vec::new());
                continue;
            }

            if !is_action_command(command) {
                return Err(InteractionTapeParseError::new(
                    line,
                    InteractionTapeParseErrorKind::UnknownCommand(command.to_owned()),
                ));
            }
            let actions = current_actions.as_mut().ok_or_else(|| {
                InteractionTapeParseError::new(
                    line,
                    InteractionTapeParseErrorKind::ActionBeforeTick(command.to_owned()),
                )
            })?;
            if actions.len() >= MAX_INTERACTION_ACTIONS_PER_RECORD {
                return Err(InteractionTapeParseError::new(
                    line,
                    InteractionTapeParseErrorKind::ActionCountLimit {
                        maximum: MAX_INTERACTION_ACTIONS_PER_RECORD,
                    },
                ));
            }
            if total_actions >= MAX_INTERACTION_TOTAL_ACTIONS {
                return Err(InteractionTapeParseError::new(
                    line,
                    InteractionTapeParseErrorKind::TotalActionCountLimit {
                        maximum: MAX_INTERACTION_TOTAL_ACTIONS,
                    },
                ));
            }
            let action = parse_action(instruction, line, command)?;
            debug_assert!(action.is_canonical());
            actions.try_reserve_exact(1).map_err(|_| {
                InteractionTapeParseError::new(
                    line,
                    InteractionTapeParseErrorKind::AllocationRefused {
                        subject: "accepted actions",
                    },
                )
            })?;
            actions.push(action);
            total_actions += 1;
        }

        if let Some(actions) = current_actions {
            push_parsed_record(&mut records, actions, source.lines().count().max(1))?;
        }

        let tape = Self {
            records,
            total_actions,
        };
        let payload_len = tape.canonical_payload_len();
        if payload_len > MAX_INTERACTION_TAPE_PAYLOAD_BYTES {
            return Err(InteractionTapeParseError::new(
                source.lines().count().max(1),
                InteractionTapeParseErrorKind::PayloadTooLarge {
                    actual: payload_len,
                    maximum: MAX_INTERACTION_TAPE_PAYLOAD_BYTES,
                },
            ));
        }
        Ok(tape)
    }
}

fn push_parsed_record(
    records: &mut Vec<TickRecord>,
    actions: Vec<AcceptedAction>,
    line: usize,
) -> Result<(), InteractionTapeParseError> {
    records.try_reserve_exact(1).map_err(|_| {
        InteractionTapeParseError::new(
            line,
            InteractionTapeParseErrorKind::AllocationRefused {
                subject: "tick records",
            },
        )
    })?;
    records.push(TickRecord {
        ordinal: records.len() as u64,
        actions,
    });
    Ok(())
}

fn is_action_command(command: &str) -> bool {
    matches!(
        command,
        "move"
            | "look"
            | "wheel"
            | "delete"
            | "place"
            | "exact"
            | "fit"
            | "sidebar"
            | "resize"
            | "clear-motion"
    )
}

fn parse_action(
    instruction: &str,
    line: usize,
    command: &str,
) -> Result<AcceptedAction, InteractionTapeParseError> {
    let expected_fields = match command {
        "move" | "look" | "resize" => 3,
        "wheel" => 2,
        "exact" => 5,
        "delete" | "place" | "fit" | "sidebar" | "clear-motion" => 1,
        _ => unreachable!("caller rejects unknown commands"),
    };
    require_field_count(instruction, line, command, expected_fields)?;
    let mut fields = instruction.split_whitespace();
    debug_assert_eq!(fields.next(), Some(command));

    Ok(match command {
        "move" => {
            let control_token = fields.next().expect("field count checked");
            let control = MoveControl::parse(control_token).ok_or_else(|| {
                InteractionTapeParseError::new(
                    line,
                    InteractionTapeParseErrorKind::UnknownMoveControl(control_token.to_owned()),
                )
            })?;
            let state_token = fields.next().expect("field count checked");
            let down = match state_token {
                "down" => true,
                "up" => false,
                _ => {
                    return Err(InteractionTapeParseError::new(
                        line,
                        InteractionTapeParseErrorKind::UnknownMoveState(state_token.to_owned()),
                    ));
                }
            };
            AcceptedAction::MoveEdge(control, down)
        }
        "look" => AcceptedAction::LookDelta(
            parse_finite(
                fields.next().expect("field count checked"),
                "horizontal",
                line,
            )?,
            parse_finite(
                fields.next().expect("field count checked"),
                "vertical",
                line,
            )?,
        ),
        "wheel" => AcceptedAction::WheelLines(parse_finite(
            fields.next().expect("field count checked"),
            "lines",
            line,
        )?),
        "delete" => AcceptedAction::AimDelete,
        "place" => AcceptedAction::AimPlace,
        "exact" => {
            let operation_token = fields.next().expect("field count checked");
            let operation = match operation_token {
                "on" => Operation::On,
                "off" => Operation::Off,
                "toggle" => Operation::Toggle,
                _ => {
                    return Err(InteractionTapeParseError::new(
                        line,
                        InteractionTapeParseErrorKind::UnknownEditOperation(
                            operation_token.to_owned(),
                        ),
                    ));
                }
            };
            let x = parse_i64(fields.next().expect("field count checked"), "x", line)?;
            let y = parse_i64(fields.next().expect("field count checked"), "y", line)?;
            let z = parse_i64(fields.next().expect("field count checked"), "z", line)?;
            AcceptedAction::ExactEdit(Edit::new(operation, Coord::new(x, y, z)))
        }
        "fit" => AcceptedAction::FitWorld,
        "sidebar" => AcceptedAction::ToggleSidebar,
        "resize" => AcceptedAction::Resize(
            parse_u32(fields.next().expect("field count checked"), "width", line)?,
            parse_u32(fields.next().expect("field count checked"), "height", line)?,
        ),
        "clear-motion" => AcceptedAction::ClearMotion,
        _ => unreachable!("caller rejects unknown commands"),
    })
}

fn require_field_count(
    instruction: &str,
    line: usize,
    command: &str,
    expected: usize,
) -> Result<(), InteractionTapeParseError> {
    let found = instruction.split_whitespace().count();
    if found == expected {
        return Ok(());
    }
    Err(InteractionTapeParseError::new(
        line,
        InteractionTapeParseErrorKind::WrongFieldCount {
            command: command.to_owned(),
            expected,
            found,
        },
    ))
}

fn parse_finite(
    token: &str,
    field: &'static str,
    line: usize,
) -> Result<f64, InteractionTapeParseError> {
    let value = token.parse::<f64>().map_err(|_| {
        InteractionTapeParseError::new(
            line,
            InteractionTapeParseErrorKind::InvalidFloat {
                field,
                value: token.to_owned(),
            },
        )
    })?;
    if !value.is_finite() {
        return Err(InteractionTapeParseError::new(
            line,
            InteractionTapeParseErrorKind::NonFiniteFloat {
                field,
                value: token.to_owned(),
            },
        ));
    }
    Ok(value)
}

fn parse_i64(
    token: &str,
    field: &'static str,
    line: usize,
) -> Result<i64, InteractionTapeParseError> {
    token.parse().map_err(|_| {
        InteractionTapeParseError::new(
            line,
            InteractionTapeParseErrorKind::InvalidInteger {
                field,
                expected: "i64",
                value: token.to_owned(),
            },
        )
    })
}

fn parse_u32(
    token: &str,
    field: &'static str,
    line: usize,
) -> Result<u32, InteractionTapeParseError> {
    token.parse().map_err(|_| {
        InteractionTapeParseError::new(
            line,
            InteractionTapeParseErrorKind::InvalidInteger {
                field,
                expected: "u32",
                value: token.to_owned(),
            },
        )
    })
}

struct Decoder<'a> {
    bytes: &'a [u8],
    cursor: usize,
}

impl<'a> Decoder<'a> {
    const fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, cursor: 0 }
    }

    const fn remaining(&self) -> usize {
        self.bytes.len() - self.cursor
    }

    fn take(
        &mut self,
        count: usize,
        field: &'static str,
    ) -> Result<&'a [u8], InteractionTapeCodecError> {
        if count > self.remaining() {
            return Err(InteractionTapeCodecError::Truncated {
                field,
                offset: self.cursor,
                needed: count,
                remaining: self.remaining(),
            });
        }
        let start = self.cursor;
        self.cursor += count;
        Ok(&self.bytes[start..self.cursor])
    }

    fn read_u8(&mut self, field: &'static str) -> Result<u8, InteractionTapeCodecError> {
        Ok(self.take(1, field)?[0])
    }

    fn read_u32(&mut self, field: &'static str) -> Result<u32, InteractionTapeCodecError> {
        Ok(u32::from_be_bytes(
            self.take(size_of::<u32>(), field)?
                .try_into()
                .expect("take returned four bytes"),
        ))
    }

    fn read_u64(&mut self, field: &'static str) -> Result<u64, InteractionTapeCodecError> {
        Ok(u64::from_be_bytes(
            self.take(size_of::<u64>(), field)?
                .try_into()
                .expect("take returned eight bytes"),
        ))
    }
}

fn decode_action(
    decoder: &mut Decoder<'_>,
    record_index: usize,
    action_index: usize,
) -> Result<AcceptedAction, InteractionTapeCodecError> {
    let opcode = decoder.read_u8("action opcode")?;
    Ok(match opcode {
        OPCODE_MOVE_EDGE => {
            let code = decoder.read_u8("move control")?;
            let control = MoveControl::from_canonical_code(code).ok_or(
                InteractionTapeCodecError::UnknownMoveControl {
                    record_index,
                    action_index,
                    code,
                },
            )?;
            let marker = decoder.read_u8("move state")?;
            let down = match marker {
                0 => false,
                1 => true,
                _ => {
                    return Err(InteractionTapeCodecError::NonCanonicalBoolean {
                        record_index,
                        action_index,
                        field: "move state",
                        value: marker,
                    });
                }
            };
            AcceptedAction::MoveEdge(control, down)
        }
        OPCODE_LOOK_DELTA => AcceptedAction::LookDelta(
            decode_finite(decoder, record_index, action_index, "look horizontal")?,
            decode_finite(decoder, record_index, action_index, "look vertical")?,
        ),
        OPCODE_WHEEL_LINES => AcceptedAction::WheelLines(decode_finite(
            decoder,
            record_index,
            action_index,
            "wheel lines",
        )?),
        OPCODE_AIM_DELETE => AcceptedAction::AimDelete,
        OPCODE_AIM_PLACE => AcceptedAction::AimPlace,
        OPCODE_EXACT_EDIT => {
            let encoded = decoder.take(EDIT_CANONICAL_LEN, "exact edit")?;
            let edit = Edit::decode_canonical(encoded).map_err(|source| {
                InteractionTapeCodecError::InvalidExactEdit {
                    record_index,
                    action_index,
                    source,
                }
            })?;
            AcceptedAction::ExactEdit(edit)
        }
        OPCODE_FIT_WORLD => AcceptedAction::FitWorld,
        OPCODE_TOGGLE_SIDEBAR => AcceptedAction::ToggleSidebar,
        OPCODE_RESIZE => AcceptedAction::Resize(
            decoder.read_u32("resize width")?,
            decoder.read_u32("resize height")?,
        ),
        OPCODE_CLEAR_MOTION => AcceptedAction::ClearMotion,
        _ => {
            return Err(InteractionTapeCodecError::UnknownActionOpcode {
                record_index,
                action_index,
                code: opcode,
            });
        }
    })
}

fn decode_finite(
    decoder: &mut Decoder<'_>,
    record_index: usize,
    action_index: usize,
    field: &'static str,
) -> Result<f64, InteractionTapeCodecError> {
    let bits = decoder.read_u64(field)?;
    let value = f64::from_bits(bits);
    if !value.is_finite() {
        return Err(InteractionTapeCodecError::NonFiniteFloat {
            record_index,
            action_index,
            field,
            bits,
        });
    }
    Ok(value)
}

/// The specific reason one source line was refused.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum InteractionTapeParseErrorKind {
    WrongFieldCount {
        command: String,
        expected: usize,
        found: usize,
    },
    UnknownCommand(String),
    ActionBeforeTick(String),
    UnknownMoveControl(String),
    UnknownMoveState(String),
    UnknownEditOperation(String),
    InvalidFloat {
        field: &'static str,
        value: String,
    },
    NonFiniteFloat {
        field: &'static str,
        value: String,
    },
    InvalidInteger {
        field: &'static str,
        expected: &'static str,
        value: String,
    },
    RecordCountLimit {
        maximum: usize,
    },
    ActionCountLimit {
        maximum: usize,
    },
    TotalActionCountLimit {
        maximum: usize,
    },
    PayloadTooLarge {
        actual: usize,
        maximum: usize,
    },
    AllocationRefused {
        subject: &'static str,
    },
}

/// A parse error with its one-based physical source line.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InteractionTapeParseError {
    line: usize,
    kind: InteractionTapeParseErrorKind,
}

impl InteractionTapeParseError {
    const fn new(line: usize, kind: InteractionTapeParseErrorKind) -> Self {
        Self { line, kind }
    }

    pub const fn line(&self) -> usize {
        self.line
    }

    pub const fn kind(&self) -> &InteractionTapeParseErrorKind {
        &self.kind
    }
}

impl fmt::Display for InteractionTapeParseError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "line {}: ", self.line)?;
        match &self.kind {
            InteractionTapeParseErrorKind::WrongFieldCount {
                command,
                expected,
                found,
            } => write!(
                formatter,
                "'{command}' requires {expected} fields including the command, found {found}"
            ),
            InteractionTapeParseErrorKind::UnknownCommand(command) => {
                write!(formatter, "unknown interaction command '{command}'")
            }
            InteractionTapeParseErrorKind::ActionBeforeTick(command) => {
                write!(
                    formatter,
                    "action '{command}' appears before the first tick"
                )
            }
            InteractionTapeParseErrorKind::UnknownMoveControl(control) => write!(
                formatter,
                "unknown move control '{control}' (expected forward, backward, left, right, up, or down)"
            ),
            InteractionTapeParseErrorKind::UnknownMoveState(state) => {
                write!(
                    formatter,
                    "unknown move state '{state}' (expected down or up)"
                )
            }
            InteractionTapeParseErrorKind::UnknownEditOperation(operation) => write!(
                formatter,
                "unknown exact-edit operation '{operation}' (expected on, off, or toggle)"
            ),
            InteractionTapeParseErrorKind::InvalidFloat { field, value } => {
                write!(formatter, "{field} value '{value}' is not an f64")
            }
            InteractionTapeParseErrorKind::NonFiniteFloat { field, value } => {
                write!(formatter, "{field} value '{value}' is not finite")
            }
            InteractionTapeParseErrorKind::InvalidInteger {
                field,
                expected,
                value,
            } => write!(formatter, "{field} value '{value}' is not a {expected}"),
            InteractionTapeParseErrorKind::RecordCountLimit { maximum } => {
                write!(formatter, "interaction tape exceeds {maximum} tick records")
            }
            InteractionTapeParseErrorKind::ActionCountLimit { maximum } => {
                write!(formatter, "tick record exceeds {maximum} accepted actions")
            }
            InteractionTapeParseErrorKind::TotalActionCountLimit { maximum } => write!(
                formatter,
                "interaction tape exceeds {maximum} accepted actions"
            ),
            InteractionTapeParseErrorKind::PayloadTooLarge { actual, maximum } => write!(
                formatter,
                "canonical interaction payload is {actual} bytes, exceeding {maximum}"
            ),
            InteractionTapeParseErrorKind::AllocationRefused { subject } => {
                write!(formatter, "could not allocate {subject}")
            }
        }
    }
}

impl std::error::Error for InteractionTapeParseError {}

/// Why bytes were refused as a canonical interaction tape.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InteractionTapeCodecError {
    PayloadTooLarge {
        actual: usize,
        maximum: usize,
    },
    Truncated {
        field: &'static str,
        offset: usize,
        needed: usize,
        remaining: usize,
    },
    RecordCountLimit {
        declared: u64,
        maximum: usize,
    },
    TotalActionCountLimit {
        declared: u64,
        maximum: usize,
    },
    RecordHeadersCannotFit {
        declared: usize,
        minimum_bytes: usize,
        actual_bytes: usize,
    },
    AllocationRefused {
        subject: &'static str,
        requested: usize,
    },
    NonContiguousOrdinal {
        record_index: usize,
        expected: u64,
        actual: u64,
    },
    ActionCountLimit {
        record_index: usize,
        declared: u64,
        maximum: usize,
    },
    TotalActionCountOverflow,
    ObservedTotalActionCountLimit {
        observed: usize,
        maximum: usize,
    },
    TotalActionCountMismatch {
        declared: u64,
        observed: usize,
    },
    ActionBytesCannotFit {
        record_index: usize,
        declared: usize,
        remaining_bytes: usize,
    },
    UnknownActionOpcode {
        record_index: usize,
        action_index: usize,
        code: u8,
    },
    UnknownMoveControl {
        record_index: usize,
        action_index: usize,
        code: u8,
    },
    NonCanonicalBoolean {
        record_index: usize,
        action_index: usize,
        field: &'static str,
        value: u8,
    },
    NonFiniteFloat {
        record_index: usize,
        action_index: usize,
        field: &'static str,
        bits: u64,
    },
    InvalidExactEdit {
        record_index: usize,
        action_index: usize,
        source: EditCodecError,
    },
    TrailingBytes {
        count: usize,
    },
    NonCanonicalEncoding,
}

impl fmt::Display for InteractionTapeCodecError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {
            Self::PayloadTooLarge { actual, maximum } => write!(
                formatter,
                "canonical interaction payload is {actual} bytes, exceeding {maximum}"
            ),
            Self::Truncated {
                field,
                offset,
                needed,
                remaining,
            } => write!(
                formatter,
                "truncated {field} at byte {offset}: need {needed} bytes, have {remaining}"
            ),
            Self::RecordCountLimit { declared, maximum } => write!(
                formatter,
                "declared record count {declared} exceeds {maximum}"
            ),
            Self::TotalActionCountLimit { declared, maximum } => write!(
                formatter,
                "declared total action count {declared} exceeds {maximum}"
            ),
            Self::RecordHeadersCannotFit {
                declared,
                minimum_bytes,
                actual_bytes,
            } => write!(
                formatter,
                "{declared} record headers need at least {minimum_bytes} bytes, found {actual_bytes}"
            ),
            Self::AllocationRefused { subject, requested } => write!(
                formatter,
                "could not reserve {requested} entries for {subject}"
            ),
            Self::NonContiguousOrdinal {
                record_index,
                expected,
                actual,
            } => write!(
                formatter,
                "record {record_index} has ordinal {actual}, expected {expected}"
            ),
            Self::ActionCountLimit {
                record_index,
                declared,
                maximum,
            } => write!(
                formatter,
                "record {record_index} declares {declared} actions, exceeding {maximum}"
            ),
            Self::TotalActionCountOverflow => {
                formatter.write_str("observed total action count overflowed usize")
            }
            Self::ObservedTotalActionCountLimit { observed, maximum } => write!(
                formatter,
                "record counts total {observed} actions, exceeding {maximum}"
            ),
            Self::TotalActionCountMismatch { declared, observed } => write!(
                formatter,
                "header declares {declared} total actions, records contain {observed}"
            ),
            Self::ActionBytesCannotFit {
                record_index,
                declared,
                remaining_bytes,
            } => write!(
                formatter,
                "record {record_index} declares {declared} actions but only {remaining_bytes} bytes remain"
            ),
            Self::UnknownActionOpcode {
                record_index,
                action_index,
                code,
            } => write!(
                formatter,
                "record {record_index} action {action_index} uses undefined opcode {code}"
            ),
            Self::UnknownMoveControl {
                record_index,
                action_index,
                code,
            } => write!(
                formatter,
                "record {record_index} action {action_index} uses undefined move control {code}"
            ),
            Self::NonCanonicalBoolean {
                record_index,
                action_index,
                field,
                value,
            } => write!(
                formatter,
                "record {record_index} action {action_index} has noncanonical {field} byte {value}"
            ),
            Self::NonFiniteFloat {
                record_index,
                action_index,
                field,
                bits,
            } => write!(
                formatter,
                "record {record_index} action {action_index} has non-finite {field} bits {bits:#018x}"
            ),
            Self::InvalidExactEdit {
                record_index,
                action_index,
                source,
            } => write!(
                formatter,
                "record {record_index} action {action_index} has invalid exact edit: {source}"
            ),
            Self::TrailingBytes { count } => {
                write!(
                    formatter,
                    "canonical interaction payload has {count} trailing bytes"
                )
            }
            Self::NonCanonicalEncoding => {
                formatter.write_str("interaction tape does not re-encode canonically")
            }
        }
    }
}

impl std::error::Error for InteractionTapeCodecError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::InvalidExactEdit { source, .. } => Some(source),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn all_variants_source() -> &'static str {
        "# comments and blank lines do not enter the tape\n\
         tick # an empty tick is retained\n\
         \n\
         tick\n\
         move forward down\n\
         move backward up\n\
         move left down\n\
         move right up\n\
         move up down\n\
         move down up\n\
         look -0 1.5\n\
         wheel -2.25\n\
         delete\n\
         place\n\
         exact toggle -9223372036854775808 -2 9223372036854775807\n\
         fit\n\
         sidebar\n\
         resize 0 4294967295\n\
         clear-motion"
    }

    #[test]
    fn text_parser_and_display_cover_every_action_and_preserve_empty_ticks() {
        let tape = InteractionTape::parse(all_variants_source()).unwrap();
        assert_eq!(tape.len(), 2);
        assert_eq!(tape.total_actions(), 15);
        assert_eq!(tape.records()[0].ordinal(), 0);
        assert!(tape.records()[0].is_empty());
        assert_eq!(tape.records()[1].ordinal(), 1);
        assert_eq!(tape.records()[1].len(), 15);
        assert_eq!(
            tape.records()[1].actions(),
            [
                AcceptedAction::MoveEdge(MoveControl::Forward, true),
                AcceptedAction::MoveEdge(MoveControl::Backward, false),
                AcceptedAction::MoveEdge(MoveControl::Left, true),
                AcceptedAction::MoveEdge(MoveControl::Right, false),
                AcceptedAction::MoveEdge(MoveControl::Up, true),
                AcceptedAction::MoveEdge(MoveControl::Down, false),
                AcceptedAction::LookDelta(-0.0, 1.5),
                AcceptedAction::WheelLines(-2.25),
                AcceptedAction::AimDelete,
                AcceptedAction::AimPlace,
                AcceptedAction::ExactEdit(Edit::new(
                    Operation::Toggle,
                    Coord::new(i64::MIN, -2, i64::MAX),
                )),
                AcceptedAction::FitWorld,
                AcceptedAction::ToggleSidebar,
                AcceptedAction::Resize(0, u32::MAX),
                AcceptedAction::ClearMotion,
            ]
        );

        let canonical_text = tape.to_string();
        assert!(canonical_text.starts_with("tick\ntick\nmove forward down"));
        assert!(canonical_text.contains("\nlook -0 1.5\nwheel -2.25\n"));
        assert!(!canonical_text.ends_with('\n'));
        assert_eq!(InteractionTape::parse(&canonical_text).unwrap(), tape);
    }

    #[test]
    fn empty_comment_only_and_empty_tick_sources_have_distinct_canonical_forms() {
        for source in ["", "\r\n # nothing\r\n"] {
            let tape = InteractionTape::parse(source).unwrap();
            assert!(tape.is_empty());
            assert_eq!(tape.total_actions(), 0);
            assert_eq!(tape.to_string(), "");
            assert_eq!(tape.encode_canonical(), vec![0; PAYLOAD_HEADER_LEN]);
        }

        let one_empty = InteractionTape::parse("tick").unwrap();
        assert_eq!(one_empty.len(), 1);
        assert_eq!(one_empty.total_actions(), 0);
        assert_eq!(one_empty.to_string(), "tick");
        assert_ne!(one_empty.encode_canonical(), vec![0; PAYLOAD_HEADER_LEN]);
    }

    #[test]
    fn actions_before_tick_are_refused_on_their_physical_line() {
        let error = InteractionTape::parse("# ignored\n\n look 1 2").unwrap_err();
        assert_eq!(error.line(), 3);
        assert_eq!(
            error.kind(),
            &InteractionTapeParseErrorKind::ActionBeforeTick("look".to_owned())
        );
        assert_eq!(
            error.to_string(),
            "line 3: action 'look' appears before the first tick"
        );
    }

    #[test]
    fn text_parser_refuses_unknown_shapes_tokens_ranges_and_nonfinite_values() {
        let cases = [
            (
                "tock",
                InteractionTapeParseErrorKind::UnknownCommand("tock".into()),
            ),
            (
                "tick 1",
                InteractionTapeParseErrorKind::WrongFieldCount {
                    command: "tick".into(),
                    expected: 1,
                    found: 2,
                },
            ),
            (
                "tick\nmove ahead down",
                InteractionTapeParseErrorKind::UnknownMoveControl("ahead".into()),
            ),
            (
                "tick\nmove left held",
                InteractionTapeParseErrorKind::UnknownMoveState("held".into()),
            ),
            (
                "tick\nlook nope 0",
                InteractionTapeParseErrorKind::InvalidFloat {
                    field: "horizontal",
                    value: "nope".into(),
                },
            ),
            (
                "tick\nlook NaN 0",
                InteractionTapeParseErrorKind::NonFiniteFloat {
                    field: "horizontal",
                    value: "NaN".into(),
                },
            ),
            (
                "tick\nwheel inf",
                InteractionTapeParseErrorKind::NonFiniteFloat {
                    field: "lines",
                    value: "inf".into(),
                },
            ),
            (
                "tick\nexact paint 0 0 0",
                InteractionTapeParseErrorKind::UnknownEditOperation("paint".into()),
            ),
            (
                "tick\nexact on 9223372036854775808 0 0",
                InteractionTapeParseErrorKind::InvalidInteger {
                    field: "x",
                    expected: "i64",
                    value: "9223372036854775808".into(),
                },
            ),
            (
                "tick\nresize -1 2",
                InteractionTapeParseErrorKind::InvalidInteger {
                    field: "width",
                    expected: "u32",
                    value: "-1".into(),
                },
            ),
            (
                "tick\ndelete now",
                InteractionTapeParseErrorKind::WrongFieldCount {
                    command: "delete".into(),
                    expected: 1,
                    found: 2,
                },
            ),
        ];

        for (source, expected) in cases {
            let error = InteractionTape::parse(source).unwrap_err();
            assert_eq!(error.kind(), &expected, "source: {source}");
        }
    }

    #[test]
    fn canonical_vector_fixes_header_order_opcodes_endianness_and_float_bits() {
        let source = "tick\n\
                      move right down\n\
                      look -0 1.5\n\
                      wheel -2.25\n\
                      exact toggle -9223372036854775808 -2 9223372036854775807\n\
                      resize 305419896 2309737967\n\
                      clear-motion";
        let tape = InteractionTape::parse(source).unwrap();
        let expected = hex("00000000000000010000000000000006\
             000000000000000000000006\
             000301\
             0180000000000000003ff8000000000000\
             02c002000000000000\
             05028000000000000000fffffffffffffffe7fffffffffffffff\
             081234567889abcdef\
             09");
        assert_eq!(tape.encode_canonical(), expected);
        assert_eq!(
            InteractionTape::decode_canonical(&expected),
            Ok(tape.clone())
        );
        assert_eq!(
            tape.content_tag().to_string(),
            "#bb.intent.sha256.zhqcq35tj65fukzzkpix6lo45nxsyfrqhzlun2ktpqsegsthiuwa"
        );
    }

    #[test]
    fn codec_round_trips_all_variants_and_exact_float_bits() {
        let tape = InteractionTape::parse(all_variants_source()).unwrap();
        let encoded = tape.encode_canonical();
        let decoded = InteractionTape::decode_canonical(&encoded).unwrap();
        assert_eq!(decoded, tape);
        match decoded.records()[1].actions()[6] {
            AcceptedAction::LookDelta(horizontal, vertical) => {
                assert_eq!(horizontal.to_bits(), (-0.0_f64).to_bits());
                assert_eq!(vertical.to_bits(), 1.5_f64.to_bits());
            }
            _ => panic!("expected look delta"),
        }
        assert_eq!(decoded.encode_canonical(), encoded);
    }

    #[test]
    fn codec_refuses_truncation_trailing_padding_and_impossible_record_headers() {
        let canonical = InteractionTape::parse("tick\nlook 1 2")
            .unwrap()
            .encode_canonical();
        for end in 0..canonical.len() {
            assert!(
                InteractionTape::decode_canonical(&canonical[..end]).is_err(),
                "accepted prefix length {end}"
            );
        }

        let mut trailing = canonical.clone();
        trailing.push(0);
        assert_eq!(
            InteractionTape::decode_canonical(&trailing),
            Err(InteractionTapeCodecError::TrailingBytes { count: 1 })
        );

        let mut impossible = Vec::new();
        impossible.extend_from_slice(&(MAX_INTERACTION_TAPE_RECORDS as u64).to_be_bytes());
        impossible.extend_from_slice(&0_u64.to_be_bytes());
        assert!(matches!(
            InteractionTape::decode_canonical(&impossible),
            Err(InteractionTapeCodecError::RecordHeadersCannotFit { .. })
        ));
    }

    #[test]
    fn codec_refuses_noncontiguous_ordinals_and_all_count_inconsistencies() {
        let base = InteractionTape::parse("tick\ndelete\ntick\nplace")
            .unwrap()
            .encode_canonical();

        let mut first_ordinal = base.clone();
        first_ordinal[16..24].copy_from_slice(&1_u64.to_be_bytes());
        assert!(matches!(
            InteractionTape::decode_canonical(&first_ordinal),
            Err(InteractionTapeCodecError::NonContiguousOrdinal {
                record_index: 0,
                expected: 0,
                actual: 1
            })
        ));

        let second_header = 16 + 12 + 1;
        let mut skipped = base.clone();
        skipped[second_header..second_header + 8].copy_from_slice(&3_u64.to_be_bytes());
        assert!(matches!(
            InteractionTape::decode_canonical(&skipped),
            Err(InteractionTapeCodecError::NonContiguousOrdinal {
                record_index: 1,
                expected: 1,
                actual: 3
            })
        ));

        let mut total_too_large = base.clone();
        total_too_large[8..16].copy_from_slice(&3_u64.to_be_bytes());
        assert_eq!(
            InteractionTape::decode_canonical(&total_too_large),
            Err(InteractionTapeCodecError::TotalActionCountMismatch {
                declared: 3,
                observed: 2
            })
        );

        let mut total_too_small = base.clone();
        total_too_small[8..16].copy_from_slice(&1_u64.to_be_bytes());
        assert_eq!(
            InteractionTape::decode_canonical(&total_too_small),
            Err(InteractionTapeCodecError::TotalActionCountMismatch {
                declared: 1,
                observed: 2
            })
        );

        let mut record_count = base.clone();
        record_count[..8].copy_from_slice(&1_u64.to_be_bytes());
        assert!(matches!(
            InteractionTape::decode_canonical(&record_count),
            Err(InteractionTapeCodecError::TotalActionCountMismatch { .. })
                | Err(InteractionTapeCodecError::TrailingBytes { .. })
        ));
    }

    #[test]
    fn codec_refuses_unknown_codes_noncanonical_bools_nonfinite_lanes_and_bad_edits() {
        let single = |line: &str| {
            InteractionTape::parse(&format!("tick\n{line}"))
                .unwrap()
                .encode_canonical()
        };
        const ACTION_OFFSET: usize = PAYLOAD_HEADER_LEN + RECORD_HEADER_LEN;

        let mut opcode = single("delete");
        opcode[ACTION_OFFSET] = 0xff;
        assert!(matches!(
            InteractionTape::decode_canonical(&opcode),
            Err(InteractionTapeCodecError::UnknownActionOpcode { code: 0xff, .. })
        ));

        let mut control = single("move forward down");
        control[ACTION_OFFSET + 1] = 6;
        assert!(matches!(
            InteractionTape::decode_canonical(&control),
            Err(InteractionTapeCodecError::UnknownMoveControl { code: 6, .. })
        ));

        let mut boolean = single("move forward down");
        boolean[ACTION_OFFSET + 2] = 2;
        assert!(matches!(
            InteractionTape::decode_canonical(&boolean),
            Err(InteractionTapeCodecError::NonCanonicalBoolean { value: 2, .. })
        ));

        for line in ["look 1 2", "wheel 1"] {
            let mut nonfinite = single(line);
            nonfinite[ACTION_OFFSET + 1..ACTION_OFFSET + 9]
                .copy_from_slice(&f64::INFINITY.to_bits().to_be_bytes());
            assert!(matches!(
                InteractionTape::decode_canonical(&nonfinite),
                Err(InteractionTapeCodecError::NonFiniteFloat { .. })
            ));
        }

        let mut edit = single("exact on 1 2 3");
        edit[ACTION_OFFSET + 1] = 3;
        assert!(matches!(
            InteractionTape::decode_canonical(&edit),
            Err(InteractionTapeCodecError::InvalidExactEdit {
                source: EditCodecError::UnknownOperation(3),
                ..
            })
        ));
    }

    #[test]
    fn decoder_checks_all_caps_before_attacker_controlled_preallocation() {
        let header = |records: u64, actions: u64| {
            let mut bytes = Vec::new();
            bytes.extend_from_slice(&records.to_be_bytes());
            bytes.extend_from_slice(&actions.to_be_bytes());
            bytes
        };

        assert!(matches!(
            InteractionTape::decode_canonical(&header(MAX_INTERACTION_TAPE_RECORDS as u64 + 1, 0)),
            Err(InteractionTapeCodecError::RecordCountLimit { .. })
        ));
        assert!(matches!(
            InteractionTape::decode_canonical(&header(0, MAX_INTERACTION_TOTAL_ACTIONS as u64 + 1)),
            Err(InteractionTapeCodecError::TotalActionCountLimit { .. })
        ));

        let mut per_record = header(1, 0);
        per_record.extend_from_slice(&0_u64.to_be_bytes());
        per_record
            .extend_from_slice(&(MAX_INTERACTION_ACTIONS_PER_RECORD as u32 + 1).to_be_bytes());
        assert!(matches!(
            InteractionTape::decode_canonical(&per_record),
            Err(InteractionTapeCodecError::ActionCountLimit { .. })
        ));

        let mut cannot_fit = header(1, MAX_INTERACTION_ACTIONS_PER_RECORD as u64);
        cannot_fit.extend_from_slice(&0_u64.to_be_bytes());
        cannot_fit.extend_from_slice(&(MAX_INTERACTION_ACTIONS_PER_RECORD as u32).to_be_bytes());
        assert!(matches!(
            InteractionTape::decode_canonical(&cannot_fit),
            Err(InteractionTapeCodecError::ActionBytesCannotFit { .. })
        ));

        let oversized = vec![0; MAX_INTERACTION_TAPE_PAYLOAD_BYTES + 1];
        assert!(matches!(
            InteractionTape::decode_canonical(&oversized),
            Err(InteractionTapeCodecError::PayloadTooLarge { .. })
        ));
    }

    #[test]
    fn tags_commit_to_tick_boundaries_action_order_and_signed_zero() {
        let base = InteractionTape::parse("tick\nlook 0 1\nwheel 2").unwrap();
        let signed_zero = InteractionTape::parse("tick\nlook -0 1\nwheel 2").unwrap();
        let reversed = InteractionTape::parse("tick\nwheel 2\nlook 0 1").unwrap();
        let split = InteractionTape::parse("tick\nlook 0 1\ntick\nwheel 2").unwrap();

        assert_ne!(base.content_tag(), signed_zero.content_tag());
        assert_ne!(base.content_tag(), reversed.content_tag());
        assert_ne!(base.content_tag(), split.content_tag());
    }

    fn hex(source: &str) -> Vec<u8> {
        let digits: String = source
            .chars()
            .filter(|character| !character.is_whitespace())
            .collect();
        assert_eq!(digits.len() % 2, 0);
        (0..digits.len())
            .step_by(2)
            .map(|index| u8::from_str_radix(&digits[index..index + 2], 16).unwrap())
            .collect()
    }
}
