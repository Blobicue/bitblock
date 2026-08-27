//! A minimal text language for deterministic, ordered world edits.
//!
//! Each non-empty line contains exactly one instruction:
//!
//! ```text
//! on x y z
//! off x y z
//! toggle x y z
//! ```
//!
//! Coordinates are decimal `i64` values. A `#` begins a comment, and blank or
//! comment-only lines are ignored. [`Program`]'s [`Display`](std::fmt::Display)
//! implementation is the canonical serialization: one normalized instruction
//! per line, in execution order, with no comments or trailing newline.

use crate::protocol::{ContentTag, TagKind, hash_canonical};
use crate::world::{Coord, World};
use std::fmt;
use std::str::FromStr;

/// One of the three primitive changes the language can make to a bit.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Operation {
    On,
    Off,
    Toggle,
}

impl Operation {
    /// The canonical one-byte code committed by [`Edit::content_tag`].
    pub const fn canonical_code(self) -> u8 {
        match self {
            Self::On => 0,
            Self::Off => 1,
            Self::Toggle => 2,
        }
    }

    /// Decodes the complete set of canonical operation codes.
    pub const fn from_canonical_code(code: u8) -> Option<Self> {
        match code {
            0 => Some(Self::On),
            1 => Some(Self::Off),
            2 => Some(Self::Toggle),
            _ => None,
        }
    }

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::On => "on",
            Self::Off => "off",
            Self::Toggle => "toggle",
        }
    }

    fn parse(token: &str) -> Option<Self> {
        match token {
            "on" => Some(Self::On),
            "off" => Some(Self::Off),
            "toggle" => Some(Self::Toggle),
            _ => None,
        }
    }
}

/// Schema bound into every standalone edit tag.
pub const EDIT_SCHEMA: &[u8] = b"bitblock/edit/op-u8-i64-be/v1";

/// One operation byte followed by signed x, y, and z coordinates in big-endian order.
pub const EDIT_CANONICAL_LEN: usize = 1 + 3 * size_of::<i64>();

/// Why bytes were refused as a canonical [`Edit`].
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EditCodecError {
    WrongLength { expected: usize, actual: usize },
    UnknownOperation(u8),
}

impl fmt::Display for EditCodecError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {
            Self::WrongLength { expected, actual } => {
                write!(
                    formatter,
                    "canonical edit requires {expected} bytes, found {actual}"
                )
            }
            Self::UnknownOperation(code) => {
                write!(
                    formatter,
                    "canonical edit operation byte {code} is undefined"
                )
            }
        }
    }
}

impl std::error::Error for EditCodecError {}

impl fmt::Display for Operation {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

/// One primitive edit at one exact coordinate.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Edit {
    pub operation: Operation,
    pub coord: Coord,
}

impl Edit {
    pub const fn new(operation: Operation, coord: Coord) -> Self {
        Self { operation, coord }
    }

    /// Encodes this edit without allocation or implementation-dependent layout.
    pub fn encode_canonical(self) -> [u8; EDIT_CANONICAL_LEN] {
        let mut bytes = [0_u8; EDIT_CANONICAL_LEN];
        bytes[0] = self.operation.canonical_code();
        bytes[1..9].copy_from_slice(&self.coord.x.to_be_bytes());
        bytes[9..17].copy_from_slice(&self.coord.y.to_be_bytes());
        bytes[17..25].copy_from_slice(&self.coord.z.to_be_bytes());
        bytes
    }

    /// Strictly decodes exactly one canonical edit.
    pub fn decode_canonical(bytes: &[u8]) -> Result<Self, EditCodecError> {
        if bytes.len() != EDIT_CANONICAL_LEN {
            return Err(EditCodecError::WrongLength {
                expected: EDIT_CANONICAL_LEN,
                actual: bytes.len(),
            });
        }

        let operation = Operation::from_canonical_code(bytes[0])
            .ok_or(EditCodecError::UnknownOperation(bytes[0]))?;
        let x = i64::from_be_bytes(bytes[1..9].try_into().expect("length checked above"));
        let y = i64::from_be_bytes(bytes[9..17].try_into().expect("length checked above"));
        let z = i64::from_be_bytes(bytes[17..25].try_into().expect("length checked above"));
        Ok(Self::new(operation, Coord::new(x, y, z)))
    }

    /// Names the exact canonical bytes under the standalone edit schema.
    pub fn content_tag(self) -> ContentTag {
        let bytes = self.encode_canonical();
        hash_canonical(
            TagKind::Edit,
            EDIT_SCHEMA,
            EDIT_CANONICAL_LEN as u64,
            |sink| sink(&bytes),
        )
        .expect("the fixed edit encoder must emit its declared length")
    }

    /// Applies this edit to `world` and reports whether the world changed.
    ///
    /// An `on` instruction for an already-on coordinate and an `off`
    /// instruction for an already-off coordinate are preserved instructions
    /// even though they are state no-ops. Attempts to turn off or toggle the
    /// permanent origin bedrock are likewise retained no-ops.
    pub fn apply(self, world: &mut World) -> bool {
        match self.operation {
            Operation::On => world.set_on(self.coord),
            Operation::Off => world.set_off(self.coord),
            Operation::Toggle => {
                let before = world.is_on(self.coord);
                let after = world.toggle(self.coord);
                before != after
            }
        }
    }
}

impl fmt::Display for Edit {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "{} {} {} {}",
            self.operation, self.coord.x, self.coord.y, self.coord.z
        )
    }
}

/// An ordered sequence of edits.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Program {
    edits: Vec<Edit>,
}

impl Program {
    pub const fn new() -> Self {
        Self { edits: Vec::new() }
    }

    pub fn from_edits(edits: impl IntoIterator<Item = Edit>) -> Self {
        Self {
            edits: edits.into_iter().collect(),
        }
    }

    pub fn parse(source: &str) -> Result<Self, ParseError> {
        source.parse()
    }

    pub fn push(&mut self, edit: Edit) {
        self.edits.push(edit);
    }

    pub fn edits(&self) -> &[Edit] {
        &self.edits
    }

    pub fn iter(&self) -> impl ExactSizeIterator<Item = &Edit> {
        self.edits.iter()
    }

    pub const fn len(&self) -> usize {
        self.edits.len()
    }

    pub const fn is_empty(&self) -> bool {
        self.edits.is_empty()
    }

    /// Applies every edit in source order and returns the number that changed
    /// the world. Every instruction remains present in the program, including
    /// idempotent `on`/`off` attempts that do not change state.
    pub fn apply(&self, world: &mut World) -> usize {
        let mut changed = 0;
        for edit in &self.edits {
            changed += usize::from(edit.apply(world));
        }
        changed
    }
}

impl From<Vec<Edit>> for Program {
    fn from(edits: Vec<Edit>) -> Self {
        Self { edits }
    }
}

impl FromIterator<Edit> for Program {
    fn from_iter<T: IntoIterator<Item = Edit>>(edits: T) -> Self {
        Self::from_edits(edits)
    }
}

impl<'a> IntoIterator for &'a Program {
    type Item = &'a Edit;
    type IntoIter = std::slice::Iter<'a, Edit>;

    fn into_iter(self) -> Self::IntoIter {
        self.edits.iter()
    }
}

impl IntoIterator for Program {
    type Item = Edit;
    type IntoIter = std::vec::IntoIter<Edit>;

    fn into_iter(self) -> Self::IntoIter {
        self.edits.into_iter()
    }
}

impl fmt::Display for Program {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (index, edit) in self.edits.iter().enumerate() {
            if index != 0 {
                formatter.write_str("\n")?;
            }
            edit.fmt(formatter)?;
        }
        Ok(())
    }
}

impl FromStr for Program {
    type Err = ParseError;

    fn from_str(source: &str) -> Result<Self, Self::Err> {
        let mut program = Self::new();

        for (line_index, source_line) in source.lines().enumerate() {
            let instruction = source_line
                .split_once('#')
                .map_or(source_line, |(before_comment, _)| before_comment)
                .trim();

            if instruction.is_empty() {
                continue;
            }

            program
                .edits
                .push(parse_instruction(instruction, line_index + 1)?);
        }

        Ok(program)
    }
}

fn parse_instruction(instruction: &str, line: usize) -> Result<Edit, ParseError> {
    let fields: Vec<_> = instruction.split_whitespace().collect();
    if fields.len() != 4 {
        return Err(ParseError::new(
            line,
            ParseErrorKind::WrongFieldCount {
                found: fields.len(),
            },
        ));
    }

    let operation = Operation::parse(fields[0]).ok_or_else(|| {
        ParseError::new(line, ParseErrorKind::UnknownOperation(fields[0].to_owned()))
    })?;
    let x = parse_coordinate(fields[1], Axis::X, line)?;
    let y = parse_coordinate(fields[2], Axis::Y, line)?;
    let z = parse_coordinate(fields[3], Axis::Z, line)?;

    Ok(Edit::new(operation, Coord::new(x, y, z)))
}

fn parse_coordinate(token: &str, axis: Axis, line: usize) -> Result<i64, ParseError> {
    token.parse().map_err(|_| {
        ParseError::new(
            line,
            ParseErrorKind::InvalidCoordinate {
                axis,
                value: token.to_owned(),
            },
        )
    })
}

/// Coordinate field named by a parse error.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Axis {
    X,
    Y,
    Z,
}

impl fmt::Display for Axis {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::X => "x",
            Self::Y => "y",
            Self::Z => "z",
        })
    }
}

/// The specific reason one source line was rejected.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ParseErrorKind {
    WrongFieldCount { found: usize },
    UnknownOperation(String),
    InvalidCoordinate { axis: Axis, value: String },
}

/// A parse error with a one-based source line number.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ParseError {
    line: usize,
    kind: ParseErrorKind,
}

impl ParseError {
    const fn new(line: usize, kind: ParseErrorKind) -> Self {
        Self { line, kind }
    }

    pub const fn line(&self) -> usize {
        self.line
    }

    pub const fn kind(&self) -> &ParseErrorKind {
        &self.kind
    }
}

impl fmt::Display for ParseError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "line {}: ", self.line)?;
        match &self.kind {
            ParseErrorKind::WrongFieldCount { found } => write!(
                formatter,
                "expected an operation and three coordinates (4 fields), found {found}"
            ),
            ParseErrorKind::UnknownOperation(operation) => write!(
                formatter,
                "unknown operation '{operation}' (expected on, off, or toggle)"
            ),
            ParseErrorKind::InvalidCoordinate { axis, value } => {
                write!(formatter, "{axis} coordinate '{value}' is not an i64")
            }
        }
    }
}

impl std::error::Error for ParseError {}

/// The exact, replayable world-language input executed by one simulation tick.
///
/// `attempts` includes inputs that were not executed because they resolved to
/// no coordinate or would have enclosed the observer. `program` retains every
/// executed instruction, including idempotent world-state no-ops.
/// Camera movement and rendering are deliberately outside this proof surface.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TickBatch {
    tick: u64,
    attempts: usize,
    program: Program,
}

impl TickBatch {
    pub fn new(tick: u64, attempts: usize, edits: impl IntoIterator<Item = Edit>) -> Self {
        let program = Program::from_edits(edits);
        assert!(
            attempts >= program.len(),
            "resolved edits cannot outnumber attempted edits"
        );
        Self {
            tick,
            attempts,
            program,
        }
    }

    pub const fn tick(&self) -> u64 {
        self.tick
    }

    pub const fn attempts(&self) -> usize {
        self.attempts
    }

    pub fn resolved_edits(&self) -> &[Edit] {
        self.program.edits()
    }

    pub const fn misses(&self) -> usize {
        self.attempts - self.program.len()
    }

    /// Replays the exact world mutation for this tick.
    pub fn replay(&self, world: &mut World) -> usize {
        self.program.apply(world)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const P: Coord = Coord::new(4, -5, 6);
    const Q: Coord = Coord::new(-7, 8, -9);

    #[test]
    fn canonical_edit_vector_covers_operation_order_endianness_and_extremes() {
        let edit = Edit::new(Operation::Toggle, Coord::new(i64::MIN, -2, i64::MAX));
        let expected = [
            0x02, 0x80, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0xff, 0xff, 0xff, 0xff, 0xff,
            0xff, 0xff, 0xfe, 0x7f, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff,
        ];

        assert_eq!(edit.encode_canonical(), expected);
        assert_eq!(Edit::decode_canonical(&expected), Ok(edit));
        assert_eq!(
            edit.content_tag().to_string(),
            "#bb.edit.sha256.l3qxygkk2tikiyma5zkvaznrgzvu46ulmtdse56c2tf4r24ugzdq"
        );
    }

    #[test]
    fn canonical_edit_round_trips_all_operations_and_coordinate_boundaries() {
        let operations = [Operation::On, Operation::Off, Operation::Toggle];
        let values = [i64::MIN, -1, 0, 1, i64::MAX];

        for operation in operations {
            for &x in &values {
                for &y in &values {
                    for &z in &values {
                        let edit = Edit::new(operation, Coord::new(x, y, z));
                        assert_eq!(Edit::decode_canonical(&edit.encode_canonical()), Ok(edit));
                    }
                }
            }
        }
    }

    #[test]
    fn canonical_edit_decoder_refuses_every_wrong_length_and_operation_code() {
        let canonical = Edit::new(Operation::On, P).encode_canonical();
        for length in 0..EDIT_CANONICAL_LEN {
            assert_eq!(
                Edit::decode_canonical(&canonical[..length]),
                Err(EditCodecError::WrongLength {
                    expected: EDIT_CANONICAL_LEN,
                    actual: length,
                })
            );
        }
        let mut overlong = canonical.to_vec();
        overlong.push(0);
        assert_eq!(
            Edit::decode_canonical(&overlong),
            Err(EditCodecError::WrongLength {
                expected: EDIT_CANONICAL_LEN,
                actual: EDIT_CANONICAL_LEN + 1,
            })
        );

        for code in 3..=u8::MAX {
            let mut invalid = canonical;
            invalid[0] = code;
            assert_eq!(
                Edit::decode_canonical(&invalid),
                Err(EditCodecError::UnknownOperation(code))
            );
        }
    }

    #[test]
    fn any_single_byte_edit_mutation_changes_its_tag() {
        let edit = Edit::new(Operation::On, Coord::new(1, -2, 3));
        let canonical = edit.encode_canonical();
        let original = edit.content_tag();

        for index in 0..EDIT_CANONICAL_LEN {
            let mut mutated = canonical;
            mutated[index] ^= 1;
            if let Ok(mutated_edit) = Edit::decode_canonical(&mutated) {
                assert_ne!(mutated_edit.content_tag(), original, "byte {index}");
            }
        }
    }

    #[test]
    fn parses_comments_blank_lines_and_all_operations() {
        let source = "\r\n# setup\r\n  on 4 -5 6  # first bit\r\n\r\n\
                      off -7 8 -9\r\n  toggle 0 0 0\r\n";
        let program = Program::parse(source).unwrap();

        assert_eq!(
            program.edits(),
            [
                Edit::new(Operation::On, P),
                Edit::new(Operation::Off, Q),
                Edit::new(Operation::Toggle, Coord::new(0, 0, 0)),
            ]
        );
        assert_eq!(program.to_string(), "on 4 -5 6\noff -7 8 -9\ntoggle 0 0 0");
    }

    #[test]
    fn application_preserves_source_order() {
        let program = Program::parse(
            "on 4 -5 6\n\
             toggle 4 -5 6\n\
             off -7 8 -9\n\
             toggle -7 8 -9",
        )
        .unwrap();
        let mut world = World::empty();

        assert_eq!(program.apply(&mut world), 3);
        assert_eq!(program.len(), 4);

        assert!(!world.is_on(P));
        assert!(world.is_on(Q));
        assert_eq!(world.len(), 2);
    }

    #[test]
    fn coordinate_extremes_parse_serialize_and_apply_exactly() {
        let min = Coord::new(i64::MIN, i64::MIN, i64::MIN);
        let max = Coord::new(i64::MAX, i64::MAX, i64::MAX);
        let source = format!(
            "on {} {} {}\non {} {} {}",
            min.x, min.y, min.z, max.x, max.y, max.z
        );
        let program = Program::parse(&source).unwrap();
        let mut world = World::empty();

        assert_eq!(program.apply(&mut world), 2);

        assert_eq!(program.to_string(), source);
        assert!(world.is_on(min));
        assert!(world.is_on(max));
        assert_eq!(world.len(), 3);
    }

    #[test]
    fn canonical_serialization_round_trips_to_the_same_program() {
        let source = " # discarded\n\
                      on +0 0001 -2 # normalized\n\
                      toggle -9223372036854775808 0 9223372036854775807\n\
                      off 8 9 10\n";
        let parsed = Program::parse(source).unwrap();
        let canonical = parsed.to_string();

        assert_eq!(
            canonical,
            "on 0 1 -2\ntoggle -9223372036854775808 0 9223372036854775807\noff 8 9 10"
        );
        assert_eq!(Program::parse(&canonical).unwrap(), parsed);
    }

    #[test]
    fn empty_and_comment_only_sources_are_the_empty_program() {
        for source in ["", "\n\r\n", "# nothing\n  # still nothing"] {
            let program = Program::parse(source).unwrap();
            assert!(program.is_empty());
            assert_eq!(program.len(), 0);
            assert_eq!(program.to_string(), "");
        }
    }

    #[test]
    fn malformed_input_is_rejected_with_its_exact_line_and_reason() {
        let cases = [
            ("on 1 2", 1, ParseErrorKind::WrongFieldCount { found: 3 }),
            (
                "off 1 2 3 extra",
                1,
                ParseErrorKind::WrongFieldCount { found: 5 },
            ),
            (
                "paint 1 2 3",
                1,
                ParseErrorKind::UnknownOperation("paint".to_owned()),
            ),
            (
                "on nope 2 3",
                1,
                ParseErrorKind::InvalidCoordinate {
                    axis: Axis::X,
                    value: "nope".to_owned(),
                },
            ),
            (
                "off 1 9223372036854775808 3",
                1,
                ParseErrorKind::InvalidCoordinate {
                    axis: Axis::Y,
                    value: "9223372036854775808".to_owned(),
                },
            ),
            (
                "toggle 1 2 -9223372036854775809",
                1,
                ParseErrorKind::InvalidCoordinate {
                    axis: Axis::Z,
                    value: "-9223372036854775809".to_owned(),
                },
            ),
        ];

        for (source, line, kind) in cases {
            let error = Program::parse(source).unwrap_err();
            assert_eq!(error.line(), line, "source: {source}");
            assert_eq!(error.kind(), &kind, "source: {source}");
        }
    }

    #[test]
    fn parser_reports_the_physical_line_after_ignored_lines() {
        let error = Program::parse("# one\n\n on 0 0 0\n\nOFF 1 2 3")
            .expect_err("operation names are deliberately case-sensitive");

        assert_eq!(error.line(), 5);
        assert_eq!(
            error.kind(),
            &ParseErrorKind::UnknownOperation("OFF".to_owned())
        );
        assert_eq!(
            error.to_string(),
            "line 5: unknown operation 'OFF' (expected on, off, or toggle)"
        );
    }

    #[test]
    fn program_can_be_built_and_iterated_without_text() {
        let edits = [Edit::new(Operation::On, P), Edit::new(Operation::Toggle, Q)];
        let mut program = Program::new();
        program.push(edits[0]);
        program.push(edits[1]);

        assert_eq!(program.iter().copied().collect::<Vec<_>>(), edits);
        assert_eq!(program.clone().into_iter().collect::<Vec<_>>(), edits);
        assert_eq!(Program::from_edits(edits), program);
        assert_eq!(edits.into_iter().collect::<Program>(), program);
    }

    #[test]
    fn application_reports_no_ops_without_discarding_them() {
        let program = Program::parse("on 4 -5 6\non 4 -5 6\noff 9 9 9").unwrap();
        let mut world = World::empty();

        assert_eq!(program.apply(&mut world), 1);
        assert_eq!(program.len(), 3);
        assert!(world.is_on(P));
    }

    #[test]
    fn bedrock_mutation_attempts_are_retained_no_ops() {
        let program = Program::parse("off 0 0 0\ntoggle 0 0 0").unwrap();
        let mut world = World::new();

        assert_eq!(program.apply(&mut world), 0);
        assert_eq!(program.len(), 2);
        assert!(world.is_on(Coord::new(0, 0, 0)));
        assert_eq!(world.len(), 1);
    }

    #[test]
    fn tick_batch_retains_resolved_no_ops_and_counts_unresolved_attempts() {
        let edits = [Edit::new(Operation::On, P), Edit::new(Operation::On, P)];
        let batch = TickBatch::new(17, 3, edits);
        let mut world = World::empty();

        assert_eq!(batch.tick(), 17);
        assert_eq!(batch.attempts(), 3);
        assert_eq!(batch.misses(), 1);
        assert_eq!(batch.resolved_edits(), edits);
        assert_eq!(batch.replay(&mut world), 1);
        assert!(world.is_on(P));
    }
}
