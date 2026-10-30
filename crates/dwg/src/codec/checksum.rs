//! The three checksums a DWG file uses (ODA specification §2.14 and §4.2).
//!
//! - [`crc16`]: the "8-bit CRC" of the specification, a 16-bit CRC with the reflected 0xA001
//!   polynomial, seeded per use.
//! - [`crc32`]: the standard reflected CRC-32 (polynomial 0xEDB88320), used by 2004+ headers.
//! - [`page_checksum`]: the Adler-style sum of 2004+ section page headers and data.
//!
//! The lookup tables are computed from the polynomials, not stored.

/// The 16-bit CRC table for the reflected polynomial 0xA001.
const CRC16_TABLE: [u16; 256] = crc16_table();
/// The 32-bit CRC table for the reflected polynomial 0xEDB88320.
const CRC32_TABLE: [u32; 256] = crc32_table();

const fn crc16_table() -> [u16; 256] {
    let mut table = [0u16; 256];
    let mut i = 0;
    while i < 256 {
        let mut c = i as u16;
        let mut k = 0;
        while k < 8 {
            c = if c & 1 != 0 { (c >> 1) ^ 0xA001 } else { c >> 1 };
            k += 1;
        }
        table[i] = c;
        i += 1;
    }
    table
}

const fn crc32_table() -> [u32; 256] {
    let mut table = [0u32; 256];
    let mut i = 0;
    while i < 256 {
        let mut c = i as u32;
        let mut k = 0;
        while k < 8 {
            c = if c & 1 != 0 { (c >> 1) ^ 0xEDB8_8320 } else { c >> 1 };
            k += 1;
        }
        table[i] = c;
        i += 1;
    }
    table
}

/// The DWG 16-bit CRC of `data`, continuing from `seed`.
pub fn crc16(seed: u16, data: &[u8]) -> u16 {
    data.iter().fold(seed, |crc, &b| (crc >> 8) ^ CRC16_TABLE[usize::from((crc as u8) ^ b)])
}

/// The standard CRC-32 of `data`, continuing from `seed` (0 for a fresh CRC).
pub fn crc32(seed: u32, data: &[u8]) -> u32 {
    !data.iter().fold(!seed, |crc, &b| (crc >> 8) ^ CRC32_TABLE[usize::from((crc as u8) ^ b)])
}

/// The 2004+ section page checksum of `data`, continuing from `seed`.
pub fn page_checksum(seed: u32, data: &[u8]) -> u32 {
    const MODULUS: u32 = 0xFFF1;
    let mut sum1 = seed & 0xFFFF;
    let mut sum2 = seed >> 16;
    // Chunks of 0x15B0 bytes keep both sums below 2^32 before each reduction.
    for chunk in data.chunks(0x15B0) {
        for &b in chunk {
            sum1 += u32::from(b);
            sum2 += sum1;
        }
        sum1 %= MODULUS;
        sum2 %= MODULUS;
    }
    (sum2 << 16) | (sum1 & 0xFFFF)
}
