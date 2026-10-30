//! The generated reaction network (`chem_network.gen.js`): a text loader,
//! planet6's mount (batch-4 minting, the mount audit, the row CSR, `NET_ROW`,
//! the table-derived `RISING` set), and the generic network law v5 (the
//! claim-rule protocol) on a [`ChemWorld`].
//!
//! Nothing here embeds the 660 KB table: [`NetTable::parse_js`] reads the
//! generated file's text at run time (or a trimmed fixture in tests).

use std::collections::BTreeMap;
use std::fmt;
use std::sync::Arc;

use super::p6::{FIRE_MAX_ROUNDS, StepError, Topology};
use super::{ChemWorld, MatId, MatRow, MatTable, NE};

/// One species row: `{ i, f, c: [H,O,C,N,S,Ca], p, u, d }`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NetSpecies {
    pub f: String,
    pub c: [i64; 6],
    /// Phase: `a` atom, `g` gas, `s` solid, `l` liquid ...
    pub p: char,
    pub u: i32,
}

/// One reaction row: `[a, sa, b, sb, pc, sc, pd, sd, dE, flags]`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct NetReaction {
    pub a: usize,
    pub sa: i64,
    pub b: usize,
    pub sb: i64,
    pub pc: usize,
    pub sc: i64,
    pub pd: usize,
    pub sd: i64,
    pub de: i64,
    pub flags: i64,
}

/// The parsed generated table.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct NetTable {
    pub species: Vec<NetSpecies>,
    pub reactions: Vec<NetReaction>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ChemNetError {
    /// The text is not a generated table (missing block, bad row ...).
    Parse(String),
    /// A reaction names a species index that does not exist.
    BadSpecies { row: usize, index: usize },
    /// planet6 "MOUNT AUDIT FAILED": one message per finding.
    Audit(Vec<String>),
    /// planet6 "MOUNT CSR: pair exceeds 64 rows".
    PairOverflow { ra: MatId, rb: MatId },
    /// Minting would run into the element-block ids (100+).
    TooManyMinted,
}

impl fmt::Display for ChemNetError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Parse(s) => write!(f, "chem network parse error: {s}"),
            Self::BadSpecies { row, index } => {
                write!(f, "reaction row {row} names missing species {index}")
            }
            Self::Audit(e) => write!(f, "MOUNT AUDIT FAILED: {}", e.join(" | ")),
            Self::PairOverflow { ra, rb } => write!(
                f,
                "MOUNT CSR: pair ({}, {}) exceeds 64 rows (cursor encoding limit)",
                ra.0, rb.0
            ),
            Self::TooManyMinted => write!(f, "network minting ran into the element ids"),
        }
    }
}

impl std::error::Error for ChemNetError {}

/// Strips `//` comments (outside strings).
fn strip_comments(src: &str) -> String {
    let mut out = String::with_capacity(src.len());
    let mut in_str: Option<char> = None;
    let mut it = src.chars().peekable();
    while let Some(ch) = it.next() {
        if let Some(q) = in_str {
            out.push(ch);
            if ch == '\\' {
                if let Some(n) = it.next() {
                    out.push(n);
                }
            } else if ch == q {
                in_str = None;
            }
            continue;
        }
        if ch == '"' || ch == '\'' {
            in_str = Some(ch);
            out.push(ch);
        } else if ch == '/' && it.peek() == Some(&'/') {
            for n in it.by_ref() {
                if n == '\n' {
                    out.push('\n');
                    break;
                }
            }
        } else {
            out.push(ch);
        }
    }
    out
}

/// The body of the `[...]` literal after `name =`.
fn array_body<'a>(src: &'a str, name: &str) -> Result<&'a str, ChemNetError> {
    let at = src
        .find(&format!("{name} = ["))
        .ok_or_else(|| ChemNetError::Parse(format!("no `{name} = [` block")))?;
    let open = at + src[at..].find('[').expect("found above");
    let mut depth = 0i32;
    let mut in_str = false;
    for (k, ch) in src[open..].char_indices() {
        match ch {
            '"' => in_str = !in_str,
            '[' | '{' if !in_str => depth += 1,
            ']' | '}' if !in_str => {
                depth -= 1;
                if depth == 0 {
                    return Ok(&src[open + 1..open + k]);
                }
            }
            _ => {}
        }
    }
    Err(ChemNetError::Parse(format!("unterminated `{name}` block")))
}

/// Top-level `{...}` / `[...]` groups of an array body (open char given).
fn groups(body: &str, open: char, close: char) -> Result<Vec<&str>, ChemNetError> {
    let mut out = Vec::new();
    let mut depth = 0i32;
    let mut start = 0usize;
    let mut in_str = false;
    for (k, ch) in body.char_indices() {
        if ch == '"' {
            in_str = !in_str;
            continue;
        }
        if in_str {
            continue;
        }
        if ch == open {
            if depth == 0 {
                start = k + 1;
            }
            depth += 1;
        } else if ch == close {
            depth -= 1;
            if depth == 0 {
                out.push(&body[start..k]);
            }
        } else if depth == 0 && !(ch.is_whitespace() || ch == ',') {
            return Err(ChemNetError::Parse(format!(
                "unexpected `{ch}` between rows"
            )));
        }
    }
    Ok(out)
}

fn split_top(s: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let (mut depth, mut start, mut in_str) = (0i32, 0usize, false);
    for (k, ch) in s.char_indices() {
        match ch {
            '"' => in_str = !in_str,
            '[' if !in_str => depth += 1,
            ']' if !in_str => depth -= 1,
            ',' if !in_str && depth == 0 => {
                out.push(s[start..k].trim());
                start = k + 1;
            }
            _ => {}
        }
    }
    let last = s[start..].trim();
    if !last.is_empty() {
        out.push(last);
    }
    out
}

fn int(s: &str) -> Result<i64, ChemNetError> {
    s.trim()
        .parse::<i64>()
        .map_err(|_| ChemNetError::Parse(format!("bad integer `{s}`")))
}

impl NetTable {
    /// Parses the text of `chem_network.gen.js` (or a trimmed copy in the
    /// same format): the `SPECIES = [...]` and `REACTIONS = [...]` literals.
    pub fn parse_js(src: &str) -> Result<Self, ChemNetError> {
        let src = strip_comments(src);
        let mut species = Vec::new();
        for (k, obj) in groups(array_body(&src, "SPECIES")?, '{', '}')?
            .into_iter()
            .enumerate()
        {
            let mut fields: BTreeMap<&str, &str> = BTreeMap::new();
            for kv in split_top(obj) {
                let (key, val) = kv
                    .split_once(':')
                    .ok_or_else(|| ChemNetError::Parse(format!("species {k}: bad field `{kv}`")))?;
                fields.insert(key.trim(), val.trim());
            }
            let get = |key: &str| {
                fields
                    .get(key)
                    .copied()
                    .ok_or_else(|| ChemNetError::Parse(format!("species {k}: no `{key}`")))
            };
            if int(get("i")?)? != k as i64 {
                return Err(ChemNetError::Parse(format!(
                    "species {k}: index out of order"
                )));
            }
            let unq = |v: &str| v.trim_matches('"').to_string();
            let cs = get("c")?.trim_start_matches('[').trim_end_matches(']');
            let lanes: Vec<i64> = cs.split(',').map(int).collect::<Result<_, _>>()?;
            let c: [i64; 6] = lanes
                .try_into()
                .map_err(|_| ChemNetError::Parse(format!("species {k}: c is not 6 lanes")))?;
            let p = unq(get("p")?)
                .chars()
                .next()
                .ok_or_else(|| ChemNetError::Parse(format!("species {k}: empty phase")))?;
            let u = i32::try_from(int(get("u")?)?)
                .map_err(|_| ChemNetError::Parse(format!("species {k}: u out of range")))?;
            species.push(NetSpecies {
                f: unq(get("f")?),
                c,
                p,
                u,
            });
        }
        let mut reactions = Vec::new();
        for (k, row) in groups(array_body(&src, "REACTIONS")?, '[', ']')?
            .into_iter()
            .enumerate()
        {
            let v: Vec<i64> = row.split(',').map(int).collect::<Result<_, _>>()?;
            if v.len() != 10 {
                return Err(ChemNetError::Parse(format!(
                    "reaction {k}: {} fields",
                    v.len()
                )));
            }
            let ix = |x: i64| {
                usize::try_from(x).map_err(|_| ChemNetError::BadSpecies {
                    row: k,
                    index: usize::MAX,
                })
            };
            reactions.push(NetReaction {
                a: ix(v[0])?,
                sa: v[1],
                b: ix(v[2])?,
                sb: v[3],
                pc: ix(v[4])?,
                sc: v[5],
                pd: ix(v[6])?,
                sd: v[7],
                de: v[8],
                flags: v[9],
            });
        }
        Ok(Self { species, reactions })
    }
}

/// planet6 `NET_ROW`: the first table row `C + O -> ... CO2 ...`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct NetRow {
    pub de: i64,
    pub sa: i64,
    pub sb: i64,
}

/// One mounted row in CSR order (planet6's 3 packed words, unpacked).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PackedRow {
    pub sa: u8,
    pub sb: u8,
    pub p0: MatId,
    pub n0: u8,
    pub p1: MatId,
    pub n1: u8,
    /// `bindDelta`: product BIND minus reactant BIND, on OUR scale.
    pub bd: i32,
}

/// planet6 `NMAT_CSR`.
pub const NMAT_CSR: usize = 128;
/// planet6's mount perf cap (`MOUNTED.slice(0, 24661)`).
pub const MOUNT_CAP: usize = 24661;
const H2O_KEY: [i64; 6] = [2, 1, 0, 0, 0, 0];
/// Ids the hand table maps by composition (planet6 `MAT_BY_COMP`).
const HAND_MOUNT: [u8; 35] = [
    7, 10, 8, 11, 9, 1, 4, 13, 14, 15, 17, 18, 19, 20, 21, 22, 23, 24, 25, 26, 27, 28, 29, 16, 30,
    31, 32, 33, 34, 35, 36, 37, 38, 39, 40,
];
const DEAD_OK: [MatId; 2] = [MatId::GYP, MatId::O2];
const LEGACY_ENERGY: [u8; 12] = [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12];

/// A mounted network: the extended material table, the row CSR, `NET_ROW`
/// and the gas set. Build once with [`ChemNet::mount`], share with `Arc`.
#[derive(Clone, Debug)]
pub struct ChemNet {
    table: Arc<MatTable>,
    pair_off: Vec<u32>,
    rows: Vec<PackedRow>,
    net_row: Option<NetRow>,
    rising: Vec<bool>,
    minted: usize,
}

fn key6(c: &[i64]) -> [i64; 6] {
    std::array::from_fn(|k| c[k])
}

impl ChemNet {
    /// Parses and mounts generated-table text.
    pub fn from_js(src: &str) -> Result<Self, ChemNetError> {
        Self::mount(&NetTable::parse_js(src)?)
    }

    /// planet6's module-level mount, step for step.
    pub fn mount(t: &NetTable) -> Result<Self, ChemNetError> {
        let mut mat = MatTable::base();
        // MAT_BY_COMP, insertion-ordered (hand keys, then minted).
        let mut by_comp: Vec<([i64; 6], MatId)> = HAND_MOUNT
            .iter()
            .map(|&id| (key6(&mat.comp(MatId(id)).expect("hand row")), MatId(id)))
            .collect();
        let find = |m: &Vec<([i64; 6], MatId)>, k: &[i64; 6]| {
            m.iter().find(|(kk, _)| kk == k).map(|(_, id)| *id)
        };
        let is_atom = |sp: &NetSpecies| sp.c.iter().sum::<i64>() == 1 && sp.p == 'a';
        let atom_keys: Vec<[i64; 6]> = t
            .species
            .iter()
            .filter(|s| is_atom(s))
            .map(|s| s.c)
            .collect();

        // Batch 4: mint every remaining species.
        let mut next: u8 = 41;
        let mut minted = 0;
        for sp in &t.species {
            if sp.c == H2O_KEY || atom_keys.contains(&sp.c) || find(&by_comp, &sp.c).is_some() {
                continue;
            }
            if next >= 100 {
                return Err(ChemNetError::TooManyMinted);
            }
            let mut comp = [0i64; NE];
            comp[..6].copy_from_slice(&sp.c);
            mat.set(
                MatId(next),
                MatRow {
                    comp,
                    bind: sp.u,
                    name: sp.f.to_lowercase(),
                },
            );
            by_comp.push((sp.c, MatId(next)));
            next += 1;
            minted += 1;
        }

        let sp_id = |sp: &NetSpecies| -> Option<MatId> {
            if sp.c == H2O_KEY {
                return Some(MatId::STEAM);
            }
            if atom_keys.contains(&sp.c) {
                let lane = sp.c.iter().position(|&v| v == 1)?;
                return Some(MatId::element(lane));
            }
            find(&by_comp, &sp.c)
        };
        let species = |row: usize, k: usize| {
            t.species
                .get(k)
                .ok_or(ChemNetError::BadSpecies { row, index: k })
        };

        // MOUNTED
        struct Mounted {
            ra: MatId,
            sa: i64,
            rb: MatId,
            sb: i64,
            pc: MatId,
            sc: i64,
            pd: MatId,
            sd: i64,
            bd: i64,
        }
        let mut mounted: Vec<Mounted> = Vec::new();
        for (r, rx) in t.reactions.iter().enumerate() {
            let (a, b) = (species(r, rx.a)?, species(r, rx.b)?);
            let (pc, pd) = (species(r, rx.pc)?, species(r, rx.pd)?);
            let (Some(ra), Some(rb), Some(pci), Some(pdi)) =
                (sp_id(a), sp_id(b), sp_id(pc), sp_id(pd))
            else {
                continue;
            };
            if rx.sa > 2 || rx.sb > 2 || rx.sc > 2 || rx.sd > 2 {
                continue;
            }
            if rx.sc < 1 || rx.sd < 1 {
                continue;
            }
            if rx.sc + rx.sd > rx.sa + rx.sb {
                continue;
            }
            let bb = |m: MatId| i64::from(mat.bind(m));
            let bd = (bb(pci) * rx.sc + bb(pdi) * rx.sd) - (bb(ra) * rx.sa + bb(rb) * rx.sb);
            mounted.push(Mounted {
                ra,
                sa: rx.sa,
                rb,
                sb: rx.sb,
                pc: pci,
                sc: rx.sc,
                pd: pdi,
                sd: rx.sd,
                bd,
            });
        }
        mounted.truncate(MOUNT_CAP);

        // THE MOUNT AUDIT (byComp: last species with a key wins, like JS).
        let mut sp_by_comp: BTreeMap<[i64; 6], &NetSpecies> = BTreeMap::new();
        for sp in &t.species {
            sp_by_comp.insert(sp.c, sp);
        }
        let mut errs = Vec::new();
        for (k, id) in &by_comp {
            let nm = mat.name(*id).unwrap_or("?").to_string();
            let Some(sp) = sp_by_comp.get(k) else {
                if !DEAD_OK.contains(id) {
                    errs.push(format!(
                        "mount key {k:?} (material {nm}) matches NO species"
                    ));
                }
                continue;
            };
            let c = mat.comp(*id).expect("mounted id has a row");
            for (lane, (&got, &want)) in c[..6].iter().zip(&sp.c[..6]).enumerate() {
                if got != want {
                    errs.push(format!(
                        "material {nm} comp lane {lane} says {}, species {} says {}",
                        got, sp.f, want
                    ));
                }
            }
            if !LEGACY_ENERGY.contains(&id.0) && mat.bind(*id) != sp.u {
                errs.push(format!(
                    "material {nm} BIND {} != table U {}",
                    mat.bind(*id),
                    sp.u
                ));
            }
        }
        if !errs.is_empty() {
            return Err(ChemNetError::Audit(errs));
        }

        // ROW CSR
        let mut pair_off = vec![0u32; NMAT_CSR * NMAT_CSR + 1];
        let pk = |row: &Mounted| -> Result<usize, ChemNetError> {
            let (a, b) = (row.ra.0 as usize, row.rb.0 as usize);
            if a >= NMAT_CSR || b >= NMAT_CSR {
                return Err(ChemNetError::Parse(format!(
                    "mounted id {a}/{b} >= {NMAT_CSR}"
                )));
            }
            Ok(a * NMAT_CSR + b)
        };
        for row in &mounted {
            pair_off[pk(row)? + 1] += 1;
        }
        for k in 0..NMAT_CSR * NMAT_CSR {
            pair_off[k + 1] += pair_off[k];
        }
        let blank = PackedRow {
            sa: 0,
            sb: 0,
            p0: MatId::AIR,
            n0: 0,
            p1: MatId::AIR,
            n1: 0,
            bd: 0,
        };
        let mut rows = vec![blank; mounted.len()];
        let mut fill = vec![0u32; NMAT_CSR * NMAT_CSR];
        for row in &mounted {
            let p = pk(row)?;
            let w = (pair_off[p] + fill[p]) as usize;
            fill[p] += 1;
            if fill[p] > 64 {
                return Err(ChemNetError::PairOverflow {
                    ra: row.ra,
                    rb: row.rb,
                });
            }
            rows[w] = PackedRow {
                sa: row.sa as u8,
                sb: row.sb as u8,
                p0: row.pc,
                n0: row.sc as u8,
                p1: row.pd,
                n1: row.sd as u8,
                bd: row.bd as i32,
            };
        }

        // NET_ROW (species indices, last-wins byComp, raw table rows).
        let mut idx_by_comp: BTreeMap<[i64; 6], usize> = BTreeMap::new();
        for (k, sp) in t.species.iter().enumerate() {
            idx_by_comp.insert(sp.c, k);
        }
        let g = |c: [i64; 6]| idx_by_comp.get(&c).copied();
        let net_row = match (
            g([0, 0, 1, 0, 0, 0]),
            g([0, 1, 0, 0, 0, 0]),
            g([0, 2, 1, 0, 0, 0]),
        ) {
            (Some(c), Some(o), Some(co2)) => t
                .reactions
                .iter()
                .find(|rx| {
                    (rx.a == c || rx.b == c)
                        && (rx.a == o || rx.b == o)
                        && (rx.pc == co2 || rx.pd == co2)
                })
                .map(|rx| NetRow {
                    de: rx.de,
                    sa: rx.sa,
                    sb: rx.sb,
                }),
            _ => None,
        };

        // RISING
        let mut rising = vec![false; super::MAT_SLOTS];
        rising[MatId::STEAM.0 as usize] = true;
        rising[MatId::O2.0 as usize] = true;
        for (k, id) in &by_comp {
            if sp_by_comp.get(k).is_some_and(|sp| sp.p == 'g') {
                rising[id.0 as usize] = true;
            }
        }

        Ok(Self {
            table: Arc::new(mat),
            pair_off,
            rows,
            net_row,
            rising,
            minted,
        })
    }

    pub fn table(&self) -> &MatTable {
        &self.table
    }

    pub(crate) fn table_arc(&self) -> Arc<MatTable> {
        self.table.clone()
    }

    pub fn mounted_rows(&self) -> usize {
        self.rows.len()
    }

    pub fn minted(&self) -> usize {
        self.minted
    }

    pub fn net_row(&self) -> Option<NetRow> {
        self.net_row
    }

    pub fn is_rising(&self, m: MatId) -> bool {
        self.rising[m.0 as usize]
    }

    /// Ids in the rising set, ascending.
    pub fn rising_ids(&self) -> Vec<u8> {
        (0..=255u8).filter(|m| self.rising[*m as usize]).collect()
    }

    /// The CSR range of mounted rows for reactant pair `(a, b)` (empty for
    /// ids >= 128).
    pub fn pair_range(&self, a: MatId, b: MatId) -> (usize, usize) {
        let (a, b) = (a.0 as usize, b.0 as usize);
        if a >= NMAT_CSR || b >= NMAT_CSR {
            return (0, 0);
        }
        let k = a * NMAT_CSR + b;
        (self.pair_off[k] as usize, self.pair_off[k + 1] as usize)
    }

    pub fn row(&self, r: usize) -> &PackedRow {
        &self.rows[r]
    }

    /// True when `a` is the reactant `a` of at least one mounted row (any
    /// partner). A cell whose material is not reactive can never post an
    /// intent in [`ChemWorld::network_pass`] (§5y exact skip).
    pub fn is_reactive(&self, a: MatId) -> bool {
        let a = a.0 as usize;
        a < NMAT_CSR && self.pair_off[a * NMAT_CSR] != self.pair_off[(a + 1) * NMAT_CSR]
    }
}

const FRESH: i32 = -1;
const UNCLAIMED: u32 = u32::MAX;

#[derive(Clone, Copy)]
struct Intent {
    row: u32,
    cursor: i32,
    n: u8,
    slots: [u32; 4],
}

impl ChemWorld {
    /// The generic network law v5: snapshot intents, lowest-index-owner
    /// claims, commit, repeat to a fixpoint (at most [`FIRE_MAX_ROUNDS`]).
    /// Returns (firings, rounds run).
    pub(crate) fn network_pass(
        &mut self,
        topo: &Topology,
        net: &ChemNet,
    ) -> Result<(u32, u32), StepError> {
        let n = self.cells.len();
        let mut prov = vec![0u8; n];
        let mut done = vec![false; n];
        let mut cursor = vec![FRESH; n];
        let mut vis_id = vec![MatId::AIR; n];
        let mut fids: Vec<[MatId; 26]> = vec![[MatId::AIR; 26]; n];
        let mut nnbs = vec![0u8; n];
        let mut intent: Vec<Option<Intent>> = vec![None; n];
        let mut claim = vec![UNCLAIMED; n];
        let mut cube = Vec::with_capacity(26);
        let reactive: [bool; 256] = std::array::from_fn(|m| net.is_reactive(MatId(m as u8)));
        let tick = self.tick;
        let mut fired_total = 0u32;
        let mut rounds = 0u32;

        for rnd in 0..FIRE_MAX_ROUNDS {
            rounds = rnd + 1;
            // pass A - INTENT
            let mut any = false;
            for i in 0..n {
                claim[i] = UNCLAIMED;
                intent[i] = None;
                let mut start = rnd == 0;
                if !start {
                    let p = prov[i];
                    if done[i] || p == 2 || p == 4 {
                        continue;
                    }
                    if p == 3 && self.mat[i] != vis_id[i] {
                        start = true;
                    }
                }
                let qi = self.q[i];
                if !start && qi < 250 {
                    continue; // nothing to refresh and every row would be skipped below
                }
                if qi < 250 || !reactive[self.mat[i].0 as usize] {
                    // §5y exact skip: this cell cannot post an intent this
                    // round (cold, or its material reacts in no mounted row),
                    // so its neighbour-id cache is never read for an intent.
                    // Its q only moves when it fires itself and its material
                    // only moves as a claimed slot (prov 3/4), which forces a
                    // fresh `start` later, so keeping just `vis_id` is
                    // bit-identical to building the cube here.
                    if start {
                        nnbs[i] = 0;
                        vis_id[i] = self.mat[i];
                        cursor[i] = FRESH;
                    }
                    continue;
                }
                topo.cube(&self.cells, i, 1, &mut cube);
                if start {
                    // Distinct neighbour ids in first-seen cube order (a 256-bit seen set
                    // instead of a scan of the ids so far; same list).
                    let mut seen = [0u64; 4];
                    let mut k = 0usize;
                    for &j in &cube {
                        let m = self.mat[j];
                        let (word, bit) = ((m.0 >> 6) as usize, 1u64 << (m.0 & 63));
                        if seen[word] & bit == 0 {
                            seen[word] |= bit;
                            fids[i][k] = m;
                            k += 1;
                        }
                    }
                    nnbs[i] = k as u8;
                    vis_id[i] = self.mat[i];
                    cursor[i] = FRESH;
                }
                let raid = self.mat[i];
                if qi < 250 {
                    continue; // planet6 walks the rows but every row is skipped
                }
                let c1 = (cursor[i] + 1) as u32;
                let mut b_idx = (c1 >> 6) as usize;
                let mut r_idx = (c1 & 63) as usize;
                'walk: while b_idx < nnbs[i] as usize {
                    let bm = fids[i][b_idx];
                    let (r0, r1) = net.pair_range(raid, bm);
                    let mut r = r0 + r_idx;
                    while r < r1 {
                        let row = net.row(r);
                        let (sa, sb) = (row.sa as usize, row.sb as usize);
                        let mut slots = [0u32; 4];
                        slots[0] = i as u32;
                        let (mut a_pick, mut b_pick) = (1usize, 0usize);
                        for &j in &cube {
                            let m = self.mat[j];
                            if m == bm {
                                if b_pick < sb {
                                    slots[sa + b_pick] = j as u32;
                                    b_pick += 1;
                                }
                            } else if m == raid && a_pick < sa && b_pick == 0 {
                                slots[a_pick] = j as u32;
                                a_pick += 1;
                            }
                        }
                        if a_pick >= sa
                            && b_pick >= sb
                            && !(row.bd < 0 && i64::from(qi) < -i64::from(row.bd))
                        {
                            let salt =
                                0x9e3u32.wrapping_add(u32::from(row.p0.0).wrapping_mul(7919));
                            if self.roll_at(topo, salt, i, tick) < 60 {
                                intent[i] = Some(Intent {
                                    row: r as u32,
                                    cursor: (b_idx * 64 + (r - r0)) as i32,
                                    n: (sa + sb) as u8,
                                    slots,
                                });
                                any = true;
                                break 'walk; // first intent wins
                            }
                        }
                        r += 1;
                    }
                    b_idx += 1;
                    r_idx = 0;
                }
            }
            if !any {
                break;
            }
            // pass B - CLAIM
            for (i, it) in intent.iter().enumerate() {
                if let Some(it) = it {
                    for &s in &it.slots[..it.n as usize] {
                        let s = s as usize;
                        claim[s] = claim[s].min(i as u32 + 1);
                    }
                }
            }
            // pass C - COMMIT
            let mut fired = 0u32;
            for i in 0..n {
                let Some(it) = intent[i] else { continue };
                let k = it.n as usize;
                if it.slots[..k]
                    .iter()
                    .any(|&s| claim[s as usize] != i as u32 + 1)
                {
                    continue;
                }
                let raid = self.mat[i];
                let row = *net.row(it.row as usize);
                let mut si = 0usize;
                for _ in 0..row.n0 {
                    if si < k {
                        self.set_mat(it.slots[si] as usize, row.p0);
                        si += 1;
                    }
                }
                for _ in 0..row.n1 {
                    if si < k {
                        self.set_mat(it.slots[si] as usize, row.p1);
                        si += 1;
                    }
                }
                while si < k {
                    self.set_mat(it.slots[si] as usize, MatId::AIR);
                    si += 1;
                }
                self.q[i] = self.q[i].wrapping_add(row.bd);
                let selfprod = row.p0 == raid;
                prov[i] = if selfprod { 1 } else { 2 };
                done[i] = !selfprod;
                cursor[i] = it.cursor;
                for &s in &it.slots[..k] {
                    let s = s as usize;
                    if s != i {
                        prov[s] = if i < s { 3 } else { 4 };
                    }
                }
                fired += 1;
            }
            fired_total += fired;
            // pass D - DECIDE
            if fired == 0 {
                break;
            }
            if rnd + 1 >= FIRE_MAX_ROUNDS {
                return Err(StepError::FireProtocolStuck { tick: self.tick });
            }
        }
        Ok((fired_total, rounds))
    }
}
