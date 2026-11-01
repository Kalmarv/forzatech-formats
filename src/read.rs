//! A sequential reader for the world files' little-endian fields (`.pgeo`, `.plib`, `.pvs`): typed reads, counts
//! bounded by what is left, the `u32`-length strings these files store, and expected constants, each refusal naming the
//! field and its offset through the module's own [`Error`] variant.
#![deny(
    clippy::arithmetic_side_effects,
    clippy::indexing_slicing,
    clippy::cast_possible_truncation
)]

use crate::{Error, Result};
use primase_foundations::bytes::{BoundsError, Cursor, Endian::Little as LE};
use std::fmt;

/// A cursor over one file's bytes whose refusals are `fail("<what> at <offset>: ...")`.
pub(crate) struct Reader<'a> {
    cursor: Cursor<'a>,
    fail: fn(String) -> Error,
}

impl<'a> Reader<'a> {
    pub(crate) fn new(bytes: &'a [u8], fail: fn(String) -> Error) -> Reader<'a> {
        Reader {
            cursor: Cursor::new(bytes),
            fail,
        }
    }

    /// The offset of the next byte.
    pub(crate) fn position(&self) -> usize {
        self.cursor.position()
    }

    /// How many bytes are left.
    pub(crate) fn remaining(&self) -> usize {
        self.cursor.remaining()
    }

    /// A refusal of the field at `at`.
    pub(crate) fn fail_at(&self, at: usize, what: impl fmt::Display) -> Error {
        (self.fail)(format!("{what} at {at:#x}"))
    }

    /// A refusal of the field at the position.
    pub(crate) fn fail(&self, what: impl fmt::Display) -> Error {
        self.fail_at(self.position(), what)
    }

    fn bounds(&self, what: &str, e: BoundsError) -> Error {
        (self.fail)(format!("{what}: {e}"))
    }

    pub(crate) fn take(&mut self, len: usize, what: &str) -> Result<&'a [u8]> {
        self.cursor.take(len).map_err(|e| self.bounds(what, e))
    }

    pub(crate) fn array<const N: usize>(&mut self, what: &str) -> Result<[u8; N]> {
        self.cursor.array().map_err(|e| self.bounds(what, e))
    }

    pub(crate) fn u8(&mut self, what: &str) -> Result<u8> {
        self.cursor.u8().map_err(|e| self.bounds(what, e))
    }

    pub(crate) fn u16(&mut self, what: &str) -> Result<u16> {
        self.cursor.u16(LE).map_err(|e| self.bounds(what, e))
    }

    pub(crate) fn u32(&mut self, what: &str) -> Result<u32> {
        self.cursor.u32(LE).map_err(|e| self.bounds(what, e))
    }

    pub(crate) fn i32(&mut self, what: &str) -> Result<i32> {
        self.cursor.i32(LE).map_err(|e| self.bounds(what, e))
    }

    pub(crate) fn i16(&mut self, what: &str) -> Result<i16> {
        self.cursor.i16(LE).map_err(|e| self.bounds(what, e))
    }

    pub(crate) fn u64(&mut self, what: &str) -> Result<u64> {
        self.cursor.u64(LE).map_err(|e| self.bounds(what, e))
    }

    pub(crate) fn f32(&mut self, what: &str) -> Result<f32> {
        self.cursor.f32(LE).map_err(|e| self.bounds(what, e))
    }

    pub(crate) fn f32s<const N: usize>(&mut self, what: &str) -> Result<[f32; N]> {
        let mut out = [0.0; N];
        for v in &mut out {
            *v = self.f32(what)?;
        }
        Ok(out)
    }

    /// A `u32` that must be `value`; the cursor stays before it when it is not.
    pub(crate) fn expect_u32(&mut self, value: u32, what: &str) -> Result<()> {
        let at = self.position();
        let found = self.u32(what)?;
        if found != value {
            return Err(self.fail_at(at, format_args!("{what}: expected {value}, found {found}")));
        }
        Ok(())
    }

    /// A byte that must be `value`.
    pub(crate) fn expect_u8(&mut self, value: u8, what: &str) -> Result<()> {
        let at = self.position();
        let found = self.u8(what)?;
        if found != value {
            return Err(self.fail_at(at, format_args!("{what}: expected {value}, found {found}")));
        }
        Ok(())
    }

    /// A `u32` count of records of at least `each` bytes, refused when what is left cannot hold them.
    pub(crate) fn count(&mut self, each: usize, what: &str) -> Result<usize> {
        self.cursor.count(LE, each).map_err(|e| self.bounds(what, e))
    }

    /// A string: a `u32` length, then that many bytes of printable ASCII (0x20 to 0x7E), no terminator.
    pub(crate) fn string(&mut self, what: &str) -> Result<String> {
        let at = self.position();
        let len = self.count(1, what)?;
        let bytes = self.take(len, what)?;
        if let Some(bad) = bytes.iter().position(|b| !(0x20..=0x7E).contains(b)) {
            return Err(self.fail_at(
                at,
                format_args!("{what}: byte {bad} of the string is not printable ASCII"),
            ));
        }
        // printable ASCII is UTF-8
        Ok(bytes.iter().map(|&b| char::from(b)).collect())
    }

    /// A string of printable ASCII ended by a zero byte (the zero not part of it).
    pub(crate) fn cstring(&mut self, what: &str) -> Result<String> {
        let at = self.position();
        let rest = self.cursor.bytes().get(at..).unwrap_or_default();
        let Some(len) = rest.iter().position(|&b| b == 0) else {
            return Err(self.fail_at(at, format_args!("{what}: no zero byte ends the string")));
        };
        let bytes = self.take(len, what)?;
        self.u8(what)?;
        if let Some(bad) = bytes.iter().position(|b| !(0x20..=0x7E).contains(b)) {
            return Err(self.fail_at(
                at,
                format_args!("{what}: byte {bad} of the string is not printable ASCII"),
            ));
        }
        Ok(bytes.iter().map(|&b| char::from(b)).collect())
    }

    /// Refuses any byte after the position.
    pub(crate) fn end(&self, what: &str) -> Result<()> {
        if self.remaining() != 0 {
            return Err(self.fail(format_args!("{what}: {} bytes follow the end", self.remaining())));
        }
        Ok(())
    }
}
