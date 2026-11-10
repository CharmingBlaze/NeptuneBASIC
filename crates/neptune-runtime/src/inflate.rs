//! Base64 and DEFLATE decoding, for compressed tile layers and `.aseprite`
//! files (Tiled's zlib and gzip layer formats, Aseprite's cels).
//!
//! Written here so that reading a map needs no extra crate. Only decoding.

/// Refuses data that would inflate past this, so a damaged file can't use up
/// all the memory.
const MAX_OUTPUT: usize = 512 * 1024 * 1024;

pub fn base64(text: &str) -> Result<Vec<u8>, String> {
    let mut out = Vec::with_capacity(text.len() * 3 / 4);
    let (mut acc, mut bits) = (0u32, 0u32);
    for c in text.bytes() {
        let v = match c {
            b'A'..=b'Z' => c - b'A',
            b'a'..=b'z' => c - b'a' + 26,
            b'0'..=b'9' => c - b'0' + 52,
            b'+' => 62,
            b'/' => 63,
            b'=' | b' ' | b'\n' | b'\r' | b'\t' => continue,
            _ => return Err("bad Base64 data".into()),
        };
        acc = (acc << 6) | v as u32;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((acc >> bits) as u8);
            acc &= (1 << bits) - 1;
        }
    }
    Ok(out)
}

/// Data with a zlib header.
pub fn zlib(data: &[u8]) -> Result<Vec<u8>, String> {
    if data.len() < 2 || data[0] & 0x0F != 8 || !(data[0] as u32 * 256 + data[1] as u32).is_multiple_of(31) {
        return Err("bad zlib data".into());
    }
    if data[1] & 0x20 != 0 {
        return Err("zlib data with a preset dictionary is not supported".into());
    }
    inflate(&data[2..])
}

/// Data with a gzip header.
pub fn gzip(data: &[u8]) -> Result<Vec<u8>, String> {
    if data.len() < 10 || data[0] != 0x1F || data[1] != 0x8B || data[2] != 8 {
        return Err("bad gzip data".into());
    }
    let flags = data[3];
    let mut at = 10;
    if flags & 4 != 0 {
        let extra = *data.get(at).ok_or("bad gzip data")? as usize + ((*data.get(at + 1).ok_or("bad gzip data")? as usize) << 8);
        at += 2 + extra;
    }
    for flag in [8, 16] {
        if flags & flag != 0 {
            at += data.get(at..).ok_or("bad gzip data")?.iter().position(|&b| b == 0).ok_or("bad gzip data")? + 1;
        }
    }
    if flags & 2 != 0 {
        at += 2;
    }
    inflate(data.get(at..).ok_or("bad gzip data")?)
}

struct Bits<'a> {
    data: &'a [u8],
    at: usize,
    acc: u32,
    count: u32,
}

impl Bits<'_> {
    fn take(&mut self, n: u32) -> Result<u32, String> {
        while self.count < n {
            let b = *self.data.get(self.at).ok_or("compressed data ends early")?;
            self.at += 1;
            self.acc |= (b as u32) << self.count;
            self.count += 8;
        }
        let v = self.acc & ((1u32 << n) - 1);
        self.acc >>= n;
        self.count -= n;
        Ok(v)
    }
}

/// A canonical Huffman code: how many codes of each length, and their symbols.
struct Huffman {
    count: [u16; 16],
    symbol: Vec<u16>,
}

impl Huffman {
    fn new(lengths: &[u8]) -> Huffman {
        let mut count = [0u16; 16];
        for &l in lengths {
            count[l as usize] += 1;
        }
        count[0] = 0;
        let mut offset = [0u16; 16];
        for i in 1..15 {
            offset[i + 1] = offset[i] + count[i];
        }
        let mut symbol = vec![0u16; lengths.len()];
        for (s, &l) in lengths.iter().enumerate().filter(|(_, l)| **l != 0) {
            symbol[offset[l as usize] as usize] = s as u16;
            offset[l as usize] += 1;
        }
        Huffman { count, symbol }
    }

    fn decode(&self, bits: &mut Bits) -> Result<u16, String> {
        let (mut code, mut first, mut index) = (0i32, 0i32, 0i32);
        for len in 1..16 {
            code |= bits.take(1)? as i32;
            let count = self.count[len] as i32;
            if code - count < first {
                return Ok(self.symbol[(index + code - first) as usize]);
            }
            index += count;
            first = (first + count) << 1;
            code <<= 1;
        }
        Err("bad compressed data".into())
    }
}

const LEN_BASE: [u16; 29] = [3, 4, 5, 6, 7, 8, 9, 10, 11, 13, 15, 17, 19, 23, 27, 31, 35, 43, 51, 59, 67, 83, 99, 115, 131, 163, 195, 227, 258];
const LEN_EXTRA: [u8; 29] = [0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 1, 1, 2, 2, 2, 2, 3, 3, 3, 3, 4, 4, 4, 4, 5, 5, 5, 5, 0];
const DIST_BASE: [u16; 30] = [1, 2, 3, 4, 5, 7, 9, 13, 17, 25, 33, 49, 65, 97, 129, 193, 257, 385, 513, 769, 1025, 1537, 2049, 3073, 4097, 6145, 8193, 12289, 16385, 24577];
const DIST_EXTRA: [u8; 30] = [0, 0, 0, 0, 1, 1, 2, 2, 3, 3, 4, 4, 5, 5, 6, 6, 7, 7, 8, 8, 9, 9, 10, 10, 11, 11, 12, 12, 13, 13];

fn codes(bits: &mut Bits, out: &mut Vec<u8>, lit: &Huffman, dist: &Huffman) -> Result<(), String> {
    loop {
        let sym = lit.decode(bits)? as usize;
        match sym {
            0..=255 => out.push(sym as u8),
            256 => return Ok(()),
            _ => {
                let s = sym - 257;
                let len = *LEN_BASE.get(s).ok_or("bad compressed data")? as usize + bits.take(LEN_EXTRA[s] as u32)? as usize;
                let d = dist.decode(bits)? as usize;
                let back = *DIST_BASE.get(d).ok_or("bad compressed data")? as usize + bits.take(DIST_EXTRA[d] as u32)? as usize;
                if back > out.len() || out.len() + len > MAX_OUTPUT {
                    return Err("bad compressed data".into());
                }
                let start = out.len() - back;
                for i in 0..len {
                    out.push(out[start + i]);
                }
            }
        }
        if out.len() > MAX_OUTPUT {
            return Err("compressed data is too large".into());
        }
    }
}

/// Raw DEFLATE.
pub fn inflate(data: &[u8]) -> Result<Vec<u8>, String> {
    let mut bits = Bits { data, at: 0, acc: 0, count: 0 };
    let mut out = Vec::new();
    loop {
        let last = bits.take(1)? == 1;
        match bits.take(2)? {
            0 => {
                bits.acc = 0;
                bits.count = 0;
                let head = data.get(bits.at..bits.at + 4).ok_or("compressed data ends early")?;
                let len = u16::from_le_bytes([head[0], head[1]]);
                if len != !u16::from_le_bytes([head[2], head[3]]) {
                    return Err("bad compressed data".into());
                }
                bits.at += 4;
                out.extend_from_slice(data.get(bits.at..bits.at + len as usize).ok_or("compressed data ends early")?);
                bits.at += len as usize;
            }
            1 => {
                let mut l = [8u8; 288];
                l[144..256].fill(9);
                l[256..280].fill(7);
                codes(&mut bits, &mut out, &Huffman::new(&l), &Huffman::new(&[5u8; 30]))?;
            }
            2 => {
                let lit_n = bits.take(5)? as usize + 257;
                let dist_n = bits.take(5)? as usize + 1;
                let code_n = bits.take(4)? as usize + 4;
                const ORDER: [usize; 19] = [16, 17, 18, 0, 8, 7, 9, 6, 10, 5, 11, 4, 12, 3, 13, 2, 14, 1, 15];
                let mut code_len = [0u8; 19];
                for &slot in &ORDER[..code_n] {
                    code_len[slot] = bits.take(3)? as u8;
                }
                let table = Huffman::new(&code_len);
                let mut lengths = vec![0u8; lit_n + dist_n];
                let mut i = 0;
                while i < lengths.len() {
                    let sym = table.decode(&mut bits)?;
                    if sym < 16 {
                        lengths[i] = sym as u8;
                        i += 1;
                        continue;
                    }
                    let (value, repeat) = match sym {
                        16 if i > 0 => (lengths[i - 1], 3 + bits.take(2)? as usize),
                        17 => (0, 3 + bits.take(3)? as usize),
                        18 => (0, 11 + bits.take(7)? as usize),
                        _ => return Err("bad compressed data".into()),
                    };
                    if i + repeat > lengths.len() {
                        return Err("bad compressed data".into());
                    }
                    lengths[i..i + repeat].fill(value);
                    i += repeat;
                }
                codes(&mut bits, &mut out, &Huffman::new(&lengths[..lit_n]), &Huffman::new(&lengths[lit_n..]))?;
            }
            _ => return Err("bad compressed data".into()),
        }
        if last {
            return Ok(out);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base64_decodes() {
        assert_eq!(base64("aGVsbG8=").unwrap(), b"hello");
        assert_eq!(base64("aGVs\n bG8=").unwrap(), b"hello");
        assert!(base64("a*b").is_err());
    }

    // produced by Python's zlib and gzip modules, level 9: dynamic Huffman codes
    const TEXT: &[u8] = b"tile tile tile tile map map map map 0123456789 tile map";

    #[test]
    fn stored_and_fixed_blocks() {
        assert_eq!(inflate(&[1, 3, 0, 0xFC, 0xFF, b'a', b'b', b'c']).unwrap(), b"abc");
        // "aaaaaaaaaa", a fixed block with a back reference
        assert_eq!(inflate(&[0x4B, 0x4C, 0x84, 0x01, 0x00]).unwrap(), b"aaaaaaaaaa");
    }

    #[test]
    fn bad_data_is_an_error() {
        assert!(zlib(&[0, 0]).is_err());
        assert!(gzip(&[1, 2, 3]).is_err());
        assert!(inflate(&[7]).is_err());
        assert!(inflate(&[]).is_err());
    }

    #[test]
    fn zlib_and_gzip_samples() {
        let z = base64(ZLIB_SAMPLE).unwrap();
        assert_eq!(zlib(&z).unwrap(), TEXT);
        let g = base64(GZIP_SAMPLE).unwrap();
        assert_eq!(gzip(&g).unwrap(), TEXT);
    }

    const ZLIB_SAMPLE: &str = "eNorycxJVShBJXITC1CwgaGRsYmpmbmFJVweABAQEeo=";
    const GZIP_SAMPLE: &str = "H4sICAAAAAAC/2EAK8nMSVUoQSVyEwtQsIGhkbGJqZm5hSVcHgC5gyMhNwAAAA==";
}
