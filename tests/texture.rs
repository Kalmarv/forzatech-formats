//! Reads every texture of a real install: the `.pb` entries of the Tokyo track's four GeoChunks
//! (`media/Tracks/Brio`) and the `.swatchbin` entries of every zip under `media`, from the install the `GAME_DATA`
//! configuration names, and compares each with two independent readings of the same bundle (the oracle sets
//! `forzatools` and `doliman`, see tests/inputs/README.md). Without the install or the keys the test skips with a line
//! naming its key.
//!
//! - Every texture reads with every check [`Texture::parse`] makes holding (the header accounts for every byte, the
//!   mips lie contiguous to the end of the payload, each mip is the size its format needs): a sample of entries drawn
//!   with the day's seed, or all of them with `[run] gate = 'full'`. Over them: flag bit 0 only on textures of 6
//!   slices, at most 128 slices, flags at most 6, the header's GUID the last GUID of the file name where the name holds one.
//! - Each PC texture's header fields (GUID, dimensions, depth, slice and mip counts, flag byte, transcoding, colour
//!   profiles, the encoding and the first mip's size, the payload's size and the DXGI format the encoding and colour
//!   profile give) are those ForzaTools' `PCTextureContentHeader` reads; each Durango texture's are those of its
//!   `DurangoTextureContentHeader`.
//! - Of a sample of PC textures, each mip of the first slice is the bytes ForzaTools.TexConv's own DDS writer and
//!   BCnEncoder.Net's DDS reader find at that mip (BCnEncoder takes each mip's size from the DXGI format alone: an
//!   independent reading of where the mips lie in the payload), for the textures of depth 1 (of a deeper one only the
//!   mip count).
//! - Each PC texture of one slice agrees with Doliman100's `Texture.deserialize` on the GUID, the dimensions, the mip
//!   count, the first mip's size (its "linear size"), the DXGI format (where it knows the encoding) and the
//!   SHA-256 of the payload it copies.
//!
//! The oracle readings are compared at every entry the test reads; at full depth the test also finds every
//! texture the oracle lists.
#![allow(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    reason = "a test reading the files you point it at"
)]

mod inputs;
mod walk;

use forzatech_formats::keys::Keys;
use forzatech_formats::texture::{Encoding, Header, Texture};
use inputs::{Game, OracleSet, install, install_file, keys, oracle};
use primase_testdata::{Depth, depth, seed};
use std::collections::{BTreeMap, HashMap};
use walk::{Walk, hex, named, sha256, tsv};

type Key = (String, usize);

#[derive(Default)]
struct Oracles {
    pc: HashMap<Key, Vec<String>>,
    durango: HashMap<Key, Vec<String>>,
    dds: HashMap<Key, Vec<Vec<String>>>,
    doliman: HashMap<Key, Vec<String>>,
    /// The textures Doliman's library raised on, with its exception (it reads no Durango texture).
    doliman_errors: HashMap<Key, String>,
}

fn key(row: &[String]) -> Key {
    (row[0].clone(), row[1].parse().unwrap())
}

fn load(forzatools: Option<std::path::PathBuf>, doliman: Option<std::path::PathBuf>) -> Oracles {
    let mut o = Oracles::default();
    if let Some(dir) = forzatools {
        let errors = tsv(&dir.join("errors.tsv"));
        assert!(
            errors.is_empty(),
            "ForzaTools raised on {} textures: {:?}",
            errors.len(),
            errors.first()
        );
        for r in tsv(&dir.join("pc.tsv")) {
            assert_eq!(r.len(), 16, "pc.tsv: {r:?}");
            o.pc.insert(key(&r), r[3..].to_vec());
        }
        for r in tsv(&dir.join("durango.tsv")) {
            assert_eq!(r.len(), 20, "durango.tsv: {r:?}");
            o.durango.insert(key(&r), r[3..].to_vec());
        }
        for r in tsv(&dir.join("dds.tsv")) {
            assert_eq!(r.len(), 8, "dds.tsv: {r:?}");
            o.dds.entry(key(&r)).or_default().push(r[3..].to_vec());
        }
    }
    if let Some(dir) = doliman {
        for e in tsv(&dir.join("errors.tsv")) {
            assert_eq!(e.len(), 4, "errors.tsv: {e:?}");
            o.doliman_errors.insert(key(&e), e[3].clone());
        }
        for r in tsv(&dir.join("textures.tsv")) {
            assert_eq!(r.len(), 12, "textures.tsv: {r:?}");
            o.doliman.insert(key(&r), r[3..].to_vec());
        }
    }
    o
}

/// The GUID of the textures whose header holds none of its own, as `guid_string` prints it.
const PLACEHOLDER_GUID: &str = "{00000000-0000-0000-0000-000000000001}";

/// The GUID three `test*_diff` textures (the engine's default and a cinematic library's) hold in place of their own.
const TEST_GUID: &str = "{25C29A40-AA32-4334-933F-DC8EB3D663A0}";

/// What the test counts.
#[derive(Default)]
struct Census {
    read: usize,
    pc: usize,
    durango: usize,
    by_source: BTreeMap<String, usize>,
    /// By where the entry is (a GeoChunk or a zip) and its name's extension.
    by_kind: BTreeMap<String, usize>,
    by_encoding: BTreeMap<String, usize>,
    by_flags: BTreeMap<String, usize>,
    by_field_34: BTreeMap<u32, usize>,
    multi_slice: usize,
    deep: usize,
    srgb: usize,
    mips: usize,
    compared_forzatools: usize,
    compared_durango: usize,
    compared_doliman: usize,
    doliman_raised: usize,
    compared_mips: usize,
    dds_deep: usize,
    doliman_format_unknown: usize,
    guid_in_name: usize,
    guid_in_name_matches: usize,
    /// Of those, the header holds the placeholder GUID 1 or the `test*_diff` GUID instead.
    guid_in_name_placeholder: usize,
}

/// The DXGI format ForzaTools' `DetermineFormat` gives: the encoding's, or its sRGB one for colour profile 1, 0 where
/// the encoding has none.
fn forzatools_format(e: Encoding, profile: u32) -> u32 {
    let (linear, srgb) = e.dxgi();
    if profile == 0 { linear } else { srgb.unwrap_or(0) }
}

/// A GUID as Python's `UUID(bytes_le=...)` prints it in braces, uppercase (the first three groups byte-swapped).
fn guid_string(g: &[u8; 16]) -> String {
    format!(
        "{{{}-{}-{}-{}-{}}}",
        hex(&[g[3], g[2], g[1], g[0]]),
        hex(&[g[5], g[4]]),
        hex(&[g[7], g[6]]),
        hex(&g[8..10]),
        hex(&g[10..])
    )
    .to_ascii_uppercase()
}

/// The GUID a file's name holds (the part after the last separator), as `guid_string` writes one but for the braces
/// and case: the last run of 36 characters of hexadecimal digits in groups of 8-4-4-4-12 joined by `-` (a name can
/// hold two, and the header's is the last).
fn name_guid(name: &str) -> Option<String> {
    let file = name.rsplit(['/', '\\']).next().unwrap_or(name);
    let b = file.as_bytes();
    (0..b.len().saturating_sub(35)).rev().find_map(|i| {
        let w = &b[i..i + 36];
        let ok = w.iter().enumerate().all(|(k, c)| {
            if matches!(k, 8 | 13 | 18 | 23) {
                *c == b'-'
            } else {
                c.is_ascii_hexdigit()
            }
        });
        ok.then(|| String::from_utf8_lossy(w).to_ascii_uppercase())
    })
}

/// ForzaTools' `Params`: it walks the header's linked list of values from the slice list, which gives the first slice's
/// encoding and then the first mip's size of every slice (the list continues through each slice's link).
fn params(h: &forzatech_formats::texture::PcHeader) -> String {
    let mut v = vec![h.slices[0].encoding.value().to_string()];
    v.extend(h.slices.iter().map(|s| s.mips[0].size.to_string()));
    v.join(",")
}

fn compare(census: &mut Census, o: &Oracles, source: &str, index: usize, name: &str, t: &Texture<'_>) {
    let k = (source.to_owned(), index);
    let at = format!("{source} entry {index} {name}");
    match &t.header {
        Header::Pc(h) => {
            let slice0 = &h.slices[0];
            if let Some(f) = o.pc.get(&k) {
                let ours = [
                    hex(&h.guid),
                    h.width.to_string(),
                    h.height.to_string(),
                    h.depth.to_string(),
                    h.slice_count.to_string(),
                    h.mip_count.to_string(),
                    h.flags.to_string(),
                    h.transcoding.to_string(),
                    h.colour_profile_2c.to_string(),
                    h.colour_profile.to_string(),
                    params(h),
                    forzatools_format(slice0.encoding, h.colour_profile).to_string(),
                    t.data.len().to_string(),
                ];
                assert_eq!(f, &ours, "{at}: ForzaTools' PC header fields");
                census.compared_forzatools += 1;
            }
            // BCnEncoder takes a mip's size from the format alone, so a texture of depth over 1 (whose mips hold
            // `depth` layers each) is not comparable past its first mip
            if let Some(rows) = o.dds.get(&k)
                && h.depth > 1
            {
                assert_eq!(rows.len(), slice0.mips.len(), "{at}: mips in the DDS");
                census.dds_deep += 1;
            } else if let Some(rows) = o.dds.get(&k) {
                assert_eq!(rows.len(), slice0.mips.len(), "{at}: mips in the DDS");
                for (m, r) in rows.iter().enumerate() {
                    assert_eq!(r[0], m.to_string(), "{at}: DDS mip order");
                    let (w, hh) = h.mip_size(u32::try_from(m).unwrap());
                    let bytes = t.mip(0, m).unwrap();
                    assert_eq!(
                        r[1..],
                        [
                            w.to_string(),
                            hh.to_string(),
                            bytes.len().to_string(),
                            sha256(bytes)
                        ],
                        "{at}: mip {m} of the DDS ForzaTools.TexConv writes and BCnEncoder reads"
                    );
                    census.compared_mips += 1;
                }
            }
            assert!(
                !o.doliman_errors.contains_key(&k),
                "{at}: Doliman's library raised on a PC texture"
            );
            if let Some(d) = o.doliman.get(&k)
                && h.slice_count == 1
            {
                let ours = [
                    guid_string(&h.guid),
                    h.width.to_string(),
                    h.height.to_string(),
                    h.mip_count.to_string(),
                    slice0.mips[0].size.to_string(),
                ];
                assert_eq!(
                    &d[..5],
                    &ours,
                    "{at}: Doliman's GUID, dimensions, mip count and linear size"
                );
                assert_eq!(d[6], t.data.len().to_string(), "{at}: Doliman's payload size");
                assert_eq!(d[7], sha256(t.data), "{at}: Doliman's payload");
                // Doliman knows the encodings 0-9 and 13; for the others it prints a warning and writes format 0
                if matches!(slice0.encoding.value(), 0..=9 | 13) {
                    let format = forzatools_format(slice0.encoding, h.colour_profile).to_string();
                    assert_eq!(d[5], format, "{at}: Doliman's format");
                    assert_eq!(d[8], "0", "{at}: Doliman warned about an encoding it knows");
                } else {
                    assert_eq!(
                        d[5], "0",
                        "{at}: Doliman's format for an encoding it does not know"
                    );
                    assert_eq!(
                        d[8], "1",
                        "{at}: Doliman's warning for an encoding it does not know"
                    );
                    census.doliman_format_unknown += 1;
                }
                census.compared_doliman += 1;
            }
        }
        Header::Durango(h) => {
            // the library has no Durango reading: it raises on each, and lists none
            assert!(
                !o.doliman.contains_key(&k),
                "{at}: Doliman lists a Durango texture"
            );
            census.doliman_raised += usize::from(o.doliman_errors.contains_key(&k));
            if let Some(f) = o.durango.get(&k) {
                let e = Encoding::from_value(h.encoding_value()).unwrap();
                let ours = [
                    hex(&h.guid),
                    h.base_width.to_string(),
                    h.base_height.to_string(),
                    h.slice_count.to_string(),
                    h.full_width.to_string(),
                    h.full_height.to_string(),
                    h.field_26.to_string(),
                    h.mip_count.to_string(),
                    h.full_mip_count.to_string(),
                    h.base_mip.to_string(),
                    h.bits.to_string(),
                    h.tile_mode().to_string(),
                    h.encoding_value().to_string(),
                    h.transcoding().to_string(),
                    h.colour_profile().to_string(),
                    forzatools_format(e, h.colour_profile()).to_string(),
                    t.data.len().to_string(),
                ];
                assert_eq!(f, &ours, "{at}: ForzaTools' Durango header fields");
                census.compared_durango += 1;
            }
        }
    }
}

#[test]
fn every_texture_reads_as_forzatools_and_doliman_read_it() {
    let (Some(root), Some(track), Some(keys_path)) = (
        install(Game::Fh6),
        install_file(Game::Fh6, "media/Tracks/Brio"),
        keys(Game::Fh6),
    ) else {
        return;
    };
    let keys = Keys::parse(&std::fs::read_to_string(&keys_path).unwrap()).unwrap();
    let ft = oracle(OracleSet::ForzaTools, Game::Fh6, "texture");
    let dl = oracle(OracleSet::Doliman, Game::Fh6, "texmat");
    let (depth, seed) = (depth(), seed());
    println!(
        "depth {depth:?}, seed {seed}, forzatools {}, doliman {}",
        ft.is_some(),
        dl.is_some()
    );
    let oracles = load(ft, dl);
    let walk = Walk {
        track,
        media: root.join("media"),
        keys: &keys,
        depth,
        seed,
    };
    let mut census = Census::default();
    let mut seen: Vec<Key> = Vec::new();
    walk.each(
        |name| named(name, &[".pb", ".swatchbin"]),
        (2048, 256),
        |source, index, name, bytes| {
            let t = Texture::parse(bytes).unwrap_or_else(|e| panic!("{source} entry {index} {name}: {e}"));
            census.read += 1;
            *census.by_source.entry(source.to_owned()).or_default() += 1;
            let ext = if named(name, &[".pb"]) {
                ".pb"
            } else {
                ".swatchbin"
            };
            let place = if source.starts_with("GeoChunk") {
                "GeoChunk"
            } else {
                "zip"
            };
            *census.by_kind.entry(format!("{place} {ext}")).or_default() += 1;
            match &t.header {
                Header::Pc(h) => {
                    census.pc += 1;
                    *census
                        .by_encoding
                        .entry(format!("{:?}", h.slices[0].encoding))
                        .or_default() += 1;
                    *census
                        .by_flags
                        .entry(format!("flags {} slices {}", h.flags, h.slice_count))
                        .or_default() += 1;
                    *census.by_field_34.entry(h.field_34).or_default() += 1;
                    assert!(
                        h.flags & 1 == 0 || h.slice_count == 6,
                        "{source} entry {index} {name}: flag bit 0 on {} slices",
                        h.slice_count
                    );
                    assert!(
                        h.slice_count <= 128 && h.flags <= 6,
                        "{source} entry {index} {name}: slice count or flags past those seen"
                    );
                    census.multi_slice += usize::from(h.slice_count > 1);
                    census.deep += usize::from(h.depth > 1);
                    census.srgb += usize::from(h.srgb());
                    census.mips += h.slices.iter().map(|s| s.mips.len()).sum::<usize>();
                }
                Header::Durango(_) => census.durango += 1,
            }
            let guid = match &t.header {
                Header::Pc(h) => guid_string(&h.guid),
                Header::Durango(h) => guid_string(&h.guid),
            };
            if let Some(g) = name_guid(name) {
                census.guid_in_name += 1;
                let same = format!("{{{g}}}") == guid;
                census.guid_in_name_matches += usize::from(same);
                let placeholder = guid == PLACEHOLDER_GUID || guid == TEST_GUID;
                census.guid_in_name_placeholder += usize::from(!same && placeholder);
                if !same {
                    println!("GUID mismatch: {source} entry {index} {name}: header {guid}");
                }
            }
            compare(&mut census, &oracles, source, index, name, &t);
            seen.push((source.to_owned(), index));
        },
    );
    println!(
        "{} textures read ({} PC, {} Durango), {} mips in PC textures, {} of them arrays or cubes, {} of depth over 1, {} sRGB",
        census.read, census.pc, census.durango, census.mips, census.multi_slice, census.deep, census.srgb
    );
    println!(
        "compared: {} PC headers with ForzaTools, {} Durango headers with ForzaTools, {} mips with TexConv's DDS read by BCnEncoder (and {} textures of depth over 1 whose DDS mip count only), {} textures with Doliman ({} of them an encoding Doliman does not know), and Doliman raising on {} Durango textures",
        census.compared_forzatools,
        census.compared_durango,
        census.compared_mips,
        census.dds_deep,
        census.compared_doliman,
        census.doliman_format_unknown,
        census.doliman_raised
    );
    for (what, map) in [
        ("by source", &census.by_source),
        ("by container and extension", &census.by_kind),
        ("by encoding", &census.by_encoding),
        ("by flags", &census.by_flags),
    ] {
        println!("{what}:");
        for (k, v) in map {
            println!("  {v:>9}  {k}");
        }
    }
    println!("the u32 at 0x34: {:?}", census.by_field_34);
    println!(
        "{} file names hold a GUID, {} of them the header's and {} of the rest with the placeholder or test GUID",
        census.guid_in_name, census.guid_in_name_matches, census.guid_in_name_placeholder
    );
    assert_eq!(
        census.guid_in_name,
        census.guid_in_name_matches + census.guid_in_name_placeholder,
        "a file name's GUID is neither the header's nor one of the two shared GUIDs"
    );
    assert!(census.read > 0, "no texture read");
    if depth == Depth::Full {
        seen.sort();
        let listed = |m: &HashMap<Key, _>| {
            let mut v: Vec<Key> = m.keys().cloned().collect();
            v.sort();
            v
        };
        if !oracles.pc.is_empty() || !oracles.durango.is_empty() {
            let mut theirs = listed(&oracles.pc);
            theirs.extend(listed(&oracles.durango));
            theirs.sort();
            assert_eq!(seen, theirs, "the textures read, and those ForzaTools lists");
        }
        if !oracles.doliman.is_empty() {
            let mut theirs = listed(&oracles.doliman);
            theirs.extend(oracles.doliman_errors.keys().cloned());
            theirs.sort();
            assert_eq!(
                seen, theirs,
                "the textures read, and those Doliman lists or raises on"
            );
            assert_eq!(
                census.doliman_raised,
                oracles.doliman_errors.len(),
                "the Durango textures, and those Doliman raises on"
            );
        }
    }
}
