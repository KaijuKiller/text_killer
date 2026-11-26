//! Minimal ZIP archive reader (stored + deflate) and writer (stored), enough
//! for OpenDocument, Office Open XML and EPUB containers.

use super::inflate::{crc32, inflate};

/// Maximum size of any single decompressed entry.
const ENTRY_LIMIT: usize = 256 * 1024 * 1024;

pub struct ZipEntry {
    pub name: String,
    method: u16,
    comp_size: usize,
    size: usize,
    offset: usize,
}

pub struct ZipArchive<'a> {
    data: &'a [u8],
    pub entries: Vec<ZipEntry>,
}

fn u16_at(d: &[u8], p: usize) -> Result<u16, String> {
    d.get(p..p + 2).map(|b| u16::from_le_bytes([b[0], b[1]])).ok_or_else(|| "truncated ZIP archive".into())
}

fn u32_at(d: &[u8], p: usize) -> Result<u32, String> {
    d.get(p..p + 4)
        .map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
        .ok_or_else(|| "truncated ZIP archive".into())
}

pub fn is_zip(data: &[u8]) -> bool {
    data.starts_with(b"PK\x03\x04")
}

impl<'a> ZipArchive<'a> {
    pub fn parse(data: &'a [u8]) -> Result<Self, String> {
        // The end-of-central-directory record is within the last 64 KiB + 22 bytes.
        let min = data.len().saturating_sub(65_557);
        let eocd = (min..data.len().saturating_sub(21))
            .rev()
            .find(|&i| data[i..].starts_with(b"PK\x05\x06"))
            .ok_or("not a ZIP archive (no central directory)")?;
        let count = u16_at(data, eocd + 10)? as usize;
        let cd_offset = u32_at(data, eocd + 16)? as usize;
        if cd_offset == 0xFFFF_FFFF || count == 0xFFFF {
            return Err("ZIP64 archives are not supported".into());
        }
        let mut entries = Vec::with_capacity(count);
        let mut p = cd_offset;
        for _ in 0..count {
            if u32_at(data, p)? != 0x0201_4b50 {
                return Err("corrupt ZIP central directory".into());
            }
            let flags = u16_at(data, p + 8)?;
            let method = u16_at(data, p + 10)?;
            let comp_size = u32_at(data, p + 20)? as usize;
            let size = u32_at(data, p + 24)? as usize;
            let name_len = u16_at(data, p + 28)? as usize;
            let extra_len = u16_at(data, p + 30)? as usize;
            let comment_len = u16_at(data, p + 32)? as usize;
            let offset = u32_at(data, p + 42)? as usize;
            if flags & 1 != 0 {
                return Err("encrypted (password-protected) documents are not supported".into());
            }
            let raw = data.get(p + 46..p + 46 + name_len).ok_or("truncated ZIP archive")?;
            let name = String::from_utf8_lossy(raw).into_owned();
            entries.push(ZipEntry { name, method, comp_size, size, offset });
            p += 46 + name_len + extra_len + comment_len;
        }
        Ok(Self { data, entries })
    }

    pub fn find(&self, name: &str) -> Option<&ZipEntry> {
        self.entries.iter().find(|e| e.name == name)
    }

    pub fn read(&self, e: &ZipEntry) -> Result<Vec<u8>, String> {
        let p = e.offset;
        if u32_at(self.data, p)? != 0x0403_4b50 {
            return Err(format!("corrupt ZIP entry {}", e.name));
        }
        let start = p + 30 + u16_at(self.data, p + 26)? as usize + u16_at(self.data, p + 28)? as usize;
        let raw = self.data.get(start..start + e.comp_size).ok_or("truncated ZIP entry")?;
        match e.method {
            0 => Ok(raw.to_vec()),
            8 => {
                let limit = if e.size > 0 { e.size.min(ENTRY_LIMIT) } else { ENTRY_LIMIT };
                inflate(raw, limit).map(|(d, _)| d)
            }
            m => Err(format!("unsupported ZIP compression method {m} in {}", e.name)),
        }
    }

    /// Reads an entry as UTF-8 text.
    pub fn read_text(&self, name: &str) -> Result<String, String> {
        let e = self.find(name).ok_or_else(|| format!("missing {name}"))?;
        let bytes = self.read(e)?;
        let bytes = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]).unwrap_or(&bytes);
        Ok(String::from_utf8_lossy(bytes).into_owned())
    }
}

/// Builds a ZIP archive with uncompressed ("stored") entries.
#[derive(Default)]
pub struct ZipWriter {
    out: Vec<u8>,
    central: Vec<u8>,
    count: u16,
}

impl ZipWriter {
    pub fn add(&mut self, name: &str, data: &[u8]) {
        let crc = crc32(data);
        let offset = self.out.len() as u32;
        let (time, date) = (0u16, 0x0021u16); // 1980-01-01 00:00
        let header = |sig: u32, central: bool| {
            let mut h = Vec::with_capacity(46 + name.len());
            h.extend_from_slice(&sig.to_le_bytes());
            if central {
                h.extend_from_slice(&20u16.to_le_bytes()); // version made by
            }
            h.extend_from_slice(&20u16.to_le_bytes()); // version needed
            h.extend_from_slice(&0x0800u16.to_le_bytes()); // UTF-8 names
            h.extend_from_slice(&0u16.to_le_bytes()); // stored
            h.extend_from_slice(&time.to_le_bytes());
            h.extend_from_slice(&date.to_le_bytes());
            h.extend_from_slice(&crc.to_le_bytes());
            h.extend_from_slice(&(data.len() as u32).to_le_bytes());
            h.extend_from_slice(&(data.len() as u32).to_le_bytes());
            h.extend_from_slice(&(name.len() as u16).to_le_bytes());
            h.extend_from_slice(&0u16.to_le_bytes()); // extra length
            if central {
                h.extend_from_slice(&[0; 6]); // comment len, disk start, internal attrs
                h.extend_from_slice(&0u32.to_le_bytes()); // ext attrs
                h.extend_from_slice(&offset.to_le_bytes());
            }
            h.extend_from_slice(name.as_bytes());
            h
        };
        self.out.extend(header(0x0403_4b50, false));
        self.out.extend_from_slice(data);
        self.central.extend(header(0x0201_4b50, true));
        self.count += 1;
    }

    pub fn finish(mut self) -> Vec<u8> {
        let cd_offset = self.out.len() as u32;
        let cd_size = self.central.len() as u32;
        self.out.extend_from_slice(&self.central);
        self.out.extend_from_slice(&0x0605_4b50u32.to_le_bytes());
        self.out.extend_from_slice(&[0; 4]);
        self.out.extend_from_slice(&self.count.to_le_bytes());
        self.out.extend_from_slice(&self.count.to_le_bytes());
        self.out.extend_from_slice(&cd_size.to_le_bytes());
        self.out.extend_from_slice(&cd_offset.to_le_bytes());
        self.out.extend_from_slice(&0u16.to_le_bytes());
        self.out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip() {
        let mut w = ZipWriter::default();
        w.add("mimetype", b"text/plain");
        w.add("dir/ünï.txt", "héllo".as_bytes());
        let bytes = w.finish();
        assert!(is_zip(&bytes));
        let z = ZipArchive::parse(&bytes).unwrap();
        assert_eq!(z.entries.len(), 2);
        assert_eq!(z.read_text("dir/ünï.txt").unwrap(), "héllo");
        assert_eq!(z.read_text("mimetype").unwrap(), "text/plain");
        assert!(z.read_text("nope").is_err());
    }
}
