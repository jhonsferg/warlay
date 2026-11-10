//! A read-only reader for the SQLite files earlier versions wrote, in Rust and
//! only as much as it takes to import their small tables once. It understands
//! the database file format (a header, B-tree pages, records, overflow pages)
//! and nothing else: no SQL, no writing, no journals.
//!
//! Format reference: <https://www.sqlite.org/fileformat2.html>.

use std::path::Path;

/// A column value as stored.
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Null,
    Int(i64),
    Real(f64),
    Text(String),
    Blob(Vec<u8>),
}

#[derive(Debug, thiserror::Error)]
pub enum LegacyError {
    #[error("could not read the database file: {0}")]
    Io(#[from] std::io::Error),
    #[error("not a readable SQLite database: {0}")]
    Format(&'static str),
}

/// Large enough for anything these programs wrote, small enough to refuse
/// something that is not one of their files.
const MAX_FILE: u64 = 64 * 1024 * 1024;
const MAX_DEPTH: usize = 32;

/// The rows of `table`, with every column as stored (an `INTEGER PRIMARY KEY`
/// column is stored as NULL and is the row id, which is not returned).
/// `Ok(None)` when the file or the table is not there.
pub fn read_table(path: &Path, table: &str) -> Result<Option<Vec<Vec<Value>>>, LegacyError> {
    let metadata = match std::fs::metadata(path) {
        Ok(m) => m,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e.into()),
    };
    if metadata.len() > MAX_FILE {
        return Err(LegacyError::Format("file too large"));
    }
    let db = Db::parse(std::fs::read(path)?)?;
    // Page 1 holds `sqlite_master`: (type, name, tbl_name, rootpage, sql).
    let mut root = None;
    for row in db.rows(1)? {
        if let [Value::Text(kind), Value::Text(name), _, Value::Int(page), ..] = row.as_slice() {
            if kind == "table" && name == table {
                root = Some(*page);
            }
        }
    }
    match root {
        None => Ok(None),
        Some(page) => {
            let page = u32::try_from(page).map_err(|_| LegacyError::Format("bad root page"))?;
            Ok(Some(db.rows(page)?))
        }
    }
}

struct Db {
    data: Vec<u8>,
    page_size: usize,
    /// Page size minus the bytes reserved at the end of each page.
    usable: usize,
}

fn be16(bytes: &[u8], at: usize) -> Option<usize> {
    Some(u16::from_be_bytes(bytes.get(at..at + 2)?.try_into().ok()?) as usize)
}

fn be32(bytes: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_be_bytes(bytes.get(at..at + 4)?.try_into().ok()?))
}

/// A SQLite variable-length integer: `(value, bytes used)`.
fn varint(bytes: &[u8], at: usize) -> Option<(u64, usize)> {
    let mut value = 0_u64;
    for i in 0..9 {
        let byte = *bytes.get(at + i)?;
        if i == 8 {
            return Some(((value << 8) | u64::from(byte), 9));
        }
        value = (value << 7) | u64::from(byte & 0x7f);
        if byte & 0x80 == 0 {
            return Some((value, i + 1));
        }
    }
    None
}

impl Db {
    fn parse(data: Vec<u8>) -> Result<Self, LegacyError> {
        if data.len() < 100 || &data[..16] != b"SQLite format 3\0" {
            return Err(LegacyError::Format("missing header"));
        }
        let page_size = match be16(&data, 16) {
            Some(1) => 65_536,
            Some(n) if (512..=32_768).contains(&n) && n.is_power_of_two() => n,
            _ => return Err(LegacyError::Format("bad page size")),
        };
        let usable = page_size - data[20] as usize;
        if usable < 480 {
            return Err(LegacyError::Format("bad reserved space"));
        }
        // 1 = UTF-8, the default and what these programs always used.
        if be32(&data, 56) != Some(1) && be32(&data, 56) != Some(0) {
            return Err(LegacyError::Format("text is not UTF-8"));
        }
        Ok(Self {
            data,
            page_size,
            usable,
        })
    }

    fn page(&self, number: u32) -> Result<&[u8], LegacyError> {
        let start = (number as usize)
            .checked_sub(1)
            .and_then(|n| n.checked_mul(self.page_size))
            .ok_or(LegacyError::Format("bad page number"))?;
        self.data
            .get(start..start + self.page_size)
            .ok_or(LegacyError::Format("page beyond the end of the file"))
    }

    /// Every row of the table whose root is `root`, in row-id order.
    fn rows(&self, root: u32) -> Result<Vec<Vec<Value>>, LegacyError> {
        let mut rows = Vec::new();
        let mut visited = 0_usize;
        self.walk(root, 0, &mut rows, &mut visited)?;
        Ok(rows)
    }

    fn walk(
        &self,
        number: u32,
        depth: usize,
        out: &mut Vec<Vec<Value>>,
        visited: &mut usize,
    ) -> Result<(), LegacyError> {
        *visited += 1;
        // A cycle or a huge tree is a damaged file.
        if depth > MAX_DEPTH || *visited > self.data.len() / self.page_size + 1 {
            return Err(LegacyError::Format("page tree is too deep"));
        }
        let page = self.page(number)?;
        // The first page's B-tree header comes after the 100-byte file header.
        let base = if number == 1 { 100 } else { 0 };
        let kind = page[base];
        let cells = be16(page, base + 3).ok_or(LegacyError::Format("short page"))?;
        let header = match kind {
            0x0d => 8,
            0x05 => 12,
            _ => return Err(LegacyError::Format("not a table page")),
        };
        for index in 0..cells {
            let pointer = be16(page, base + header + index * 2).ok_or(LegacyError::Format("short page"))?;
            if kind == 0x05 {
                let child = be32(page, pointer).ok_or(LegacyError::Format("short cell"))?;
                self.walk(child, depth + 1, out, visited)?;
            } else {
                out.push(self.leaf_row(page, pointer)?);
            }
        }
        if kind == 0x05 {
            let last = be32(page, base + 8).ok_or(LegacyError::Format("short page"))?;
            self.walk(last, depth + 1, out, visited)?;
        }
        Ok(())
    }

    /// One table leaf cell: payload size, row id, then the record (its start
    /// on this page and, if it is large, the rest on a chain of overflow pages).
    fn leaf_row(&self, page: &[u8], at: usize) -> Result<Vec<Value>, LegacyError> {
        let short = LegacyError::Format("short cell");
        let (size, n1) = varint(page, at).ok_or(LegacyError::Format("short cell"))?;
        let (_rowid, n2) = varint(page, at + n1).ok_or(LegacyError::Format("short cell"))?;
        let size = usize::try_from(size).map_err(|_| LegacyError::Format("bad payload size"))?;
        let start = at + n1 + n2;
        let max_local = self.usable - 35;
        let local = if size <= max_local {
            size
        } else {
            let min_local = (self.usable - 12) * 32 / 255 - 23;
            let k = min_local + (size - min_local) % (self.usable - 4);
            if k <= max_local {
                k
            } else {
                min_local
            }
        };
        let mut payload = page.get(start..start + local).ok_or(short)?.to_vec();
        if size > local {
            let mut next = be32(page, start + local).ok_or(LegacyError::Format("short cell"))?;
            while payload.len() < size {
                if next == 0 {
                    return Err(LegacyError::Format("overflow chain ends early"));
                }
                let overflow = self.page(next)?;
                next = be32(overflow, 0).ok_or(LegacyError::Format("short page"))?;
                let take = (size - payload.len()).min(self.usable - 4);
                payload.extend_from_slice(overflow.get(4..4 + take).ok_or(LegacyError::Format("short page"))?);
            }
        }
        decode_record(&payload)
    }
}

fn decode_record(payload: &[u8]) -> Result<Vec<Value>, LegacyError> {
    let bad = LegacyError::Format("bad record");
    let (header_len, mut at) = varint(payload, 0).ok_or(LegacyError::Format("bad record"))?;
    let header_len = header_len as usize;
    let mut serials = Vec::new();
    while at < header_len {
        let (serial, used) = varint(payload, at).ok_or(LegacyError::Format("bad record"))?;
        serials.push(serial);
        at += used;
    }
    let mut cursor = header_len;
    let mut values = Vec::with_capacity(serials.len());
    for serial in serials {
        // Text is (n - 13) / 2 bytes, a blob (n - 12) / 2.
        let length = match serial {
            0 | 8 | 9 => 0,
            1 => 1,
            2 => 2,
            3 => 3,
            4 => 4,
            5 => 6,
            6 | 7 => 8,
            10 | 11 => return Err(bad),
            n if n % 2 == 0 => ((n - 12) / 2) as usize,
            n => ((n - 13) / 2) as usize,
        };
        let bytes = payload
            .get(cursor..cursor + length)
            .ok_or(LegacyError::Format("bad record"))?;
        cursor += length;
        values.push(match serial {
            0 => Value::Null,
            8 => Value::Int(0),
            9 => Value::Int(1),
            1..=6 => {
                // Big-endian two's complement of 1 to 8 bytes.
                let mut value = if bytes[0] & 0x80 != 0 { -1_i64 } else { 0 };
                for b in bytes {
                    value = (value << 8) | i64::from(*b);
                }
                Value::Int(value)
            }
            7 => Value::Real(f64::from_be_bytes(
                bytes.try_into().map_err(|_| LegacyError::Format("bad record"))?,
            )),
            n if n % 2 == 0 => Value::Blob(bytes.to_vec()),
            _ => Value::Text(String::from_utf8_lossy(bytes).into_owned()),
        });
    }
    Ok(values)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(name: &str) -> std::path::PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures").join(name)
    }

    fn text(value: &Value) -> &str {
        match value {
            Value::Text(t) => t,
            other => panic!("not text: {other:?}"),
        }
    }

    #[test]
    fn reads_a_small_table_written_by_sqlite() {
        let rows = read_table(&fixture("client.sqlite3"), "settings").unwrap().unwrap();
        assert_eq!(rows.len(), 4);
        let find = |key: &str| {
            rows.iter()
                .find(|r| text(&r[0]) == key)
                .map(|r| text(&r[1]).to_string())
        };
        assert_eq!(
            find("client_id").as_deref(),
            Some("0f8fad5b-d9cb-469f-a165-70867728950e")
        );
        assert!(find("app_settings").unwrap().contains("Frozen Throne.exe"));
        assert_eq!(find("running_version_since").as_deref(), Some("1700000000"));
    }

    /// Interior pages, an overflow chain, every integer width, floats, blobs,
    /// NULLs and UTF-8, at two page sizes.
    #[test]
    fn reads_every_row_of_a_table_with_interior_and_overflow_pages() {
        for name in ["big-4k.sqlite3", "big-1k.sqlite3"] {
            let rows = read_table(&fixture(name), "big").unwrap().unwrap();
            assert_eq!(rows.len(), 302, "{name}");
            // `id INTEGER PRIMARY KEY` is the row id and is stored as NULL.
            assert!(rows.iter().all(|r| r[0] == Value::Null), "{name}");
            assert_eq!(text(&rows[0][1]), "row-1-ñandú");
            assert_eq!(text(&rows[149][1]), "x".repeat(20_000), "{name}: the overflow chain");
            for (index, row) in rows.iter().take(300).enumerate() {
                let i = index as i64 + 1;
                let expected_n = if i % 2 == 1 { i * 1_000_003 - 150_000_000 } else { -i };
                assert_eq!(row[3], Value::Int(expected_n), "{name} row {i}");
                // SQLite stores a REAL that is a whole number (7 / 7) as an integer.
                let real = match row[4] {
                    Value::Real(r) => r,
                    Value::Int(n) => n as f64,
                    ref other => panic!("not a number: {other:?}"),
                };
                assert_eq!(real, i as f64 / 7.0, "{name} row {i}");
                let blob: Vec<u8> = (0..(i % 50)).map(|j| (j % 256) as u8).collect();
                assert_eq!(row[2], Value::Blob(blob), "{name} row {i}");
                assert_eq!(row[5], Value::Null);
            }
            assert_eq!(rows[300][3], Value::Int((1_i64 << 62) + 12_345));
            assert_eq!(rows[300][4], Value::Real(-0.5));
            assert_eq!(rows[301][3], Value::Int(0));
        }
    }

    #[test]
    fn finds_a_table_by_name_and_says_when_it_is_not_there() {
        let other = read_table(&fixture("big-4k.sqlite3"), "other").unwrap().unwrap();
        assert_eq!(other, vec![vec![Value::Text("hello".into())]]);
        assert_eq!(read_table(&fixture("big-4k.sqlite3"), "missing").unwrap(), None);
        assert_eq!(read_table(&fixture("does-not-exist.sqlite3"), "big").unwrap(), None);
    }

    #[test]
    fn refuses_files_that_are_not_sqlite_or_are_cut_short() {
        let dir = std::env::temp_dir().join(format!("warlay-legacy-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let junk = dir.join("junk.db");
        std::fs::write(&junk, vec![b'x'; 300]).unwrap();
        assert!(read_table(&junk, "settings").is_err());
        // A real database cut in the middle of its pages.
        let good = std::fs::read(fixture("big-4k.sqlite3")).unwrap();
        let cut = dir.join("cut.db");
        std::fs::write(&cut, &good[..good.len() / 2]).unwrap();
        assert!(read_table(&cut, "big").is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn varints_follow_the_sqlite_encoding() {
        assert_eq!(varint(&[0x05], 0), Some((5, 1)));
        assert_eq!(varint(&[0x81, 0x00], 0), Some((128, 2)));
        assert_eq!(varint(&[0xff; 9], 0), Some((u64::MAX, 9)));
        assert_eq!(varint(&[0x81], 0), None);
    }
}
