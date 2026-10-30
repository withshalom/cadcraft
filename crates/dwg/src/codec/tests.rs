//! Tests of the codec's building blocks: the specification's worked examples, standard checksum
//! check values, hostile input, and (when `MI201_CORPUS` names a folder of AutoCAD-saved files) a
//! measurement over real drawings.

use super::bits::{BitReader, ByteReader};
use super::checksum::{crc16, crc32, page_checksum};
use super::container::{Container, Version, magic_sequence};
use super::{Limits, lz77, objmap};

/// Packs a string of '0'/'1' into bytes, MSB first, padding the last byte with zeros.
fn bits_of(s: &str) -> Vec<u8> {
    let mut out = vec![0u8; s.len().div_ceil(8)];
    for (i, c) in s.chars().enumerate() {
        if c == '1' {
            out[i / 8] |= 0x80 >> (i % 8);
        }
    }
    out
}

#[test]
fn checksums_match_their_standard_check_values() {
    assert_eq!(crc32(0, b"123456789"), 0xCBF4_3926);
    assert_eq!(crc16(0, b"123456789"), 0xBB3D);
    // Continuing a CRC equals computing it in one go.
    assert_eq!(crc32(crc32(0, b"1234"), b"56789"), crc32(0, b"123456789"));
    assert_eq!(page_checksum(0, &[]), 0);
    assert_eq!(page_checksum(0, b"abc"), 0x024A_0126);
}

#[test]
fn the_magic_sequence_starts_as_the_specification_lists_it() {
    assert_eq!(magic_sequence(4), [0x29, 0x23, 0xBE, 0x84]);
    // The printed table stops after 92 of the 0x6C bytes; its last byte is 0xB7.
    assert_eq!(magic_sequence(92).last(), Some(&0xB7));
}

#[test]
fn bitshorts_parse_as_in_the_specification() {
    let data = bits_of("0000000001000000011011010000111110");
    let mut r = BitReader::new(&data);
    let got: Vec<i16> = (0..5).map(|_| r.bs().unwrap()).collect();
    assert_eq!(got, [257, 0, 256, 15, 0]);
}

#[test]
fn bitlongs_parse_as_in_the_specification() {
    let data = bits_of("000000000100000001000000000000000010010000111110");
    let mut r = BitReader::new(&data);
    let got: Vec<i32> = (0..4).map(|_| r.bl().unwrap()).collect();
    assert_eq!(got, [257, 0, 15, 0]);
}

#[test]
fn modular_chars_and_shorts_parse_as_in_the_specification() {
    assert_eq!(ByteReader::new(&[0x82, 0x24]).mc_unsigned().unwrap(), 4610);
    assert_eq!(ByteReader::new(&[0xE9, 0x97, 0xE6, 0x35]).mc_unsigned().unwrap(), 112_823_273);
    assert_eq!(ByteReader::new(&[0x85, 0x4B]).mc_signed().unwrap(), -1413);
    assert_eq!(ByteReader::new(&[0x31, 0xF4, 0x8D, 0x00]).ms().unwrap(), 4_650_033);
}

#[test]
fn handles_and_doubles_with_defaults() {
    // 5.2.05.E7: a hard pointer to handle 0x5E7.
    let mut r = BitReader::new(&[0b0101_0010, 0x05, 0xE7]);
    let h = r.handle().unwrap();
    assert_eq!((h.code, h.value), (5, 0x5E7));
    // DD code 00 keeps the default.
    let mut r = BitReader::new(&[0x00]);
    assert_eq!(r.dd(2.5).unwrap(), 2.5);
}

#[test]
fn decompression_copies_overlapping_runs() {
    // "abcd", then copy 3 bytes from 4 back, then end.
    let stream = [0x01, b'a', b'b', b'c', b'd', 0x4C, 0x00, 0x11];
    assert_eq!(lz77::decompress(&stream, 64).unwrap(), b"abcdabc");
    // The expected size is a hard cap.
    assert!(lz77::decompress(&stream, 5).is_err());
}

/// A deterministic byte generator for hostile-input tests.
fn noise(seed: u32, len: usize) -> Vec<u8> {
    let mut s = seed;
    (0..len)
        .map(|_| {
            s = s.wrapping_mul(1_103_515_245).wrapping_add(12_345);
            (s >> 16) as u8
        })
        .collect()
}

#[test]
fn hostile_input_never_panics() {
    for seed in 0..300u32 {
        let junk = noise(seed, 64 + (seed as usize * 7) % 2000);
        let _ = lz77::decompress(&junk, 1 << 16);
        let _ = objmap::read(&junk, Limits::DEFAULT);
        let mut r = BitReader::new(&junk);
        while r.remaining() > 0 && r.bd().is_ok() && r.handle().is_ok() && r.bll().is_ok() {}
        let mut file = b"AC1032".to_vec();
        file.extend(&junk);
        let _ = Container::read(&file, Limits::DEFAULT);
    }
    // Truncations of a valid-looking prefix.
    for n in 0..0x100 {
        let _ = Container::read(&b"AC1018".repeat(n / 6 + 1)[..n.min(6 * (n / 6 + 1))], Limits::DEFAULT);
    }
}

/// The folder of AutoCAD-saved test drawings, when the environment names one.
fn corpus() -> Option<std::path::PathBuf> {
    std::env::var_os("MI201_CORPUS").map(std::path::PathBuf::from).filter(|p| p.is_dir())
}

/// Reads every 2004-family DWG of the corpus: maps, every section, the object map.
#[test]
fn corpus_containers_read_whole() {
    let Some(dir) = corpus() else { return };
    let mut files: Vec<_> =
        std::fs::read_dir(&dir).unwrap().filter_map(|e| e.ok().map(|e| e.path())).filter(|p| p.extension().is_some_and(|x| x == "dwg")).collect();
    files.sort();
    for path in files {
        let bytes = std::fs::read(&path).unwrap();
        let Some(version) = bytes.get(0..6).and_then(Version::from_magic) else {
            println!("{}: not a 2004-family file, skipped", path.display());
            continue;
        };
        let c = Container::read(&bytes, Limits::DEFAULT).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        assert_eq!(c.version, version);
        let mut total = 0usize;
        for info in &c.sections {
            let data = c.section_data(info).unwrap_or_else(|e| panic!("{} {}: {e}", path.display(), info.name));
            assert_eq!(data.len() as u64, info.size, "{} {}", path.display(), info.name);
            total += data.len();
        }
        let handles = c.section("AcDb:Handles").unwrap().expect("object map");
        let objects = c.section("AcDb:AcDbObjects").unwrap().expect("objects");
        let map = objmap::read(&handles, Limits::DEFAULT).unwrap();
        assert!(!map.entries.is_empty());
        assert!(map.entries.windows(2).all(|w| w[0].handle < w[1].handle), "handles ascend");
        assert!(map.entries.iter().all(|e| (e.offset as usize) < objects.len()), "offsets inside the objects section");
        println!(
            "{}: {} sections, {} bytes decompressed, {} objects, {} bad object-map CRCs, notes {:?}",
            path.file_name().unwrap().to_string_lossy(),
            c.sections.len(),
            total,
            map.entries.len(),
            map.bad_crcs,
            c.notes
        );
    }
}
