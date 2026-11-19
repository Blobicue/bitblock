//! §6ah lagged chem: the chem engine publishes one batch behind the play session, in a Web
//! Worker or (fallback) on the main thread, with the same schedule and the same results.
//!
//! The play session never edits chem state itself. Each tick's chem edits are resolved on the
//! main thread (aim ray, enclose-observer check, cut refusal) against the play world as last
//! published, and queued as [`ChemOp`]s in tick order. On a send tick the queued ops and the
//! chem steps due then form one [`ChemBatch`]; the engine applies the ops in order and then
//! runs the steps ([`ChemPlay::process`]). The result is published on the batch's landing
//! tick: the next tick a chem step is due (auto-step running), else the next tick. Landing
//! mirrors the engine's materials into the play world inside that tick's transaction, and
//! the tick then sends the next batch. So with the default cadence the play world shows the
//! chem world one chem step (four ticks) behind, and an edit appears at the landing after
//! the batch that carried it.
//!
//! Edits are re-validated by the engine against the chem state at application, which is the
//! state the play world shows from that same tick on: a dig needs a non-air chem cell with
//! `Q >= BIND`, a place an air (or outside-domain, unoccupied) cell and an affordable bag.
//! A failed check refuses that edit (`chem_last` says why) and changes nothing. Ops on one
//! cell apply in order (a second dig of a dug cell is refused), so every edit is applied or
//! refused exactly once: none is lost or duplicated, and the bag pays for exactly what was
//! placed.
//!
//! Worker path: the main session keeps a display replica of the chem world (cells,
//! materials, occupancy; [`ChemPlay::replicate`]) and exchanges [`encode_batch`] /
//! [`encode_delta`] bytes with a second wasm instance in the worker, which owns the real chem
//! world ([`ChemPlay::process`]). Fallback: the main session's own chem world is the engine
//! and processes each batch at its landing tick. Between landings neither engine changes the
//! chem state, so both paths publish identical states on identical ticks, and the play
//! world, frames, history and readouts are identical.

use bitblock_core::chem::{Lanes, MatId, NE};
use bitblock_core::program::Operation;
use bitblock_core::world::Coord;

use crate::play_chem::ChemPlay;

/// One chem edit or control, applied by the engine in queue order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ChemOp {
    /// Aim delete resolved to a cell: [`ChemPlay::dig`].
    Dig(Coord),
    /// Aim place resolved to a cell: [`ChemPlay::place`].
    Place(Coord),
    /// Exact edit on a cell: off digs an on-bit, on places into an off-bit, toggle by the bit
    /// (the chem world's bit at application).
    Exact(Operation, Coord),
    /// A refusal decided on the main thread, reported through `chem_last` in queue order.
    Refuse(String),
    /// [`ChemPlay::set_dynamics`] (validated on the main thread).
    Dynamics(u32, u32),
}

/// What the engine does for one send tick: the ops queued since the last send, then `steps`
/// chem steps.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ChemBatch {
    pub steps: u32,
    pub ops: Vec<ChemOp>,
}

/// The engine-side figures of the chem readouts, as of the last landed batch.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChemReport {
    pub tick: u64,
    pub steps_run: u64,
    pub vapor: i64,
    pub evaporated: u64,
    pub rained: u64,
    pub material_count: u64,
    pub domain_len: u64,
    pub space_q: i64,
    pub energy: i64,
    pub energy_drift: i64,
    pub books_ok: bool,
    pub lanes: Lanes,
    pub lane_drift: Lanes,
    pub inventory: Lanes,
    pub place_mat: MatId,
    pub last: String,
    pub error: Option<String>,
}

impl Default for ChemReport {
    fn default() -> Self {
        Self {
            tick: 0,
            steps_run: 0,
            vapor: 0,
            evaporated: 0,
            rained: 0,
            material_count: 0,
            domain_len: 0,
            space_q: 0,
            energy: 0,
            energy_drift: 0,
            books_ok: true,
            lanes: [0; NE],
            lane_drift: [0; NE],
            inventory: [0; NE],
            place_mat: MatId::AIR,
            last: String::new(),
            error: None,
        }
    }
}

/// The conservation figures of a report (one full pass over the chem cells plus the
/// occupancy walk of the books check). §6ah: a report carries the figures of the engine state
/// *before* its batch, so the worker can compute them while it waits for the next batch (off
/// the critical path, [`ChemPlay::precompute_books`]); every state is still checked, one
/// landing later.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Books {
    pub material_count: u64,
    pub energy: i64,
    pub energy_drift: i64,
    pub books_ok: bool,
    pub lanes: Lanes,
    pub lane_drift: Lanes,
}

impl Books {
    pub fn of(c: &ChemPlay) -> Self {
        let books = c.world.book_totals();
        Self {
            material_count: books.material_count as u64,
            energy: books.energy,
            energy_drift: books.energy_drift,
            books_ok: c.world.check_books_with(&books).is_ok(),
            lanes: books.lanes,
            lane_drift: books.lane_drift,
        }
    }
}

impl ChemReport {
    /// Every figure from `c` as it is now (genesis / tests).
    pub fn of(c: &ChemPlay) -> Self {
        Self::with_books(c, Books::of(c))
    }

    /// `c`'s counters and readouts now, with the conservation figures `b`.
    pub fn with_books(c: &ChemPlay, b: Books) -> Self {
        Self {
            tick: c.world.tick(),
            steps_run: c.steps_run,
            vapor: c.world.vapor(),
            evaporated: c.world.evaporated(),
            rained: c.world.rained(),
            material_count: b.material_count,
            domain_len: c.world.domain_len() as u64,
            space_q: c.world.space_q(),
            energy: b.energy,
            energy_drift: b.energy_drift,
            books_ok: b.books_ok,
            lanes: b.lanes,
            lane_drift: b.lane_drift,
            inventory: c.world.inventory(),
            place_mat: c.place_mat,
            last: c.last.clone(),
            error: c.error.clone(),
        }
    }
}

/// An engine result for the worker path: the cells the batch added to the domain (in the
/// engine's insertion order), every domain cell's material after it, the readout figures and
/// how many edits it refused.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ChemDelta {
    pub inserted: Vec<Coord>,
    pub mats: Vec<MatId>,
    pub report: ChemReport,
    pub refused: u32,
}

/// The schedule state of a chem play session (both paths).
#[derive(Debug, Default)]
pub struct ChemLink {
    /// Ops queued since the last send, in tick order.
    pub pending: Vec<ChemOp>,
    /// The batch sent and not yet landed (at most one).
    pub in_flight: Option<ChemBatch>,
    /// Worker path: results arrive through `inbox`; the main chem world is a replica.
    pub remote: bool,
    /// Worker path: the encoded batch of the last send tick, until the client takes it.
    pub outbox: Option<Vec<u8>>,
    /// Worker path: the encoded result of `in_flight`, once the client delivered it.
    pub inbox: Option<Vec<u8>>,
    /// Readout figures as of the last landing (genesis before the first).
    pub report: ChemReport,
    pub sent: u64,
    pub landed: u64,
    /// Remote-only recovery journal: precisely the batches whose results were published.
    /// Replayed from genesis on worker failure; the still-in-flight batch is NOT replayed.
    pub published_batches: Vec<ChemBatch>,
}

impl ChemLink {
    pub fn new(chem: &ChemPlay) -> Self {
        Self {
            report: ChemReport::of(chem),
            ..Self::default()
        }
    }

    /// Whether the batch in flight lands on sim tick `tick`.
    pub fn lands_on(&self, chem: &ChemPlay, tick: u64) -> bool {
        self.in_flight.is_some() && (chem.due_on(tick) || !chem.auto_running())
    }

    /// Whether tick `tick` sends a batch (evaluated after its landing, if any).
    pub fn sends_on(&self, chem: &ChemPlay, tick: u64) -> bool {
        self.in_flight.is_none()
            && (chem.due_on(tick) || (!chem.auto_running() && !self.pending.is_empty()))
    }

    /// Worker path: tick `tick` must wait for the result of the batch in flight.
    pub fn blocked(&self, chem: &ChemPlay, tick: u64) -> bool {
        self.remote && self.inbox.is_none() && self.lands_on(chem, tick)
    }
}

const BATCH_MAGIC: u32 = 0x4243_4231; // "1BCB"
const DELTA_MAGIC: u32 = 0x4443_4231; // "1BCD"

struct Writer(Vec<u8>);

impl Writer {
    fn u8(&mut self, v: u8) {
        self.0.push(v);
    }
    fn u32(&mut self, v: u32) {
        self.0.extend_from_slice(&v.to_le_bytes());
    }
    fn u64(&mut self, v: u64) {
        self.0.extend_from_slice(&v.to_le_bytes());
    }
    fn i64(&mut self, v: i64) {
        self.0.extend_from_slice(&v.to_le_bytes());
    }
    fn coord(&mut self, c: Coord) {
        self.i64(c.x);
        self.i64(c.y);
        self.i64(c.z);
    }
    fn lanes(&mut self, l: &Lanes) {
        for &v in l {
            self.i64(v);
        }
    }
    fn str(&mut self, s: &str) {
        self.u32(s.len() as u32);
        self.0.extend_from_slice(s.as_bytes());
    }
}

struct Reader<'a>(&'a [u8]);

impl Reader<'_> {
    fn take(&mut self, n: usize) -> Option<&[u8]> {
        if self.0.len() < n {
            return None;
        }
        let (a, b) = self.0.split_at(n);
        self.0 = b;
        Some(a)
    }
    fn u8(&mut self) -> Option<u8> {
        self.take(1).map(|b| b[0])
    }
    fn u32(&mut self) -> Option<u32> {
        self.take(4)
            .map(|b| u32::from_le_bytes(b.try_into().unwrap()))
    }
    fn u64(&mut self) -> Option<u64> {
        self.take(8)
            .map(|b| u64::from_le_bytes(b.try_into().unwrap()))
    }
    fn i64(&mut self) -> Option<i64> {
        self.take(8)
            .map(|b| i64::from_le_bytes(b.try_into().unwrap()))
    }
    fn coord(&mut self) -> Option<Coord> {
        Some(Coord::new(self.i64()?, self.i64()?, self.i64()?))
    }
    fn lanes(&mut self) -> Option<Lanes> {
        let mut l = [0; NE];
        for v in &mut l {
            *v = self.i64()?;
        }
        Some(l)
    }
    fn str(&mut self) -> Option<String> {
        let n = self.u32()? as usize;
        String::from_utf8(self.take(n)?.to_vec()).ok()
    }
}

fn op_code(op: Operation) -> u8 {
    match op {
        Operation::Off => 0,
        Operation::On => 1,
        Operation::Toggle => 2,
    }
}

pub fn encode_batch(b: &ChemBatch) -> Vec<u8> {
    let mut w = Writer(Vec::new());
    w.u32(BATCH_MAGIC);
    w.u32(b.steps);
    w.u32(b.ops.len() as u32);
    for op in &b.ops {
        match op {
            ChemOp::Dig(c) => {
                w.u8(0);
                w.coord(*c);
            }
            ChemOp::Place(c) => {
                w.u8(1);
                w.coord(*c);
            }
            ChemOp::Exact(o, c) => {
                w.u8(2);
                w.u8(op_code(*o));
                w.coord(*c);
            }
            ChemOp::Refuse(s) => {
                w.u8(3);
                w.str(s);
            }
            ChemOp::Dynamics(e, c) => {
                w.u8(4);
                w.u32(*e);
                w.u32(*c);
            }
        }
    }
    w.0
}

pub fn decode_batch(bytes: &[u8]) -> Option<ChemBatch> {
    let mut r = Reader(bytes);
    if r.u32()? != BATCH_MAGIC {
        return None;
    }
    let steps = r.u32()?;
    let n = r.u32()? as usize;
    let mut ops = Vec::with_capacity(n.min(bytes.len()));
    for _ in 0..n {
        ops.push(match r.u8()? {
            0 => ChemOp::Dig(r.coord()?),
            1 => ChemOp::Place(r.coord()?),
            2 => {
                let o = match r.u8()? {
                    0 => Operation::Off,
                    1 => Operation::On,
                    2 => Operation::Toggle,
                    _ => return None,
                };
                ChemOp::Exact(o, r.coord()?)
            }
            3 => ChemOp::Refuse(r.str()?),
            4 => ChemOp::Dynamics(r.u32()?, r.u32()?),
            _ => return None,
        });
    }
    r.0.is_empty().then_some(ChemBatch { steps, ops })
}

/// The engine's result of its last [`ChemPlay::process`] for the worker path.
pub fn encode_delta(c: &ChemPlay, report: &ChemReport, refused: u32) -> Vec<u8> {
    let mats = c.world.mats();
    let mut w = Writer(Vec::with_capacity(mats.len() + 512 + 24 * c.inserted.len()));
    w.u32(DELTA_MAGIC);
    w.u32(refused);
    w.u32(c.inserted.len() as u32);
    for &p in &c.inserted {
        w.coord(p);
    }
    w.u32(mats.len() as u32);
    w.0.extend(mats.iter().map(|m| m.0));
    let r = report;
    w.u64(r.tick);
    w.u64(r.steps_run);
    w.i64(r.vapor);
    w.u64(r.evaporated);
    w.u64(r.rained);
    w.u64(r.material_count);
    w.u64(r.domain_len);
    w.i64(r.space_q);
    w.i64(r.energy);
    w.i64(r.energy_drift);
    w.u8(u8::from(r.books_ok));
    w.lanes(&r.lanes);
    w.lanes(&r.lane_drift);
    w.lanes(&r.inventory);
    w.u8(r.place_mat.0);
    w.str(&r.last);
    match &r.error {
        Some(e) => {
            w.u8(1);
            w.str(e);
        }
        None => w.u8(0),
    }
    w.0
}

pub fn decode_delta(bytes: &[u8]) -> Option<ChemDelta> {
    let mut r = Reader(bytes);
    if r.u32()? != DELTA_MAGIC {
        return None;
    }
    let refused = r.u32()?;
    let n = r.u32()? as usize;
    let mut inserted = Vec::with_capacity(n.min(bytes.len() / 24));
    for _ in 0..n {
        inserted.push(r.coord()?);
    }
    let m = r.u32()? as usize;
    let mats = r.take(m)?.iter().map(|&b| MatId(b)).collect();
    let report = ChemReport {
        tick: r.u64()?,
        steps_run: r.u64()?,
        vapor: r.i64()?,
        evaporated: r.u64()?,
        rained: r.u64()?,
        material_count: r.u64()?,
        domain_len: r.u64()?,
        space_q: r.i64()?,
        energy: r.i64()?,
        energy_drift: r.i64()?,
        books_ok: r.u8()? != 0,
        lanes: r.lanes()?,
        lane_drift: r.lanes()?,
        inventory: r.lanes()?,
        place_mat: MatId(r.u8()?),
        last: r.str()?,
        error: match r.u8()? {
            0 => None,
            1 => Some(r.str()?),
            _ => return None,
        },
    };
    r.0.is_empty().then_some(ChemDelta {
        inserted,
        mats,
        report,
        refused,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn batch_and_delta_round_trip() {
        let b = ChemBatch {
            steps: 3,
            ops: vec![
                ChemOp::Dig(Coord::new(1, -2, 3)),
                ChemOp::Place(Coord::new(-4, 5, i64::MAX)),
                ChemOp::Exact(Operation::Toggle, Coord::new(0, 0, 0)),
                ChemOp::Exact(Operation::Off, Coord::new(7, 8, 9)),
                ChemOp::Refuse("place_refused:cut".into()),
                ChemOp::Dynamics(2, 1),
            ],
        };
        let bytes = encode_batch(&b);
        assert_eq!(decode_batch(&bytes), Some(b));
        assert_eq!(decode_batch(&bytes[..bytes.len() - 1]), None);
        let mut c = ChemPlay::new(crate::play_chem::CHEM_PRESET_OCEAN, 7, 6).unwrap();
        let (report, refused) = c.process(&ChemBatch {
            steps: 2,
            ops: vec![ChemOp::Dig(Coord::new(99, 99, 99))],
        });
        assert_eq!(refused, 1);
        assert_eq!(report.tick, 2);
        let d = decode_delta(&encode_delta(&c, &report, refused)).unwrap();
        assert_eq!(d.report, report);
        assert_eq!(d.refused, 1);
        assert_eq!(d.mats, c.world.mats());
        assert!(d.inserted.is_empty());
        assert!(d.report.books_ok);
    }
}
