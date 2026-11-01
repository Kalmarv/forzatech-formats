//! Reads the game objects files of the install the `GAME_DATA` configuration names: the Tokyo track's
//! `Ribbon_00/GameObjs.xml`, the open world's `media/Stripped/gs/brio/gameobjs.xml` and the home space's. No
//! third-party reader of these files exists, so the check is the census: every file reads whole (`GameObjs::parse`
//! refuses any element, attribute, value or byte the layout does not account for). Without the install each test
//! skips with a line naming its key.
#![allow(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    reason = "a test reading the files you point it at"
)]

mod inputs;

use forzatech_formats::gameobjs::GameObjs;
use inputs::{Game, install_file};
use std::collections::BTreeSet;

fn read(path: &std::path::Path) -> GameObjs {
    let bytes = std::fs::read(path).unwrap();
    GameObjs::parse(&bytes).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

/// The most an axis' length or two axes' dot product differs from a rotation's: (length, dot).
fn orthonormality(g: &GameObjs) -> (f32, f32) {
    let dot = |a: [f32; 3], b: [f32; 3]| a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
    let (mut length, mut dots) = (0.0f32, 0.0f32);
    for o in &g.objects {
        for a in o.axes {
            length = length.max((dot(a, a).sqrt() - 1.0).abs());
        }
        for (i, j) in [(0, 1), (0, 2), (1, 2)] {
            dots = dots.max(dot(o.axes[i], o.axes[j]).abs());
        }
    }
    (length, dots)
}

#[test]
fn the_track_file_reads_whole() {
    let Some(track) = install_file(Game::Fh6, "media/Tracks/Brio") else {
        return;
    };
    let g = read(&track.join("Ribbon_00/GameObjs.xml"));
    let ids: BTreeSet<_> = g.objects.iter().map(|o| o.id.as_str()).collect();
    let (length, dots) = orthonormality(&g);
    println!(
        "{} objects, {} ids, {} with a guid, axes off unit length by at most {length}, off orthogonal by at most {dots}",
        g.objects.len(),
        ids.len(),
        g.objects.iter().filter(|o| o.guid.is_some()).count()
    );
    assert_eq!((g.objects.len(), ids.len()), (793, 793));
    assert!(g.objects.iter().all(|o| o.guid.is_none()));
    let first = &g.objects[0];
    assert_eq!(first.id, "ANIM_CM_E0001_Ambient_HighSpeedTrain_Carriage");
    assert_eq!(first.position, [0.0; 3]);
}

#[test]
fn the_open_world_file_reads_whole() {
    let Some(dir) = install_file(Game::Fh6, "media/Stripped/gs/brio") else {
        return;
    };
    let g = read(&dir.join("gameobjs.xml"));
    let ids: BTreeSet<_> = g.objects.iter().map(|o| o.id.as_str()).collect();
    let (length, dots) = orthonormality(&g);
    println!(
        "{} objects, {} ids, axes off unit length by at most {length}, off orthogonal by at most {dots}",
        g.objects.len(),
        ids.len()
    );
    assert_eq!((g.objects.len(), ids.len()), (59, 45));
    assert!(g.objects.iter().all(|o| o.guid.is_some()));
}

#[test]
fn the_home_space_file_holds_no_object() {
    let Some(file) = install_file(Game::Fh6, "media/Tracks/homespace02/Ribbon_00/GameObjs.xml") else {
        return;
    };
    assert!(read(&file).objects.is_empty());
}
