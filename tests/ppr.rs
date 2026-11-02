//! Reads the property files of the install the `GAME_DATA` configuration names: `media/TimeOfDay/TimeOfDay_Brio.ppr`
//! (the Tokyo track's sky presets), `TimeOfDay.ppr` (the older tracks') and `Homespace.ppr`. No third-party reader
//! of these files exists, so the check is the census: each file reads to its last byte (`Ppr::parse` refuses any
//! group, leaf, key or byte the layout does not account for, so a wrong stride anywhere would shift every later
//! record and end in a refusal), and the counts below are the ones the engine wiki's time-of-day page states. Without
//! the install each test skips with a line naming its key.
#![allow(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    reason = "a test reading the files you point it at"
)]

mod inputs;

use forzatech_formats::ppr::{Group, Ppr, Value};
use inputs::{Game, install_file};
use std::collections::BTreeMap;

fn read(name: &str) -> Option<Ppr> {
    let folder = install_file(Game::Fh6, "media/TimeOfDay")?;
    let path = folder.join(name);
    let bytes = std::fs::read(&path).unwrap();
    Some(Ppr::parse(&bytes).unwrap_or_else(|e| panic!("{}: {e}", path.display())))
}

fn leaves(p: &Ppr) -> usize {
    p.groups.iter().map(|g| g.leaves.len()).sum()
}

/// A group's scalar curve by property name: its (time, value) keys.
fn scalar_keys(p: &Ppr, g: &Group, name: &str) -> Vec<(f32, f32)> {
    let leaf = p.leaf(g, name).unwrap().unwrap_or_else(|| panic!("no {name}"));
    let Value::Curve(c) = &leaf.value else {
        panic!("{name} is not a curve");
    };
    c.scalar_keys()
        .unwrap_or_else(|| panic!("{name} is not one scalar component"))
        .collect()
}

/// A curve's shape the way the wiki counts it: its value when every key holds one, else its range.
fn shape(keys: &[(f32, f32)]) -> String {
    let lo = keys.iter().map(|k| k.1).fold(f32::INFINITY, f32::min);
    let hi = keys.iter().map(|k| k.1).fold(f32::NEG_INFINITY, f32::max);
    if lo == hi {
        format!("constant {lo}")
    } else {
        format!("curve {lo} to {hi}")
    }
}

#[test]
fn the_tokyo_file_reads_whole() {
    let Some(p) = read("TimeOfDay_Brio.ppr") else {
        return;
    };
    println!(
        "{} names, {} groups, {} leaves",
        p.name_count(),
        p.groups.len(),
        leaves(&p)
    );
    assert_eq!(p.version, 3);
    assert_eq!(p.name(p.root), "TimeOfDay_Brio");
    assert_eq!((p.name_count(), p.groups.len(), leaves(&p)), (2954, 2350, 54109));
    let mut kinds: BTreeMap<&str, usize> = BTreeMap::new();
    for g in &p.groups {
        *kinds.entry(p.name(g.kind)).or_default() += 1;
    }
    println!("{kinds:?}");
    assert_eq!(kinds.len(), 22);
    assert_eq!(kinds["Sky"], 203);
    assert_eq!(kinds["SkyPreset"], 1113);
    assert_eq!(kinds["SkyGlobals"], 1);
    // the first group is the file's globals, over the whole day
    assert_eq!(p.name(p.groups[0].kind), "SkyGlobals");
    assert_eq!(p.groups[0].range, [0.0, 1440.0]);
    assert_eq!(p.groups[0].leaves.len(), 48);
}

#[test]
fn every_sky_has_the_same_199_leaves() {
    let Some(p) = read("TimeOfDay_Brio.ppr") else {
        return;
    };
    let skies: Vec<&Group> = p.groups_of("Sky").collect();
    assert_eq!(skies.len(), 203);
    let names = |g: &Group| g.leaves.iter().map(|l| l.name).collect::<Vec<_>>();
    let first = names(skies[0]);
    assert_eq!(first.len(), 199);
    assert!(skies.iter().all(|g| names(g) == first));
    // a name occurs once in a group, so a lookup by name never refuses here
    for g in &skies {
        for l in &g.leaves {
            p.leaf(g, p.name(l.name)).unwrap().unwrap();
        }
    }
    // 13 of the 190 labels occur twice
    let mut labels: BTreeMap<&str, usize> = BTreeMap::new();
    for g in &skies {
        *labels.entry(p.name(g.label)).or_default() += 1;
    }
    println!(
        "{} labels, {} repeated",
        labels.len(),
        labels.values().filter(|&&n| n > 1).count()
    );
    assert_eq!(labels.len(), 190);
    assert_eq!(labels.values().filter(|&&n| n > 1).count(), 13);
}

/// The wiki's census of `EmissiveScales.TrackEmissiveEV` over the 203 skies, as its shapes.
#[test]
fn track_emissive_ev_census() {
    let Some(p) = read("TimeOfDay_Brio.ppr") else {
        return;
    };
    let mut census: BTreeMap<String, usize> = BTreeMap::new();
    for g in p.groups_of("Sky") {
        let keys = scalar_keys(&p, g, "EmissiveScales.TrackEmissiveEV");
        *census.entry(shape(&keys)).or_default() += 1;
    }
    println!("{census:#?}");
    let n = |s: &str| census.get(s).copied().unwrap_or(0);
    assert_eq!(census.values().sum::<usize>(), 203);
    assert_eq!(n("constant 1"), 71);
    assert_eq!(n("constant 6600"), 50);
    assert_eq!(n("curve 1 to 50"), 30);
    assert_eq!(n("constant 100"), 25);
    assert_eq!(n("constant 50"), 8);
    for once in ["constant 35", "constant 1200", "constant 50000", "constant 5"] {
        assert_eq!(n(once), 2, "{once}");
    }
    // the others, one sky each
    assert_eq!(census.values().filter(|&&c| c == 1).count(), 11);
}

#[test]
fn forte_summer_01() {
    let Some(p) = read("TimeOfDay_Brio.ppr") else {
        return;
    };
    let g = p.group("Sky", "Forte_Summer_01").unwrap().unwrap();
    let ev = scalar_keys(&p, g, "EmissiveScales.TrackEmissiveEV");
    println!("{ev:?} range {:?}", g.range);
    assert_eq!(ev.len(), 6);
    let peak = ev.iter().map(|k| k.1).fold(f32::NEG_INFINITY, f32::max);
    assert_eq!(peak, 50.0);
    assert_eq!(ev[0], (0.0, 1.0));
    assert_eq!(ev[5], (1.0, 1.0));
    let scale = p
        .leaf(g, "PhysicalLightUnits.SkyToLuminanceScale")
        .unwrap()
        .unwrap();
    assert_eq!(scale.value, Value::Float(13_200.0));
    let sun = scalar_keys(&p, g, "Sun.LightingIlluminanceLux");
    assert_eq!(
        sun.iter().map(|k| k.1).fold(f32::NEG_INFINITY, f32::max),
        65_000.0
    );
    // a label on two groups is refused, not picked from
    let twice = p
        .groups_of("Sky")
        .find(|g| p.name(g.label) == "BeautifulArea_BA03_010")
        .unwrap();
    assert!(p.group("Sky", p.name(twice.label)).is_err());
}

#[test]
fn the_older_tracks_file_reads_whole() {
    let Some(p) = read("TimeOfDay.ppr") else {
        return;
    };
    println!(
        "{} names, {} groups, {} leaves",
        p.name_count(),
        p.groups.len(),
        leaves(&p)
    );
    assert_eq!(p.version, 2);
    assert_eq!(p.name(p.root), "TimeOfDay");
    assert_eq!((p.name_count(), p.groups.len(), leaves(&p)), (1962, 1337, 33500));
}

#[test]
fn the_home_space_file_has_no_group() {
    let Some(p) = read("Homespace.ppr") else {
        return;
    };
    assert_eq!(p.version, 2);
    assert_eq!(p.name(p.root), "Homespace");
    assert!(p.groups.is_empty());
}
