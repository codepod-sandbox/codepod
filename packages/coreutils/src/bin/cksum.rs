//! cksum - compute CRC-32 checksum and byte count (POSIX)

use std::env;
use std::fs::File;
use std::io::{self, Read};
use std::process;

/// POSIX CRC-32 lookup table using reflected polynomial 0xEDB88320
const fn make_crc_table() -> [u32; 256] {
    let mut table = [0u32; 256];
    let mut i = 0u32;
    while i < 256 {
        let mut crc = i;
        let mut j = 0;
        while j < 8 {
            if crc & 1 != 0 {
                crc = (crc >> 1) ^ 0xEDB88320;
            } else {
                crc >>= 1;
            }
            j += 1;
        }
        table[i as usize] = crc;
        i += 1;
    }
    table
}

static CRC_TABLE: [u32; 256] = make_crc_table();

/// Compute POSIX cksum CRC.
/// The POSIX cksum algorithm processes all data bytes, then the byte count
/// (length) encoded as octets (most-significant first, excluding leading zeros),
/// then inverts the result.
fn posix_cksum(data: &[u8]) -> u32 {
    let mut crc: u32 = 0xFFFFFFFF;

    // Process data bytes
    for &byte in data {
        let idx = ((crc ^ byte as u32) & 0xFF) as usize;
        crc = CRC_TABLE[idx] ^ (crc >> 8);
    }

    // Process the length
    let mut len = data.len();
    while len > 0 {
        let byte = (len & 0xFF) as u8;
        let idx = ((crc ^ byte as u32) & 0xFF) as usize;
        crc = CRC_TABLE[idx] ^ (crc >> 8);
        len >>= 8;
    }

    !crc
}

fn cksum_reader<R: Read>(mut reader: R, name: &str) {
    let mut data = Vec::new();
    if let Err(e) = reader.read_to_end(&mut data) {
        eprintln!("cksum: {}", e);
        process::exit(1);
    }

    let crc = posix_cksum(&data);
    let byte_count = data.len();

    if name.is_empty() {
        println!("{} {}", crc, byte_count);
    } else {
        println!("{} {} {}", crc, byte_count, name);
    }
}

fn main() {
    let args: Vec<String> = env::args().collect();

    if args.iter().any(|a| a == "--help") {
        println!("Usage: cksum [FILE...]");
        println!("Compute CRC-32 checksum and byte count for each FILE.");
        return;
    }

    let files: Vec<&str> = args[1..].iter().map(|s| s.as_str()).collect();

    if files.is_empty() {
        cksum_reader(io::stdin().lock(), "");
    } else {
        for path in &files {
            match File::open(path) {
                Ok(f) => cksum_reader(f, path),
                Err(e) => {
                    eprintln!("cksum: {}: {}", path, e);
                    process::exit(1);
                }
            }
        }
    }
}
