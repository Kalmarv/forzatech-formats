//! Reads the `.pgeo` placement files of the Tokyo track (`media/Tracks/Brio`: 96,234 entries of type 0 in
//! `GeoChunk0.minizip`) from the install the `GAME_DATA` configuration names. No third-party reader of these files
//! exists, so the check is the census:
//!
//! - Every file reads to its last byte with every check `Pgeo::parse` makes holding, and its path has a cell
//!   (`cellsize\<size>\<x>_<z>`). A sample drawn with the day's seed at smoke depth, every file with `[run] gate =
//!   'full'` (about 6 minutes: run it as a job).
//! - Every instance's position is inside its file's header box widened by a fixed-point step and an `f32` rounding
//!   (the rule `Header::may_hold` selects files by); a crowd spline file's instances are all at the origin and its
//!   splines' boxes are inside the header box.
//! - Each block, in each season the file is shown in, names a model template of the category's library for that
//!   season, and its source ranges name placement templates of it; the model templates' model paths are looked up
//!   in the track's GeoChunk index. Those that have none are counted, not refused: the files do not hold them.
//! - The placements in one x/z box (the Shibuya Scramble section) found by reading only the files whose header box
//!   may hold some are those found by reading every file read.
//!
//! Without the install the test skips with a line naming its key.
#![allow(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    reason = "a test reading the files you point it at"
)]

mod inputs;
mod support;

use forzatech_formats::minizip::MiniZip;
use forzatech_formats::pgeo::{Curve, Header, Kind, Pgeo};
use forzatech_formats::plib::Library;
use forzatech_formats::track::{PGEO, Track};
use forzatech_formats::world::{Area, Cell, FIXED_STEP, Season};
use inputs::{Game, install_file};
use primase_testdata::{Depth, depth, seed};
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fs::File;
use std::io::BufReader;
use std::path::Path;
use support::{Chunks, index, sampled};

/// The libraries by season and category.
fn libraries(track: &Path) -> HashMap<(Season, String), Library> {
    let mut libs = HashMap::new();
    for s in Season::ALL {
        let dir = track.join(format!("Ribbon_00/instancetemplates/{}/objects", s.folder()));
        for e in std::fs::read_dir(dir).unwrap() {
            let lib = Library::parse(&std::fs::read(e.unwrap().path()).unwrap()).unwrap();
            libs.insert((s, lib.category.clone()), lib);
        }
    }
    libs
}

#[derive(Default)]
struct Census {
    files: usize,
    blocks: usize,
    instances: u64,
    kinds: BTreeMap<&'static str, usize>,
    categories: BTreeMap<String, usize>,
    cell_sizes: BTreeMap<u32, usize>,
    masks: BTreeMap<u8, usize>,
    instances_by_category: BTreeMap<String, u64>,
    instances_by_mask: BTreeMap<u8, u64>,
    // curve records: the blocks that have them, the spline records and the no-curve ones
    curve_blocks: usize,
    curve_splines: u64,
    curve_none: u64,
    // source ranges, and the blocks with none by name
    sources: u64,
    no_source_blocks: BTreeMap<String, usize>,
    outside_cell: u64,
    outside_box: u64,
    origin_instances: u64,
    splines: u64,
    box_splines: u64,
    box_splines_all: u64,
    // instance-by-shown-season counts
    no_template: u64,
    // the block names with no template, and with several, as `<season> <category> <name>`
    no_template_names: BTreeMap<String, u64>,
    several_template_names: BTreeSet<String>,
    one_template: u64,
    several_templates: u64,
    sources_found: u64,
    sources_missing: u64,
    models_found: u64,
    models_missing: u64,
    // the box query
    box_files: usize,
    box_instances: u64,
    box_instances_all: u64,
}

fn kind_name(k: Kind) -> &'static str {
    match k {
        Kind::Category => "category",
        Kind::Crowd => "crowd",
        Kind::CrowdSpline => "crowd spline",
    }
}

impl Census {
    fn file(
        &mut self,
        name: &str,
        bytes: &[u8],
        libs: &HashMap<(Season, String), Library>,
        index: &Track,
        area: &Area,
    ) {
        let p = Pgeo::parse(bytes).unwrap_or_else(|e| panic!("{name}: {e}"));
        let h = &p.header;
        // the header alone reads the same header
        assert_eq!(&Header::parse(bytes).unwrap(), h, "{name}");
        let cell = Cell::from_path(name).unwrap_or_else(|| panic!("{name}: no cell"));
        self.files += 1;
        *self.kinds.entry(kind_name(h.kind)).or_default() += 1;
        *self.categories.entry(h.category.clone()).or_default() += 1;
        *self.cell_sizes.entry(cell.size).or_default() += 1;
        *self.masks.entry(h.season_exclusion.byte()).or_default() += 1;
        self.splines += p.splines.len() as u64;
        let cell_area = cell.area();
        let may_hold = h.may_hold(area);
        let in_box = p.instances_in(area).count() as u64;
        let splines_in_box = p.splines_in(area).count() as u64;
        self.box_instances_all += in_box;
        self.box_splines_all += splines_in_box;
        if may_hold {
            self.box_files += 1;
            self.box_instances += in_box;
            self.box_splines += splines_in_box;
        } else {
            assert_eq!(
                in_box + splines_in_box,
                0,
                "{name}: the header box misses the area but the file has placements in it"
            );
        }
        let widen = |v: f32, up: bool| {
            if up {
                f64::from(v.next_up()) + FIXED_STEP
            } else {
                f64::from(v.next_down()) - FIXED_STEP
            }
        };
        for s in &p.splines {
            for (((lo, hi), box_lo), box_hi) in s.min.into_iter().zip(s.max).zip(h.min).zip(h.max) {
                if f64::from(lo) < widen(box_lo, false) || f64::from(hi) > widen(box_hi, true) {
                    self.outside_box += 1;
                }
            }
        }
        for b in p.blocks() {
            self.blocks += 1;
            self.instances += b.instances.len() as u64;
            *self.instances_by_category.entry(h.category.clone()).or_default() += b.instances.len() as u64;
            *self
                .instances_by_mask
                .entry(h.season_exclusion.byte())
                .or_default() += b.instances.len() as u64;
            self.sources += b.sources.len() as u64;
            if b.sources.is_empty() {
                *self.no_source_blocks.entry(b.name.clone()).or_default() += 1;
            }
            if let Some(curves) = &b.curves {
                self.curve_blocks += 1;
                for c in curves {
                    match c {
                        Curve::None => self.curve_none += 1,
                        Curve::Spline { .. } => self.curve_splines += 1,
                    }
                }
            }
            for i in &b.instances {
                if h.kind == Kind::CrowdSpline {
                    assert_eq!(
                        i.position, [0; 3],
                        "{name}: a crowd spline file's instance away from the origin"
                    );
                    self.origin_instances += 1;
                    continue;
                }
                let m = i.metres();
                for ((v, lo), hi) in m.into_iter().zip(h.min).zip(h.max) {
                    if v < widen(lo, false) || v > widen(hi, true) {
                        self.outside_box += 1;
                    }
                }
                if !cell_area.contains(m[0], m[2]) {
                    self.outside_cell += 1;
                }
            }
            let n = b.instances.len() as u64;
            for s in Season::ALL {
                if !h.season_exclusion.shows(s) {
                    continue;
                }
                let lib = libs
                    .get(&(s, h.category.clone()))
                    .unwrap_or_else(|| panic!("{name}: no {s:?} library for {}", h.category));
                let templates: Vec<_> = lib.model_templates_named(&b.name).collect();
                match templates.len() {
                    0 => {
                        self.no_template += n;
                        *self
                            .no_template_names
                            .entry(format!("{s:?} {} {}", h.category, b.name))
                            .or_default() += n;
                    }
                    1 => self.one_template += n,
                    _ => {
                        self.several_templates += n;
                        self.several_template_names
                            .insert(format!("{s:?} {} {}", h.category, b.name));
                    }
                }
                for t in &templates {
                    for m in &t.models {
                        if index.template_models(m).next().is_some() {
                            self.models_found += n;
                        } else {
                            self.models_missing += n;
                        }
                    }
                }
                for src in &b.sources {
                    if lib.placement_templates_named(&src.template).next().is_some() {
                        self.sources_found += 1;
                    } else {
                        self.sources_missing += 1;
                    }
                }
            }
        }
    }
}

#[test]
fn every_placement_file_reads_and_links() {
    let Some(track) = install_file(Game::Fh6, "media/Tracks/Brio") else {
        return;
    };
    let (depth, seed) = (depth(), seed());
    let chunks = Chunks::read(&track);
    let index = index(&chunks);
    let libs = libraries(&track);
    let entries = chunks.of_type(PGEO);
    assert_eq!(entries.len(), 96_234);
    assert!(
        entries.iter().all(|&(chunk, _)| chunk == 0),
        "a .pgeo outside GeoChunk0"
    );
    let lines = chunks.lines();
    let mut f = BufReader::with_capacity(1 << 20, File::open(track.join("GeoChunk0.minizip")).unwrap());
    let zip = MiniZip::read(&mut f).unwrap();
    // the Shibuya Scramble section: x -1550..-1100, z -5000..-4566.67
    let area = Area::new(-1550.0, -5000.0, -1100.0, -13_700.0 / 3.0);
    println!("depth {depth:?}, seed {seed}");
    let mut c = Census::default();
    for (k, &(_, entry)) in entries.iter().enumerate() {
        if depth == Depth::Smoke && !sampled(seed, k, 64) {
            continue;
        }
        let path = lines[0][entry].zipcache_path().unwrap();
        let bytes = zip.read_entry(&mut f, entry).unwrap();
        c.file(path, &bytes, &libs, &index, &area);
    }
    println!(
        "{} files, {} blocks, {} instances, {} splines\nkinds {:?}\ncategories {:?}\ncell sizes {:?}\nseason masks {:?}\n\
         instances by category {:?}\ninstances by season mask {:?}\n\
         blocks with curve records {}: {} splines, {} no-curve; source ranges {}; blocks with none, by name {:?}\n\
         outside the cell {}, outside the header box {}, crowd spline instances at the origin {}\n\
         block names by shown season, in instances: no template {}, one {}, several {}\n\
         block names with no template {}: {:?}\nblock names with several {:?}\n\
         sources: found {}, missing {}\nmodel paths, in instances: with a modelbin {}, without {}\n\
         box: {} files whose header may hold, {} instances and {} splines there, {} and {} found over every file read",
        c.files,
        c.blocks,
        c.instances,
        c.splines,
        c.kinds,
        c.categories,
        c.cell_sizes,
        c.masks,
        c.instances_by_category,
        c.instances_by_mask,
        c.curve_blocks,
        c.curve_splines,
        c.curve_none,
        c.sources,
        c.no_source_blocks,
        c.outside_cell,
        c.outside_box,
        c.origin_instances,
        c.no_template,
        c.one_template,
        c.several_templates,
        c.no_template_names.len(),
        c.no_template_names,
        c.several_template_names,
        c.sources_found,
        c.sources_missing,
        c.models_found,
        c.models_missing,
        c.box_files,
        c.box_instances,
        c.box_splines,
        c.box_instances_all,
        c.box_splines_all,
    );
    assert_eq!(c.box_instances, c.box_instances_all);
    assert_eq!(c.box_splines, c.box_splines_all);
    assert_eq!(c.outside_box, 0, "positions outside the header box");
    assert_eq!(c.sources_missing, 0, "sources naming no placement template");
    if depth == Depth::Full {
        assert_eq!(c.files, 96_234);
        assert_eq!(c.blocks, 665_298);
        assert_eq!(c.instances, 29_164_671);
        assert_eq!(c.splines, 244);
        assert_eq!(
            c.kinds,
            BTreeMap::from([("category", 95_694), ("crowd", 296), ("crowd spline", 244)])
        );
        assert_eq!(c.masks.values().sum::<usize>(), 96_234);
        assert_eq!(c.masks.len(), 10);
        assert_eq!(c.cell_sizes.len(), 10);
        assert_eq!(
            (c.curve_blocks, c.curve_splines, c.curve_none),
            (11_658, 243_394, 11_634)
        );
        assert_eq!(c.sources, 739_181);
        assert_eq!(
            c.no_source_blocks,
            BTreeMap::from([
                ("ANIMATED".to_owned(), 273),
                ("ANIMATED_SPRITE".to_owned(), 273),
                ("PEDESTRIAN".to_owned(), 315),
                ("bsh_flat_urb_garden_3D".to_owned(), 555),
            ])
        );
        assert_eq!(c.origin_instances, 2_914);
        assert_eq!(c.outside_cell, 337_778);
        assert_eq!(
            (c.no_template, c.one_template, c.several_templates),
            (37_066, 65_988_966, 3_579)
        );
        assert_eq!(c.no_template_names.len(), 27);
        assert!(
            c.no_template_names
                .keys()
                .all(|n| n.starts_with("Winter GRASSTEMPLATE ") && n.ends_with("_3D_HIGH")),
            "no-template names: {:?}",
            c.no_template_names
        );
        assert_eq!(c.no_template_names.values().sum::<u64>(), c.no_template);
        let several: BTreeSet<String> = ["Autumn", "Spring", "Summer", "Winter"]
            .map(|s| format!("{s} BARRIERS bar_gbl_crct_foam_01_b_3D"))
            .into_iter()
            .chain(["Autumn TREES tre_urb_ginkgo_d_3D".to_owned()])
            .collect();
        assert_eq!(c.several_template_names, several);
        assert_eq!(c.sources_found, 2_748_074);
        assert_eq!((c.models_found, c.models_missing), (65_058_898, 937_226));
        assert_eq!((c.box_files, c.box_instances, c.box_splines), (297, 68_556, 30));
    }
}
