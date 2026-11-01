//! Reads a real install's track archives: every GeoChunk of Forza Horizon 6's Tokyo track (`media/Tracks/Brio`) with
//! its chunk map and contents list, from the install the `GAME_DATA` configuration names, and compares them with Nenkai
//! ForzaTools' reading of the same files (the oracle set `forzatools`, see tests/inputs/README.md). Without them each
//! test skips with a line naming its key.
//!
//! - Every archive's directory reads; its chunk map and contents list have one record per entry; every contents line's
//!   folder is the folder the archive's folder index puts the entry in. Entries decode: a sample drawn with the day's
//!   seed, or every entry (about 96 GB of stored bytes) with `[run] gate = 'full'`.
//! - Every entry's offset, sizes, method, padding, folder field, chunk-map record and name is what ForzaTools reads,
//!   every folder index value is ForzaTools', and each entry ForzaTools sampled decodes to the same SHA-256.
#![allow(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    reason = "a test reading the files you point it at"
)]

mod inputs;

use forzatech_formats::minizip::{ChunkMapRecord, Method, MiniZip, parse_chunk_map, parse_contents};
use inputs::{Game, OracleSet, install_file, oracle};
use primase_testdata::{Depth, depth, seed};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::fs::File;
use std::io::BufReader;
use std::path::Path;

const CHUNKS: std::ops::Range<u32> = 0..4;

struct Chunk {
    zip: MiniZip,
    file: BufReader<File>,
    map: Vec<ChunkMapRecord>,
    contents: String,
}

fn open(track: &Path, n: u32) -> Chunk {
    let path = track.join(format!("GeoChunk{n}.minizip"));
    let mut file = BufReader::with_capacity(1 << 20, File::open(&path).unwrap());
    let zip = MiniZip::read(&mut file).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    let map = std::fs::read(track.join(format!("ChunkMap{n}.dat"))).unwrap();
    let map = parse_chunk_map(&map, zip.header.entry_count).unwrap();
    let contents = std::fs::read_to_string(track.join(format!("ChunkContentsMiniZip{n}.txt"))).unwrap();
    Chunk {
        zip,
        file,
        map,
        contents,
    }
}

/// `count` entries of `pool` drawn with `seed` (splitmix64), in order; all of them when the pool is no larger.
fn sample(pool: &[usize], count: usize, seed: u64) -> Vec<usize> {
    if pool.len() <= count {
        return pool.to_vec();
    }
    let mut state = seed;
    let mut picked = BTreeSet::new();
    while picked.len() < count {
        state = state.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = state;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^= z >> 31;
        picked.insert(pool[(z % pool.len() as u64) as usize]);
    }
    picked.into_iter().collect()
}

#[test]
fn every_geochunk_reads_with_its_companions() {
    let Some(track) = install_file(Game::Fh6, "media/Tracks/Brio") else {
        return;
    };
    let (depth, seed) = (depth(), seed());
    println!("depth {depth:?}, seed {seed}");
    for n in CHUNKS {
        let mut c = open(&track, n);
        let lines = parse_contents(&c.contents).unwrap();
        let entries = c.zip.entries().len();
        assert_eq!(
            c.map.len(),
            entries,
            "GeoChunk{n}: one chunk map record per entry"
        );
        assert_eq!(lines.len(), entries, "GeoChunk{n}: one contents line per entry");
        for (k, line) in lines.iter().enumerate() {
            assert_eq!(
                c.zip.folder_of(k),
                Some(line.folder as usize),
                "GeoChunk{n} entry {k}: the contents line's folder is the folder index's"
            );
            assert_eq!(
                line.prezipped,
                line.zipcache_path().is_some(),
                "GeoChunk{n} entry {k}: a <PREZIPPED> line names a zipcache path, a bare line a bare name: {:?}",
                line.name
            );
        }
        let mut by_method: BTreeMap<Method, Vec<usize>> = BTreeMap::new();
        for (k, e) in c.zip.entries().iter().enumerate() {
            by_method.entry(e.method).or_default().push(k);
        }
        let mut decoded = 0usize;
        for (method, pool) in &by_method {
            let pick = match depth {
                Depth::Full => pool.clone(),
                Depth::Smoke => sample(
                    pool,
                    32,
                    seed ^ (u64::from(n) << 16) ^ (u64::from(method.number()) << 32),
                ),
            };
            for k in pick {
                let bytes = c
                    .zip
                    .read_entry(&mut c.file, k)
                    .unwrap_or_else(|e| panic!("GeoChunk{n} entry {k} (seed {seed}): {e}"));
                assert_eq!(bytes.len(), c.zip.entries()[k].size as usize);
                decoded += 1;
            }
        }
        let methods: Vec<String> = by_method
            .iter()
            .map(|(m, v)| format!("{m:?} {}", v.len()))
            .collect();
        println!(
            "GeoChunk{n}: {entries} entries in {} folders ({}), {decoded} decoded",
            c.zip.folder_count(),
            methods.join(", ")
        );
    }
}

/// A TSV file's rows after its header line, split at tabs.
fn tsv(path: &Path) -> Vec<Vec<String>> {
    let text = std::fs::read_to_string(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    text.lines()
        .skip(1)
        .map(|l| l.split('\t').map(str::to_owned).collect())
        .collect()
}

fn num(s: &str) -> u64 {
    s.parse().unwrap_or_else(|_| panic!("not a number: {s:?}"))
}

#[test]
fn geochunks_read_as_forzatools_reads_them() {
    let (Some(track), Some(dir)) = (
        install_file(Game::Fh6, "media/Tracks/Brio"),
        oracle(OracleSet::ForzaTools, Game::Fh6, "minizip"),
    ) else {
        return;
    };
    let headers = tsv(&dir.join("chunks.tsv"));
    let samples = tsv(&dir.join("samples.tsv"));
    let mut known = 0usize;
    for n in CHUNKS {
        let mut c = open(&track, n);
        let h = c.zip.header;
        let row = headers
            .iter()
            .find(|r| r[0] == n.to_string())
            .expect("a chunks.tsv row");
        let last_start = *c.zip.sub_chunk_starts().last().unwrap();
        assert_eq!(
            row[1..].iter().map(|s| num(s)).collect::<Vec<_>>(),
            [
                u64::from(h.version),
                u64::from(h.entry_count),
                u64::from(h.folder_count),
                u64::from(h.entries_per_sub_chunk),
                u64::from(h.sub_chunk_count),
                h.folder_index_len() / 4,
                last_start,
                c.zip.data_end() - last_start,
            ],
            "GeoChunk{n}: the header, folder index length, last sub-chunk start and end record"
        );

        let indices: Vec<u64> = std::fs::read_to_string(dir.join(format!("GeoChunk{n}.indices.tsv")))
            .unwrap()
            .lines()
            .map(num)
            .collect();
        let folders = c.zip.folder_count();
        for (f, &v) in indices.iter().enumerate() {
            let ours = match f {
                f if f < folders => c.zip.folder(f).unwrap().start as u64,
                f if f == folders => c.zip.entries().len() as u64,
                _ => 0,
            };
            assert_eq!(v, ours, "GeoChunk{n}: folder index value {f}");
        }

        let lines = parse_contents(&c.contents).unwrap();
        let rows = tsv(&dir.join(format!("GeoChunk{n}.entries.tsv")));
        assert_eq!(rows.len(), c.zip.entries().len(), "GeoChunk{n}: entry count");
        let per = u64::from(h.entries_per_sub_chunk);
        let last = rows.len() - 1;
        for (k, (row, e)) in rows.iter().zip(c.zip.entries()).enumerate() {
            let m = c.map[k];
            // ForzaTools sizes only compressed entries, and never sets its end record's offset, so a compressed last
            // entry's size wraps: a known difference, counted
            let compressed = match (e.method, k == last) {
                (Method::Stored, _) => 0,
                (_, true) => {
                    known += 1;
                    num(&row[3])
                }
                _ => e.stored_size,
            };
            let ours = [
                k as u64,
                k as u64 / per,
                e.offset,
                compressed,
                u64::from(e.size),
                u64::from(e.method.number()),
                u64::from(e.padding),
                u64::from(e.folder_low),
                u64::from(m.id),
                u64::from(m.resource_type),
            ];
            // ForzaTools reads the chunk map record as an i16 and two bytes; the id is the i16's bits and the first byte
            let low = row[8].parse::<i16>().unwrap().cast_unsigned();
            let id = u64::from(low) | num(&row[9]) << 16;
            let theirs: Vec<u64> = row[..8]
                .iter()
                .map(|s| num(s))
                .chain([id, num(&row[10])])
                .collect();
            assert_eq!(theirs, ours, "GeoChunk{n} entry {k}");
            // ForzaTools' name: the line before its '|', every "<PREZIPPED>" and "d:\" removed
            let name = lines[k].name.replace("d:\\", "");
            assert_eq!(row[11], name, "GeoChunk{n} entry {k}: name");
        }

        let mut checked = 0usize;
        for s in samples.iter().filter(|s| s[0] == n.to_string()) {
            let k = num(&s[1]) as usize;
            if s[4].starts_with("error:") {
                assert_eq!(
                    k, last,
                    "GeoChunk{n} entry {k}: ForzaTools fails only on the last entry, whose size it wraps"
                );
                known += 1;
                continue;
            }
            let bytes = c.zip.read_entry(&mut c.file, k).unwrap();
            let hash: String = Sha256::digest(&bytes)
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect();
            assert_eq!(
                (bytes.len() as u64, hash.as_str()),
                (num(&s[3]), s[4].as_str()),
                "GeoChunk{n} entry {k} (method {} by {})",
                s[2],
                s[5]
            );
            checked += 1;
        }
        println!(
            "GeoChunk{n}: {} entries and {} folder index values as ForzaTools reads them; {checked} sampled entries \
             decode to its SHA-256",
            rows.len(),
            indices.len()
        );
    }
    println!(
        "known differences: {known} (ForzaTools' size of a compressed last entry, and its failure to read it)"
    );
}
