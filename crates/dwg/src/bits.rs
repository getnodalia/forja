// SPDX-License-Identifier: MIT OR Apache-2.0
//! The bit-level reader DWG objects are written with: most-significant bit first, with the
//! variable-length codes the format builds on (BS, BL, BD, DD, handles, modular numbers).

use crate::error::{Error, Result};

/// A cursor over a byte slice, counted in bits. Reads past the end fail with
/// [`Error::Truncated`]; a reader built with [`Bits::limited`] stops earlier.
#[derive(Clone)]
pub struct Bits<'a> {
    data: &'a [u8],
    pos: usize,
    end: usize,
}

impl<'a> Bits<'a> {
    /// A reader over all of `data`, starting at the first bit.
    pub fn new(data: &'a [u8]) -> Self {
        Bits { data, pos: 0, end: data.len().saturating_mul(8) }
    }

    /// Current position in bits.
    pub fn pos(&self) -> usize {
        self.pos
    }

    /// Bits left before the end.
    pub fn remaining(&self) -> usize {
        self.end.saturating_sub(self.pos)
    }

    /// Move to an absolute bit position (at most the end).
    pub fn seek(&mut self, bit: usize) -> Result<()> {
        if bit > self.end {
            return Err(Error::Truncated("a seek"));
        }
        self.pos = bit;
        Ok(())
    }

    /// Skip to the next byte boundary.
    pub fn align(&mut self) {
        self.pos = self.pos.div_ceil(8).saturating_mul(8).min(self.end);
    }

    /// One bit (B).
    pub fn bit(&mut self) -> Result<bool> {
        if self.pos >= self.end {
            return Err(Error::Truncated("a bit"));
        }
        let byte = self.data.get(self.pos / 8).copied().ok_or(Error::Truncated("a bit"))?;
        let v = (byte >> (7 - (self.pos % 8))) & 1 == 1;
        self.pos += 1;
        Ok(v)
    }

    /// `n` bits (at most 32) as an unsigned number, first bit most significant.
    pub fn bits(&mut self, n: u32) -> Result<u32> {
        if n > 32 {
            return Err(Error::Invalid("more than 32 bits asked at once"));
        }
        if self.remaining() < n as usize {
            return Err(Error::Truncated("some bits"));
        }
        let mut v = 0u32;
        for _ in 0..n {
            v = (v << 1) | u32::from(self.bit()?);
        }
        Ok(v)
    }

    /// A raw byte (RC), not necessarily aligned.
    pub fn rc(&mut self) -> Result<u8> {
        Ok(self.bits(8)? as u8)
    }

    /// `n` raw bytes.
    pub fn bytes(&mut self, n: usize) -> Result<Vec<u8>> {
        if self.remaining() / 8 < n {
            return Err(Error::Truncated("some bytes"));
        }
        (0..n).map(|_| self.rc()).collect()
    }

    /// A raw little-endian short (RS).
    pub fn rs(&mut self) -> Result<u16> {
        let lo = u16::from(self.rc()?);
        let hi = u16::from(self.rc()?);
        Ok(lo | (hi << 8))
    }

    /// A raw little-endian long (RL).
    pub fn rl(&mut self) -> Result<u32> {
        let lo = u32::from(self.rs()?);
        let hi = u32::from(self.rs()?);
        Ok(lo | (hi << 16))
    }

    /// A raw little-endian double (RD).
    pub fn rd(&mut self) -> Result<f64> {
        let mut b = [0u8; 8];
        for x in &mut b {
            *x = self.rc()?;
        }
        Ok(f64::from_le_bytes(b))
    }

    /// Two bits (BB).
    pub fn bb(&mut self) -> Result<u8> {
        Ok(self.bits(2)? as u8)
    }

    /// A bit-short (BS): 16 bits, 8 bits, 0 or 256 by a two-bit tag.
    pub fn bs(&mut self) -> Result<u16> {
        match self.bb()? {
            0 => self.rs(),
            1 => Ok(u16::from(self.rc()?)),
            2 => Ok(0),
            _ => Ok(256),
        }
    }

    /// A bit-long (BL): 32 bits, 8 bits or 0 by a two-bit tag.
    pub fn bl(&mut self) -> Result<u32> {
        match self.bb()? {
            0 => self.rl(),
            1 => Ok(u32::from(self.rc()?)),
            2 => Ok(0),
            _ => Err(Error::Invalid("bit-long tag 3")),
        }
    }

    /// A bit-double (BD): a full double, 1.0 or 0.0 by a two-bit tag.
    pub fn bd(&mut self) -> Result<f64> {
        match self.bb()? {
            0 => self.rd(),
            1 => Ok(1.0),
            2 => Ok(0.0),
            _ => Err(Error::Invalid("bit-double tag 3")),
        }
    }

    /// Three bit-doubles (3BD).
    pub fn bd3(&mut self) -> Result<[f64; 3]> {
        Ok([self.bd()?, self.bd()?, self.bd()?])
    }

    /// A bit-double with default (DD): the default, a patched default or a full double.
    pub fn dd(&mut self, default: f64) -> Result<f64> {
        let mut b = default.to_le_bytes();
        match self.bb()? {
            0 => return Ok(default),
            1 => {
                for i in 0..4 {
                    *b.get_mut(i).ok_or(Error::Invalid("double byte"))? = self.rc()?;
                }
            }
            2 => {
                for i in [4usize, 5, 0, 1, 2, 3] {
                    *b.get_mut(i).ok_or(Error::Invalid("double byte"))? = self.rc()?;
                }
            }
            _ => return self.rd(),
        }
        Ok(f64::from_le_bytes(b))
    }

    /// A bit-thickness (BT): 0.0 when the flag bit is set, otherwise a bit-double.
    pub fn bt(&mut self) -> Result<f64> {
        if self.bit()? { Ok(0.0) } else { self.bd() }
    }

    /// A bit-extrusion (BE): the default normal (0, 0, 1) when the flag bit is set.
    pub fn be(&mut self) -> Result<[f64; 3]> {
        if self.bit()? { Ok([0.0, 0.0, 1.0]) } else { self.bd3() }
    }

    /// A handle (H): its code and its value (up to 8 bytes, big endian).
    pub fn handle(&mut self) -> Result<(u8, u64)> {
        let code = self.bits(4)? as u8;
        let n = self.bits(4)?;
        if n > 8 {
            return Err(Error::Invalid("handle longer than 8 bytes"));
        }
        let mut v = 0u64;
        for _ in 0..n {
            v = (v << 8) | u64::from(self.rc()?);
        }
        Ok((code, v))
    }

    /// An unsigned modular char (MC): 7 bits per byte, low bytes first, high bit continues.
    pub fn mc_unsigned(&mut self) -> Result<u64> {
        let mut v = 0u64;
        for i in 0..10u32 {
            let b = self.rc()?;
            v |= u64::from(b & 0x7F).checked_shl(7 * i).ok_or(Error::Invalid("modular char too long"))?;
            if b & 0x80 == 0 {
                return Ok(v);
            }
        }
        Err(Error::Invalid("modular char too long"))
    }

    /// A signed modular char (MC): like the unsigned one, but bit 6 of the last byte is the sign.
    pub fn mc_signed(&mut self) -> Result<i64> {
        let mut v = 0i64;
        for i in 0..9u32 {
            let b = self.rc()?;
            if b & 0x80 == 0 {
                v |= i64::from(b & 0x3F) << (7 * i);
                return Ok(if b & 0x40 != 0 { -v } else { v });
            }
            v |= i64::from(b & 0x7F) << (7 * i);
        }
        Err(Error::Invalid("modular char too long"))
    }

    /// A modular short (MS): 15 bits per 16-bit word, the top bit of a word continues.
    pub fn ms(&mut self) -> Result<u64> {
        let mut v = 0u64;
        for i in 0..4u32 {
            let w = self.rs()?;
            v |= u64::from(w & 0x7FFF) << (15 * i);
            if w & 0x8000 == 0 {
                return Ok(v);
            }
        }
        Err(Error::Invalid("modular short too long"))
    }

    /// A text value (TV) in the 2000 format: a bit-short length then that many code-page bytes.
    /// The result is the raw bytes without the trailing zero.
    pub fn text(&mut self) -> Result<Vec<u8>> {
        let n = usize::from(self.bs()?);
        let mut b = self.bytes(n)?;
        while b.last() == Some(&0) {
            b.pop();
        }
        Ok(b)
    }

    /// A reader over the same data that stops `bits` after the current position.
    pub fn limited(&self, bits: usize) -> Bits<'a> {
        Bits { data: self.data, pos: self.pos, end: self.pos.saturating_add(bits).min(self.end) }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_bits_most_significant_first() {
        let mut b = Bits::new(&[0b1010_0000]);
        assert!(b.bit().unwrap());
        assert!(!b.bit().unwrap());
        assert_eq!(b.bits(2).unwrap(), 0b10);
        assert_eq!(b.pos(), 4);
    }

    /// Bytes from a string of 0 and 1 (spaces ignored), padded with zero bits.
    fn pack(bits: &str) -> Vec<u8> {
        let b: Vec<u8> = bits.bytes().filter(|c| *c == b'0' || *c == b'1').map(|c| c - b'0').collect();
        b.chunks(8).map(|c| c.iter().enumerate().fold(0u8, |a, (i, x)| a | (x << (7 - i)))).collect()
    }

    #[test]
    fn short_codes() {
        // BS tag 10 -> 0, tag 11 -> 256, tag 01 -> a byte, tag 00 -> a short.
        assert_eq!(Bits::new(&[0b1000_0000]).bs().unwrap(), 0);
        assert_eq!(Bits::new(&[0b1100_0000]).bs().unwrap(), 256);
        assert_eq!(Bits::new(&[0b0100_0000, 0b1100_0000]).bs().unwrap(), 0b0000_0011);
        // tag 00 then the short 0x1234 little endian: bytes 0x34, 0x12 unaligned by two bits.
        assert_eq!(Bits::new(&pack("00 00110100 00010010")).bs().unwrap(), 0x1234);
    }

    #[test]
    fn long_and_double_codes() {
        assert_eq!(Bits::new(&[0b1000_0000]).bl().unwrap(), 0);
        assert!(Bits::new(&[0b1100_0000]).bl().is_err());
        assert_eq!(Bits::new(&[0b0100_0000]).bd().unwrap(), 1.0);
        assert_eq!(Bits::new(&[0b1000_0000]).bd().unwrap(), 0.0);
        assert!(Bits::new(&[0b1100_0000]).bd().is_err());
    }

    #[test]
    fn default_double_patches() {
        // tag 00: the default itself.
        assert_eq!(Bits::new(&[0]).dd(2.5).unwrap(), 2.5);
        // tag 11: a full double after the tag.
        let mut text = String::from("11");
        for byte in 7.25f64.to_le_bytes() {
            text.push_str(&format!("{byte:08b}"));
        }
        assert_eq!(Bits::new(&pack(&text)).dd(0.0).unwrap(), 7.25);
        // tag 01: the first four bytes of the default are replaced.
        let mut patched = String::from("01");
        for byte in 1.0f32.to_le_bytes() {
            patched.push_str(&format!("{byte:08b}"));
        }
        let default = f64::from_le_bytes([0, 0, 0, 0, 0x11, 0x22, 0x33, 0x44]);
        let got = Bits::new(&pack(&patched)).dd(default).unwrap();
        assert_eq!(&got.to_le_bytes()[..4], &1.0f32.to_le_bytes());
        assert_eq!(&got.to_le_bytes()[4..], &[0x11, 0x22, 0x33, 0x44]);
    }

    #[test]
    fn handles_and_modular_numbers() {
        // code 5, two bytes: 0x0102.
        assert_eq!(Bits::new(&[0x52, 0x01, 0x02]).handle().unwrap(), (5, 0x0102));
        assert!(Bits::new(&[0x59, 0, 0, 0, 0, 0, 0, 0, 0, 0]).handle().is_err());
        assert_eq!(Bits::new(&[0x85, 0x01]).mc_unsigned().unwrap(), 5 + 128);
        assert_eq!(Bits::new(&[0x45]).mc_signed().unwrap(), -5);
        assert_eq!(Bits::new(&[0x05]).mc_signed().unwrap(), 5);
        assert_eq!(Bits::new(&[0x01, 0x00]).ms().unwrap(), 1);
        assert_eq!(Bits::new(&[0x01, 0x80, 0x01, 0x00]).ms().unwrap(), 1 + (1 << 15));
    }

    #[test]
    fn hostile_input_ends_in_errors() {
        assert!(Bits::new(&[]).bit().is_err());
        assert!(Bits::new(&[0xFF; 3]).bits(33).is_err());
        assert!(Bits::new(&[0xFF; 20]).mc_unsigned().is_err());
        assert!(Bits::new(&[0xFF; 20]).ms().is_err());
        assert!(Bits::new(&[0xFF; 4]).bytes(usize::MAX).is_err());
        let mut b = Bits::new(&[1, 2]);
        assert!(b.seek(17).is_err());
        let lim = Bits::new(&[1, 2, 3]).limited(8);
        assert_eq!(lim.remaining(), 8);
    }
}
