//! Reads every particle instances file of the install the `GAME_DATA` configuration names: the Tokyo track's
//! `Ribbon_00/ParticleInstances.particlebin`, the home space's, and the 168 route definitions under
//! `media/Stripped/gs/brio/routedefinitions`. No third-party reader of these files exists, so the check is the census:
//! every file reads to its last byte (`Particles::parse` refuses otherwise: every constant, link and string offset is
//! checked). Without the install each test skips with a line naming its key.
#![allow(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    reason = "a test reading the files you point it at"
)]

mod inputs;

use forzatech_formats::particles::Particles;
use inputs::{Game, install_file};
use std::collections::BTreeSet;

fn read(path: &std::path::Path) -> Particles {
    let bytes = std::fs::read(path).unwrap();
    Particles::parse(&bytes).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

#[test]
fn the_track_file_reads_to_its_last_byte() {
    let Some(track) = install_file(Game::Fh6, "media/Tracks/Brio") else {
        return;
    };
    let p = read(&track.join("Ribbon_00/ParticleInstances.particlebin"));
    let names: BTreeSet<_> = p.instances.iter().map(|i| i.name.as_str()).collect();
    let effects: BTreeSet<_> = p.instances.iter().map(|i| i.effect.as_str()).collect();
    let groups: BTreeSet<_> = p.instances.iter().map(|i| i.group.as_str()).collect();
    println!(
        "{} instances, {} names, {} effects, {} groups, flag set on {}",
        p.instances.len(),
        names.len(),
        effects.len(),
        groups.len(),
        p.instances.iter().filter(|i| i.flag).count()
    );
    assert_eq!(p.instances.len(), 3_959);
    assert_eq!((names.len(), effects.len(), groups.len()), (986, 50, 26));
    assert_eq!(p.instances.iter().filter(|i| i.flag).count(), 248);
    let first = &p.instances[0];
    assert_eq!(first.name, "falling_leaves_customizable_Instance12");
    assert_eq!(first.effect, "falling_leaves_customizable");
    assert_eq!(first.group, "leaves_ginkgo_falling");
    assert_eq!(
        first.position.map(f32::to_bits),
        [0xc404_7115, 0x42dc_7327, 0xc55f_ff1b]
    );
}

#[test]
fn the_home_space_file_holds_no_instance() {
    let Some(file) = install_file(
        Game::Fh6,
        "media/Tracks/homespace02/Ribbon_00/ParticleInstances.particlebin",
    ) else {
        return;
    };
    assert!(read(&file).instances.is_empty());
}

#[test]
fn every_route_definition_reads_to_its_last_byte() {
    let Some(dir) = install_file(Game::Fh6, "media/Stripped/gs/brio") else {
        return;
    };
    let (mut files, mut instances, mut empty) = (0, 0, 0);
    for entry in std::fs::read_dir(dir.join("routedefinitions")).unwrap() {
        let path = entry.unwrap().path();
        if path.extension().is_none_or(|e| e != "particlebin") {
            continue;
        }
        let p = read(&path);
        files += 1;
        instances += p.instances.len();
        empty += usize::from(p.instances.is_empty());
    }
    println!("{files} route definitions, {instances} instances, {empty} without any");
    assert_eq!((files, instances, empty), (168, 508, 91));
}
