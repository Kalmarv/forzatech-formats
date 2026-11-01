//! The strict XML reading the game's XML files share: a UTF-8 file with a byte order mark and the declaration
//! `<?xml version="1.0" encoding="utf-8"?>`, elements read in an expected order with exactly the attributes expected,
//! comments and whitespace between elements skipped, anything else refused with the byte offset.
use crate::{Error, Result};
use quick_xml::XmlVersion;
use quick_xml::events::{BytesStart, Event};
use quick_xml::reader::Reader;

/// What the next tag is.
pub(crate) enum Tag {
    Start(String, Vec<(String, String)>),
    Empty(String, Vec<(String, String)>),
    End,
    Eof,
}

/// A tag's attributes, taken one by one by name; [`Attrs::done`] refuses any left untaken.
pub(crate) struct Attrs {
    element: String,
    attrs: Vec<(String, String)>,
}

impl Attrs {
    pub(crate) fn new(element: &str, attrs: Vec<(String, String)>) -> Attrs {
        Attrs {
            element: element.to_owned(),
            attrs,
        }
    }

    /// The value of `key`, when the tag has it (the parser has refused a tag with a repeated attribute).
    pub(crate) fn take(&mut self, key: &str) -> Option<String> {
        let k = self.attrs.iter().position(|(n, _)| n == key)?;
        Some(self.attrs.swap_remove(k).1)
    }

    /// The value of `key`, refused when the tag lacks it.
    pub(crate) fn need(&mut self, p: &Parser<'_>, key: &str) -> Result<String> {
        self.take(key)
            .ok_or_else(|| p.err(format!("<{}> has no {key}", self.element)))
    }

    /// The value of `key` read as a `T`, refused when the tag lacks it or the text does not read.
    pub(crate) fn number<T: std::str::FromStr>(&mut self, p: &Parser<'_>, key: &str) -> Result<T> {
        let v = self.need(p, key)?;
        v.parse()
            .map_err(|_| p.err(format!("<{} {key}={v:?}>: not the number expected", self.element)))
    }

    /// The value of `key` read as a `T` when the tag has it, refused when the text does not read.
    pub(crate) fn optional_number<T: std::str::FromStr>(
        &mut self,
        p: &Parser<'_>,
        key: &str,
    ) -> Result<Option<T>> {
        match self.take(key) {
            Some(v) => v
                .parse()
                .map(Some)
                .map_err(|_| p.err(format!("<{} {key}={v:?}>: not the number expected", self.element))),
            None => Ok(None),
        }
    }

    /// Refused when an attribute is left untaken.
    pub(crate) fn done(self, p: &Parser<'_>) -> Result<()> {
        match self.attrs.first() {
            Some((k, _)) => Err(p.err(format!(
                "<{}> has an attribute {k:?} that is not read",
                self.element
            ))),
            None => Ok(()),
        }
    }
}

pub(crate) struct Parser<'a> {
    reader: Reader<&'a [u8]>,
    fail: fn(String) -> Error,
}

impl<'a> Parser<'a> {
    /// A parser of a whole file's bytes, past its byte order mark and declaration; its refusals are `fail(...)`.
    pub(crate) fn new(bytes: &'a [u8], fail: fn(String) -> Error) -> Result<Parser<'a>> {
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
        let mut p = Parser { reader, fail };
        p.declaration()?;
        Ok(p)
    }

    pub(crate) fn err(&self, what: impl std::fmt::Display) -> Error {
        (self.fail)(format!("at byte {}: {what}", self.reader.buffer_position()))
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
    pub(crate) fn tag(&mut self) -> Result<Tag> {
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
    pub(crate) fn element(&mut self, name: &str, names: &[&str]) -> Result<(Vec<String>, bool)> {
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

    pub(crate) fn end(&mut self, name: &str) -> Result<()> {
        match self.tag()? {
            Tag::End => Ok(()),
            _ => Err(self.err(format!("expected the end of <{name}>"))),
        }
    }

    /// An element with no children (written `<name value="..." />`, or with a start and an end tag).
    pub(crate) fn leaf(&mut self, name: &str, names: &[&str]) -> Result<Vec<String>> {
        let (values, children) = self.element(name, names)?;
        if children {
            self.end(name)?;
        }
        Ok(values)
    }
}
