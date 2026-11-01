//! Template libraries (`.plib`): the instance templates a track's placement files name, kept beside the track at
//! `Ribbon_00\instancetemplates\<season>\objects\<category>.plib` (version 38, [`Library`]) and
//! `Ribbon_00\instancetemplates\<season>\Placement\GRASSTEMPLATE.plib` (version 9, [`GroupLibrary`]).
//!
//! A [`crate::pgeo`] block's name is the name of one of its category library's model templates
//! ([`Library::model_templates_named`]); the template's model path (`tracks\brio\scene\models\...\<stem>.max`) names
//! the model, whose files are the GeoChunk entries `...\<stem>_cluster<NNN>.<x>.modelbin`
//! ([`crate::track::Track::template_models`]). Each season folder has its own libraries; which season a caller reads is the
//! caller's choice.
//!
//! Layout of version 38 (little-endian; a string is a u32 length and that many bytes of printable ASCII, no
//! terminator; a list is a u32 count and that many entries):
//!
//! | field | size | value |
//! |---|---|---|
//! | version | 4 | u32 38 |
//! | unknown | 4 | u32 |
//! | category | string | |
//! | hash | 8 | bytes |
//! | draw behaviours | list | [`DrawBehaviour`] |
//! | model templates | list | [`ModelTemplate`] |
//! | tailors | list | string name, string UV entry, u32 0, string scale entry, u32 0, u32 0, string density entry |
//! | UV entries | list | string name, f32 × 6 |
//! | empty list | 4 | u32 0 |
//! | scale entries | list | string name, f32 × 2 |
//! | empty lists | 8 | u32 0, u32 0 |
//! | density entries | list | string name, f32 × 5 |
//! | materials | list | string name, string material, u32 0, u32 index, u32 0 |
//! | placement templates | list | [`PlacementTemplate`] |
//! | empty list | 4 | u32 0 |
//! | names | list | strings |
//!
//! A draw behaviour: string name, u32 mode, f32 × 2, 13 bytes, then 65 slots ([`SLOT_COUNT`]), each a list of
//! distance ranges (u32 kind, f32 start, end, fade in, fade out, f32 -1, f32 -1, 4 bytes, u8 0: 33 bytes) and then
//! f32 -1, f32 -1, f32, u8.
//!
//! A model template: string name, string draw behaviour, f32 min x, y, z, 0, max x, y, z, 0, f32 1, 11 bytes, f32 min x
//! and max x again, u8 1, a list of model paths (strings), u8 0, 65 bytes (the LOD set of each slot), a list of LOD sets,
//! u8 0. A LOD set is a list of (u8 0, u32 lod, u32, list of u32 ids); set 0 is empty, and a slot's set index is below
//! the set count.
//!
//! A placement template: string name, string proxy path, u32, string billboard template, string billboard tailor,
//! string model template, string model tailor, string material, f32 × 2, i32.
//!
//! The empty lists are lists whose entries have never been seen; a library that has any is refused, since their layout
//! is not known. Version 9 is laid out on [`GroupLibrary`]. Both parsers refuse any other value or arrangement, with
//! the offset.
//!
//! What has been checked, and how: no third-party reader of these files exists, so the check is the census in
//! `tests/plib.rs`: every library of the Tokyo track read with no refusal, every byte accounted for.
#![deny(
    clippy::arithmetic_side_effects,
    clippy::indexing_slicing,
    clippy::cast_possible_truncation
)]

use crate::read::Reader;
use crate::{Error, Result};

/// The slots of a draw behaviour, and the entries of a model template's slot-to-set map.
pub const SLOT_COUNT: usize = 65;
/// The version [`Library::parse`] reads.
pub const LIBRARY_VERSION: u32 = 38;
/// The version [`GroupLibrary::parse`] reads.
pub const GROUP_LIBRARY_VERSION: u32 = 9;

fn fail(what: String) -> Error {
    Error::Plib(what)
}

/// The version word a library starts with.
pub fn version(bytes: &[u8]) -> Result<u32> {
    Reader::new(bytes, fail).u32("version")
}

/// A distance range of a draw behaviour's slot.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Range {
    /// The kind word (0 to 3 on the Tokyo track), meaning not known.
    pub kind: u32,
    /// Where the range starts, in metres.
    pub start: f32,
    /// Where it ends.
    pub end: f32,
    /// The fade-in distance.
    pub fade_in: f32,
    /// The fade-out distance.
    pub fade_out: f32,
    /// Four bytes, meaning not known.
    pub flags: [u8; 4],
}

/// A draw behaviour's slot.
#[derive(Clone, Debug, PartialEq)]
pub struct Slot {
    /// Its distance ranges (none in most slots).
    pub ranges: Vec<Range>,
    /// The third float after the ranges, meaning not known.
    pub value: f32,
    /// The byte after it, meaning not known.
    pub flag: u8,
}

/// A draw behaviour: the distance ranges of each slot.
#[derive(Clone, Debug, PartialEq)]
pub struct DrawBehaviour {
    /// The name model templates refer to it by.
    pub name: String,
    /// The mode word, meaning not known.
    pub mode: u32,
    /// Two floats, meaning not known.
    pub values: [f32; 2],
    /// Thirteen bytes, meaning not known.
    pub flags: [u8; 13],
    /// The [`SLOT_COUNT`] slots.
    pub slots: Vec<Slot>,
}

/// A model LOD of a template's LOD set.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LodRef {
    /// The LOD number.
    pub lod: u32,
    /// A word, meaning not known.
    pub value: u32,
    /// The ids.
    pub ids: Vec<u32>,
}

/// A model template: what a `.pgeo` block of its name places.
#[derive(Clone, Debug, PartialEq)]
pub struct ModelTemplate {
    /// The name (`tre_gbl_elm_a_3D`, `3D_GBL_SUSUKI_SUM_A_3D_HIGH`, `tre_gbl_elm_a_impostor`).
    pub name: String,
    /// The draw behaviour's name.
    pub draw_behaviour: String,
    /// The box's minimum x, y, z.
    pub min: [f32; 3],
    /// The box's maximum x, y, z.
    pub max: [f32; 3],
    /// Eleven bytes, meaning not known.
    pub flags: [u8; 11],
    /// The model paths (`tracks\brio\scene\models\...\<stem>.max`, `.model`, or no extension).
    pub models: Vec<String>,
    /// The LOD set of each slot of the draw behaviour; 0 (the empty set) where it draws nothing.
    pub slot_sets: [u8; SLOT_COUNT],
    /// The LOD sets; set 0 is empty.
    pub lod_sets: Vec<Vec<LodRef>>,
}

/// A tailor: the names of its UV, scale and density entries.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Tailor {
    /// The name.
    pub name: String,
    /// The UV entry's name, or empty.
    pub uv: String,
    /// The scale entry's name, or empty.
    pub scale: String,
    /// The density entry's name.
    pub density: String,
}

/// A named list of floats (the UV, scale and density entries).
#[derive(Clone, Debug, PartialEq)]
pub struct Values<const N: usize> {
    /// The name.
    pub name: String,
    /// The floats, meaning not known.
    pub values: [f32; N],
}

/// A material entry.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Material {
    /// The name placement templates refer to it by.
    pub name: String,
    /// The material it names (empty for `MATERIAL_DEFAULT`).
    pub material: String,
    /// A word, meaning not known (0 to 20 on the Tokyo track).
    pub index: u32,
}

/// A placement template: the templates one placement rule uses.
#[derive(Clone, Debug, PartialEq)]
pub struct PlacementTemplate {
    /// The name (the `.pgeo` blocks' source name).
    pub name: String,
    /// The proxy model's path, or empty.
    pub proxy: String,
    /// A word, meaning not known (an index into [`Library::names`] in the crowd library).
    pub index: u32,
    /// The billboard model template's name, or empty.
    pub billboard: String,
    /// The billboard's tailor, or empty.
    pub billboard_tailor: String,
    /// The model template's name, or empty.
    pub model: String,
    /// The model's tailor, or empty.
    pub model_tailor: String,
    /// The material entry's name, or empty.
    pub material: String,
    /// Two floats, meaning not known.
    pub values: [f32; 2],
    /// A word, meaning not known (-1 on most).
    pub value: i32,
}

/// A version 38 template library, as stored.
#[derive(Clone, Debug, PartialEq)]
pub struct Library {
    /// The word after the version, meaning not known.
    pub unknown: u32,
    /// The category (`PROPS`, `GRASSTEMPLATE`, ...): a `.pgeo` header's category.
    pub category: String,
    /// Eight bytes, meaning not known.
    pub hash: [u8; 8],
    /// The draw behaviours.
    pub draw_behaviours: Vec<DrawBehaviour>,
    /// The model templates.
    pub model_templates: Vec<ModelTemplate>,
    /// The tailors.
    pub tailors: Vec<Tailor>,
    /// The UV entries.
    pub uvs: Vec<Values<6>>,
    /// The scale entries.
    pub scales: Vec<Values<2>>,
    /// The density entries.
    pub densities: Vec<Values<5>>,
    /// The materials.
    pub materials: Vec<Material>,
    /// The placement templates.
    pub placement_templates: Vec<PlacementTemplate>,
    /// The closing list of names (the crowd animations in the crowd library; one empty name elsewhere).
    pub names: Vec<String>,
}

impl Library {
    /// Reads a version 38 library, refusing any byte the layout does not account for.
    pub fn parse(bytes: &[u8]) -> Result<Library> {
        let mut r = Reader::new(bytes, fail);
        r.expect_u32(LIBRARY_VERSION, "version")?;
        let unknown = r.u32("word after the version")?;
        let category = r.string("category")?;
        let hash = r.array("hash")?;
        let draw_behaviours = list(&mut r, "draw behaviour count", draw_behaviour)?;
        let model_templates = list(&mut r, "model template count", model_template)?;
        let tailors = list(&mut r, "tailor count", |r| {
            let name = r.string("tailor name")?;
            let uv = r.string("tailor UV entry")?;
            r.expect_u32(0, "tailor word after the UV entry")?;
            let scale = r.string("tailor scale entry")?;
            r.expect_u32(0, "tailor word after the scale entry")?;
            r.expect_u32(0, "tailor second word after the scale entry")?;
            let density = r.string("tailor density entry")?;
            Ok(Tailor {
                name,
                uv,
                scale,
                density,
            })
        })?;
        let uvs = list(&mut r, "UV entry count", values)?;
        r.expect_u32(
            0,
            "count of the list after the UV entries (entries of an unknown layout)",
        )?;
        let scales = list(&mut r, "scale entry count", values)?;
        r.expect_u32(
            0,
            "count of the first list after the scale entries (entries of an unknown layout)",
        )?;
        r.expect_u32(
            0,
            "count of the second list after the scale entries (entries of an unknown layout)",
        )?;
        let densities = list(&mut r, "density entry count", values)?;
        let materials = list(&mut r, "material count", |r| {
            let name = r.string("material name")?;
            let material = r.string("material's material")?;
            r.expect_u32(0, "material word before the index")?;
            let index = r.u32("material index")?;
            r.expect_u32(0, "material word after the index")?;
            Ok(Material {
                name,
                material,
                index,
            })
        })?;
        let placement_templates = list(&mut r, "placement template count", |r| {
            Ok(PlacementTemplate {
                name: r.string("placement template name")?,
                proxy: r.string("placement template proxy")?,
                index: r.u32("placement template word")?,
                billboard: r.string("placement template billboard")?,
                billboard_tailor: r.string("placement template billboard tailor")?,
                model: r.string("placement template model")?,
                model_tailor: r.string("placement template model tailor")?,
                material: r.string("placement template material")?,
                values: r.f32s("placement template floats")?,
                value: r.i32("placement template last word")?,
            })
        })?;
        r.expect_u32(
            0,
            "count of the list after the placement templates (entries of an unknown layout)",
        )?;
        let names = list(&mut r, "name count", |r| r.string("name"))?;
        r.end("plib")?;
        Ok(Library {
            unknown,
            category,
            hash,
            draw_behaviours,
            model_templates,
            tailors,
            uvs,
            scales,
            densities,
            materials,
            placement_templates,
            names,
        })
    }

    /// The model templates of a name, in file order (a library can hold more than one of a name).
    pub fn model_templates_named<'a>(
        &'a self,
        name: &'a str,
    ) -> impl Iterator<Item = &'a ModelTemplate> + 'a {
        self.model_templates.iter().filter(move |t| t.name == name)
    }

    /// The draw behaviours of a name, in file order.
    pub fn draw_behaviours_named<'a>(
        &'a self,
        name: &'a str,
    ) -> impl Iterator<Item = &'a DrawBehaviour> + 'a {
        self.draw_behaviours.iter().filter(move |b| b.name == name)
    }

    /// The placement templates of a name, in file order.
    pub fn placement_templates_named<'a>(
        &'a self,
        name: &'a str,
    ) -> impl Iterator<Item = &'a PlacementTemplate> + 'a {
        self.placement_templates.iter().filter(move |t| t.name == name)
    }
}

fn list<T>(
    r: &mut Reader<'_>,
    what: &str,
    mut each: impl FnMut(&mut Reader<'_>) -> Result<T>,
) -> Result<Vec<T>> {
    let n = r.count(1, what)?;
    (0..n).map(|_| each(r)).collect()
}

fn values<const N: usize>(r: &mut Reader<'_>) -> Result<Values<N>> {
    Ok(Values {
        name: r.string("entry name")?,
        values: r.f32s("entry floats")?,
    })
}

/// A float that must be `value`, bit for bit.
fn expect_f32(r: &mut Reader<'_>, value: f32, what: &str) -> Result<()> {
    let at = r.position();
    let found = r.f32(what)?;
    if found.to_bits() != value.to_bits() {
        return Err(r.fail_at(at, format_args!("{what}: expected {value}, found {found}")));
    }
    Ok(())
}

fn draw_behaviour(r: &mut Reader<'_>) -> Result<DrawBehaviour> {
    let name = r.string("draw behaviour name")?;
    let mode = r.u32("draw behaviour mode")?;
    let values = r.f32s("draw behaviour floats")?;
    let flags = r.array("draw behaviour flags")?;
    let mut slots = Vec::with_capacity(SLOT_COUNT);
    for _ in 0..SLOT_COUNT {
        let ranges = list(r, "range count", |r| {
            let kind = r.u32("range kind")?;
            let [start, end, fade_in, fade_out] = r.f32s("range distances")?;
            expect_f32(r, -1.0, "range fifth float")?;
            expect_f32(r, -1.0, "range sixth float")?;
            let flags = r.array("range flags")?;
            r.expect_u8(0, "range last byte")?;
            Ok(Range {
                kind,
                start,
                end,
                fade_in,
                fade_out,
                flags,
            })
        })?;
        expect_f32(r, -1.0, "slot first float")?;
        expect_f32(r, -1.0, "slot second float")?;
        let value = r.f32("slot third float")?;
        let flag = r.u8("slot byte")?;
        slots.push(Slot { ranges, value, flag });
    }
    Ok(DrawBehaviour {
        name,
        mode,
        values,
        flags,
        slots,
    })
}

fn model_template(r: &mut Reader<'_>) -> Result<ModelTemplate> {
    let name = r.string("model template name")?;
    let draw_behaviour = r.string("model template draw behaviour")?;
    let min: [f32; 3] = r.f32s("model template box")?;
    expect_f32(r, 0.0, "model template box's fourth float")?;
    let max: [f32; 3] = r.f32s("model template box")?;
    expect_f32(r, 0.0, "model template box's eighth float")?;
    expect_f32(r, 1.0, "model template float after the box")?;
    let flags = r.array("model template flags")?;
    // the x range of the box, stored again
    let [min_x, max_x] = [min[0], max[0]];
    expect_f32(r, min_x, "model template box's min x again")?;
    expect_f32(r, max_x, "model template box's max x again")?;
    r.expect_u8(1, "model template byte before the models")?;
    let models = list(r, "model count", |r| r.string("model path"))?;
    r.expect_u8(0, "model template byte after the models")?;
    let at = r.position();
    let slot_sets: [u8; SLOT_COUNT] = r.array("slot sets")?;
    let lod_sets = list(r, "LOD set count", |r| {
        list(r, "LOD count", |r| {
            r.expect_u8(0, "LOD's first byte")?;
            Ok(LodRef {
                lod: r.u32("LOD number")?,
                value: r.u32("LOD word")?,
                ids: list(r, "LOD id count", |r| r.u32("LOD id"))?,
            })
        })
    })?;
    if lod_sets.first().is_some_and(|s| !s.is_empty()) {
        return Err(r.fail_at(at, format_args!("{name}: LOD set 0 is not empty")));
    }
    if let Some(bad) = slot_sets.iter().find(|&&s| usize::from(s) >= lod_sets.len()) {
        return Err(r.fail_at(
            at,
            format_args!("{name}: a slot names LOD set {bad} of {}", lod_sets.len()),
        ));
    }
    r.expect_u8(0, "model template last byte")?;
    Ok(ModelTemplate {
        name,
        draw_behaviour,
        min,
        max,
        flags,
        models,
        slot_sets,
        lod_sets,
    })
}

/// An entry of a [`Group`].
#[derive(Clone, Debug, PartialEq)]
pub struct GroupItem {
    /// The placement template's name (in the same season's `GRASSTEMPLATE` library).
    pub template: String,
    /// A byte, meaning not known (0 or 2 on the Tokyo track).
    pub kind: u8,
    /// Seven floats, meaning not known.
    pub values: [f32; 7],
    /// A word, meaning not known (-1, 0 or 1 on the Tokyo track).
    pub word: i32,
    /// Three more floats, meaning not known.
    pub more: [f32; 3],
}

/// A group of grass placement templates.
#[derive(Clone, Debug, PartialEq)]
pub struct Group {
    /// The name.
    pub name: String,
    /// A float, meaning not known.
    pub value: f32,
    /// The entries.
    pub items: Vec<GroupItem>,
}

/// A named set of groups.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GroupSet {
    /// The name.
    pub name: String,
    /// The groups' names.
    pub groups: Vec<String>,
}

/// A version 9 library (`Placement\GRASSTEMPLATE.plib`), as stored.
///
/// | field | size | value |
/// |---|---|---|
/// | version | 4 | u32 9 |
/// | unknown | 4 | u32 |
/// | name | string | |
/// | second name | string | |
/// | groups | list | string name, u32 n, f32, n entries of (string template, u8, f32 × 7, i32, f32 × 3) |
/// | sets | list | string name, list of strings (group names) |
/// | last | 4 | f32 |
#[derive(Clone, Debug, PartialEq)]
pub struct GroupLibrary {
    /// The word after the version, meaning not known.
    pub unknown: u32,
    /// The first name (`GRASSTEMPLATE`).
    pub name: String,
    /// The second name (`GRASSTEMPLATE`).
    pub second_name: String,
    /// The groups.
    pub groups: Vec<Group>,
    /// The sets of groups.
    pub sets: Vec<GroupSet>,
    /// The last float, meaning not known.
    pub last: f32,
}

impl GroupLibrary {
    /// Reads a version 9 library, refusing any byte the layout does not account for.
    pub fn parse(bytes: &[u8]) -> Result<GroupLibrary> {
        let mut r = Reader::new(bytes, fail);
        r.expect_u32(GROUP_LIBRARY_VERSION, "version")?;
        let unknown = r.u32("word after the version")?;
        let name = r.string("name")?;
        let second_name = r.string("second name")?;
        let n = r.count(1, "group count")?;
        let mut groups = Vec::with_capacity(n);
        for _ in 0..n {
            let name = r.string("group name")?;
            let k = r.count(1, "group entry count")?;
            let value = r.f32("group float")?;
            let mut items = Vec::with_capacity(k);
            for _ in 0..k {
                items.push(GroupItem {
                    template: r.string("group entry template")?,
                    kind: r.u8("group entry byte")?,
                    values: r.f32s("group entry floats")?,
                    word: r.i32("group entry word")?,
                    more: r.f32s("group entry last floats")?,
                });
            }
            groups.push(Group { name, value, items });
        }
        let n = r.count(1, "set count")?;
        let mut sets = Vec::with_capacity(n);
        for _ in 0..n {
            let name = r.string("set name")?;
            let k = r.count(1, "set group count")?;
            let groups = (0..k)
                .map(|_| r.string("set group"))
                .collect::<Result<Vec<_>>>()?;
            sets.push(GroupSet { name, groups });
        }
        let last = r.f32("last float")?;
        r.end("plib")?;
        Ok(GroupLibrary {
            unknown,
            name,
            second_name,
            groups,
            sets,
            last,
        })
    }
}

#[cfg(test)]
#[allow(
    clippy::arithmetic_side_effects,
    clippy::indexing_slicing,
    clippy::cast_possible_truncation,
    clippy::unwrap_used,
    reason = "tests on records built by hand"
)]
mod tests {
    use super::*;

    #[derive(Default)]
    struct W(Vec<u8>);
    impl W {
        fn u(&mut self, v: u32) -> &mut Self {
            self.0.extend(v.to_le_bytes());
            self
        }
        fn f(&mut self, v: f32) -> &mut Self {
            self.0.extend(v.to_le_bytes());
            self
        }
        fn b(&mut self, v: &[u8]) -> &mut Self {
            self.0.extend(v);
            self
        }
        fn s(&mut self, v: &str) -> &mut Self {
            self.u(v.len() as u32).b(v.as_bytes())
        }
    }

    /// One behaviour with one range in slot 13, one template drawing LOD 0 in slot 13, a tailor, one entry of each
    /// table, a material, a placement template and the closing name.
    fn library() -> Vec<u8> {
        let mut w = W::default();
        w.u(38).u(0x66B3A).s("TREES").b(&[1, 2, 3, 4, 5, 6, 7, 8]);
        w.u(1).s("TREE_3D_DEFAULT").u(3).f(-1.0).f(-1.0).b(&[0; 13]);
        for slot in 0..SLOT_COUNT {
            if slot == 13 {
                w.u(1)
                    .u(2)
                    .f(0.0)
                    .f(25.0)
                    .f(1.0)
                    .f(2.0)
                    .f(-1.0)
                    .f(-1.0)
                    .b(&[1, 0, 1, 0, 0]);
            } else {
                w.u(0);
            }
            w.f(-1.0).f(-1.0).f(1.0).b(&[0]);
        }
        w.u(1).s("tre_a_3D").s("TREE_3D_DEFAULT");
        w.f(-2.0)
            .f(0.0)
            .f(-2.0)
            .f(0.0)
            .f(2.0)
            .f(9.0)
            .f(2.0)
            .f(0.0)
            .f(1.0)
            .b(&[0; 11])
            .f(-2.0)
            .f(2.0);
        w.b(&[1])
            .u(1)
            .s(r"tracks\brio\scene\models\trees\tre_a\tre_a.max")
            .b(&[0]);
        let mut sets = [0u8; SLOT_COUNT];
        sets[13] = 1;
        w.b(&sets).u(2).u(0).u(1).b(&[0]).u(0).u(0).u(1).u(77).b(&[0]);
        w.u(1)
            .s("TAILOR_DEFAULT")
            .s("")
            .u(0)
            .s("")
            .u(0)
            .u(0)
            .s("DENSITY_DEFAULT");
        w.u(1).s("UV").f(0.0).f(0.5).f(0.25).f(0.25).f(0.5).f(0.0).u(0);
        w.u(1).s("SCALE").f(19.8).f(0.0).u(0).u(0);
        w.u(1).s("DENSITY_DEFAULT").f(1.0).f(1.0).f(1.0).f(1.0).f(1.0);
        w.u(1).s("MATERIAL_DEFAULT").s("").u(0).u(0).u(0);
        w.u(1)
            .s("tre_a")
            .s("proxy.max")
            .u(0)
            .s("tre_a_impostor")
            .s("tailor")
            .s("tre_a_3D")
            .s("TAILOR_DEFAULT")
            .s("");
        w.f(1.0).f(1.0).u(u32::MAX).u(0).u(1).s("");
        w.0
    }

    #[test]
    fn reads_a_library() {
        let l = Library::parse(&library()).unwrap();
        assert_eq!((l.category.as_str(), l.unknown), ("TREES", 0x66B3A));
        let b = &l.draw_behaviours[0];
        assert_eq!(b.slots.len(), SLOT_COUNT);
        assert_eq!(b.slots[13].ranges[0].end, 25.0);
        assert_eq!(b.slots[13].ranges[0].flags, [1, 0, 1, 0]);
        let t = l.model_templates_named("tre_a_3D").next().unwrap();
        assert_eq!((t.min, t.max), ([-2.0, 0.0, -2.0], [2.0, 9.0, 2.0]));
        assert_eq!(
            t.lod_sets[usize::from(t.slot_sets[13])],
            [LodRef {
                lod: 0,
                value: 0,
                ids: vec![77]
            }]
        );
        assert!(l.draw_behaviours_named(&t.draw_behaviour).next().is_some());
        assert_eq!(l.scales[0].values, [19.8, 0.0]);
        let p = l.placement_templates_named("tre_a").next().unwrap();
        assert_eq!((p.model.as_str(), p.value), ("tre_a_3D", -1));
        assert_eq!(l.names, [""]);
    }

    #[test]
    fn refuses_what_the_layout_does_not_hold() {
        let good = library();
        for len in 0..good.len() {
            assert!(Library::parse(&good[..len]).is_err(), "{len}");
        }
        let mut bad = good.clone();
        bad.push(0);
        assert!(Library::parse(&bad).is_err());
        let at = |needle: &[u8]| good.windows(needle.len()).position(|w| w == needle).unwrap();
        // a slot whose set is past the sets
        let mut bad = good.clone();
        let sets = at(br"tre_a.max") + 9 + 1;
        bad[sets + 13] = 2;
        assert!(
            Library::parse(&bad)
                .unwrap_err()
                .to_string()
                .contains("LOD set 2 of 2")
        );
        // the x range not repeating the box
        let mut bad = good.clone();
        let x = at(br"tracks\brio") - 4 - 4 - 1 - 4;
        bad[x..x + 4].copy_from_slice(&3.0f32.to_le_bytes());
        assert!(
            Library::parse(&bad)
                .unwrap_err()
                .to_string()
                .contains("max x again")
        );
        // an entry in a list whose layout is not known
        let mut bad = good.clone();
        let z = at(b"SCALE") - 4 - 4 - 4;
        bad[z] = 1;
        assert!(
            Library::parse(&bad)
                .unwrap_err()
                .to_string()
                .contains("unknown layout")
        );
        // another version
        assert!(Library::parse(&[9, 0, 0, 0]).is_err());
    }

    #[test]
    fn reads_a_group_library() {
        let mut w = W::default();
        w.u(9).u(5).s("GRASSTEMPLATE").s("GRASSTEMPLATE");
        w.u(1).s("SUSUKI").u(1).f(0.5).s("GBL_SUSUKI_SUM_A_HIGH").b(&[2]);
        for v in [0.1, 0.2, 0.3, 1.0, 0.7, 0.6, 1.0] {
            w.f(v);
        }
        w.u(u32::MAX).f(1.0).f(2.0).f(3.0);
        w.u(1).s("SET").u(1).s("SUSUKI").f(1.0);
        let g = GroupLibrary::parse(&w.0).unwrap();
        assert_eq!(g.groups[0].items[0].template, "GBL_SUSUKI_SUM_A_HIGH");
        assert_eq!((g.groups[0].items[0].kind, g.groups[0].items[0].word), (2, -1));
        assert_eq!(g.sets[0].groups, ["SUSUKI"]);
        assert_eq!(g.last, 1.0);
        for len in 0..w.0.len() {
            assert!(GroupLibrary::parse(&w.0[..len]).is_err(), "{len}");
        }
        assert_eq!(version(&w.0), Ok(9));
    }
}
