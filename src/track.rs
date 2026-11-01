//! Where a track's resources are: the index the other world files name GeoChunk entries by, built from each
//! GeoChunk's chunk map and contents list ([`crate::minizip::parse_chunk_map`], [`crate::minizip::parse_contents`]).
//!
//! Two ways in:
//!
//! - By chunk map reference (`type << 24 | id`, [`ChunkMapRecord::reference`]): a `.pvs` model number is the id of a
//!   type-4 record ([`Track::pvs_model`]), a `.pvs` cell record's second reference a type-12 (`.pvsz`) record.
//! - By model path: a template library's model (`tracks\brio\scene\models\...\<name>.max`, [`crate::plib`]) names the
//!   `.modelbin` entries `...\zipcache\pc\tracks\brio\scene\models\...\<name>_cluster<NNN>.<k>.modelbin`, one per
//!   cluster ([`Track::template_models`]). The contents lists are lower case and the libraries keep their authors'
//!   case, so the path is compared with ASCII case ignored.
//!
//! [`Track::new`] refuses a chunk map and contents list of different lengths, a reference in two records, and a
//! type-4 record whose name is not a `.modelbin` of that form. A reference or path with no entry is `None` or empty:
//! the files do not hold one.
//!
//! What has been checked, and how: `tests/pvs.rs` and `tests/plib.rs` build the index of the Forza Horizon 6 Tokyo
//! track's four GeoChunks (495,394 entries, no reference in two records) and count what the `.pvs` and the template
//! libraries name that it holds and does not.
#![deny(
    clippy::arithmetic_side_effects,
    clippy::indexing_slicing,
    clippy::cast_possible_truncation
)]

use crate::minizip::{ChunkMapRecord, ContentsLine};
use crate::{Error, Result};
use std::collections::{BTreeSet, HashMap};

/// The chunk map type of a `.modelbin`.
pub const MODELBIN: u8 = 4;
/// The chunk map type of a `.pgeo`.
pub const PGEO: u8 = 0;
/// The chunk map type of a `.pvsz`.
pub const PVSZ: u8 = 12;

/// A GeoChunk entry.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Location {
    /// The GeoChunk's number (`GeoChunk<chunk>.minizip`).
    pub chunk: usize,
    /// The entry's index in it.
    pub entry: usize,
}

/// A `.modelbin` entry and what its name says.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ModelEntry {
    /// Where it is.
    pub location: Location,
    /// Its chunk map id.
    pub id: u32,
    /// The cluster number of its name (`_cluster<NNN>`).
    pub cluster: u16,
    /// The letter before `.modelbin` (`i` or `a` on the Tokyo track).
    pub kind: char,
}

/// The index of a track's GeoChunk entries.
#[derive(Clone, Debug, Default)]
pub struct Track {
    references: HashMap<u32, Location>,
    models: HashMap<u32, ModelEntry>,
    by_path: HashMap<String, BTreeSet<u32>>,
}

fn fail(what: String) -> Error {
    Error::Track(what)
}

/// A `.modelbin` name's zipcache path split into the model path (lower case), the cluster and the letter.
fn split_modelbin(path: &str) -> Option<(String, u16, char)> {
    let rest = path.strip_suffix(".modelbin")?;
    let (rest, kind) = rest.rsplit_once('.')?;
    let mut letters = kind.chars();
    let (Some(kind), None) = (letters.next(), letters.next()) else {
        return None;
    };
    let (model, cluster) = rest.rsplit_once("_cluster")?;
    if cluster.len() != 3 || !cluster.bytes().all(|b| b.is_ascii_digit()) || model.is_empty() {
        return None;
    }
    Some((model.to_ascii_lowercase(), cluster.parse().ok()?, kind))
}

/// A template path's model path: the path less the extension of its last component, lower case.
fn model_path(path: &str) -> String {
    let name_at = path.rfind('\\').map_or(0, |i| i.saturating_add(1));
    let stem = match path.get(name_at..).and_then(|name| name.rfind('.')) {
        Some(dot) => path.get(..name_at.saturating_add(dot)).unwrap_or(path),
        None => path,
    };
    stem.to_ascii_lowercase()
}

impl Track {
    /// The index of the GeoChunks given in order (chunk 0 first), each as its chunk map and contents list.
    pub fn new<'a, 'b: 'a, I>(chunks: I) -> Result<Track>
    where
        I: IntoIterator<Item = (&'a [ChunkMapRecord], &'a [ContentsLine<'b>])>,
    {
        let mut track = Track::default();
        for (chunk, (map, lines)) in chunks.into_iter().enumerate() {
            if map.len() != lines.len() {
                return Err(fail(format!(
                    "GeoChunk{chunk}: {} chunk map records and {} contents lines",
                    map.len(),
                    lines.len()
                )));
            }
            for (entry, (record, line)) in map.iter().zip(lines).enumerate() {
                let location = Location { chunk, entry };
                if let Some(first) = track.references.insert(record.reference(), location) {
                    return Err(fail(format!(
                        "GeoChunk{chunk} entry {entry}: reference {:#010x} is also GeoChunk{} entry {}'s",
                        record.reference(),
                        first.chunk,
                        first.entry
                    )));
                }
                if record.resource_type != MODELBIN {
                    continue;
                }
                let Some((path, cluster, kind)) = line.zipcache_path().and_then(split_modelbin) else {
                    return Err(fail(format!(
                        "GeoChunk{chunk} entry {entry}: type {MODELBIN}, but {:?} is not \
                         zipcache\\pc\\<path>_cluster<NNN>.<k>.modelbin",
                        line.name
                    )));
                };
                track.models.insert(
                    record.id,
                    ModelEntry {
                        location,
                        id: record.id,
                        cluster,
                        kind,
                    },
                );
                track.by_path.entry(path).or_default().insert(record.id);
            }
        }
        Ok(track)
    }

    /// The entry a chunk map reference (`type << 24 | id`) names.
    pub fn reference(&self, reference: u32) -> Option<Location> {
        self.references.get(&reference).copied()
    }

    /// The entry of a resource type and id.
    pub fn entry(&self, resource_type: u8, id: u32) -> Option<Location> {
        let record = ChunkMapRecord { id, resource_type };
        (id < 1 << 24)
            .then(|| self.reference(record.reference()))
            .flatten()
    }

    /// The `.modelbin` a `.pvs` model number names.
    pub fn pvs_model(&self, model: u32) -> Option<&ModelEntry> {
        self.models.get(&model)
    }

    /// The `.modelbin` entries of a template library's model path, in id order (one per cluster).
    pub fn template_models(&self, path: &str) -> impl Iterator<Item = &ModelEntry> + '_ {
        self.by_path
            .get(&model_path(path))
            .into_iter()
            .flatten()
            .filter_map(|id| self.models.get(id))
    }

    /// How many entries the index holds.
    pub fn len(&self) -> usize {
        self.references.len()
    }

    /// Whether it holds none.
    pub fn is_empty(&self) -> bool {
        self.references.is_empty()
    }

    /// How many `.modelbin` entries.
    pub fn model_count(&self) -> usize {
        self.models.len()
    }
}

#[cfg(test)]
#[allow(clippy::indexing_slicing, reason = "tests on records built by hand")]
mod tests {
    use super::*;

    fn line(name: &str) -> ContentsLine<'_> {
        ContentsLine {
            prezipped: true,
            name,
            folder: 0,
        }
    }

    #[test]
    fn index() {
        let map = [
            ChunkMapRecord {
                id: 7,
                resource_type: MODELBIN,
            },
            ChunkMapRecord {
                id: 8,
                resource_type: MODELBIN,
            },
            ChunkMapRecord {
                id: 7,
                resource_type: PVSZ,
            },
        ];
        let lines = [
            line(r"d:\x\zipcache\pc\tracks\brio\scene\models\a\rock_a_cluster001.i.modelbin"),
            line(r"d:\x\zipcache\pc\tracks\brio\scene\models\a\rock_a_cluster000.a.modelbin"),
            line("__00007.pvsz"),
        ];
        let t = Track::new([(&map[..], &lines[..])]).unwrap();
        assert_eq!(t.len(), 3);
        assert_eq!(t.entry(PVSZ, 7), Some(Location { chunk: 0, entry: 2 }));
        assert_eq!(t.reference(0x0C00_0007), Some(Location { chunk: 0, entry: 2 }));
        assert_eq!(t.pvs_model(8).map(|m| (m.cluster, m.kind)), Some((0, 'a')));
        assert!(t.pvs_model(9).is_none());
        let found: Vec<_> = t
            .template_models(r"tracks\brio\scene\models\a\Rock_A.max")
            .map(|m| m.location.entry)
            .collect();
        assert_eq!(found, [0, 1]);
        assert_eq!(t.template_models("").count(), 0);
        // the same reference twice
        assert!(Track::new([(&map[..1], &lines[..1]), (&map[..1], &lines[..1])]).is_err());
        // a type-4 record that is not a modelbin
        assert!(Track::new([(&map[..1], &lines[2..])]).is_err());
        // lengths differ
        assert!(Track::new([(&map[..], &lines[..2])]).is_err());
    }

    #[test]
    fn paths() {
        assert_eq!(model_path(r"a\b.c\Name.max"), r"a\b.c\name");
        assert_eq!(model_path(r"a\b.c\name"), r"a\b.c\name");
        assert_eq!(
            split_modelbin("x_cluster012.i.modelbin"),
            Some(("x".into(), 12, 'i'))
        );
        assert_eq!(split_modelbin("x_cluster12.i.modelbin"), None);
        assert_eq!(split_modelbin("_cluster012.i.modelbin"), None);
        assert_eq!(split_modelbin("x_cluster012.ii.modelbin"), None);
    }
}
