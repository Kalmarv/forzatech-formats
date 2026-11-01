//! The world's frame, the placement cell grid, the seasons, and the fixed-point coordinates placements store.
//!
//! The world is Y-up, one unit a metre; x and z are the ground plane. Placement files (`.pgeo`) are kept one per cell
//! size, cell and category, under a zipcache path `...\scene\proc\cellsize\<size>\<x>_<z>\<file>`; the cell
//! `(x, z)` of size `s` covers `[x s, (x + 1) s)` in x and `[z s, (z + 1) s)` in z ([`Cell::from_path`],
//! [`Cell::area`]). An [`Area`] is an x/z box a caller asks for placements in.
//!
//! A placement's position is three words in sign-magnitude fixed point: bit 31 the sign, bits 16 to 30 the whole
//! metres, bits 0 to 15 a fraction of 65,535 (not 65,536), as the game's instanced vertex shaders decode it
//! ([`fixed`]).
//!
//! The season masks of `.pgeo` headers and `.pvs` gameplay records are a byte of two equal nibbles; bit `s` of a
//! nibble set means the content is not shown in season `s` (0 spring, 1 summer, 2 autumn, 3 winter: [`Season`],
//! [`SeasonMask`]).
//!
//! What has been checked, and how: see `tests/pgeo.rs` (every `.pgeo` path has a cell; every season mask is two equal
//! nibbles; the instances of a file lie in its header box, and in its cell only for some categories).

use std::fmt;

/// The metres between two fixed-point values: a position is stored to this step.
pub const FIXED_STEP: f64 = 1.0 / 65535.0;

/// The metres a sign-magnitude fixed-point word stands for: `sign × (((w >> 16) & 0x7FFF) + (w & 0xFFFF) / 65535)`.
/// The value is exact in an `f64`.
pub fn fixed(word: u32) -> f64 {
    let whole = f64::from((word >> 16) & 0x7FFF);
    let fraction = f64::from(word & 0xFFFF) / 65535.0;
    let magnitude = whole + fraction;
    if word & 0x8000_0000 != 0 {
        -magnitude
    } else {
        magnitude
    }
}

/// The value of an IEEE 754 half-precision float's bits (the `.pvs` matrices and boxes store them).
pub fn half(bits: u16) -> f32 {
    let sign = if bits & 0x8000 != 0 { -1.0 } else { 1.0 };
    let exponent = i32::from((bits >> 10) & 0x1F);
    let mantissa = f32::from(bits & 0x3FF);
    match exponent {
        0 => sign * mantissa * 2f32.powi(-24),
        31 if mantissa == 0.0 => sign * f32::INFINITY,
        31 => f32::NAN,
        e => sign * (1.0 + mantissa / 1024.0) * 2f32.powi(e - 15),
    }
}

/// An x/z box in world metres: `min_x <= x < max_x` and `min_z <= z < max_z`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Area {
    /// The smallest x inside.
    pub min_x: f64,
    /// The smallest z inside.
    pub min_z: f64,
    /// The first x past the box.
    pub max_x: f64,
    /// The first z past the box.
    pub max_z: f64,
}

impl Area {
    /// The box `[min_x, max_x) × [min_z, max_z)`.
    pub fn new(min_x: f64, min_z: f64, max_x: f64, max_z: f64) -> Area {
        Area {
            min_x,
            min_z,
            max_x,
            max_z,
        }
    }

    /// Whether the point `(x, z)` is inside.
    pub fn contains(&self, x: f64, z: f64) -> bool {
        self.min_x <= x && x < self.max_x && self.min_z <= z && z < self.max_z
    }

    /// Whether the two boxes share a point.
    pub fn overlaps(&self, other: &Area) -> bool {
        self.min_x < other.max_x
            && other.min_x < self.max_x
            && self.min_z < other.max_z
            && other.min_z < self.max_z
    }
}

/// A placement cell: its size in metres and its index on each axis.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Cell {
    /// The edge length in metres (50, 75, 100, 125, 150, 200, 250, 300, 500 or 10,000 on the Tokyo track).
    pub size: u32,
    /// The index along x: the cell starts at `x × size`.
    pub x: i32,
    /// The index along z: the cell starts at `z × size`.
    pub z: i32,
}

impl Cell {
    /// The cell a zipcache path names with its `\cellsize\<size>\<x>_<z>\` folders (either slash), or `None` when it
    /// names none or more than one.
    pub fn from_path(path: &str) -> Option<Cell> {
        let mut found = None;
        let parts: Vec<&str> = path.split(['\\', '/']).collect();
        for (k, part) in parts.iter().enumerate() {
            if !part.eq_ignore_ascii_case("cellsize") {
                continue;
            }
            let size = parts.get(k.checked_add(1)?)?;
            let index = parts.get(k.checked_add(2)?)?;
            // the cell is a folder: something follows it
            parts.get(k.checked_add(3)?)?;
            let (x, z) = index.split_once('_')?;
            let cell = Cell {
                size: number(size)?,
                x: signed(x)?,
                z: signed(z)?,
            };
            if cell.size == 0 || found.replace(cell).is_some() {
                return None;
            }
        }
        found
    }

    /// The x/z box the cell covers.
    pub fn area(&self) -> Area {
        let s = f64::from(self.size);
        Area::new(
            f64::from(self.x) * s,
            f64::from(self.z) * s,
            (f64::from(self.x) + 1.0) * s,
            (f64::from(self.z) + 1.0) * s,
        )
    }
}

impl fmt::Display for Cell {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}\\{}_{}", self.size, self.x, self.z)
    }
}

/// Decimal digits, no sign, no leading zero unless the number is 0.
fn number(s: &str) -> Option<u32> {
    if s.is_empty() || !s.bytes().all(|b| b.is_ascii_digit()) || (s.len() > 1 && s.starts_with('0')) {
        return None;
    }
    s.parse().ok()
}

/// [`number`], optionally after a `-` (never `-0`).
fn signed(s: &str) -> Option<i32> {
    match s.strip_prefix('-') {
        Some(rest) => {
            let n = number(rest)?;
            if n == 0 {
                return None;
            }
            i32::try_from(n).ok()?.checked_neg()
        }
        None => i32::try_from(number(s)?).ok(),
    }
}

/// A season, in the order the files index them.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Season {
    /// Index 0.
    Spring,
    /// Index 1.
    Summer,
    /// Index 2.
    Autumn,
    /// Index 3.
    Winter,
}

impl Season {
    /// The four, by index.
    pub const ALL: [Season; 4] = [Season::Spring, Season::Summer, Season::Autumn, Season::Winter];

    /// The season's index: 0 spring, 1 summer, 2 autumn, 3 winter.
    pub fn index(self) -> u8 {
        match self {
            Season::Spring => 0,
            Season::Summer => 1,
            Season::Autumn => 2,
            Season::Winter => 3,
        }
    }

    /// The folder name the template libraries are kept under (`spring`, `summer`, `autumn`, `winter`).
    pub fn folder(self) -> &'static str {
        match self {
            Season::Spring => "spring",
            Season::Summer => "summer",
            Season::Autumn => "autumn",
            Season::Winter => "winter",
        }
    }
}

/// A season-exclusion mask as stored: a byte whose high nibble repeats its low one.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct SeasonMask(u8);

impl SeasonMask {
    /// The mask a byte holds, or `None` when its two nibbles differ.
    pub fn new(byte: u8) -> Option<SeasonMask> {
        (byte >> 4 == byte & 0x0F).then_some(SeasonMask(byte))
    }

    /// The byte as stored.
    pub fn byte(self) -> u8 {
        self.0
    }

    /// Whether the mask keeps the content out of `season`.
    pub fn excludes(self, season: Season) -> bool {
        self.0 & (1 << season.index()) != 0
    }

    /// Whether the content is shown in `season`.
    pub fn shows(self, season: Season) -> bool {
        !self.excludes(season)
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

    #[test]
    fn fixed_point_is_sign_magnitude_over_65535() {
        assert_eq!(fixed(0), 0.0);
        assert_eq!(fixed(0x0001_0000), 1.0);
        assert_eq!(fixed(0x0000_FFFF), 1.0);
        assert_eq!(fixed(0x8001_8000), -(1.0 + 32768.0 / 65535.0));
        // the wiki's example: 0x96F33B00 is about -5875.23, not a two's complement value
        assert!((fixed(0x96F3_3B00) + 5875.2304).abs() < 1e-4);
    }

    #[test]
    fn halves() {
        assert_eq!(half(0x3C00), 1.0);
        assert_eq!(half(0xC000), -2.0);
        assert_eq!(half(0x7BFF), 65504.0);
        assert_eq!(half(0x0001), 2f32.powi(-24));
        assert_eq!(half(0x3555), 0.333_251_95);
        assert!(half(0x7E00).is_nan());
        assert_eq!(half(0xFC00), f32::NEG_INFINITY);
    }

    #[test]
    fn cells_from_paths() {
        let p = r"d:\x\zipcache\pc\tracks\brio\scene\proc\cellsize\75\-36_105\c75_grasstemplate_so_238.pgeo";
        let c = Cell::from_path(p).unwrap();
        assert_eq!(
            c,
            Cell {
                size: 75,
                x: -36,
                z: 105
            }
        );
        assert_eq!(c.area(), Area::new(-2700.0, 7875.0, -2625.0, 7950.0));
        assert_eq!(c.to_string(), r"75\-36_105");
        assert_eq!(
            Cell::from_path("a/cellsize/10000/-1_0/f.pgeo")
                .unwrap()
                .area()
                .min_x,
            -10000.0
        );
        for bad in [
            "no/cell/here.pgeo",
            "cellsize/75/1_2",
            "cellsize/75/1-2/f",
            "cellsize/075/1_2/f",
            "cellsize/0/1_2/f",
            "cellsize/75/-0_2/f",
            "cellsize/75/+1_2/f",
            "cellsize/75/1_2/cellsize/75/1_2/f",
        ] {
            assert_eq!(Cell::from_path(bad), None, "{bad}");
        }
    }

    #[test]
    fn areas() {
        let a = Area::new(0.0, 0.0, 10.0, 10.0);
        assert!(a.contains(0.0, 9.999));
        assert!(!a.contains(10.0, 5.0));
        assert!(a.overlaps(&Area::new(9.0, 9.0, 20.0, 20.0)));
        assert!(!a.overlaps(&Area::new(10.0, 0.0, 20.0, 10.0)));
    }

    #[test]
    fn season_masks() {
        assert_eq!(SeasonMask::new(0x12), None);
        let winter_only = SeasonMask::new(0x77).unwrap();
        assert!(winter_only.shows(Season::Winter));
        assert!(Season::ALL[..3].iter().all(|&s| winter_only.excludes(s)));
        let summer_only = SeasonMask::new(0xDD).unwrap();
        assert_eq!(
            Season::ALL.map(|s| summer_only.shows(s)),
            [false, true, false, false]
        );
    }
}
