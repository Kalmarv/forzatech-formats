//! Asset manifests (`AssetManifest.xml`): what a track's build put where. Each `bin.zip` (a track's, the home space's,
//! the open world's under `Stripped\gs`) holds one naming its shader and material entries; a track folder can also hold
//! a loose one beside its GeoChunks, listing the textures and models of its hand-placed instance file.
//!
//! A UTF-8 file with a byte order mark and the declaration `<?xml version="1.0" encoding="utf-8"?>`, its sections in
//! this order, the last two only in some files:
//!
//! ```xml
//! <AssetManifest>
//!   <Shaders>
//!     <Shader Name="shaders\_0x00000000\emissiveonly.shaderbin" Source="_library\shaders\pg_fx\emissiveonly.shader" />
//!   </Shaders>
//!   <Materials>
//!     <Material Name="materials\_0x00000016.materialbin" Source="tracks\brio\materials\...\x.material" />
//!   </Materials>
//!   <Textures>
//!     <Texture Source="_library\textures\...\swatches\x_<guid>.swatch" SourceHash="1117984230" PVSTextureIndex="0" />
//!   </Textures>
//!   <Models>
//!     <Model Name="brioout.00000.modelbin" >
//!       <Mesh Name="n/a" Source="tracks\brio\scene\areabounds\area_01_boundary.max" />
//!       <Instance Id="74a7a8db-4616-4c39-bef7-f1ce11557d1d" Ribbon="0" Index="18565" />
//!     </Model>
//!   </Models>
//! </AssetManifest>
//! ```
//!
//! A `Source` is the path an asset was built from, relative to the game's media folder (the loose manifest's shaders
//! instead give an absolute path on the machine that built them, and a `Name` that is no entry of the zip).
//!
//! In a `bin.zip` manifest a shader's or material's `Name` is its entry's name in the zip, with `\` where the zip's
//! name has `/` and ASCII case aside (the home space's zip writes `Materials/` where its manifest writes
//! `materials\`). The zip's `materials\spring`, `autumn` and `winter` folders hold entries of the same names as
//! `materials`, the seasons' forms of each material, which the manifest does not list. Two names refer to one:
//!
//! - a model's material instance names its parent material by its source's game path, `Game:\Media\<source less
//!   .material>.materialbin`, and a material its shader the same way, `.shaderbin` for `.shader`:
//!   [`AssetManifest::material_for`] and [`AssetManifest::shader_for`] look one up by that path, ASCII case ignored;
//! - a track's hand-placed instance file names its shaders and materials by the entry,
//!   `game:\media\tracks\<track>\bin\<name>`.
//!
//! A texture's `SourceHash` is the CRC-32 of its game path `game:\media\<source less .swatch>.swatchbin` in lower case,
//! the path hash a material's texture parameters name it by. A texture has a `PVSTextureIndex` (in the loose manifest,
//! counting up from 0 in file order, one per texture descriptor of the instance file) or the build record [`Built`]
//! (in a `bin.zip` manifest), each read as stored; a texture with only part of the build record is refused.
//!
//! A model has one `Mesh` and any number of `Instance` elements; `MaterialKeys` and an instance's `IsLone` are kept as
//! written when present. In the loose manifest a model's name is `brioout.<number>.modelbin`, its number one of the
//! instance file's model numbers (each once, not in file order), and an instance's `Index` is a LOD row of that
//! instance file whose model is that number (each row once); its `Id` is a GUID, one shared by several instances. What
//! `Ribbon` and `IsLone` mean is not established.
//!
//! Refused, with the byte offset: bytes that are not UTF-8 or lack the byte order mark or the declaration, XML that is
//! not well formed, text between elements, an element or attribute the layout above does not have; the sections in
//! another order, or `Shaders` or `Materials` missing; a number that does not read; two shaders, two materials or two
//! textures with one source (ASCII case ignored).
//!
//! What has been checked, and how: no third-party reader of these files exists, so the check is the census in
//! `tests/asset_manifest.rs` over the four manifests of one install (Steam build 25588841): each reads whole; each
//! `bin.zip` manifest's names are entries of its zip, every material and shader entry of the zip is named, and the
//! season folders mirror `materials`; every texture's hash is its path's CRC-32 (the loose manifest's 48,733 and the home
//! space's 6); the loose manifest's 134,330 models and 593,706 instances against the Tokyo track's instance file, whose
//! 46 shader and 419 material paths each name an entry of the track's `bin.zip` manifest. The game paths models and
//! materials give are not checked here, since this crate does not read materials yet.
use crate::xml::{Attrs, Parser, Tag};
use crate::{Error, Result};
use std::collections::HashMap;

fn fail(detail: String) -> Error {
    Error::AssetManifest(detail)
}

/// A shader or material: its zip entry's name and the source path it was built from.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Asset {
    /// The `Name` attribute: the entry's name in the zip, `\`-separated.
    pub name: String,
    /// The `Source` attribute: the source path, relative to the media folder.
    pub source: String,
}

/// A texture's build record, as a `bin.zip` manifest writes it: each attribute as stored.
#[derive(Clone, Debug, PartialEq)]
pub struct Built {
    /// `Name`: the name of its first quality tier's entry (`<name>_<guid>_quality1.pb`).
    pub name: String,
    /// `Limited`.
    pub limited: u32,
    /// `AtlasIndex`.
    pub atlas_index: i32,
    /// `AtlasX`, `AtlasY`, `AtlasWidth`, `AtlasHeight`.
    pub atlas: [f32; 4],
    /// `QualityLevels`.
    pub quality_levels: u32,
    /// `Width`, `Height`, `Depth`.
    pub size: [u32; 3],
    /// `LightMapType`, as written.
    pub light_map_type: String,
    /// `LargeTextureDim`.
    pub large_texture_dim: u32,
    /// `MipReduce`.
    pub mip_reduce: u32,
    /// `OriginalWidth`, `OriginalHeight`, `OriginalDepth`.
    pub original_size: [u32; 3],
    /// `Compression`, as written (`Bc7`, `UnsignedBc5`, `R8G8B8A8`, ...).
    pub compression: String,
}

/// A texture.
#[derive(Clone, Debug, PartialEq)]
pub struct Texture {
    /// `Source`: the `.swatch` it was built from, relative to the media folder.
    pub source: String,
    /// `SourceHash`.
    pub source_hash: u32,
    /// `PVSTextureIndex`, when written.
    pub pvs_texture_index: Option<u32>,
    /// The build record, when written.
    pub built: Option<Built>,
}

/// A model's `Mesh`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Mesh {
    /// `Name`.
    pub name: String,
    /// `Source`: the scene file it was exported from.
    pub source: String,
}

/// A model's `Instance`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Instance {
    /// `Id`, as written.
    pub id: String,
    /// `Ribbon`.
    pub ribbon: u32,
    /// `Index`.
    pub index: u32,
    /// `IsLone`, when written.
    pub is_lone: Option<u32>,
}

/// A model.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Model {
    /// `Name`: the model file's name (`brioout.00000.modelbin`).
    pub name: String,
    /// `MaterialKeys`, as written, when present.
    pub material_keys: Option<String>,
    /// Its `Mesh`.
    pub mesh: Mesh,
    /// Its `Instance` elements, in file order.
    pub instances: Vec<Instance>,
}

/// An asset manifest, each section in file order.
#[derive(Clone, Debug, PartialEq)]
pub struct AssetManifest {
    /// The `Shader` elements.
    pub shaders: Vec<Asset>,
    /// The `Material` elements.
    pub materials: Vec<Asset>,
    /// The `Texture` elements, when the file has a `Textures` section.
    pub textures: Option<Vec<Texture>>,
    /// The `Model` elements, when the file has a `Models` section.
    pub models: Option<Vec<Model>>,
    shader_index: HashMap<String, usize>,
    material_index: HashMap<String, usize>,
}

/// A game path's source key: `Game:\Media\<path><ext>` less the prefix, with `source_ext` for `ext`, lower case; `None`
/// when it is not a game path with that extension.
fn key(game_path: &str, ext: &str, source_ext: &str) -> Option<String> {
    let low = game_path.to_ascii_lowercase();
    let rel = low.strip_prefix("game:\\media\\")?;
    Some(format!("{}{source_ext}", rel.strip_suffix(ext)?))
}

const SECTIONS: [&str; 4] = ["Shaders", "Materials", "Textures", "Models"];

impl AssetManifest {
    /// Reads an `AssetManifest.xml`: the bytes of the whole file.
    pub fn parse(bytes: &[u8]) -> Result<AssetManifest> {
        let mut p = Parser::new(bytes, fail)?;
        let (_, children) = p.element("AssetManifest", &[])?;
        if !children {
            return Err(p.err("<AssetManifest> is empty"));
        }
        let mut shaders = (Vec::new(), HashMap::new());
        let mut materials = (Vec::new(), HashMap::new());
        let mut textures = None;
        let mut models = None;
        // the next section allowed: Shaders and Materials in turn, then Textures or Models, then Models
        let mut next = 0;
        loop {
            let (name, attrs, children) = match p.tag()? {
                Tag::End if next >= 2 => break,
                Tag::Start(n, a) => (n, a, true),
                Tag::Empty(n, a) => (n, a, false),
                _ => return Err(p.err(format!("expected <{}>", SECTIONS[next.min(3)]))),
            };
            let at = SECTIONS
                .iter()
                .position(|s| *s == name)
                .filter(|&k| if next < 2 { k == next } else { k >= next });
            let Some(at) = at else {
                return Err(p.err(format!(
                    "<{name}> where a section of <AssetManifest> was expected, in order"
                )));
            };
            Attrs::new(&name, attrs).done(&p)?;
            next = at + 1;
            match at {
                0 => shaders = assets(&mut p, children, "Shader")?,
                1 => materials = assets(&mut p, children, "Material")?,
                2 => textures = Some(texture_section(&mut p, children)?),
                _ => models = Some(model_section(&mut p, children)?),
            }
            if next == SECTIONS.len() {
                p.end("AssetManifest")?;
                break;
            }
        }
        match p.tag()? {
            Tag::Eof => Ok(AssetManifest {
                shaders: shaders.0,
                materials: materials.0,
                textures,
                models,
                shader_index: shaders.1,
                material_index: materials.1,
            }),
            _ => Err(p.err("content after the root element")),
        }
    }

    /// The shader a game path (`Game:\Media\...\<name>.shaderbin`) names.
    pub fn shader_for(&self, game_path: &str) -> Option<&Asset> {
        let k = key(game_path, ".shaderbin", ".shader")?;
        self.shaders.get(*self.shader_index.get(&k)?)
    }

    /// The material a game path (`Game:\Media\...\<name>.materialbin`) names.
    pub fn material_for(&self, game_path: &str) -> Option<&Asset> {
        let k = key(game_path, ".materialbin", ".material")?;
        self.materials.get(*self.material_index.get(&k)?)
    }
}

/// The next child of a section, `item`: its attributes and whether it holds children; `None` at the section's end
/// (at once when the section was written empty, `open` false).
fn child(p: &mut Parser<'_>, open: bool, item: &str) -> Result<Option<(Attrs, bool)>> {
    if !open {
        return Ok(None);
    }
    match p.tag()? {
        Tag::End => Ok(None),
        Tag::Empty(n, a) if n == item => Ok(Some((Attrs::new(item, a), false))),
        Tag::Start(n, a) if n == item => Ok(Some((Attrs::new(item, a), true))),
        _ => Err(p.err(format!("expected <{item}> or the end of its section"))),
    }
}

/// Indexes a source by its lower-case text, refused when an earlier entry of the section has it.
fn unique(p: &Parser<'_>, index: &mut HashMap<String, usize>, source: &str, at: usize) -> Result<()> {
    match index.insert(source.to_ascii_lowercase(), at) {
        Some(_) => Err(p.err(format!("{source:?} is the source of two entries of a section"))),
        None => Ok(()),
    }
}

fn assets(p: &mut Parser<'_>, children: bool, item: &str) -> Result<(Vec<Asset>, HashMap<String, usize>)> {
    let mut assets = Vec::new();
    let mut index = HashMap::new();
    while let Some((mut a, held)) = child(p, children, item)? {
        if held {
            return Err(p.err(format!("<{item}> holds something")));
        }
        let name = a.need(p, "Name")?;
        let source = a.need(p, "Source")?;
        a.done(p)?;
        unique(p, &mut index, &source, assets.len())?;
        assets.push(Asset { name, source });
    }
    Ok((assets, index))
}

/// The build record: `None` when the texture has no `Name`, otherwise every attribute of it, each required.
fn built(p: &Parser<'_>, a: &mut Attrs) -> Result<Option<Built>> {
    let Some(name) = a.take("Name") else {
        return Ok(None);
    };
    Ok(Some(Built {
        name,
        limited: a.number(p, "Limited")?,
        atlas_index: a.number(p, "AtlasIndex")?,
        atlas: [
            a.number(p, "AtlasX")?,
            a.number(p, "AtlasY")?,
            a.number(p, "AtlasWidth")?,
            a.number(p, "AtlasHeight")?,
        ],
        quality_levels: a.number(p, "QualityLevels")?,
        size: [
            a.number(p, "Width")?,
            a.number(p, "Height")?,
            a.number(p, "Depth")?,
        ],
        light_map_type: a.need(p, "LightMapType")?,
        large_texture_dim: a.number(p, "LargeTextureDim")?,
        mip_reduce: a.number(p, "MipReduce")?,
        original_size: [
            a.number(p, "OriginalWidth")?,
            a.number(p, "OriginalHeight")?,
            a.number(p, "OriginalDepth")?,
        ],
        compression: a.need(p, "Compression")?,
    }))
}

fn texture_section(p: &mut Parser<'_>, children: bool) -> Result<Vec<Texture>> {
    let mut textures = Vec::new();
    let mut index = HashMap::new();
    while let Some((mut a, held)) = child(p, children, "Texture")? {
        if held {
            return Err(p.err("<Texture> holds something"));
        }
        let source = a.need(p, "Source")?;
        let source_hash = a.number(p, "SourceHash")?;
        let pvs_texture_index = a.optional_number(p, "PVSTextureIndex")?;
        let built = built(p, &mut a)?;
        a.done(p)?;
        unique(p, &mut index, &source, textures.len())?;
        textures.push(Texture {
            source,
            source_hash,
            pvs_texture_index,
            built,
        });
    }
    Ok(textures)
}

fn model_section(p: &mut Parser<'_>, children: bool) -> Result<Vec<Model>> {
    let mut models = Vec::new();
    while let Some((mut a, held)) = child(p, children, "Model")? {
        if !held {
            return Err(p.err("<Model> has no <Mesh>"));
        }
        let name = a.need(p, "Name")?;
        let material_keys = a.take("MaterialKeys");
        a.done(p)?;
        let [mesh_name, source] = <[String; 2]>::try_from(p.leaf("Mesh", &["Name", "Source"])?)
            .map_err(|_| p.err("<Mesh>: expected Name and Source"))?;
        let mut instances = Vec::new();
        while let Some((mut i, held)) = child(p, true, "Instance")? {
            if held {
                return Err(p.err("<Instance> holds something"));
            }
            let instance = Instance {
                id: i.need(p, "Id")?,
                ribbon: i.number(p, "Ribbon")?,
                index: i.number(p, "Index")?,
                is_lone: i.optional_number(p, "IsLone")?,
            };
            i.done(p)?;
            instances.push(instance);
        }
        models.push(Model {
            name,
            material_keys,
            mesh: Mesh {
                name: mesh_name,
                source,
            },
            instances,
        });
    }
    Ok(models)
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    reason = "tests on files built by hand"
)]
mod tests {
    use super::*;

    fn file(body: &str) -> Vec<u8> {
        let mut b = b"\xEF\xBB\xBF<?xml version=\"1.0\" encoding=\"utf-8\"?>\r\n".to_vec();
        b.extend_from_slice(body.as_bytes());
        b
    }

    const ZIP: &str = r#"<AssetManifest>
  <Shaders>
    <Shader Name="shaders\_0x00000000\emissiveonly.shaderbin" Source="_library\shaders\pg_fx\emissiveonly.shader" />
  </Shaders>
  <Materials>
    <Material Name="materials\_0x00000016.materialbin" Source="tracks\brio\materials\a\b.material" />
  </Materials>
</AssetManifest>"#;

    const BUILT: &str = r#"<Texture Name="t_quality1.pb" Source="x\t.swatch" Limited="0" AtlasIndex="-1" AtlasX="0.000000" AtlasY="0.000000" AtlasWidth="0.000000" AtlasHeight="0.000000" QualityLevels="2" Width="256" Height="128" Depth="1" LightMapType="Default" LargeTextureDim="0" MipReduce="0" OriginalWidth="0" OriginalHeight="0" OriginalDepth="0" Compression="Bc7" SourceHash="786090219" />"#;

    fn full() -> String {
        ZIP.replace(
            "</AssetManifest>",
            &format!(
                r#"<Textures>
    <Texture Source="y\u.swatch" SourceHash="1117984230" PVSTextureIndex="0" />
    {BUILT}
  </Textures>
  <Models>
    <Model Name="brioout.00000.modelbin" >
      <Mesh Name="n/a" Source="tracks\brio\scene\a.max" />
      <Instance Id="74a7a8db-4616-4c39-bef7-f1ce11557d1d" Ribbon="0" Index="18565" />
      <Instance Id="93ebdefb-a575-4c6a-a47e-822c992ebc26" Ribbon="0" Index="3" IsLone="0" />
    </Model>
    <Model Name="m.modelbin" MaterialKeys="Material__26, Material__28"><Mesh Name="a|b" Source="s.max" /></Model>
  </Models>
</AssetManifest>"#
            ),
        )
    }

    #[test]
    fn a_zip_manifest_reads_and_looks_up_game_paths() {
        let m = AssetManifest::parse(&file(ZIP)).unwrap();
        assert_eq!(
            (m.shaders.len(), m.textures.is_none(), m.models.is_none()),
            (1, true, true)
        );
        assert_eq!(
            m.shader_for(r"Game:\Media\_library\shaders\pg_fx\EmissiveOnly.shaderbin")
                .map(|a| a.name.as_str()),
            Some(r"shaders\_0x00000000\emissiveonly.shaderbin")
        );
        assert_eq!(
            m.material_for(r"Game:\Media\Tracks\Brio\materials\a\b.materialbin")
                .map(|a| a.name.as_str()),
            Some(r"materials\_0x00000016.materialbin")
        );
        assert!(m.material_for(r"tracks\brio\materials\a\b.material").is_none());
    }

    #[test]
    fn textures_and_models_read() {
        let m = AssetManifest::parse(&file(&full())).unwrap();
        let [loose, built] = m.textures.as_deref().unwrap() else {
            panic!()
        };
        assert_eq!(
            (loose.source_hash, loose.pvs_texture_index, loose.built.is_none()),
            (1117984230, Some(0), true)
        );
        let b = built.built.as_ref().unwrap();
        assert_eq!(
            (b.atlas_index, b.quality_levels, b.size, b.compression.as_str()),
            (-1, 2, [256, 128, 1], "Bc7")
        );
        let [one, two] = m.models.as_deref().unwrap() else {
            panic!()
        };
        assert_eq!(
            (
                one.mesh.name.as_str(),
                one.instances.len(),
                one.material_keys.as_deref()
            ),
            ("n/a", 2, None)
        );
        assert_eq!(
            (one.instances[0].index, one.instances[1].is_lone),
            (18565, Some(0))
        );
        assert_eq!(
            (two.instances.len(), two.material_keys.as_deref()),
            (0, Some("Material__26, Material__28"))
        );
    }

    #[test]
    fn what_the_layout_does_not_have_is_refused() {
        let full = full();
        for bad in [
            ZIP.replace("<Shaders>", "<Shaders x=\"1\">"),
            ZIP.replace("Source=\"_library", "Other=\"1\" Source=\"_library"),
            ZIP.replace(
                "</Materials>",
                "<Material Name=\"n\" Source=\"Tracks\\brio\\materials\\a\\b.material\" />\n</Materials>",
            ),
            ZIP.replace("<Shaders>", "<Materials></Materials><Shaders>"),
            ZIP.replace("<Materials>", "<Shaders></Shaders><Materials>"),
            ZIP.replace("<Materials>", "<Textures /><Materials>"),
            ZIP.replace("<Materials>", "<Other /><Materials>"),
            format!("{ZIP}<More />"),
            full.replace("<Textures>", "<Models /><Textures>"),
            full.replace("</Models>", "</Models><Models />"),
            full.replace(" Limited=\"0\"", ""),
            full.replace("AtlasIndex=\"-1\"", "AtlasIndex=\"x\""),
            full.replace("SourceHash=\"1117984230\"", "SourceHash=\"-1\""),
            full.replace("y\\u.swatch", "X\\T.swatch"),
            full.replace("<Mesh Name=\"a|b\" Source=\"s.max\" />", ""),
            full.replace(" IsLone=\"0\"", " IsLone=\"0\" Other=\"1\""),
            full.replace("Ribbon=\"0\" Index=\"3\"", "Index=\"3\""),
        ] {
            assert!(AssetManifest::parse(&file(&bad)).is_err(), "{bad}");
        }
    }
}
