//! Decoding WebP still images: the RIFF container (RFC 9649), lossy VP8 key frames (RFC 6386,
//! following its reference decoder), lossless VP8L, and the ALPH alpha chunk.
//!
//! Earlier releases stored every uploaded photo as WebP; the exports decode them to embed them
//! in PDF and Excel files, which cannot carry WebP.

use crate::png::Image;
use crate::webp_tables::{AC_QLOOKUP, COEFF_UPDATE_PROBS, DC_QLOOKUP, DEFAULT_COEFF_PROBS, KF_BMODE_PROBS};

type Result<T> = std::result::Result<T, String>;

const MAX_PIXELS: u64 = 40_000_000;

pub fn decode(bytes: &[u8]) -> Result<Image> {
    if bytes.len() < 20 || &bytes[..4] != b"RIFF" || &bytes[8..12] != b"WEBP" {
        return Err("not a WebP image".into());
    }
    let end = (u32::from_le_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]) as usize + 8).min(bytes.len());
    let mut at = 12;
    let mut alpha: Option<&[u8]> = None;
    while at + 8 <= end {
        let kind = &bytes[at..at + 4];
        let size = u32::from_le_bytes([bytes[at + 4], bytes[at + 5], bytes[at + 6], bytes[at + 7]]) as usize;
        let data = bytes.get(at + 8..at + 8 + size).ok_or("WebP chunk is cut short")?;
        match kind {
            b"ALPH" => alpha = Some(data),
            b"VP8 " => {
                let (width, height, mut rgba) = decode_vp8(data)?;
                if let Some(alpha) = alpha {
                    let values = decode_alpha(alpha, width, height)?;
                    for (pixel, value) in rgba.chunks_exact_mut(4).zip(values) {
                        pixel[3] = value;
                    }
                }
                return Ok(Image { width, height, rgba });
            }
            b"VP8L" => return decode_vp8l(data),
            b"ANIM" | b"ANMF" => return Err("animated WebP images are not supported".into()),
            _ => {}
        }
        at += 8 + size + (size & 1);
    }
    Err("the WebP file has no image".into())
}

// ---- The boolean entropy decoder (RFC 6386 section 7) ----

struct BoolDecoder<'a> {
    input: &'a [u8],
    position: usize,
    value: u32,
    range: u32,
    bit_count: u32,
}

impl<'a> BoolDecoder<'a> {
    fn new(input: &'a [u8]) -> BoolDecoder<'a> {
        if input.len() >= 2 {
            BoolDecoder { input, position: 2, value: (input[0] as u32) << 8 | input[1] as u32, range: 255, bit_count: 0 }
        } else {
            BoolDecoder { input, position: input.len(), value: 0, range: 255, bit_count: 0 }
        }
    }

    fn get(&mut self, probability: u8) -> bool {
        let split = 1 + (((self.range - 1) * probability as u32) >> 8);
        let big_split = split << 8;
        let bit = if self.value >= big_split {
            self.range -= split;
            self.value -= big_split;
            true
        } else {
            self.range = split;
            false
        };
        while self.range < 128 {
            self.value <<= 1;
            self.range <<= 1;
            self.bit_count += 1;
            if self.bit_count == 8 {
                self.bit_count = 0;
                if let Some(byte) = self.input.get(self.position) {
                    self.value |= *byte as u32;
                    self.position += 1;
                }
            }
        }
        bit
    }

    fn bit(&mut self) -> bool {
        self.get(128)
    }

    fn literal(&mut self, bits: u32) -> u32 {
        let mut value = 0;
        for _ in 0..bits {
            value = value << 1 | self.bit() as u32;
        }
        value
    }

    fn signed(&mut self, bits: u32) -> i32 {
        let value = self.literal(bits) as i32;
        if self.bit() {
            -value
        } else {
            value
        }
    }

    fn maybe_signed(&mut self, bits: u32) -> i32 {
        if self.bit() {
            self.signed(bits)
        } else {
            0
        }
    }

    /// Read a value with a tree of node pairs; leaves are stored negated (RFC 6386 section 8.1).
    fn tree(&mut self, tree: &[i32], probabilities: &[u8]) -> i32 {
        let mut index = 0i32;
        loop {
            index = tree[(index + self.get(probabilities[(index >> 1) as usize]) as i32) as usize];
            if index <= 0 {
                return -index;
            }
        }
    }
}

// ---- Modes ----

const DC_PRED: i32 = 0;
const V_PRED: i32 = 1;
const H_PRED: i32 = 2;
const TM_PRED: i32 = 3;
const B_PRED: i32 = 4;

const B_DC_PRED: u8 = 0;
const B_TM_PRED: u8 = 1;
const B_VE_PRED: u8 = 2;
const B_HE_PRED: u8 = 3;
const B_LD_PRED: u8 = 4;
const B_RD_PRED: u8 = 5;
const B_VR_PRED: u8 = 6;
const B_VL_PRED: u8 = 7;
const B_HD_PRED: u8 = 8;
const B_HU_PRED: u8 = 9;

const KF_Y_MODE_TREE: [i32; 8] = [-B_PRED, 2, 4, 6, -DC_PRED, -V_PRED, -H_PRED, -TM_PRED];
const KF_Y_MODE_PROBS: [u8; 4] = [145, 156, 163, 128];
const UV_MODE_TREE: [i32; 6] = [-DC_PRED, 2, -V_PRED, 4, -H_PRED, -TM_PRED];
const KF_UV_MODE_PROBS: [u8; 3] = [142, 114, 183];
const B_MODE_TREE: [i32; 18] = [
    -(B_DC_PRED as i32), 2,
    -(B_TM_PRED as i32), 4,
    -(B_VE_PRED as i32), 6,
    8, 12,
    -(B_HE_PRED as i32), 10,
    -(B_RD_PRED as i32), -(B_VR_PRED as i32),
    -(B_LD_PRED as i32), 14,
    -(B_VL_PRED as i32), 16,
    -(B_HD_PRED as i32), -(B_HU_PRED as i32),
];

#[derive(Clone, Copy, Default)]
struct MacroBlock {
    y_mode: i32,
    uv_mode: i32,
    modes: [u8; 16],
    segment: usize,
    skip: bool,
    has_coefficients: bool,
}

/// The subblock mode a neighbouring macroblock presents at its edge.
fn edge_mode(mb: &MacroBlock, subblock: usize) -> u8 {
    match mb.y_mode {
        DC_PRED => B_DC_PRED,
        V_PRED => B_VE_PRED,
        H_PRED => B_HE_PRED,
        TM_PRED => B_TM_PRED,
        _ => mb.modes[subblock],
    }
}

// ---- Tokens (RFC 6386 section 13, the reference decoder's tokens.c) ----

const ZIGZAG: [usize; 16] = [0, 1, 4, 8, 5, 2, 3, 6, 9, 12, 13, 10, 7, 11, 14, 15];
const BANDS: [usize; 17] = [0, 1, 2, 3, 6, 4, 5, 6, 6, 6, 6, 6, 6, 6, 6, 7, 0];
const LEFT_CONTEXT: [usize; 25] = [0, 0, 0, 0, 1, 1, 1, 1, 2, 2, 2, 2, 3, 3, 3, 3, 4, 4, 5, 5, 6, 6, 7, 7, 8];
const ABOVE_CONTEXT: [usize; 25] = [0, 1, 2, 3, 0, 1, 2, 3, 0, 1, 2, 3, 0, 1, 2, 3, 4, 5, 4, 5, 6, 7, 6, 7, 8];
const CAT3: [u8; 3] = [173, 148, 140];
const CAT4: [u8; 4] = [176, 155, 140, 135];
const CAT5: [u8; 5] = [180, 157, 141, 134, 130];
const CAT6: [u8; 11] = [254, 254, 243, 230, 196, 177, 153, 140, 133, 130, 129];

type Probabilities = [[[[u8; 11]; 3]; 8]; 4];

/// Decode one block's coefficients, dequantized, into `block`; returns the index it stopped at.
fn decode_block(decoder: &mut BoolDecoder, probabilities: &Probabilities, kind: usize, context: usize, factors: [i32; 2], block: &mut [i16]) -> usize {
    let first = if kind == 0 { 1 } else { 0 };
    let mut index = first;
    let mut probs = &probabilities[kind][BANDS[index]][context];
    // After a zero the end-of-block check is skipped (a block cannot end on a zero).
    let mut check_end = true;
    while index < 16 {
        if check_end && !decoder.get(probs[0]) {
            break;
        }
        if !decoder.get(probs[1]) {
            // A zero coefficient.
            if index == 15 {
                break;
            }
            index += 1;
            probs = &probabilities[kind][BANDS[index]][0];
            check_end = false;
            continue;
        }
        let value: i32 = if !decoder.get(probs[2]) {
            1
        } else if !decoder.get(probs[3]) {
            if !decoder.get(probs[4]) {
                2
            } else if !decoder.get(probs[5]) {
                3
            } else {
                4
            }
        } else if !decoder.get(probs[6]) {
            if !decoder.get(probs[7]) {
                5 + decoder.get(159) as i32
            } else {
                7 + 2 * decoder.get(165) as i32 + decoder.get(145) as i32
            }
        } else {
            let (base, table): (i32, &[u8]) = if !decoder.get(probs[8]) {
                if !decoder.get(probs[9]) {
                    (11, &CAT3)
                } else {
                    (19, &CAT4)
                }
            } else if !decoder.get(probs[10]) {
                (35, &CAT5)
            } else {
                (67, &CAT6)
            };
            let mut extra = 0;
            for probability in table {
                extra = extra << 1 | decoder.get(*probability) as i32;
            }
            base + extra
        };
        let signed = if decoder.bit() { -value } else { value };
        block[ZIGZAG[index]] = (signed * factors[(index > 0) as usize]) as i16;
        let next_context = if value == 1 { 1 } else { 2 };
        index += 1;
        if index < 16 {
            probs = &probabilities[kind][BANDS[index]][next_context];
        }
        check_end = true;
    }
    index
}

// ---- Transforms (the reference decoder's idct_add.c) ----

fn walsh(input: &[i16], output: &mut [i16; 16]) {
    let mut temp = [0i32; 16];
    for i in 0..4 {
        let a1 = input[i] as i32 + input[12 + i] as i32;
        let b1 = input[4 + i] as i32 + input[8 + i] as i32;
        let c1 = input[4 + i] as i32 - input[8 + i] as i32;
        let d1 = input[i] as i32 - input[12 + i] as i32;
        temp[i] = (a1 + b1) as i16 as i32;
        temp[4 + i] = (c1 + d1) as i16 as i32;
        temp[8 + i] = (a1 - b1) as i16 as i32;
        temp[12 + i] = (d1 - c1) as i16 as i32;
    }
    for row in 0..4 {
        let ip = &temp[row * 4..row * 4 + 4];
        let a1 = ip[0] + ip[3];
        let b1 = ip[1] + ip[2];
        let c1 = ip[1] - ip[2];
        let d1 = ip[0] - ip[3];
        output[row * 4] = ((a1 + b1 + 3) >> 3) as i16;
        output[row * 4 + 1] = ((c1 + d1 + 3) >> 3) as i16;
        output[row * 4 + 2] = ((a1 - b1 + 3) >> 3) as i16;
        output[row * 4 + 3] = ((d1 - c1 + 3) >> 3) as i16;
    }
}

const COS: i32 = 20091;
const SIN: i32 = 35468;

/// Add the inverse DCT of `coefficients` to the 4x4 block at `at`.
fn idct_add(pixels: &mut [u8], at: usize, stride: usize, coefficients: &[i16]) {
    let mut temp = [0i32; 16];
    for i in 0..4 {
        let (i0, i4, i8, i12) = (coefficients[i] as i32, coefficients[4 + i] as i32, coefficients[8 + i] as i32, coefficients[12 + i] as i32);
        let a1 = i0 + i8;
        let b1 = i0 - i8;
        let c1 = ((i4 * SIN) >> 16) - (i12 + ((i12 * COS) >> 16));
        let d1 = (i4 + ((i4 * COS) >> 16)) + ((i12 * SIN) >> 16);
        temp[i] = (a1 + d1) as i16 as i32;
        temp[12 + i] = (a1 - d1) as i16 as i32;
        temp[4 + i] = (b1 + c1) as i16 as i32;
        temp[8 + i] = (b1 - c1) as i16 as i32;
    }
    for row in 0..4 {
        let c = &temp[row * 4..row * 4 + 4];
        let a1 = c[0] + c[2];
        let b1 = c[0] - c[2];
        let c1 = ((c[1] * SIN) >> 16) - (c[3] + ((c[3] * COS) >> 16));
        let d1 = (c[1] + ((c[1] * COS) >> 16)) + ((c[3] * SIN) >> 16);
        let line = at + row * stride;
        let add = |pixel: u8, delta: i32| (pixel as i32 + delta).clamp(0, 255) as u8;
        pixels[line] = add(pixels[line], (a1 + d1 + 4) >> 3);
        pixels[line + 3] = add(pixels[line + 3], (a1 - d1 + 4) >> 3);
        pixels[line + 1] = add(pixels[line + 1], (b1 + c1 + 4) >> 3);
        pixels[line + 2] = add(pixels[line + 2], (b1 - c1 + 4) >> 3);
    }
}

// ---- Intra prediction (the reference decoder's predict.c) ----

struct Plane {
    pixels: Vec<u8>,
    stride: usize,
    origin: usize,
}

const BORDER: usize = 32;

impl Plane {
    fn new(width: usize, height: usize) -> Plane {
        let stride = width + 2 * BORDER;
        Plane { pixels: vec![0; stride * (height + 2 * BORDER)], stride, origin: BORDER * stride + BORDER }
    }

    fn at(&self, x: usize, y: usize) -> usize {
        self.origin + y * self.stride + x
    }
}

fn predict_dc(p: &mut [u8], at: usize, stride: usize, n: usize) {
    let mut total = 0u32;
    for i in 0..n {
        total += p[at + i * stride - 1] as u32 + p[at - stride + i] as u32;
    }
    let shift = match n {
        16 => 5,
        8 => 4,
        _ => 3,
    };
    let dc = ((total + (1 << (shift - 1))) >> shift) as u8;
    for i in 0..n {
        p[at + i * stride..at + i * stride + n].fill(dc);
    }
}

fn predict_v(p: &mut [u8], at: usize, stride: usize, n: usize) {
    for i in 0..n {
        p.copy_within(at - stride..at - stride + n, at + i * stride);
    }
}

fn predict_h(p: &mut [u8], at: usize, stride: usize, n: usize) {
    for i in 0..n {
        let left = p[at + i * stride - 1];
        p[at + i * stride..at + i * stride + n].fill(left);
    }
}

fn predict_tm(p: &mut [u8], at: usize, stride: usize, n: usize) {
    let corner = p[at - stride - 1] as i32;
    for i in 0..n {
        let left = p[at + i * stride - 1] as i32;
        for j in 0..n {
            let above = p[at - stride + j] as i32;
            p[at + i * stride + j] = (left + above - corner).clamp(0, 255) as u8;
        }
    }
}

fn predict_subblock(p: &mut [u8], at: usize, stride: usize, mode: u8) {
    let s = stride;
    // Above row A[-1..8) and left column L[0..4), read before writing.
    let a = |offset: isize| p[(at as isize - s as isize + offset) as usize] as i32;
    let l = |row: usize| p[at + row * s - 1] as i32;
    let above: [i32; 9] = [a(-1), a(0), a(1), a(2), a(3), a(4), a(5), a(6), a(7)];
    let left: [i32; 4] = [l(0), l(1), l(2), l(3)];
    let top_left = above[0];
    let above = &above[1..];
    let avg3 = |x: i32, y: i32, z: i32| ((x + 2 * y + z + 2) >> 2) as u8;
    let avg2 = |x: i32, y: i32| ((x + y + 1) >> 1) as u8;
    let mut out = [[0u8; 4]; 4];
    match mode {
        B_DC_PRED => {
            let total: i32 = above[..4].iter().sum::<i32>() + left.iter().sum::<i32>();
            out = [[((total + 4) >> 3) as u8; 4]; 4];
        }
        B_TM_PRED => {
            for (i, row) in out.iter_mut().enumerate() {
                for (j, cell) in row.iter_mut().enumerate() {
                    *cell = (left[i] + above[j] - top_left).clamp(0, 255) as u8;
                }
            }
        }
        B_VE_PRED => {
            let row = [avg3(top_left, above[0], above[1]), avg3(above[0], above[1], above[2]), avg3(above[1], above[2], above[3]), avg3(above[2], above[3], above[4])];
            out = [row; 4];
        }
        B_HE_PRED => {
            out[0] = [avg3(top_left, left[0], left[1]); 4];
            out[1] = [avg3(left[0], left[1], left[2]); 4];
            out[2] = [avg3(left[1], left[2], left[3]); 4];
            out[3] = [avg3(left[2], left[3], left[3]); 4];
        }
        B_LD_PRED => {
            let e = |i: usize| if i < 6 { avg3(above[i], above[i + 1], above[i + 2]) } else { avg3(above[6], above[7], above[7]) };
            for (i, row) in out.iter_mut().enumerate() {
                for (j, cell) in row.iter_mut().enumerate() {
                    *cell = e(i + j);
                }
            }
        }
        B_RD_PRED => {
            // Edge from bottom-left to top-right: L3 L2 L1 L0 TL A0 A1 A2 A3.
            let edge = [left[3], left[2], left[1], left[0], top_left, above[0], above[1], above[2], above[3]];
            for (i, row) in out.iter_mut().enumerate() {
                for (j, cell) in row.iter_mut().enumerate() {
                    let k = 3 - i + j;
                    *cell = avg3(edge[k], edge[k + 1], edge[k + 2]);
                }
            }
        }
        B_VR_PRED => {
            out[0] = [avg2(top_left, above[0]), avg2(above[0], above[1]), avg2(above[1], above[2]), avg2(above[2], above[3])];
            out[1] = [avg3(left[0], top_left, above[0]), avg3(top_left, above[0], above[1]), avg3(above[0], above[1], above[2]), avg3(above[1], above[2], above[3])];
            out[2] = [avg3(left[1], left[0], top_left), out[0][0], out[0][1], out[0][2]];
            out[3] = [avg3(left[2], left[1], left[0]), out[1][0], out[1][1], out[1][2]];
        }
        B_VL_PRED => {
            out[0] = [avg2(above[0], above[1]), avg2(above[1], above[2]), avg2(above[2], above[3]), avg2(above[3], above[4])];
            out[1] = [avg3(above[0], above[1], above[2]), avg3(above[1], above[2], above[3]), avg3(above[2], above[3], above[4]), avg3(above[3], above[4], above[5])];
            out[2] = [out[0][1], out[0][2], out[0][3], avg3(above[4], above[5], above[6])];
            out[3] = [out[1][1], out[1][2], out[1][3], avg3(above[5], above[6], above[7])];
        }
        B_HD_PRED => {
            out[0] = [avg2(left[0], top_left), avg3(left[0], top_left, above[0]), avg3(top_left, above[0], above[1]), avg3(above[0], above[1], above[2])];
            out[1] = [avg2(left[1], left[0]), avg3(left[1], left[0], top_left), out[0][0], out[0][1]];
            out[2] = [avg2(left[2], left[1]), avg3(left[2], left[1], left[0]), out[1][0], out[1][1]];
            out[3] = [avg2(left[3], left[2]), avg3(left[3], left[2], left[1]), out[2][0], out[2][1]];
        }
        _ => {
            // B_HU_PRED
            out[0] = [avg2(left[0], left[1]), avg3(left[0], left[1], left[2]), avg2(left[1], left[2]), avg3(left[1], left[2], left[3])];
            out[1] = [out[0][2], out[0][3], avg2(left[2], left[3]), avg3(left[2], left[3], left[3])];
            out[2] = [out[1][2], out[1][3], left[3] as u8, left[3] as u8];
            out[3] = [left[3] as u8; 4];
        }
    }
    for (i, row) in out.iter().enumerate() {
        p[at + i * s..at + i * s + 4].copy_from_slice(row);
    }
}

fn predict_block(p: &mut [u8], at: usize, stride: usize, n: usize, mode: i32) {
    match mode {
        DC_PRED => predict_dc(p, at, stride, n),
        V_PRED => predict_v(p, at, stride, n),
        H_PRED => predict_h(p, at, stride, n),
        _ => predict_tm(p, at, stride, n),
    }
}

/// The reference decoder's edge handling: the frame's left column reads 129, its top row 127, and
/// DC prediction at an edge averages the one side it has (by copying it to the missing side).
fn fixup_left(plane: &mut Plane, at: usize, n: usize, row: usize, mode: i32) {
    let stride = plane.stride;
    if mode == DC_PRED && row > 0 {
        for i in 0..n {
            plane.pixels[at + i * stride - 1] = plane.pixels[at - stride + i];
        }
    } else {
        for i in 0..=n {
            plane.pixels[at + i * stride - stride - 1] = 129;
        }
    }
}

fn fixup_above(plane: &mut Plane, at: usize, n: usize, column: usize, mode: i32) {
    let stride = plane.stride;
    if mode == DC_PRED && column > 0 {
        for i in 0..n {
            plane.pixels[at - stride + i] = plane.pixels[at + i * stride - 1];
        }
    } else {
        plane.pixels[at - stride - 1..at - stride + n].fill(127);
    }
    plane.pixels[at - stride + n..at - stride + n + 4].fill(127);
}

// ---- Loop filter (the reference decoder's dixie_loopfilter.c) ----

fn clamp_signed(value: i32) -> i32 {
    value.clamp(-128, 127)
}

fn clamp_pixel(value: i32) -> u8 {
    value.clamp(0, 255) as u8
}

struct Edge<'a> {
    pixels: &'a mut [u8],
    at: usize,
    step: usize,
}

impl Edge<'_> {
    fn get(&self, offset: isize) -> i32 {
        self.pixels[(self.at as isize + offset * self.step as isize) as usize] as i32
    }

    fn set(&mut self, offset: isize, value: u8) {
        let index = (self.at as isize + offset * self.step as isize) as usize;
        self.pixels[index] = value;
    }

    fn simple_threshold(&self, limit: i32) -> bool {
        (self.get(-1) - self.get(0)).abs() * 2 + ((self.get(-2) - self.get(1)).abs() >> 1) <= limit
    }

    fn normal_threshold(&self, edge: i32, interior: i32) -> bool {
        let (p3, p2, p1, p0, q0, q1, q2, q3) = (self.get(-4), self.get(-3), self.get(-2), self.get(-1), self.get(0), self.get(1), self.get(2), self.get(3));
        self.simple_threshold(2 * edge + interior)
            && (p3 - p2).abs() <= interior
            && (p2 - p1).abs() <= interior
            && (p1 - p0).abs() <= interior
            && (q3 - q2).abs() <= interior
            && (q2 - q1).abs() <= interior
            && (q1 - q0).abs() <= interior
    }

    fn high_variance(&self, threshold: i32) -> bool {
        (self.get(-2) - self.get(-1)).abs() > threshold || (self.get(1) - self.get(0)).abs() > threshold
    }

    fn common(&mut self, outer_taps: bool) {
        let (p1, p0, q0, q1) = (self.get(-2), self.get(-1), self.get(0), self.get(1));
        let mut a = 3 * (q0 - p0);
        if outer_taps {
            a += clamp_signed(p1 - q1);
        }
        a = clamp_signed(a);
        let f1 = (if a + 4 > 127 { 127 } else { a + 4 }) >> 3;
        let f2 = (if a + 3 > 127 { 127 } else { a + 3 }) >> 3;
        self.set(-1, clamp_pixel(p0 + f2));
        self.set(0, clamp_pixel(q0 - f1));
        if !outer_taps {
            let a = (f1 + 1) >> 1;
            self.set(-2, clamp_pixel(p1 + a));
            self.set(1, clamp_pixel(q1 - a));
        }
    }

    fn macroblock_edge(&mut self) {
        let (p2, p1, p0, q0, q1, q2) = (self.get(-3), self.get(-2), self.get(-1), self.get(0), self.get(1), self.get(2));
        let w = clamp_signed(clamp_signed(p1 - q1) + 3 * (q0 - p0));
        let a = (27 * w + 63) >> 7;
        self.set(-1, clamp_pixel(p0 + a));
        self.set(0, clamp_pixel(q0 - a));
        let a = (18 * w + 63) >> 7;
        self.set(-2, clamp_pixel(p1 + a));
        self.set(1, clamp_pixel(q1 - a));
        let a = (9 * w + 63) >> 7;
        self.set(-3, clamp_pixel(p2 + a));
        self.set(2, clamp_pixel(q2 - a));
    }
}

/// Filter `count` positions along an edge. `across` steps over the edge, `along` follows it.
#[allow(clippy::too_many_arguments)]
fn filter_edge(pixels: &mut [u8], start: usize, along: usize, across: usize, count: usize, macroblock: bool, edge: i32, interior: i32, hev: i32) {
    for i in 0..count {
        let mut point = Edge { pixels, at: start + i * along, step: across };
        if point.normal_threshold(edge, interior) {
            let high = point.high_variance(hev);
            if macroblock && !high {
                point.macroblock_edge();
            } else {
                point.common(high);
            }
        }
    }
}

fn filter_simple(pixels: &mut [u8], start: usize, along: usize, across: usize, limit: i32) {
    for i in 0..16 {
        let mut point = Edge { pixels, at: start + i * along, step: across };
        if point.simple_threshold(limit) {
            point.common(true);
        }
    }
}

// ---- Frames ----

struct Header {
    segmentation: bool,
    update_map: bool,
    absolute: bool,
    segment_quant: [i32; 4],
    segment_filter: [i32; 4],
    tree_probs: [u8; 3],
    simple_filter: bool,
    filter_level: i32,
    sharpness: i32,
    delta_enabled: bool,
    ref_delta: [i32; 4],
    mode_delta: [i32; 4],
}

fn decode_vp8(data: &[u8]) -> Result<(u32, u32, Vec<u8>)> {
    if data.len() < 10 {
        return Err("VP8 data is too short".into());
    }
    let raw = data[0] as u32 | (data[1] as u32) << 8 | (data[2] as u32) << 16;
    if raw & 1 != 0 {
        return Err("a WebP image must be a VP8 key frame".into());
    }
    let first_size = (raw >> 5) as usize;
    if data[3..6] != [0x9d, 0x01, 0x2a] {
        return Err("bad VP8 start code".into());
    }
    let width = (u16::from_le_bytes([data[6], data[7]]) & 0x3fff) as usize;
    let height = (u16::from_le_bytes([data[8], data[9]]) & 0x3fff) as usize;
    if width == 0 || height == 0 || (width * height) as u64 > MAX_PIXELS {
        return Err("VP8 image size is out of range".into());
    }
    let body = &data[10..];
    if body.len() < first_size {
        return Err("VP8 first partition is cut short".into());
    }
    let mut decoder = BoolDecoder::new(&body[..first_size]);
    decoder.literal(2); // colour space and clamping type
    let mut header = Header {
        segmentation: false,
        update_map: false,
        absolute: false,
        segment_quant: [0; 4],
        segment_filter: [0; 4],
        tree_probs: [255; 3],
        simple_filter: false,
        filter_level: 0,
        sharpness: 0,
        delta_enabled: false,
        ref_delta: [0; 4],
        mode_delta: [0; 4],
    };
    header.segmentation = decoder.bit();
    if header.segmentation {
        header.update_map = decoder.bit();
        let update_data = decoder.bit();
        if update_data {
            header.absolute = decoder.bit();
            for value in header.segment_quant.iter_mut() {
                *value = decoder.maybe_signed(7);
            }
            for value in header.segment_filter.iter_mut() {
                *value = decoder.maybe_signed(6);
            }
        }
        if header.update_map {
            for probability in header.tree_probs.iter_mut() {
                *probability = if decoder.bit() { decoder.literal(8) as u8 } else { 255 };
            }
        }
    }
    header.simple_filter = decoder.bit();
    header.filter_level = decoder.literal(6) as i32;
    header.sharpness = decoder.literal(3) as i32;
    header.delta_enabled = decoder.bit();
    if header.delta_enabled && decoder.bit() {
        for value in header.ref_delta.iter_mut() {
            *value = decoder.maybe_signed(6);
        }
        for value in header.mode_delta.iter_mut() {
            *value = decoder.maybe_signed(6);
        }
    }
    let partitions = 1usize << decoder.literal(2);
    let rest = &body[first_size..];
    if rest.len() < 3 * (partitions - 1) {
        return Err("VP8 partition sizes are cut short".into());
    }
    let mut sizes = Vec::with_capacity(partitions);
    let mut remaining = rest.len() - 3 * (partitions - 1);
    for index in 0..partitions {
        let size = if index + 1 < partitions {
            let at = index * 3;
            rest[at] as usize | (rest[at + 1] as usize) << 8 | (rest[at + 2] as usize) << 16
        } else {
            remaining
        };
        if size > remaining {
            return Err("VP8 partition is cut short".into());
        }
        remaining -= size;
        sizes.push(size);
    }
    let mut start = 3 * (partitions - 1);
    let mut token_decoders: Vec<BoolDecoder> = Vec::with_capacity(partitions);
    for size in &sizes {
        token_decoders.push(BoolDecoder::new(&rest[start..start + size]));
        start += size;
    }
    let q_index = decoder.literal(7) as i32;
    let y1_dc = decoder.maybe_signed(4);
    let y2_dc = decoder.maybe_signed(4);
    let y2_ac = decoder.maybe_signed(4);
    let uv_dc = decoder.maybe_signed(4);
    let uv_ac = decoder.maybe_signed(4);
    decoder.bit(); // refresh entropy probabilities: irrelevant for one frame
    let mut probabilities: Probabilities = DEFAULT_COEFF_PROBS;
    for i in 0..4 {
        for j in 0..8 {
            for k in 0..3 {
                for l in 0..11 {
                    if decoder.get(COEFF_UPDATE_PROBS[i][j][k][l]) {
                        probabilities[i][j][k][l] = decoder.literal(8) as u8;
                    }
                }
            }
        }
    }
    let skip_enabled = decoder.bit();
    let skip_probability = if skip_enabled { decoder.literal(8) as u8 } else { 0 };
    // Dequantization factors per segment: [Y1, Y2, UV] × [DC, AC].
    let dc = |q: i32| DC_QLOOKUP[q.clamp(0, 127) as usize];
    let ac = |q: i32| AC_QLOOKUP[q.clamp(0, 127) as usize];
    let mut factors = [[[0i32; 2]; 3]; 4];
    for (segment, slot) in factors.iter_mut().enumerate() {
        let q = if header.segmentation { if header.absolute { header.segment_quant[segment] } else { q_index + header.segment_quant[segment] } } else { q_index };
        slot[0] = [dc(q + y1_dc), ac(q)];
        slot[1] = [dc(q + y2_dc) * 2, (ac(q + y2_ac) * 155 / 100).max(8)];
        slot[2] = [dc(q + uv_dc).min(132), ac(q + uv_ac)];
    }

    let mb_cols = width.div_ceil(16);
    let mb_rows = height.div_ceil(16);
    let mut y = Plane::new(mb_cols * 16, mb_rows * 16);
    let mut u = Plane::new(mb_cols * 8, mb_rows * 8);
    let mut v = Plane::new(mb_cols * 8, mb_rows * 8);
    let mut above_blocks = vec![MacroBlock::default(); mb_cols];
    let mut above_context = vec![[0u8; 9]; mb_cols];
    let mut blocks = Vec::with_capacity(mb_cols * mb_rows);
    let mut coefficients = vec![0i16; 25 * 16];
    for row in 0..mb_rows {
        let mut left_block = MacroBlock::default();
        let mut left_context = [0u8; 9];
        let token_decoder = &mut token_decoders[row % partitions];
        for column in 0..mb_cols {
            // Modes, from the first partition.
            let mut mb = MacroBlock::default();
            if header.update_map {
                mb.segment = if decoder.get(header.tree_probs[0]) { 2 + decoder.get(header.tree_probs[2]) as usize } else { decoder.get(header.tree_probs[1]) as usize };
            }
            if skip_enabled {
                mb.skip = decoder.get(skip_probability);
            }
            mb.y_mode = decoder.tree(&KF_Y_MODE_TREE, &KF_Y_MODE_PROBS);
            if mb.y_mode == B_PRED {
                for index in 0..16 {
                    let above = if index < 4 { edge_mode(&above_blocks[column], index + 12) } else { mb.modes[index - 4] };
                    let left = if index & 3 == 0 { edge_mode(&left_block, index + 3) } else { mb.modes[index - 1] };
                    mb.modes[index] = decoder.tree(&B_MODE_TREE, &KF_BMODE_PROBS[above as usize][left as usize]) as u8;
                }
            }
            mb.uv_mode = decoder.tree(&UV_MODE_TREE, &KF_UV_MODE_PROBS);
            // Coefficients, from this row's token partition.
            coefficients.fill(0);
            let context = &mut above_context[column];
            if mb.skip {
                left_context[..8].fill(0);
                context[..8].fill(0);
                if mb.y_mode != B_PRED {
                    left_context[8] = 0;
                    context[8] = 0;
                }
            } else {
                let segment_factors = factors[mb.segment];
                let mut any = false;
                let (first, kind) = if mb.y_mode != B_PRED { (24, 1) } else { (0, 3) };
                let mut order: Vec<(usize, usize, [i32; 2])> = Vec::with_capacity(25);
                if first == 24 {
                    order.push((24, 1, segment_factors[1]));
                    for block in 0..16 {
                        order.push((block, 0, segment_factors[0]));
                    }
                } else {
                    for block in 0..16 {
                        order.push((block, kind, segment_factors[0]));
                    }
                }
                for block in 16..24 {
                    order.push((block, 2, segment_factors[2]));
                }
                for (block, block_kind, block_factors) in order {
                    let slot = left_context[LEFT_CONTEXT[block]] + context[ABOVE_CONTEXT[block]];
                    let stop = decode_block(token_decoder, &probabilities, block_kind, slot as usize, block_factors, &mut coefficients[block * 16..block * 16 + 16]);
                    let nonzero = stop != if block_kind == 0 { 1 } else { 0 };
                    any |= nonzero;
                    left_context[LEFT_CONTEXT[block]] = nonzero as u8;
                    context[ABOVE_CONTEXT[block]] = nonzero as u8;
                }
                mb.has_coefficients = any;
            }
            // Prediction and reconstruction.
            let y_at = y.at(column * 16, row * 16);
            let u_at = u.at(column * 8, row * 8);
            let v_at = v.at(column * 8, row * 8);
            if column == 0 {
                fixup_left(&mut y, y_at, 16, row, mb.y_mode);
                fixup_left(&mut u, u_at, 8, row, mb.uv_mode);
                fixup_left(&mut v, v_at, 8, row, mb.uv_mode);
                if row == 0 {
                    y.pixels[y_at - y.stride - 1] = 127;
                }
            }
            if row == 0 {
                fixup_above(&mut y, y_at, 16, column, mb.y_mode);
                fixup_above(&mut u, u_at, 8, column, mb.uv_mode);
                fixup_above(&mut v, v_at, 8, column, mb.uv_mode);
            }
            let stride = y.stride;
            if mb.y_mode == B_PRED {
                // The above-right pixels of the macroblock serve the right column of subblocks.
                let source = y_at + 16 - stride;
                let corner: [u8; 4] = y.pixels[source..source + 4].try_into().unwrap_or([127; 4]);
                for line in [3, 7, 11] {
                    let target = y_at + 16 + line * stride;
                    y.pixels[target..target + 4].copy_from_slice(&corner);
                }
                for index in 0..16 {
                    let at = y_at + (index >> 2) * 4 * stride + (index & 3) * 4;
                    predict_subblock(&mut y.pixels, at, stride, mb.modes[index]);
                    idct_add(&mut y.pixels, at, stride, &coefficients[index * 16..index * 16 + 16]);
                }
            } else {
                predict_block(&mut y.pixels, y_at, stride, 16, mb.y_mode);
                let mut dc = [0i16; 16];
                walsh(&coefficients[24 * 16..25 * 16], &mut dc);
                for (index, value) in dc.iter().enumerate() {
                    coefficients[index * 16] = *value;
                }
                for index in 0..16 {
                    let at = y_at + (index >> 2) * 4 * stride + (index & 3) * 4;
                    idct_add(&mut y.pixels, at, stride, &coefficients[index * 16..index * 16 + 16]);
                }
            }
            for (plane, at, first) in [(&mut u, u_at, 16usize), (&mut v, v_at, 20usize)] {
                let stride = plane.stride;
                predict_block(&mut plane.pixels, at, stride, 8, mb.uv_mode);
                for index in 0..4 {
                    let block = first + index;
                    let position = at + (index >> 1) * 4 * stride + (index & 1) * 4;
                    idct_add(&mut plane.pixels, position, stride, &coefficients[block * 16..block * 16 + 16]);
                }
            }
            left_block = mb;
            above_blocks[column] = mb;
            blocks.push(mb);
        }
        // The next row's rightmost macroblock takes its above-right from this row's last pixel.
        let edge = y.at(mb_cols * 16, row * 16 + 15);
        let value = y.pixels[edge - 1];
        y.pixels[edge..edge + 4].fill(value);
    }
    if header.filter_level != 0 {
        loop_filter(&header, &blocks, mb_cols, mb_rows, &mut y, &mut u, &mut v);
    }
    Ok((width as u32, height as u32, yuv_to_rgba(&y, &u, &v, width, height)))
}

fn loop_filter(header: &Header, blocks: &[MacroBlock], mb_cols: usize, mb_rows: usize, y: &mut Plane, u: &mut Plane, v: &mut Plane) {
    for row in 0..mb_rows {
        for column in 0..mb_cols {
            let mb = &blocks[row * mb_cols + column];
            let mut level = header.filter_level;
            if header.segmentation {
                level = if header.absolute { header.segment_filter[mb.segment] } else { level + header.segment_filter[mb.segment] };
            }
            level = level.clamp(0, 63);
            if header.delta_enabled {
                level += header.ref_delta[0];
                if mb.y_mode == B_PRED {
                    level += header.mode_delta[0];
                }
            }
            let level = level.clamp(0, 63);
            if level == 0 {
                continue;
            }
            let mut interior = level;
            if header.sharpness > 0 {
                interior >>= if header.sharpness > 4 { 2 } else { 1 };
                interior = interior.min(9 - header.sharpness);
            }
            let interior = interior.max(1);
            let hev = (level >= 15) as i32 + (level >= 40) as i32;
            let inner = mb.has_coefficients || mb.y_mode == B_PRED;
            let (ys, us) = (y.stride, u.stride);
            let y_at = y.at(column * 16, row * 16);
            let u_at = u.at(column * 8, row * 8);
            let v_at = v.at(column * 8, row * 8);
            if header.simple_filter {
                let mb_limit = (level + 2) * 2 + interior;
                let b_limit = level * 2 + interior;
                if column > 0 {
                    filter_simple(&mut y.pixels, y_at, ys, 1, mb_limit);
                }
                if inner {
                    for offset in [4, 8, 12] {
                        filter_simple(&mut y.pixels, y_at + offset, ys, 1, b_limit);
                    }
                }
                if row > 0 {
                    filter_simple(&mut y.pixels, y_at, 1, ys, mb_limit);
                }
                if inner {
                    for offset in [4, 8, 12] {
                        filter_simple(&mut y.pixels, y_at + offset * ys, 1, ys, b_limit);
                    }
                }
                continue;
            }
            if column > 0 {
                filter_edge(&mut y.pixels, y_at, ys, 1, 16, true, level + 2, interior, hev);
                filter_edge(&mut u.pixels, u_at, us, 1, 8, true, level + 2, interior, hev);
                filter_edge(&mut v.pixels, v_at, us, 1, 8, true, level + 2, interior, hev);
            }
            if inner {
                for offset in [4, 8, 12] {
                    filter_edge(&mut y.pixels, y_at + offset, ys, 1, 16, false, level, interior, hev);
                }
                filter_edge(&mut u.pixels, u_at + 4, us, 1, 8, false, level, interior, hev);
                filter_edge(&mut v.pixels, v_at + 4, us, 1, 8, false, level, interior, hev);
            }
            if row > 0 {
                filter_edge(&mut y.pixels, y_at, 1, ys, 16, true, level + 2, interior, hev);
                filter_edge(&mut u.pixels, u_at, 1, us, 8, true, level + 2, interior, hev);
                filter_edge(&mut v.pixels, v_at, 1, us, 8, true, level + 2, interior, hev);
            }
            if inner {
                for offset in [4, 8, 12] {
                    filter_edge(&mut y.pixels, y_at + offset * ys, 1, ys, 16, false, level, interior, hev);
                }
                filter_edge(&mut u.pixels, u_at + 4 * us, 1, us, 8, false, level, interior, hev);
                filter_edge(&mut v.pixels, v_at + 4 * us, 1, us, 8, false, level, interior, hev);
            }
        }
    }
}

/// BT.601 conversion as libwebp performs it (14-bit fixed point), with chroma upsampled by the
/// 9-3-3-1 weighting of neighbouring samples ("fancy upsampling").
fn yuv_to_rgba(y: &Plane, u: &Plane, v: &Plane, width: usize, height: usize) -> Vec<u8> {
    let mult = |value: i32, coefficient: i32| (value * coefficient) >> 8;
    let clip = |value: i32| -> u8 { if value & !16383 == 0 { (value >> 6) as u8 } else if value < 0 { 0 } else { 255 } };
    let chroma_width = width.div_ceil(2);
    let chroma_height = height.div_ceil(2);
    let sample = |plane: &Plane, x: isize, y: isize| -> i32 {
        let x = x.clamp(0, chroma_width as isize - 1) as usize;
        let y = y.clamp(0, chroma_height as isize - 1) as usize;
        plane.pixels[plane.at(x, y)] as i32
    };
    let mut out = vec![0u8; width * height * 4];
    for row in 0..height {
        // The two chroma rows nearest this luma row, nearer first.
        let near = (row / 2) as isize;
        let far = if row % 2 == 0 { near - 1 } else { near + 1 };
        for column in 0..width {
            let cx = (column / 2) as isize;
            let cx_far = if column % 2 == 0 { cx - 1 } else { cx + 1 };
            let blend = |plane: &Plane| -> i32 {
                (9 * sample(plane, cx, near) + 3 * sample(plane, cx_far, near) + 3 * sample(plane, cx, far) + sample(plane, cx_far, far) + 8) >> 4
            };
            let (luma, cu, cv) = (y.pixels[y.at(column, row)] as i32, blend(u), blend(v));
            let index = (row * width + column) * 4;
            out[index] = clip(mult(luma, 19077) + mult(cv, 26149) - 14234);
            out[index + 1] = clip(mult(luma, 19077) - mult(cu, 6419) - mult(cv, 13320) + 8708);
            out[index + 2] = clip(mult(luma, 19077) + mult(cu, 33050) - 17685);
            out[index + 3] = 255;
        }
    }
    out
}

// ---- Alpha (the ALPH chunk) ----

fn decode_alpha(data: &[u8], width: u32, height: u32) -> Result<Vec<u8>> {
    let header = *data.first().ok_or("empty alpha chunk")?;
    let (method, filter) = (header & 3, (header >> 2) & 3);
    let (width, height) = (width as usize, height as usize);
    let mut alpha = match method {
        0 => data.get(1..1 + width * height).ok_or("alpha data is cut short")?.to_vec(),
        1 => {
            let mut reader = BitReader::new(&data[1..]);
            let argb = decode_image_stream(&mut reader, width, height, true)?;
            argb.iter().map(|pixel| (pixel >> 8) as u8).collect()
        }
        _ => return Err("unknown alpha compression".into()),
    };
    if filter != 0 {
        for row in 0..height {
            for column in 0..width {
                let index = row * width + column;
                let left = if column > 0 { Some(alpha[index - 1]) } else { None };
                let top = if row > 0 { Some(alpha[index - width]) } else { None };
                let predictor = match (filter, left, top) {
                    (_, None, None) => 0,
                    (1, Some(left), _) => left,
                    (1, None, Some(top)) => top,
                    (2, _, Some(top)) => top,
                    (2, Some(left), None) => left,
                    (3, Some(left), Some(top)) => (left as i32 + top as i32 - alpha[index - width - 1] as i32).clamp(0, 255) as u8,
                    (3, Some(left), None) => left,
                    (3, None, Some(top)) => top,
                    _ => 0,
                };
                alpha[index] = alpha[index].wrapping_add(predictor);
            }
        }
    }
    Ok(alpha)
}

// ---- Lossless (VP8L) ----

struct BitReader<'a> {
    data: &'a [u8],
    position: usize,
    bits: u64,
    count: u32,
}

impl<'a> BitReader<'a> {
    fn new(data: &'a [u8]) -> BitReader<'a> {
        BitReader { data, position: 0, bits: 0, count: 0 }
    }

    fn read(&mut self, width: u32) -> Result<u32> {
        if width == 0 {
            return Ok(0);
        }
        while self.count < width {
            let byte = *self.data.get(self.position).ok_or("lossless data is cut short")?;
            self.position += 1;
            self.bits |= (byte as u64) << self.count;
            self.count += 8;
        }
        let value = (self.bits & ((1u64 << width) - 1)) as u32;
        self.bits >>= width;
        self.count -= width;
        Ok(value)
    }
}

/// A canonical prefix code; a code with one symbol is read without consuming bits.
struct Prefix {
    counts: [u16; 16],
    symbols: Vec<u16>,
    single: Option<u16>,
}

impl Prefix {
    fn new(lengths: &[u8]) -> Result<Prefix> {
        let used: Vec<usize> = (0..lengths.len()).filter(|&symbol| lengths[symbol] != 0).collect();
        if used.len() <= 1 {
            return Ok(Prefix { counts: [0; 16], symbols: Vec::new(), single: Some(used.first().copied().unwrap_or(0) as u16) });
        }
        let mut counts = [0u16; 16];
        for &length in lengths {
            if length > 15 {
                return Err("prefix code length is too long".into());
            }
            counts[length as usize] += 1;
        }
        counts[0] = 0;
        let mut offsets = [0u16; 16];
        for length in 1..16 {
            offsets[length] = offsets[length - 1] + counts[length - 1];
        }
        let mut symbols = vec![0u16; used.len()];
        for (symbol, &length) in lengths.iter().enumerate() {
            if length != 0 {
                symbols[offsets[length as usize] as usize] = symbol as u16;
                offsets[length as usize] += 1;
            }
        }
        Ok(Prefix { counts, symbols, single: None })
    }

    fn read(&self, reader: &mut BitReader) -> Result<u16> {
        if let Some(symbol) = self.single {
            return Ok(symbol);
        }
        let (mut code, mut first, mut index) = (0i32, 0i32, 0i32);
        for length in 1..16 {
            code |= reader.read(1)? as i32;
            let count = self.counts[length] as i32;
            if code - count < first {
                return self.symbols.get((index + code - first) as usize).copied().ok_or_else(|| "bad prefix code".to_string());
            }
            index += count;
            first = (first + count) << 1;
            code <<= 1;
        }
        Err("bad prefix code".into())
    }
}

const CODE_LENGTH_ORDER: [usize; 19] = [17, 18, 0, 1, 2, 3, 4, 5, 16, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15];

fn read_prefix(reader: &mut BitReader, alphabet: usize) -> Result<Prefix> {
    let mut lengths = vec![0u8; alphabet];
    if reader.read(1)? == 1 {
        let count = reader.read(1)? + 1;
        let first_bits = if reader.read(1)? == 1 { 8 } else { 1 };
        let first = reader.read(first_bits)? as usize;
        if first >= alphabet {
            return Err("prefix symbol out of range".into());
        }
        lengths[first] = 1;
        if count == 2 {
            let second = reader.read(8)? as usize;
            if second >= alphabet {
                return Err("prefix symbol out of range".into());
            }
            lengths[second] = 1;
        }
        return Prefix::new(&lengths);
    }
    let mut code_lengths = [0u8; 19];
    let count = 4 + reader.read(4)? as usize;
    for &slot in CODE_LENGTH_ORDER.iter().take(count) {
        code_lengths[slot] = reader.read(3)? as u8;
    }
    let lengths_code = Prefix::new(&code_lengths)?;
    let mut max_symbol = if reader.read(1)? == 1 {
        let bits = 2 + 2 * reader.read(3)?;
        2 + reader.read(bits)? as usize
    } else {
        alphabet
    };
    if max_symbol > alphabet {
        return Err("too many prefix code lengths".into());
    }
    let mut symbol = 0;
    let mut previous = 8u8;
    while symbol < alphabet {
        if max_symbol == 0 {
            break;
        }
        max_symbol -= 1;
        let code = lengths_code.read(reader)?;
        match code {
            0..=15 => {
                lengths[symbol] = code as u8;
                symbol += 1;
                if code != 0 {
                    previous = code as u8;
                }
            }
            _ => {
                let (repeat, value) = match code {
                    16 => (3 + reader.read(2)? as usize, previous),
                    17 => (3 + reader.read(3)? as usize, 0),
                    _ => (11 + reader.read(7)? as usize, 0),
                };
                if symbol + repeat > alphabet {
                    return Err("prefix code lengths overrun".into());
                }
                lengths[symbol..symbol + repeat].fill(value);
                symbol += repeat;
            }
        }
    }
    Prefix::new(&lengths)
}

const DISTANCE_MAP: [(i32, i32); 120] = [
    (0, 1), (1, 0), (1, 1), (-1, 1), (0, 2), (2, 0), (1, 2), (-1, 2), (2, 1), (-2, 1), (2, 2), (-2, 2), (0, 3), (3, 0), (1, 3), (-1, 3), (3, 1), (-3, 1),
    (2, 3), (-2, 3), (3, 2), (-3, 2), (0, 4), (4, 0), (1, 4), (-1, 4), (4, 1), (-4, 1), (3, 3), (-3, 3), (2, 4), (-2, 4), (4, 2), (-4, 2), (0, 5), (3, 4),
    (-3, 4), (4, 3), (-4, 3), (5, 0), (1, 5), (-1, 5), (5, 1), (-5, 1), (2, 5), (-2, 5), (5, 2), (-5, 2), (4, 4), (-4, 4), (3, 5), (-3, 5), (5, 3), (-5, 3),
    (0, 6), (6, 0), (1, 6), (-1, 6), (6, 1), (-6, 1), (2, 6), (-2, 6), (6, 2), (-6, 2), (4, 5), (-4, 5), (5, 4), (-5, 4), (3, 6), (-3, 6), (6, 3), (-6, 3),
    (0, 7), (7, 0), (1, 7), (-1, 7), (5, 5), (-5, 5), (7, 1), (-7, 1), (4, 6), (-4, 6), (6, 4), (-6, 4), (2, 7), (-2, 7), (7, 2), (-7, 2), (3, 7), (-3, 7),
    (7, 3), (-7, 3), (5, 6), (-5, 6), (6, 5), (-6, 5), (8, 0), (4, 7), (-4, 7), (7, 4), (-7, 4), (8, 1), (8, 2), (6, 6), (-6, 6), (8, 3), (5, 7), (-5, 7),
    (7, 5), (-7, 5), (8, 4), (6, 7), (-6, 7), (7, 6), (-7, 6), (8, 5), (7, 7), (-7, 7), (8, 6), (8, 7),
];

fn prefix_value(reader: &mut BitReader, code: u32) -> Result<usize> {
    if code < 4 {
        return Ok(code as usize + 1);
    }
    let extra = (code - 2) >> 1;
    let offset = (2 + (code & 1)) << extra;
    Ok((offset + reader.read(extra)? + 1) as usize)
}

/// The entropy-coded pixels of an image (or sub-image) of the given size.
fn decode_entropy_image(reader: &mut BitReader, width: usize, height: usize, top_level: bool) -> Result<Vec<u32>> {
    let cache_bits = if reader.read(1)? == 1 {
        let bits = reader.read(4)?;
        if !(1..=11).contains(&bits) {
            return Err("bad colour cache size".into());
        }
        bits
    } else {
        0
    };
    let cache_size = if cache_bits > 0 { 1usize << cache_bits } else { 0 };
    let mut meta_bits = 0;
    let mut meta: Vec<u32> = Vec::new();
    let mut meta_width = 0;
    if top_level && reader.read(1)? == 1 {
        meta_bits = reader.read(3)? + 2;
        meta_width = width.div_ceil(1 << meta_bits);
        let meta_height = height.div_ceil(1 << meta_bits);
        meta = decode_image_stream(reader, meta_width, meta_height, false)?;
    }
    let groups = meta.iter().map(|pixel| (pixel >> 8) & 0xffff).max().map(|max| max as usize + 1).unwrap_or(1);
    let mut codes = Vec::with_capacity(groups);
    for _ in 0..groups {
        codes.push([
            read_prefix(reader, 256 + 24 + cache_size)?,
            read_prefix(reader, 256)?,
            read_prefix(reader, 256)?,
            read_prefix(reader, 256)?,
            read_prefix(reader, 40)?,
        ]);
    }
    let total = width * height;
    let mut pixels = vec![0u32; total];
    let mut cache = vec![0u32; cache_size];
    let mut cached = 0usize;
    let mut position = 0usize;
    let cache_key = |pixel: u32| (0x1e35_a7bd_u32.wrapping_mul(pixel) >> (32 - cache_bits)) as usize;
    while position < total {
        let group = if meta.is_empty() {
            0
        } else {
            let (x, y) = (position % width, position / width);
            ((meta[(y >> meta_bits) * meta_width + (x >> meta_bits)] >> 8) & 0xffff) as usize
        };
        let code = &codes[group];
        let green = code[0].read(reader)? as u32;
        if green < 256 {
            let red = code[1].read(reader)? as u32;
            let blue = code[2].read(reader)? as u32;
            let alpha = code[3].read(reader)? as u32;
            pixels[position] = alpha << 24 | red << 16 | green << 8 | blue;
            position += 1;
        } else if green < 256 + 24 {
            let length = prefix_value(reader, green - 256)?;
            let distance_symbol = code[4].read(reader)? as u32;
            let distance_code = prefix_value(reader, distance_symbol)?;
            let distance = if distance_code > 120 {
                distance_code - 120
            } else {
                let (dx, dy) = DISTANCE_MAP[distance_code - 1];
                (dx + dy * width as i32).max(1) as usize
            };
            if distance > position || position + length > total {
                return Err("lossless back-reference out of range".into());
            }
            for _ in 0..length {
                pixels[position] = pixels[position - distance];
                position += 1;
            }
        } else {
            let index = (green - 256 - 24) as usize;
            if index >= cache_size {
                return Err("colour cache index out of range".into());
            }
            // Bring the cache up to date before reading from it.
            while cached < position {
                cache[cache_key(pixels[cached])] = pixels[cached];
                cached += 1;
            }
            pixels[position] = cache[index];
            position += 1;
        }
        if cache_size > 0 {
            while cached < position {
                cache[cache_key(pixels[cached])] = pixels[cached];
                cached += 1;
            }
        }
    }
    Ok(pixels)
}

enum Transform {
    Predictor { width: usize, bits: u32, data: Vec<u32> },
    Colour { width: usize, bits: u32, data: Vec<u32> },
    SubtractGreen,
    Indexing { width: usize, bits: u32, table: Vec<u32> },
}

fn add_pixels(a: u32, b: u32) -> u32 {
    let alpha = ((a >> 24) + (b >> 24)) & 0xff;
    let red = (((a >> 16) & 0xff) + ((b >> 16) & 0xff)) & 0xff;
    let green = (((a >> 8) & 0xff) + ((b >> 8) & 0xff)) & 0xff;
    let blue = ((a & 0xff) + (b & 0xff)) & 0xff;
    alpha << 24 | red << 16 | green << 8 | blue
}

fn average(a: u32, b: u32) -> u32 {
    (((a ^ b) & 0xfefe_fefe) >> 1) + (a & b)
}

fn channels(pixel: u32) -> [i32; 4] {
    [(pixel >> 24) as i32, ((pixel >> 16) & 0xff) as i32, ((pixel >> 8) & 0xff) as i32, (pixel & 0xff) as i32]
}

fn join(values: [i32; 4]) -> u32 {
    (values[0].clamp(0, 255) as u32) << 24 | (values[1].clamp(0, 255) as u32) << 16 | (values[2].clamp(0, 255) as u32) << 8 | values[3].clamp(0, 255) as u32
}

fn select(left: u32, top: u32, top_left: u32) -> u32 {
    let (l, t, tl) = (channels(left), channels(top), channels(top_left));
    let estimate: Vec<i32> = (0..4).map(|i| l[i] + t[i] - tl[i]).collect();
    let to_left: i32 = (0..4).map(|i| (estimate[i] - l[i]).abs()).sum();
    let to_top: i32 = (0..4).map(|i| (estimate[i] - t[i]).abs()).sum();
    if to_left < to_top {
        left
    } else {
        top
    }
}

fn clamp_add_subtract_full(a: u32, b: u32, c: u32) -> u32 {
    let (a, b, c) = (channels(a), channels(b), channels(c));
    join([a[0] + b[0] - c[0], a[1] + b[1] - c[1], a[2] + b[2] - c[2], a[3] + b[3] - c[3]])
}

fn clamp_add_subtract_half(a: u32, b: u32) -> u32 {
    let (a, b) = (channels(a), channels(b));
    join([a[0] + (a[0] - b[0]) / 2, a[1] + (a[1] - b[1]) / 2, a[2] + (a[2] - b[2]) / 2, a[3] + (a[3] - b[3]) / 2])
}

fn predict_pixel(mode: u32, left: u32, top: u32, top_right: u32, top_left: u32) -> u32 {
    match mode {
        1 => left,
        2 => top,
        3 => top_right,
        4 => top_left,
        5 => average(average(left, top_right), top),
        6 => average(left, top_left),
        7 => average(left, top),
        8 => average(top_left, top),
        9 => average(top, top_right),
        10 => average(average(left, top_left), average(top, top_right)),
        11 => select(left, top, top_left),
        12 => clamp_add_subtract_full(left, top, top_left),
        13 => clamp_add_subtract_half(average(left, top), top_left),
        _ => 0xff00_0000,
    }
}

fn colour_delta(transform: u32, colour: u32) -> i32 {
    ((transform as u8 as i8 as i32) * (colour as u8 as i8 as i32)) >> 5
}

fn decode_image_stream(reader: &mut BitReader, width: usize, height: usize, top_level: bool) -> Result<Vec<u32>> {
    let mut transforms = Vec::new();
    let mut coded_width = width;
    if top_level {
        let mut seen = [false; 4];
        while reader.read(1)? == 1 {
            let kind = reader.read(2)? as usize;
            if seen[kind] {
                return Err("a lossless transform is repeated".into());
            }
            seen[kind] = true;
            match kind {
                0 | 1 => {
                    let bits = reader.read(3)? + 2;
                    let data = decode_image_stream(reader, coded_width.div_ceil(1 << bits), height.div_ceil(1 << bits), false)?;
                    transforms.push(if kind == 0 { Transform::Predictor { width: coded_width, bits, data } } else { Transform::Colour { width: coded_width, bits, data } });
                }
                2 => transforms.push(Transform::SubtractGreen),
                _ => {
                    let colours = reader.read(8)? as usize + 1;
                    let mut table = decode_image_stream(reader, colours, 1, false)?;
                    for index in 1..table.len() {
                        table[index] = add_pixels(table[index - 1], table[index]);
                    }
                    let bits = match colours {
                        1..=2 => 3,
                        3..=4 => 2,
                        5..=16 => 1,
                        _ => 0,
                    };
                    transforms.push(Transform::Indexing { width: coded_width, bits, table });
                    coded_width = coded_width.div_ceil(1 << bits);
                }
            }
        }
    }
    let mut pixels = decode_entropy_image(reader, coded_width, height, top_level)?;
    for transform in transforms.iter().rev() {
        match transform {
            Transform::Predictor { width, bits, data } => {
                let (width, tiles) = (*width, width.div_ceil(1 << bits));
                for y in 0..height {
                    for x in 0..width {
                        let index = y * width + x;
                        let prediction = if y == 0 && x == 0 {
                            0xff00_0000
                        } else if y == 0 {
                            pixels[index - 1]
                        } else if x == 0 {
                            pixels[index - width]
                        } else {
                            let mode = (data[(y >> bits) * tiles + (x >> bits)] >> 8) & 0xf;
                            predict_pixel(mode, pixels[index - 1], pixels[index - width], pixels[index - width + 1], pixels[index - width - 1])
                        };
                        pixels[index] = add_pixels(pixels[index], prediction);
                    }
                }
            }
            Transform::Colour { width, bits, data } => {
                let (width, tiles) = (*width, width.div_ceil(1 << bits));
                for y in 0..height {
                    for x in 0..width {
                        let element = data[(y >> bits) * tiles + (x >> bits)];
                        let pixel = pixels[y * width + x];
                        let green = (pixel >> 8) & 0xff;
                        let red = (((pixel >> 16) & 0xff) as i32 + colour_delta(element, green)) as u32 & 0xff;
                        let blue = ((pixel & 0xff) as i32 + colour_delta(element >> 8, green) + colour_delta(element >> 16, red)) as u32 & 0xff;
                        pixels[y * width + x] = (pixel & 0xff00_ff00) | red << 16 | blue;
                    }
                }
            }
            Transform::SubtractGreen => {
                for pixel in pixels.iter_mut() {
                    let green = (*pixel >> 8) & 0xff;
                    let red = (((*pixel >> 16) & 0xff) + green) & 0xff;
                    let blue = ((*pixel & 0xff) + green) & 0xff;
                    *pixel = (*pixel & 0xff00_ff00) | red << 16 | blue;
                }
            }
            Transform::Indexing { width, bits, table } => {
                let (width, packed_width) = (*width, width.div_ceil(1 << bits));
                let per = 8 >> bits;
                let mask = (1u32 << per) - 1;
                let mut expanded = vec![0u32; width * height];
                for y in 0..height {
                    for x in 0..width {
                        let green = (pixels[y * packed_width + (x >> bits)] >> 8) & 0xff;
                        let index = if *bits == 0 { green } else { (green >> ((x & ((1 << bits) - 1)) as u32 * per)) & mask };
                        expanded[y * width + x] = table.get(index as usize).copied().unwrap_or(0);
                    }
                }
                pixels = expanded;
            }
        }
    }
    Ok(pixels)
}

fn decode_vp8l(data: &[u8]) -> Result<Image> {
    if data.len() < 5 || data[0] != 0x2f {
        return Err("bad VP8L signature".into());
    }
    let bits = u32::from_le_bytes([data[1], data[2], data[3], data[4]]);
    let width = (bits & 0x3fff) as usize + 1;
    let height = ((bits >> 14) & 0x3fff) as usize + 1;
    if (width * height) as u64 > MAX_PIXELS || (bits >> 29) != 0 {
        return Err("VP8L image is out of range".into());
    }
    let mut reader = BitReader::new(&data[5..]);
    let argb = decode_image_stream(&mut reader, width, height, true)?;
    let mut rgba = Vec::with_capacity(width * height * 4);
    for pixel in argb {
        rgba.extend_from_slice(&[(pixel >> 16) as u8, (pixel >> 8) as u8, pixel as u8, (pixel >> 24) as u8]);
    }
    Ok(Image { width: width as u32, height: height as u32, rgba })
}


#[cfg(test)]
mod tests {
    use super::*;

    /// The pixels libwebp (through Pillow) decodes these files to, as SHA-256 of their RGBA.
    #[test]
    fn matches_libwebp() {
        for (name, expected) in [
            ("alpha_37x23.webp", "534ce54d3da2cba93e6e8b1a202a4f1fd48c035700311d5ca868ca1fa5f63afd"),
            ("signature_lossless.webp", "4d6a2a076f39447eff531c46464c7dc4531202f216acc0a59ea8fee34d3cb804"),
            ("lossy_37x23_q30.webp", "882450e19fac4a2ae114e7ea0599e61dcbec47c71b9aefab8f6ed85c0585fc15"),
        ] {
            let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/data").join(name);
            let image = decode(&std::fs::read(path).unwrap()).unwrap();
            assert_eq!(crate::crypto::sha256_hex(&image.rgba), expected, "{name}");
        }
    }

    /// Decodes every .webp in $WEBP_SAMPLES to a .rgba file beside it, for comparison with
    /// another decoder: `WEBP_SAMPLES=dir cargo test webp -- --ignored`.
    #[test]
    #[ignore]
    fn decode_samples() {
        let Ok(dir) = std::env::var("WEBP_SAMPLES") else { return };
        for entry in std::fs::read_dir(&dir).unwrap() {
            let path = entry.unwrap().path();
            if path.extension().is_some_and(|ext| ext == "webp") {
                match decode(&std::fs::read(&path).unwrap()) {
                    Ok(image) => std::fs::write(path.with_extension("rgba"), &image.rgba).unwrap(),
                    Err(error) => eprintln!("{}: {error}", path.display()),
                }
            }
        }
    }
}
