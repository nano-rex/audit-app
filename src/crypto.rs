//! SHA-256, HMAC, PBKDF2, randomness, and the text encodings around them.
//!
//! SHA-256 follows FIPS 180-4; PBKDF2-HMAC-SHA256 follows RFC 8018. Both are checked against
//! the published test vectors below. Random bytes come from the kernel (/dev/urandom).

use std::io::Read;

const K: [u32; 64] = [
    0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4, 0xab1c5ed5, 0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3,
    0x72be5d74, 0x80deb1fe, 0x9bdc06a7, 0xc19bf174, 0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f, 0x4a7484aa, 0x5cb0a9dc, 0x76f988da,
    0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7, 0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967, 0x27b70a85, 0x2e1b2138, 0x4d2c6dfc, 0x53380d13,
    0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85, 0xa2bfe8a1, 0xa81a664b, 0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070,
    0x19a4c116, 0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3, 0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208,
    0x90befffa, 0xa4506ceb, 0xbef9a3f7, 0xc67178f2,
];

const INITIAL: [u32; 8] = [0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab, 0x5be0cd19];

fn compress(state: &mut [u32; 8], block: &[u8]) {
    let mut w = [0u32; 64];
    for (index, chunk) in block.chunks_exact(4).take(16).enumerate() {
        w[index] = u32::from_be_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]);
    }
    for index in 16..64 {
        let s0 = w[index - 15].rotate_right(7) ^ w[index - 15].rotate_right(18) ^ (w[index - 15] >> 3);
        let s1 = w[index - 2].rotate_right(17) ^ w[index - 2].rotate_right(19) ^ (w[index - 2] >> 10);
        w[index] = w[index - 16].wrapping_add(s0).wrapping_add(w[index - 7]).wrapping_add(s1);
    }
    let [mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut h] = *state;
    for index in 0..64 {
        let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
        let choose = (e & f) ^ (!e & g);
        let temp1 = h.wrapping_add(s1).wrapping_add(choose).wrapping_add(K[index]).wrapping_add(w[index]);
        let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
        let majority = (a & b) ^ (a & c) ^ (b & c);
        let temp2 = s0.wrapping_add(majority);
        h = g;
        g = f;
        f = e;
        e = d.wrapping_add(temp1);
        d = c;
        c = b;
        b = a;
        a = temp1.wrapping_add(temp2);
    }
    for (slot, value) in state.iter_mut().zip([a, b, c, d, e, f, g, h]) {
        *slot = slot.wrapping_add(value);
    }
}

/// An incremental SHA-256 hash.
#[derive(Clone)]
pub struct Sha256 {
    state: [u32; 8],
    buffer: [u8; 64],
    filled: usize,
    length: u64,
}

impl Default for Sha256 {
    fn default() -> Self {
        Self::new()
    }
}

impl Sha256 {
    pub fn new() -> Sha256 {
        Sha256 { state: INITIAL, buffer: [0; 64], filled: 0, length: 0 }
    }

    pub fn update(&mut self, mut data: &[u8]) {
        self.length += data.len() as u64;
        if self.filled > 0 {
            let take = (64 - self.filled).min(data.len());
            self.buffer[self.filled..self.filled + take].copy_from_slice(&data[..take]);
            self.filled += take;
            data = &data[take..];
            if self.filled < 64 {
                return;
            }
            let block = self.buffer;
            compress(&mut self.state, &block);
            self.filled = 0;
        }
        let mut chunks = data.chunks_exact(64);
        for block in &mut chunks {
            compress(&mut self.state, block);
        }
        let rest = chunks.remainder();
        self.buffer[..rest.len()].copy_from_slice(rest);
        self.filled = rest.len();
    }

    pub fn finish(mut self) -> [u8; 32] {
        let bits = self.length.wrapping_mul(8);
        let mut tail = vec![0x80u8];
        while (self.filled + tail.len()) % 64 != 56 {
            tail.push(0);
        }
        tail.extend_from_slice(&bits.to_be_bytes());
        let length = self.length;
        self.update(&tail);
        self.length = length;
        let mut out = [0u8; 32];
        for (index, word) in self.state.iter().enumerate() {
            out[index * 4..index * 4 + 4].copy_from_slice(&word.to_be_bytes());
        }
        out
    }
}

pub fn sha256(data: &[u8]) -> [u8; 32] {
    let mut hash = Sha256::new();
    hash.update(data);
    hash.finish()
}

pub fn sha256_hex(data: &[u8]) -> String {
    hex(&sha256(data))
}

/// HMAC-SHA256 with its keyed inner and outer states prepared once.
#[derive(Clone)]
struct Hmac {
    inner: Sha256,
    outer: Sha256,
}

impl Hmac {
    fn new(key: &[u8]) -> Hmac {
        let mut block = [0u8; 64];
        if key.len() > 64 {
            block[..32].copy_from_slice(&sha256(key));
        } else {
            block[..key.len()].copy_from_slice(key);
        }
        let mut inner = Sha256::new();
        inner.update(&block.map(|byte| byte ^ 0x36));
        let mut outer = Sha256::new();
        outer.update(&block.map(|byte| byte ^ 0x5c));
        Hmac { inner, outer }
    }

    fn mac(&self, message: &[u8]) -> [u8; 32] {
        let mut inner = self.inner.clone();
        inner.update(message);
        let mut outer = self.outer.clone();
        outer.update(&inner.finish());
        outer.finish()
    }

    /// The MAC of a 32-byte message, computed on the raw states: PBKDF2's inner loop runs this
    /// hundreds of thousands of times, so it avoids the general buffering.
    fn mac32(&self, message: &[u8; 32]) -> [u8; 32] {
        let mut block = [0u8; 64];
        block[..32].copy_from_slice(message);
        block[32] = 0x80;
        block[62..64].copy_from_slice(&((64u64 + 32) * 8).to_be_bytes()[6..8]);
        let mut state = self.inner.state;
        compress(&mut state, &block);
        let mut digest = [0u8; 32];
        for (index, word) in state.iter().enumerate() {
            digest[index * 4..index * 4 + 4].copy_from_slice(&word.to_be_bytes());
        }
        block[..32].copy_from_slice(&digest);
        let mut state = self.outer.state;
        compress(&mut state, &block);
        for (index, word) in state.iter().enumerate() {
            digest[index * 4..index * 4 + 4].copy_from_slice(&word.to_be_bytes());
        }
        digest
    }
}

pub fn hmac_sha256(key: &[u8], message: &[u8]) -> [u8; 32] {
    Hmac::new(key).mac(message)
}

/// PBKDF2-HMAC-SHA256 producing one 32-byte block (the length hashlib returns by default).
pub fn pbkdf2_sha256(password: &[u8], salt: &[u8], iterations: u32) -> [u8; 32] {
    let hmac = Hmac::new(password);
    let mut first = salt.to_vec();
    first.extend_from_slice(&1u32.to_be_bytes());
    let mut u = hmac.mac(&first);
    let mut result = u;
    for _ in 1..iterations.max(1) {
        u = hmac.mac32(&u);
        for (slot, byte) in result.iter_mut().zip(u.iter()) {
            *slot ^= byte;
        }
    }
    result
}

/// Equality that takes the same time wherever the inputs differ.
pub fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

pub fn random_bytes(count: usize) -> Vec<u8> {
    let mut bytes = vec![0u8; count];
    let mut source = std::fs::File::open("/dev/urandom").expect("the kernel's random source /dev/urandom is required");
    source.read_exact(&mut bytes).expect("reading /dev/urandom");
    bytes
}

/// Python's secrets.token_hex(n).
pub fn token_hex(count: usize) -> String {
    hex(&random_bytes(count))
}

/// Python's secrets.token_urlsafe(n): base64url without padding.
pub fn token_urlsafe(count: usize) -> String {
    base64_encode_with(&random_bytes(count), URL_ALPHABET, false)
}

pub fn hex(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push(DIGITS[(byte >> 4) as usize] as char);
        out.push(DIGITS[(byte & 15) as usize] as char);
    }
    out
}

const STANDARD_ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
const URL_ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";

pub fn base64_encode(bytes: &[u8]) -> String {
    base64_encode_with(bytes, STANDARD_ALPHABET, true)
}

fn base64_encode_with(bytes: &[u8], alphabet: &[u8; 64], pad: bool) -> String {
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let n = (chunk[0] as u32) << 16 | (*chunk.get(1).unwrap_or(&0) as u32) << 8 | *chunk.get(2).unwrap_or(&0) as u32;
        let symbols = chunk.len() + 1;
        for index in 0..4 {
            if index < symbols {
                out.push(alphabet[((n >> (18 - 6 * index)) & 63) as usize] as char);
            } else if pad {
                out.push('=');
            }
        }
    }
    out
}

/// Strict standard base64, as base64.b64decode(validate=True): only alphabet characters and
/// correct padding.
pub fn base64_decode(text: &str) -> Result<Vec<u8>, String> {
    let bytes = text.as_bytes();
    if bytes.len() % 4 != 0 {
        return Err("Incorrect padding".into());
    }
    let mut out = Vec::with_capacity(bytes.len() / 4 * 3);
    for (index, chunk) in bytes.chunks(4).enumerate() {
        let last = index == bytes.len() / 4 - 1;
        let mut n = 0u32;
        let mut padding = 0;
        for (position, &symbol) in chunk.iter().enumerate() {
            let value = match symbol {
                b'A'..=b'Z' => symbol - b'A',
                b'a'..=b'z' => symbol - b'a' + 26,
                b'0'..=b'9' => symbol - b'0' + 52,
                b'+' => 62,
                b'/' => 63,
                b'=' if last && position >= 2 => {
                    padding += 1;
                    0
                }
                _ => return Err("Non-base64 digit found".into()),
            };
            if padding > 0 && symbol != b'=' {
                return Err("Discontinuous padding".into());
            }
            n = n << 6 | value as u32;
        }
        out.push((n >> 16) as u8);
        if padding < 2 {
            out.push((n >> 8) as u8);
        }
        if padding < 1 {
            out.push(n as u8);
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sha256_vectors() {
        assert_eq!(sha256_hex(b""), "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855");
        assert_eq!(sha256_hex(b"abc"), "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad");
        let long = vec![b'a'; 1_000_000];
        assert_eq!(sha256_hex(&long), "cdc76e5c9914fb9281a1c7e284d73e67f1809a48a497200e046d39ccc7112cd0");
        let mut split = Sha256::new();
        split.update(b"ab");
        split.update(b"c");
        assert_eq!(hex(&split.finish()), sha256_hex(b"abc"));
    }

    #[test]
    fn hmac_and_pbkdf2_vectors() {
        // RFC 4231 case 2.
        assert_eq!(hex(&hmac_sha256(b"Jefe", b"what do ya want for nothing?")), "5bdcc146bf60754e6a042426089575c75a003f089d2739839dec58b964ec3843");
        // RFC 7914 section 11.
        assert_eq!(hex(&pbkdf2_sha256(b"passwd", b"salt", 1)), "55ac046e56e3089fec1691c22544b605f94185216dde0465e68b9d57c20dacbc");
        assert_eq!(hex(&pbkdf2_sha256(b"password", b"salt", 4096)), "c5e478d59288c841aa530db6845c4c8d962893a001ce4e11a4963873aa98134a");
    }

    #[test]
    fn base64_round_trip() {
        assert_eq!(base64_encode(b"hello!?"), "aGVsbG8hPw==");
        assert_eq!(base64_decode("aGVsbG8hPw==").unwrap(), b"hello!?");
        assert!(base64_decode("aGVsbG8hPw=").is_err());
        assert!(base64_decode("aGV$bG8h").is_err());
        assert_eq!(token_urlsafe(32).len(), 43);
    }
}
