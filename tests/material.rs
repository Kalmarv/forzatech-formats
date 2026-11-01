//! Reads every material of a real install: the `.materialbin` and `.shaderbin` entries and every material instance
//! nested in a `.modelbin` (the `MatI` blobs), in the Tokyo track's four GeoChunks (`media/Tracks/Brio`) and in every
//! zip under `media`, from the install the `GAME_DATA` configuration names, and compares each with Doliman100's
//! reading of the same bytes (the oracle set `doliman`, entry `texmat`, see tests/inputs/README.md). Without the
//! install or the keys the test skips with a line naming its key.
//!
//! - Every `.materialbin` reads as a [`Material`], every `.shaderbin` as a [`Shader`] and every non-empty `MatI` as
//!   an [`Instance`], each with every check the readers make holding: a sample of entries drawn with the day's seed,
//!   or all of them with `[run] gate = 'full'`.
//! - For each, the parameter list is the one Doliman's `ShaderParameter` reads, line by line (the object's parent
//!   path and name, and per parameter its hash, type, GUID, the value where the library keeps one, and the position
//!   after it), compared as one SHA-256 per entry; at full depth the oracle lists no entry the test did not read.
//!   Doliman reads a sampler (type 7) as 12 bytes and has no case for type 12, Forza Horizon 5's layout, so its
//!   reading of a list stops after the first parameter of either type, and so does the comparison.
//! - Facts the readers' documentation states, asserted at every parameter: the hash after a texture parameter's path
//!   is the CRC-32 of the lowercased path (for every non-empty path), and in a shader every `PARM` record's first
//!   u32 is the CRC-32 of its name, every `CBMP`, `TXMP` and `SPMP` hash is a `PARM` hash and every flagged `DFPR`
//!   default's flag hash is its parameter's name hash.
#![allow(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    reason = "a test reading the files you point it at"
)]

mod inputs;
mod walk;

use forzatech_formats::bundle::Bundle;
use forzatech_formats::keys::Keys;
use forzatech_formats::material::{Instance, MAT_I, Material, Parameters, Value, path_hash};
use forzatech_formats::shader::Shader;
use inputs::{Game, OracleSet, install, install_file, keys, oracle};
use primase_foundations::hash::crc32;
use primase_testdata::{Depth, depth, seed};
use std::collections::{BTreeMap, HashMap};
use walk::{Walk, hex, named, sha256, tsv};

type Key = (String, usize);

/// The oracle's rows: entry to (objects, parameters, SHA-256 of its reading); and its sampled readings line by line.
struct Oracle {
    rows: HashMap<Key, (usize, usize, String)>,
    sample: HashMap<Key, Vec<String>>,
}

fn load(dir: &std::path::Path) -> Oracle {
    for e in tsv(&dir.join("errors.tsv")) {
        let low = e[2].to_ascii_lowercase();
        assert!(
            !low.ends_with(".materialbin") && !low.ends_with(".shaderbin") && !low.ends_with(".modelbin"),
            "Doliman raised on a material entry: {e:?}"
        );
    }
    let mut rows = HashMap::new();
    for r in tsv(&dir.join("materials.tsv")) {
        assert_eq!(r.len(), 6, "materials.tsv: {r:?}");
        rows.insert(
            (r[0].clone(), r[1].parse().unwrap()),
            (r[3].parse().unwrap(), r[4].parse().unwrap(), r[5].clone()),
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

fn ok<T>(r: forzatech_formats::Result<T>, at: &str) -> T {
    r.unwrap_or_else(|e| panic!("{at}: {e}"))
}

/// A float as the oracle writes it: Python's `struct.pack("<f", v).hex()` of the float it read (a signaling NaN
/// comes back quiet through Python's double).
fn float(x: f32) -> String {
    hex(&(f64::from(x) as f32).to_bits().to_le_bytes())
}

/// The value as the oracle keeps it (`-` for the types whose value the library skips).
fn value(v: &Value<'_>) -> String {
    match v {
        Value::Colour(c) => c.map(float).join(","),
        Value::Vector2(c) => c.map(float).join(","),
        Value::Float(x) => float(*x),
        Value::Bool(b) => if *b != 0 { "True" } else { "False" }.to_owned(),
        Value::Texture { path, .. } => (*path).to_owned(),
        _ => "-".to_owned(),
    }
}

/// An object's reading in the oracle's line format (`dumps/doliman-texmat/texmat_dump.py`): the parent tag, parent
/// string, name and count, then a parameter per line up to the first sampler or type 12. Returns how many parameters
/// the oracle reads (the ones the lines hold).
fn lines(path: &str, head: (&str, &str, &str), p: &Parameters<'_>, out: &mut Vec<String>) -> usize {
    let mut read = 0;
    out.push(format!(
        "O\t{path}\t{}\t{}\t{}\t{}",
        head.0,
        head.1,
        head.2,
        p.list.len()
    ));
    for (i, q) in p.list.iter().enumerate() {
        let kind = q.value.type_number();
        // the oracle's position after a sampler is 4 short of the parameter's end, and after a type 12 (no case in
        // the library) where its GUID ends
        let end = match kind {
            7 => q.end - 4,
            12 => q.end - 16,
            _ => q.end,
        };
        out.push(format!(
            "p\t{path}\t{i}\t{:08x}\t{kind}\t{}\t{}\t{end}",
            q.hash,
            hex(&q.guid),
            value(&q.value)
        ));
        read += 1;
        if matches!(kind, 7 | 12) {
            out.push(format!("x\t{path}\t{i}\tcut"));
            break;
        }
    }
    read
}

#[derive(Default)]
struct Census {
    materialbins: usize,
    shaderbins: usize,
    models: usize,
    models_without: usize,
    instances: usize,
    empty_instances: usize,
    parameters: usize,
    by_type: BTreeMap<String, usize>,
    flagged: usize,
    textures_with_path: usize,
    textures_empty: usize,
    textures_empty_hash_zero: usize,
    names: usize,
    map_entries: usize,
    flagged_defaults: usize,
    compared: usize,
}

fn count(c: &mut Census, p: &Parameters<'_>) {
    for q in &p.list {
        c.parameters += 1;
        *c.by_type
            .entry(format!("v{} type {}", q.version, q.value.type_number()))
            .or_default() += 1;
        c.flagged += usize::from(q.flag_hash.is_some());
        if let Value::Texture { path, path_hash: h } = q.value {
            if path.is_empty() {
                c.textures_empty += 1;
                c.textures_empty_hash_zero += usize::from(h == 0);
            } else {
                c.textures_with_path += 1;
                assert_eq!(h, path_hash(path), "the hash after the texture path {path:?}");
            }
        }
    }
}

fn shader_facts(c: &mut Census, s: &Shader<'_>) {
    for n in &s.names {
        c.names += 1;
        assert_eq!(
            n.name_hash,
            crc32(n.name.as_bytes()),
            "PARM {:?}'s first u32",
            n.name
        );
    }
    for (what, map) in [
        ("CBMP", &s.constants),
        ("TXMP", &s.textures),
        ("SPMP", &s.samplers),
    ] {
        for e in map {
            c.map_entries += 1;
            assert!(
                s.name_of(e.hash).is_some(),
                "{what} hash {:#010x} is no PARM hash",
                e.hash
            );
        }
    }
    for q in &s.defaults.list {
        if let Some(flag) = q.flag_hash {
            c.flagged_defaults += 1;
            let n = s
                .name_of(q.hash)
                .unwrap_or_else(|| panic!("DFPR parameter {:#010x} is no PARM hash", q.hash));
            assert_eq!(flag, n.name_hash, "the flag hash of DFPR {:?}", n.name);
        }
    }
}

/// What an entry's reading is compared on: where it is, how many objects and parameters it holds, and its lines.
struct Entry<'a> {
    source: &'a str,
    index: usize,
    name: &'a str,
    objects: usize,
    parameters: usize,
    reading: &'a [String],
}

fn check(census: &mut Census, oracle: &mut Option<Oracle>, e: &Entry<'_>) {
    let Some(o) = oracle else { return };
    let Entry {
        source,
        index,
        name,
        reading,
        ..
    } = *e;
    let key = (source.to_owned(), index);
    let row = o
        .rows
        .remove(&key)
        .unwrap_or_else(|| panic!("{source} entry {index} {name}: an entry the oracle does not list"));
    let sha = sha256(
        reading
            .join(
                "
",
            )
            .as_bytes(),
    );
    if sha != row.2 {
        let detail = match o.sample.get(&key) {
            Some(theirs) => match reading.iter().zip(theirs).position(|(a, b)| a != b) {
                Some(i) => format!(
                    "first difference at line {i}: ours {:?}, the oracle's {:?}",
                    reading[i], theirs[i]
                ),
                None => format!("{} lines, the oracle {}", reading.len(), theirs.len()),
            },
            None => "not in the oracle's sample".to_owned(),
        };
        panic!("{source} entry {index} {name}: the reading differs from the oracle's ({detail})");
    }
    assert_eq!(
        (e.objects, e.parameters),
        (row.0, row.1),
        "{source} entry {index} {name}: objects and parameters read"
    );
    census.compared += 1;
}

#[test]
fn every_material_reads_as_doliman_reads_it() {
    let (Some(root), Some(track), Some(keys_path)) = (
        install(Game::Fh6),
        install_file(Game::Fh6, "media/Tracks/Brio"),
        keys(Game::Fh6),
    ) else {
        return;
    };
    let keys = Keys::parse(&std::fs::read_to_string(&keys_path).unwrap()).unwrap();
    let mut oracle = oracle(OracleSet::Doliman, Game::Fh6, "texmat").map(|dir| load(&dir));
    let (depth, seed) = (depth(), seed());
    println!("depth {depth:?}, seed {seed}, oracle {}", oracle.is_some());
    let walk = Walk {
        track,
        media: root.join("media"),
        keys: &keys,
        depth,
        seed,
    };
    let mut census = Census::default();
    let mut seen = 0usize;
    walk.each(
        |name| named(name, &[".materialbin", ".shaderbin", ".modelbin"]),
        (512, 64),
        |source, index, name, bytes| {
            let bundle = ok(Bundle::parse(bytes), &format!("{source} entry {index} {name}"));
            let at = format!("{source} entry {index} {name}");
            let mut reading = Vec::new();
            let (mut objects, mut parameters) = (0usize, 0usize);
            if named(name, &[".materialbin"]) {
                let m = ok(Material::from_bundle(&bundle), &at);
                census.materialbins += 1;
                count(&mut census, &m.parameters);
                parameters = lines("-", ("MATL", m.shader, m.name), &m.parameters, &mut reading);
                objects = 1;
            } else if named(name, &[".shaderbin"]) {
                let s = ok(Shader::from_bundle(&bundle), &at);
                census.shaderbins += 1;
                count(&mut census, &s.defaults);
                shader_facts(&mut census, &s);
                parameters = lines("-", ("-", "-", "-"), &s.defaults, &mut reading);
                objects = 1;
            } else {
                census.models += 1;
                for (k, blob) in bundle.tagged(MAT_I).enumerate() {
                    objects += 1;
                    let path = format!("{:08x}:{k}", MAT_I.0);
                    match ok(Instance::from_model_blob(blob), &at) {
                        Some(i) => {
                            census.instances += 1;
                            count(&mut census, &i.parameters);
                            parameters +=
                                lines(&path, ("MATI", i.parent, i.name), &i.parameters, &mut reading);
                        }
                        None => {
                            // the library cannot read a bundle from no bytes
                            census.empty_instances += 1;
                            reading.push(format!("e\t{path}\tTypeError"));
                        }
                    }
                }
                if objects == 0 {
                    census.models_without += 1;
                    return;
                }
            }
            seen += 1;
            let entry = Entry {
                source,
                index,
                name,
                objects,
                parameters,
                reading: &reading,
            };
            check(&mut census, &mut oracle, &entry);
        },
    );
    println!(
        "{} materialbins, {} shaderbins, {} modelbins ({} without a MatI), {} material instances and {} empty MatI blobs read",
        census.materialbins,
        census.shaderbins,
        census.models,
        census.models_without,
        census.instances,
        census.empty_instances
    );
    println!(
        "{} parameters ({} flagged); {} texture parameters with a path (each hash the CRC-32 of the lowercased path), {} with an empty path ({} of them with hash 0)",
        census.parameters,
        census.flagged,
        census.textures_with_path,
        census.textures_empty,
        census.textures_empty_hash_zero
    );
    println!(
        "shaders: {} PARM records, {} CBMP/TXMP/SPMP entries, {} flagged DFPR defaults; {} entries compared with the oracle",
        census.names, census.map_entries, census.flagged_defaults, census.compared
    );
    for (k, v) in &census.by_type {
        println!("  {v:>9}  {k}");
    }
    assert!(seen > 0, "no material entry read");
    if let (Depth::Full, Some(o)) = (depth, &oracle) {
        let left: Vec<_> = o.rows.keys().take(5).collect();
        assert!(
            o.rows.is_empty(),
            "{} entries the oracle lists were not read: {left:?}",
            o.rows.len()
        );
    }
}
