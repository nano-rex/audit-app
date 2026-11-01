//! DEFLATE (RFC 1951) compression and decompression, with the gzip (RFC 1952) and zlib
//! (RFC 1950) wrappers and their checksums.
//!
//! The compressor finds repeats with hash chains over a 32 KiB window and writes them with the
//! fixed Huffman codes; that is simple to review and compresses text (pages, JSON, workbook XML)
//! well. The decompressor reads every block type.

const LENGTH_BASE: [u16; 29] = [3, 4, 5, 6, 7, 8, 9, 10, 11, 13, 15, 17, 19, 23, 27, 31, 35, 43, 51, 59, 67, 83, 99, 115, 131, 163, 195, 227, 258];
const LENGTH_EXTRA: [u8; 29] = [0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 1, 1, 2, 2, 2, 2, 3, 3, 3, 3, 4, 4, 4, 4, 5, 5, 5, 5, 0];
const DISTANCE_BASE: [u16; 30] = [1, 2, 3, 4, 5, 7, 9, 13, 17, 25, 33, 49, 65, 97, 129, 193, 257, 385, 513, 769, 1025, 1537, 2049, 3073, 4097, 6145, 8193, 12289, 16385, 24577];
const DISTANCE_EXTRA: [u8; 30] = [0, 0, 0, 0, 1, 1, 2, 2, 3, 3, 4, 4, 5, 5, 6, 6, 7, 7, 8, 8, 9, 9, 10, 10, 11, 11, 12, 12, 13, 13];

static CRC_TABLE: std::sync::OnceLock<[u32; 256]> = std::sync::OnceLock::new();

pub fn crc32(data: &[u8]) -> u32 {
    let table = CRC_TABLE.get_or_init(|| {
        let mut table = [0u32; 256];
        for (index, slot) in table.iter_mut().enumerate() {
            let mut value = index as u32;
            for _ in 0..8 {
                value = if value & 1 != 0 { 0xEDB8_8320 ^ (value >> 1) } else { value >> 1 };
            }
            *slot = value;
        }
        table
    });
    let mut crc = !0u32;
    for &byte in data {
        crc = table[((crc ^ byte as u32) & 0xff) as usize] ^ (crc >> 8);
    }
    !crc
}

pub fn adler32(data: &[u8]) -> u32 {
    let (mut a, mut b) = (1u32, 0u32);
    for chunk in data.chunks(5552) {
        for &byte in chunk {
            a += byte as u32;
            b += a;
        }
        a %= 65521;
        b %= 65521;
    }
    b << 16 | a
}

struct BitWriter {
    out: Vec<u8>,
    bits: u64,
    count: u32,
}

impl BitWriter {
    fn put(&mut self, value: u32, width: u32) {
        self.bits |= (value as u64) << self.count;
        self.count += width;
        while self.count >= 8 {
            self.out.push(self.bits as u8);
            self.bits >>= 8;
            self.count -= 8;
        }
    }

    /// A Huffman code, which DEFLATE stores most significant bit first.
    fn code(&mut self, code: u32, width: u32) {
        let mut reversed = 0;
        for index in 0..width {
            reversed |= ((code >> index) & 1) << (width - 1 - index);
        }
        self.put(reversed, width);
    }

    fn finish(mut self) -> Vec<u8> {
        if self.count > 0 {
            self.out.push(self.bits as u8);
        }
        self.out
    }
}

fn fixed_literal(writer: &mut BitWriter, symbol: u32) {
    match symbol {
        0..=143 => writer.code(0x30 + symbol, 8),
        144..=255 => writer.code(0x190 + symbol - 144, 9),
        256..=279 => writer.code(symbol - 256, 7),
        _ => writer.code(0xC0 + symbol - 280, 8),
    }
}

fn write_length(writer: &mut BitWriter, length: usize) {
    let index = LENGTH_BASE.iter().rposition(|&base| base as usize <= length).unwrap_or(0);
    fixed_literal(writer, 257 + index as u32);
    let extra = LENGTH_EXTRA[index] as u32;
    if extra > 0 {
        writer.put((length - LENGTH_BASE[index] as usize) as u32, extra);
    }
}

fn write_distance(writer: &mut BitWriter, distance: usize) {
    let index = DISTANCE_BASE.iter().rposition(|&base| base as usize <= distance).unwrap_or(0);
    writer.code(index as u32, 5);
    let extra = DISTANCE_EXTRA[index] as u32;
    if extra > 0 {
        writer.put((distance - DISTANCE_BASE[index] as usize) as u32, extra);
    }
}

/// Raw DEFLATE data of `input`.
pub fn deflate(input: &[u8]) -> Vec<u8> {
    const WINDOW: usize = 32 * 1024;
    const HASH_SIZE: usize = 1 << 15;
    const MAX_CHAIN: usize = 64;
    let mut writer = BitWriter { out: Vec::with_capacity(input.len() / 3 + 16), bits: 0, count: 0 };
    writer.put(1, 1); // the only, final block
    writer.put(1, 2); // fixed Huffman codes
    let mut head = vec![usize::MAX; HASH_SIZE];
    let mut previous = vec![usize::MAX; input.len()];
    let hash = |position: usize| -> usize {
        let value = (input[position] as u32) << 16 | (input[position + 1] as u32) << 8 | input[position + 2] as u32;
        (value.wrapping_mul(2_654_435_761) >> 17) as usize & (HASH_SIZE - 1)
    };
    let insert = |position: usize, head: &mut Vec<usize>, previous: &mut Vec<usize>| {
        if position + 2 < input.len() {
            let key = hash(position);
            previous[position] = head[key];
            head[key] = position;
        }
    };
    let mut position = 0;
    while position < input.len() {
        let mut best_length = 0;
        let mut best_distance = 0;
        if position + 2 < input.len() {
            let mut candidate = head[hash(position)];
            let limit = (input.len() - position).min(258);
            let mut chain = 0;
            while candidate != usize::MAX && position - candidate <= WINDOW && chain < MAX_CHAIN {
                if input[candidate + best_length.min(limit - 1)] == input[position + best_length.min(limit - 1)] {
                    let mut length = 0;
                    while length < limit && input[candidate + length] == input[position + length] {
                        length += 1;
                    }
                    if length > best_length {
                        best_length = length;
                        best_distance = position - candidate;
                        if length == limit {
                            break;
                        }
                    }
                }
                candidate = previous[candidate];
                chain += 1;
            }
        }
        if best_length >= 3 {
            write_length(&mut writer, best_length);
            write_distance(&mut writer, best_distance);
            for offset in 0..best_length {
                insert(position + offset, &mut head, &mut previous);
            }
            position += best_length;
        } else {
            fixed_literal(&mut writer, input[position] as u32);
            insert(position, &mut head, &mut previous);
            position += 1;
        }
    }
    fixed_literal(&mut writer, 256);
    writer.finish()
}

pub fn gzip(input: &[u8]) -> Vec<u8> {
    // No file name, modification time zero (as gzip.compress(mtime=0)), unknown OS.
    let mut out = vec![0x1f, 0x8b, 8, 0, 0, 0, 0, 0, 0, 255];
    out.extend_from_slice(&deflate(input));
    out.extend_from_slice(&crc32(input).to_le_bytes());
    out.extend_from_slice(&(input.len() as u32).to_le_bytes());
    out
}

pub fn zlib(input: &[u8]) -> Vec<u8> {
    let mut out = vec![0x78, 0x9c];
    out.extend_from_slice(&deflate(input));
    out.extend_from_slice(&adler32(input).to_be_bytes());
    out
}

struct BitReader<'a> {
    data: &'a [u8],
    position: usize,
    bits: u64,
    count: u32,
}

impl BitReader<'_> {
    fn need(&mut self, width: u32) -> Result<(), String> {
        while self.count < width {
            let byte = *self.data.get(self.position).ok_or("compressed data ends early")?;
            self.position += 1;
            self.bits |= (byte as u64) << self.count;
            self.count += 8;
        }
        Ok(())
    }

    fn take(&mut self, width: u32) -> Result<u32, String> {
        if width == 0 {
            return Ok(0);
        }
        self.need(width)?;
        let value = (self.bits & ((1u64 << width) - 1)) as u32;
        self.bits >>= width;
        self.count -= width;
        Ok(value)
    }

    fn align(&mut self) {
        let skip = self.count % 8;
        self.bits >>= skip;
        self.count -= skip;
    }
}

/// A canonical Huffman decoding table: symbols by code length, as in zlib's puff.
struct Huffman {
    counts: [u16; 16],
    symbols: Vec<u16>,
}

impl Huffman {
    fn new(lengths: &[u8]) -> Huffman {
        let mut counts = [0u16; 16];
        for &length in lengths {
            counts[length as usize] += 1;
        }
        counts[0] = 0;
        let mut offsets = [0u16; 16];
        for length in 1..16 {
            offsets[length] = offsets[length - 1] + counts[length - 1];
        }
        let mut symbols = vec![0u16; lengths.len()];
        for (symbol, &length) in lengths.iter().enumerate() {
            if length != 0 {
                symbols[offsets[length as usize] as usize] = symbol as u16;
                offsets[length as usize] += 1;
            }
        }
        Huffman { counts, symbols }
    }

    fn decode(&self, reader: &mut BitReader) -> Result<u16, String> {
        let (mut code, mut first, mut index) = (0i32, 0i32, 0i32);
        for length in 1..16 {
            code |= reader.take(1)? as i32;
            let count = self.counts[length] as i32;
            if code - count < first {
                return self.symbols.get((index + code - first) as usize).copied().ok_or_else(|| "bad Huffman code".to_string());
            }
            index += count;
            first += count;
            first <<= 1;
            code <<= 1;
        }
        Err("bad Huffman code".into())
    }
}

/// Decompress raw DEFLATE data, refusing output beyond `limit` bytes.
pub fn inflate(data: &[u8], limit: usize) -> Result<Vec<u8>, String> {
    let mut reader = BitReader { data, position: 0, bits: 0, count: 0 };
    let mut out: Vec<u8> = Vec::new();
    loop {
        let last = reader.take(1)?;
        match reader.take(2)? {
            0 => {
                reader.align();
                let length = reader.take(16)? as usize;
                let complement = reader.take(16)? as usize;
                if length != !complement & 0xffff {
                    return Err("stored block length is corrupt".into());
                }
                for _ in 0..length {
                    out.push(reader.take(8)? as u8);
                }
            }
            kind @ (1 | 2) => {
                let (literals, distances) = if kind == 1 {
                    let mut lengths = [0u8; 288];
                    lengths[..144].fill(8);
                    lengths[144..256].fill(9);
                    lengths[256..280].fill(7);
                    lengths[280..].fill(8);
                    (Huffman::new(&lengths), Huffman::new(&[5u8; 30]))
                } else {
                    let literal_count = reader.take(5)? as usize + 257;
                    let distance_count = reader.take(5)? as usize + 1;
                    let code_count = reader.take(4)? as usize + 4;
                    const ORDER: [usize; 19] = [16, 17, 18, 0, 8, 7, 9, 6, 10, 5, 11, 4, 12, 3, 13, 2, 14, 1, 15];
                    let mut code_lengths = [0u8; 19];
                    for &slot in ORDER.iter().take(code_count) {
                        code_lengths[slot] = reader.take(3)? as u8;
                    }
                    let codes = Huffman::new(&code_lengths);
                    let mut lengths = Vec::with_capacity(literal_count + distance_count);
                    while lengths.len() < literal_count + distance_count {
                        let symbol = codes.decode(&mut reader)?;
                        match symbol {
                            0..=15 => lengths.push(symbol as u8),
                            16 => {
                                let previous = *lengths.last().ok_or("repeat with no previous length")?;
                                for _ in 0..3 + reader.take(2)? {
                                    lengths.push(previous);
                                }
                            }
                            17 => lengths.extend(std::iter::repeat_n(0, 3 + reader.take(3)? as usize)),
                            _ => lengths.extend(std::iter::repeat_n(0, 11 + reader.take(7)? as usize)),
                        }
                    }
                    if lengths.len() > literal_count + distance_count {
                        return Err("code lengths overrun".into());
                    }
                    (Huffman::new(&lengths[..literal_count]), Huffman::new(&lengths[literal_count..]))
                };
                loop {
                    let symbol = literals.decode(&mut reader)? as usize;
                    if symbol < 256 {
                        out.push(symbol as u8);
                    } else if symbol == 256 {
                        break;
                    } else {
                        let index = symbol - 257;
                        if index >= 29 {
                            return Err("bad length symbol".into());
                        }
                        let length = LENGTH_BASE[index] as usize + reader.take(LENGTH_EXTRA[index] as u32)? as usize;
                        let code = distances.decode(&mut reader)? as usize;
                        if code >= 30 {
                            return Err("bad distance symbol".into());
                        }
                        let distance = DISTANCE_BASE[code] as usize + reader.take(DISTANCE_EXTRA[code] as u32)? as usize;
                        if distance > out.len() {
                            return Err("distance reaches before the start".into());
                        }
                        let start = out.len() - distance;
                        for offset in 0..length {
                            out.push(out[start + offset]);
                        }
                    }
                    if out.len() > limit {
                        return Err("decompressed data is too large".into());
                    }
                }
            }
            _ => return Err("invalid block type".into()),
        }
        if out.len() > limit {
            return Err("decompressed data is too large".into());
        }
        if last == 1 {
            return Ok(out);
        }
    }
}

/// Decompress zlib-wrapped data (PNG image data), checking its checksum.
pub fn unzlib(data: &[u8], limit: usize) -> Result<Vec<u8>, String> {
    if data.len() < 6 || (u16::from(data[0]) << 8 | u16::from(data[1])) % 31 != 0 || data[0] & 0x0f != 8 {
        return Err("not zlib data".into());
    }
    if data[1] & 0x20 != 0 {
        return Err("preset dictionaries are not supported".into());
    }
    let out = inflate(&data[2..], limit)?;
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn checksums() {
        assert_eq!(crc32(b"123456789"), 0xCBF4_3926);
        assert_eq!(adler32(b"Wikipedia"), 0x11E6_0398);
    }

    #[test]
    fn deflate_round_trips() {
        let mut text = Vec::new();
        for index in 0..5000 {
            text.extend_from_slice(format!("{{\"id\":{index},\"name\":\"Item {}\",\"outlet\":\"STP\"}},", index % 37).as_bytes());
        }
        let packed = deflate(&text);
        assert!(packed.len() < text.len() / 4, "{} of {}", packed.len(), text.len());
        assert_eq!(inflate(&packed, usize::MAX).unwrap(), text);
        for sample in [&b""[..], b"a", b"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa", &[0u8, 255, 3, 7, 0, 255, 3, 7, 0, 255]] {
            assert_eq!(inflate(&deflate(sample), usize::MAX).unwrap(), sample);
        }
        let wrapped = zlib(b"hello hello hello");
        assert_eq!(unzlib(&wrapped, 1000).unwrap(), b"hello hello hello");
    }

    #[test]
    fn inflates_zlib_output() {
        // Python's zlib at level 9: a fixed-code block, then a dynamic-code one.
        let data = [0xcb, 0x48, 0xcd, 0xc9, 0xc9, 0x57, 0x28, 0xcf, 0x2f, 0xca, 0x49, 0x51, 0xc8, 0x40, 0xb0, 0x01];
        assert_eq!(inflate(&data, 1000).unwrap(), b"hello world hello world");
        let mut text = Vec::new();
        for index in 0..120usize {
            text.extend_from_slice(format!("line {index}: {}\n", "abcdefghij"[..index % 10].repeat(index % 7)).as_bytes());
        }
        let packed = "9d965972c2301044ff390547507b93c46d484212aaa8dcff3318b546bb6cc3626b1663b75af8f971ffbb9dd5e57c7aac035cce57371a9ea38feb870bc635f8741f97995e99aff8eb0ab32bdc6a1bd7b1b0e3bbbf75cdda5f99f147fdfcba840d89bb0ca822e859053d5f8c28cabd998bb5652231c9ef78590c330dcc6ab9a474c77224a1b56767555b6740cb44f60019bd24732c5219672632db7090d54df7d8d79a8bce040d46ae3a9e002e4111373a4f19449632531aca4265cda6d2c759ce213219177298ef78ca8e9d5ef26f252a27ae5c2f741aa4e2043288443153fac742c73a76ec338dcd3b96737d4a26eba5cc22777ec9e5583c659c39ca6cc34e567bf7a16852e645ae208867a2229095bea7bccd89b6c55bc930b90b7991cb28ed14c62811c25cd549d6363c64d701f378c4d17b52361d5aa64307c8e88432bac48c6e72466f83461f238da04617acd11dd818116612d8980a6c4c073646606352d8982a6c4c1f3666376ccc7bb0b122db0a6c6c041b9bc1c6566163bbb0b13b6163df808d15d8d8123650e10942c5b8812a7803d5020ed42671a0e670a28c39500de8406d5207ea1076103f3165e00186508bd10314ec015af00136e9031cc10ff0367fb03e5f9dfe01";
        let bytes: Vec<u8> = (0..packed.len()).step_by(2).map(|index| u8::from_str_radix(&packed[index..index + 2], 16).unwrap()).collect();
        assert_eq!(inflate(&bytes, 10_000).unwrap(), text);
        assert!(inflate(&bytes, 100).is_err());
    }
}
