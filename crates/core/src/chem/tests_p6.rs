//! planet6 tick parity and conservation tests. Golden values come from the
//! real `games/planet/core.js` run by
//! `gameladder_chem_findings/p6golden/golden.js` on the same integer fixture.

use std::sync::Arc;

use super::*;

const TRIMMED: &str = include_str!("fixtures/chemnet_trimmed.js.txt");

// planet6 element ids (100 + Z).
const H: u8 = 101;
const O: u8 = 108;
const C: u8 = 106;
const N: u8 = 107;
const S: u8 = 116;
const CA: u8 = 120;
const FE: u8 = 126;
const NI: u8 = 128;
const SI: u8 = 114;
const MG: u8 = 112;

/// The golden fixture: planet6's 48^3 sphere mask, banded by squared
/// distance with two integer perturbations (mirror of golden.js).
fn fixture_cells() -> Vec<(Coord, MatId, i32)> {
    const BANDS: [u8; 17] = [FE, NI, SI, MG, O, C, S, CA, O, H, N, H, O, C, 2, 12, 2];
    let mut out = Vec::new();
    for x in 0..48i64 {
        for y in 0..48i64 {
            for z in 0..48i64 {
                let s = (2 * x - 47).pow(2) + (2 * y - 47).pow(2) + (2 * z - 47).pow(2);
                if s > 1764 {
                    continue;
                }
                let mut b = 17 * s / 1765;
                if (x * 7 + y * 13 + z * 29) % 11 == 0 && b + 1 < 17 {
                    b += 1;
                }
                if (x * 5 + y * 3 + z * 17) % 13 == 0 && b > 0 {
                    b -= 1;
                }
                let m = MatId(BANDS[b as usize]);
                let q = match m {
                    MatId::WATER => 40,
                    MatId::STEAM => 100,
                    _ => (3000 - 1800 * s / 1764).max(300) as i32,
                };
                out.push((Coord::new(x, y, z), m, q));
            }
        }
    }
    out
}

const FIX_CELLS: usize = 39024;
const FIX_FP0: u32 = 654_854_396;
const FIX_E0: i64 = 61_561_412;
const FIX_LANES: Lanes = [
    38503, 23912, 5173, 2726, 2004, 2374, 570, 1007, 1292, 1543, 0, 0,
];

fn fixture(params: P6Params, net: Option<Arc<ChemNet>>) -> ChemWorld {
    let mut w = ChemWorld::from_cells(fixture_cells(), [0; NE], Arc::new(MatTable::base()), params)
        .expect("fixture books");
    if let Some(n) = net {
        w.attach_net(n);
    }
    assert_eq!(w.domain_len(), FIX_CELLS);
    assert_eq!(w.fingerprint(), FIX_FP0);
    assert_eq!(w.energy_total(), FIX_E0);
    assert_eq!(w.lane_totals(), FIX_LANES);
    w
}

fn trimmed() -> Arc<ChemNet> {
    Arc::new(ChemNet::from_js(TRIMMED).expect("trimmed table mounts"))
}

fn golden_params(mode: char) -> P6Params {
    // goldens A-E were generated with planet6's five surface laws off
    let mut p = P6Params::planet6_defaults().without_surface_laws();
    p.seed = 7;
    match mode {
        'A' => p.elements = false,
        'B' => {
            p.elements = true;
            p.net_chem = false;
            p.net_burn = false;
        }
        'C' | 'E' => p.elements = true,
        'D' => {
            p.elements = true;
            p.combine = false;
        }
        _ => unreachable!(),
    }
    p
}

/// Runs `fps.len()` ticks, asserting fingerprint, space heat and both books
/// every tick.
fn run_golden(w: &mut ChemWorld, fps: &[u32], space: &[i64]) {
    for (t, (&fp, &sq)) in fps.iter().zip(space).enumerate() {
        w.step().expect("tick");
        assert_eq!(w.fingerprint(), fp, "fingerprint at tick {}", t + 1);
        assert_eq!(w.space_q(), sq, "space_q at tick {}", t + 1);
        assert_eq!(w.energy_drift(), 0, "energy drift at tick {}", t + 1);
        assert_eq!(w.lane_drift(), [0; NE], "lane drift at tick {}", t + 1);
        assert_eq!(w.energy_total(), FIX_E0);
    }
    assert_eq!(w.check_books(), Ok(()));
}

fn census(w: &ChemWorld) -> Vec<(String, usize)> {
    let mut m: std::collections::BTreeMap<String, usize> = Default::default();
    for (_, mat, _) in w.domain() {
        *m.entry(w.table().name(mat).unwrap_or("?").to_string())
            .or_default() += 1;
    }
    m.into_iter().collect()
}

fn assert_census(w: &ChemWorld, want: &[(&str, usize)]) {
    let mut want: Vec<(String, usize)> = want.iter().map(|(n, c)| (n.to_string(), *c)).collect();
    want.sort();
    assert_eq!(census(w), want);
}

const A_FP: [u32; 30] = [
    2075053831, 4102877360, 3649859746, 4066429016, 382289700, 1276990830, 150064004, 2580571197,
    3337747358, 1771814248, 1615716822, 132205649, 3519070990, 2594713612, 91487938, 914082690,
    1527003856, 1953951038, 2737497404, 2162461468, 3209048228, 338391212, 1607389343, 1618356115,
    1936690452, 891201207, 905056270, 786255970, 3709993258, 947856154,
];
const A_SPACE: [i64; 30] = [
    13392, 26784, 40176, 53568, 66960, 80352, 93744, 107136, 120528, 133920, 147312, 160704,
    174096, 187488, 200880, 214272, 227664, 241056, 254359, 267343, 280215, 292837, 305073, 316996,
    328775, 340500, 352213, 363896, 375499, 387083,
];
const B_FP: [u32; 30] = [
    240651297, 1366466702, 2988771184, 2665863763, 1233189339, 2308794600, 1213220749, 2239469728,
    3679313670, 887294751, 3002961086, 2434314600, 2013283342, 2666676655, 3867656300, 1395748037,
    3251089012, 2050161741, 1704262038, 2301246682, 305456778, 1811647933, 4085787331, 3185911342,
    2411331782, 4061569106, 432177452, 2834744544, 78438230, 2523850608,
];
const D_FP: [u32; 30] = [
    2783017137, 1115252361, 3264704760, 1222937967, 3555690022, 3483251234, 1991992097, 1742405787,
    925235730, 1206964411, 4069798879, 3392792094, 3025782677, 543877298, 1236703546, 1763146549,
    2504946410, 2181776752, 3332137703, 3156894925, 586215645, 1212957668, 2973552470, 4142172757,
    2494280779, 674493291, 3772525574, 2503907251, 4003133287, 2466155565,
];
const E_FP: [u32; 30] = [
    2154499117, 3416828478, 2847814783, 2962414478, 2965804108, 1389815451, 1312127686, 2095156684,
    4133736200, 4215066311, 3931797110, 3156166426, 56467350, 1824246498, 1341833122, 3257148950,
    1851098206, 1956098286, 2241296847, 2212213291, 2804937632, 2122637149, 1361638226, 3606855269,
    1640800674, 3309494266, 109170037, 999139384, 1404245348, 2273446158,
];
const C_FP: [u32; 30] = [
    561731789, 3329650995, 2964654485, 1264276070, 770399472, 2401731897, 547669363, 1807945261,
    3990530951, 2363156150, 2890359814, 2275952561, 3465404994, 979373629, 2815754308, 1834306321,
    2436332517, 2266110864, 98924304, 861118555, 2542301734, 3438887663, 1590515273, 2284712485,
    526770498, 2441054457, 1115023483, 3565266066, 3191233008, 525903195,
];

#[test]
fn p6_lattice_matches_planet6() {
    // spot values from core.js `lattice` (node)
    assert_eq!(lattice(7, 1234, 1), 2_382_026_660);
    assert_eq!(lattice(0xdead_beef, 65535, 65535), 682_969_581);
    assert_eq!(lattice(7 ^ 0xb0b81e, 39000, 30), 3_874_876_776);
    assert_eq!(lattice(7, 0, 0), lattice(7, 0x1_0000, 0x1_0000));
    assert_ne!(lattice(7, 1, 1), lattice(7, 1, 2));
}

#[test]
fn p6_golden_a_mineral_world_heat_only_laws() {
    let mut w = fixture(golden_params('A'), None);
    run_golden(&mut w, &A_FP, &A_SPACE);
}

#[test]
fn p6_golden_b_hand_families_no_net() {
    let mut space = [66960i64; 30];
    for (t, s) in space.iter_mut().enumerate().take(12) {
        *s = 13392 + 4464 * t as i64;
    }
    let mut w = fixture(golden_params('B'), None);
    run_golden(&mut w, &B_FP, &space);
}

#[test]
fn p6_golden_d_trimmed_net_law_only() {
    let mut w = fixture(golden_params('D'), Some(trimmed()));
    run_golden(&mut w, &D_FP, &[13392; 30]);
    assert_census(
        &w,
        &[
            ("air", 9151),
            ("water", 7991),
            ("co2", 2433),
            ("quicklime", 1288),
            ("steam", 703),
            ("h2s", 325),
            ("so", 289),
            ("cas", 761),
            ("methane", 4),
            ("C", 2736),
            ("H", 4467),
            ("O", 784),
            ("N", 2726),
            ("S", 629),
            ("Ca", 325),
            ("Mg", 1543),
            ("Si", 1292),
            ("Ni", 1007),
            ("Fe", 570),
        ],
    );
}

#[test]
fn p6_golden_e_trimmed_net_all_laws() {
    let mut w = fixture(golden_params('E'), Some(trimmed()));
    run_golden(&mut w, &E_FP, &[13392; 30]);
    assert_census(
        &w,
        &[
            ("air", 13690),
            ("rock", 350),
            ("water", 8920),
            ("co2", 1079),
            ("nh3", 404),
            ("limestone", 283),
            ("o2", 334),
            ("quicklime", 20),
            ("steam", 701),
            ("so", 6),
            ("cas", 1156),
            ("C", 3811),
            ("O", 270),
            ("H", 209),
            ("N", 2322),
            ("Ca", 565),
            ("S", 492),
            ("Mg", 1543),
            ("Si", 1292),
            ("Ni", 1007),
            ("Fe", 570),
        ],
    );
}

/// Full generated table (not vendored): set `BITBLOCK_CHEMNET_JS` or keep
/// the file at the findings path. `cargo test -p bitblock-core -- --ignored`.
#[test]
#[ignore = "needs the 676 KB chem_network.gen.js outside the repo"]
fn p6_golden_c_full_table() {
    let path = std::env::var("BITBLOCK_CHEMNET_JS")
        .unwrap_or_else(|_| "/workspace/gameladder_chem_findings/chem_network.gen.js".into());
    let src = std::fs::read_to_string(&path).expect("full table text");
    let net = ChemNet::from_js(&src).expect("full table mounts");
    assert_eq!(net.mounted_rows(), 21577);
    assert_eq!(net.minted(), 50);
    assert_eq!(
        net.net_row(),
        Some(NetRow {
            de: -28,
            sa: 2,
            sb: 2
        })
    );
    assert_eq!(
        net.rising_ids(),
        vec![
            7, 8, 10, 12, 13, 14, 16, 18, 19, 20, 21, 28, 29, 30, 31, 32, 33, 34, 35, 36, 37, 38,
            39, 41, 44, 46, 47, 49, 51, 53, 54, 56, 59, 60, 62, 64, 65, 67, 69, 70, 72, 74, 75, 77,
            78, 80, 81, 84, 86, 88, 89, 90
        ]
    );
    let mut w = fixture(golden_params('C'), Some(Arc::new(net)));
    run_golden(&mut w, &C_FP, &[13392; 30]);
}

#[test]
fn p6_trimmed_loader_mounts_like_planet6() {
    let t = NetTable::parse_js(TRIMMED).unwrap();
    assert!(!t.species.is_empty() && !t.reactions.is_empty());
    let net = ChemNet::mount(&t).unwrap();
    assert_eq!(net.mounted_rows(), 11);
    assert_eq!(net.minted(), 0);
    assert_eq!(
        net.net_row(),
        Some(NetRow {
            de: -28,
            sa: 2,
            sb: 2
        })
    );
    assert_eq!(
        net.rising_ids(),
        vec![
            7, 8, 10, 12, 13, 14, 16, 18, 19, 20, 21, 28, 29, 30, 31, 32, 33, 34, 35, 36, 37, 38,
            39
        ]
    );
    // row 0 of the table: Ca + 2 S -> S + CaS, bindDelta on our scale +12
    let (r0, r1) = net.pair_range(MatId(CA), MatId(S));
    assert!(r1 > r0);
    let row = net.row(r0);
    assert_eq!(
        (row.sa, row.sb, row.p0, row.n0, row.p1, row.n1, row.bd),
        (1, 2, MatId(S), 1, MatId::CAS, 1, 12)
    );
    assert_eq!(net.pair_range(MatId(NI), MatId(S)), (0, 0));
}

#[test]
fn p6_loader_rejects_garbage() {
    assert!(matches!(
        NetTable::parse_js("nothing here"),
        Err(ChemNetError::Parse(_))
    ));
    let bad_row = "const SPECIES = [ {i:0,f:\"H\",c:[1,0,0,0,0,0],p:\"a\",u:0,d:0} ];\nconst REACTIONS = [ [0,1,0] ];";
    assert!(matches!(
        NetTable::parse_js(bad_row),
        Err(ChemNetError::Parse(_))
    ));
    let bad_ix = "const SPECIES = [ {i:0,f:\"H\",c:[1,0,0,0,0,0],p:\"a\",u:0,d:0} ];\nconst REACTIONS = [ [0,1,5,1,0,1,0,1,0,0] ];";
    assert!(matches!(
        ChemNet::from_js(bad_ix),
        Err(ChemNetError::BadSpecies { .. })
    ));
    // the hand keys need species: an otherwise valid table without them fails the audit
    let no_keys = "// c\nconst SPECIES = [ {i:0,f:\"H\",c:[1,0,0,0,0,0],p:\"a\",u:0,d:0} ];\nconst REACTIONS = [ ];";
    assert!(matches!(
        ChemNet::from_js(no_keys),
        Err(ChemNetError::Audit(_))
    ));
}

fn chem_only() -> P6Params {
    P6Params {
        elements: true,
        combine: false,
        net_chem: false,
        net_burn: false,
        condense: false,
        gas_rise: false,
        heat: false,
        phase: false,
        ..P6Params::planet6_defaults().without_surface_laws()
    }
}

fn run_until(w: &mut ChemWorld, max: u32, mut f: impl FnMut(&StepReport) -> bool) -> u64 {
    for _ in 0..max {
        let r = w.step().unwrap();
        assert_eq!(w.check_books(), Ok(()));
        if f(&r) {
            return r.tick;
        }
    }
    panic!("never fired in {max} ticks");
}

#[test]
fn p6_network_row_fires_and_conserves() {
    // Ca between two S: table row 0, Ca + 2 S -> S + CaS
    let cells = vec![
        (Coord::new(10, 11, 11), MatId(S), 0),
        (Coord::new(11, 11, 11), MatId(CA), 300),
        (Coord::new(12, 11, 11), MatId(S), 0),
    ];
    let mut w = ChemWorld::from_cells(
        cells,
        [0; NE],
        Arc::new(MatTable::base()),
        P6Params {
            net_chem: true,
            ..chem_only()
        },
    )
    .unwrap();
    w.attach_net(trimmed());
    let e0 = w.energy_total();
    let t = run_until(&mut w, 500, |r| r.net_fired > 0);
    assert_eq!(w.material(Coord::new(11, 11, 11)), MatId(S));
    assert_eq!(w.material(Coord::new(10, 11, 11)), MatId::CAS);
    assert_eq!(w.material(Coord::new(12, 11, 11)), MatId::AIR);
    assert_eq!(w.heat(Coord::new(11, 11, 11)), Some(312));
    assert_eq!(w.energy_total(), e0);
    assert_eq!(w.energy_drift(), 0);
    assert_eq!(w.lane_drift(), [0; NE]);
    assert!(!w.world().is_on(Coord::new(12, 11, 11)));
    eprintln!("network Ca+2S fired at tick {t}");
}

#[test]
fn p6_hand_family_fires_and_conserves() {
    // N + 3 H -> NH3 (Q += 12)
    let mut cells = vec![(Coord::new(5, 5, 5), MatId(N), 260)];
    for (k, x) in [4, 6].into_iter().enumerate() {
        cells.push((Coord::new(x, 5, 5), MatId(H), k as i32));
    }
    cells.push((Coord::new(5, 6, 5), MatId(H), 0));
    let mut w = ChemWorld::from_cells(
        cells,
        [0; NE],
        Arc::new(MatTable::base()),
        P6Params {
            combine: true,
            ..chem_only()
        },
    )
    .unwrap();
    let e0 = w.energy_total();
    run_until(&mut w, 500, |r| r.hand[2] > 0);
    assert_eq!(w.material(Coord::new(5, 5, 5)), MatId::NH3);
    assert_eq!(w.heat(Coord::new(5, 5, 5)), Some(272));
    assert_eq!(w.material_count(), 1);
    assert_eq!(w.energy_total(), e0);
    assert_eq!(w.lane_drift(), [0; NE]);
}

fn small_budget() -> GenesisConfig {
    GenesisConfig {
        budget: GenesisBudget::new([4000, 3000, 400, 120, 300, 350, 200, 60, 180, 150, 40, 30]),
        seed: 7,
        shape: GenesisShape {
            center: Coord::new(0, 0, 0),
            radius: 10,
            hollow: None,
        },
        atom_cap: DEFAULT_ATOM_CAP,
    }
}

#[test]
fn p6_heat_only_genesis_has_zero_drift() {
    let mut w = genesis_p6(&small_budget(), &P6Params::heat_only()).unwrap();
    let e0 = w.energy_total();
    assert_eq!(w.t0_energy(), e0);
    let before: Vec<_> = w.domain().map(|(c, m, _)| (c, m)).collect();
    let mut leaked = 0;
    for _ in 0..50 {
        let r = w.step().unwrap();
        leaked += r.leaked;
        assert_eq!(w.energy_drift(), 0);
        assert_eq!(w.lane_drift(), [0; NE]);
    }
    assert!(w.space_q() > 0);
    assert_eq!(w.space_q(), leaked);
    let after: Vec<_> = w.domain().map(|(c, m, _)| (c, m)).collect();
    assert_eq!(before, after, "heat-only ticks never move matter");
    assert_eq!(w.check_books(), Ok(()));
    eprintln!("heat-only r10: 50 ticks leaked {leaked}, energy {e0}");
}

#[test]
fn p6_genesis_heat_is_radial() {
    let w = genesis_p6(&small_budget(), &P6Params::default()).unwrap();
    let core = w.heat(Coord::new(0, 0, 0)).unwrap();
    let rim = w.heat(Coord::new(10, 0, 0)).unwrap();
    assert!(core > rim, "core {core} rim {rim}");
    for (_, m, q) in w.domain() {
        if m == MatId::AIR {
            assert_eq!(q, 0);
        } else {
            assert!(q >= w.table().bind(m), "every material affords its own dig");
        }
    }
}

#[test]
fn p6_full_tick_on_genesis_is_deterministic_and_conserves() {
    let run = || {
        let mut w = genesis_p6(&small_budget(), &P6Params::default()).unwrap();
        w.attach_net(trimmed());
        let mut fps = Vec::new();
        for _ in 0..40 {
            w.step().unwrap();
            assert_eq!(w.energy_drift(), 0);
            assert_eq!(w.lane_drift(), [0; NE]);
            fps.push(w.fingerprint());
        }
        assert_eq!(w.check_books(), Ok(()));
        (fps, w.space_q())
    };
    assert_eq!(run(), run());
}

#[test]
fn p6_greenhouse_modes_conserve() {
    for gh in [Greenhouse::Off, Greenhouse::Planet6, Greenhouse::Rational] {
        let mut w = fixture(
            P6Params {
                greenhouse: gh,
                ..golden_params('E')
            },
            Some(trimmed()),
        );
        for _ in 0..5 {
            w.step().unwrap();
        }
        assert_eq!(w.energy_drift(), 0, "{gh:?}");
        assert_eq!(w.lane_drift(), [0; NE], "{gh:?}");
    }
}

#[test]
fn p6_dig_needs_heat_and_place_pays_it_back() {
    let mut w = ChemWorld::from_cells(
        vec![
            (Coord::new(1, 1, 1), MatId::ROCK, 10),
            (Coord::new(2, 1, 1), MatId::ROCK, 50),
        ],
        [0; NE],
        Arc::new(MatTable::base()),
        chem_only(),
    )
    .unwrap();
    assert_eq!(w.dig(Coord::new(1, 1, 1)), None, "Q 10 < BIND 36");
    assert_eq!(w.dig(Coord::new(2, 1, 1)), Some(MatId::ROCK));
    assert_eq!(w.heat(Coord::new(2, 1, 1)), Some(14));
    assert_eq!(w.energy_drift(), 0);
    assert!(w.place(Coord::new(2, 1, 1), MatId::ROCK));
    assert_eq!(w.heat(Coord::new(2, 1, 1)), Some(50));
    assert_eq!(w.check_books(), Ok(()));
}

// ---------------------------------------------------------------------------
// The five surface laws: water cycle, erode, dissolve, settle, flow.
// Goldens F/M/G come from `p6golden/golden.js` with every planet6 law on
// (compact text via `p6golden/to_fixture.js`).

const GOLDEN_F: &str = include_str!("fixtures/p6golden_F.txt");
const GOLDEN_M: &str = include_str!("fixtures/p6golden_M.txt");

/// One parsed golden: header, per-tick `(fp, space_q, vapor, evaporated,
/// rained)` and the final census.
struct Golden {
    cells: usize,
    fp0: u32,
    e0: i64,
    l0: Lanes,
    ticks: Vec<(u32, i64, i64, u64, u64)>,
    census: Vec<(String, usize)>,
}

fn parse_golden(src: &str) -> Golden {
    let mut g = Golden {
        cells: 0,
        fp0: 0,
        e0: 0,
        l0: [0; NE],
        ticks: Vec::new(),
        census: Vec::new(),
    };
    for line in src.lines() {
        let f: Vec<&str> = line.split_whitespace().collect();
        match f.first().copied() {
            Some("#") => {
                g.cells = f[2].parse().unwrap();
                g.fp0 = f[3].parse().unwrap();
                g.e0 = f[4].parse().unwrap();
            }
            Some("l0") => {
                for (k, v) in f[1].split(',').enumerate() {
                    g.l0[k] = v.parse().unwrap();
                }
            }
            Some("census") => {
                for kv in &f[1..] {
                    let (k, v) = kv.split_once('=').unwrap();
                    g.census.push((k.to_string(), v.parse().unwrap()));
                }
            }
            Some(t) => {
                assert_eq!(t.parse::<usize>().unwrap(), g.ticks.len() + 1);
                g.ticks.push((
                    f[1].parse().unwrap(),
                    f[2].parse().unwrap(),
                    f[3].parse().unwrap(),
                    f[4].parse().unwrap(),
                    f[5].parse().unwrap(),
                ));
            }
            None => {}
        }
    }
    g
}

/// The mineral golden fixture (mirror of golden.js mode M): 12 bands of
/// rock / mineral / gypsum / buried hot water / outer cold ocean by squared
/// distance, the same two perturbations, and an AIR pocket lattice.
fn mineral_fixture_cells() -> Vec<(Coord, MatId, i32)> {
    use MatId as M;
    const MB: [MatId; 12] = [
        M::ROCK,
        M::MIN,
        M::ROCK,
        M::GYP,
        M::ROCK,
        M::MIN,
        M::WATER,
        M::GYP,
        M::ROCK,
        M::GYP,
        M::WATER,
        M::WATER,
    ];
    let mut out = Vec::new();
    for x in 0..48i64 {
        for y in 0..48i64 {
            for z in 0..48i64 {
                let s = (2 * x - 47).pow(2) + (2 * y - 47).pow(2) + (2 * z - 47).pow(2);
                if s > 1764 {
                    continue;
                }
                let mut b = 12 * s / 1765;
                if (x * 7 + y * 13 + z * 29) % 11 == 0 && b + 1 < 12 {
                    b += 1;
                }
                if (x * 5 + y * 3 + z * 17) % 13 == 0 && b > 0 {
                    b -= 1;
                }
                let mut m = MB[b as usize];
                if b < 10 && (x * 3 + y * 7 + z * 11) % 17 == 0 {
                    m = M::AIR;
                }
                let q = if m == M::AIR {
                    0
                } else if m == M::WATER && b >= 10 {
                    40
                } else {
                    (3000 - 2100 * s / 1764).max(120) as i32
                };
                out.push((Coord::new(x, y, z), m, q));
            }
        }
    }
    out
}

/// Totals of every law counter over a run.
#[derive(Debug, Default)]
struct LawTotals {
    evaporated: u64,
    rained: u64,
    eroded: u64,
    dissolved: u64,
    settled: u64,
    flowed: u64,
    net_fired: u64,
    rose: u64,
}

fn add_report(t: &mut LawTotals, r: &StepReport) {
    t.evaporated += u64::from(r.evaporated);
    t.rained += u64::from(r.rained);
    t.eroded += u64::from(r.eroded);
    t.dissolved += u64::from(r.dissolved);
    t.settled += u64::from(r.settled);
    t.flowed += u64::from(r.flowed);
    t.net_fired += u64::from(r.net_fired);
    t.rose += u64::from(r.rose);
}

/// Runs every golden tick, asserting fingerprint, space heat, vapor and the
/// water-cycle counters against planet6, and both books, every tick.
fn run_golden_file(w: &mut ChemWorld, g: &Golden) -> LawTotals {
    assert_eq!(w.domain_len(), g.cells);
    assert_eq!(w.fingerprint(), g.fp0);
    assert_eq!(w.energy_total(), g.e0);
    assert_eq!(w.lane_totals(), g.l0);
    let mut tot = LawTotals::default();
    for (k, &(fp, sq, vapor, evap, rained)) in g.ticks.iter().enumerate() {
        let t = k + 1;
        let r = w.step().expect("tick");
        add_report(&mut tot, &r);
        assert_eq!(w.fingerprint(), fp, "fingerprint at tick {t}");
        assert_eq!(w.space_q(), sq, "space_q at tick {t}");
        assert_eq!(w.vapor(), vapor, "vapor at tick {t}");
        assert_eq!(
            (w.evaporated(), w.rained()),
            (evap, rained),
            "water cycle at tick {t}"
        );
        assert_eq!(w.energy_drift(), 0, "energy drift at tick {t}");
        assert_eq!(w.lane_drift(), [0; NE], "lane drift at tick {t}");
        assert_eq!(w.energy_total(), g.e0);
    }
    assert_eq!(w.check_books(), Ok(()));
    let mut want = g.census.clone();
    want.sort();
    assert_eq!(census(w), want, "census");
    tot
}

fn every_law(elements: bool) -> P6Params {
    P6Params {
        seed: 7,
        elements,
        ..P6Params::planet6_defaults()
    }
}

#[test]
fn p6_golden_f_trimmed_net_every_law() {
    let g = parse_golden(GOLDEN_F);
    assert_eq!(g.ticks.len(), 160);
    let mut w = ChemWorld::from_cells(
        fixture_cells(),
        [0; NE],
        Arc::new(MatTable::base()),
        every_law(true),
    )
    .unwrap();
    w.attach_net(trimmed());
    let tot = run_golden_file(&mut w, &g);
    eprintln!("golden F laws: {tot:?}");
    assert!(
        tot.evaporated > 0 && tot.rained > 0 && tot.eroded > 0 && tot.settled > 0 && tot.flowed > 0
    );
    assert!(tot.net_fired > 0 && tot.rose > 0);
}

#[test]
fn p6_golden_m_mineral_world_every_law() {
    let g = parse_golden(GOLDEN_M);
    assert_eq!(g.ticks.len(), 160);
    let mut w = ChemWorld::from_cells(
        mineral_fixture_cells(),
        [0; NE],
        Arc::new(MatTable::base()),
        every_law(false),
    )
    .unwrap();
    let tot = run_golden_file(&mut w, &g);
    eprintln!("golden M laws: {tot:?}");
    assert!(tot.evaporated > 0 && tot.rained > 0);
    assert!(tot.eroded > 0 && tot.dissolved > 0 && tot.settled > 0 && tot.flowed > 0);
}

/// Mode F on the full generated table (`-- --ignored`; needs the golden
/// JSON written by `node golden.js G 160` and the 676 KB table).
#[test]
#[ignore = "needs chem_network.gen.js and p6golden/golden_G.json outside the repo"]
fn p6_golden_g_full_table_every_law() {
    let dir = "/workspace/gameladder_chem_findings";
    let path = std::env::var("BITBLOCK_CHEMNET_JS")
        .unwrap_or_else(|_| format!("{dir}/chem_network.gen.js"));
    let net = ChemNet::from_js(&std::fs::read_to_string(&path).expect("full table text")).unwrap();
    let json = std::fs::read_to_string(format!("{dir}/p6golden/golden_G.json")).expect("golden G");
    // minimal extraction of the per-tick fields from golden.js's JSON
    let field = |obj: &str, key: &str| -> i64 {
        let at = obj.find(&format!("\"{key}\":")).unwrap() + key.len() + 3;
        let end = obj[at..].find([',', '}']).unwrap();
        obj[at..at + end].parse().unwrap()
    };
    let ticks_at = json.find("\"ticks\":[").unwrap();
    let mut w = ChemWorld::from_cells(
        fixture_cells(),
        [0; NE],
        Arc::new(MatTable::base()),
        every_law(true),
    )
    .unwrap();
    w.attach_net(Arc::new(net));
    let mut n = 0;
    for obj in json[ticks_at..].split("{\"t\":").skip(1) {
        let obj = format!("{{\"t\":{obj}");
        w.step().expect("tick");
        n += 1;
        assert_eq!(
            i64::from(w.fingerprint()),
            field(&obj, "fp"),
            "fp at tick {n}"
        );
        assert_eq!(w.space_q(), field(&obj, "spaceQ"), "space_q at tick {n}");
        assert_eq!(w.vapor(), field(&obj, "vapor"), "vapor at tick {n}");
        assert_eq!(w.energy_drift(), 0);
        assert_eq!(w.lane_drift(), [0; NE]);
    }
    assert_eq!(n, 160);
}

/// Only the five surface laws' switches matter: every other law off.
fn surface_only() -> P6Params {
    P6Params {
        elements: false,
        ..chem_only()
    }
}

fn assert_books(w: &ChemWorld, e0: i64) {
    assert_eq!(w.energy_total(), e0);
    assert_eq!(w.energy_drift(), 0);
    assert_eq!(w.lane_drift(), [0; NE]);
    assert_eq!(w.check_books(), Ok(()));
}

#[test]
fn p6_evaporation_and_rain_conserve() {
    // cold buried water at y0, hot surface water at y1, sky at y2
    let cells = vec![
        (Coord::new(0, 0, 0), MatId::WATER, 100),
        (Coord::new(0, 1, 0), MatId::WATER, 500),
        (Coord::new(0, 2, 0), MatId::AIR, 0),
    ];
    let mut w = ChemWorld::from_cells(
        cells,
        [0; NE],
        Arc::new(MatTable::base()),
        P6Params {
            rain: true,
            ..surface_only()
        },
    )
    .unwrap();
    let e0 = w.energy_total();
    let lanes0 = w.lane_totals();
    let t = run_until(&mut w, 20_000, |r| r.evaporated > 0);
    assert_eq!(w.material(Coord::new(0, 1, 0)), MatId::AIR);
    assert_eq!(
        w.heat(Coord::new(0, 1, 0)),
        Some(470),
        "evaporation pays LATENT_PAIR"
    );
    assert_eq!(w.vapor(), 1);
    assert_eq!(
        w.lane_totals(),
        lanes0,
        "the pair's 4 H + 2 O ride in vapor"
    );
    assert_books(&w, e0);
    let rt = run_until(&mut w, 2000, |r| r.rained > 0);
    assert!(
        (RAIN_START..RAIN_END).contains(&(rt % SEASON)),
        "rain at tick {rt}"
    );
    assert_eq!(
        w.material(Coord::new(0, 1, 0)),
        MatId::WATER,
        "rain fills the AIR on the waterline"
    );
    assert_eq!(
        w.heat(Coord::new(0, 1, 0)),
        Some(500),
        "latent heat comes home"
    );
    assert_eq!((w.vapor(), w.evaporated(), w.rained()), (0, 1, 1));
    assert_books(&w, e0);
    eprintln!("evaporated at tick {t}, rained at tick {rt}");
}

#[test]
fn p6_rain_waits_without_a_waterline() {
    let cells = vec![
        (Coord::new(0, 0, 0), MatId::ROCK, 100),
        (Coord::new(0, 1, 0), MatId::WATER, 500),
        (Coord::new(0, 2, 0), MatId::AIR, 0),
    ];
    let mut w = ChemWorld::from_cells(
        cells,
        [0; NE],
        Arc::new(MatTable::base()),
        P6Params {
            rain: true,
            ..surface_only()
        },
    )
    .unwrap();
    let e0 = w.energy_total();
    run_until(&mut w, 20_000, |r| r.evaporated > 0);
    for _ in 0..SEASON {
        assert_eq!(
            w.step().unwrap().rained,
            0,
            "no water left to touch: humid sky"
        );
    }
    assert_eq!(w.vapor(), 1);
    assert_books(&w, e0);
}

#[test]
fn p6_erode_weathers_exposed_rock_in_storms_only() {
    // a 3x3x3 rock/mineral cube: every cell but the centre faces space
    let mut cells = Vec::new();
    for x in 0..3 {
        for y in 0..3 {
            for z in 0..3 {
                let m = if (x + y + z) % 2 == 0 {
                    MatId::ROCK
                } else {
                    MatId::MIN
                };
                cells.push((Coord::new(x, y, z), m, 50));
            }
        }
    }
    let mut w = ChemWorld::from_cells(
        cells,
        [0; NE],
        Arc::new(MatTable::base()),
        P6Params {
            erode: true,
            ..surface_only()
        },
    )
    .unwrap();
    let e0 = w.energy_total();
    let mut eroded = 0;
    for _ in 0..(2 * SEASON) {
        let r = w.step().unwrap();
        if r.eroded > 0 {
            assert!(
                r.tick % SEASON < STORM_LEN,
                "erosion outside the storm at tick {}",
                r.tick
            );
        }
        eroded += r.eroded;
        assert_books(&w, e0);
    }
    assert!(eroded > 0);
    assert_eq!(
        w.material(Coord::new(1, 1, 1)),
        MatId::MIN,
        "the buried centre never weathers"
    );
    let sed = w.domain().filter(|(_, m, _)| *m == MatId::SED).count();
    assert_eq!(sed as u32, eroded);
}

#[test]
fn p6_dissolve_frees_crystal_water_into_the_pocket() {
    // water | gypsum | air
    let cells = vec![
        (Coord::new(0, 0, 0), MatId::WATER, 40),
        (Coord::new(1, 0, 0), MatId::GYP, 80),
        (Coord::new(2, 0, 0), MatId::AIR, 0),
    ];
    let mut w = ChemWorld::from_cells(
        cells,
        [0; NE],
        Arc::new(MatTable::base()),
        P6Params {
            dissolve: true,
            ..surface_only()
        },
    )
    .unwrap();
    let e0 = w.energy_total();
    let t = run_until(&mut w, 1000, |r| r.dissolved > 0);
    assert!(t % SEASON < STORM_LEN);
    assert_eq!(w.material(Coord::new(1, 0, 0)), MatId::SED);
    assert_eq!(w.material(Coord::new(2, 0, 0)), MatId::WATER);
    assert_eq!(
        w.heat(Coord::new(1, 0, 0)),
        Some(80),
        "bind 72 = 36 + 36: no heat moves"
    );
    assert_books(&w, e0);
}

#[test]
fn p6_dissolve_contested_pocket_goes_to_lowest_index() {
    // two wet gypsum crystals share one pocket: water | gyp | air | gyp | water
    let cells = || {
        vec![
            (Coord::new(0, 0, 0), MatId::WATER, 40),
            (Coord::new(1, 0, 0), MatId::GYP, 80),
            (Coord::new(2, 0, 0), MatId::AIR, 0),
            (Coord::new(3, 0, 0), MatId::GYP, 80),
            (Coord::new(4, 0, 0), MatId::WATER, 40),
        ]
    };
    let key = |x: i64| super::p6::cell_key(Coord::new(x, 0, 0));
    let mut contested = 0;
    for seed in 0..400u32 {
        let p = P6Params {
            dissolve: true,
            seed,
            ..surface_only()
        };
        let mut w = ChemWorld::from_cells(cells(), [0; NE], Arc::new(MatTable::base()), p).unwrap();
        let e0 = w.energy_total();
        let t = run_until(&mut w, 1000, |r| r.dissolved > 0);
        let roll = |x| lattice(seed ^ 0x77aa11, key(x), t as u32) % 1000;
        assert_books(&w, e0);
        let pocket_water = w.material(Coord::new(2, 0, 0)) == MatId::WATER;
        assert!(pocket_water);
        let seds: Vec<i64> = (0..5)
            .filter(|&x| w.material(Coord::new(x, 0, 0)) == MatId::SED)
            .collect();
        assert_eq!(seds.len(), 1, "one pocket, one dissolution");
        if roll(1) < 60 && roll(3) < 60 {
            assert_eq!(seds, vec![1], "seed {seed}: lowest index owns the claim");
            contested += 1;
        }
    }
    assert!(contested > 0, "no contested tick found");
    eprintln!("contested claims: {contested}");
}

#[test]
fn p6_settle_sinks_sediment_and_bedrock_holds() {
    // column A: water y0, air y1, sediment y2. column B: air y0, rock y1.
    let cells = vec![
        (Coord::new(0, 0, 0), MatId::WATER, 40),
        (Coord::new(0, 1, 0), MatId::AIR, 7),
        (Coord::new(0, 2, 0), MatId::SED, 90),
        (Coord::new(5, 0, 0), MatId::AIR, 0),
        (Coord::new(5, 1, 0), MatId::ROCK, 60),
    ];
    let mut w = ChemWorld::from_cells(
        cells,
        [0; NE],
        Arc::new(MatTable::base()),
        P6Params {
            settle: true,
            ..surface_only()
        },
    )
    .unwrap();
    let e0 = w.energy_total();
    let col = |w: &ChemWorld| {
        (0..3)
            .map(|y| w.material(Coord::new(0, y, 0)))
            .collect::<Vec<_>>()
    };
    assert_eq!(w.step().unwrap().settled, 1);
    assert_eq!(
        col(&w),
        vec![MatId::WATER, MatId::SED, MatId::AIR],
        "one cell per tick"
    );
    assert_eq!(w.step().unwrap().settled, 1);
    assert_eq!(
        col(&w),
        vec![MatId::SED, MatId::WATER, MatId::AIR],
        "water displaced upward"
    );
    assert_eq!(w.step().unwrap().settled, 0);
    assert_eq!(
        w.material(Coord::new(5, 1, 0)),
        MatId::ROCK,
        "bedrock does not cave"
    );
    assert_eq!(
        w.heat(Coord::new(0, 2, 0)),
        Some(90),
        "heat stays with the cell"
    );
    assert_books(&w, e0);
}

#[test]
fn p6_flow_falls_then_spreads_in_fixed_order() {
    // falls: water above air
    let fall = vec![
        (Coord::new(1, 1, 1), MatId::WATER, 40),
        (Coord::new(1, 0, 1), MatId::AIR, 0),
        (Coord::new(1, 1, 0), MatId::AIR, 0),
    ];
    let mut w = ChemWorld::from_cells(
        fall,
        [0; NE],
        Arc::new(MatTable::base()),
        P6Params {
            flow: true,
            ..surface_only()
        },
    )
    .unwrap();
    let e0 = w.energy_total();
    assert_eq!(w.step().unwrap().flowed, 1);
    assert_eq!(
        w.material(Coord::new(1, 0, 1)),
        MatId::WATER,
        "falling beats spreading"
    );
    assert_eq!(w.material(Coord::new(1, 1, 1)), MatId::AIR);
    assert_books(&w, e0);
    // spreads: no air below; -z is tried before -x, +z, +x
    let spread = vec![
        (Coord::new(1, 0, 1), MatId::WATER, 40),
        (Coord::new(2, 0, 1), MatId::AIR, 0),
        (Coord::new(0, 0, 1), MatId::AIR, 0),
        (Coord::new(1, 0, 0), MatId::AIR, 0),
    ];
    let mut w = ChemWorld::from_cells(
        spread,
        [0; NE],
        Arc::new(MatTable::base()),
        P6Params {
            flow: true,
            ..surface_only()
        },
    )
    .unwrap();
    let e0 = w.energy_total();
    w.step().unwrap();
    assert_eq!(w.material(Coord::new(1, 0, 0)), MatId::WATER, "-z first");
    assert_eq!(w.material(Coord::new(1, 0, 1)), MatId::AIR);
    assert_books(&w, e0);
}

#[test]
fn p6_surface_laws_default_on_and_heat_only_off() {
    let d = P6Params::planet6_defaults();
    assert!(d.rain && d.erode && d.dissolve && d.settle && d.flow);
    let h = P6Params::heat_only();
    assert!(!(h.rain || h.erode || h.dissolve || h.settle || h.flow));
}

// ---- M1 booked energy input ----

#[test]
fn m1_energy_input_off_by_default() {
    assert_eq!(P6Params::default().energy_in, EnergyInput::OFF);
    assert_eq!(P6Params::planet6_defaults().energy_in, EnergyInput::OFF);
    assert!(EnergyInput::OFF.is_off());
    let mut w = fixture(every_law(true), Some(trimmed()));
    for _ in 0..20 {
        assert_eq!(w.step().unwrap().injected, 0);
    }
    assert_eq!(w.input_q(), 0);
    w.check_books().unwrap();
}

#[test]
fn m1_sun_dir_is_a_day() {
    let e = EnergyInput {
        sun: 8,
        day: 600,
        core: 0,
        core_r: 0,
    };
    assert_eq!(e.sun_dir(0), (1024, 0)); // sunrise on +x
    assert_eq!(e.sun_dir(150), (0, 1024)); // noon
    assert_eq!(e.sun_dir(300), (-1024, 0)); // sunset
    assert_eq!(e.sun_dir(450), (0, -1024)); // midnight
    assert_eq!(e.sun_dir(600), e.sun_dir(0));
    assert_eq!(EnergyInput { day: 0, ..e }.sun_dir(12345), (0, 1024));
}

#[test]
fn m1_energy_input_is_booked_every_tick() {
    let p = P6Params {
        energy_in: EnergyInput {
            sun: 8,
            day: 40,
            core: 200,
            core_r: 0,
        },
        ..every_law(true)
    };
    let mut w =
        ChemWorld::from_cells(fixture_cells(), [0; NE], Arc::new(MatTable::base()), p).unwrap();
    w.attach_net(trimmed());
    let mut off = fixture(every_law(true), Some(trimmed()));
    let (mut day, mut night) = (0, 0);
    let mut sum = 0i64;
    for _ in 0..80 {
        let r = w.step().unwrap();
        off.step().unwrap();
        // night half of the day: only the core term lands
        if r.injected > 200 {
            day += 1
        } else {
            night += 1
        }
        assert!(r.injected >= 200);
        sum += r.injected;
        w.check_books().unwrap();
        assert_eq!(w.energy_drift(), 0);
        assert!(w.lane_drift().iter().all(|&x| x == 0));
    }
    assert_eq!(w.input_q(), sum);
    assert!(day > 0 && night > 0, "day {day} night {night}");
    assert_ne!(
        w.fingerprint(),
        off.fingerprint(),
        "input changes the world"
    );
}

// ---- WaterCycle::Driven (unscripted water cycle) ----

#[test]
fn driven_cycle_is_off_by_default() {
    assert_eq!(P6Params::default().cycle, WaterCycle::Classic);
    assert_eq!(P6Params::planet6_defaults().cycle, WaterCycle::Classic);
    assert_eq!(P6Params::heat_only().cycle, WaterCycle::Classic);
}

#[test]
fn driven_wide_roll_has_no_16bit_repeat() {
    // planet6's lattice repeats every 65,536 ticks and every 65,536 cell keys
    use super::p6::cell_key;
    let c = Coord::new(3, 5, 7);
    assert_eq!(
        lattice(7, cell_key(c), 10),
        lattice(7, cell_key(c), 10 + 65_536)
    );
    let mut same_t = 0;
    let mut same_c = 0;
    for t in 0..2000u64 {
        if wide_roll(7, c, t) % 1000 == wide_roll(7, c, t + 65_536) % 1000 {
            same_t += 1;
        }
        // cells 65,536 keys apart in planet6's 48-lattice: x + 28.44.. -> use
        // a coordinate whose key differs by exactly 65,536 (dx*2304 + dy*48 + dz)
        let d = Coord::new(3 + 28, 5 + 21, 7 + 16); // 28*2304 + 21*48 + 16 = 65,536
        assert_eq!(cell_key(d).wrapping_sub(cell_key(c)), 65_536);
        if wide_roll(7, c, t) % 1000 == wide_roll(7, d, t) % 1000 {
            same_c += 1;
        }
    }
    // independent rolls agree ~1/1000 of the time
    assert!(
        same_t < 12 && same_c < 12,
        "tick repeats {same_t}, cell repeats {same_c}"
    );
}

/// A 24x24 rock plate under one layer of air: no water anywhere, so the
/// classic law has no waterline and never rains.
fn plate(params: P6Params, vapor: i64) -> ChemWorld {
    let mut cells = Vec::new();
    for x in 0..24i64 {
        for z in 0..24i64 {
            cells.push((Coord::new(x, 0, z), MatId::ROCK, 300));
            // a cold corner and a warm rest so placement weights differ
            let q = if x < 6 && z < 6 { 0 } else { 400 };
            cells.push((Coord::new(x, 1, z), MatId::AIR, q));
        }
    }
    let mut w = ChemWorld::from_cells(cells, [0; NE], Arc::new(MatTable::base()), params).unwrap();
    w.vapor = vapor;
    w.pin_t0();
    w
}

fn plate_params(cycle: WaterCycle) -> P6Params {
    P6Params {
        cycle,
        heat: false,
        phase: false,
        flow: false,
        erode: false,
        dissolve: false,
        settle: false,
        ..P6Params::planet6_defaults()
    }
}

#[test]
fn driven_rain_follows_vapor_not_the_calendar_and_spreads() {
    let mut classic = plate(plate_params(WaterCycle::Classic), 4000);
    // a flat plate: world -y is its down (the core fall test is for balls, §5s)
    let mut w = plate(
        P6Params {
            rain_fall: RainFall::Down,
            ..plate_params(WaterCycle::Driven)
        },
        4000,
    );
    let mut landed = std::collections::BTreeSet::new();
    let mut off_window = 0u32;
    let mut cold = 0usize;
    for _ in 0..60 {
        assert_eq!(
            classic.step().unwrap().rained,
            0,
            "classic needs a waterline"
        );
        let before: Vec<MatId> = w.domain().map(|(_, m, _)| m).collect();
        let r = w.step().unwrap();
        // vapor-driven rate: vapor/RAIN_TAU drops (+ a fractional roll)
        let phase = w.tick() % SEASON;
        if !(RAIN_START..RAIN_END).contains(&phase) {
            off_window += r.rained;
        }
        for (k, (c, m, _)) in w.domain().enumerate() {
            if before[k] == MatId::AIR && m == MatId::WATER {
                landed.insert(c);
                if c.x < 6 && c.z < 6 {
                    cold += 1;
                }
            }
        }
        w.check_books().unwrap();
        assert_eq!(w.energy_drift(), 0);
        assert!(w.lane_drift().iter().all(|&x| x == 0));
    }
    assert!(off_window > 0, "rain outside planet6's rain window");
    assert!(
        w.rained() >= 60 * 7,
        "~vapor/512 = 7 drops per tick, got {}",
        w.rained()
    );
    assert!(
        landed.len() > 100,
        "rain spreads over the plate: {} cells",
        landed.len()
    );
    // the cold corner (36/576 of the area, weight 16 vs 1) fills first
    assert_eq!(cold, 36, "every cold cell got rain");
}

#[test]
fn driven_running_water_erodes_dissolves_entrains_and_conserves() {
    let p = P6Params {
        seed: 7,
        cycle: WaterCycle::Driven,
        energy_in: EnergyInput::M1,
        ..P6Params::planet6_defaults()
    };
    let mut w = ChemWorld::from_cells(
        mineral_fixture_cells(),
        [0; NE],
        Arc::new(MatTable::base()),
        p,
    )
    .unwrap();
    let mut tot = StepReport::default();
    let mut storm_free = 0u32;
    for _ in 0..240 {
        let r = w.step().unwrap();
        tot.eroded += r.eroded;
        tot.dissolved += r.dissolved;
        tot.entrained += r.entrained;
        tot.evaporated += r.evaporated;
        tot.rained += r.rained;
        if w.tick() % SEASON >= STORM_LEN {
            storm_free += r.eroded + r.dissolved;
        }
        w.check_books().unwrap();
        assert_eq!(w.energy_drift(), 0);
        assert!(w.lane_drift().iter().all(|&x| x == 0));
    }
    eprintln!("driven M fixture 240 ticks: {tot:?}");
    assert!(
        tot.eroded > 0 && tot.dissolved > 0 && tot.entrained > 0,
        "{tot:?}"
    );
    assert!(tot.evaporated > 0 && tot.rained > 0);
    assert!(storm_free > 0, "weathering outside planet6's storm window");
}

// ---- Driven relief laws: dry weathering, sediment scour, core-heat uplift ----

fn relief_params(cycle: WaterCycle) -> P6Params {
    P6Params {
        seed: 11,
        cycle,
        energy_in: EnergyInput::M1,
        // the plate (and the §5t/§5u sky-shell tests built on these params)
        // are world -y worlds; §5w coreward gravity has its own tests
        gravity: Gravity::Down,
        ..P6Params::planet6_defaults()
    }
}

/// 8x8 plate: ROCK y=0..=3 (hot roots on x<4), optional SED blanket at y=4
/// on x<4, a WATER heap at y=5..=6 over x<2 (which flows), AIR up to y=9.
fn relief_plate(cycle: WaterCycle, sed: bool, water: bool) -> ChemWorld {
    let mut cells = Vec::new();
    for x in 0..8i64 {
        for z in 0..8i64 {
            for y in 0..10i64 {
                let (m, q) = match y {
                    0..=3 => (MatId::ROCK, if x < 4 { 2400 } else { 300 }),
                    4 if sed && x < 4 => (MatId::SED, 300),
                    5 | 6 if water && x < 2 => (MatId::WATER, 40),
                    _ => (MatId::AIR, 30),
                };
                cells.push((Coord::new(x, y, z), m, q));
            }
        }
    }
    ChemWorld::from_cells(
        cells,
        [0; NE],
        Arc::new(MatTable::base()),
        relief_params(cycle),
    )
    .unwrap()
}

fn run_relief(w: &mut ChemWorld, ticks: u32) -> StepReport {
    let mut tot = StepReport::default();
    for _ in 0..ticks {
        let r = w.step().unwrap();
        tot.weathered += r.weathered;
        tot.scoured += r.scoured;
        tot.uplifted += r.uplifted;
        tot.eroded += r.eroded;
        w.check_books().unwrap();
        assert_eq!(w.energy_drift(), 0);
        assert!(w.lane_drift().iter().all(|&x| x == 0));
    }
    tot
}

#[test]
fn driven_dry_rock_weathers_without_water_and_classic_does_not() {
    let mut w = relief_plate(WaterCycle::Driven, false, false);
    let tot = run_relief(&mut w, 600);
    eprintln!("dry weathering 600 ticks: {tot:?}");
    assert!(tot.weathered > 0, "{tot:?}");
    assert_eq!(tot.eroded, 0, "no water: nothing for water erosion to do");
    let mut c = relief_plate(WaterCycle::Classic, false, false);
    let ct = run_relief(&mut c, 600);
    assert_eq!(
        (ct.weathered, ct.scoured, ct.uplifted),
        (0, 0, 0),
        "classic is untouched"
    );
}

#[test]
fn driven_moving_water_scours_sediment_off_rock() {
    let mut w = relief_plate(WaterCycle::Driven, true, true);
    let tot = run_relief(&mut w, 200);
    eprintln!("scour 200 ticks: {tot:?}");
    assert!(tot.scoured > 0, "{tot:?}");
}

/// [`relief_plate`] thickened to a 10-deep slab (hot side heated from below,
/// not cooled by space under its roots) on a thin ROCK stem down to y = -24 under (3, 3), with the
/// core pinned at the stem foot: the radial frame of uplift/plume then points
/// (almost) straight up through the plate, as under the pole of a big ball.
fn relief_plate_deep(cycle: WaterCycle) -> ChemWorld {
    let mut cells = Vec::new();
    for x in 0..8i64 {
        for z in 0..8i64 {
            for y in -6..10i64 {
                let (m, q) = match y {
                    -6..=3 => (MatId::ROCK, if x < 4 { 2400 } else { 300 }),
                    _ => (MatId::AIR, 30),
                };
                cells.push((Coord::new(x, y, z), m, q));
            }
        }
    }
    for y in -24..-6i64 {
        cells.push((Coord::new(3, y, 3), MatId::ROCK, 300));
    }
    let mut w = ChemWorld::from_cells(
        cells,
        [0; NE],
        Arc::new(MatTable::base()),
        relief_params(cycle),
    )
    .unwrap();
    w.core_pin = Some(Coord::new(3, -24, 3));
    w.topo = None;
    w
}

#[test]
fn driven_core_heat_lifts_hot_columns_by_permutation() {
    // Core heat only: with transparent air the sun now lands on the rock tops
    // and warms this thin plate from above, which (correctly) flips the root
    // gradient that `heated_from_below` reads; the law under test is the core.
    let mut w = relief_plate_deep(WaterCycle::Driven);
    let mut p = relief_params(WaterCycle::Driven);
    p.energy_in.sun = 0;
    w.set_params(p);
    let count = |w: &ChemWorld| {
        let mut n = std::collections::BTreeMap::new();
        for (_, m, _) in w.domain() {
            *n.entry(m.0).or_insert(0usize) += 1;
        }
        n
    };
    let tot = run_relief(&mut w, 4000);
    eprintln!("uplift 4000 ticks: {tot:?}");
    assert!(tot.uplifted > 0, "{tot:?}");
    // rock-family cells (ROCK+MIN+SED) are only permuted / re-labelled
    let n = count(&w);
    let rocky: usize = [MatId::ROCK, MatId::MIN, MatId::SED]
        .iter()
        .map(|m| n.get(&m.0).copied().unwrap_or(0))
        .sum();
    assert_eq!(rocky, 8 * 8 * 10 + 18);
    // uplift off without core heat
    p.energy_in.core = 0;
    let mut cold = relief_plate_deep(WaterCycle::Driven);
    cold.set_params(p);
    assert_eq!(run_relief(&mut cold, 400).uplifted, 0);
}

// ---- Driven deep convection cell (longer-range relief driver) ----

/// 16x6 slab: ROCK y=0..=9 (hot roots on x<4), AIR y=10..=13.
fn plume_slab(cycle: WaterCycle, core: i32) -> ChemWorld {
    let mut cells = Vec::new();
    for x in 0..16i64 {
        for z in 0..6i64 {
            for y in 0..14i64 {
                let (m, q) = if y <= 9 {
                    (MatId::ROCK, if x < 4 { 2400 } else { 300 })
                } else {
                    (MatId::AIR, 30)
                };
                cells.push((Coord::new(x, y, z), m, q));
            }
        }
    }
    let mut p = relief_params(cycle);
    p.energy_in.core = core;
    ChemWorld::from_cells(cells, [0; NE], Arc::new(MatTable::base()), p).unwrap()
}

fn tops(w: &ChemWorld) -> std::collections::BTreeMap<(i64, i64), i64> {
    let mut t = std::collections::BTreeMap::new();
    for (c, m, _) in w.domain() {
        if m != MatId::AIR && m != MatId::WATER {
            let e = t.entry((c.x, c.z)).or_insert(c.y);
            *e = (*e).max(c.y);
        }
    }
    t
}

#[test]
fn driven_plume_builds_broad_relief_by_permutation() {
    let mut w = plume_slab(WaterCycle::Driven, 40);
    let count = |w: &ChemWorld| {
        let mut n = std::collections::BTreeMap::new();
        for (_, m, _) in w.domain() {
            *n.entry(m.0).or_insert(0usize) += 1;
        }
        n
    };
    let before = count(&w);
    let mut plumed = 0u32;
    for _ in 0..3000 {
        let r = w.step().unwrap();
        plumed += r.plumed;
        w.check_books().unwrap();
        assert_eq!(w.energy_drift(), 0);
        assert!(w.lane_drift().iter().all(|&x| x == 0));
    }
    eprintln!("plume 3000 ticks: plumed={plumed} tops={:?}", tops(&w));
    assert!(plumed > 0);
    // pure permutation / polymorph relabels: rock family and air counts kept
    let after = count(&w);
    let fam = |n: &std::collections::BTreeMap<u8, usize>| -> usize {
        [MatId::ROCK, MatId::MIN, MatId::SED]
            .iter()
            .map(|m| n.get(&m.0).copied().unwrap_or(0))
            .sum()
    };
    assert_eq!(fam(&before), fam(&after));
    assert_eq!(before.get(&MatId::AIR.0), after.get(&MatId::AIR.0));
    // the hot side stands higher on average than the cool far side
    let t = tops(&w);
    let mean = |xs: std::ops::Range<i64>| {
        let v: Vec<i64> = t
            .iter()
            .filter(|((x, _), _)| xs.contains(x))
            .map(|(_, y)| *y)
            .collect();
        v.iter().sum::<i64>() as f64 / v.len() as f64
    };
    assert!(
        mean(0..4) > mean(6..16),
        "hot {} cool {}",
        mean(0..4),
        mean(6..16)
    );
    // whole-column form: the cold column subsides as a block, so the plume
    // seeds no void under rock (the old root-only form buried surface air)
    let buried_air = w
        .domain()
        .filter(|(c, m, _)| *m == MatId::AIR && t.get(&(c.x, c.z)).is_some_and(|&top| c.y < top))
        .count();
    eprintln!("buried air cells {buried_air}");
    assert_eq!(buried_air, 0);
    let mut worst = 0;
    for (&(x, z), &y) in &t {
        for (dx, dz) in [(1, 0), (0, 1)] {
            if let Some(&y2) = t.get(&(x + dx, z + dz)) {
                worst = worst.max((y - y2).abs());
            }
        }
    }
    eprintln!("worst neighbour step {worst}");
    // no plume without core heat, and none in classic
    let mut cold = plume_slab(WaterCycle::Driven, 0);
    let mut cl = plume_slab(WaterCycle::Classic, 40);
    for _ in 0..400 {
        assert_eq!(cold.step().unwrap().plumed, 0);
        assert_eq!(cl.step().unwrap().plumed, 0);
    }
}

/// Plume-only slab: [`plume_slab`] geometry with `air` fluid layers over the
/// rock (tops at `y = 9`), Q fixed (the plume is called alone, no heat pass),
/// core pinned under the slab's middle.
fn plume_only_slab(air: i64) -> ChemWorld {
    let mut cells = Vec::new();
    for x in 0..16i64 {
        for z in 0..6i64 {
            for y in 0..10 + air {
                let (m, q) = if y <= 9 {
                    (MatId::ROCK, if x < 4 { 2400 } else { 300 })
                } else {
                    (MatId::AIR, 30)
                };
                cells.push((Coord::new(x, y, z), m, q));
            }
        }
    }
    let mut p = relief_params(WaterCycle::Driven);
    p.energy_in.core = 40;
    let mut w = ChemWorld::from_cells(cells, [0; NE], Arc::new(MatTable::base()), p).unwrap();
    // core well below the slab: every column runs along its outward axis
    w.core_pin = Some(Coord::new(8, 0, 3));
    w.topo = None;
    w
}

/// Run `plume_pass` alone for `ticks` ticks; `each(world, plumed, tops before)`.
fn run_plume_only(
    w: &mut ChemWorld,
    ticks: u64,
    mut each: impl FnMut(&ChemWorld, u32, &std::collections::BTreeMap<(i64, i64), i64>),
) -> u32 {
    let topo = w.topology();
    let mut n = 0;
    for _ in 0..ticks {
        w.tick += 1;
        let before = tops(w);
        let mut rep = StepReport::default();
        w.plume_pass(&topo, &mut rep);
        n += rep.plumed;
        w.check_books().unwrap();
        assert_eq!(w.energy_drift(), 0);
        assert!(w.lane_drift().iter().all(|&x| x == 0));
        each(w, rep.plumed, &before);
    }
    n
}

#[test]
fn whole_column_plume_lowers_one_cold_top_and_raises_one_hot_top_per_firing() {
    let mut w = plume_only_slab(4);
    let rock0 = w.domain().filter(|(_, m, _)| *m == MatId::ROCK).count();
    let fired = run_plume_only(&mut w, 4000, |w, plumed, before| {
        let after = tops(w);
        let (mut down, mut up) = (0u32, 0u32);
        for (k, &y0) in before {
            match after[k] - y0 {
                0 => {}
                -1 => down += 1,
                1 => up += 1,
                d => panic!("column {k:?} moved {d} in one tick"),
            }
        }
        // each firing: exactly one cold top a cell down, one hot top a cell up
        assert_eq!((down, up), (plumed, plumed));
        // and never a void under rock
        assert!(
            !w.domain()
                .any(|(c, m, _)| m == MatId::AIR && c.y < after[&(c.x, c.z)])
        );
    });
    assert!(fired > 0);
    assert_eq!(
        w.domain().filter(|(_, m, _)| *m == MatId::ROCK).count(),
        rock0
    );
    let t = tops(&w);
    let mean = |xs: std::ops::Range<i64>| {
        let v: Vec<i64> = t
            .iter()
            .filter(|((x, _), _)| xs.contains(x))
            .map(|(_, y)| *y)
            .collect();
        v.iter().sum::<i64>() as f64 / v.len() as f64
    };
    eprintln!(
        "whole-column plume: fired {fired}, hot {} cool {}",
        mean(0..4),
        mean(4..16)
    );
    assert!(mean(0..4) > 9.0 && mean(4..16) < 9.0);
}

#[test]
fn whole_column_plume_keeps_neighbour_steps_within_repose() {
    use crate::chem::p6_rest::PLUME_REPOSE;
    let mut w = plume_only_slab(12);
    let mut worst = 0;
    let fired = run_plume_only(&mut w, 30_000, |w, _, _| {
        let t = tops(w);
        for (&(x, z), &y) in &t {
            for (dx, dz) in [(1, 0), (0, 1)] {
                if let Some(&y2) = t.get(&(x + dx, z + dz)) {
                    worst = worst.max((y - y2).abs());
                }
            }
        }
        assert!(
            worst <= PLUME_REPOSE,
            "step {worst} > repose {PLUME_REPOSE}"
        );
    });
    eprintln!("repose: fired {fired}, worst step {worst}");
    // relief built up to (but never past) the repose bound
    assert!(fired > 0 && worst >= 2);
}

#[test]
fn whole_column_plume_needs_headroom_over_the_hot_slot() {
    use crate::chem::p6_rest::PLUME_HEADROOM;
    assert_eq!(PLUME_HEADROOM, 1);
    // one fluid layer: the hot top would rise into the outermost layer
    let mut tight = plume_only_slab(1);
    assert_eq!(run_plume_only(&mut tight, 3000, |_, _, _| {}), 0);
    // two fluid layers: it fires, and rock never reaches the top layer
    let mut room = plume_only_slab(2);
    let fired = run_plume_only(&mut room, 3000, |w, _, _| {
        assert!(!w.domain().any(|(c, m, _)| c.y == 11 && m != MatId::AIR));
    });
    assert!(fired > 0);
}

#[test]
fn whole_column_plume_only_moves_columns_along_their_outward_axis() {
    use crate::chem::p6_rest::outward_step;
    // radial ball with a lateral hot half (x > 0), core pinned at the origin
    let mut cells = Vec::new();
    let r = 8i64;
    for x in -r..=r {
        for z in -r..=r {
            let rho2 = x * x + z * z;
            if rho2 > r * r {
                continue;
            }
            let top = (r * r - rho2).isqrt();
            for y in -top..=top + 6 {
                let d2 = rho2 + y * y;
                let c = if y <= top {
                    (
                        MatId::ROCK,
                        (3000 - 30 * d2 + if x > 0 { 900 } else { 0 }) as i32,
                    )
                } else {
                    (MatId::AIR, 30)
                };
                cells.push((Coord::new(x, y, z), c.0, c.1));
            }
        }
    }
    let mut p = relief_params(WaterCycle::Driven);
    p.energy_in.core = 40;
    let mut w = ChemWorld::from_cells(cells, [0; NE], Arc::new(MatTable::base()), p).unwrap();
    w.core_pin = Some(Coord::new(0, 0, 0));
    w.topo = None;
    let fired = run_plume_only(&mut w, 6000, |w, _, before| {
        for (k, y) in tops(w) {
            if y != before[&k] {
                assert_eq!(
                    outward_step((k.0, before[&k], k.1)).1,
                    1,
                    "flank column {k:?} moved"
                );
            }
        }
    });
    eprintln!("flank gate ball: fired {fired}");
    assert!(fired > 0);
}

// ---- Driven hydrothermal degassing (return leg for plume-buried water) ----

/// 8x8 plate: ROCK y=0..=3 (Q 1200), a hot WATER pool y=4..=7 (Q 900) on x<6,
/// AIR up to y=11 (Q 30).
fn degas_plate(cycle: WaterCycle, core: i32) -> ChemWorld {
    let mut cells = Vec::new();
    for x in 0..8i64 {
        for z in 0..8i64 {
            for y in 0..12i64 {
                let (m, q) = match y {
                    0..=3 => (MatId::ROCK, 1200),
                    4..=7 if x < 6 => (MatId::WATER, 900),
                    _ => (MatId::AIR, 30),
                };
                cells.push((Coord::new(x, y, z), m, q));
            }
        }
    }
    let mut p = relief_params(cycle);
    p.energy_in.core = core;
    ChemWorld::from_cells(cells, [0; NE], Arc::new(MatTable::base()), p).unwrap()
}

#[test]
fn driven_hot_submerged_water_degasses_to_vapor_with_exact_books() {
    let count =
        |w: &ChemWorld, ms: &[MatId]| w.domain().filter(|(_, m, _)| ms.contains(m)).count() as i64;
    let mut w = degas_plate(WaterCycle::Driven, 40);
    let fluid0 = count(&w, &[MatId::AIR, MatId::WATER, MatId::ICE]);
    let rock0 = count(&w, &[MatId::ROCK, MatId::MIN, MatId::SED]);
    let h2o0 = count(&w, &[MatId::WATER, MatId::ICE]) + w.vapor();
    let (mut degassed, mut evap, mut max_tick) = (0u32, 0u32, 0u32);
    for _ in 0..600 {
        let r = w.step().unwrap();
        degassed += r.degassed;
        evap += r.evaporated;
        max_tick = max_tick.max(r.degassed);
        w.check_books().unwrap();
        assert_eq!(w.energy_drift(), 0);
        assert!(w.lane_drift().iter().all(|&x| x == 0));
        // water family is only moved between cells and the vapor pool
        assert_eq!(count(&w, &[MatId::WATER, MatId::ICE]) + w.vapor(), h2o0);
        // occupancy: water <-> air only, rock family untouched by degassing
        assert_eq!(count(&w, &[MatId::AIR, MatId::WATER, MatId::ICE]), fluid0);
        assert_eq!(count(&w, &[MatId::ROCK, MatId::MIN, MatId::SED]), rock0);
    }
    eprintln!(
        "degas 600 ticks: degassed={degassed} evaporated={evap} vapor={}",
        w.vapor()
    );
    assert!(degassed > 0);
    assert!(max_tick <= super::p6_rest::DEGAS_MAX_PER_TICK);
    // silent without core heat and in classic
    let mut cold = degas_plate(WaterCycle::Driven, 0);
    let mut cl = degas_plate(WaterCycle::Classic, 40);
    for _ in 0..300 {
        assert_eq!(cold.step().unwrap().degassed, 0);
        assert_eq!(cl.step().unwrap().degassed, 0);
    }
}

#[test]
fn degas_needs_a_rock_bed() {
    // a hot pool on the domain floor (no cell under it): submerged but no
    // crust to heat it from below, so degassing never fires (evaporation does)
    let mut cells = Vec::new();
    for x in 0..6i64 {
        for z in 0..6i64 {
            for y in 0..8i64 {
                let (m, q) = if y <= 3 {
                    (MatId::WATER, 900)
                } else {
                    (MatId::AIR, 30)
                };
                cells.push((Coord::new(x, y, z), m, q));
            }
        }
    }
    let mut w = ChemWorld::from_cells(
        cells,
        [0; NE],
        Arc::new(MatTable::base()),
        relief_params(WaterCycle::Driven),
    )
    .unwrap();
    let mut evap = 0;
    for _ in 0..300 {
        let r = w.step().unwrap();
        assert_eq!(r.degassed, 0);
        evap += r.evaporated;
        w.check_books().unwrap();
    }
    assert!(evap > 0);
}

// ---- Sky shell: AIR headroom above the ball (outer relief, not void depth) ----

fn sky_params() -> P6Params {
    relief_params(WaterCycle::Driven)
}

#[test]
fn sky_shell_keeps_t0_books_cells_and_core() {
    let cfg = small_budget();
    let mut w0 = genesis_p6(&cfg, &sky_params()).unwrap();
    let mut w1 = genesis_sky(&cfg, &sky_params(), 5).unwrap();
    assert_eq!(w1.sky_height(), 5);
    assert_eq!(w1.energy_total(), w0.energy_total());
    assert_eq!(w1.t0_energy(), w0.t0_energy());
    assert_eq!(w1.lane_totals(), w0.lane_totals());
    assert_eq!(w1.check_books(), Ok(()));
    // every old cell keeps its material and heat; every new cell is AIR, Q=0,
    // above the ball, inside the footprint, at most 5 above its column top
    let old: std::collections::HashMap<Coord, (MatId, i32)> =
        w0.domain().map(|(c, m, q)| (c, (m, q))).collect();
    let r = cfg.shape.radius as i64;
    let mut added = 0;
    for (c, m, q) in w1.domain() {
        if let Some(&o) = old.get(&c) {
            assert_eq!(o, (m, q));
            continue;
        }
        added += 1;
        assert_eq!((m, q), (MatId::AIR, 0));
        let (dx, dy, dz) = (
            c.x - cfg.shape.center.x,
            c.y - cfg.shape.center.y,
            c.z - cfg.shape.center.z,
        );
        let rho2 = dx * dx + dz * dz;
        assert!(rho2 <= r * r);
        let top = isqrt((r * r - rho2) as u64) as i64;
        assert!(dy > top && dy <= top + 5, "{c:?}");
    }
    let cols = (-r..=r)
        .flat_map(|x| (-r..=r).map(move |z| (x, z)))
        .filter(|(x, z)| x * x + z * z <= r * r)
        .count();
    assert_eq!(added, cols * 5);
    assert_eq!(w1.domain_len(), w0.domain_len() + added);
    // core heat input stays at the pre-shell centre
    let k0 = w0.topology().core.unwrap() as usize;
    let c0 = w0.cells[k0];
    let k1 = w1.topology().core.unwrap() as usize;
    let c1 = w1.cells[k1];
    assert_eq!(c0, c1);
    // genesis surface is the datum: nothing stands above it yet
    let lr = w1.land_relief(&cfg.shape);
    assert_eq!(lr.columns as usize, cols);
    assert_eq!(lr.above_datum, 0);
    assert!(lr.max_h <= 0);
    assert_eq!(
        w0.land_relief(&cfg.shape).sum_h,
        lr.sum_h,
        "probe agrees with/without shell"
    );
}

#[test]
fn sky_zero_is_a_noop() {
    let cfg = small_budget();
    let w0 = genesis_p6(&cfg, &sky_params()).unwrap();
    let mut w1 = genesis_p6(&cfg, &sky_params()).unwrap();
    assert_eq!(w1.add_sky_shell(&cfg.shape, 0), 0);
    assert_eq!(w1.sky_height(), 0);
    assert_eq!(w0.fingerprint(), w1.fingerprint());
    let mut w2 = genesis_sky(&cfg, &sky_params(), 0).unwrap();
    let mut w0 = w0;
    for _ in 0..20 {
        w0.step().unwrap();
        w2.step().unwrap();
    }
    assert_eq!(w0.fingerprint(), w2.fingerprint());
}

#[test]
fn sky_shell_driven_m1_books_exact_every_tick() {
    let cfg = small_budget();
    let mut w = genesis_sky(&cfg, &sky_params(), 6).unwrap();
    w.attach_net(trimmed());
    let mut tot = StepReport::default();
    let mut best = 0u32;
    for _ in 0..400 {
        let r = w.step().unwrap();
        tot.uplifted += r.uplifted;
        tot.weathered += r.weathered;
        w.check_books().unwrap();
        assert_eq!(w.energy_drift(), 0);
        assert_eq!(w.lane_drift(), [0; NE]);
        best = best.max(w.land_relief(&cfg.shape).above_datum);
    }
    let lr = w.land_relief(&cfg.shape);
    eprintln!(
        "sky r10 h6 400t: uplifted {} weathered {} above_datum now {} best {best} sigma_h {:.3} mean_h {:.3} max_h {}",
        tot.uplifted,
        tot.weathered,
        lr.above_datum,
        lr.std_h(),
        lr.mean_h(),
        lr.max_h
    );
    assert_eq!(
        w.lane_totals(),
        genesis_p6(&cfg, &sky_params()).unwrap().lane_totals()
    );
}

#[test]
fn sky_shell_classic_books_exact_and_unpadded_classic_unchanged() {
    // Classic + shell is allowed (opt-in) and conserves; the shell never
    // touches an unpadded world, so planet6 goldens are untouched.
    let cfg = small_budget();
    let p = relief_params(WaterCycle::Classic);
    let mut w = genesis_sky(&cfg, &p, 4).unwrap();
    w.attach_net(trimmed());
    for _ in 0..120 {
        w.step().unwrap();
        w.check_books().unwrap();
        assert_eq!(w.energy_drift(), 0);
        assert_eq!(w.lane_drift(), [0; NE]);
    }
}

#[test]
fn core_sphere_is_the_ball_and_splits_core_heat_exactly() {
    // core_r > 0 spreads `core` per tick over every domain cell within r^2 of
    // the (pinned) centre: per tick the total is exactly `core`, and over n
    // ticks every sphere cell has received exactly `core` (rotating remainder).
    let cfg = small_budget();
    for (h, r, core) in [(6u32, 3u32, 40i32), (0, 2, 40), (6, 4, 1000), (6, 1, 7)] {
        let mut w = genesis_sky(&cfg, &sky_params(), h).unwrap();
        let topo = w.topology();
        let c = topo.core.unwrap() as usize;
        let ball = w.core_sphere(&topo, c, r);
        let o = w.cells[c];
        let brute: Vec<usize> = (0..w.cells.len())
            .filter(|&j| {
                let p = w.cells[j];
                let (dx, dy, dz) = (p.x - o.x, p.y - o.y, p.z - o.z);
                dx * dx + dy * dy + dz * dz <= i64::from(r * r)
            })
            .collect();
        assert_eq!(ball, brute, "h{h} r{r}");
        assert!(ball.contains(&c));
        let n = ball.len();
        let ein = EnergyInput {
            sun: 0,
            day: 0,
            core,
            core_r: r,
        };
        let q0 = w.q.clone();
        for t in 0..n as u64 {
            w.tick = 1000 + t;
            assert_eq!(w.core_sphere_input(&topo, c, ein), i64::from(core));
        }
        let got: Vec<i64> = (0..w.cells.len())
            .map(|j| i64::from(w.q[j]) - i64::from(q0[j]))
            .collect();
        for (j, d) in got.iter().enumerate() {
            let want = if ball.binary_search(&j).is_ok() {
                i64::from(core)
            } else {
                0
            };
            assert_eq!(*d, want, "h{h} r{r} cell {j}");
        }
        eprintln!(
            "core sphere h{h} r{r}: {n} cells, core {core}/tick -> {} + {} rotating",
            core as usize / n,
            core as usize % n
        );
    }
}

#[test]
fn core_sphere_books_exact_every_tick_driven_and_classic() {
    let cfg = small_budget();
    for (cycle, h, ticks) in [
        (WaterCycle::Driven, 6u32, 400),
        (WaterCycle::Driven, 0, 200),
        (WaterCycle::Classic, 4, 120),
    ] {
        let mut p = relief_params(cycle);
        p.energy_in = EnergyInput::M1.with_core_r(4);
        let mut w = genesis_sky(&cfg, &p, h).unwrap();
        w.attach_net(trimmed());
        let mut w0 = genesis_sky(&cfg, &relief_params(cycle), h).unwrap();
        w0.attach_net(trimmed());
        let mut inj = (0i64, 0i64);
        for _ in 0..ticks {
            let r = w.step().unwrap();
            let r0 = w0.step().unwrap();
            // the core always lands in full, on top of a non-negative sun share
            assert!(
                r.injected >= i64::from(p.energy_in.core)
                    && r0.injected >= i64::from(p.energy_in.core)
            );
            inj.0 += r.injected;
            inj.1 += r0.injected;
            w.check_books().unwrap();
            assert_eq!(w.energy_drift(), 0);
            assert_eq!(w.lane_drift(), [0; NE]);
        }
        assert_eq!(w.input_q(), inj.0);
        // Classic: same booked input as the single-cell core (rim sun is pure
        // geometry). Driven: the transparent-air sun depends on which cells are
        // AIR, so the two trajectories may book slightly different sun totals.
        if cycle == WaterCycle::Classic {
            assert_eq!(
                inj.0, inj.1,
                "{cycle:?} h{h}: spread core books the same total input"
            );
        }
        assert_ne!(w.fingerprint(), w0.fingerprint(), "core_r is wired");
    }
}

#[test]
fn core_r_zero_is_the_original_m1() {
    assert_eq!(EnergyInput::M1.core_r, 0);
    assert_eq!(EnergyInput::M1.with_core_r(0), EnergyInput::M1);
    assert_eq!(EnergyInput::OFF.core_r, 0);
}

// ---- Radial frame for uplift / plume root selection ----

/// ROCK ball of radius `r` about the origin with `sky` AIR cells over every
/// column top, core pinned at the origin. Rock Q is purely radial:
/// `3000 - 30 * d^2`; air Q 30.
fn radial_ball(r: i64, sky: i64, cycle: WaterCycle) -> ChemWorld {
    radial_ball_with(r, sky, cycle, |_, m| m)
}

/// [`radial_ball`] with `carve(coord, mat)` overriding each cell's material.
fn radial_ball_with(
    r: i64,
    sky: i64,
    cycle: WaterCycle,
    carve: impl Fn(Coord, MatId) -> MatId,
) -> ChemWorld {
    let mut cells = Vec::new();
    for x in -r..=r {
        for z in -r..=r {
            let rho2 = x * x + z * z;
            if rho2 > r * r {
                continue;
            }
            let top = (r * r - rho2).isqrt();
            for y in -top..=top + sky {
                let d2 = rho2 + y * y;
                let c = if y <= top {
                    (MatId::ROCK, (3000 - 30 * d2) as i32)
                } else {
                    (MatId::AIR, 30)
                };
                let at = Coord::new(x, y, z);
                cells.push((at, carve(at, c.0), c.1));
            }
        }
    }
    let mut w = ChemWorld::from_cells(
        cells,
        [0; NE],
        Arc::new(MatTable::base()),
        relief_params(cycle),
    )
    .unwrap();
    w.core_pin = Some(Coord::new(0, 0, 0));
    w.topo = None;
    w
}

#[test]
fn radial_peer_matches_radius_and_heated_from_below_reads_outward() {
    let mut w = radial_ball(6, 4, WaterCycle::Driven);
    let topo = w.topology();
    let o = Coord::new(0, 0, 0);
    assert_eq!(topo.core.map(|i| w.cells[i as usize]), Some(o));
    let at = |w: &ChemWorld, x, y, z| w.index_of(Coord::new(x, y, z)).unwrap();
    let d2 = |w: &ChemWorld, i: usize| {
        let c = w.cells[i];
        c.x * c.x + c.y * c.y + c.z * c.z
    };
    // same-y neighbour one step out is farther; the peer drops to the same radius
    let hot = at(&w, 2, 3, 0);
    let p = w.radial_peer(&topo, o, hot, at(&w, 3, 3, 0)).unwrap();
    assert_eq!(w.cells[p], Coord::new(3, 2, 0));
    assert_eq!(d2(&w, p), d2(&w, hot));
    // and climbs when the cold column is more central
    let hot = at(&w, 4, 2, 0);
    let p = w.radial_peer(&topo, o, hot, at(&w, 2, 2, 0)).unwrap();
    assert_eq!(w.cells[p], Coord::new(2, 4, 0));
    // every peer is the nearest-radius cell of its column (brute force)
    for (hx, hy, hz) in [(1, 4, 1), (3, 3, -2), (-4, 1, 2), (0, 5, 0), (2, -3, 1)] {
        let hot = at(&w, hx, hy, hz);
        for (dx, dz) in [(1, 0), (-1, 0), (0, 2), (3, 0)] {
            let Some(col) = w.index_of(Coord::new(hx + dx, hy, hz + dz)) else {
                continue;
            };
            let best = (-12..=12)
                .filter_map(|y| w.index_of(Coord::new(hx + dx, y, hz + dz)))
                .filter(|&i| {
                    (w.cells[i].y >= 0) == (hy >= 0)
                        && (w.cells[i].y - hy).abs() <= crate::chem::p6_rest::RADIAL_PEER_SPAN
                })
                .map(|i| ((d2(&w, i) - d2(&w, hot)).abs(), w.cells[i].y))
                .min();
            if let Some(p) = w.radial_peer(&topo, o, hot, col) {
                assert_eq!(
                    Some((d2(&w, p) - d2(&w, hot)).abs()),
                    best.map(|b| b.0),
                    "{hx},{hy},{hz} +{dx},{dz}"
                )
            }
        }
    }
    // the far rim column cannot reach a deep root's radius
    let hot = at(&w, 0, 1, 0);
    assert_eq!(w.radial_peer(&topo, o, hot, at(&w, 5, 1, 0)), None);
    // heated from below: radial field falls outward everywhere under cover
    for (x, y, z) in [(0, 4, 0), (3, 2, 0), (-4, 1, 2), (1, -3, 1), (0, 0, 0)] {
        assert!(
            w.heated_from_below(&topo, o, at(&w, x, y, z)),
            "{x},{y},{z}"
        );
    }
    // near a diagonal the outward cell is the diagonal one: a flank root
    // whose dominant-axis step (5,1,3) is buried rock but whose true outward
    // cell (5,1,4) is outside the ball has no radial overburden
    assert!(w.index_of(Coord::new(5, 1, 3)).is_some());
    assert!(w.index_of(Coord::new(5, 1, 4)).is_none());
    assert!(!w.heated_from_below(&topo, o, at(&w, 4, 1, 3)));
    // a root under a hotter outward cell (sky-warmed) is not; the outward
    // cell of (4,2,0) is the diagonal (5,3,0)
    let c = at(&w, 4, 2, 0);
    let out = at(&w, 5, 3, 0);
    w.q[out] = w.q[c] + 1;
    assert!(!w.heated_from_below(&topo, o, c));
    // a flank root with space or air outward has no radial overburden
    assert!(!w.heated_from_below(&topo, o, at(&w, 6, 0, 0)));
    assert!(!w.heated_from_below(&topo, o, at(&w, 0, 6, 0)));
}

#[test]
fn outward_step_is_the_nearest_lattice_direction() {
    use crate::chem::p6_rest::outward_step;
    assert_eq!(outward_step((0, 0, 0)), (0, 0, 0));
    // on an axis and off-axis by less than ~26.6 degrees: the single step
    assert_eq!(outward_step((5, 0, 0)), (1, 0, 0));
    assert_eq!(outward_step((0, -7, 3)), (0, -1, 0));
    assert_eq!(outward_step((-9, 2, 4)), (-1, 0, 0));
    // within ~26.6 degrees of a face or body diagonal: the diagonal
    assert_eq!(outward_step((4, 0, 2)), (1, 0, 1));
    assert_eq!(outward_step((6, -3, -5)), (1, -1, -1));
    assert_eq!(outward_step((-3, 3, 3)), (-1, 1, 1));
    // always a step outward: the core distance never shrinks
    for x in -9i64..=9 {
        for y in -9i64..=9 {
            for z in -9i64..=9 {
                let (sx, sy, sz) = outward_step((x, y, z));
                let d2 = x * x + y * y + z * z;
                let e2 = (x + sx).pow(2) + (y + sy).pow(2) + (z + sz).pow(2);
                assert!(d2 == 0 || e2 > d2, "{x},{y},{z}");
                // the dominant axis is always part of the step
                let m = x.abs().max(y.abs()).max(z.abs());
                if m > 0 {
                    assert!(
                        (x.abs() == m && sx == x.signum())
                            || (y.abs() == m && sy == y.signum())
                            || (z.abs() == m && sz == z.signum())
                    );
                }
            }
        }
    }
}

/// Mean top height (relative to genesis) over columns with rho^2 <= `in2`,
/// and the number of those columns whose top is at the sky ceiling.
fn polar_rise(w: &ChemWorld, r: i64, sky: i64, in2: i64) -> (f64, usize) {
    let t = tops(w);
    let (mut sum, mut n, mut cap) = (0i64, 0i64, 0usize);
    for (&(x, z), &y) in &t {
        let rho2 = x * x + z * z;
        if rho2 > in2 {
            continue;
        }
        let h = y - (r * r - rho2).isqrt();
        sum += h;
        n += 1;
        cap += usize::from(h >= sky);
    }
    (sum as f64 / n.max(1) as f64, cap)
}

#[test]
fn radial_heat_alone_builds_no_polar_dome() {
    // A purely radial heat field (hotter inward, no lateral contrast) gives
    // the radial-frame laws nothing to push on: at a common world y the
    // central roots read hotter, but at a common radius they do not, so the
    // pole does not stack up into a dome against the sky ceiling.
    let (r, sky) = (8i64, 6i64);
    let mut w = radial_ball(r, sky, WaterCycle::Driven);
    let mut p = *w.params();
    p.heat = false; // freeze the radial field: no diffusion, no leak
    w.set_params(p);
    let (mut up, mut pl) = (0u32, 0u32);
    for _ in 0..3000 {
        let rep = w.step().unwrap();
        up += rep.uplifted;
        pl += rep.plumed;
        w.check_books().unwrap();
        assert_eq!(w.energy_drift(), 0);
        assert!(w.lane_drift().iter().all(|&x| x == 0));
    }
    let (pole, cap) = polar_rise(&w, r, sky, 9);
    let (all, _) = polar_rise(&w, r, sky, r * r);
    eprintln!(
        "radial ball 3000t: uplifted {up} plumed {pl} polar mean h {pole:.2} (cap {cap}) all mean h {all:.2}"
    );
    assert_eq!(cap, 0, "no polar column at the ceiling");
    assert!(pole <= 1.0, "polar mean rise {pole}");
}

// ---- Driven void collapse (unsupported solids fall toward the core) ----

/// [`radial_ball`] with only gravity-type laws left: no heat, no weathering,
/// no core input (so no uplift / plume / degas), books checked every tick.
fn collapse_ball(cycle: WaterCycle, carve: &[((i64, i64, i64), MatId)]) -> ChemWorld {
    let mut w = radial_ball_with(6, 4, cycle, |c, m| {
        carve
            .iter()
            .find(|(p, _)| Coord::new(p.0, p.1, p.2) == c)
            .map_or(m, |&(_, n)| n)
    });
    let mut p = *w.params();
    p.heat = false;
    p.erode = false;
    p.energy_in = EnergyInput::OFF;
    w.set_params(p);
    w
}

fn run_collapse(w: &mut ChemWorld, ticks: u32) -> u32 {
    let mut n = 0;
    for _ in 0..ticks {
        let r = w.step().unwrap();
        n += r.collapsed;
        assert!(r.collapsed == 0 || w.params().cycle == WaterCycle::Driven);
        w.check_books().unwrap();
        assert_eq!(w.energy_drift(), 0);
        assert!(w.lane_drift().iter().all(|&x| x == 0));
    }
    n
}

#[test]
fn inward_support_is_the_inward_step_plus_its_axes() {
    use crate::chem::ChemWorld as W;
    let o = Coord::new(0, 0, 0);
    let (s, k) = W::inward_support(o, Coord::new(0, 0, 0));
    assert_eq!(k, 0, "the core never falls: {s:?}");
    let (s, k) = W::inward_support(o, Coord::new(1, 5, 0));
    assert_eq!(&s[..k], &[(0, -1, 0)]);
    let (s, k) = W::inward_support(o, Coord::new(4, 4, 0));
    assert_eq!(&s[..k], &[(-1, -1, 0), (-1, 0, 0), (0, -1, 0)]);
    let (s, k) = W::inward_support(o, Coord::new(-3, 3, 3));
    assert_eq!(&s[..k], &[(1, -1, -1), (1, 0, 0), (0, -1, 0), (0, 0, -1)]);
}

#[test]
fn driven_buried_void_collapses_to_the_surface_and_compacts_the_column() {
    // a dry pore 3 cells under the pole top (0,6,0)
    let mut w = collapse_ball(WaterCycle::Driven, &[((0, 3, 0), MatId::AIR)]);
    let at = |w: &ChemWorld, x, y, z| w.index_of(Coord::new(x, y, z)).unwrap();
    let rock0 = w.domain().filter(|(_, m, _)| *m == MatId::ROCK).count();
    let q0: Vec<i32> = w.q.clone();
    let n = run_collapse(&mut w, 2000);
    // the pore climbed (0,4,0) -> (0,5,0) -> (0,6,0): three one-cell drops
    assert_eq!(n, 3);
    for y in 0..=5 {
        assert_eq!(w.mat[at(&w, 0, y, 0)], MatId::ROCK, "y={y}");
    }
    assert_eq!(
        w.mat[at(&w, 0, 6, 0)],
        MatId::AIR,
        "the column compacted by the pore's volume"
    );
    assert_eq!(tops(&w)[&(0, 0)], 5);
    assert_eq!(
        w.domain().filter(|(_, m, _)| *m == MatId::ROCK).count(),
        rock0
    );
    assert_eq!(w.q, q0, "heat stays with the cells (pure material swap)");
}

#[test]
fn void_collapse_spares_held_cells_wet_pores_the_underside_and_classic() {
    let at = |w: &ChemWorld, x, y, z| w.index_of(Coord::new(x, y, z)).unwrap();
    // diagonal staircase: (3,3,0) has its inward diagonal (2,2,0) empty but
    // rests on its axis treads (2,3,0) / (3,2,0); a pore under the south
    // pole has its inward step pointing up (+y), against world gravity, so
    // nothing falls into either
    let mut w = collapse_ball(WaterCycle::Driven, &[((2, 2, 0), MatId::AIR)]);
    let before = w.mat.clone();
    assert_eq!(run_collapse(&mut w, 2000), 0);
    assert_eq!(w.mat, before);
    // under world -y gravity (Gravity::Down) the south-pole pore's inward
    // step points up, so it is spared; under §5w coreward gravity (the
    // Driven default) the underside compacts toward the core like the top
    for (g, fell) in [(Gravity::Down, false), (Gravity::Core, true)] {
        let mut w = collapse_ball(WaterCycle::Driven, &[((0, -3, 0), MatId::AIR)]);
        let mut p = *w.params();
        p.gravity = g;
        w.set_params(p);
        let before = w.mat.clone();
        let n = run_collapse(&mut w, 2000);
        assert_eq!(n > 0, fell, "{g:?}: {n}");
        if !fell {
            assert_eq!(w.mat, before);
        }
    }
    // a water-filled pore under the pole holds the column for as long as it
    // stays wet (it may drain later; only then may the column settle)
    let mut w = collapse_ball(WaterCycle::Driven, &[((0, 3, 0), MatId::WATER)]);
    let wet = at(&w, 0, 3, 0);
    let mut held = 0;
    while w.mat[wet] == MatId::WATER && held < 2000 {
        assert_eq!(
            run_collapse(&mut w, 1),
            0,
            "collapsed into a wet pore at tick {held}"
        );
        held += 1;
    }
    assert!(held > 0);
    // Classic never calls the pass: a dry pore under the pole stays put
    let mut c = collapse_ball(WaterCycle::Classic, &[((0, 3, 0), MatId::AIR)]);
    let before = c.mat.clone();
    assert_eq!(run_collapse(&mut c, 500), 0);
    assert_eq!(c.mat, before);
}

#[test]
fn void_collapse_drops_an_overhang() {
    // a rock ledge cell hanging over open AIR on the upper flank falls one
    // cell toward the core and then rests on the rock below
    // pole column top is (0,6,0); hang a ledge at (0,8,0) over AIR (0,7,0)
    let mut w = collapse_ball(WaterCycle::Driven, &[((0, 8, 0), MatId::ROCK)]);
    let at = |w: &ChemWorld, x, y, z| w.index_of(Coord::new(x, y, z)).unwrap();
    assert_eq!(run_collapse(&mut w, 2000), 1);
    assert_eq!(w.mat[at(&w, 0, 7, 0)], MatId::ROCK);
    assert_eq!(w.mat[at(&w, 0, 8, 0)], MatId::AIR);
}

// ---- Transparent-air sun: sunlight crosses AIR and heats the first non-AIR cell ----

/// Energy pass only (every reaction, phase, heat and surface law off), with
/// a sun of 16 and no core heat.
fn sun_only(cycle: WaterCycle, day: u32) -> P6Params {
    P6Params {
        cycle,
        energy_in: EnergyInput {
            sun: 16,
            day,
            core: 0,
            core_r: 0,
        },
        ..chem_only()
    }
}

/// One 1x1 column at (0, y, 0) for `y` in `0..mats.len()`, every cell at Q=100.
fn sun_column(cycle: WaterCycle, mats: &[MatId]) -> ChemWorld {
    let cells = mats
        .iter()
        .enumerate()
        .map(|(y, &m)| (Coord::new(0, y as i64, 0), m, 100))
        .collect();
    let mut w = ChemWorld::from_cells(
        cells,
        [0; NE],
        Arc::new(MatTable::base()),
        sun_only(cycle, 0),
    )
    .unwrap();
    w.pin_t0();
    w
}

fn q_at(w: &ChemWorld, c: Coord) -> i32 {
    w.domain()
        .find(|(at, _, _)| *at == c)
        .map(|(_, _, q)| q)
        .unwrap()
}

#[test]
fn sun_passes_through_air_and_heats_the_rock_below() {
    use MatId as M;
    let col = [
        M::ROCK,
        M::ROCK,
        M::ROCK,
        M::AIR,
        M::AIR,
        M::AIR,
        M::AIR,
        M::AIR,
    ];
    let mut w = sun_column(WaterCycle::Driven, &col);
    let r = w.step().unwrap();
    // zenith sun: only the +y face of the top AIR cell is lit; all 16 land on y=2
    assert_eq!(r.injected, 16);
    assert_eq!(w.input_q(), 16);
    assert_eq!(q_at(&w, Coord::new(0, 2, 0)), 116);
    for y in [0, 1, 3, 4, 5, 6, 7] {
        assert_eq!(q_at(&w, Coord::new(0, y, 0)), 100, "y={y}");
    }
    w.check_books().unwrap();
    assert_eq!(w.energy_drift(), 0);
    // Classic keeps the planet6 rim absorber: the top AIR cell takes it
    let mut c = sun_column(WaterCycle::Classic, &col);
    assert_eq!(c.step().unwrap().injected, 16);
    assert_eq!(q_at(&c, Coord::new(0, 7, 0)), 116);
    assert_eq!(q_at(&c, Coord::new(0, 2, 0)), 100);
    c.check_books().unwrap();
}

#[test]
fn sun_heats_a_wet_surface_and_stops_at_ice_and_sediment() {
    use MatId as M;
    for top in [M::WATER, M::ICE, M::SED] {
        let mut w = sun_column(WaterCycle::Driven, &[M::ROCK, top, M::AIR, M::AIR, M::AIR]);
        let r = w.step().unwrap();
        // the ray stops at the top cell; albedo (§5n) sends part of it back out
        assert_eq!(r.injected + r.reflected, 16, "{top:?}");
        assert_eq!(
            q_at(&w, Coord::new(0, 1, 0)),
            100 + r.injected as i32,
            "{top:?}"
        );
        assert_eq!(
            q_at(&w, Coord::new(0, 0, 0)),
            100,
            "{top:?} shades the rock"
        );
        if top == M::SED {
            assert_eq!(r.injected, 16, "sediment has no albedo");
        }
        w.check_books().unwrap();
        assert_eq!(w.energy_drift(), 0);
    }
    // a surface cell that is not AIR absorbs at the rim, exactly as before
    let mut w = sun_column(WaterCycle::Driven, &[M::ROCK, M::ROCK]);
    assert_eq!(w.step().unwrap().injected, 16);
    assert_eq!(q_at(&w, Coord::new(0, 1, 0)), 116);
}

#[test]
fn an_all_air_column_lets_the_light_through_unbooked() {
    use MatId as M;
    let mut w = sun_column(WaterCycle::Driven, &[M::AIR; 6]);
    let e0 = w.energy_total();
    for _ in 0..50 {
        assert_eq!(w.step().unwrap().injected, 0);
        w.check_books().unwrap();
        assert_eq!(w.energy_drift(), 0);
    }
    assert_eq!(w.input_q(), 0);
    assert_eq!(
        w.reflected_q(),
        0,
        "AIR has no albedo: pass-through is not reflection"
    );
    assert_eq!(w.energy_total(), e0);
    assert!(w.domain().all(|(_, _, q)| q == 100));
}

#[test]
fn a_low_sun_crosses_air_sideways_to_a_cliff() {
    use MatId as M;
    // a row along x at y=0: ROCK at x=0, AIR x=1..=4; find a tick with sx > 0, sy > 0
    let p = sun_only(WaterCycle::Driven, 600);
    let t = (1..600u64).find(|&t| {
        let (sx, sy) = p.energy_in.sun_dir(t);
        sx > 0 && sy > 0 && 16 * sx / 1024 > 0 && 16 * sy / 1024 > 0
    });
    let t = t.unwrap();
    let (sx, sy) = p.energy_in.sun_dir(t);
    let (fx, fy) = (16 * sx / 1024, 16 * sy / 1024);
    let cells: Vec<_> = (0..5i64)
        .map(|x| {
            (
                Coord::new(x, 0, 0),
                if x == 0 { M::ROCK } else { M::AIR },
                100,
            )
        })
        .collect();
    let mut w = ChemWorld::from_cells(cells, [0; NE], Arc::new(MatTable::base()), p).unwrap();
    w.pin_t0();
    w.tick = t - 1;
    let r = w.step().unwrap();
    // the +x face of x=4 rides 4 AIR cells to the rock; the rock's own +y face
    // lands on it; the AIR cells' +y rays cross their cell and leave unbooked
    assert_eq!(q_at(&w, Coord::new(0, 0, 0)), 100 + (fx + fy) as i32);
    for x in 1..5 {
        assert_eq!(q_at(&w, Coord::new(x, 0, 0)), 100, "x={x}");
    }
    assert_eq!(r.injected, fx + fy);
    assert_eq!(w.input_q(), fx + fy);
    w.check_books().unwrap();
    assert_eq!(w.energy_drift(), 0);
}

#[test]
fn sky_shell_sun_never_lands_on_air_and_books_every_quantum() {
    let cfg = small_budget();
    let mut p = sky_params();
    p.energy_in = EnergyInput {
        sun: 16,
        day: 0,
        core: 0,
        core_r: 0,
    };
    p = P6Params {
        heat: false,
        phase: false,
        combine: false,
        net_chem: false,
        net_burn: false,
        condense: false,
        gas_rise: false,
        ..p.without_surface_laws()
    };
    let mut w = genesis_sky(&cfg, &p, 6).unwrap();
    let before: Vec<(Coord, MatId, i32)> = w.domain().collect();
    let r = w.step().unwrap();
    let after: Vec<(Coord, MatId, i32)> = w.domain().collect();
    let mut sum = 0i64;
    let mut solid_hits = 0;
    for (b, a) in before.iter().zip(&after) {
        assert_eq!((b.0, b.1), (a.0, a.1));
        let dq = i64::from(a.2 - b.2);
        if b.1 == MatId::AIR {
            assert_eq!(dq, 0, "sun landed on AIR at {:?}", b.0);
        } else if dq > 0 {
            solid_hits += 1;
        }
        sum += dq;
    }
    // the beam reaches the ground, and every injected quantum is in a non-AIR Q
    assert!(r.injected > 0 && solid_hits > 0);
    assert_eq!(sum, r.injected);
    w.check_books().unwrap();
    assert_eq!(w.energy_drift(), 0);
}

#[test]
fn transparent_sun_books_exact_every_tick_under_a_sky_shell_full_driven() {
    let cfg = small_budget();
    let mut w = genesis_sky(&cfg, &sky_params(), 6).unwrap();
    w.attach_net(trimmed());
    let mut inj = 0i64;
    for _ in 0..700 {
        let r = w.step().unwrap();
        inj += r.injected;
        w.check_books().unwrap();
        assert_eq!(w.energy_drift(), 0);
        assert_eq!(w.lane_drift(), [0; NE]);
    }
    assert_eq!(w.input_q(), inj);
    assert!(inj > 0);
}

// ---- Albedo (§5n): ice and water reflect part of the Driven sun ----

/// Zenith sun on `[ROCK, top, AIR, AIR]` for `n` ticks (energy pass only).
/// Returns (absorbed by `top`, reflected); checks books every tick.
fn albedo_column(top: MatId, n: u64) -> (i64, i64) {
    albedo_column_wa(top, n, WaterAlbedo::Fixed)
}

/// [`albedo_column`] under an explicit water-albedo model.
fn albedo_column_wa(top: MatId, n: u64, wa: WaterAlbedo) -> (i64, i64) {
    use MatId as M;
    let mut w = sun_column(WaterCycle::Driven, &[M::ROCK, top, M::AIR, M::AIR]);
    w.params.water_albedo = wa;
    let e0 = w.energy_total();
    let (mut inj, mut refl) = (0i64, 0i64);
    for _ in 0..n {
        let r = w.step().unwrap();
        assert_eq!(
            r.injected + r.reflected,
            16,
            "{top:?}: every quantum is absorbed or reflected"
        );
        assert!(r.reflected >= 0 && r.injected >= 0);
        inj += r.injected;
        refl += r.reflected;
        w.check_books().unwrap();
        assert_eq!(w.energy_drift(), 0);
        assert_eq!(w.lane_drift(), [0; NE]);
    }
    assert_eq!(w.input_q(), inj, "only absorbed light is booked");
    assert_eq!(w.reflected_q(), refl);
    assert_eq!(
        w.energy_total(),
        e0,
        "reflected light never entered the books"
    );
    assert_eq!(i64::from(q_at(&w, Coord::new(0, 1, 0))), 100 + inj);
    assert_eq!(q_at(&w, Coord::new(0, 0, 0)), 100);
    (inj, refl)
}

#[test]
fn albedo_ice_reflects_more_than_water_and_water_more_than_rock() {
    use MatId as M;
    let n = 4096u64;
    let (ice, ice_r) = albedo_column(M::ICE, n);
    let (water, water_r) = albedo_column(M::WATER, n);
    let (rock, rock_r) = albedo_column(M::ROCK, n);
    let (sed, sed_r) = albedo_column(M::SED, n);
    let full = 16 * n as i64;
    assert_eq!((rock, rock_r), (full, 0), "rock absorbs all of it");
    assert_eq!((sed, sed_r), (full, 0), "sediment absorbs all of it");
    assert!(
        ice_r > water_r && water_r > rock_r,
        "ice {ice_r} > water {water_r} > rock {rock_r}"
    );
    assert!(ice < water && water < rock);
    // the dither is unbiased: the mean matches the albedo to ~1%
    let exp = |a: i64| full * (ALBEDO_DEN - a) / ALBEDO_DEN;
    for (got, a) in [(ice, ALBEDO_ICE), (water, ALBEDO_WATER)] {
        let e = exp(a);
        assert!(
            (got - e).abs() * 100 <= e,
            "absorbed {got} vs expected {e} (a={a})"
        );
    }
}

#[test]
fn albedo_share_is_exact_unbiased_and_never_exceeds_the_quantum() {
    use super::p6::absorbed_share;
    for f in 0..=20i64 {
        assert_eq!(
            absorbed_share(f, 0, 3, 7, 1),
            f,
            "zero albedo is the identity"
        );
    }
    for a in [ALBEDO_WATER, ALBEDO_ICE, 1023] {
        for f in 1..=16i64 {
            let mut sum = 0i64;
            let n = 20_000u64;
            for t in 0..n {
                let s = absorbed_share(f, a, t, (t * 31 % 977) as usize, (t % 6) as usize);
                assert!((0..=f).contains(&s));
                assert!(
                    s == (f * (ALBEDO_DEN - a)) / ALBEDO_DEN
                        || s == (f * (ALBEDO_DEN - a)) / ALBEDO_DEN + 1
                );
                sum += s;
            }
            let exp = (f * (ALBEDO_DEN - a)) as f64 * n as f64 / ALBEDO_DEN as f64;
            assert!(
                ((sum as f64) - exp).abs() <= 0.02 * exp + 50.0,
                "f={f} a={a}: {sum} vs {exp}"
            );
        }
    }
    // a dawn quantum of 1 on water is mostly absorbed, not truncated to 0
    let s: i64 = (0..1000u64)
        .map(|t| absorbed_share(1, ALBEDO_WATER, t, 5, 3))
        .sum();
    assert!(s > 750, "{s}");
}

#[test]
fn classic_sun_has_no_albedo() {
    use MatId as M;
    for top in [M::ICE, M::WATER] {
        let mut c = sun_column(WaterCycle::Classic, &[M::ROCK, top]);
        for _ in 0..64 {
            let r = c.step().unwrap();
            assert_eq!((r.injected, r.reflected), (16, 0));
        }
        assert_eq!(c.reflected_q(), 0);
        c.check_books().unwrap();
    }
}

#[test]
fn albedo_full_driven_sky_shell_books_exact_and_reflects_off_water() {
    let cfg = small_budget();
    let mut w = genesis_sky(&cfg, &sky_params(), 6).unwrap();
    w.attach_net(trimmed());
    let (mut inj, mut refl) = (0i64, 0i64);
    for _ in 0..700 {
        let r = w.step().unwrap();
        inj += r.injected;
        refl += r.reflected;
        w.check_books().unwrap();
        assert_eq!(w.energy_drift(), 0);
        assert_eq!(w.lane_drift(), [0; NE]);
    }
    assert_eq!(w.input_q(), inj);
    assert_eq!(w.reflected_q(), refl);
    assert!(refl > 0, "a wet world reflects some sun");
    assert!(refl < inj, "but absorbs most of it");
}

// ---- angle-dependent water albedo (§5p): Fresnel-like, glancing sun reflects more ----

#[test]
fn water_albedo_curve_is_fresnel_like_monotone_and_bounded() {
    for mu in [-50, 0, 1, 300, 512, 1000, 1024, 5000] {
        assert_eq!(
            water_albedo(mu, WaterAlbedo::Fixed),
            ALBEDO_WATER,
            "Fixed ignores the angle"
        );
    }
    let f = WaterAlbedo::FRESNEL;
    assert_eq!(
        water_albedo(ALBEDO_DEN, f),
        i64::from(WATER_ALBEDO_NADIR),
        "normal incidence = nadir"
    );
    assert_eq!(
        water_albedo(0, f),
        i64::from(WATER_ALBEDO_GRAZE),
        "grazing = graze"
    );
    assert_eq!(water_albedo(-7, f), water_albedo(0, f), "mu clamps below");
    assert_eq!(
        water_albedo(4096, f),
        water_albedo(ALBEDO_DEN, f),
        "mu clamps above"
    );
    let mut prev = i64::MAX;
    for mu in 0..=ALBEDO_DEN {
        let a = water_albedo(mu, f);
        assert!((0..=ALBEDO_DEN).contains(&a));
        assert!(a <= prev, "monotone non-increasing in mu (mu={mu})");
        prev = a;
    }
    // high sun absorbs more than the fixed 0.15, glancing sun reflects more
    assert!(
        water_albedo(512, f) < ALBEDO_WATER,
        "30 degrees elevation: {}",
        water_albedo(512, f)
    );
    assert!(
        water_albedo(200, f) > ALBEDO_WATER,
        "~11 degrees: {}",
        water_albedo(200, f)
    );
    // albedo_at swaps the curve in for WATER only
    assert_eq!(albedo_at(MatId::ICE, 0, f), ALBEDO_ICE);
    assert_eq!(albedo_at(MatId::ICE, ALBEDO_DEN, f), ALBEDO_ICE);
    assert_eq!(albedo_at(MatId::ROCK, 0, f), 0);
    assert_eq!(albedo_at(MatId::WATER, 300, f), water_albedo(300, f));
    // a flat curve is the fixed model
    let flat = WaterAlbedo::Fresnel {
        nadir: 154,
        graze: 154,
        pow: 5,
    };
    for mu in [0, 100, 700, 1024] {
        assert_eq!(water_albedo(mu, flat), ALBEDO_WATER);
    }
}

#[test]
fn zenith_water_absorbs_more_under_fresnel_and_ice_is_unchanged() {
    use MatId as M;
    let n = 4096u64;
    let full = 16 * n as i64;
    let (wf, wf_r) = albedo_column_wa(M::WATER, n, WaterAlbedo::FRESNEL);
    let (wx, wx_r) = albedo_column_wa(M::WATER, n, WaterAlbedo::Fixed);
    assert!(
        wf > wx && wf_r < wx_r,
        "zenith water: fresnel {wf} vs fixed {wx}"
    );
    let e = full * (ALBEDO_DEN - i64::from(WATER_ALBEDO_NADIR)) / ALBEDO_DEN;
    assert!(
        (wf - e).abs() * 100 <= e,
        "absorbed {wf} vs nadir expectation {e}"
    );
    assert_eq!(
        albedo_column_wa(M::ICE, n, WaterAlbedo::FRESNEL),
        albedo_column_wa(M::ICE, n, WaterAlbedo::Fixed),
        "ice albedo is angle-free"
    );
    assert_eq!(
        albedo_column_wa(M::ROCK, n, WaterAlbedo::FRESNEL),
        (full, 0)
    );
}

/// A 1x1 `[ROCK, top]` column under a rotating 600-tick sun for one day,
/// books checked every tick. Returns (absorbed, reflected, water_sun_q).
fn day_column(top: MatId, wa: WaterAlbedo) -> (i64, i64, [i64; 4]) {
    let cells = vec![
        (Coord::new(0, 0, 0), MatId::ROCK, 100),
        (Coord::new(0, 1, 0), top, 100),
    ];
    let p = P6Params {
        water_albedo: wa,
        ..sun_only(WaterCycle::Driven, 600)
    };
    let mut w = ChemWorld::from_cells(cells, [0; NE], Arc::new(MatTable::base()), p).unwrap();
    w.pin_t0();
    let e0 = w.energy_total();
    let (mut inj, mut refl) = (0i64, 0i64);
    for _ in 0..600 {
        let r = w.step().unwrap();
        inj += r.injected;
        refl += r.reflected;
        w.check_books().unwrap();
        assert_eq!(w.energy_drift(), 0);
        assert_eq!(w.lane_drift(), [0; NE]);
    }
    assert_eq!(w.input_q(), inj, "only absorbed light is booked");
    assert_eq!(w.reflected_q(), refl);
    assert_eq!(
        w.energy_total(),
        e0,
        "reflected light never entered the books"
    );
    let qsum: i64 = w.domain().map(|(_, _, q)| i64::from(q)).sum();
    assert_eq!(qsum, 200 + inj, "only the absorbed share is in Q");
    (inj, refl, w.water_sun_q())
}

#[test]
fn glancing_sun_reflects_more_off_water_over_a_day_and_never_mints() {
    use MatId as M;
    let (fi, fr, ws) = day_column(M::WATER, WaterAlbedo::FRESNEL);
    let (xi, xr, wx) = day_column(M::WATER, WaterAlbedo::Fixed);
    // the light reaching the water does not depend on its albedo
    assert_eq!(fi + fr, xi + xr);
    // the rock's side faces are lit too (rock albedo 0), so water is a subset
    assert!(
        ws[0] + ws[2] > 0 && ws[0] + ws[2] <= fi + fr,
        "water hits are binned"
    );
    assert_eq!(ws[1] + ws[3], fr);
    assert_eq!(
        (ws[0], ws[2]),
        (wx[0], wx[2]),
        "same incident light per bin"
    );
    assert!(
        ws[0] > 0 && ws[2] > 0,
        "the day has both high and low sun: {ws:?}"
    );
    // effective albedo per bin (x1024): high sun below 0.15, low sun above high sun
    let hi = ws[1] * ALBEDO_DEN / ws[0];
    let lo = ws[3] * ALBEDO_DEN / ws[2];
    assert!(hi < ALBEDO_WATER, "high-sun water albedo {hi}");
    assert!(
        lo > 2 * hi,
        "glancing sun reflects much more: lo {lo} hi {hi}"
    );
    // the fixed model is flat in both bins (within dither noise)
    let (xh, xl) = (wx[1] * ALBEDO_DEN / wx[0], wx[3] * ALBEDO_DEN / wx[2]);
    assert!(
        (xh - ALBEDO_WATER).abs() <= 8 && (xl - ALBEDO_WATER).abs() <= 8,
        "fixed {xh} {xl}"
    );
    // flux-weighted over the day the Fresnel pond reflects less than 0.15
    assert!(fr < xr, "day-mean: fresnel reflects {fr}, fixed {xr}");
    // ice: identical under both models, and never counted as water
    let (ii, ir, iw) = day_column(M::ICE, WaterAlbedo::FRESNEL);
    assert_eq!((ii, ir), {
        let (a, b, _) = day_column(M::ICE, WaterAlbedo::Fixed);
        (a, b)
    });
    assert_eq!(iw, [0; 4]);
}

#[test]
fn classic_ignores_the_water_albedo_model() {
    let run = |wa: WaterAlbedo| {
        let p = P6Params {
            water_albedo: wa,
            energy_in: EnergyInput::M1,
            ..golden_params('E')
        };
        assert_eq!(p.cycle, WaterCycle::Classic);
        let mut w = fixture(p, Some(trimmed()));
        for _ in 0..300 {
            w.step().unwrap();
        }
        w.check_books().unwrap();
        assert_eq!(w.reflected_q(), 0);
        assert_eq!(w.water_sun_q(), [0; 4]);
        (
            w.domain().collect::<Vec<_>>(),
            w.input_q(),
            w.space_q(),
            w.vapor(),
        )
    };
    assert_eq!(run(WaterAlbedo::FRESNEL), run(WaterAlbedo::Fixed));
}

#[test]
fn fresnel_full_driven_sky_shell_books_exact_and_bins_water_light() {
    let cfg = small_budget();
    let mut w = genesis_sky(&cfg, &sky_params(), 6).unwrap();
    assert_eq!(
        w.params().water_albedo,
        WaterAlbedo::FRESNEL,
        "Fresnel is the default"
    );
    w.attach_net(trimmed());
    let (mut inj, mut refl) = (0i64, 0i64);
    for _ in 0..700 {
        let r = w.step().unwrap();
        inj += r.injected;
        refl += r.reflected;
        w.check_books().unwrap();
        assert_eq!(w.energy_drift(), 0);
        assert_eq!(w.lane_drift(), [0; NE]);
    }
    assert_eq!(w.input_q(), inj);
    assert_eq!(w.reflected_q(), refl);
    let ws = w.water_sun_q();
    assert!(ws[0] > 0, "water sees high sun: {ws:?}");
    assert!(
        ws[1] + ws[3] <= refl,
        "water reflection is part of all reflection"
    );
    assert!(0 < refl && refl < inj);
}

// ---- vapor greenhouse (Driven): the vapor pool holds back outbound IR ----

#[test]
fn vapor_trap_is_zero_dry_saturates_and_scales_with_the_column() {
    assert_eq!(vapor_trap(0, 1000), 0);
    assert_eq!(vapor_trap(-5, 1000), 0);
    assert_eq!(vapor_trap(100, 0), 0);
    let mut last = 0;
    for v in [1, 10, 100, 1000, 10_000, 1_000_000] {
        let t = vapor_trap(v, 1000);
        assert!(
            t >= last && t <= GH_VAPOR_MAX,
            "monotone and capped: {v} -> {t}"
        );
        last = t;
    }
    assert!(
        last > GH_VAPOR_MAX * 99 / 100,
        "a huge column nearly saturates"
    );
    // half-saturation at HALF/1024 pairs per rim cell
    let rim = 2048;
    assert_eq!(
        vapor_trap(GH_VAPOR_HALF * rim / GH_VAPOR_DEN, rim),
        GH_VAPOR_MAX / 2
    );
    // same pool over a bigger rim is a thinner column: traps less
    assert!(vapor_trap(500, 4000) < vapor_trap(500, 1000));
}

/// The 24x24 rock plate with heat on (so the rim leaks) and every surface
/// law that could move vapor off; `vapor` pairs pinned in the sky.
fn gh_plate(cycle: WaterCycle, greenhouse: Greenhouse, vapor: i64) -> ChemWorld {
    plate(
        P6Params {
            heat: true,
            rain: false,
            greenhouse,
            ..plate_params(cycle)
        },
        vapor,
    )
}

#[test]
fn vapor_greenhouse_holds_back_leak_without_minting_heat() {
    let rim = 24 * 24 * 2; // every plate cell borders space
    let mut dry = gh_plate(WaterCycle::Driven, Greenhouse::Planet6, 0);
    let col = rim * GH_VAPOR_HALF / GH_VAPOR_DEN; // the half-saturation column
    let mut wet = gh_plate(WaterCycle::Driven, Greenhouse::Planet6, col);
    let (ed, ew) = (dry.energy_total(), wet.energy_total());
    // first tick from identical Q: what the wet sky trapped is exactly what
    // the dry sky leaked extra
    let (rd, rw) = (dry.step().unwrap(), wet.step().unwrap());
    assert_eq!(rd.trapped, 0, "a dry sky traps nothing");
    assert!(rw.trapped > 0);
    assert_eq!(
        rw.leaked + rw.trapped,
        rd.leaked,
        "trapped heat is leak that stayed, not new heat"
    );
    // 40 ticks: the plate has no input, so given long enough both drain to 0
    let (mut ld, mut lw, mut tw) = (rd.leaked, rw.leaked, rw.trapped);
    for _ in 0..40 {
        let (a, b) = (dry.step().unwrap(), wet.step().unwrap());
        ld += a.leaked;
        lw += b.leaked;
        tw += b.trapped;
        assert_books(&dry, ed);
        assert_books(&wet, ew);
    }
    assert_eq!(wet.trapped_q(), tw);
    assert_eq!(dry.trapped_q(), 0);
    assert_eq!(wet.space_q(), lw);
    // ~25% trapped at the half-saturation column (MAX/2 of 1024ths)
    let share = 1024 * (ld - lw) / ld;
    assert!(
        share > 150 && share < 300,
        "wet sky leaked {lw} vs dry {ld} (share {share}/1024)"
    );
    let q = |w: &ChemWorld| w.domain().map(|(_, _, q)| i64::from(q)).sum::<i64>();
    assert_eq!(
        q(&wet) - q(&dry),
        ld - lw,
        "everything not leaked is still in Q"
    );
    assert_eq!(wet.vapor(), col, "vapor is only a column, never spent");
}

#[test]
fn vapor_greenhouse_back_radiates_onto_the_ground_under_open_air() {
    // one rock under three AIR cells: the top AIR cell's held-back IR goes
    // down the column to the rock, not into the air
    use MatId as M;
    let cells = [M::ROCK, M::AIR, M::AIR, M::AIR]
        .iter()
        .enumerate()
        .map(|(y, &m)| (Coord::new(0, y as i64, 0), m, if y == 3 { 5000 } else { 0 }))
        .collect();
    let prm = P6Params {
        heat: true,
        rain: false,
        ..plate_params(WaterCycle::Driven)
    };
    let mut w = ChemWorld::from_cells(cells, [0; NE], Arc::new(MatTable::base()), prm).unwrap();
    w.vapor = 4000; // a thick column over 4 rim cells: near saturation
    w.pin_t0();
    let e0 = w.energy_total();
    let r = w.step().unwrap();
    assert!(r.trapped > 0);
    assert_books(&w, e0);
    // diffusion alone moves nothing to the rock in one tick (it is 3 cells
    // down), so all its Q is back-radiation: the top cell's share. (The y=2
    // cell, warmed by diffusion this tick, has space only at its sides, whose
    // columns are all AIR, so its small share stays put.)
    let rock = i64::from(q_at(&w, Coord::new(0, 0, 0)));
    assert!(
        rock > 0 && rock <= r.trapped && rock * 4 >= r.trapped * 3,
        "rock {rock} trapped {}",
        r.trapped
    );
}

#[test]
fn a_thin_vapor_column_barely_traps() {
    let rim = 24 * 24 * 2;
    let mut dry = gh_plate(WaterCycle::Driven, Greenhouse::Planet6, 0);
    let mut thin = gh_plate(WaterCycle::Driven, Greenhouse::Planet6, rim / 50); // rocky-world column
    let (mut ld, mut lt) = (0, 0);
    for _ in 0..40 {
        ld += dry.step().unwrap().leaked;
        lt += thin.step().unwrap().leaked;
    }
    assert!(lt < ld);
    assert!(
        (ld - lt) * 100 < ld * 2,
        "a 0.02 column holds back < 2%: {lt} vs {ld}"
    );
}

#[test]
fn classic_and_greenhouse_off_have_no_vapor_greenhouse() {
    let rim = 24 * 24 * 2;
    for (cycle, gh) in [
        (WaterCycle::Classic, Greenhouse::Planet6),
        (WaterCycle::Driven, Greenhouse::Off),
    ] {
        let mut dry = gh_plate(cycle, gh, 0);
        let mut wet = gh_plate(cycle, gh, rim);
        for _ in 0..500 {
            let (a, b) = (dry.step().unwrap(), wet.step().unwrap());
            assert_eq!(b.trapped, 0, "{cycle:?}/{gh:?}");
            assert_eq!(
                a.leaked, b.leaked,
                "{cycle:?}/{gh:?}: vapor does not touch the leak"
            );
        }
        assert_eq!(wet.trapped_q(), 0);
    }
}

#[test]
fn greenhouse_full_driven_sky_shell_books_exact_traps_and_albedo_still_reflects() {
    let cfg = small_budget();
    let mut w = genesis_sky(&cfg, &sky_params(), 6).unwrap();
    w.attach_net(trimmed());
    let (mut refl, mut trap, mut leak) = (0i64, 0i64, 0i64);
    for _ in 0..1500 {
        let r = w.step().unwrap();
        refl += r.reflected;
        trap += r.trapped;
        leak += r.leaked;
        w.check_books().unwrap();
        assert_eq!(w.energy_drift(), 0);
        assert_eq!(w.lane_drift(), [0; NE]);
    }
    assert_eq!(w.trapped_q(), trap);
    assert_eq!(w.reflected_q(), refl);
    assert!(refl > 0, "albedo still reflects");
    assert!(
        w.vapor() > 0 && trap > 0,
        "a wet sky traps some IR (vapor {} trap {trap})",
        w.vapor()
    );
    assert!(trap < leak, "but most IR still leaves");
}

// ---- vapor cloud albedo (§5q): the vapor pool reflects sunlight before the surface ----

#[test]
fn cloud_reflect_is_zero_dry_saturates_and_scales_with_the_column() {
    let m = i64::from(CLOUD_MAX);
    assert!(
        m > 0 && m < CLOUD_DEN / 2,
        "the default cap stays well below a white-out"
    );
    assert_eq!(CloudAlbedo::default(), CloudAlbedo::VAPOR);
    assert_eq!(cloud_reflect(0, 1000, CloudAlbedo::VAPOR), 0);
    assert_eq!(cloud_reflect(-5, 1000, CloudAlbedo::VAPOR), 0);
    assert_eq!(cloud_reflect(100, 0, CloudAlbedo::VAPOR), 0);
    assert_eq!(
        cloud_reflect(1_000_000, 10, CloudAlbedo::Off),
        0,
        "Off never reflects"
    );
    let mut last = 0;
    for v in [1, 10, 100, 1000, 10_000, 1_000_000, i64::MAX / 2] {
        let c = cloud_reflect(v, 1000, CloudAlbedo::VAPOR);
        assert!(c >= last && c <= m, "monotone and capped: {v} -> {c}");
        last = c;
    }
    assert!(last > m * 99 / 100, "a huge column nearly saturates");
    // half the cap at HALF/1024 pairs per rim cell (same column unit as the greenhouse)
    let rim = 2048;
    assert_eq!(
        cloud_reflect(
            i64::from(CLOUD_HALF) * rim / CLOUD_DEN,
            rim,
            CloudAlbedo::VAPOR
        ),
        m / 2
    );
    assert!(
        cloud_reflect(500, 4000, CloudAlbedo::VAPOR) < cloud_reflect(500, 1000, CloudAlbedo::VAPOR)
    );
    // explicit models: cap clamps to 1.0, half moves the knee
    let full = CloudAlbedo::Vapor {
        max: 4000,
        half: 1024,
    };
    assert!(cloud_reflect(1_000_000, 1, full) <= CLOUD_DEN);
    let soft = CloudAlbedo::Vapor {
        max: CLOUD_MAX,
        half: CLOUD_HALF * 4,
    };
    assert!(cloud_reflect(2000, 1000, soft) < cloud_reflect(2000, 1000, CloudAlbedo::VAPOR));
}

/// Zenith sun on `[ROCK, top, AIR, AIR]` (rim = 4 cells) with `vapor` pairs
/// pinned in the sky under `prm`, `n` ticks. Returns (absorbed, reflected,
/// cloud-reflected, water_sun_q); books checked every tick and every quantum
/// accounted for.
fn cloud_column(top: MatId, vapor: i64, n: u64, prm: P6Params) -> (i64, i64, i64, [i64; 4]) {
    use MatId as M;
    let cells = [M::ROCK, top, M::AIR, M::AIR]
        .iter()
        .enumerate()
        .map(|(y, &m)| (Coord::new(0, y as i64, 0), m, 100))
        .collect();
    let mut w = ChemWorld::from_cells(cells, [0; NE], Arc::new(MatTable::base()), prm).unwrap();
    w.vapor = vapor;
    w.pin_t0();
    let e0 = w.energy_total();
    let (mut inj, mut refl) = (0i64, 0i64);
    for _ in 0..n {
        let r = w.step().unwrap();
        assert_eq!(
            r.injected + r.reflected,
            16,
            "{top:?}: every quantum is absorbed or reflected"
        );
        inj += r.injected;
        refl += r.reflected;
        w.check_books().unwrap();
        assert_eq!(w.energy_drift(), 0);
        assert_eq!(w.lane_drift(), [0; NE]);
    }
    assert_eq!(w.vapor(), vapor, "clouds never spend vapor");
    assert_eq!(w.input_q(), inj, "only absorbed light is booked");
    assert_eq!(w.reflected_q(), refl);
    assert!(w.cloud_q() <= refl, "cloud light is part of all reflection");
    assert_eq!(
        w.energy_total(),
        e0,
        "cloud-reflected light never entered the books"
    );
    assert_eq!(i64::from(q_at(&w, Coord::new(0, 1, 0))), 100 + inj);
    (inj, refl, w.cloud_q(), w.water_sun_q())
}

#[test]
fn vapor_clouds_reflect_sun_before_the_surface_and_never_mint() {
    use MatId as M;
    let n = 4096u64;
    let p = sun_only(WaterCycle::Driven, 0);
    assert_eq!(p.cloud, CloudAlbedo::VAPOR, "clouds on by default");
    // half-saturation column over the 4 rim cells: cloud = MAX/2
    let half = i64::from(CLOUD_HALF) * 4 / CLOUD_DEN;
    let c = cloud_reflect(half, 4, CloudAlbedo::VAPOR);
    assert_eq!(c, i64::from(CLOUD_MAX) / 2);
    // rock has no surface albedo: all reflection is cloud, ~c/1024 of the light
    let (ri, rr, rc, _) = cloud_column(M::ROCK, half, n, p);
    assert_eq!(rr, rc, "rock reflects nothing itself");
    let want = 16 * n as i64 * c / CLOUD_DEN;
    assert!(
        (rc - want).abs() * 100 < want * 2,
        "cloud reflected {rc} want ~{want}"
    );
    assert_eq!(ri + rr, 16 * n as i64);
    // a dry sky is the §5p stack bit-for-bit
    let (di, dr, dc, _) = cloud_column(M::ROCK, 0, n, p);
    assert_eq!((di, dr, dc), (16 * n as i64, 0, 0));
    // more vapor -> more cloud -> less absorbed
    let (ti, _, tc, _) = cloud_column(M::ROCK, half * 8, n, p);
    assert!(tc > rc && ti < ri, "thick {tc}/{ti} vs half {rc}/{ri}");
    // water: clouds first, then Fresnel on what is left; water_sun counts the
    // light under the clouds
    let (wi, wr, wc, ws) = cloud_column(M::WATER, half, n, p);
    let (xi, xr, xc, xs) = cloud_column(M::WATER, 0, n, p);
    assert_eq!(xc, 0);
    assert!(
        wc > 0 && wr > wc && wr > xr && wi < xi,
        "wet {wi}/{wr}/{wc} dry {xi}/{xr}"
    );
    assert_eq!(
        ws[0] + ws[2],
        16 * n as i64 - wc,
        "water sees only the light under the clouds"
    );
    assert_eq!(xs[0] + xs[2], 16 * n as i64);
    assert_eq!(
        wr - wc,
        ws[1] + ws[3],
        "the rest of the reflection is the water's own"
    );
}

#[test]
fn cloud_albedo_off_classic_and_greenhouse_off_leave_the_sun_unchanged() {
    use MatId as M;
    let n = 1200u64;
    let wet = i64::from(CLOUD_HALF) * 4 / CLOUD_DEN * 4;
    let base = sun_only(WaterCycle::Driven, 0);
    let dry = cloud_column(M::WATER, 0, n, base);
    for p in [
        P6Params {
            cloud: CloudAlbedo::Off,
            ..base
        },
        P6Params {
            greenhouse: Greenhouse::Off,
            ..base
        },
        P6Params {
            cloud: CloudAlbedo::Vapor {
                max: 0,
                half: CLOUD_HALF,
            },
            ..base
        },
    ] {
        let r = cloud_column(M::WATER, wet, n, p);
        assert_eq!(r, dry, "{:?}/{:?}: no cloud stage", p.cloud, p.greenhouse);
    }
    // Classic has no albedo at all: the rim absorber takes every quantum
    let mut c = sun_column(WaterCycle::Classic, &[M::ROCK, M::WATER, M::AIR, M::AIR]);
    c.vapor = wet;
    c.pin_t0();
    for _ in 0..n {
        let r = c.step().unwrap();
        assert_eq!(r.reflected, 0);
        c.check_books().unwrap();
    }
    assert_eq!(c.cloud_q(), 0);
    assert_eq!(c.cloud_last(), 0);
}

#[test]
fn cloud_albedo_full_driven_sky_shell_books_exact_and_clouds_reflect() {
    let cfg = small_budget();
    let run = |cloud: CloudAlbedo| {
        let mut w = genesis_sky(&cfg, &sky_params(), 6).unwrap();
        w.attach_net(trimmed());
        let mut p = *w.params();
        p.cloud = cloud;
        w.set_params(p);
        let (mut inj, mut refl) = (0i64, 0i64);
        for _ in 0..1500 {
            let r = w.step().unwrap();
            inj += r.injected;
            refl += r.reflected;
            w.check_books().unwrap();
            assert_eq!(w.energy_drift(), 0);
            assert_eq!(w.lane_drift(), [0; NE]);
        }
        assert_eq!(w.input_q(), inj);
        assert_eq!(w.reflected_q(), refl);
        (inj, refl, w.cloud_q(), w.vapor())
    };
    let (_, refl, cq, vap) = run(CloudAlbedo::VAPOR);
    assert!(
        vap > 0 && cq > 0,
        "a wet sky reflects some sun off its clouds (vapor {vap} cloud {cq})"
    );
    assert!(cq < refl, "surface albedo still reflects too");
    let (_, _, oq, _) = run(CloudAlbedo::Off);
    assert_eq!(oq, 0, "Off: no cloud light");
}

// ---- local vapor (§5r): regional vapor bins drive clouds and the greenhouse ----

#[test]
fn vapor_patch_is_a_cube_sphere_split_of_directions() {
    assert_eq!(VaporLocality::default(), VaporLocality::PATCHES);
    assert_eq!(VaporLocality::Global.patch_count(), 1);
    assert_eq!(
        VaporLocality::PATCHES.patch_count(),
        6 * usize::from(VAPOR_PATCH_N).pow(2)
    );
    assert_eq!(vapor_patch((0, 0, 0), 3), 0);
    // the six axis directions land on six different faces, centre tile
    let n = 3u8;
    let faces: Vec<usize> = [
        (-5, 0, 0),
        (5, 0, 0),
        (0, -5, 0),
        (0, 5, 0),
        (0, 0, -5),
        (0, 0, 5),
    ]
    .iter()
    .map(|&d| vapor_patch(d, n))
    .collect();
    for (f, &p) in faces.iter().enumerate() {
        assert_eq!(p, f * 9 + 4, "axis {f} -> centre tile of face {f}");
    }
    // every region of a ball shell is used, ids stay in range
    let mut seen = vec![0u32; 54];
    for x in -12i64..=12 {
        for y in -12i64..=12 {
            for z in -12i64..=12 {
                let r2 = x * x + y * y + z * z;
                if (100..=144).contains(&r2) {
                    seen[vapor_patch((x, y, z), n)] += 1;
                }
            }
        }
    }
    assert!(
        seen.iter().all(|c| *c > 0),
        "all 54 regions populated: {seen:?}"
    );
}

fn local_sky_world(loc: VaporLocality) -> ChemWorld {
    let cfg = small_budget();
    let mut w = genesis_sky(&cfg, &sky_params(), 6).unwrap();
    w.attach_net(trimmed());
    let mut p = *w.params();
    p.vapor_local = loc;
    w.set_params(p);
    w
}

#[test]
fn local_vapor_bins_sum_to_the_pool_and_books_stay_exact() {
    let mut w = local_sky_world(VaporLocality::PATCHES);
    assert_eq!(w.params().cycle, WaterCycle::Driven);
    let mut max_live = 0usize;
    for _ in 0..1500 {
        w.step().unwrap();
        let b = w.vapor_bins();
        assert_eq!(b.len(), VaporLocality::PATCHES.patch_count());
        assert_eq!(
            b.iter().sum::<i64>(),
            w.vapor(),
            "bins are an attribution of the booked pool"
        );
        assert!(b.iter().all(|v| *v >= 0));
        max_live = max_live.max(b.iter().filter(|v| **v > 0).count());
        w.check_books().unwrap();
        assert_eq!(w.energy_drift(), 0);
        assert_eq!(w.lane_drift(), [0; NE]);
    }
    assert!(
        w.vapor() > 0 && max_live > 1,
        "vapor spreads over several regions ({max_live})"
    );
    let cs = w.cloud_split();
    assert_eq!(
        cs[1] + cs[3],
        w.cloud_q(),
        "the split covers every cloud quantum"
    );
    let d = w.vapor_bin_diag();
    assert_eq!(d.iter().map(|r| r[2]).sum::<i64>(), w.cloud_q());
    assert_eq!(d.iter().map(|r| r[4]).sum::<i64>(), w.trapped_q());
    assert_eq!(d.iter().map(|r| r[0]).sum::<i64>(), cs[0] + cs[2]);
    assert!(w.vapor_bin_rim().iter().sum::<i64>() > 0);
}

#[test]
fn global_locality_is_one_bin_and_rescales_on_switch() {
    let mut w = local_sky_world(VaporLocality::Global);
    for _ in 0..600 {
        w.step().unwrap();
        assert_eq!(w.vapor_bins(), &[w.vapor()][..]);
        w.check_books().unwrap();
    }
    // switching to regions re-spreads the same pool by rim, books unchanged
    let v = w.vapor();
    let mut p = *w.params();
    p.vapor_local = VaporLocality::PATCHES;
    w.set_params(p);
    let e = w.energy_total();
    let lanes = w.lane_totals();
    let topo = w.topology();
    w.vapor_sync(&topo);
    assert_eq!(w.vapor_bins().iter().sum::<i64>(), v);
    assert_eq!(w.energy_total(), e);
    assert_eq!(w.lane_totals(), lanes);
    // a pool written directly (tests, loaders) is re-attributed too
    w.vapor += 7;
    w.vapor_sync(&topo);
    assert_eq!(w.vapor_bins().iter().sum::<i64>(), v + 7);
    w.vapor -= 7;
    w.pin_t0();
    for _ in 0..300 {
        w.step().unwrap();
        w.check_books().unwrap();
        assert_eq!(w.vapor_bins().iter().sum::<i64>(), w.vapor());
    }
}

#[test]
fn local_clouds_and_greenhouse_sit_only_over_the_region_holding_vapor() {
    let mut w = local_sky_world(VaporLocality::PATCHES);
    for _ in 0..50 {
        w.step().unwrap();
    }
    // freeze the water cycle so the bins stay put, then load one region
    let mut p = *w.params();
    p.rain = false;
    w.set_params(p);
    // the most sunlit region (by incident sun so far)
    let d0: Vec<[i64; 5]> = w.vapor_bin_diag().to_vec();
    let hot = (0..d0.len())
        .max_by_key(|&k| (d0[k][0], std::cmp::Reverse(k)))
        .unwrap();
    let rim = w.vapor_bin_rim()[hot];
    assert!(d0[hot][0] > 0 && rim > 0);
    let load = rim * i64::from(CLOUD_HALF) / CLOUD_DEN * 4;
    let total = w.vapor() + load;
    let mut bins = vec![0i64; d0.len()];
    bins[hot] = total;
    w.vapor = total;
    w.vapor_bins = bins.clone();
    w.pin_t0();
    for _ in 0..600 {
        w.step().unwrap();
        w.check_books().unwrap();
        assert_eq!(w.energy_drift(), 0);
        assert_eq!(w.lane_drift(), [0; NE]);
    }
    assert_eq!(
        w.vapor_bins(),
        &bins[..],
        "no rain, no evaporation: the bins never move without the cycle"
    );
    let d1 = w.vapor_bin_diag();
    for k in 0..d1.len() {
        let cloud = d1[k][2] - d0[k][2];
        let held = d1[k][4] - d0[k][4];
        if k == hot {
            assert!(cloud > 0, "clouds over the wet region");
        } else {
            assert_eq!(
                (cloud, held),
                (0, 0),
                "region {k} has no vapor: clear sky, no trap"
            );
        }
    }
    assert!(
        d1[hot][4] - d0[hot][4] > 0,
        "the wet region traps outbound IR"
    );
}

#[test]
fn vapor_mix_conserves_and_relaxes_toward_equal_column_density() {
    let mut w = local_sky_world(VaporLocality::Patches { n: 3, mix_tau: 64 });
    w.step().unwrap();
    let pc = w.vapor_bins().len();
    let total = 50_000i64;
    w.vapor = total;
    let mut bins = vec![0i64; pc];
    bins[0] = total;
    w.vapor_bins = bins;
    for t in 0..4000u64 {
        w.tick = 10 + t;
        w.vapor_mix_pass();
        assert_eq!(w.vapor_bins().iter().sum::<i64>(), total);
        assert!(w.vapor_bins().iter().all(|v| *v >= 0));
    }
    let rim = w.vapor_bin_rim().to_vec();
    let rt: i64 = rim.iter().sum();
    for (k, &v) in w.vapor_bins().iter().enumerate() {
        if rim[k] == 0 {
            continue;
        }
        let want = total * rim[k] / rt;
        assert!(
            (v - want).abs() * 10 <= want.max(10),
            "region {k}: {v} vs {want}"
        );
    }
    // Global and mix_tau 0 never move a pair
    let mut g = local_sky_world(VaporLocality::Patches { n: 3, mix_tau: 0 });
    g.step().unwrap();
    let before = g.vapor_bins().to_vec();
    g.vapor_mix_pass();
    assert_eq!(g.vapor_bins(), &before[..]);
}

#[test]
fn classic_never_touches_the_local_vapor_bins() {
    use MatId as M;
    let mut c = sun_column(WaterCycle::Classic, &[M::ROCK, M::WATER, M::AIR, M::AIR]);
    c.vapor = 40;
    c.pin_t0();
    for _ in 0..500 {
        c.step().unwrap();
        c.check_books().unwrap();
    }
    assert!(c.vapor_bins().is_empty());
    assert_eq!(c.cloud_split(), [0; 4]);
}

#[test]
fn local_rain_spills_from_regions_without_landing_ground() {
    // no transport: a region's vapor leaves only as its own rain, so the
    // underside regions (no sky-open ground under the -y fall test) must
    // spill their drops onto the planet's landing line instead of waiting
    // (§5r regression: the world -y fall test; §5s defaults to RainFall::Core)
    let mut w = local_sky_world(VaporLocality::Patches { n: 3, mix_tau: 0 });
    let mut g = local_sky_world(VaporLocality::Global);
    for x in [&mut w, &mut g] {
        let mut p = *x.params();
        p.rain_fall = RainFall::Down;
        x.set_params(p);
    }
    for _ in 0..2000 {
        w.step().unwrap();
        g.step().unwrap();
        assert_eq!(w.vapor_bins().iter().sum::<i64>(), w.vapor());
        w.check_books().unwrap();
        assert_eq!(w.energy_drift(), 0);
        assert_eq!(w.lane_drift(), [0; NE]);
    }
    assert_eq!(g.rain_spilled(), 0, "one region never spills");
    assert!(w.rain_spilled() > 0, "underside rain spills");
    assert!(w.rain_spilled() <= w.rained());
    // the -y face (regions 18..27) cannot hold its vapor forever
    let under: i64 = w.vapor_bins()[18..27].iter().sum();
    eprintln!(
        "spilled {} of {} rained; vapor {} (under {}) vs global {}",
        w.rain_spilled(),
        w.rained(),
        w.vapor(),
        under,
        g.vapor()
    );
    assert!(w.vapor() < 2 * g.vapor() + 64, "no stranded pile-up");
}

/// Per region: (landing spots under the -y test, under the core test,
/// waterline AIR cells), from the live world.
fn rain_ground(w: &mut ChemWorld) -> Vec<(usize, usize, usize)> {
    let topo = w.topology();
    w.vapor_sync(&topo);
    let o = w.cells[topo.core.unwrap() as usize];
    let mut out = vec![(0, 0, 0); w.vapor_bins().len()];
    for i in 0..w.cells.len() {
        if w.mat[i] != MatId::AIR {
            continue;
        }
        let b = w.vapor_bin_of(i);
        let air_or_space = |d| {
            topo.offset(&w.cells, i, d)
                .is_none_or(|u| w.mat[u] == MatId::AIR)
        };
        let solid = |d| {
            topo.offset(&w.cells, i, d)
                .is_some_and(|u| w.mat[u] != MatId::AIR)
        };
        if air_or_space((0, 1, 0)) && solid((0, -1, 0)) {
            out[b].0 += 1;
        }
        let p = w.cells[i];
        let s = super::p6_rest::outward_step((p.x - o.x, p.y - o.y, p.z - o.z));
        let (set, k) = ChemWorld::inward_support(o, p);
        if k > 0 && air_or_space(s) && set[..k].iter().any(|d| solid(*d)) {
            out[b].1 += 1;
        }
        if topo
            .nb6(&w.cells, i)
            .into_iter()
            .flatten()
            .any(|j| w.mat[j] == MatId::WATER || w.mat[j] == MatId::ICE)
        {
            out[b].2 += 1;
        }
    }
    out
}

#[test]
fn core_rain_finds_more_landing_ground_below_the_equator_than_down_rain() {
    // the -y test only lands on ground that faces world-up; on the lower
    // hemisphere that is pockets only, while the core test lands on any
    // ground under an outward AIR step
    let mut w = local_sky_world(VaporLocality::Patches { n: 3, mix_tau: 0 });
    for _ in 0..300 {
        w.step().unwrap();
    }
    let topo = w.topology();
    let o = w.cells[topo.core.unwrap() as usize];
    let gr = rain_ground(&mut w);
    // regions whose cells mostly sit below the core
    let pc = gr.len();
    let (mut below, mut all) = (vec![0usize; pc], vec![0usize; pc]);
    for i in 0..w.cells.len() {
        let b = w.vapor_bin_of(i);
        all[b] += 1;
        if w.cells[i].y < o.y {
            below[b] += 1;
        }
    }
    let lower: Vec<usize> = (0..pc).filter(|&r| below[r] * 2 > all[r]).collect();
    let down: usize = lower.iter().map(|&r| gr[r].0).sum();
    let core: usize = lower.iter().map(|&r| gr[r].1).sum();
    eprintln!(
        "{} lower regions: landing spots down {down} core {core}",
        lower.len()
    );
    assert!(lower.len() >= 9);
    assert!(core > 2 * down, "core {core} vs down {down}");
}

#[test]
fn nearest_ground_walks_region_hops() {
    let mut w = local_sky_world(VaporLocality::PATCHES);
    w.step().unwrap();
    let pc = w.vapor_bins().len();
    assert_eq!(pc, 54);
    let mut has = vec![false; pc];
    assert_eq!(
        w.nearest_ground(5, &has),
        (Vec::new(), 0),
        "no ground anywhere: empty (spill)"
    );
    has[5] = true;
    assert_eq!(w.nearest_ground(5, &has), (vec![5], 0));
    // every region reaches region 5; hop counts are symmetric and bounded
    let mut far = 0;
    for b in 0..pc {
        let (near, h) = w.nearest_ground(b, &has);
        assert_eq!(near, vec![5]);
        let mut back = vec![false; pc];
        back[b] = true;
        assert_eq!(w.nearest_ground(5, &back).1, h);
        far = far.max(h);
    }
    assert!(
        (3..=8).contains(&far),
        "graph diameter from region 5: {far}"
    );
    // ties come back sorted, all at the same hop count
    let mut only0 = vec![false; pc];
    only0[0] = true;
    let nb: Vec<usize> = (1..pc)
        .filter(|&r| w.nearest_ground(r, &only0).1 == 1)
        .collect();
    let mut two = vec![false; pc];
    assert!(nb.len() >= 2, "region 0 neighbours {nb:?}");
    for &r in &nb {
        two[r] = true;
    }
    assert_eq!(w.nearest_ground(0, &two), (nb.clone(), 1));
}

#[test]
fn core_rain_keeps_books_and_never_spills_with_transport() {
    // the default stack (n=3, tau 2048, RainFall::Core): exact books, no
    // planet-wide spill, and less stranded vapor than the -y test without
    // a spill would leave
    let mut w = local_sky_world(VaporLocality::PATCHES);
    assert_eq!(w.params().rain_fall, RainFall::Core);
    for _ in 0..2000 {
        w.step().unwrap();
        w.check_books().unwrap();
        assert_eq!(w.energy_drift(), 0);
        assert_eq!(w.lane_drift(), [0; NE]);
        assert_eq!(w.vapor_bins().iter().sum::<i64>(), w.vapor());
    }
    let (shed, hops) = w.rain_shed();
    eprintln!(
        "rained {} shed {shed} (mean hops {:.2}) spilled {} vapor {}",
        w.rained(),
        hops as f64 / shed.max(1) as f64,
        w.rain_spilled(),
        w.vapor()
    );
    assert_eq!(w.rain_spilled(), 0);
    assert!(w.rained() > 0 && shed < w.rained());
    // one region: the core fall never sheds or spills
    let mut g = local_sky_world(VaporLocality::Global);
    for _ in 0..600 {
        g.step().unwrap();
        g.check_books().unwrap();
    }
    assert_eq!((g.rain_spilled(), g.rain_shed()), (0, (0, 0)));
    assert!(g.rained() > 0);
}

/// (Up-open, Core-open) WATER cell counts split by hemisphere relative to the
/// core: `[upper, lower]`, plus Up-open cells whose outward step is WATER
/// (the +y test evaporating the buried end of a radial column).
fn evap_tops(w: &mut ChemWorld) -> ([usize; 2], [usize; 2], usize) {
    let topo = w.topology();
    let o = w.cells[topo.core.unwrap() as usize];
    let mut p = *w.params();
    let (mut up, mut core, mut buried_up) = ([0; 2], [0; 2], 0);
    for i in 0..w.cells.len() {
        if w.mat[i] != MatId::WATER {
            continue;
        }
        let h = usize::from(w.cells[i].y < o.y);
        p.evap_top = EvapTop::Up;
        w.set_params(p);
        let u = w.evap_open(&topo, w.evap_frame(&topo), i);
        p.evap_top = EvapTop::Core;
        w.set_params(p);
        let fr = w.evap_frame(&topo);
        assert_eq!(fr, Some(o));
        let c = w.evap_open(&topo, fr, i);
        let c0 = w.cells[i];
        let s = super::p6_rest::outward_step((c0.x - o.x, c0.y - o.y, c0.z - o.z));
        let out = topo.offset(&w.cells, i, s).map(|j| w.mat[j]);
        // Core-open is exactly "outward step is space or not WATER"
        assert_eq!(
            c,
            s != (0, 0, 0) && out.is_none_or(|m| m != MatId::WATER),
            "cell {c0:?}"
        );
        if s == (0, 1, 0) {
            assert_eq!(c, u, "straight up: Core and Up are the same test");
        }
        up[h] += usize::from(u);
        core[h] += usize::from(c);
        buried_up += usize::from(u && out == Some(MatId::WATER));
    }
    (up, core, buried_up)
}

#[test]
fn core_evap_opens_the_outward_face_of_flank_and_underside_water() {
    // §5t: evaporation uses rain's core-relative top; the +y test lets the
    // buried end of a flank/underside column evaporate and skips its outer end
    let mut w = local_sky_world(VaporLocality::PATCHES);
    assert_eq!(w.params().evap_top, EvapTop::Core);
    assert_eq!(w.params().rain_fall, RainFall::Core);
    let mut tot = ([0usize; 2], [0usize; 2], 0usize);
    for t in 0..1200 {
        w.step().unwrap();
        if t % 100 == 99 {
            let (u, c, b) = evap_tops(&mut w);
            for h in 0..2 {
                tot.0[h] += u[h];
                tot.1[h] += c[h];
            }
            tot.2 += b;
        }
    }
    eprintln!(
        "evap tops summed over 12 samples: up {:?} core {:?} (upper, lower); up-open with outward WATER {}",
        tot.0, tot.1, tot.2
    );
    assert!(
        tot.1[1] > 0,
        "the lower hemisphere has open water faces under the core frame"
    );
    assert!(
        tot.2 > 0,
        "the +y test evaporates buried column ends somewhere"
    );
    w.check_books().unwrap();
}

#[test]
fn evap_top_up_and_core_both_keep_exact_books_and_diverge() {
    let run = |e: EvapTop| {
        let mut w = local_sky_world(VaporLocality::PATCHES);
        let mut p = *w.params();
        p.evap_top = e;
        w.set_params(p);
        for _ in 0..1500 {
            w.step().unwrap();
            w.check_books().unwrap();
            assert_eq!(w.energy_drift(), 0);
            assert_eq!(w.lane_drift(), [0; NE]);
            assert_eq!(w.vapor_bins().iter().sum::<i64>(), w.vapor());
        }
        assert_eq!(w.rain_spilled(), 0);
        (w.evaporated(), w.rained(), w.mat.clone())
    };
    let (eu, ru, mu) = run(EvapTop::Up);
    let (ec, rc, mc) = run(EvapTop::Core);
    let (ea, ra, ma) = run(EvapTop::CoreAir);
    eprintln!(
        "1500 ticks: up evap {eu} rained {ru}; core evap {ec} rained {rc}; core-air evap {ea} rained {ra}"
    );
    assert!(eu > 0 && ec > 0 && ea > 0 && ru > 0 && rc > 0 && ra > 0);
    assert_ne!(mu, mc, "the evaporation frame changes the world");
    assert_ne!(mc, ma, "the strict AIR test changes the world");
    assert!(ea <= ec, "AIR-only opens a subset of not-WATER tops");
}

// ---- §5u full radial sky shell (flanks and underside get headroom too) ----

#[test]
fn radial_sky_shell_keeps_t0_books_cells_and_covers_every_direction() {
    let cfg = small_budget();
    let mut w0 = genesis_p6(&cfg, &sky_params()).unwrap();
    let mut w1 = genesis_sky_radial(&cfg, &sky_params(), 4).unwrap();
    let wp = genesis_sky(&cfg, &sky_params(), 4).unwrap();
    assert!(w1.sky_radial() && w1.outward_guard() && !wp.sky_radial() && !wp.outward_guard());
    assert_eq!(w1.sky_height(), 4);
    assert_eq!(w1.energy_total(), w0.energy_total());
    assert_eq!(w1.t0_energy(), w0.t0_energy());
    assert_eq!(w1.lane_totals(), w0.lane_totals());
    assert_eq!(w1.check_books(), Ok(()));
    let old: std::collections::HashMap<Coord, (MatId, i32)> =
        w0.domain().map(|(c, m, q)| (c, (m, q))).collect();
    let o = cfg.shape.center;
    let (r, ro) = (cfg.shape.radius as i64, cfg.shape.radius as i64 + 4);
    let mut added = 0usize;
    for (c, m, q) in w1.domain() {
        if let Some(&v) = old.get(&c) {
            assert_eq!(v, (m, q));
            continue;
        }
        added += 1;
        assert_eq!((m, q), (MatId::AIR, 0));
        let d2 = (c.x - o.x).pow(2) + (c.y - o.y).pow(2) + (c.z - o.z).pow(2);
        assert!(d2 > r * r && d2 <= ro * ro, "{c:?}");
    }
    // exactly the lattice shell R^2 < d^2 <= (R+H)^2 (genesis fills the ball)
    let shell = (-ro..=ro)
        .flat_map(|x| (-ro..=ro).flat_map(move |y| (-ro..=ro).map(move |z| x * x + y * y + z * z)))
        .filter(|&d2| d2 > r * r && d2 <= ro * ro)
        .count();
    assert_eq!(added, shell);
    assert_eq!(w1.domain_len(), w0.domain_len() + shell);
    // the polar shell is a subset
    let radial: std::collections::HashSet<Coord> = w1.domain().map(|(c, _, _)| c).collect();
    assert!(wp.domain().all(|(c, _, _)| radial.contains(&c)));
    // every ball cell's outward step lands in the domain (no space face on the
    // ball: the flanks and underside all have air headroom)
    for (c, _, _) in w0.domain() {
        let s = super::p6_rest::outward_step((c.x - o.x, c.y - o.y, c.z - o.z));
        if s == (0, 0, 0) {
            continue;
        }
        for k in 1..=4 {
            let at = Coord::new(c.x + s.0 * k, c.y + s.1 * k, c.z + s.2 * k);
            let d2 = (at.x - o.x).pow(2) + (at.y - o.y).pow(2) + (at.z - o.z).pow(2);
            if d2 <= ro * ro {
                assert!(radial.contains(&at), "{c:?} step {k}");
            }
        }
    }
    // core stays at the pre-shell centre; relief probe agrees with the ball
    let k0 = w0.topology().core.unwrap() as usize;
    let k1 = w1.topology().core.unwrap() as usize;
    assert_eq!(w0.cells[k0], w1.cells[k1]);
    assert_eq!(
        w0.land_relief(&cfg.shape).sum_h,
        w1.land_relief(&cfg.shape).sum_h
    );
    eprintln!(
        "radial r10 h4: ball {} + shell {shell} (polar h4 adds {})",
        w0.domain_len(),
        wp.domain_len() - w0.domain_len()
    );
}

#[test]
fn radial_sky_zero_is_a_noop() {
    let cfg = small_budget();
    let mut w0 = genesis_p6(&cfg, &sky_params()).unwrap();
    let mut w1 = genesis_sky_radial(&cfg, &sky_params(), 0).unwrap();
    assert!(!w1.sky_radial() && !w1.outward_guard());
    assert_eq!(w0.fingerprint(), w1.fingerprint());
    for _ in 0..20 {
        w0.step().unwrap();
        w1.step().unwrap();
    }
    assert_eq!(w0.fingerprint(), w1.fingerprint());
}

#[test]
fn radial_guard_blocks_moves_into_radial_only_cells_only() {
    let cfg = small_budget(); // R = 10 about the origin
    let mut w = genesis_sky_radial(&cfg, &sky_params(), 4).unwrap();
    let ix = |w: &ChemWorld, x: i64, y: i64, z: i64| w.index_of(Coord::new(x, y, z)).unwrap();
    // radial-only cells: below the ball, beside the equator, under the rim
    for (x, y, z) in [
        (0, -11, 0),
        (0, -14, 0),
        (11, 0, 0),
        (10, -1, 0),
        (10, -5, 0),
        (0, 0, -12),
        (12, 6, 0),
    ] {
        assert!(w.shell_blocked(ix(&w, x, y, z)), "({x},{y},{z})");
    }
    // genesis ball and polar columns: never blocked (§5t behaviour)
    for (x, y, z) in [
        (0, -10, 0),
        (3, -4, 0),
        (10, 0, 0),
        (0, 11, 0),
        (0, 14, 0),
        (4, 12, 0),
        (10, 4, 0),
    ] {
        assert!(!w.shell_blocked(ix(&w, x, y, z)), "({x},{y},{z})");
    }
    w.set_outward_guard(false);
    assert!(!w.outward_guard());
    assert!(!w.shell_blocked(ix(&w, 0, -11, 0)));
    let p = genesis_sky(&cfg, &sky_params(), 4).unwrap();
    assert!(!p.outward_guard());
    assert!((0..p.domain_len()).all(|t| !p.shell_blocked(t)));
}

/// WATER cells farther than `R + 1.5` from the centre (dripped out into the
/// shell), and WATER below the centre.
fn shell_water(w: &ChemWorld, cfg: &GenesisConfig) -> (usize, usize) {
    let o = cfg.shape.center;
    let lim = (cfg.shape.radius as f64 + 1.5).powi(2);
    let far = w
        .domain()
        .filter(|(c, m, _)| {
            *m == MatId::WATER
                && (((c.x - o.x).pow(2) + (c.y - o.y).pow(2) + (c.z - o.z).pow(2)) as f64) > lim
        })
        .count();
    let low = w
        .domain()
        .filter(|(c, m, _)| *m == MatId::WATER && c.y < o.y)
        .count();
    (far, low)
}

#[test]
fn radial_sky_driven_m1_books_exact_and_guard_holds_the_water() {
    let cfg = small_budget();
    let run = |guard: bool| {
        let mut w = genesis_sky_radial(&cfg, &sky_params(), 6).unwrap();
        w.set_outward_guard(guard);
        w.attach_net(trimmed());
        for _ in 0..800 {
            w.step().unwrap();
            w.check_books().unwrap();
            assert_eq!(w.energy_drift(), 0);
            assert_eq!(w.lane_drift(), [0; NE]);
            assert_eq!(w.vapor_bins().iter().sum::<i64>(), w.vapor());
        }
        assert_eq!(
            w.lane_totals(),
            genesis_p6(&cfg, &sky_params()).unwrap().lane_totals()
        );
        assert_eq!(w.rain_spilled(), 0);
        let (far, low) = shell_water(&w, &cfg);
        let water = w.domain().filter(|(_, m, _)| *m == MatId::WATER).count();
        eprintln!(
            "radial h6 guard={guard} 800t: water {water} far(>R+1.5) {far} below-centre {low} rained {} shed {:?}",
            w.rained(),
            w.rain_shed()
        );
        (far, low, water)
    };
    let (far_on, _, water_on) = run(true);
    let (far_off, low_off, water_off) = run(false);
    // guarded: flow never enters radial-only cells; only rain lands (and
    // stacks) there, so most water still hugs the ball
    assert!(
        far_on * 2 < water_on,
        "guarded water hugs the ball: {far_on}/{water_on}"
    );
    // free: -y flow drains the whole ocean down through the shell
    assert!(
        far_off * 10 >= water_off * 9 && low_off * 10 >= water_off * 9,
        "unguarded water drips out ({far_off}/{water_off})"
    );
}

#[test]
fn radial_guard_holds_equator_water_and_free_flow_drips() {
    let cfg = small_budget();
    for guard in [true, false] {
        let mut w = genesis_sky_radial(&cfg, &sky_params(), 4).unwrap();
        w.set_outward_guard(guard);
        let topo = w.topology();
        let at = |x, y, z| w.index_of(Coord::new(x, y, z)).unwrap();
        let (i, below, inner) = (at(10, 0, 0), at(10, -1, 0), at(9, 0, 0));
        w.mat[i] = MatId::WATER;
        w.mat[inner] = MatId::ROCK;
        assert_eq!(w.mat[below], MatId::AIR);
        let mut rep = StepReport::default();
        w.flow_pass(&topo, &mut rep);
        if guard {
            assert_eq!(
                w.mat[i],
                MatId::WATER,
                "the equator cell has only radial-only air around it"
            );
        } else {
            assert_eq!(
                (w.mat[i], w.mat[below]),
                (MatId::AIR, MatId::WATER),
                "free flow falls into the shell"
            );
        }
    }
}

#[test]
fn radial_sky_sheds_less_rain_than_the_polar_shell() {
    let cfg = small_budget();
    let run = |radial: bool| {
        let mut w = if radial {
            genesis_sky_radial(&cfg, &sky_params(), 6)
        } else {
            genesis_sky(&cfg, &sky_params(), 6)
        }
        .unwrap();
        w.attach_net(trimmed());
        for _ in 0..2000 {
            w.step().unwrap();
        }
        w.check_books().unwrap();
        assert_eq!(w.energy_drift(), 0);
        assert_eq!(w.rain_spilled(), 0);
        (w.rained(), w.rain_shed().0)
    };
    let (rp, sp) = run(false);
    let (rr, sr) = run(true);
    eprintln!(
        "2000t h6: polar rained {rp} shed {sp} ({:.1}%); radial rained {rr} shed {sr} ({:.1}%)",
        100.0 * sp as f64 / rp.max(1) as f64,
        100.0 * sr as f64 / rr.max(1) as f64
    );
    assert!(rp > 0 && rr > 0);
    assert!(
        sr * rp * 4 <= sp * rr,
        "radial shed fraction at most a quarter of polar"
    );
}

// ---- §5w coreward gravity (flow / settle / scour / gas toward the core) ----

/// [`sky_params`] with §5w coreward gravity (the Driven default).
fn core_sky_params() -> P6Params {
    P6Params {
        gravity: Gravity::Core,
        ..sky_params()
    }
}

#[test]
fn gravity_core_is_the_driven_default_and_auto_drops_the_guard() {
    assert_eq!(P6Params::planet6_defaults().gravity, Gravity::Core);
    let cfg = small_budget();
    let mut w = genesis_sky_radial(&cfg, &core_sky_params(), 4).unwrap();
    assert!(
        w.coreward() && w.sky_radial() && !w.outward_guard(),
        "auto: no guard under coreward flow"
    );
    w.set_outward_guard(true);
    assert!(w.outward_guard(), "forced on for the A/B");
    let mut p = *w.params();
    p.cycle = WaterCycle::Classic;
    w.set_params(p);
    assert!(!w.coreward(), "Classic is always world -y");
    let d = genesis_sky_radial(&cfg, &sky_params(), 4).unwrap();
    assert!(
        !d.coreward() && d.outward_guard(),
        "Gravity::Down keeps the §5u auto guard"
    );
}

#[test]
fn coreward_flow_falls_toward_the_core_and_down_flow_falls_to_minus_y() {
    let cfg = small_budget(); // R = 10 about the origin
    for g in [Gravity::Core, Gravity::Down] {
        let mut w = genesis_sky_radial(
            &cfg,
            &P6Params {
                gravity: g,
                ..sky_params()
            },
            4,
        )
        .unwrap();
        w.set_outward_guard(false);
        let at = |w: &ChemWorld, x, y, z| w.index_of(Coord::new(x, y, z)).unwrap();
        // equator: water at (10,0,0) over an inner AIR pocket (9,0,0)
        // south pole: water at (0,-10,0) with an AIR pocket above it, (0,-9,0)
        let (e, ei, eb) = (at(&w, 10, 0, 0), at(&w, 9, 0, 0), at(&w, 10, -1, 0));
        let (s, si, sb) = (at(&w, 0, -10, 0), at(&w, 0, -9, 0), at(&w, 0, -11, 0));
        for (c, m) in [
            (e, MatId::WATER),
            (ei, MatId::AIR),
            (s, MatId::WATER),
            (si, MatId::AIR),
        ] {
            w.mat[c] = m;
        }
        assert_eq!((w.mat[eb], w.mat[sb]), (MatId::AIR, MatId::AIR));
        let topo = w.topology();
        let mut rep = StepReport::default();
        w.flow_pass(&topo, &mut rep);
        match g {
            Gravity::Core => {
                assert_eq!(
                    (w.mat[ei], w.mat[e]),
                    (MatId::WATER, MatId::AIR),
                    "equator water falls inward"
                );
                assert_eq!(
                    (w.mat[si], w.mat[s]),
                    (MatId::WATER, MatId::AIR),
                    "south-pole water falls up (toward the core)"
                );
                assert_eq!((w.mat[eb], w.mat[sb]), (MatId::AIR, MatId::AIR));
            }
            Gravity::Down => {
                assert_eq!(
                    (w.mat[eb], w.mat[e]),
                    (MatId::WATER, MatId::AIR),
                    "equator water falls -y"
                );
                assert_eq!(
                    (w.mat[sb], w.mat[s]),
                    (MatId::WATER, MatId::AIR),
                    "south-pole water drips off the ball"
                );
            }
        }
        assert!(rep.flowed >= 2);
    }
}

#[test]
fn coreward_side_flow_never_climbs_and_corners_are_not_squeezed() {
    let cfg = small_budget();
    let mut w = genesis_sky_radial(&cfg, &core_sky_params(), 4).unwrap();
    let at = |w: &ChemWorld, x, y, z| w.index_of(Coord::new(x, y, z)).unwrap();
    // a water cell on the 45-degree flank (8,8,0) whose inward diagonal
    // (7,7,0) is AIR but both axis treads (7,8,0)/(8,7,0) are ROCK: it may
    // not squeeze through; the outer neighbours all climb, so it stays
    let i = at(&w, 8, 8, 0);
    let ring = [
        (7, 7, 0, MatId::AIR),
        (7, 8, 0, MatId::ROCK),
        (8, 7, 0, MatId::ROCK),
        (8, 8, 1, MatId::ROCK),
        (8, 8, -1, MatId::ROCK),
    ];
    for (x, y, z, m) in ring {
        let c = at(&w, x, y, z);
        w.mat[c] = m;
    }
    w.mat[i] = MatId::WATER;
    let (o9, n9) = (at(&w, 9, 8, 0), at(&w, 8, 9, 0));
    w.mat[o9] = MatId::AIR;
    w.mat[n9] = MatId::AIR;
    let topo = w.topology();
    let mut rep = StepReport::default();
    w.flow_pass(&topo, &mut rep);
    assert_eq!(
        w.mat[i],
        MatId::WATER,
        "held: closed corner, every open neighbour is higher"
    );
    assert_eq!(rep.flowed, 0);
}

#[test]
fn coreward_radial_shell_holds_the_ocean_without_the_guard() {
    let cfg = small_budget();
    let run = |g: Gravity, guard: Option<bool>| {
        let mut w = genesis_sky_radial(
            &cfg,
            &P6Params {
                gravity: g,
                ..sky_params()
            },
            6,
        )
        .unwrap();
        if let Some(b) = guard {
            w.set_outward_guard(b);
        }
        w.attach_net(trimmed());
        for _ in 0..800 {
            w.step().unwrap();
            w.check_books().unwrap();
            assert_eq!(w.energy_drift(), 0);
            assert_eq!(w.lane_drift(), [0; NE]);
            assert_eq!(w.vapor_bins().iter().sum::<i64>(), w.vapor());
        }
        assert_eq!(
            w.lane_totals(),
            genesis_p6(&cfg, &sky_params()).unwrap().lane_totals()
        );
        assert_eq!(w.rain_spilled(), 0);
        let (far, low) = shell_water(&w, &cfg);
        let water = w.domain().filter(|(_, m, _)| *m == MatId::WATER).count();
        eprintln!(
            "§5w {g:?} guard={:?} 800t: water {water} far {far} below-centre {low} rained {} shed {:?}",
            w.outward_guard(),
            w.rained(),
            w.rain_shed()
        );
        (far, low, water)
    };
    let (far_c, low_c, water_c) = run(Gravity::Core, None);
    let (far_g, _, water_g) = run(Gravity::Core, Some(true));
    let (far_d, low_d, water_d) = run(Gravity::Down, Some(false));
    assert!(
        far_c * 4 < water_c,
        "coreward, no guard: the ocean hugs the ball ({far_c}/{water_c})"
    );
    assert!(
        low_c * 10 < water_c * 8,
        "coreward: water is not pooled under the core ({low_c}/{water_c})"
    );
    assert!(
        far_g * 4 < water_g,
        "guard on top changes little ({far_g}/{water_g})"
    );
    assert!(
        far_d * 10 >= water_d * 9 && low_d * 10 >= water_d * 9,
        "-y without guard still drains ({far_d}/{water_d})"
    );
}

#[test]
fn classic_ignores_gravity() {
    let cfg = small_budget();
    let mk = |g| {
        let p = P6Params {
            gravity: g,
            cycle: WaterCycle::Classic,
            ..sky_params()
        };
        genesis_sky_radial(&cfg, &p, 4).unwrap()
    };
    let (mut a, mut b) = (mk(Gravity::Core), mk(Gravity::Down));
    for _ in 0..200 {
        a.step().unwrap();
        b.step().unwrap();
    }
    assert_eq!(a.fingerprint(), b.fingerprint());
}

// ---- §5v full-sphere sun (SunSweep) ----

#[test]
fn sun_sweep_upper_is_the_legacy_sun_and_full_never_sets() {
    let e = EnergyInput::M1;
    for t in 0..1200u64 {
        let (sx, sy) = e.sun_dir(t);
        assert_eq!(e.sun_dir3(t, SunSweep::Upper), (sx, sy, 0, sy > 0), "t={t}");
        assert_eq!(e.sun_dir3(t, SunSweep::FULL), (sx, sy, 0, true), "t={t}");
        // tilt with year 0 is the untilted full sun
        assert_eq!(
            e.sun_dir3(t, SunSweep::Full { tilt: 400, year: 0 }),
            (sx, sy, 0, true)
        );
    }
    assert_eq!(P6Params::default().sun_sweep, SunSweep::Upper);
    assert_eq!(SunSweep::Upper.track_mask(), 0b00_1011);
    assert_eq!(SunSweep::FULL.track_mask(), 0b00_1111);
    assert_eq!(
        SunSweep::Full { tilt: 410, year: 8 }.track_mask(),
        0b11_1111
    );
}

#[test]
fn full_sun_gives_every_track_face_a_noon_and_tilt_keeps_l1() {
    let e = EnergyInput::M1;
    // per NB6 face (-x, +x, -y, +y, -z, +z): max of d.s over the period
    let best = |sweep: SunSweep, ticks: u64| {
        let mut m = [i64::MIN; 6];
        for t in 0..ticks {
            let (sx, sy, sz, lit) = e.sun_dir3(t, sweep);
            if !lit {
                continue;
            }
            for (k, d) in [-sx, sx, -sy, sy, -sz, sz].into_iter().enumerate() {
                m[k] = m[k].max(d);
            }
        }
        m
    };
    let up = best(SunSweep::Upper, 600);
    assert!(up[2] <= 0, "Upper never lights a -y face: {up:?}");
    let full = best(SunSweep::FULL, 600);
    assert!(
        full[..4].iter().all(|&d| d >= 1000),
        "every x-y face gets noon: {full:?}"
    );
    assert_eq!((full[4], full[5]), (0, 0), "equinox: z caps unlit");
    let tilt = SunSweep::Full { tilt: 410, year: 8 };
    let tl = best(tilt, 600 * 8);
    assert!(
        tl.iter().all(|&d| d > 0),
        "tilted year lights all six faces: {tl:?}"
    );
    assert!(tl[4] >= 400 && tl[5] >= 400, "{tl:?}");
    for t in 0..600 * 8 {
        let (sx, sy, sz, _) = e.sun_dir3(t, tilt);
        let l1 = sx.abs() + sy.abs() + sz.abs();
        assert!((1022..=1024).contains(&l1), "t={t} |s|_1={l1}");
    }
}

#[test]
fn full_sun_lights_the_underside_face_and_books_it() {
    // one ROCK cell with all six faces on space; a tick with sy < 0
    let base = sun_only(WaterCycle::Driven, 600);
    let t = (1..600u64)
        .find(|&t| {
            let (sx, sy) = base.energy_in.sun_dir(t);
            sy < 0 && 16 * -sy / 1024 > 0 && 16 * sx.abs() / 1024 > 0
        })
        .unwrap();
    let (sx, sy) = base.energy_in.sun_dir(t);
    for sweep in [SunSweep::Upper, SunSweep::FULL] {
        let p = P6Params {
            sun_sweep: sweep,
            ..base
        };
        let mut w = ChemWorld::from_cells(
            vec![(Coord::new(0, 0, 0), MatId::ROCK, 100)],
            [0; NE],
            Arc::new(MatTable::base()),
            p,
        )
        .unwrap();
        w.pin_t0();
        w.tick = t - 1;
        let r = w.step().unwrap();
        let want = if sweep == SunSweep::Upper {
            0
        } else {
            16 * -sy / 1024 + 16 * sx.abs() / 1024
        };
        assert_eq!(r.injected, want, "{sweep:?}");
        assert_eq!(w.input_q(), want);
        assert_eq!(q_at(&w, Coord::new(0, 0, 0)), 100 + want as i32);
        assert_eq!(r.reflected, 0, "ROCK has no albedo");
        w.check_books().unwrap();
        assert_eq!(w.energy_drift(), 0);
    }
}

#[test]
fn full_sun_radial_sky_books_exact_and_lights_the_lower_hemisphere() {
    let cfg = small_budget();
    let run = |sweep: SunSweep| {
        let p = P6Params {
            sun_sweep: sweep,
            ..sky_params()
        };
        let mut w = genesis_sky_radial(&cfg, &p, 6).unwrap();
        w.attach_net(trimmed());
        let mut lower_q = 0i64;
        for _ in 0..1200 {
            w.step().unwrap();
            w.check_books().unwrap();
            assert_eq!(w.energy_drift(), 0);
            assert_eq!(w.lane_drift(), [0; NE]);
        }
        let cy = cfg.shape.center.y;
        for (c, m, q) in w.domain() {
            if c.y < cy - 3 && m != MatId::AIR {
                lower_q += i64::from(q);
            }
        }
        assert_eq!(w.lane_totals(), genesis_p6(&cfg, &p).unwrap().lane_totals());
        let h = w.hemi_diag();
        assert_eq!(h[3], w.rained(), "every Driven drop is split by hemisphere");
        assert!(h[1] <= w.evaporated() && h[0] <= h[1] && h[2] <= h[3]);
        eprintln!(
            "§5v {sweep:?} 1200t: input_q {} reflected {} lower_q {lower_q} hemi {h:?} evap {}",
            w.input_q(),
            w.reflected_q(),
            w.evaporated()
        );
        (w.input_q(), lower_q, h)
    };
    let (in_up, lq_up, _) = run(SunSweep::Upper);
    let (in_full, lq_full, _) = run(SunSweep::FULL);
    // no night: about twice the booked sun over whole days
    assert!(in_full * 10 > in_up * 15, "full {in_full} vs upper {in_up}");
    assert!(
        lq_full > lq_up,
        "the y < core side warms: {lq_full} vs {lq_up}"
    );
}

/// §5y gas convection: opt-in (default off is the unchanged tick), swaps only
/// rising-gas pairs (mat + Q together), and keeps every book closed.
#[test]
fn convect_is_opt_in_and_conserves() {
    let n = 900i64; // ~r6 sphere
    let (fe, h2s, nh3) = (n / 20, n / 25, n / 5);
    let ch4 = n - fe - h2s - nh3;
    let mut lanes = [0i64; NE];
    lanes[6] = fe;
    lanes[0] = h2s * 2 + nh3 * 3 + ch4 * 4;
    lanes[4] = h2s;
    lanes[3] = nh3;
    lanes[2] = ch4;
    let cfg = GenesisConfig {
        budget: GenesisBudget::new(lanes),
        seed: 3,
        shape: GenesisShape {
            center: Coord::new(0, 7, 0),
            radius: 6,
            hollow: None,
        },
        atom_cap: DEFAULT_ATOM_CAP,
    };
    let make = |convect: bool| {
        let mut w = genesis(&cfg).unwrap();
        w.attach_net(trimmed());
        let mut p = *w.params();
        p.energy_in = EnergyInput {
            sun: 8,
            day: 600,
            core: 40,
            core_r: 0,
        };
        p.sun_sweep = SunSweep::FULL;
        p.cycle = WaterCycle::Driven;
        p.convect = convect;
        w.set_params(p);
        w
    };
    assert!(!P6Params::default().convect, "off by default");
    let (mut off, mut on, mut plain) = (make(false), make(true), genesis(&cfg).unwrap());
    plain.attach_net(trimmed());
    let mut p = *plain.params();
    p.energy_in = EnergyInput {
        sun: 8,
        day: 600,
        core: 40,
        core_r: 0,
    };
    p.sun_sweep = SunSweep::FULL;
    p.cycle = WaterCycle::Driven;
    plain.set_params(p);
    let mut swapped = 0u64;
    for _ in 0..60 {
        assert_eq!(off.step().unwrap().convected, 0);
        plain.step().unwrap();
        swapped += u64::from(on.step().unwrap().convected);
        assert_eq!(on.check_books(), Ok(()));
    }
    assert_eq!(
        off.fingerprint(),
        plain.fingerprint(),
        "convect off is the unchanged tick"
    );
    assert!(swapped > 0, "a hot gas envelope convects");
    assert_ne!(on.fingerprint(), off.fingerprint());
    assert_eq!(on.energy_drift(), 0);
    assert_eq!(on.lane_drift(), [0; NE]);
}
