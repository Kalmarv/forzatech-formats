//! Reads every template library of the Tokyo track (`media/Tracks/Brio/Ribbon_00/instancetemplates`: a version 38
//! library per category in each season's `objects` folder and in the shared one, a version 9 group library in each
//! `Placement` folder) from the install the `GAME_DATA` configuration names. No third-party reader of these files
//! exists, so the check is the census: every library reads to its last byte (`Library::parse` and
//! `GroupLibrary::parse` refuse otherwise), and the model paths of the model templates are looked up in the track's
//! GeoChunk index. Without the install the test skips with a line naming its key.
#![allow(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    reason = "a test reading the files you point it at"
)]

mod inputs;
mod support;

use forzatech_formats::plib::{GROUP_LIBRARY_VERSION, GroupLibrary, LIBRARY_VERSION, Library, version};
use inputs::{Game, install_file};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use support::{Chunks, index};

/// Every `.plib` under `dir`, sorted.
fn plibs(dir: &Path) -> Vec<PathBuf> {
    let mut found = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        for entry in std::fs::read_dir(&d).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                stack.push(path);
            } else if path.extension().is_some_and(|e| e == "plib") {
                found.push(path);
            }
        }
    }
    found.sort();
    found
}

#[test]
fn every_library_reads_to_its_last_byte() {
    let Some(track) = install_file(Game::Fh6, "media/Tracks/Brio") else {
        return;
    };
    let files = plibs(&track.join("Ribbon_00/instancetemplates"));
    let chunks = Chunks::read(&track);
    let index = index(&chunks);
    let (mut libraries, mut groups) = (0, 0);
    let (mut templates, mut placement_templates, mut draw_behaviours) = (0, 0, 0);
    let (mut group_count, mut group_items, mut sets) = (0, 0, 0);
    // model paths of the model templates by extension: (looked up and found, not found)
    let mut paths: BTreeMap<String, (usize, usize)> = BTreeMap::new();
    let mut categories = BTreeMap::new();
    for file in &files {
        let bytes = std::fs::read(file).unwrap();
        let shown = file.display();
        match version(&bytes).unwrap() {
            LIBRARY_VERSION => {
                let lib = Library::parse(&bytes).unwrap_or_else(|e| panic!("{shown}: {e}"));
                libraries += 1;
                *categories.entry(lib.category.clone()).or_insert(0usize) += 1;
                templates += lib.model_templates.len();
                placement_templates += lib.placement_templates.len();
                draw_behaviours += lib.draw_behaviours.len();
                for t in &lib.model_templates {
                    for m in &t.models {
                        let ext = m.rsplit_once('.').map_or(String::new(), |(_, e)| e.to_owned());
                        let e = paths.entry(ext).or_default();
                        if index.template_models(m).next().is_some() {
                            e.0 += 1;
                        } else {
                            e.1 += 1;
                        }
                    }
                }
            }
            GROUP_LIBRARY_VERSION => {
                let lib = GroupLibrary::parse(&bytes).unwrap_or_else(|e| panic!("{shown}: {e}"));
                groups += 1;
                group_count += lib.groups.len();
                group_items += lib.groups.iter().map(|g| g.items.len()).sum::<usize>();
                sets += lib.sets.len();
            }
            v => panic!("{shown}: version {v}"),
        }
    }
    println!(
        "{libraries} libraries ({draw_behaviours} draw behaviours, {templates} model templates, {placement_templates} \
         placement templates), {groups} group libraries ({group_count} groups, {group_items} items, {sets} sets)"
    );
    println!("model paths by extension (found, not found): {paths:?}");
    println!("libraries per category: {categories:?}");
    assert_eq!(libraries, 65);
    assert_eq!(groups, 5);
    assert_eq!(categories.len(), 13);
    let found: usize = paths.values().map(|p| p.0).sum();
    let missing: usize = paths.values().map(|p| p.1).sum();
    assert_eq!(found + missing, 19_776);
    assert_eq!(found, 12_566);
}
