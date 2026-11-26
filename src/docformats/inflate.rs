//! A compact, dependency-free DEFLATE (RFC 1951) decompressor plus CRC-32
//! and gzip (RFC 1952) support. Modelled on zlib's `puff` reference decoder.

const LEN_BASE: [u16; 29] = [
    3, 4, 5, 6, 7, 8, 9, 10, 11, 13, 15, 17, 19, 23, 27, 31, 35, 43, 51, 59, 67, 83, 99, 115, 131, 163, 195, 227, 258,
];
const LEN_EXTRA: [u8; 29] = [0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 1, 1, 2, 2, 2, 2, 3, 3, 3, 3, 4, 4, 4, 4, 5, 5, 5, 5, 0];
const DIST_BASE: [u16; 30] = [
    1, 2, 3, 4, 5, 7, 9, 13, 17, 25, 33, 49, 65, 97, 129, 193, 257, 385, 513, 769, 1025, 1537, 2049, 3073, 4097,
    6145, 8193, 12289, 16385, 24577,
];
const DIST_EXTRA: [u8; 30] = [0, 0, 0, 0, 1, 1, 2, 2, 3, 3, 4, 4, 5, 5, 6, 6, 7, 7, 8, 8, 9, 9, 10, 10, 11, 11, 12, 12, 13, 13];
const CL_ORDER: [usize; 19] = [16, 17, 18, 0, 8, 7, 9, 6, 10, 5, 11, 4, 12, 3, 13, 2, 14, 1, 15];

struct Bits<'a> {
    data: &'a [u8],
    pos: usize,
    buf: u64,
    cnt: u32,
}

impl Bits<'_> {
    fn need(&mut self, n: u32) -> Result<u32, String> {
        while self.cnt < n {
            let b = *self.data.get(self.pos).ok_or("compressed data is truncated")?;
            self.buf |= u64::from(b) << self.cnt;
            self.pos += 1;
            self.cnt += 8;
        }
        let v = (self.buf & ((1u64 << n) - 1)) as u32;
        self.buf >>= n;
        self.cnt -= n;
        Ok(v)
    }

    /// Discards bits up to the next byte boundary and returns any whole
    /// bytes still buffered to the input stream.
    fn align(&mut self) {
        self.pos -= (self.cnt / 8) as usize;
        self.buf = 0;
        self.cnt = 0;
    }
}

struct Huffman {
    counts: [u16; 16],
    symbols: Vec<u16>,
}

impl Huffman {
    fn new(lengths: &[u8]) -> Result<Self, String> {
        let mut counts = [0u16; 16];
        for &l in lengths {
            counts[l as usize] += 1;
        }
        let mut offs = [0u16; 16];
        for len in 1..15 {
            offs[len + 1] = offs[len] + counts[len];
        }
        let mut symbols = vec![0u16; lengths.len()];
        for (sym, &l) in lengths.iter().enumerate() {
            if l != 0 {
                symbols[offs[l as usize] as usize] = sym as u16;
                offs[l as usize] += 1;
            }
        }
        Ok(Self { counts, symbols })
    }

    fn decode(&self, bits: &mut Bits) -> Result<u16, String> {
        let (mut code, mut first, mut index) = (0i32, 0i32, 0i32);
        for len in 1..16 {
            code |= bits.need(1)? as i32;
            let count = i32::from(self.counts[len]);
            if code - count < first {
                return self
                    .symbols
                    .get((index + (code - first)) as usize)
                    .copied()
                    .ok_or_else(|| "invalid Huffman code".to_string());
            }
            index += count;
            first += count;
            first <<= 1;
            code <<= 1;
        }
        Err("invalid Huffman code".into())
    }
}

/// Decompresses a raw DEFLATE stream. Output larger than `limit` is refused
/// (protection against "zip bombs"). Returns the data and bytes consumed.
pub fn inflate(data: &[u8], limit: usize) -> Result<(Vec<u8>, usize), String> {
    let mut bits = Bits { data, pos: 0, buf: 0, cnt: 0 };
    let mut out: Vec<u8> = Vec::with_capacity(data.len().saturating_mul(3).min(limit));
    loop {
        let last = bits.need(1)?;
        match bits.need(2)? {
            0 => {
                bits.align();
                let p = bits.pos;
                if p + 4 > data.len() {
                    return Err("compressed data is truncated".into());
                }
                let len = u16::from_le_bytes([data[p], data[p + 1]]) as usize;
                let nlen = u16::from_le_bytes([data[p + 2], data[p + 3]]) as usize;
                if len != !nlen & 0xffff {
                    return Err("corrupt stored block".into());
                }
                let start = p + 4;
                let block = data.get(start..start + len).ok_or("compressed data is truncated")?;
                out.extend_from_slice(block);
                bits.pos = start + len;
            }
            1 => {
                let mut l = [0u8; 288];
                l[..144].fill(8);
                l[144..256].fill(9);
                l[256..280].fill(7);
                l[280..].fill(8);
                let lit = Huffman::new(&l)?;
                let dist = Huffman::new(&[5u8; 30])?;
                codes(&mut bits, &mut out, &lit, &dist, limit)?;
            }
            2 => {
                let hlit = bits.need(5)? as usize + 257;
                let hdist = bits.need(5)? as usize + 1;
                let hclen = bits.need(4)? as usize + 4;
                let mut cl = [0u8; 19];
                for &i in &CL_ORDER[..hclen] {
                    cl[i] = bits.need(3)? as u8;
                }
                let clh = Huffman::new(&cl)?;
                let mut lengths = vec![0u8; hlit + hdist];
                let mut i = 0;
                while i < hlit + hdist {
                    let sym = clh.decode(&mut bits)?;
                    let (val, rep) = match sym {
                        0..=15 => (sym as u8, 1),
                        16 => {
                            let prev = *lengths.get(i.wrapping_sub(1)).ok_or("repeat with no previous length")?;
                            (prev, 3 + bits.need(2)? as usize)
                        }
                        17 => (0, 3 + bits.need(3)? as usize),
                        _ => (0, 11 + bits.need(7)? as usize),
                    };
                    if i + rep > lengths.len() {
                        return Err("too many code lengths".into());
                    }
                    lengths[i..i + rep].fill(val);
                    i += rep;
                }
                let lit = Huffman::new(&lengths[..hlit])?;
                let dist = Huffman::new(&lengths[hlit..])?;
                codes(&mut bits, &mut out, &lit, &dist, limit)?;
            }
            _ => return Err("invalid block type".into()),
        }
        if last == 1 {
            bits.align();
            return Ok((out, bits.pos));
        }
    }
}

fn codes(bits: &mut Bits, out: &mut Vec<u8>, lit: &Huffman, dist: &Huffman, limit: usize) -> Result<(), String> {
    loop {
        let sym = lit.decode(bits)? as usize;
        if sym < 256 {
            out.push(sym as u8);
        } else if sym == 256 {
            return Ok(());
        } else {
            let s = sym - 257;
            if s >= 29 {
                return Err("invalid length code".into());
            }
            let len = LEN_BASE[s] as usize + bits.need(u32::from(LEN_EXTRA[s]))? as usize;
            let ds = dist.decode(bits)? as usize;
            if ds >= 30 {
                return Err("invalid distance code".into());
            }
            let d = DIST_BASE[ds] as usize + bits.need(u32::from(DIST_EXTRA[ds]))? as usize;
            if d > out.len() {
                return Err("distance too far back".into());
            }
            let start = out.len() - d;
            for k in 0..len {
                out.push(out[start + k]);
            }
        }
        if out.len() > limit {
            return Err("decompressed data is too large".into());
        }
    }
}

/// Standard CRC-32 (IEEE 802.3), as used by ZIP and gzip.
pub fn crc32(data: &[u8]) -> u32 {
    static TABLE: std::sync::OnceLock<[u32; 256]> = std::sync::OnceLock::new();
    let table = TABLE.get_or_init(|| {
        let mut t = [0u32; 256];
        for (i, e) in t.iter_mut().enumerate() {
            let mut c = i as u32;
            for _ in 0..8 {
                c = if c & 1 != 0 { 0xEDB8_8320 ^ (c >> 1) } else { c >> 1 };
            }
            *e = c;
        }
        t
    });
    let mut c = 0xFFFF_FFFFu32;
    for &b in data {
        c = table[((c ^ u32::from(b)) & 0xff) as usize] ^ (c >> 8);
    }
    c ^ 0xFFFF_FFFF
}

/// Decompresses a gzip file (all concatenated members).
pub fn gunzip(data: &[u8], limit: usize) -> Result<Vec<u8>, String> {
    let mut out = Vec::new();
    let mut p = 0;
    while p + 18 <= data.len() && data[p] == 0x1f && data[p + 1] == 0x8b {
        if data[p + 2] != 8 {
            return Err("unsupported gzip compression method".into());
        }
        let flags = data[p + 3];
        let mut q = p + 10;
        if flags & 4 != 0 {
            let xlen = u16::from_le_bytes([data[q], *data.get(q + 1).ok_or("truncated gzip header")?]) as usize;
            q += 2 + xlen;
        }
        for bit in [8u8, 16] {
            if flags & bit != 0 {
                while *data.get(q).ok_or("truncated gzip header")? != 0 {
                    q += 1;
                }
                q += 1;
            }
        }
        if flags & 2 != 0 {
            q += 2;
        }
        let body = data.get(q..).ok_or("truncated gzip header")?;
        let (chunk, used) = inflate(body, limit.saturating_sub(out.len()))?;
        let trailer = q + used;
        let crc = data.get(trailer..trailer + 4).ok_or("truncated gzip trailer")?;
        if u32::from_le_bytes([crc[0], crc[1], crc[2], crc[3]]) != crc32(&chunk) {
            return Err("gzip checksum mismatch (file is corrupt)".into());
        }
        out.extend_from_slice(&chunk);
        p = trailer + 8;
    }
    if out.is_empty() && p == 0 {
        return Err("not a gzip file".into());
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    // Raw DEFLATE streams produced by Python's zlib (wbits=-15).
    const FIXED: &[u8] = &[0xcb, 0x48, 0xcd, 0xc9, 0xc9, 0x57, 0x28, 0xcf, 0x2f, 0xca, 0x49, 0x01, 0x00];

    #[test]
    fn fixed_huffman() {
        let (out, used) = inflate(FIXED, 1 << 20).unwrap();
        assert_eq!(out, b"hello world");
        assert_eq!(used, FIXED.len());
    }

    #[test]
    fn stored_block() {
        let data = [0x01, 0x03, 0x00, 0xfc, 0xff, b'a', b'b', b'c'];
        assert_eq!(inflate(&data, 100).unwrap().0, b"abc");
    }

    #[test]
    fn crc() {
        assert_eq!(crc32(b"123456789"), 0xCBF4_3926);
        assert_eq!(crc32(b""), 0);
    }

    #[test]
    fn rejects_garbage() {
        assert!(inflate(&[0xff, 0xff, 0xff], 100).is_err());
        assert!(gunzip(b"not gzip at all, sorry!", 100).is_err());
    }
}
