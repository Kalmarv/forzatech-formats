//! The settings files whose root element is `DatabaseFile`: the render settings of `Rendering.zip` (`Megatexture.xml`,
//! `Livery.xml`, `DecalManager.xml` and the rest) and the data-manager files of `GameTunableSettings.zip`. They are
//! encrypted zip entries ([`crate::zip::Zip::decrypt`]); this module reads the XML they decrypt to.
//!
//! A file is a list of blocks, each a `Data` element (`path`, `type`: the defaults of one settings object) or a
//! `Settings` element (`path`, optional `tags`: the values that replace the defaults when the tags hold). A block holds
//! `property` elements (`id`, optional `type`, `value` and `tags`). A property holds nested properties, list
//! `element`s (optional `type` and `value`, holding properties and elements) and `map_element`s (a `key` then a
//! `value`, each with optional `id`, `type` and `value`; the value holds properties and map elements).
//!
//! Values are kept as the text the file stores (`"0.75"`, `"true"`, `"240"`): which type a value has is the settings
//! object's, named by the `type` attributes where the file gives one, and the caller's to apply.
//!
//! Refused, never skipped: bytes that are not UTF-8, XML that is not well formed (an end tag that names another
//! element than the one it closes, as the last block of `Rendering.zip`'s `Cubemap.xml` has), an element or attribute
//! the layout above does not have, a missing `path` or `id`, a repeated attribute, text between elements, CDATA, a
//! declaration, a processing instruction or a document type. Comments are allowed.
//!
//! What has been checked, and how: on one install of Steam build 25588841, the 71 `DatabaseFile` files of
//! `Rendering.zip` and `GameTunableSettings.zip` read with no refusal but `Cubemap.xml`'s. `tests/settings.rs` repeats
//! that with your keys.

use crate::{Error, Result};
use quick_xml::XmlVersion;
use quick_xml::events::{BytesStart, Event};
use quick_xml::reader::Reader;

/// A `DatabaseFile`: its `path` attribute and its blocks, in file order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DatabaseFile {
    /// The root's `path` attribute (empty in every file read).
    pub path: String,
    /// The `Data` and `Settings` blocks, in file order.
    pub blocks: Vec<Block>,
}

/// What a block is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BlockKind {
    /// A `Data` element: the defaults of a settings object of this type.
    Data {
        /// The `type` attribute: the settings object's type name.
        type_name: String,
    },
    /// A `Settings` element: values that apply when its tags hold.
    Settings {
        /// The `tags` attribute as stored (an expression such as `Config.CubemapResolution.High OR ...`), if any.
        tags: Option<String>,
    },
}

/// A `Data` or `Settings` block.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Block {
    /// `Data` or `Settings`, with the attributes that belong to each.
    pub kind: BlockKind,
    /// The `path` attribute: the settings object the block sets (`MegaTexture.Params`).
    pub path: String,
    /// The block's properties, in file order.
    pub properties: Vec<Property>,
}

impl Block {
    /// The first property with this id.
    pub fn property(&self, id: &str) -> Option<&Property> {
        self.properties.iter().find(|p| p.id == id)
    }
}

/// A `property`: its id and what it holds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Property {
    /// The `id` attribute.
    pub id: String,
    /// The `tags` attribute, if any.
    pub tags: Option<String>,
    /// The type, value and children.
    pub node: Node,
}

impl Property {
    /// The first nested property with this id.
    pub fn property(&self, id: &str) -> Option<&Property> {
        self.node.properties.iter().find(|p| p.id == id)
    }
}

/// What a property, a list element, a map key or a map value holds.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Node {
    /// The `type` attribute, if any.
    pub type_name: Option<String>,
    /// The `value` attribute as stored, if any.
    pub value: Option<String>,
    /// Nested properties, in file order.
    pub properties: Vec<Property>,
    /// List `element`s, in file order.
    pub elements: Vec<Node>,
    /// `map_element`s, in file order.
    pub map: Vec<MapEntry>,
}

/// A `map_element`: a `key`, then a `value`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MapEntry {
    /// The `key`'s `id` attribute, if any.
    pub key_id: Option<String>,
    /// The key.
    pub key: Node,
    /// The `value`'s `id` attribute, if any.
    pub value_id: Option<String>,
    /// The value.
    pub value: Node,
}

impl DatabaseFile {
    /// Read a `DatabaseFile` from the bytes of a decrypted settings entry.
    pub fn parse(bytes: &[u8]) -> Result<Self> {
        let text = std::str::from_utf8(bytes).map_err(|e| fail(format!("not UTF-8: {e}")))?;
        let mut p = Parser {
            reader: Reader::from_str(text),
        };
        let config = p.reader.config_mut();
        config.check_end_names = true;
        config.allow_unmatched_ends = false;
        config.check_comments = true;
        config.expand_empty_elements = false;
        config.trim_text(false);
        p.file()
    }

    /// The first block with this path and kind `Data`.
    pub fn data(&self, path: &str) -> Option<&Block> {
        self.blocks
            .iter()
            .find(|b| b.path == path && matches!(b.kind, BlockKind::Data { .. }))
    }

    /// Every `Settings` block with this path, in file order.
    pub fn settings<'a>(&'a self, path: &'a str) -> impl Iterator<Item = &'a Block> + 'a {
        self.blocks
            .iter()
            .filter(move |b| b.path == path && matches!(b.kind, BlockKind::Settings { .. }))
    }
}

/// The name of the root element of an XML file, read past a UTF-8 byte order mark, a declaration, comments,
/// processing instructions and a document type: which reader a settings entry is for (`DatabaseFile` is this
/// module's). It reads no further than the root's start tag.
pub fn root(bytes: &[u8]) -> Result<String> {
    let bytes = bytes.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(bytes);
    let text = std::str::from_utf8(bytes).map_err(|e| fail(format!("not UTF-8: {e}")))?;
    let mut reader = Reader::from_str(text);
    loop {
        let at = reader.buffer_position();
        match reader
            .read_event()
            .map_err(|e| fail(format!("at byte {at}: {e}")))?
        {
            Event::Start(s) | Event::Empty(s) => {
                let qname = s.name();
                let name: &str = qname.as_ref();
                return Ok(name.to_owned());
            }
            Event::Decl(_) | Event::Comment(_) | Event::PI(_) | Event::DocType(_) => {}
            Event::Text(t) if t.bytes().all(|b| b.is_ascii_whitespace()) => {}
            _ => return Err(fail(format!("at byte {at}: no root element"))),
        }
    }
}

fn fail(detail: String) -> Error {
    Error::Settings(detail)
}

/// The attributes an element may have, read into place by name.
struct Attrs {
    names: &'static [&'static str],
    values: Vec<Option<String>>,
}

impl Attrs {
    fn take(&mut self, name: &str) -> Option<String> {
        let i = self.names.iter().position(|n| *n == name)?;
        self.values.get_mut(i).and_then(Option::take)
    }
}

/// An element read as a start tag (`children` true) or an empty-element tag.
struct Open {
    name: String,
    attrs: Attrs,
    children: bool,
}

struct Parser<'a> {
    reader: Reader<&'a [u8]>,
}

impl Parser<'_> {
    fn at(&self) -> u64 {
        self.reader.buffer_position()
    }

    fn err(&self, what: impl std::fmt::Display) -> Error {
        fail(format!("at byte {}: {what}", self.at()))
    }

    /// The next start, empty or end tag, skipping comments and whitespace; `None` for an end tag, and an error at the
    /// end of the input.
    fn next(&mut self, allowed: &[(&'static str, &'static [&'static str])]) -> Result<Option<Open>> {
        loop {
            let event = self.reader.read_event().map_err(|e| self.err(e))?;
            let (start, children) = match event {
                Event::Start(s) => (s, true),
                Event::Empty(s) => (s, false),
                Event::End(_) => return Ok(None),
                Event::Comment(_) => continue,
                Event::Text(t) if t.bytes().all(|b| b.is_ascii_whitespace()) => continue,
                Event::Text(_) => return Err(self.err("text between elements")),
                Event::Eof => return Err(self.err("the file ends inside an element")),
                Event::CData(_) => return Err(self.err("a CDATA section")),
                Event::Decl(_) => return Err(self.err("an XML declaration")),
                Event::PI(_) => return Err(self.err("a processing instruction")),
                Event::DocType(_) => return Err(self.err("a document type")),
                Event::GeneralRef(_) => return Err(self.err("an entity reference between elements")),
            };
            return self.open(&start, children, allowed).map(Some);
        }
    }

    fn open(
        &self,
        start: &BytesStart<'_>,
        children: bool,
        allowed: &[(&'static str, &'static [&'static str])],
    ) -> Result<Open> {
        let qname = start.name();
        let name: &str = qname.as_ref();
        let name = name.to_owned();
        let names = allowed
            .iter()
            .find(|(n, _)| *n == name)
            .map(|(_, a)| *a)
            .ok_or_else(|| self.err(format!("element <{name}> is not read here")))?;
        let mut attrs = Attrs {
            names,
            values: vec![None; names.len()],
        };
        for attr in start.attributes().with_checks(true) {
            let attr = attr.map_err(|e| self.err(format!("<{name}>: {e}")))?;
            let key: &str = attr.key.as_ref();
            let slot = names
                .iter()
                .position(|n| *n == key)
                .and_then(|i| attrs.values.get_mut(i))
                .ok_or_else(|| self.err(format!("<{name}> has an attribute {key:?} that is not read")))?;
            let value = attr
                .normalized_value(XmlVersion::Implicit1_0)
                .map_err(|e| self.err(format!("<{name} {key}>: {e}")))?;
            *slot = Some(value.into_owned());
        }
        Ok(Open {
            name,
            attrs,
            children,
        })
    }

    fn file(&mut self) -> Result<DatabaseFile> {
        let Some(mut root) = self.next(&[("DatabaseFile", &["path"])])? else {
            return Err(self.err("an end tag before the root element"));
        };
        let path = root
            .attrs
            .take("path")
            .ok_or_else(|| self.err("<DatabaseFile> has no path"))?;
        let mut blocks = Vec::new();
        if root.children {
            while let Some(mut open) =
                self.next(&[("Data", &["path", "type"]), ("Settings", &["path", "tags"])])?
            {
                let path = open
                    .attrs
                    .take("path")
                    .ok_or_else(|| self.err(format!("<{}> has no path", open.name)))?;
                let kind = if open.name == "Data" {
                    BlockKind::Data {
                        type_name: open
                            .attrs
                            .take("type")
                            .ok_or_else(|| self.err("<Data> has no type"))?,
                    }
                } else {
                    BlockKind::Settings {
                        tags: open.attrs.take("tags"),
                    }
                };
                let properties = if open.children {
                    self.properties_only()?
                } else {
                    Vec::new()
                };
                blocks.push(Block {
                    kind,
                    path,
                    properties,
                });
            }
        }
        loop {
            match self.reader.read_event().map_err(|e| self.err(e))? {
                Event::Eof => break,
                Event::Comment(_) => {}
                Event::Text(t) if t.bytes().all(|b| b.is_ascii_whitespace()) => {}
                _ => return Err(self.err("content after the root element")),
            }
        }
        Ok(DatabaseFile { path, blocks })
    }

    /// The children of a block or a `key`-less value: properties only, up to the end tag.
    fn properties_only(&mut self) -> Result<Vec<Property>> {
        let mut out = Vec::new();
        while let Some(open) = self.next(&[PROPERTY])? {
            out.push(self.property(open)?);
        }
        Ok(out)
    }

    fn property(&mut self, mut open: Open) -> Result<Property> {
        let id = open
            .attrs
            .take("id")
            .ok_or_else(|| self.err("<property> has no id"))?;
        let tags = open.attrs.take("tags");
        let node = self.node(open, &[PROPERTY, ELEMENT, MAP_ELEMENT])?;
        Ok(Property { id, tags, node })
    }

    /// The type and value of an open element and, when it has children, those `allowed` here.
    fn node(&mut self, mut open: Open, allowed: &[(&'static str, &'static [&'static str])]) -> Result<Node> {
        let mut node = Node {
            type_name: open.attrs.take("type"),
            value: open.attrs.take("value"),
            ..Node::default()
        };
        if !open.children {
            return Ok(node);
        }
        while let Some(child) = self.next(allowed)? {
            match child.name.as_str() {
                "property" => node.properties.push(self.property(child)?),
                "element" => node.elements.push(self.node(child, &[PROPERTY, ELEMENT])?),
                _ => node.map.push(self.map_entry(child)?),
            }
        }
        Ok(node)
    }

    fn map_entry(&mut self, open: Open) -> Result<MapEntry> {
        if !open.children {
            return Err(self.err("an empty <map_element>"));
        }
        let Some(mut key) = self.next(&[KEY])? else {
            return Err(self.err("a <map_element> without a key"));
        };
        let key_id = key.attrs.take("id");
        let key = self.node(key, &[])?;
        let Some(mut value) = self.next(&[VALUE])? else {
            return Err(self.err("a <map_element> without a value"));
        };
        let value_id = value.attrs.take("id");
        let value = self.node(value, &[PROPERTY, MAP_ELEMENT])?;
        if self.next(&[])?.is_some() {
            return Err(self.err("a <map_element> with more than a key and a value"));
        }
        Ok(MapEntry {
            key_id,
            key,
            value_id,
            value,
        })
    }
}

const PROPERTY: (&str, &[&str]) = ("property", &["id", "type", "value", "tags"]);
const ELEMENT: (&str, &[&str]) = ("element", &["type", "value"]);
const MAP_ELEMENT: (&str, &[&str]) = ("map_element", &[]);
const KEY: (&str, &[&str]) = ("key", &["id", "type", "value"]);
const VALUE: (&str, &[&str]) = ("value", &["id", "type", "value"]);

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"<DatabaseFile path="">
    <!-- Defaults -->
    <Data path="MegaTexture.Params" type="MegaTextureParams">
        <property id="Sharpness">
            <element value="240"/>
            <element value="235"/>
        </property>
        <property id="DirectionBias" value="0.75"/>
        <property id="Nested" type="Inner">
            <property id="A" value="1" type="int"/>
        </property>
        <property id="Table">
            <map_element>
                <key id="Name" value="High"/>
                <value id="Params" type="P"><property id="X" value="2"/></value>
            </map_element>
        </property>
    </Data>
    <Settings path="MegaTexture.Params" tags="Config.High OR Config.Ultra">
        <property id="DirectionBias" value="0.5" tags="x"/>
    </Settings>
</DatabaseFile>
"#;

    #[test]
    fn reads_blocks_properties_lists_and_maps_as_stored() {
        let f = DatabaseFile::parse(SAMPLE.as_bytes()).unwrap();
        assert_eq!(f.path, "");
        assert_eq!(f.blocks.len(), 2);
        let data = f.data("MegaTexture.Params").unwrap();
        assert_eq!(
            data.kind,
            BlockKind::Data {
                type_name: "MegaTextureParams".into()
            }
        );
        let sharp = data.property("Sharpness").unwrap();
        let values: Vec<_> = sharp
            .node
            .elements
            .iter()
            .map(|e| e.value.as_deref().unwrap())
            .collect();
        assert_eq!(values, ["240", "235"]);
        assert_eq!(
            data.property("DirectionBias").unwrap().node.value.as_deref(),
            Some("0.75")
        );
        let nested = data.property("Nested").unwrap();
        assert_eq!(nested.node.type_name.as_deref(), Some("Inner"));
        assert_eq!(
            nested.property("A").unwrap().node.type_name.as_deref(),
            Some("int")
        );
        let table = &data.property("Table").unwrap().node.map[0];
        assert_eq!(
            (table.key_id.as_deref(), table.key.value.as_deref()),
            (Some("Name"), Some("High"))
        );
        assert_eq!(table.value_id.as_deref(), Some("Params"));
        assert_eq!(table.value.properties[0].node.value.as_deref(), Some("2"));
        let settings: Vec<_> = f.settings("MegaTexture.Params").collect();
        assert_eq!(settings.len(), 1);
        assert_eq!(
            settings[0].kind,
            BlockKind::Settings {
                tags: Some("Config.High OR Config.Ultra".into())
            }
        );
        assert_eq!(settings[0].properties[0].tags.as_deref(), Some("x"));
    }

    fn refused(text: &str) -> String {
        match DatabaseFile::parse(text.as_bytes()) {
            Err(Error::Settings(d)) => d,
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn refuses_what_the_layout_does_not_have() {
        // an end tag naming another element, as Cubemap.xml's last block has
        assert!(refused(r#"<DatabaseFile path=""><Settings path="a"><property id="b" value="1"/></property></DatabaseFile>"#).contains("byte"));
        assert!(refused(r#"<DatabaseFile path=""><Other path="a"/></DatabaseFile>"#).contains("<Other>"));
        assert!(
            refused(r#"<DatabaseFile path=""><Data path="a" type="t" extra="1"/></DatabaseFile>"#)
                .contains("extra")
        );
        assert!(refused(r#"<DatabaseFile path=""><Data path="a"/></DatabaseFile>"#).contains("no type"));
        assert!(
            refused(
                r#"<DatabaseFile path=""><Data path="a" type="t"><property value="1"/></Data></DatabaseFile>"#
            )
            .contains("no id")
        );
        assert!(
            refused(r#"<DatabaseFile path=""><Data path="a" path="b" type="t"/></DatabaseFile>"#)
                .contains("Data")
        );
        assert!(refused(r#"<DatabaseFile path="">text</DatabaseFile>"#).contains("text"));
        assert!(refused(r#"<?xml version="1.0"?><DatabaseFile path=""/>"#).contains("declaration"));
        assert!(refused(r#"<DatabaseFile path=""><Data path="a" type="t">"#).contains("ends"));
        assert!(refused(r#"<DatabaseFile path=""/><DatabaseFile path=""/>"#).contains("after the root"));
        assert!(refused(r#"<DatabaseFile path=""><Data path="a" type="t"><property id="m"><map_element><value/></map_element></property></Data></DatabaseFile>"#).contains("<value>"));
        assert!(refused(r#"<DatabaseFile path=""><Data path="a" type="t"><property id="m"><map_element><key value="1"/></map_element></property></Data></DatabaseFile>"#).contains("without a value"));
        assert!(matches!(
            DatabaseFile::parse(b"<DatabaseFile path=\"\xff\"/>"),
            Err(Error::Settings(_))
        ));
    }

    #[test]
    fn every_prefix_is_refused_without_a_panic() {
        for n in 0..SAMPLE.len() - 1 {
            if let Some(prefix) = SAMPLE.get(..n) {
                assert!(DatabaseFile::parse(prefix.as_bytes()).is_err(), "prefix {n}");
            }
        }
    }
}
