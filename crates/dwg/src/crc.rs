// SPDX-License-Identifier: MIT OR Apache-2.0
//! The 16-bit checksum DWG uses for its file header and sections: the reflected polynomial
//! `0xA001` (CRC-16/ARC family) with a seed that depends on what is being summed.

/// Seed for section, class and object checksums.
pub const SECTION_SEED: u16 = 0xC0C1;

/// Checksum of `data`, starting from `seed`.
pub fn crc16(data: &[u8], seed: u16) -> u16 {
    let mut crc = seed;
    for &b in data {
        crc ^= u16::from(b);
        for _ in 0..8 {
            crc = if crc & 1 == 1 { (crc >> 1) ^ 0xA001 } else { crc >> 1 };
        }
    }
    crc
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn standard_check_value() {
        // CRC-16/ARC of "123456789" is 0xBB3D (seed 0): a published reference value.
        assert_eq!(crc16(b"123456789", 0), 0xBB3D);
    }

    #[test]
    fn seed_changes_the_sum() {
        assert_ne!(crc16(b"abc", 0), crc16(b"abc", SECTION_SEED));
        assert_eq!(crc16(b"", SECTION_SEED), SECTION_SEED);
    }
}
