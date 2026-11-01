//! Reads the Tokyo track's hand-placed instance file (`media/Tracks/Brio/Ribbon_00/brio_00.pvs`, 42.8 MB) and the
//! texture-only one beside the general scene assets (`media/Stripped/gs/generalsceneassets.pvs`) from the install the
//! `GAME_DATA` configuration names. No third-party reader of these files exists, so the check is the census: both
//! files read to their last byte, every reference inside what it refers to (`Pvs::parse` refuses otherwise), and the
//! references into the GeoChunks resolve through the track's chunk maps. Without the install each test skips with a
//! line naming its key.
#![allow(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    reason = "a test reading the files you point it at"
)]

mod inputs;
mod support;

use forzatech_formats::pvs::{CATALOGUE, CELL_REFERENCE_TYPE, Pvs};
use forzatech_formats::track::PVSZ;
use forzatech_formats::world::Area;
use inputs::{Game, install_file};
use std::collections::BTreeSet;
use support::{Chunks, index};

#[test]
fn the_track_file_reads_to_its_last_byte() {
    let Some(track) = install_file(Game::Fh6, "media/Tracks/Brio") else {
        return;
    };
    let bytes = std::fs::read(track.join("Ribbon_00/brio_00.pvs")).unwrap();
    let pvs = Pvs::parse(&bytes).unwrap();
    let w = pvs.world.as_ref().unwrap();
    println!(
        "textures {} megatextures {} bounds {} gameplay {} shaders {} materials {} models {} cells {} dependencies {} \
         frames {} transforms {} rows {} lod groups {} profiles {} instances {} model bounds {} clips {} clip files {} \
         clip records {}",
        pvs.textures.len(),
        w.megatextures.entries.len(),
        w.bounds.len(),
        w.gameplay.records.len(),
        w.shaders.len(),
        w.materials.len(),
        w.models.len(),
        w.cells.len(),
        w.dependencies.len(),
        w.frames.len(),
        w.transforms.len(),
        w.lod_rows.len(),
        w.lod_profiles.len(),
        w.lod_profiles.iter().map(Vec::len).sum::<usize>(),
        w.instances.len(),
        w.model_bounds.len(),
        w.tail.clips.len(),
        w.tail.clip_files.len(),
        w.tail.clip_records.len(),
    );
    assert_eq!(bytes.len(), 42_782_135);
    assert!(pvs.header.world);
    assert_eq!(pvs.header.unknown_25, 14_000_000);
    assert_eq!(pvs.textures.len(), 48_733);
    assert_eq!(w.megatextures.entries.len(), 1_476);
    assert_eq!(w.bounds.len(), 76_907);
    assert_eq!(w.gameplay.records.len(), 1_079);
    assert_eq!(w.gameplay.values.len(), 120_225);
    assert_eq!(w.index_tables.len(), 5);
    assert!(w.index_tables.iter().all(|t| t.len() == 15_104));
    assert_eq!(w.shaders.len(), 46);
    assert_eq!(w.materials.len(), 419);
    assert_eq!(w.models.len(), 134_330);
    assert_eq!(w.model_bounds.len(), 134_330);
    assert_eq!(w.cells.len(), 41_106);
    assert_eq!(w.dependencies.len(), 17_560);
    assert_eq!(w.frames.len(), 5_489);
    assert_eq!(w.transforms.len(), 111_452);
    assert_eq!(w.lod_rows.len(), 593_706);
    assert_eq!(w.lod_profiles.len(), 155);
    assert_eq!(w.lod_profiles.iter().map(Vec::len).sum::<usize>(), 2_232);
    assert_eq!(w.instances.len(), 154_254);
    assert_eq!(w.tail.name, "brioout");
    assert_eq!(w.tail.clips.len(), 14);
    assert_eq!(w.tail.clip_files.len(), 5);
    assert_eq!(w.tail.clip_records.len(), 14);
    assert_eq!(w.tail.texture_keys.len(), pvs.textures.len());

    // catalogue copies sit at the origin
    let catalogue: Vec<_> = w.instances.iter().filter(|i| i.flags & CATALOGUE != 0).collect();
    assert_eq!(catalogue.len(), 3_483);
    for i in catalogue {
        assert_eq!(w.transform_of(i).unwrap().position, [0.0; 3]);
    }
    // every transform with a LOD profile group is used, and the ones without are not
    let mut used = vec![false; w.transforms.len()];
    for i in &w.instances {
        used[i.transform as usize] = true;
    }
    for (t, used) in w.transforms.iter().zip(&used) {
        if t.lod_group.is_none() {
            assert!(!used, "a transform with no LOD profile group is an instance's");
        }
    }
    assert_eq!(
        w.transforms.iter().filter(|t| t.lod_group.is_none()).count(),
        2_069
    );
}

#[test]
fn the_track_file_names_geochunk_entries() {
    let Some(track) = install_file(Game::Fh6, "media/Tracks/Brio") else {
        return;
    };
    let pvs = Pvs::parse(&std::fs::read(track.join("Ribbon_00/brio_00.pvs")).unwrap()).unwrap();
    let w = pvs.world.as_ref().unwrap();
    let chunks = Chunks::read(&track);
    let index = index(&chunks);
    assert_eq!(index.len(), 495_394);
    assert_eq!(index.model_count(), 113_868);

    // model numbers: the ids of the type-4 records
    let numbers: BTreeSet<u32> = (0..u32::try_from(w.models.len()).unwrap()).collect();
    let resolved: BTreeSet<u32> = numbers
        .iter()
        .copied()
        .filter(|&n| index.pvs_model(n).is_some())
        .collect();
    let rows_use: BTreeSet<u32> = w.lod_rows.iter().map(|r| r.model).collect();
    let rows_unresolved = rows_use.difference(&resolved).count();
    println!(
        "model numbers {}: {} have a modelbin; LOD rows use {}, of which {rows_unresolved} have none",
        numbers.len(),
        resolved.len(),
        rows_use.len()
    );
    // every modelbin of the install is named by a model number
    assert_eq!(resolved.len(), 113_868);
    assert_eq!(numbers.len() - resolved.len(), 20_462);
    assert_eq!(rows_unresolved, 19_436);

    // cells: the second reference of a record is a `.pvsz` entry; the first is not an entry of the install
    let mut seconds = BTreeSet::new();
    for c in &w.cells {
        assert_eq!(c.first >> 24, u32::from(CELL_REFERENCE_TYPE));
        assert!(index.reference(c.first).is_none(), "{:#x}", c.first);
        let at = index.reference(c.second).unwrap();
        assert_eq!(at.chunk, 0);
        assert_eq!(chunks.maps[at.chunk][at.entry].resource_type, PVSZ);
        assert_eq!(
            (c.first & 0xFF_FFFF) - (c.second & 0xFF_FFFF),
            u32::from(c.level) * 6_851,
            "cell {:#x} {:#x} level {}",
            c.first,
            c.second,
            c.level
        );
        seconds.insert(c.second);
    }
    assert_eq!(seconds.len(), 13_702);
    // each `.pvsz` entry has one record at each of levels 1, 2 and 3, and the firsts are all different
    let mut levels: std::collections::BTreeMap<u32, Vec<u8>> = std::collections::BTreeMap::new();
    for c in &w.cells {
        levels.entry(c.second).or_default().push(c.level);
    }
    assert!(levels.values().all(|l| l[..] == [1, 2, 3]));
    assert_eq!(
        w.cells.iter().map(|c| c.first).collect::<BTreeSet<_>>().len(),
        w.cells.len()
    );
    assert_eq!(seconds.len(), chunks.of_type(PVSZ).len());
}

#[test]
fn placements_in_one_box() {
    let Some(track) = install_file(Game::Fh6, "media/Tracks/Brio") else {
        return;
    };
    let pvs = Pvs::parse(&std::fs::read(track.join("Ribbon_00/brio_00.pvs")).unwrap()).unwrap();
    let w = pvs.world.as_ref().unwrap();
    // the Shibuya Scramble section: x -1550..-1100, z -5000..-4566.67
    let area = Area::new(-1550.0, -5000.0, -1100.0, -13_700.0 / 3.0);
    let inside: Vec<_> = w.instances_in(&area).collect();
    let placed = inside.iter().filter(|(_, i)| i.flags & CATALOGUE == 0).count();
    let rows: usize = inside.iter().map(|&(k, _)| w.rows_of(k).len()).sum();
    println!(
        "{} instances in the box ({placed} placed), {rows} LOD rows",
        inside.len()
    );
    assert!(placed > 0);
    for (_, i) in &inside {
        let [x, _, z] = w.transform_of(i).unwrap().position;
        assert!(area.contains(f64::from(x), f64::from(z)));
    }
}

#[test]
fn the_general_scene_assets_file_is_textures_only() {
    let Some(path) = install_file(Game::Fh6, "media/Stripped/gs/generalsceneassets.pvs") else {
        return;
    };
    let bytes = std::fs::read(path).unwrap();
    let pvs = Pvs::parse(&bytes).unwrap();
    assert_eq!(bytes.len(), 381_229);
    assert!(!pvs.header.world);
    assert_eq!(pvs.header.unknown_25, 0);
    assert_eq!(pvs.textures.len(), 6_807);
    assert!(pvs.world.is_none());
}
