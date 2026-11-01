//! Reads every Grub bundle of a real install: the entries of the Tokyo track's four GeoChunks
//! (`media/Tracks/Brio`) and of every zip under `media` (encrypted entries decrypted with the keys) whose first bytes
//! are `burG`, with every bundle nested in a blob, from the install the `GAME_DATA` configuration names, and compares
//! each with Doliman100 ForzaTech-extraction-tools' reading of the same bundle (the oracle set `doliman`, see
//! tests/inputs/README.md). Without the install or the keys the test skips with a line naming its key.
//!
//! - Every bundle reads with every check [`Bundle::parse`] makes holding, and so does every blob whose data is a
//!   bundle: a sample of entries drawn with the day's seed, or all of them with `[run] gate = 'full'`.
//! - Every bundle read is the one the oracle lists, and its blobs (tags, versions, metadata counts and offsets, data
//!   offsets and sizes) and metadata (tags, versions, payloads), and its nested bundles', give the oracle's reading;
//!   at full depth the oracle lists no bundle the test did not read.
#![allow(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    reason = "a test reading the files you point it at"
)]

mod inputs;

use forzatech_formats::bundle::{Bundle, SIZE_ESCAPE, Tag, is_bundle};
use forzatech_formats::keys::Keys;
use forzatech_formats::minizip::{MiniZip, parse_contents};
use forzatech_formats::zip::Zip;
use inputs::{Game, OracleSet, install, install_file, keys, oracle};
use primase_testdata::{Depth, depth, seed};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, HashMap};
use std::fmt::Write as _;
use std::fs::File;
use std::io::BufReader;
use std::path::{Path, PathBuf};

/// Every `.zip` under `dir`, sorted.
fn zips(dir: &Path) -> Vec<PathBuf> {
    let mut found = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        for entry in std::fs::read_dir(&d).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                stack.push(path);
            } else if path.extension().is_some_and(|e| e.eq_ignore_ascii_case("zip")) {
                found.push(path);
            }
        }
    }
    found.sort();
    found
}

/// Whether entry `k` of archive `a` is in a smoke sample drawn with `seed`: about one entry in `every`.
fn sampled(seed: u64, a: usize, k: usize, every: u64) -> bool {
    let mut z = seed ^ ((a as u64) << 32) ^ k as u64;
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    (z ^ (z >> 31)).is_multiple_of(every)
}

/// What the test counts over the bundles it reads.
#[derive(Default)]
struct Census {
    bundles: usize,
    nested: usize,
    by_extension: BTreeMap<String, usize>,
    blobs: BTreeMap<String, usize>,
    metadata: BTreeMap<String, usize>,
    /// Metadata payloads of 0xFFF bytes or more (their size escaped).
    escaped: usize,
    /// Bundles whose header ends where the metadata ends, unaligned.
    unaligned_header: usize,
    /// Bundles that end where their last blob's data ends, unaligned.
    unaligned_end: usize,
}

/// A bundle's reading in the oracle's line format (`dumps/doliman-bundle/bundle_dump.py`): the bundle, then its blobs
/// grouped by tag in first-seen order, each blob's metadata by tag in first-seen order with a repeated tag's last
/// record, then each nested bundle the same way. Counts what it reads into `census`.
fn reading(
    bundle: &Bundle<'_>,
    len: usize,
    path: &str,
    lines: &mut Vec<String>,
    census: &mut Census,
    bundles: &mut usize,
) {
    *bundles += 1;
    if !bundle.header_size.is_multiple_of(4) {
        census.unaligned_header += 1;
    }
    if !len.is_multiple_of(4) {
        census.unaligned_end += 1;
    }
    lines.push(format!(
        "B\t{path}\t{}.{}\t{}",
        bundle.version.major,
        bundle.version.minor,
        bundle.blobs.len()
    ));
    let mut tags: Vec<Tag> = Vec::new();
    for b in &bundle.blobs {
        if !tags.contains(&b.tag) {
            tags.push(b.tag);
        }
        *census
            .blobs
            .entry(format!("{} v{}", b.tag, b.version))
            .or_default() += 1;
        for m in &b.metadata {
            *census
                .metadata
                .entry(format!("{} on {} v{}", m.tag, b.tag, m.version))
                .or_default() += 1;
        }
    }
    let mut nested = Vec::new();
    for &tag in &tags {
        for (k, b) in bundle.tagged(tag).enumerate() {
            lines.push(format!(
                "b\t{path}\t{:08x}\t{k}\t{}.{}\t{}\t{}\t{}\t{}",
                tag.0,
                b.version.major,
                b.version.minor,
                b.metadata.len(),
                b.metadata_offset,
                b.data_offset,
                b.data.len()
            ));
            let mut seen: Vec<(Tag, usize)> = Vec::new();
            for (j, m) in b.metadata.iter().enumerate() {
                match seen.iter_mut().find(|(t, _)| *t == m.tag) {
                    Some(slot) => slot.1 = j,
                    None => seen.push((m.tag, j)),
                }
            }
            for (mtag, j) in seen {
                let m = &b.metadata[j];
                // The oracle reads a payload's size from the record's 12-bit field alone, so for an escaped size it
                // reads the first 0xFFF bytes of the payload.
                let shown = m.payload.get(..usize::from(SIZE_ESCAPE)).unwrap_or(m.payload);
                if shown.len() < m.payload.len() {
                    census.escaped += 1;
                }
                let mut hex = String::with_capacity(shown.len() * 2);
                for byte in shown {
                    write!(hex, "{byte:02x}").unwrap();
                }
                lines.push(format!(
                    "m\t{path}\t{:08x}\t{k}\t{:08x}\t{}\t{hex}",
                    tag.0, mtag.0, m.version
                ));
            }
            if is_bundle(b.data) {
                let inner = if path == "-" {
                    format!("{:08x}:{k}", tag.0)
                } else {
                    format!("{path}/{:08x}:{k}", tag.0)
                };
                nested.push((inner, b.data));
            }
        }
    }
    for (inner, data) in nested {
        let b = Bundle::parse(data).unwrap_or_else(|e| panic!("the bundle in blob {inner}: {e}"));
        census.nested += 1;
        reading(&b, data.len(), &inner, lines, census, bundles);
    }
}

/// A bundle's source (`GeoChunkN` or a zip's path under `media`) and entry index.
type Key = (String, usize);
/// The oracle's row for a bundle: name, its blob count, the bundles read, the SHA-256 of its reading.
type Row = (String, usize, usize, String);

/// The oracle's rows, and the readings of its sampled bundles line by line.
struct Oracle {
    rows: HashMap<Key, Row>,
    sample: HashMap<Key, Vec<String>>,
}

fn load_oracle(dir: &Path) -> Oracle {
    let text = std::fs::read_to_string(dir.join("errors.tsv")).unwrap();
    let errors: Vec<&str> = text.lines().skip(1).collect();
    assert!(
        errors.is_empty(),
        "the oracle raised on {} bundles: {:?}",
        errors.len(),
        &errors[..errors.len().min(5)]
    );
    let mut rows = HashMap::new();
    let text = std::fs::read_to_string(dir.join("bundles.tsv")).unwrap();
    for line in text.lines().skip(1) {
        let f: Vec<&str> = line.split('\t').collect();
        assert_eq!(f.len(), 6, "bundles.tsv: {line}");
        rows.insert(
            (f[0].to_owned(), f[1].parse().unwrap()),
            (
                f[2].to_owned(),
                f[3].parse().unwrap(),
                f[4].parse().unwrap(),
                f[5].to_owned(),
            ),
        );
    }
    let mut sample: HashMap<Key, Vec<String>> = HashMap::new();
    let text = std::fs::read_to_string(dir.join("sample.tsv")).unwrap();
    for line in text.lines().skip(1) {
        let mut f = line.splitn(3, '\t');
        let (source, index, rest) = (f.next().unwrap(), f.next().unwrap(), f.next().unwrap());
        sample
            .entry((source.to_owned(), index.parse().unwrap()))
            .or_default()
            .push(rest.to_owned());
    }
    Oracle { rows, sample }
}

struct Check {
    census: Census,
    oracle: Option<Oracle>,
    compared: usize,
}

impl Check {
    fn bundle(&mut self, source: &str, index: usize, name: &str, bytes: &[u8]) {
        let b = Bundle::parse(bytes).unwrap_or_else(|e| panic!("{source} entry {index} {name}: {e}"));
        self.census.bundles += 1;
        let ext = name.rsplit(['/', '\\']).next().unwrap_or(name);
        let ext = ext
            .rsplit_once('.')
            .map_or("(none)", |(_, e)| e)
            .to_ascii_lowercase();
        *self.census.by_extension.entry(ext).or_default() += 1;
        let (mut lines, mut count) = (Vec::new(), 0usize);
        reading(&b, bytes.len(), "-", &mut lines, &mut self.census, &mut count);
        let Some(Oracle { rows, sample }) = &mut self.oracle else {
            return;
        };
        let key = (source.to_owned(), index);
        let row = rows
            .remove(&key)
            .unwrap_or_else(|| panic!("{source} entry {index} {name}: a bundle the oracle does not list"));
        let sha: String = Sha256::digest(lines.join("\n").as_bytes())
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect();
        if sha != row.3 {
            let detail = match sample.get(&key) {
                Some(theirs) => {
                    let at = lines.iter().zip(theirs).position(|(a, b)| a != b);
                    match at {
                        Some(i) => format!(
                            "first difference at line {i}: ours {:?}, the oracle's {:?}",
                            lines[i], theirs[i]
                        ),
                        None => format!("{} lines, the oracle {}", lines.len(), theirs.len()),
                    }
                }
                None => "not in the oracle's sample".to_owned(),
            };
            panic!("{source} entry {index} {name}: the reading differs from the oracle's ({detail})");
        }
        assert_eq!(
            (name, b.blobs.len(), count),
            (row.0.as_str(), row.1, row.2),
            "{source} entry {index}: name, blob count, bundles read"
        );
        self.compared += 1;
    }
}

#[test]
fn every_bundle_reads_as_doliman_reads_it() {
    let (Some(root), Some(track), Some(keys_path)) = (
        install(Game::Fh6),
        install_file(Game::Fh6, "media/Tracks/Brio"),
        keys(Game::Fh6),
    ) else {
        return;
    };
    let keys = Keys::parse(&std::fs::read_to_string(&keys_path).unwrap()).unwrap();
    let oracle = oracle(OracleSet::Doliman, Game::Fh6, "bundle").map(|dir| load_oracle(&dir));
    let (depth, seed) = (depth(), seed());
    println!("depth {depth:?}, seed {seed}, oracle {}", oracle.is_some());
    let mut check = Check {
        census: Census::default(),
        oracle,
        compared: 0,
    };

    for n in 0..4usize {
        let mut f = BufReader::with_capacity(
            1 << 20,
            File::open(track.join(format!("GeoChunk{n}.minizip"))).unwrap(),
        );
        let zip = MiniZip::read(&mut f).unwrap();
        let text = std::fs::read_to_string(track.join(format!("ChunkContentsMiniZip{n}.txt"))).unwrap();
        let lines = parse_contents(&text).unwrap();
        assert_eq!(lines.len(), zip.entries().len());
        let source = format!("GeoChunk{n}");
        for (k, line) in lines.iter().enumerate() {
            if depth == Depth::Smoke && !sampled(seed, n, k, 512) {
                continue;
            }
            let bytes = zip.read_entry(&mut f, k).unwrap();
            if is_bundle(&bytes) {
                check.bundle(&source, k, line.name, &bytes);
            }
        }
    }
    let media = root.join("media");
    for (a, path) in zips(&media).into_iter().enumerate() {
        let source = path
            .strip_prefix(&media)
            .unwrap()
            .to_string_lossy()
            .replace('\\', "/");
        let mut f = BufReader::new(File::open(&path).unwrap());
        let zip = Zip::read(&mut f).unwrap();
        for (k, e) in zip.entries().iter().enumerate() {
            if e.name.ends_with('/') || (depth == Depth::Smoke && !sampled(seed, 4 + a, k, 128)) {
                continue;
            }
            let bytes = zip.read_entry(&mut f, k, Some(&keys)).unwrap();
            if is_bundle(&bytes) {
                check.bundle(&source, k, &e.name, &bytes);
            }
        }
    }

    let c = &check.census;
    println!(
        "{} bundles read ({} nested in blobs), {} compared with the oracle",
        c.bundles, c.nested, check.compared
    );
    println!(
        "{} metadata payloads with an escaped size; {} bundles whose header ends unaligned, {} that end unaligned",
        c.escaped, c.unaligned_header, c.unaligned_end
    );
    for (what, map) in [
        ("by extension", &c.by_extension),
        ("blobs", &c.blobs),
        ("metadata", &c.metadata),
    ] {
        println!("{what}:");
        for (k, v) in map {
            println!("  {v:>9}  {k}");
        }
    }
    assert!(c.bundles > 0, "no bundle read");
    if let (Depth::Full, Some(Oracle { rows, .. })) = (depth, &check.oracle) {
        let left: Vec<_> = rows.keys().take(5).collect();
        assert!(
            rows.is_empty(),
            "{} bundles the oracle lists were not read: {left:?}",
            rows.len()
        );
    }
}
