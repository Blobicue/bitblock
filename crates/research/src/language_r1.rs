//! `LANGUAGE-R1`: strict `+X/+Y/+Z` framed 3D spatial grammar parser, canonical AST serializer,
//! and voxel encoder over Bitblock's 1-bit sparse `World`.
//!
//! Implements the spatial grammar contract frozen in `docs/LANGUAGE_R1_DECISION.md`:
//! - **Canonical `+X/+Y/+Z` orientation:** every spatial program is anchored at `anchor` inside an
//!   axis-aligned bounding frame marked by a strictly chiral 3-axis L-fiducial `(len +X=4, +Y=3,
//!   +Z=2)` at local `(0, 0, 0)`. Any rotation or reflection (all 47 non-identity octahedral
//!   symmetries) fails closed as [`SpatialParseError::InvalidOrientationOrReflection`].
//! - **Exact delimiter & glyph framing:** outer bounding corners `(frame_x-1, 0, 0)`,
//!   `(0, 5, 0)`, `(frame_x-1, 5, 2)` frame a 4-bit `namespace_id` (`y=4, z=0`), a 4-bit
//!   `instruction_count` (`1..=12`, `y=5, z=0`), a clear spacer plane (`z=1`, empty except
//!   `(0,0,1)`), and `instruction_count` visually legible 3×3×1 opcode glyphs on `z=2` separated
//!   by empty delimiter columns.
//! - **Local VM authority boundary:** parsed programs carry zero randomness and execute first
//!   strictly as opt-in local deterministic VM input (`VM-D1`/`VM-1`), never v1 consensus code.

use std::fmt;

use crate::history::world_tag;
use crate::protocol::{ContentTag, TagKind, hash_canonical};
use crate::world::{Coord, World};

/// Canonical schema string for a serialized [`ParsedSpatialProgramV1`] AST.
pub const LANGUAGE_R1_PROGRAM_SCHEMA: &[u8] = b"bitblock/language-r1/parsed-program/v1";
/// Canonical schema string for a deterministic [`SpatialParseReportV1`] world scan receipt.
pub const LANGUAGE_R1_PARSE_REPORT_SCHEMA: &[u8] = b"bitblock/language-r1/parse-report/v1";

/// Grammar version pinned by `LANGUAGE-R1`.
pub const SPATIAL_GRAMMAR_VERSION: u16 = 1;
/// Fixed frame height along `+Y` (`6` voxels: `y in 0..6`).
pub const SPATIAL_FRAME_Y: u16 = 6;
/// Fixed frame depth along `+Z` (`3` voxels: `z in 0..3`).
pub const SPATIAL_FRAME_Z: u16 = 3;
/// Maximum instruction glyphs per framed spatial program (`12`).
pub const MAX_SPATIAL_INSTRUCTIONS: usize = 12;

/// Typed fail-closed parse errors classifying every invalid spatial pattern explicitly.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SpatialParseError {
    /// Expected chiral `+X/+Y/+Z` L-fiducial was absent at the anchor.
    MissingFiducial,
    /// Detected a rotated or reflected L-fiducial rather than canonical `+X/+Y/+Z` orientation.
    InvalidOrientationOrReflection,
    /// Outer frame corner delimiter bit was missing or inconsistent.
    MissingDelimiter(&'static str),
    /// Header encoded an instruction count outside `1..=MAX_SPATIAL_INSTRUCTIONS`.
    InvalidInstructionCount(u8),
    /// Stray occupied voxel appeared in a reserved spacer plane or delimiter column.
    DelimiterCollision(Coord),
    /// 3×3 glyph slot contained an unrecognized 9-bit voxel bitmap.
    UnknownGlyphBitmap { slot: usize, bitmap: u16 },
    /// Two parsed spatial program bounding boxes overlapped in `i64³`.
    OverlappingProgramFrames,
    /// Wire encoding was invalid or had trailing bytes.
    InvalidWire(&'static str),
}

impl fmt::Display for SpatialParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}

impl std::error::Error for SpatialParseError {}

/// Canonical 3×3×1 voxel opcodes recognized by `LANGUAGE-R1` (1-to-1 bijection with 9-bit glyph
/// bitmaps, bit index `row * 3 + col` where `row = y - 1 in 0..3` bottom-to-top and `col = dx in 0..3`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(u8)]
pub enum SpatialOpcodeV1 {
    /// Single center dot (`0b000_010_000`).
    Nop = 0,
    /// Vertical bar `|` (`0b010_010_010`): push integer `1`.
    PushOne = 1,
    /// Hollow `O` ring (`0b111_101_111`): push integer `0`.
    PushZero = 2,
    /// Plus `+` cross (`0b010_111_010`): pop two integers and push checked sum.
    Add = 3,
    /// Horizontal bar `-` (`0b000_111_000`): pop two integers and push checked difference.
    Sub = 4,
    /// Double bars `||` (`0b101_101_101`): duplicate top of stack.
    Dup = 5,
    /// Diagonal `X` (`0b101_010_101`): swap top two stack elements.
    Swap = 6,
    /// Solid 3×3 square `#` (`0b111_111_111`): halt deterministically.
    Halt = 7,
}

impl SpatialOpcodeV1 {
    /// Returns the canonical 1-byte wire code.
    pub const fn code(self) -> u8 {
        self as u8
    }

    /// Decodes a canonical 1-byte opcode wire code.
    pub const fn from_code(code: u8) -> Option<Self> {
        match code {
            0 => Some(Self::Nop),
            1 => Some(Self::PushOne),
            2 => Some(Self::PushZero),
            3 => Some(Self::Add),
            4 => Some(Self::Sub),
            5 => Some(Self::Dup),
            6 => Some(Self::Swap),
            7 => Some(Self::Halt),
            _ => None,
        }
    }

    /// Returns the unique 9-bit 3×3 voxel glyph bitmap for this opcode.
    pub const fn glyph_bitmap(self) -> u16 {
        match self {
            Self::Nop => 0b000_010_000,
            Self::PushOne => 0b010_010_010,
            Self::PushZero => 0b111_101_111,
            Self::Add => 0b010_111_010,
            Self::Sub => 0b000_111_000,
            Self::Dup => 0b101_101_101,
            Self::Swap => 0b101_010_101,
            Self::Halt => 0b111_111_111,
        }
    }

    /// Maps an exact 9-bit 3×3 glyph bitmap back to its unique [`SpatialOpcodeV1`].
    pub const fn from_glyph_bitmap(bitmap: u16) -> Option<Self> {
        match bitmap {
            0b000_010_000 => Some(Self::Nop),
            0b010_010_010 => Some(Self::PushOne),
            0b111_101_111 => Some(Self::PushZero),
            0b010_111_010 => Some(Self::Add),
            0b000_111_000 => Some(Self::Sub),
            0b101_101_101 => Some(Self::Dup),
            0b101_010_101 => Some(Self::Swap),
            0b111_111_111 => Some(Self::Halt),
            _ => None,
        }
    }
}

/// Canonical parsed AST for one strict `+X/+Y/+Z` framed 3D spatial program.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct ParsedSpatialProgramV1 {
    /// Spatial grammar version (`1`).
    pub grammar_version: u16,
    /// 4-bit program namespace identifier (`0..=15`).
    pub namespace_id: u8,
    /// World-space anchor coordinate of the chiral L-fiducial `(0, 0, 0)`.
    pub anchor: Coord,
    /// Axis-aligned bounding-box width along `+X` (`4 * instructions.len() + 3`).
    pub frame_x: u16,
    /// Ordered opcode stream parsed left-to-right along `+X`.
    pub instructions: Vec<SpatialOpcodeV1>,
}

impl ParsedSpatialProgramV1 {
    /// Constructs a validated spatial program AST with exact derived `frame_x`.
    pub fn new(
        namespace_id: u8,
        anchor: Coord,
        instructions: Vec<SpatialOpcodeV1>,
    ) -> Result<Self, SpatialParseError> {
        if namespace_id > 15 {
            return Err(SpatialParseError::InvalidWire(
                "namespace_id must fit in 4 bits (0..=15)",
            ));
        }
        if instructions.is_empty() || instructions.len() > MAX_SPATIAL_INSTRUCTIONS {
            return Err(SpatialParseError::InvalidInstructionCount(
                instructions.len() as u8,
            ));
        }
        let frame_x = (4 * instructions.len() + 3) as u16;
        Ok(Self {
            grammar_version: SPATIAL_GRAMMAR_VERSION,
            namespace_id,
            anchor,
            frame_x,
            instructions,
        })
    }

    /// Encodes the translation-normalized AST into canonical big-endian bytes (excluding world
    /// `anchor` so identical translated programs share one canonical source identity!).
    pub fn encode_canonical_source(&self) -> Result<Vec<u8>, SpatialParseError> {
        if self.grammar_version != SPATIAL_GRAMMAR_VERSION || self.namespace_id > 15 {
            return Err(SpatialParseError::InvalidWire(
                "invalid grammar_version or namespace_id",
            ));
        }
        if self.instructions.is_empty() || self.instructions.len() > MAX_SPATIAL_INSTRUCTIONS {
            return Err(SpatialParseError::InvalidInstructionCount(
                self.instructions.len() as u8,
            ));
        }
        let expected_fx = (4 * self.instructions.len() + 3) as u16;
        if self.frame_x != expected_fx {
            return Err(SpatialParseError::InvalidWire("frame_x mismatch"));
        }
        let mut out = Vec::with_capacity(10 + self.instructions.len());
        out.extend_from_slice(&self.grammar_version.to_be_bytes());
        out.push(self.namespace_id);
        out.extend_from_slice(&self.frame_x.to_be_bytes());
        out.extend_from_slice(&SPATIAL_FRAME_Y.to_be_bytes());
        out.extend_from_slice(&SPATIAL_FRAME_Z.to_be_bytes());
        out.push(self.instructions.len() as u8);
        for &op in &self.instructions {
            out.push(op.code());
        }
        Ok(out)
    }

    /// Decodes a translation-normalized canonical source payload, attaching `anchor`.
    pub fn decode_canonical_source(bytes: &[u8], anchor: Coord) -> Result<Self, SpatialParseError> {
        if bytes.len() < 10 {
            return Err(SpatialParseError::InvalidWire(
                "truncated spatial program source",
            ));
        }
        let grammar_version = u16::from_be_bytes([bytes[0], bytes[1]]);
        let namespace_id = bytes[2];
        let frame_x = u16::from_be_bytes([bytes[3], bytes[4]]);
        let frame_y = u16::from_be_bytes([bytes[5], bytes[6]]);
        let frame_z = u16::from_be_bytes([bytes[7], bytes[8]]);
        let count = bytes[9] as usize;
        if grammar_version != SPATIAL_GRAMMAR_VERSION
            || frame_y != SPATIAL_FRAME_Y
            || frame_z != SPATIAL_FRAME_Z
        {
            return Err(SpatialParseError::InvalidWire(
                "unexpected grammar version or frame dimensions",
            ));
        }
        if count == 0 || count > MAX_SPATIAL_INSTRUCTIONS || bytes.len() != 10 + count {
            return Err(SpatialParseError::InvalidWire(
                "invalid instruction count or trailing bytes",
            ));
        }
        let mut instructions = Vec::with_capacity(count);
        for &b in &bytes[10..] {
            let op = SpatialOpcodeV1::from_code(b)
                .ok_or(SpatialParseError::InvalidWire("unknown opcode byte"))?;
            instructions.push(op);
        }
        let prog = Self::new(namespace_id, anchor, instructions)?;
        if prog.frame_x != frame_x {
            return Err(SpatialParseError::InvalidWire("frame_x mismatch"));
        }
        Ok(prog)
    }

    /// Computes the translation-invariant canonical `ContentTag` of this parsed spatial program.
    pub fn source_tag(&self) -> Result<ContentTag, SpatialParseError> {
        let bytes = self.encode_canonical_source()?;
        Ok(hash_canonical(
            TagKind::Policy,
            LANGUAGE_R1_PROGRAM_SCHEMA,
            bytes.len() as u64,
            |w| w(&bytes),
        )
        .expect("static schema and bounded length"))
    }
}

/// Deterministic scan report over a `World` committing to parsed program tags (`LANGUAGE-R1`).
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct SpatialParseReportV1 {
    /// Canonical `world_tag` digest of the scanned world.
    pub world_digest: [u8; 32],
    /// Sorted `(anchor, source_tag_digest)` pairs for every parsed program frame.
    pub programs: Vec<(Coord, [u8; 32])>,
}

impl SpatialParseReportV1 {
    /// Encodes the parse report into canonical big-endian bytes.
    pub fn encode_canonical(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(32 + 4 + self.programs.len() * 56);
        out.extend_from_slice(&self.world_digest);
        out.extend_from_slice(&(self.programs.len() as u32).to_be_bytes());
        for &(anchor, digest) in &self.programs {
            out.extend_from_slice(&anchor.x.to_be_bytes());
            out.extend_from_slice(&anchor.y.to_be_bytes());
            out.extend_from_slice(&anchor.z.to_be_bytes());
            out.extend_from_slice(&digest);
        }
        out
    }

    /// Computes the canonical `ContentTag` for this parse report.
    pub fn tag(&self) -> ContentTag {
        let bytes = self.encode_canonical();
        hash_canonical(
            TagKind::Receipt,
            LANGUAGE_R1_PARSE_REPORT_SCHEMA,
            bytes.len() as u64,
            |w| w(&bytes),
        )
        .expect("static schema and bounded length")
    }
}

/// Returns the exact local offsets of the chiral `+X/+Y/+Z` L-fiducial (`7` voxels).
pub const fn chiral_fiducial_offsets() -> [(i64, i64, i64); 7] {
    [
        (0, 0, 0),
        (1, 0, 0),
        (2, 0, 0),
        (3, 0, 0),
        (0, 1, 0),
        (0, 2, 0),
        (0, 0, 1),
    ]
}

/// Computes the exact world-space voxel set representing `program`.
pub fn spatial_program_voxels(
    program: &ParsedSpatialProgramV1,
) -> Result<Vec<Coord>, SpatialParseError> {
    let _ = program.encode_canonical_source()?;
    let a = program.anchor;
    let at = |dx: i64, dy: i64, dz: i64| Coord::new(a.x + dx, a.y + dy, a.z + dz);
    let mut coords = Vec::new();
    // 1. Chiral L-fiducial at (0,0,0)
    for (dx, dy, dz) in chiral_fiducial_offsets() {
        coords.push(at(dx, dy, dz));
    }
    // 2. Outer corner frame delimiters
    let fx = i64::from(program.frame_x);
    coords.push(at(fx - 1, 0, 0));
    coords.push(at(0, 5, 0));
    coords.push(at(fx - 1, 5, 2));
    // 3. Header bits on z = 0: 4-bit namespace_id (y = 4, x = 2..6), 4-bit count (y = 5, x = 2..6)
    let count = program.instructions.len() as u8;
    for bit in 0..4_u8 {
        if ((program.namespace_id >> bit) & 1) != 0 {
            coords.push(at(2 + i64::from(bit), 4, 0));
        }
        if ((count >> bit) & 1) != 0 {
            coords.push(at(2 + i64::from(bit), 5, 0));
        }
    }
    // 4. 3x3x1 opcode glyphs on z = 2 (slot i at x = 2 + 4*i ..= 4 + 4*i, y = 1..=3)
    for (slot, &op) in program.instructions.iter().enumerate() {
        let bm = op.glyph_bitmap();
        let x_base = 2 + 4 * (slot as i64);
        for row in 0..3_u16 {
            for col in 0..3_u16 {
                if ((bm >> (row * 3 + col)) & 1) != 0 {
                    coords.push(at(x_base + i64::from(col), 1 + i64::from(row), 2));
                }
            }
        }
    }
    Ok(coords)
}

/// Stamps all voxels of `program` into `world`.
pub fn encode_program_to_world(
    world: &mut World,
    program: &ParsedSpatialProgramV1,
) -> Result<(), SpatialParseError> {
    for c in spatial_program_voxels(program)? {
        world.set_on(c);
    }
    Ok(())
}

/// Checks whether a rotated or reflected L-fiducial exists at `anchor` (for explicit error
/// classification when canonical `+X/+Y/+Z` fiducial is absent).
fn has_noncanonical_oriented_fiducial(world: &World, anchor: Coord) -> bool {
    use crate::rule_1::octahedral_transform;
    for sym in 1u8..48 {
        let all_on = chiral_fiducial_offsets().iter().all(|&(dx, dy, dz)| {
            let t = octahedral_transform(Coord::new(dx, dy, dz), sym);
            world.is_on(Coord::new(anchor.x + t.x, anchor.y + t.y, anchor.z + t.z))
        });
        if all_on {
            return true;
        }
    }
    false
}

/// Strictly parses one framed `+X/+Y/+Z` spatial program anchored at `anchor`, returning typed
/// fail-closed errors for missing/rotated fiducials, missing delimiters, delimiter collisions, or
/// unrecognized 3×3 glyph bitmaps.
pub fn parse_program_at(
    world: &World,
    anchor: Coord,
) -> Result<ParsedSpatialProgramV1, SpatialParseError> {
    let at = |dx: i64, dy: i64, dz: i64| Coord::new(anchor.x + dx, anchor.y + dy, anchor.z + dz);

    // 1. Verify exact canonical +X/+Y/+Z chiral L-fiducial (and clear tip termination gaps).
    let fiducial_ok = chiral_fiducial_offsets()
        .iter()
        .all(|&(dx, dy, dz)| world.is_on(at(dx, dy, dz)));
    if !fiducial_ok {
        return if has_noncanonical_oriented_fiducial(world, anchor) {
            Err(SpatialParseError::InvalidOrientationOrReflection)
        } else {
            Err(SpatialParseError::MissingFiducial)
        };
    }
    for (gx, gy, gz) in [(4, 0, 0), (0, 3, 0), (0, 0, 2)] {
        if world.is_on(at(gx, gy, gz)) {
            return Err(SpatialParseError::DelimiterCollision(at(gx, gy, gz)));
        }
    }

    // 2. Verify top-left-front delimiter and decode 4-bit header fields on z = 0.
    if !world.is_on(at(0, 5, 0)) {
        return Err(SpatialParseError::MissingDelimiter("missing (0, 5, 0)"));
    }
    let mut namespace_id = 0u8;
    let mut inst_count = 0u8;
    for bit in 0..4_u8 {
        if world.is_on(at(2 + i64::from(bit), 4, 0)) {
            namespace_id |= 1 << bit;
        }
        if world.is_on(at(2 + i64::from(bit), 5, 0)) {
            inst_count |= 1 << bit;
        }
    }
    if inst_count == 0 || usize::from(inst_count) > MAX_SPATIAL_INSTRUCTIONS {
        return Err(SpatialParseError::InvalidInstructionCount(inst_count));
    }

    // 3. Verify right-side outer frame corner delimiters.
    let frame_x = 4 * i64::from(inst_count) + 3;
    if !world.is_on(at(frame_x - 1, 0, 0)) {
        return Err(SpatialParseError::MissingDelimiter(
            "missing (frame_x - 1, 0, 0)",
        ));
    }
    if !world.is_on(at(frame_x - 1, 5, 2)) {
        return Err(SpatialParseError::MissingDelimiter(
            "missing (frame_x - 1, 5, 2)",
        ));
    }

    // 4. Verify reserved spacer plane z = 1 (must be completely clear except fiducial (0,0,1))
    //    and unused cells of z = 0 outside fiducial/header/delimiters.
    for dx in 0..frame_x {
        for dy in 0..i64::from(SPATIAL_FRAME_Y) {
            if !(dx == 0 && dy == 0) && world.is_on(at(dx, dy, 1)) {
                return Err(SpatialParseError::DelimiterCollision(at(dx, dy, 1)));
            }
            let allowed_z0 = (dy == 0 && (dx <= 3 || dx == frame_x - 1))
                || (dx == 0 && (dy <= 2 || dy == 5))
                || ((dy == 4 || dy == 5) && (2..6).contains(&dx));
            if !allowed_z0 && world.is_on(at(dx, dy, 0)) {
                return Err(SpatialParseError::DelimiterCollision(at(dx, dy, 0)));
            }
        }
    }

    // 5. Verify delimiter borders on z = 2 and decode each 3x3 glyph slot.
    for dx in 0..frame_x {
        for dy in 0..i64::from(SPATIAL_FRAME_Y) {
            let in_glyph_y = (1..=3).contains(&dy);
            let is_corner = dx == frame_x - 1 && dy == 5;
            let in_glyph_x = dx >= 2 && dx < frame_x - 1 && ((dx - 2) % 4) != 3;
            if !(is_corner || (in_glyph_y && in_glyph_x)) && world.is_on(at(dx, dy, 2)) {
                return Err(SpatialParseError::DelimiterCollision(at(dx, dy, 2)));
            }
        }
    }

    let mut instructions = Vec::with_capacity(usize::from(inst_count));
    for slot in 0..usize::from(inst_count) {
        let x_base = 2 + 4 * (slot as i64);
        let mut bitmap = 0u16;
        for row in 0..3_u16 {
            for col in 0..3_u16 {
                if world.is_on(at(x_base + i64::from(col), 1 + i64::from(row), 2)) {
                    bitmap |= 1 << (row * 3 + col);
                }
            }
        }
        let op = SpatialOpcodeV1::from_glyph_bitmap(bitmap)
            .ok_or(SpatialParseError::UnknownGlyphBitmap { slot, bitmap })?;
        instructions.push(op);
    }

    ParsedSpatialProgramV1::new(namespace_id, anchor, instructions)
}

/// Scans an explicit list of candidate anchors in `world`, parses every framed program, rejects
/// any overlapping 3D bounding boxes fail-closed, and returns a canonical [`SpatialParseReportV1`].
pub fn parse_world_programs(
    world: &World,
    anchors: &[Coord],
) -> Result<(Vec<ParsedSpatialProgramV1>, SpatialParseReportV1), SpatialParseError> {
    let mut parsed = Vec::with_capacity(anchors.len());
    for &anchor in anchors {
        parsed.push(parse_program_at(world, anchor)?);
    }
    parsed.sort_by_key(|p| (p.anchor.x, p.anchor.y, p.anchor.z));
    // Reject any overlapping bounding boxes (including identical anchors).
    for i in 0..parsed.len() {
        for j in (i + 1)..parsed.len() {
            let a = &parsed[i];
            let b = &parsed[j];
            let overlap_x = a.anchor.x < b.anchor.x + i64::from(b.frame_x)
                && b.anchor.x < a.anchor.x + i64::from(a.frame_x);
            let overlap_y = a.anchor.y < b.anchor.y + i64::from(SPATIAL_FRAME_Y)
                && b.anchor.y < a.anchor.y + i64::from(SPATIAL_FRAME_Y);
            let overlap_z = a.anchor.z < b.anchor.z + i64::from(SPATIAL_FRAME_Z)
                && b.anchor.z < a.anchor.z + i64::from(SPATIAL_FRAME_Z);
            if overlap_x && overlap_y && overlap_z {
                return Err(SpatialParseError::OverlappingProgramFrames);
            }
        }
    }
    let mut entries = Vec::with_capacity(parsed.len());
    for p in &parsed {
        entries.push((p.anchor, p.source_tag()?.digest()));
    }
    let report = SpatialParseReportV1 {
        world_digest: world_tag(world).digest(),
        programs: entries,
    };
    Ok((parsed, report))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rule_1::octahedral_transform;

    #[test]
    fn spatial_program_roundtrip_translation_invariance_and_rotation_rejection() {
        let ops = vec![
            SpatialOpcodeV1::PushOne,
            SpatialOpcodeV1::PushOne,
            SpatialOpcodeV1::Add,
            SpatialOpcodeV1::Dup,
            SpatialOpcodeV1::Add,
            SpatialOpcodeV1::Halt,
        ];
        let p1 = ParsedSpatialProgramV1::new(3, Coord::new(10, 0, 5), ops.clone()).unwrap();
        let p2 = ParsedSpatialProgramV1::new(3, Coord::new(-50, 12, -30), ops).unwrap();
        assert_eq!(p1.source_tag().unwrap(), p2.source_tag().unwrap());

        let mut world = World::new();
        encode_program_to_world(&mut world, &p1).unwrap();
        encode_program_to_world(&mut world, &p2).unwrap();
        let (parsed, report) =
            parse_world_programs(&world, &[Coord::new(10, 0, 5), Coord::new(-50, 12, -30)])
                .unwrap();
        assert_eq!(parsed.len(), 2);
        assert_eq!(report.programs.len(), 2);

        // Every one of the 47 non-identity rotations/reflections must fail closed!
        let voxels = spatial_program_voxels(&p1).unwrap();
        for sym in 1u8..48 {
            let mut rot_world = World::new();
            let base = Coord::new(100, 100, 100);
            for c in &voxels {
                let rel = Coord::new(c.x - p1.anchor.x, c.y - p1.anchor.y, c.z - p1.anchor.z);
                let t = octahedral_transform(rel, sym);
                rot_world.set_on(Coord::new(base.x + t.x, base.y + t.y, base.z + t.z));
            }
            assert_eq!(
                parse_program_at(&rot_world, base),
                Err(SpatialParseError::InvalidOrientationOrReflection),
                "non-canonical orientation sym={sym} was not rejected"
            );
        }
    }
}
