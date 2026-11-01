//! Game objects (`GameObjs.xml`, `gameobjs.xml`): the places gameplay refers to by name, kept beside a track at
//! `Ribbon_00\GameObjs.xml` and for the open world at `Stripped\gs\<track>\gameobjs.xml`.
//!
//! A UTF-8 file, with a byte order mark and an XML declaration (`<?xml version="1.0" encoding="utf-8"?>`):
//!
//! ```xml
//! <GameObjs>
//!   <GameObjectLocators />
//!   <Objs>
//!     <Obj GameplayID="ESTATE_ENTRANCE_13" Guid="b73e3645-2a4b-4be1-a894-a48a3147844c">
//!       <Pos value="-2015.01,113.67,2481.72" />
//!       <Orientation>
//!         <XAxis value="0.16,0.026,0.987" />
//!         <YAxis value="-0.004,0.9997,-0.026" />
//!         <ZAxis value="-0.987,0,0.16" />
//!       </Orientation>
//!     </Obj>
//!   </Objs>
//! </GameObjs>
//! ```
//!
//! `Guid` is present in the `Stripped\gs` file and absent from the track's. A value is three decimal numbers separated
//! by commas, read as the nearest `f32` ([`Object::position`], [`Object::axes`]); the files were written by more than
//! one program, so some numbers are the shortest text of an `f32` (`-1792.549`) and others 15 digits long
//! (`101.982322692871`), and a number need not be exactly the `f32` it reads as. The reader checks nothing about the
//! axes beyond that each is three finite numbers; whether they are unit length and orthogonal, and whether they are the
//! rows or the columns of a rotation, is the caller's to establish.
//!
//! `GameObjectLocators` is empty in every file seen, and a file in which it holds anything is refused, since its
//! layout is not known.
//!
//! Refused, with the byte offset: bytes that are not UTF-8 or lack the byte order mark or the declaration, XML that is
//! not well formed, an element or attribute the layout above does not have, a missing or repeated attribute, an
//! element in another order, text between elements, a value that is not three finite numbers, and content after the
//! root.
//!
//! What has been checked, and how: no third-party reader of these files exists, so the check is the census in
//! `tests/gameobjs.rs`: the three files of one install (Steam build 25588841: the track's 793 objects, the open world's
//! 59, the home space's none) read with no refusal.
use crate::{Error, Result};
use quick_xml::XmlVersion;
use quick_xml::events::{BytesStart, Event};
use quick_xml::reader::Reader;

fn fail(detail: String) -> Error {
    Error::GameObjs(detail)
}

/// One object: a gameplay name and where it stands.
#[derive(Clone, Debug, PartialEq)]
pub struct Object {
    /// The `GameplayID` attribute. Not unique within a file (the `Stripped\gs` file holds 59 objects with 45 ids).
    pub id: String,
    /// The `Guid` attribute, as written, when the file has one.
    pub guid: Option<String>,
    /// The `Pos` value.
    pub position: [f32; 3],
    /// The `XAxis`, `YAxis` and `ZAxis` values of the `Orientation`, in that order.
    pub axes: [[f32; 3]; 3],
}

/// A game objects file: its objects, in file order.
#[derive(Clone, Debug, PartialEq)]
pub struct GameObjs {
    /// The `Obj` elements of `Objs`.
    pub objects: Vec<Object>,
}

impl GameObjs {
    /// Reads a `GameObjs.xml`: the bytes of the whole file.
    pub fn parse(bytes: &[u8]) -> Result<GameObjs> {
        let body = bytes
            .strip_prefix(b"\xEF\xBB\xBF")
            .ok_or_else(|| fail("at byte 0: expected a UTF-8 byte order mark".to_owned()))?;
        let text = std::str::from_utf8(body).map_err(|e| fail(format!("not UTF-8: {e}")))?;
        let mut reader = Reader::from_str(text);
        let config = reader.config_mut();
        config.check_end_names = true;
        config.allow_unmatched_ends = false;
        config.check_comments = true;
        config.expand_empty_elements = false;
        config.trim_text(false);
        let mut p = Parser { reader };
        p.declaration()?;
        p.file()
    }
}

/// What the next tag is.
enum Tag {
    Start(String, Vec<(String, String)>),
    Empty(String, Vec<(String, String)>),
    End,
    Eof,
}

struct Parser<'a> {
    reader: Reader<&'a [u8]>,
}

impl Parser<'_> {
    fn err(&self, what: impl std::fmt::Display) -> Error {
        fail(format!("at byte {}: {what}", self.reader.buffer_position()))
    }

    fn declaration(&mut self) -> Result<()> {
        let Event::Decl(d) = self.reader.read_event().map_err(|e| self.err(e))? else {
            return Err(self.err("expected the XML declaration first"));
        };
        let version = d.version().map_err(|e| self.err(e))?;
        let encoding = d.encoding().transpose().map_err(|e| self.err(e))?;
        let standalone = d.standalone().transpose().map_err(|e| self.err(e))?;
        if version.as_ref() != "1.0"
            || !encoding.is_some_and(|e| e.eq_ignore_ascii_case("utf-8"))
            || standalone.is_some()
        {
            return Err(self.err("expected the declaration <?xml version=\"1.0\" encoding=\"utf-8\"?>"));
        }
        Ok(())
    }

    fn start(&self, s: &BytesStart<'_>) -> Result<(String, Vec<(String, String)>)> {
        let qname = s.name();
        let name: &str = qname.as_ref();
        let mut attrs: Vec<(String, String)> = Vec::new();
        for attr in s.attributes().with_checks(true) {
            let attr = attr.map_err(|e| self.err(format!("<{name}>: {e}")))?;
            let key: &str = attr.key.as_ref();
            let value = attr
                .normalized_value(XmlVersion::Implicit1_0)
                .map_err(|e| self.err(format!("<{name} {key}>: {e}")))?;
            attrs.push((key.to_owned(), value.into_owned()));
        }
        Ok((name.to_owned(), attrs))
    }

    /// The next start, empty or end tag, skipping comments and whitespace.
    fn tag(&mut self) -> Result<Tag> {
        loop {
            let event = self.reader.read_event().map_err(|e| self.err(e))?;
            return Ok(match event {
                Event::Start(s) => {
                    let (name, attrs) = self.start(&s)?;
                    Tag::Start(name, attrs)
                }
                Event::Empty(s) => {
                    let (name, attrs) = self.start(&s)?;
                    Tag::Empty(name, attrs)
                }
                Event::End(_) => Tag::End,
                Event::Eof => Tag::Eof,
                Event::Comment(_) => continue,
                Event::Text(t) if t.bytes().all(|b| b.is_ascii_whitespace()) => continue,
                Event::Text(_) => return Err(self.err("text between elements")),
                Event::CData(_) => return Err(self.err("a CDATA section")),
                Event::Decl(_) => return Err(self.err("a second XML declaration")),
                Event::PI(_) => return Err(self.err("a processing instruction")),
                Event::DocType(_) => return Err(self.err("a document type")),
                Event::GeneralRef(_) => return Err(self.err("an entity reference between elements")),
            });
        }
    }

    /// An element named `name` with the attributes `names` (each once), holding children when `children`.
    fn element(&mut self, name: &str, names: &[&str]) -> Result<(Vec<String>, bool)> {
        let (found, attrs, children) = match self.tag()? {
            Tag::Start(n, a) => (n, a, true),
            Tag::Empty(n, a) => (n, a, false),
            Tag::End => return Err(self.err(format!("expected <{name}>, found an end tag"))),
            Tag::Eof => return Err(self.err(format!("expected <{name}>, found the end of the file"))),
        };
        if found != name {
            return Err(self.err(format!("expected <{name}>, found <{found}>")));
        }
        let mut values = Vec::with_capacity(names.len());
        for wanted in names {
            let mut hits = attrs.iter().filter(|(k, _)| k == wanted);
            let value = hits
                .next()
                .map(|(_, v)| v.clone())
                .ok_or_else(|| self.err(format!("<{name}> has no {wanted}")))?;
            if hits.next().is_some() {
                return Err(self.err(format!("<{name}> repeats {wanted}")));
            }
            values.push(value);
        }
        if let Some((k, _)) = attrs.iter().find(|(k, _)| !names.contains(&k.as_str())) {
            return Err(self.err(format!("<{name}> has an attribute {k:?} that is not read")));
        }
        Ok((values, children))
    }

    fn end(&mut self, name: &str) -> Result<()> {
        match self.tag()? {
            Tag::End => Ok(()),
            _ => Err(self.err(format!("expected the end of <{name}>"))),
        }
    }

    /// An element with no children (written `<name value="..." />`, or with a start and an end tag).
    fn leaf(&mut self, name: &str, names: &[&str]) -> Result<Vec<String>> {
        let (values, children) = self.element(name, names)?;
        if children {
            self.end(name)?;
        }
        Ok(values)
    }

    fn vector(&mut self, name: &str) -> Result<[f32; 3]> {
        let [value] = self
            .leaf(name, &["value"])?
            .try_into()
            .map_err(|_| self.err("value"))?;
        let mut out = [0.0f32; 3];
        let mut parts = value.split(',');
        for slot in &mut out {
            let part = parts
                .next()
                .ok_or_else(|| self.err(format!("<{name}> value {value:?}: expected three numbers")))?;
            *slot = part
                .parse::<f32>()
                .ok()
                .filter(|v| v.is_finite())
                .ok_or_else(|| {
                    self.err(format!(
                        "<{name}> value {value:?}: {part:?} is not a finite number"
                    ))
                })?;
        }
        if parts.next().is_some() {
            return Err(self.err(format!("<{name}> value {value:?}: expected three numbers")));
        }
        Ok(out)
    }

    fn file(&mut self) -> Result<GameObjs> {
        let (_, children) = self.element("GameObjs", &[])?;
        if !children {
            return Err(self.err("<GameObjs> is empty"));
        }
        let (_, children) = self.element("GameObjectLocators", &[])?;
        if children && !matches!(self.tag()?, Tag::End) {
            return Err(self.err("<GameObjectLocators> holds something: its layout is not known"));
        }
        let mut objects = Vec::new();
        let (_, children) = self.element("Objs", &[])?;
        if children {
            loop {
                let (values, children) = match self.tag()? {
                    Tag::End => break,
                    Tag::Start(n, a) if n == "Obj" => (a, true),
                    Tag::Empty(n, a) if n == "Obj" => (a, false),
                    _ => return Err(self.err("expected <Obj> or the end of <Objs>")),
                };
                if !children {
                    return Err(self.err("<Obj> is empty"));
                }
                let mut id = None;
                let mut guid = None;
                for (k, v) in values {
                    let slot = match k.as_str() {
                        "GameplayID" => &mut id,
                        "Guid" => &mut guid,
                        _ => return Err(self.err(format!("<Obj> has an attribute {k:?} that is not read"))),
                    };
                    if slot.replace(v).is_some() {
                        return Err(self.err(format!("<Obj> repeats {k}")));
                    }
                }
                let id = id.ok_or_else(|| self.err("<Obj> has no GameplayID"))?;
                let position = self.vector("Pos")?;
                let (_, oriented) = self.element("Orientation", &[])?;
                if !oriented {
                    return Err(self.err("<Orientation> is empty"));
                }
                let axes = [
                    self.vector("XAxis")?,
                    self.vector("YAxis")?,
                    self.vector("ZAxis")?,
                ];
                self.end("Orientation")?;
                self.end("Obj")?;
                objects.push(Object {
                    id,
                    guid,
                    position,
                    axes,
                });
            }
        }
        self.end("GameObjs")?;
        match self.tag()? {
            Tag::Eof => Ok(GameObjs { objects }),
            _ => Err(self.err("content after the root element")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const HEAD: &str = "\u{feff}<?xml version=\"1.0\" encoding=\"utf-8\"?>\r\n";

    fn file(body: &str) -> Vec<u8> {
        format!("{HEAD}<GameObjs>\r\n  <GameObjectLocators />\r\n{body}</GameObjs>").into_bytes()
    }

    const OBJ: &str = "<Obj GameplayID=\"A\"><Pos value=\"1,2.5,-3E-02\" /><Orientation>\
        <XAxis value=\"1,0,0\" /><YAxis value=\"0,1,0\" /><ZAxis value=\"0,0,1\" /></Orientation></Obj>";

    #[test]
    fn an_object_reads() {
        let g = GameObjs::parse(&file(&format!("<Objs>{OBJ}</Objs>"))).unwrap();
        let [o] = g.objects.as_slice() else { panic!() };
        assert_eq!((o.id.as_str(), o.guid.as_deref()), ("A", None));
        assert_eq!(o.position, [1.0, 2.5, -0.03]);
        assert_eq!(o.axes, [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]]);
    }

    #[test]
    fn no_objects_is_a_file() {
        assert!(GameObjs::parse(&file("<Objs />")).unwrap().objects.is_empty());
    }

    #[test]
    fn a_guid_is_kept() {
        let obj = OBJ.replace("GameplayID=\"A\"", "GameplayID=\"A\" Guid=\"g\"");
        let g = GameObjs::parse(&file(&format!("<Objs>{obj}</Objs>"))).unwrap();
        assert_eq!(g.objects.first().and_then(|o| o.guid.as_deref()), Some("g"));
    }

    #[test]
    fn what_the_layout_does_not_have_is_refused() {
        let bad = [
            OBJ.replace("1,2.5,-3E-02", "1,2.5"),
            OBJ.replace("1,2.5,-3E-02", "1,2.5,x"),
            OBJ.replace("1,2.5,-3E-02", "1,2.5,3,4"),
            OBJ.replace("GameplayID=\"A\"", "GameplayID=\"A\" Extra=\"1\""),
            OBJ.replace("GameplayID=\"A\"", "Id=\"A\""),
            OBJ.replace("<YAxis value=\"0,1,0\" />", ""),
            OBJ.replace("<Pos value=\"1,2.5,-3E-02\" />", "<Pos value=\"1,2.5,NaN\" />"),
        ];
        for obj in bad {
            assert!(
                GameObjs::parse(&file(&format!("<Objs>{obj}</Objs>"))).is_err(),
                "{obj}"
            );
        }
        let locators =
            format!("{HEAD}<GameObjs><GameObjectLocators><x /></GameObjectLocators><Objs /></GameObjs>");
        assert!(GameObjs::parse(locators.as_bytes()).is_err());
        assert!(
            GameObjs::parse(&file("<Objs />")[3..]).is_err(),
            "no byte order mark"
        );
        let mut trailing = file("<Objs />");
        trailing.extend_from_slice(b"x");
        assert!(GameObjs::parse(&trailing).is_err());
    }
}
