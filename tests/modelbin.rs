//! Reads every model (`.modelbin`) of a real install: the entries of the Tokyo track's four GeoChunks
//! (`media/Tracks/Brio`) and of every zip under `media` (encrypted entries decrypted with the keys), from the install
//! the `GAME_DATA` configuration names, and compares the Tokyo track's models with two third-party readings of the
//! same files (the oracle sets `forzatools` and `doliman`, see tests/inputs/README.md). Without the install or the keys
//! the test skips with a line naming its key.
//!
//! - Every model reads with every check [`Model::parse`] makes holding, and every mesh of it decodes
//!   ([`Model::mesh_data`]: its indices inside the index buffer, its vertices inside the vertex buffers, every
//!   element in a format the vertex decoder lists): a sample of entries drawn with the day's seed, or all of them with
//!   `[run] gate = 'full'`.
//! - Doliman100's importer reads each Tokyo model with its own classes and builds its meshes. Per model its counts,
//!   level-of-detail flags and bone count equal ours; per mesh every field its `Mesh` reads equals ours, and what it
//!   builds (a vertex count, a face count, the hash of its faces, and the minimum, maximum and sum of every position,
//!   normal, texture coordinate set and colour) equals what the importer's own arithmetic (written out below) gives
//!   from our decoded arrays; every vertex of every 500th model is compared one by one.
//! - ForzaTools reads each Tokyo model with `Bundle.Load`. Per model its blob list, the model header's eight u16 and
//!   bounds, every vertex layout, every buffer's header and the hash of its elements, the skeleton, every material
//!   instance's name, id and bundle, and the leading fields of every version 1.12 mesh equal ours; the tags its
//!   classes do not know (kept as raw bytes by the patch the harness is built with) are those we expect it not to
//!   know. The fields of `MeshBlob` from the morph count on are not compared: it stores that count in one byte, the
//!   model's meshes in four.
#![allow(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::arithmetic_side_effects,
    reason = "a test reading the files you point it at"
)]

mod inputs;

use forzatech_formats::bundle::{Bundle, Tag, is_bundle};
use forzatech_formats::keys::Keys;
use forzatech_formats::minizip::{MiniZip, parse_contents};
use forzatech_formats::modelbin::{Material, MeshData, Model};
use forzatech_formats::zip::Zip;
use inputs::{Game, OracleSet, install, install_file, keys, oracle};
use primase_testdata::{Depth, depth, seed};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, HashMap};
use std::fs::File;
use std::io::{BufRead, BufReader};
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

fn is_model(name: &str) -> bool {
    name.to_ascii_lowercase().ends_with(".modelbin")
}

fn sha_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes).iter().map(|b| format!("{b:02x}")).collect()
}

/// A model's source (`GeoChunkN` or a zip's path under `media`) and entry index: the oracles' key.
type Key = (String, usize);

/// One oracle TSV file whose rows are in the order the test visits the models: read as the test asks for each model's
/// rows, so a file of hundreds of megabytes is never held whole.
struct Table {
    file: String,
    lines: std::io::Lines<BufReader<File>>,
    cols: HashMap<String, usize>,
    peeked: Option<(Key, Vec<String>)>,
    /// Rows whose model the test did not ask for (every one is a model it did not read, or read out of order).
    skipped: usize,
    taken: usize,
}

impl Table {
    fn open(dir: &Path, file: &str) -> Table {
        let mut lines = BufReader::with_capacity(1 << 20, File::open(dir.join(file)).unwrap()).lines();
        let header = lines.next().unwrap().unwrap();
        let cols = header
            .split('\t')
            .enumerate()
            .map(|(i, c)| (c.to_owned(), i))
            .collect();
        let mut t = Table {
            file: file.to_owned(),
            lines,
            cols,
            peeked: None,
            skipped: 0,
            taken: 0,
        };
        t.fill();
        t
    }

    fn fill(&mut self) {
        if self.peeked.is_none()
            && let Some(line) = self.lines.next()
        {
            let row: Vec<String> = line.unwrap().split('\t').map(str::to_owned).collect();
            assert_eq!(
                row.len(),
                self.cols.len(),
                "{}: a row of {} fields: {row:?}",
                self.file,
                row.len()
            );
            self.peeked = Some(((row[0].clone(), row[1].parse().unwrap()), row));
        }
    }

    /// The rows of model `key`, in file order; rows of models before it are skipped (and counted).
    fn take(&mut self, key: &Key) -> Vec<Vec<String>> {
        let mut out = Vec::new();
        loop {
            self.fill();
            match &self.peeked {
                Some((k, _)) if k < key => {
                    self.skipped += 1;
                    self.peeked = None;
                }
                Some((k, _)) if k == key => {
                    out.push(self.peeked.take().unwrap().1);
                    self.taken += 1;
                }
                _ => return out,
            }
        }
    }

    fn col(&self, row: &[String], name: &str) -> String {
        let i = *self
            .cols
            .get(name)
            .unwrap_or_else(|| panic!("{}: no column {name}", self.file));
        row[i].clone()
    }

    /// Rows left over after the last model asked for: at full depth there must be none.
    fn finish(mut self) -> (usize, usize) {
        let mut left = self.skipped;
        while self.peeked.is_some() || self.lines.next().is_some() {
            self.peeked = None;
            left += 1;
        }
        (self.taken, left)
    }
}

/// The oracle's `errors.tsv`: the models it raised on, with the exception.
fn errors(dir: &Path) -> HashMap<Key, String> {
    let text = std::fs::read_to_string(dir.join("errors.tsv")).unwrap();
    text.lines()
        .skip(1)
        .map(|l| {
            let f: Vec<&str> = l.split('\t').collect();
            assert_eq!(f.len(), 4, "errors.tsv: {l}");
            ((f[0].to_owned(), f[1].parse().unwrap()), f[3].to_owned())
        })
        .collect()
}

fn floats(s: &str) -> Vec<f64> {
    if s == "-" {
        return Vec::new();
    }
    s.split(',')
        .map(|x| x.parse().unwrap_or_else(|_| panic!("a number: {x:?}")))
        .collect()
}

fn hex_bits(f: f32) -> String {
    format!("{:08x}", f.to_bits())
}

/// The importer's arithmetic on one mesh's decoded arrays (`scripts/modelbin_importer.py`, the loop over a mesh's
/// vertices): a position is `snorm16 × scale + translate` through the mesh's bone's matrix, then `(-x, -z, y)`; a
/// normal is `(position w, snorm16, snorm16)` through the matrix's upper 3×3, normalised, then the same swap; a
/// texture coordinate is `(u × u scale + u offset, 1 - (v × v scale + v offset))`; a colour is its four unorm8 values.
/// All in double precision, as Python computes.
struct Built {
    positions: Vec<[f64; 3]>,
    normals: Option<Vec<[f64; 3]>>,
    uvs: [Option<Vec<[f64; 2]>>; 5],
    colors: Option<Vec<[f64; 4]>>,
    /// The mesh's position scale and offset and its bone's translation, summed in magnitude: the scale of the values
    /// the single-precision decode is compared at.
    magnitude: f64,
}

/// The skeleton's bone matrices as the importer composes them: each bone's matrix times its parent's (as the parent
/// stands at that point), in stored order.
fn bone_matrices(model: &Model<'_>) -> Vec<[[f64; 4]; 4]> {
    let mut bones: Vec<[[f64; 4]; 4]> = model
        .skeleton
        .bones
        .iter()
        .map(|b| {
            let mut m = [[0.; 4]; 4];
            for (j, row) in m.iter_mut().enumerate() {
                for (i, c) in row.iter_mut().enumerate() {
                    *c = f64::from(b.matrix[j * 4 + i]);
                }
            }
            m
        })
        .collect();
    for (k, bone) in model.skeleton.bones.iter().enumerate() {
        if let Ok(parent) = usize::try_from(bone.parent) {
            let (own, tr) = (bones[k], bones[parent]);
            let mut out = [[0.; 4]; 4];
            for i in 0..4 {
                for j in 0..4 {
                    for x in 0..4 {
                        out[i][j] += own[i][x] * tr[x][j];
                    }
                }
            }
            bones[k] = out;
        }
    }
    bones
}

fn build(bones: &[[[f64; 4]; 4]], data: &MeshData<'_>) -> Built {
    let mesh = data.mesh;
    let t = bones[usize::try_from(mesh.rigid_bone).unwrap()];
    let positions = data.positions().expect("POSITION0").unwrap();
    let raw = data.element("POSITION", 0).unwrap();
    let mut out_pos = Vec::new();
    let mut out_nrm = Vec::new();
    let normals = data.normals();
    for (i, p) in positions.iter().enumerate() {
        // the importer multiplies the decoded value by the scale in double precision
        let [sx, sy, sz, _] = mesh.position_scale;
        let [tx, ty, tz, _] = mesh.position_offset;
        let r = raw.values[i];
        let v = [
            f64::from(r[0]) * f64::from(sx) + f64::from(tx),
            f64::from(r[1]) * f64::from(sy) + f64::from(ty),
            f64::from(r[2]) * f64::from(sz) + f64::from(tz),
        ];
        assert!(
            p.iter()
                .zip(&v)
                .all(|(a, b)| (f64::from(*a) - b).abs() <= 1e-3 + 1e-6 * b.abs()),
            "the decoded position differs from its recomputation"
        );
        let mut v2 = [0.; 3];
        for j in 0..3 {
            v2[j] = v[0] * t[0][j] + v[1] * t[1][j] + v[2] * t[2][j] + t[3][j];
        }
        out_pos.push([-v2[0], -v2[2], v2[1]]);
        if let Some(n) = &normals {
            let n = n[i].map(f64::from);
            let mut n2 = [0.; 3];
            for j in 0..3 {
                n2[j] = n[0] * t[0][j] + n[1] * t[1][j] + n[2] * t[2][j];
            }
            let len = (n2[0] * n2[0] + n2[1] * n2[1] + n2[2] * n2[2]).sqrt();
            out_nrm.push([-n2[0] / len, -n2[2] / len, n2[1] / len]);
        }
    }
    let mut uvs: [Option<Vec<[f64; 2]>>; 5] = Default::default();
    for (set, slot) in uvs.iter_mut().enumerate() {
        *slot = data.uvs(u32::try_from(set).unwrap()).map(|v| {
            v.iter()
                .map(|[u, v]| [f64::from(*u), 1. - f64::from(*v)])
                .collect()
        });
    }
    let magnitude = 1.
        + mesh
            .position_scale
            .iter()
            .chain(&mesh.position_offset)
            .map(|x| f64::from(x.abs()))
            .sum::<f64>()
        + t.iter().flatten().map(|x| x.abs()).sum::<f64>()
        + mesh
            .uv_transforms
            .iter()
            .flatten()
            .map(|x| f64::from(x.abs()))
            .sum::<f64>();
    Built {
        positions: out_pos,
        normals: normals.map(|_| out_nrm),
        uvs,
        colors: data
            .colors()
            .map(|c| c.iter().map(|c| c.map(f64::from)).collect()),
        magnitude,
    }
}

/// Whether two numbers agree to the single-precision decode's accuracy at `scale`.
fn near(a: f64, b: f64, scale: f64) -> bool {
    (a - b).abs() <= 1e-6 * scale + 1e-9
}

/// Compares the oracle's `lo…, hi…, sum…` statistics of `width` components with those of `rows`.
fn stats(what: &str, oracle: &str, rows: &[Vec<f64>], width: usize, scale: f64) {
    let o = floats(oracle);
    assert_eq!(o.len(), 3 * width, "{what}: the oracle's statistics {oracle:?}");
    let n = rows.len() as f64;
    for c in 0..width {
        let column = rows.iter().map(|r| r[c]);
        let lo = column.clone().fold(f64::INFINITY, f64::min);
        let hi = column.clone().fold(f64::NEG_INFINITY, f64::max);
        let sum: f64 = column.clone().sum();
        let abs: f64 = column.map(f64::abs).sum();
        assert!(
            near(lo, o[c], scale),
            "{what}: component {c}'s minimum {lo}, the oracle's {}",
            o[c]
        );
        assert!(
            near(hi, o[width + c], scale),
            "{what}: component {c}'s maximum {hi}, the oracle's {}",
            o[width + c]
        );
        assert!(
            (sum - o[2 * width + c]).abs() <= n * 1e-6 * scale + 1e-6 * abs + 1e-9,
            "{what}: component {c}'s sum {sum}, the oracle's {}",
            o[2 * width + c]
        );
    }
}

/// What the test counts over the models it reads.
#[derive(Default)]
struct Census {
    models: usize,
    meshes: usize,
    indices: u64,
    vertices: u64,
    blobs: BTreeMap<String, usize>,
    formats: BTreeMap<String, usize>,
    empty_materials: usize,
    morph_meshes: usize,
    skinned_meshes: usize,
}

impl Census {
    /// Reads one model and decodes every mesh of it.
    fn read<'a>(&mut self, label: &str, bytes: &'a [u8]) -> Model<'a> {
        let bundle = Bundle::parse(bytes).unwrap_or_else(|e| panic!("{label}: {e}"));
        for b in &bundle.blobs {
            *self.blobs.entry(format!("{} v{}", b.tag, b.version)).or_default() += 1;
        }
        let model = Model::parse(bytes).unwrap_or_else(|e| panic!("{label}: {e}"));
        self.models += 1;
        self.empty_materials += model.materials.iter().filter(|m| m.instance.is_none()).count();
        for k in 0..model.meshes.len() {
            let d = model.mesh_data(k).unwrap_or_else(|e| panic!("{label}: {e}"));
            self.meshes += 1;
            self.indices += d.indices.len() as u64;
            self.vertices += d.vertex_count as u64;
            self.morph_meshes += usize::from(d.morph.is_some());
            self.skinned_meshes += usize::from(d.skin.is_some());
            for e in &d.elements {
                *self
                    .formats
                    .entry(format!(
                        "{}{} {:?}",
                        e.element.element.semantic, e.element.element.index, e.element.element.format
                    ))
                    .or_default() += 1;
            }
        }
        model
    }

    fn print(&self, what: &str) {
        println!(
            "{what}: {} models, {} meshes decoded ({} indices, {} vertices; {} with morph deltas, {} skinned), {} material slots without an instance",
            self.models,
            self.meshes,
            self.indices,
            self.vertices,
            self.morph_meshes,
            self.skinned_meshes,
            self.empty_materials
        );
        println!("blobs:");
        for (k, v) in &self.blobs {
            println!("  {v:>9}  {k}");
        }
        println!("elements:");
        for (k, v) in &self.formats {
            println!("  {v:>9}  {k}");
        }
    }
}

/// How much of the oracles the comparisons covered.
#[derive(Default)]
struct Compared {
    doliman_models: usize,
    doliman_meshes: usize,
    doliman_built: usize,
    doliman_sample_vertices: usize,
    doliman_errors: usize,
    doliman_warnings: usize,
    doliman_misread: usize,
    doliman_error_kinds: BTreeMap<String, usize>,
    doliman_unexplained: Vec<String>,
    forzatools_models: usize,
    forzatools_meshes: usize,
    forzatools_errors: usize,
    forzatools_error_kinds: BTreeMap<String, usize>,
    forzatools_opaque: BTreeMap<String, usize>,
}

struct Oracles {
    d: DolimanTables,
    f: ForzaToolsTables,
    compared: Compared,
}

struct DolimanTables {
    models: Table,
    meshes: Table,
    sample: Table,
    errors: HashMap<Key, String>,
}

struct ForzaToolsTables {
    models: Table,
    meshes: Table,
    layouts: Table,
    buffers: Table,
    skeletons: Table,
    morphs: Table,
    materials: Table,
    errors: HashMap<Key, String>,
}

/// Every vertex of the importer's `sample.tsv` rows of a model against our arrays.
fn compare_sample(rows: &[Vec<String>], t: &Table, built: &HashMap<usize, Built>, compared: &mut Compared) {
    for r in rows {
        let k: usize = t.col(r, "k").parse().unwrap();
        let v: usize = t.col(r, "vertex").parse().unwrap();
        let b = built
            .get(&k)
            .unwrap_or_else(|| panic!("sample row for mesh {k}, which the importer did not build"));
        let label = format!("sample mesh {k} vertex {v}");
        let p = floats(&t.col(r, "position"));
        for c in 0..3 {
            assert!(
                near(b.positions[v][c], p[c], b.magnitude),
                "{label}: position {:?}, the importer's {p:?}",
                b.positions[v]
            );
        }
        let n = floats(&t.col(r, "normal"));
        if let Some(ours) = &b.normals {
            for c in 0..3 {
                assert!(
                    near(ours[v][c], n[c], 1.),
                    "{label}: normal {:?}, the importer's {n:?}",
                    ours[v]
                );
            }
        }
        for (set, ours) in b.uvs.iter().enumerate() {
            let u = t.col(r, &format!("texcoord{set}"));
            match ours {
                Some(ours) => {
                    let u = floats(&u);
                    for c in 0..2 {
                        assert!(
                            near(ours[v][c], u[c], b.magnitude),
                            "{label}: uv set {set} {:?}, the importer's {u:?}",
                            ours[v]
                        );
                    }
                }
                None => assert_eq!(u, "-", "{label}: uv set {set}"),
            }
        }
        let c = t.col(r, "color0");
        match &b.colors {
            Some(ours) => {
                let c = floats(&c);
                for x in 0..4 {
                    assert!(
                        near(ours[v][x], c[x], 1.),
                        "{label}: colour {:?}, the importer's {c:?}",
                        ours[v]
                    );
                }
            }
            None => assert_eq!(c, "-", "{label}: colour"),
        }
        compared.doliman_sample_vertices += 1;
    }
}

fn compare_doliman(d: &mut DolimanTables, compared: &mut Compared, key: &Key, name: &str, model: &Model<'_>) {
    let label = format!("{} entry {} {name}", key.0, key.1);
    let model_rows = d.models.take(key);
    let mesh_rows = d.meshes.take(key);
    let sample_rows = d.sample.take(key);
    // The importer reads a version 1.13 mesh's material groups as one (it skips the group count and reads the first
    // group), so after a mesh with more groups it reads every field from the wrong place: it raises on the model
    // (running out of bytes) or builds from wrong values. Those models are not compared field by field; the importer
    // raising on any other model is a failure of this test.
    let misread = model.meshes.iter().any(|m| m.material_groups.len() != 1);
    if let Some(e) = d.errors.get(key) {
        assert!(
            model_rows.is_empty() && mesh_rows.is_empty(),
            "{label}: rows for a model the importer raised on ({e})"
        );
        let kind = e.split(':').next().unwrap_or_default().to_owned();
        // Every error the importer raised has to be explained by the file: by the misread above, or by a vertex whose
        // stored normal is (0, 0, 0) (it divides by the normal's length).
        let zero_normal = || {
            (0..model.meshes.len()).any(|k| {
                model
                    .mesh_data(k)
                    .ok()
                    .and_then(|d| d.normals())
                    .is_some_and(|n| n.contains(&[0.; 3]))
            })
        };
        let cause = if misread {
            Some("a model with several material groups")
        } else if kind == "ZeroDivisionError" && zero_normal() {
            Some("a model with a zero normal")
        } else {
            None
        };
        let Some(cause) = cause else {
            compared.doliman_unexplained.push(format!("{label}: {e}"));
            compared.doliman_errors += 1;
            return;
        };
        let kind = format!("{kind} on {cause}");
        *compared.doliman_error_kinds.entry(kind).or_default() += 1;
        compared.doliman_errors += 1;
        return;
    }
    if misread {
        compared.doliman_misread += 1;
        return;
    }
    assert_eq!(model_rows.len(), 1, "{label}: the importer's models.tsv rows");
    let m = &model_rows[0];
    let t = &d.models;
    let h = &model.header;
    let num = |name: &str| -> usize { t.col(m, name).parse().unwrap() };
    assert_eq!(t.col(m, "name"), name, "{label}: name");
    assert_eq!(
        [
            num("meshes"),
            num("buffers"),
            num("vertex_layouts"),
            num("materials")
        ],
        [h.mesh_count, h.buffer_count, h.layout_count, h.material_count].map(usize::from),
        "{label}: meshes, buffers, vertex layouts, materials"
    );
    assert_eq!(
        num("levels_of_detail"),
        usize::from(h.lod_flags),
        "{label}: levels of detail"
    );
    assert_eq!(
        num("decompress_flags"),
        usize::from(h.decompress_flags),
        "{label}: decompress flags"
    );
    assert_eq!(num("bones"), model.skeleton.bones.len(), "{label}: bones");
    let warnings = t.col(m, "warnings");
    if warnings != "-" {
        compared.doliman_warnings += 1;
        println!("{label}: the importer warns: {warnings}");
    }

    assert_eq!(
        mesh_rows.len(),
        model.meshes.len(),
        "{label}: the importer's mesh rows"
    );
    let bones = bone_matrices(model);
    let mt = &d.meshes;
    let mut built = HashMap::new();
    for (k, row) in mesh_rows.iter().enumerate() {
        let mesh = &model.meshes[k];
        let at = format!("{label} mesh {k} {:?}", mesh.name);
        let int = |name: &str| -> i64 { mt.col(row, name).parse().unwrap() };
        assert_eq!(int("k"), k as i64, "{at}: k");
        assert_eq!(mt.col(row, "name"), mesh.name, "{at}: name");
        // the importer keeps the second of the first group's four ids (summer) as the mesh's material
        assert_eq!(
            int("material_id"),
            i64::from(mesh.material_groups[0][1]),
            "{at}: material id"
        );
        assert_eq!(int("bone_index"), i64::from(mesh.rigid_bone), "{at}: bone");
        assert_eq!(
            int("levels_of_detail"),
            i64::from(mesh.lod_flags),
            "{at}: levels of detail"
        );
        assert_eq!(
            int("render_pass"),
            i64::from(mesh.bucket_flags),
            "{at}: render pass"
        );
        assert_eq!(
            int("skinning_elements_count"),
            i64::from(mesh.skinning_elements),
            "{at}: skinning elements"
        );
        assert_eq!(
            int("morph_weights_count"),
            i64::from(mesh.morph_targets),
            "{at}: morph targets"
        );
        assert_eq!(
            int("index_buffer_id"),
            i64::from(mesh.index_buffer),
            "{at}: index buffer"
        );
        assert_eq!(
            int("start_index_location"),
            i64::from(mesh.start_index),
            "{at}: start index"
        );
        assert_eq!(
            int("base_vertex_location"),
            i64::from(mesh.base_vertex),
            "{at}: base vertex"
        );
        assert_eq!(
            int("index_count"),
            i64::from(mesh.index_count),
            "{at}: index count"
        );
        assert_eq!(
            int("vertex_layout_id"),
            i64::from(mesh.layout),
            "{at}: vertex layout"
        );
        let mut vbs: Vec<_> = mesh.vertex_buffers.iter().collect();
        vbs.sort_by_key(|b| b.slot);
        let vbs: Vec<String> = vbs
            .iter()
            .map(|b| format!("{}:{}:{}:{}", b.slot, b.buffer, b.stride, b.offset))
            .collect();
        assert_eq!(
            mt.col(row, "vertex_buffers"),
            if vbs.is_empty() {
                "-".to_owned()
            } else {
                vbs.join(",")
            },
            "{at}: vertex buffers"
        );
        assert_eq!(
            int("morph_data_buffer_id"),
            i64::from(mesh.morph_buffer),
            "{at}: morph buffer"
        );
        assert_eq!(
            int("skinning_data_buffer_id"),
            i64::from(mesh.skinning_buffer),
            "{at}: skinning buffer"
        );
        assert_eq!(
            int("constant_buffers"),
            mesh.constant_buffers.len() as i64,
            "{at}: constant buffers"
        );
        let uv: Vec<f64> = mesh
            .uv_transforms
            .iter()
            .flatten()
            .map(|x| f64::from(*x))
            .collect();
        assert_eq!(floats(&mt.col(row, "uv_transforms")), uv, "{at}: uv transforms");
        let scale: Vec<f64> = mesh.position_scale.iter().map(|x| f64::from(*x)).collect();
        let translate: Vec<f64> = mesh.position_offset.iter().map(|x| f64::from(*x)).collect();
        assert_eq!(floats(&mt.col(row, "scale")), scale, "{at}: position scale");
        assert_eq!(
            floats(&mt.col(row, "translate")),
            translate,
            "{at}: position offset"
        );
        compared.doliman_meshes += 1;

        // the importer builds a mesh that has a level of detail and the 0x10 render pass bit
        let builds = mesh.lod_flags != 0 && mesh.bucket_flags & 0x10 != 0;
        if !builds {
            assert_eq!(mt.col(row, "built_name"), "-", "{at}: built");
            continue;
        }
        let data = model.mesh_data(k).unwrap();
        let b = build(&bones, &data);
        let material = model
            .material(mesh.material_groups[0][1])
            .map_or("", |m: &Material<'_>| m.name.as_str());
        assert_eq!(
            mt.col(row, "built_name"),
            format!("{} {material}", mesh.name),
            "{at}: the built mesh's name"
        );
        assert_eq!(int("vertices"), data.vertex_count as i64, "{at}: vertices");
        assert_eq!(int("faces"), i64::from(mesh.index_count / 3), "{at}: faces");
        let local = data.local_indices();
        let mut faces = String::new();
        for f in local.as_chunks::<3>().0 {
            faces.push_str(&format!("{} {} {}\n", f[0], f[2], f[1]));
        }
        assert_eq!(
            mt.col(row, "faces_sha256"),
            sha_hex(faces.as_bytes()),
            "{at}: the faces"
        );
        let mag = b.magnitude;
        let cols = |v: &[[f64; 3]]| -> Vec<Vec<f64>> { v.iter().map(|x| x.to_vec()).collect() };
        stats(
            &format!("{at}: positions"),
            &mt.col(row, "positions"),
            &cols(&b.positions),
            3,
            mag,
        );
        match &b.normals {
            Some(n) => stats(
                &format!("{at}: normals"),
                &mt.col(row, "normals"),
                &cols(n),
                3,
                1.,
            ),
            // without a NORMAL0 the importer leaves its starting value (1, 0, 0) in every vertex
            None => assert_eq!(floats(&mt.col(row, "normals")).len(), 9, "{at}: normals"),
        }
        let uv_text = mt.col(row, "texcoords");
        let mut theirs: BTreeMap<usize, String> = BTreeMap::new();
        if uv_text != "-" {
            for part in uv_text.split(';') {
                let (i, s) = part.split_once(':').unwrap();
                theirs.insert(i.parse().unwrap(), s.to_owned());
            }
        }
        for (set, ours) in b.uvs.iter().enumerate() {
            match (ours, theirs.remove(&set)) {
                (Some(ours), Some(s)) => {
                    let rows: Vec<Vec<f64>> = ours.iter().map(|x| x.to_vec()).collect();
                    stats(&format!("{at}: uv set {set}"), &s, &rows, 2, mag);
                }
                (None, None) => {}
                (o, t) => panic!("{at}: uv set {set}: ours {}, the importer's {t:?}", o.is_some()),
            }
        }
        assert!(
            theirs.is_empty(),
            "{at}: uv sets {theirs:?} the importer built and we have no element for"
        );
        match &b.colors {
            Some(c) => {
                let rows: Vec<Vec<f64>> = c.iter().map(|x| x.to_vec()).collect();
                stats(&format!("{at}: colours"), &mt.col(row, "colors"), &rows, 4, 1.);
            }
            None => assert_eq!(mt.col(row, "colors"), "-", "{at}: colours"),
        }
        built.insert(k, b);
        compared.doliman_built += 1;
    }
    assert_eq!(num("built"), built.len(), "{label}: meshes built");
    compare_sample(&sample_rows, &d.sample, &built, compared);
    compared.doliman_models += 1;
}

/// The tags ForzaTools' bundle classes have: any other is kept as raw bytes by the patch.
const KNOWN_BLOBS: [&[u8; 4]; 12] = [
    b"Skel", b"Mrph", b"MatI", b"MATI", b"MTPR", b"Mesh", b"IndB", b"VLay", b"VerB", b"MBuf", b"Modl",
    b"TXCB",
];
const KNOWN_METADATA: [&[u8; 4]; 5] = [b"Name", b"Id  ", b"ATST", b"BBox", b"TXCH"];

fn count(found: &mut BTreeMap<String, usize>, key: String) {
    *found.entry(key).or_default() += 1;
}

/// What ForzaTools reads as raw bytes in a bundle and the bundles nested in its `MatI` blobs.
fn opaque(bundle: &Bundle<'_>, nested: bool, found: &mut BTreeMap<String, usize>) {
    let at = if nested { " nested" } else { "" };
    for b in &bundle.blobs {
        if !KNOWN_BLOBS.iter().any(|t| b.tag == Tag::new(t)) {
            count(found, format!("blob {}{at}", b.tag));
        }
        for m in &b.metadata {
            if !KNOWN_METADATA.iter().any(|t| m.tag == Tag::new(t)) {
                count(found, format!("metadata {} on {}{at}", m.tag, b.tag));
            }
        }
        if b.tag == Tag::new(b"MatI") && is_bundle(b.data) {
            opaque(&Bundle::parse(b.data).unwrap(), true, found);
        }
    }
}

fn compare_forzatools(
    f: &mut ForzaToolsTables,
    compared: &mut Compared,
    key: &Key,
    name: &str,
    bytes: &[u8],
    model: &Model<'_>,
) {
    let label = format!("{} entry {} {name}", key.0, key.1);
    let rows = (
        f.models.take(key),
        f.meshes.take(key),
        f.layouts.take(key),
        f.buffers.take(key),
        f.skeletons.take(key),
        f.morphs.take(key),
        f.materials.take(key),
    );
    if let Some(e) = f.errors.get(key) {
        // a model the library throws on: nothing of it to compare
        assert!(
            rows.0.is_empty() && rows.1.is_empty(),
            "{label}: rows for a model the library threw on ({e})"
        );
        // What stops the library is one of two things: a mesh record laid out as its Forza Horizon 5 reader expects (it
        // stores the morph count in one byte, version 1.10 and later in four, so every field after it is misplaced),
        // or a material slot with no instance, which is not a bundle to load.
        let empty = model.materials.iter().any(|m| m.instance.is_none());
        if e.contains("Unexpected tag for bundle") {
            assert!(
                empty,
                "{label}: the library found no bundle in a model without an empty material slot: {e}"
            );
        } else {
            assert!(
                e.contains("MeshBlob.ReadBlobData"),
                "{label}: the library threw somewhere else than in a mesh: {e}"
            );
        }
        *compared
            .forzatools_error_kinds
            .entry(e.split(':').next().unwrap().to_owned())
            .or_default() += 1;
        compared.forzatools_errors += 1;
        return;
    }
    let bundle = Bundle::parse(bytes).unwrap();
    assert_eq!(rows.0.len(), 1, "{label}: the library's models.tsv rows");
    let (t, m) = (&f.models, &rows.0[0]);
    assert_eq!(t.col(m, "name"), name, "{label}: name");
    let blobs: Vec<String> = bundle
        .blobs
        .iter()
        .map(|b| format!("{}:{}", b.tag, b.version))
        .collect();
    assert_eq!(t.col(m, "blobs"), blobs.join(","), "{label}: blobs");
    let h = &model.header;
    let num = |c: &str| -> String { t.col(m, c) };
    assert_eq!(num("mesh_count"), h.mesh_count.to_string(), "{label}: mesh count");
    assert_eq!(
        num("buffers_count"),
        h.buffer_count.to_string(),
        "{label}: buffer count"
    );
    assert_eq!(
        num("vertex_layout_count"),
        h.layout_count.to_string(),
        "{label}: layout count"
    );
    assert_eq!(
        num("material_count"),
        h.material_count.to_string(),
        "{label}: material count"
    );
    // the library reads bytes 8 to 15 as four u16
    let u16s = [
        u16::from(h.has_lod) | (u16::from(h.min_lod as u8) << 8),
        u16::from(h.max_lod as u8),
        h.lod_flags,
        u16::from(h.decompress_flags),
    ];
    assert_eq!(
        [num("unk"), num("unk2"), num("unk3"), num("unk4")],
        u16s.map(|x| x.to_string()),
        "{label}: the model header's last four u16"
    );
    let b = h.bounds;
    let bbox: Vec<String> = b.min.iter().chain(&b.max).map(|x| hex_bits(*x)).collect();
    assert_eq!(num("bbox"), bbox.join(","), "{label}: bounds");
    let mut expected = BTreeMap::new();
    opaque(&bundle, false, &mut expected);
    let expected = if expected.is_empty() {
        "-".to_owned()
    } else {
        expected
            .iter()
            .map(|(k, v)| format!("{k} {v}"))
            .collect::<Vec<_>>()
            .join(";")
    };
    assert_eq!(
        num("opaque"),
        expected,
        "{label}: the tags the library has no class for"
    );
    for part in expected.split(';').filter(|p| *p != "-") {
        let (k, v) = part.rsplit_once(' ').unwrap();
        *compared.forzatools_opaque.entry(k.to_owned()).or_default() += v.parse::<usize>().unwrap();
    }

    // meshes: version 1.12 stores the fields as the library's leading reads expect them
    let mesh_blobs: Vec<_> = bundle.tagged(Tag::new(b"Mesh")).collect();
    assert_eq!(rows.1.len(), mesh_blobs.len(), "{label}: the library's mesh rows");
    for (k, row) in rows.1.iter().enumerate() {
        let (mesh, mt) = (&model.meshes[k], &f.meshes);
        let at = format!("{label} mesh {k} {:?}", mesh.name);
        assert_eq!(mt.col(row, "k"), k.to_string(), "{at}: k");
        assert_eq!(mt.col(row, "version"), mesh.version.to_string(), "{at}: version");
        assert_eq!(mt.col(row, "name"), mesh.name, "{at}: name");
        let b = mesh.bounds;
        let bbox: Vec<String> = b.min.iter().chain(&b.max).map(|x| hex_bits(*x)).collect();
        assert_eq!(mt.col(row, "bbox"), bbox.join(","), "{at}: bounds");
        compared.forzatools_meshes += 1;
        if mesh.version.to_string() != "1.12" {
            continue;
        }
        let g = mesh.material_groups[0];
        assert_eq!(
            mt.col(row, "unkv9"),
            format!("{},{},{},{}", g[0], g[1], g[2], g[3]),
            "{at}: material ids"
        );
        assert_eq!(
            mt.col(row, "unk1"),
            mesh.rigid_bone.to_string(),
            "{at}: rigid bone"
        );
        assert_eq!(
            mt.col(row, "unk2"),
            (mesh.lod_flags as i16).to_string(),
            "{at}: LOD flags"
        );
        assert_eq!(
            mt.col(row, "lod_level1"),
            mesh.lod_level1.to_string(),
            "{at}: LOD level 1"
        );
        assert_eq!(
            mt.col(row, "lod_level2"),
            mesh.lod_level2.to_string(),
            "{at}: LOD level 2"
        );
        assert_eq!(
            mt.col(row, "unk3"),
            (mesh.bucket_flags as i16).to_string(),
            "{at}: bucket flags"
        );
        assert_eq!(
            mt.col(row, "unk4"),
            mesh.bucket_order.to_string(),
            "{at}: bucket order"
        );
        assert_eq!(
            mt.col(row, "unkv2"),
            mesh.skinning_elements.to_string(),
            "{at}: skinning elements"
        );
        assert_eq!(
            mt.col(row, "unkv2_2"),
            (mesh.morph_targets & 0xFF).to_string(),
            "{at}: the morph count's first byte"
        );
    }

    // vertex layouts
    let lt = &f.layouts;
    assert_eq!(
        rows.2.len(),
        model.layouts.len(),
        "{label}: the library's layout rows"
    );
    for (k, (row, l)) in rows.2.iter().zip(&model.layouts).enumerate() {
        let at = format!("{label} layout {k}");
        assert_eq!(lt.col(row, "id"), l.id.to_string(), "{at}: id");
        assert_eq!(lt.col(row, "semantic_names"), l.names.join(","), "{at}: names");
        let elements: Vec<String> = l
            .elements
            .iter()
            .map(|e| {
                let name = l.names.iter().position(|n| *n == e.semantic).unwrap();
                let slot = i32::from(e.slot) | (i32::from(e.per_instance) << 16);
                format!(
                    "{name}:{}:{slot}:{}:{}:{}",
                    e.semantic_index, e.format, e.offset, e.step_rate as i32
                )
            })
            .collect();
        assert_eq!(lt.col(row, "elements"), elements.join(";"), "{at}: elements");
        let packed: Vec<String> = l.packed_formats.iter().map(u32::to_string).collect();
        assert_eq!(
            lt.col(row, "packed_formats"),
            packed.join(","),
            "{at}: packed formats"
        );
        assert_eq!(lt.col(row, "flags"), l.flags.to_string(), "{at}: flags");
    }

    // buffers, in the library's order: by blob
    let mut expected_buffers = Vec::new();
    let mut seen: HashMap<Tag, usize> = HashMap::new();
    for blob in &bundle.blobs {
        let list = match &blob.tag.0.to_be_bytes() {
            b"IndB" => &model.index_buffers,
            b"VerB" => &model.vertex_buffers,
            b"MBuf" => &model.morph_buffers,
            _ => continue,
        };
        let k = seen.entry(blob.tag).or_default();
        expected_buffers.push((blob.tag, *k, &list[*k]));
        *k += 1;
    }
    let bt = &f.buffers;
    assert_eq!(
        rows.3.len(),
        expected_buffers.len(),
        "{label}: the library's buffer rows"
    );
    for (row, (tag, k, buf)) in rows.3.iter().zip(&expected_buffers) {
        let at = format!("{label} {tag} {k}");
        assert_eq!(bt.col(row, "tag"), tag.to_string(), "{at}: tag");
        assert_eq!(bt.col(row, "k"), k.to_string(), "{at}: k");
        assert_eq!(bt.col(row, "id"), buf.id.to_string(), "{at}: id");
        assert_eq!(bt.col(row, "count"), buf.count.to_string(), "{at}: count");
        assert_eq!(
            bt.col(row, "buffer_width"),
            buf.stride.to_string(),
            "{at}: stride"
        );
        assert_eq!(
            bt.col(row, "num_elements"),
            buf.values.to_string(),
            "{at}: values"
        );
        assert_eq!(bt.col(row, "type"), buf.format.to_string(), "{at}: format");
        assert_eq!(
            bt.col(row, "data_sha256").to_ascii_lowercase(),
            sha_hex(buf.data),
            "{at}: elements"
        );
    }

    // skeleton
    assert_eq!(rows.4.len(), 1, "{label}: the library's skeleton rows");
    let sk = &model.skeleton;
    let bones: Vec<String> = sk
        .bones
        .iter()
        .map(|b| {
            let m: Vec<String> = b.matrix.iter().map(|x| hex_bits(*x)).collect();
            format!(
                "{}|{}|{}|{}|{}",
                b.name,
                b.parent,
                b.first_child,
                b.next_sibling,
                m.join(",")
            )
        })
        .collect();
    assert_eq!(
        f.skeletons.col(&rows.4[0], "bones"),
        bones.join(";"),
        "{label}: bones"
    );
    assert_eq!(
        f.skeletons.col(&rows.4[0], "unk"),
        (sk.trailing as i32).to_string(),
        "{label}: the u32 after the bones"
    );

    // morph target names: the library reads a count and one name
    if let Some(m) = &model.morph_targets {
        assert_eq!(rows.5.len(), 1, "{label}: the library's morph rows");
        assert_eq!(
            f.morphs.col(&rows.5[0], "unk"),
            m.names.len().to_string(),
            "{label}: morph count"
        );
        assert_eq!(
            f.morphs.col(&rows.5[0], "name"),
            m.names[0],
            "{label}: first morph name"
        );
    } else {
        assert!(rows.5.is_empty(), "{label}: morph rows for a model without Mrph");
    }

    // material instances
    assert_eq!(
        rows.6.len(),
        model.materials.len(),
        "{label}: the library's material rows"
    );
    for (k, (row, m)) in rows.6.iter().zip(&model.materials).enumerate() {
        let at = format!("{label} material {k}");
        assert_eq!(f.materials.col(row, "name"), m.name, "{at}: name");
        assert_eq!(f.materials.col(row, "id"), m.id.to_string(), "{at}: id");
        let inner = match &m.instance {
            Some(b) => {
                let tags: Vec<String> = b
                    .blobs
                    .iter()
                    .map(|x| format!("{}:{}", x.tag, x.version))
                    .collect();
                format!("{}:{}", b.version, tags.join(","))
            }
            None => "-".to_owned(),
        };
        assert_eq!(f.materials.col(row, "bundle"), inner, "{at}: material bundle");
    }
    compared.forzatools_models += 1;
}

#[test]
fn every_model_reads_as_the_oracles_read_it() {
    let (Some(root), Some(track), Some(keys_path)) = (
        install(Game::Fh6),
        install_file(Game::Fh6, "media/Tracks/Brio"),
        keys(Game::Fh6),
    ) else {
        return;
    };
    let keys = Keys::parse(&std::fs::read_to_string(&keys_path).unwrap()).unwrap();
    let mut oracles = match (
        oracle(OracleSet::Doliman, Game::Fh6, "modelbin"),
        oracle(OracleSet::ForzaTools, Game::Fh6, "modelbin"),
    ) {
        (Some(d), Some(f)) => Some(Oracles {
            d: DolimanTables {
                models: Table::open(&d, "models.tsv"),
                meshes: Table::open(&d, "meshes.tsv"),
                sample: Table::open(&d, "sample.tsv"),
                errors: errors(&d),
            },
            f: ForzaToolsTables {
                models: Table::open(&f, "models.tsv"),
                meshes: Table::open(&f, "meshes.tsv"),
                layouts: Table::open(&f, "layouts.tsv"),
                buffers: Table::open(&f, "buffers.tsv"),
                skeletons: Table::open(&f, "skeletons.tsv"),
                morphs: Table::open(&f, "morphs.tsv"),
                materials: Table::open(&f, "materials.tsv"),
                errors: errors(&f),
            },
            compared: Compared::default(),
        }),
        _ => None,
    };
    let (depth, seed) = (depth(), seed());
    println!("depth {depth:?}, seed {seed}, oracles {}", oracles.is_some());

    // the dumps keep every 500th model's vertices: the models the harness counted, in the feed's order
    const SAMPLE_EVERY: usize = 500;
    let mut tokyo = Census::default();
    let mut counted = 0usize;
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
            if !is_model(line.name) {
                continue;
            }
            let index = counted;
            counted += 1;
            if depth == Depth::Smoke && !sampled(seed, n, k, 512) && !index.is_multiple_of(SAMPLE_EVERY) {
                continue;
            }
            let bytes = zip.read_entry(&mut f, k).unwrap();
            assert!(
                is_bundle(&bytes),
                "{source} entry {k} {}: a model that is not a bundle",
                line.name
            );
            let label = format!("{source} entry {k} {}", line.name);
            let model = tokyo.read(&label, &bytes);
            if let Some(o) = &mut oracles {
                let key = (source.clone(), k);
                compare_doliman(&mut o.d, &mut o.compared, &key, line.name, &model);
                compare_forzatools(&mut o.f, &mut o.compared, &key, line.name, &bytes, &model);
            }
        }
    }
    tokyo.print("the Tokyo track");
    assert!(tokyo.models > 0, "no model read");
    if depth == Depth::Full {
        assert_eq!(tokyo.models, 113_868, "the Tokyo track's models");
    }

    if let Some(o) = oracles {
        let c = &o.compared;
        println!(
            "Doliman: {} models compared ({} meshes, {} built, {} vertices one by one), {} models it raised on, {} read from meshes with several material groups (not compared), {} with warnings",
            c.doliman_models,
            c.doliman_meshes,
            c.doliman_built,
            c.doliman_sample_vertices,
            c.doliman_errors,
            c.doliman_misread,
            c.doliman_warnings
        );
        for (k, v) in &c.doliman_error_kinds {
            println!("  {v:>9}  the importer raised {k}");
        }
        println!(
            "ForzaTools: {} models compared ({} meshes), {} models its classes throw on; opaque tags matched:",
            c.forzatools_models, c.forzatools_meshes, c.forzatools_errors
        );
        for (k, v) in &c.forzatools_error_kinds {
            println!("  {v:>9}  the library threw {k}");
        }
        for (k, v) in &c.forzatools_opaque {
            println!("  {v:>9}  {k}");
        }
        let tables = [
            ("doliman models", o.d.models.finish()),
            ("doliman meshes", o.d.meshes.finish()),
            ("doliman sample", o.d.sample.finish()),
            ("forzatools models", o.f.models.finish()),
            ("forzatools meshes", o.f.meshes.finish()),
            ("forzatools layouts", o.f.layouts.finish()),
            ("forzatools buffers", o.f.buffers.finish()),
            ("forzatools skeletons", o.f.skeletons.finish()),
            ("forzatools morphs", o.f.morphs.finish()),
            ("forzatools materials", o.f.materials.finish()),
        ];
        for (what, (taken, left)) in &tables {
            println!("{what}: {taken} rows compared, {left} not asked for");
        }
        assert!(
            c.doliman_unexplained.is_empty(),
            "the importer raised on {} models for no reason the file gives, e.g.:
{}",
            c.doliman_unexplained.len(),
            c.doliman_unexplained
                .iter()
                .take(10)
                .cloned()
                .collect::<Vec<_>>()
                .join(
                    "
"
                )
        );
        if depth == Depth::Full {
            for (what, (_, left)) in &tables {
                assert_eq!(*left, 0, "{what}: rows of models this test did not read");
            }
            assert_eq!(
                c.doliman_models + c.doliman_errors + c.doliman_misread,
                tokyo.models,
                "every model is one the importer read or raised on"
            );
            assert_eq!(
                c.forzatools_models + c.forzatools_errors,
                tokyo.models,
                "every model is one the library read or threw on"
            );
        }
    }

    // every model of every zip under the media folder: no oracle, the strict reader alone
    let media = root.join("media");
    let mut zipped = Census::default();
    for (a, path) in zips(&media).into_iter().enumerate() {
        let source = path
            .strip_prefix(&media)
            .unwrap()
            .to_string_lossy()
            .replace('\\', "/");
        let mut f = BufReader::new(File::open(&path).unwrap());
        let zip = Zip::read(&mut f).unwrap();
        for (k, e) in zip.entries().iter().enumerate() {
            if e.name.ends_with('/')
                || !is_model(&e.name)
                || (depth == Depth::Smoke && !sampled(seed, 4 + a, k, 64))
            {
                continue;
            }
            let bytes = zip.read_entry(&mut f, k, Some(&keys)).unwrap();
            zipped.read(&format!("{source} entry {k} {}", e.name), &bytes);
        }
    }
    zipped.print("the zips under media");
    if depth == Depth::Full {
        assert_eq!(zipped.models, 82_752, "the zips' models");
    }
}

/// Bytes of a real model changed one at a time never panic the reader: each is read or refused.
#[test]
fn changed_bytes_are_read_or_refused() {
    let (Some(track), Some(_)) = (install_file(Game::Fh6, "media/Tracks/Brio"), keys(Game::Fh6)) else {
        return;
    };
    let mut f = BufReader::with_capacity(1 << 20, File::open(track.join("GeoChunk0.minizip")).unwrap());
    let zip = MiniZip::read(&mut f).unwrap();
    let text = std::fs::read_to_string(track.join("ChunkContentsMiniZip0.txt")).unwrap();
    let lines = parse_contents(&text).unwrap();
    // the first model with a skin buffer is small enough to change every byte of
    let mut tried = 0;
    for (k, line) in lines.iter().enumerate().take(400) {
        if !is_model(line.name) {
            continue;
        }
        let bytes = zip.read_entry(&mut f, k).unwrap();
        if bytes.len() > 8192 {
            continue;
        }
        for at in 0..bytes.len() {
            for change in [0x00u8, 0xFF, 0x7F] {
                let mut b = bytes.clone();
                b[at] = change;
                if let Ok(model) = Model::parse(&b) {
                    for m in 0..model.meshes.len() {
                        let _ = model.mesh_data(m);
                    }
                }
            }
        }
        tried += 1;
        if tried == 3 {
            break;
        }
    }
    assert!(tried > 0, "no small model among the first entries");
}
