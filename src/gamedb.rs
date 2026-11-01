//! The game database's second layer: `gamedbRC.slt` is a large-block [container](crate::container) whose decrypted
//! blocks are XORed with a keystream; removing it gives an SQLite database image.
//!
//! The keystream runs in 4-byte groups from the start of the decrypted data. Group `q` (bytes `4q..4q+4`) is XORed
//! with the little-endian bytes of the IEEE CRC-32 of the four little-endian bytes of `q + C × (q + 1)` (wrapping
//! 32-bit arithmetic), each byte in `A..=Z` first folded to lower case. `C` is a constant you supply
//! ([`crate::keys::Keys::gamedb_scramble`]). The XOR is its own inverse.
//!
//! This is the layer as a community tool that decrypts the file describes it; it has not been checked against the
//! game's own code. A database that decodes to `SQLite format 3` and passes SQLite's integrity check is the check
//! `tests/real_data.rs` makes when you have an install and its keys.
//!
//! The image is an SQLite 3 database; open it with `primase_foundations::sqlite::Database::open`. [`car_wheels`] reads
//! each car's stock tyre and rim sizes from it.
use crate::container::Container;
use crate::keys::{TransportKeys, UsageKeys};
use crate::{Error, Result};
use primase_foundations::hash::crc32;
use primase_foundations::sqlite::{Database, Row, Table, Value};
use std::collections::BTreeMap;

/// XORs `data`, taken from the start of the decrypted database, with the keystream of constant `scramble`. Applying
/// it to decrypted blocks gives the SQLite image; applying it to an image gives the blocks back.
pub fn descramble(data: &mut [u8], scramble: u32) {
    for (q, group) in (0u32..).zip(data.chunks_mut(4)) {
        let seed = q.wrapping_add(scramble.wrapping_mul(q.wrapping_add(1)));
        let folded = seed.to_le_bytes().map(|b| b.to_ascii_lowercase());
        for (byte, key) in group.iter_mut().zip(crc32(&folded).to_le_bytes()) {
            *byte ^= key;
        }
    }
}

/// The game database's SQLite image from its container: every block decrypted ([`Container::decrypt`], the padded
/// length; SQLite reads its own page count from the image's header), then descrambled.
pub fn decrypt(
    container: &Container<'_>,
    transport: &TransportKeys,
    keys: &UsageKeys,
    scramble: u32,
) -> Vec<u8> {
    let mut image = container.decrypt(transport, keys);
    descramble(&mut image, scramble);
    image
}

/// Whether `image` starts with SQLite's header string.
pub fn is_sqlite(image: &[u8]) -> bool {
    image.starts_with(b"SQLite format 3\0")
}

/// One axle's stock tyre and rim, as `Data_Car` stores them: the tyre size in the ISO metric notation's terms
/// (225/40 R19 is a width of 225 mm, an aspect ratio of 40 and a rim diameter of 19 inches).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Axle {
    /// The tyre's section width in millimetres (`...TireWidthMM`).
    pub tire_width_mm: u32,
    /// The tyre's aspect ratio: sidewall height as a percentage of the width (`...TireAspect`).
    pub tire_aspect: u32,
    /// The rim's diameter in inches (`...WheelDiameterIN`).
    pub rim_diameter_in: u32,
}

/// A car's stock wheels: a row of `Data_Car`, with its stock wheel's row of `List_Wheels`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CarWheels {
    /// The car's id (`Data_Car.Id`).
    pub car_id: i64,
    /// The car's media name (`Data_Car.MediaName`): the name of its archive under `media\Cars`.
    pub media_name: String,
    /// The front axle.
    pub front: Axle,
    /// The rear axle.
    pub rear: Axle,
    /// The stock wheel's id (`Data_Car.StockWheelID`, a `List_Wheels.ID`).
    pub stock_wheel_id: i64,
    /// The stock wheel's media name (`List_Wheels.MediaName`).
    pub stock_wheel: String,
}

/// Every car's stock tyre and rim sizes and stock wheel, in `Data_Car`'s row order. Refused: a missing table or
/// column, a value of another type than an integer (a text for the names), a size that is negative or past `u32`,
/// and a stock wheel id that `List_Wheels` does not hold.
pub fn car_wheels(db: &Database<'_>) -> Result<Vec<CarWheels>> {
    let cars = table(db, "Data_Car")?;
    let wheels = table(db, "List_Wheels")?;
    let (wheel_id, wheel_name) = (column(&wheels, "ID")?, column(&wheels, "MediaName")?);
    let mut names = BTreeMap::new();
    for row in &wheels.rows {
        names.insert(int(&wheels, row, wheel_id)?, text(&wheels, row, wheel_name)?);
    }
    let col = |name: &str| column(&cars, name);
    let (id, media) = (col("Id")?, col("MediaName")?);
    let axle = |prefix: &str| -> Result<[usize; 3]> {
        Ok([
            col(&format!("{prefix}TireWidthMM"))?,
            col(&format!("{prefix}TireAspect"))?,
            col(&format!("{prefix}WheelDiameterIN"))?,
        ])
    };
    let (front, rear, stock) = (axle("Front")?, axle("Rear")?, col("StockWheelID")?);
    let read_axle = |row: &Row, [w, a, d]: [usize; 3]| -> Result<Axle> {
        Ok(Axle {
            tire_width_mm: size(&cars, row, w)?,
            tire_aspect: size(&cars, row, a)?,
            rim_diameter_in: size(&cars, row, d)?,
        })
    };
    cars.rows
        .iter()
        .map(|row| {
            let stock_wheel_id = int(&cars, row, stock)?;
            let stock_wheel = names.get(&stock_wheel_id).cloned().ok_or_else(|| {
                fail(format!(
                    "Data_Car row {}: StockWheelID {stock_wheel_id} is not in List_Wheels",
                    row.rowid
                ))
            })?;
            Ok(CarWheels {
                car_id: int(&cars, row, id)?,
                media_name: text(&cars, row, media)?,
                front: read_axle(row, front)?,
                rear: read_axle(row, rear)?,
                stock_wheel_id,
                stock_wheel,
            })
        })
        .collect()
}

fn fail(detail: String) -> Error {
    Error::GameDb(detail)
}

fn table(db: &Database<'_>, name: &str) -> Result<Table> {
    db.table(name).map_err(|e| fail(format!("{name}: {e}")))
}

fn column(t: &Table, name: &str) -> Result<usize> {
    t.column(name)
        .ok_or_else(|| fail(format!("{} has no column {name}", t.name)))
}

fn value<'a>(t: &Table, row: &'a Row, c: usize) -> Result<&'a Value> {
    row.values
        .get(c)
        .ok_or_else(|| fail(format!("{} row {}: no column {c}", t.name, row.rowid)))
}

fn name_of(t: &Table, c: usize) -> &str {
    t.columns.get(c).map_or("?", String::as_str)
}

fn int(t: &Table, row: &Row, c: usize) -> Result<i64> {
    match value(t, row, c)? {
        Value::Int(n) => Ok(*n),
        v => Err(fail(format!(
            "{} row {} column {}: {v:?}, not an integer",
            t.name,
            row.rowid,
            name_of(t, c)
        ))),
    }
}

fn size(t: &Table, row: &Row, c: usize) -> Result<u32> {
    let n = int(t, row, c)?;
    u32::try_from(n).map_err(|_| {
        fail(format!(
            "{} row {} column {}: {n} is not a size",
            t.name,
            row.rowid,
            name_of(t, c)
        ))
    })
}

fn text(t: &Table, row: &Row, c: usize) -> Result<String> {
    match value(t, row, c)? {
        Value::Text(s) => Ok(s.clone()),
        v => Err(fail(format!(
            "{} row {} column {}: {v:?}, not a text",
            t.name,
            row.rowid,
            name_of(t, c)
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Keystreams worked by hand with an independent CRC-32 (Python's `zlib.crc32`) from the description above; both
    /// constants put a byte in `A..=Z` into a seed, so the case folding is exercised.
    #[test]
    fn keystream_matches_hand_worked_values() {
        for (constant, expected) in [
            (
                0x41,
                [
                    0x7a, 0x4f, 0xde, 0xa2, 0xc9, 0xc6, 0xa8, 0xde, 0x28, 0xc1, 0xd7, 0x60,
                ],
            ),
            (
                0x1234_5678,
                [
                    0x32, 0xc1, 0x20, 0x97, 0xa4, 0x0f, 0xdb, 0x5e, 0xec, 0x17, 0xcc, 0x7c,
                ],
            ),
        ] {
            let mut zeros = [0u8; 12];
            descramble(&mut zeros, constant);
            assert_eq!(zeros, expected, "constant {constant:#x}");
        }
    }

    #[test]
    fn descramble_is_its_own_inverse_and_handles_a_short_tail() {
        let original: Vec<u8> = (0..=254).collect(); // 255 bytes: the last group is 3 long
        let mut data = original.clone();
        descramble(&mut data, 0xdead_beef);
        assert_ne!(data, original);
        descramble(&mut data, 0xdead_beef);
        assert_eq!(data, original);
    }
}
