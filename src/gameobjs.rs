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
use crate::xml::{Parser, Tag};
use crate::{Error, Result};

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
        file(&mut Parser::new(bytes, fail)?)
    }
}

fn vector(p: &mut Parser<'_>, name: &str) -> Result<[f32; 3]> {
    let [value] = p.leaf(name, &["value"])?.try_into().map_err(|_| p.err("value"))?;
    let mut out = [0.0f32; 3];
    let mut parts = value.split(',');
    for slot in &mut out {
        let part = parts
            .next()
            .ok_or_else(|| p.err(format!("<{name}> value {value:?}: expected three numbers")))?;
        *slot = part
            .parse::<f32>()
            .ok()
            .filter(|v| v.is_finite())
            .ok_or_else(|| {
                p.err(format!(
                    "<{name}> value {value:?}: {part:?} is not a finite number"
                ))
            })?;
    }
    if parts.next().is_some() {
        return Err(p.err(format!("<{name}> value {value:?}: expected three numbers")));
    }
    Ok(out)
}

fn file(p: &mut Parser<'_>) -> Result<GameObjs> {
    let (_, children) = p.element("GameObjs", &[])?;
    if !children {
        return Err(p.err("<GameObjs> is empty"));
    }
    let (_, children) = p.element("GameObjectLocators", &[])?;
    if children && !matches!(p.tag()?, Tag::End) {
        return Err(p.err("<GameObjectLocators> holds something: its layout is not known"));
    }
    let mut objects = Vec::new();
    let (_, children) = p.element("Objs", &[])?;
    if children {
        loop {
            let (values, children) = match p.tag()? {
                Tag::End => break,
                Tag::Start(n, a) if n == "Obj" => (a, true),
                Tag::Empty(n, a) if n == "Obj" => (a, false),
                _ => return Err(p.err("expected <Obj> or the end of <Objs>")),
            };
            if !children {
                return Err(p.err("<Obj> is empty"));
            }
            let mut id = None;
            let mut guid = None;
            for (k, v) in values {
                let slot = match k.as_str() {
                    "GameplayID" => &mut id,
                    "Guid" => &mut guid,
                    _ => return Err(p.err(format!("<Obj> has an attribute {k:?} that is not read"))),
                };
                if slot.replace(v).is_some() {
                    return Err(p.err(format!("<Obj> repeats {k}")));
                }
            }
            let id = id.ok_or_else(|| p.err("<Obj> has no GameplayID"))?;
            let position = vector(p, "Pos")?;
            let (_, oriented) = p.element("Orientation", &[])?;
            if !oriented {
                return Err(p.err("<Orientation> is empty"));
            }
            let axes = [vector(p, "XAxis")?, vector(p, "YAxis")?, vector(p, "ZAxis")?];
            p.end("Orientation")?;
            p.end("Obj")?;
            objects.push(Object {
                id,
                guid,
                position,
                axes,
            });
        }
    }
    p.end("GameObjs")?;
    match p.tag()? {
        Tag::Eof => Ok(GameObjs { objects }),
        _ => Err(p.err("content after the root element")),
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
