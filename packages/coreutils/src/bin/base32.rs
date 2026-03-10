//! base32 - encode or decode base32

use std::env;
use std::io::{self, Read, Write};
use std::process;

const ENCODE_TABLE: &[u8; 32] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ234567";

fn decode_char(c: u8) -> Option<u8> {
    match c {
        b'A'..=b'Z' => Some(c - b'A'),
        b'a'..=b'z' => Some(c - b'a'),
        b'2'..=b'7' => Some(c - b'2' + 26),
        _ => None,
    }
}

fn encode(input: &[u8]) -> String {
    let mut output = Vec::new();
    let mut i = 0;
    while i < input.len() {
        let remaining = input.len() - i;

        let b0 = input[i] as u64;
        let b1 = if remaining > 1 {
            input[i + 1] as u64
        } else {
            0
        };
        let b2 = if remaining > 2 {
            input[i + 2] as u64
        } else {
            0
        };
        let b3 = if remaining > 3 {
            input[i + 3] as u64
        } else {
            0
        };
        let b4 = if remaining > 4 {
            input[i + 4] as u64
        } else {
            0
        };

        let block = (b0 << 32) | (b1 << 24) | (b2 << 16) | (b3 << 8) | b4;

        output.push(ENCODE_TABLE[((block >> 35) & 0x1F) as usize]);
        output.push(ENCODE_TABLE[((block >> 30) & 0x1F) as usize]);

        if remaining > 1 {
            output.push(ENCODE_TABLE[((block >> 25) & 0x1F) as usize]);
            output.push(ENCODE_TABLE[((block >> 20) & 0x1F) as usize]);
        } else {
            output.push(b'=');
            output.push(b'=');
        }

        if remaining > 2 {
            output.push(ENCODE_TABLE[((block >> 15) & 0x1F) as usize]);
        } else {
            output.push(b'=');
        }

        if remaining > 3 {
            output.push(ENCODE_TABLE[((block >> 10) & 0x1F) as usize]);
            output.push(ENCODE_TABLE[((block >> 5) & 0x1F) as usize]);
        } else {
            output.push(b'=');
            output.push(b'=');
        }

        if remaining > 4 {
            output.push(ENCODE_TABLE[(block & 0x1F) as usize]);
        } else {
            output.push(b'=');
        }

        i += 5;
    }

    String::from_utf8(output).unwrap()
}

fn decode(input: &str) -> Result<Vec<u8>, String> {
    let filtered: Vec<u8> = input
        .bytes()
        .filter(|&b| b != b'\n' && b != b'\r' && b != b' ')
        .collect();
    let mut output = Vec::new();
    let mut i = 0;

    while i < filtered.len() {
        if filtered[i] == b'=' {
            break;
        }

        let mut vals = [0u8; 8];
        let mut count = 0;
        for j in 0..8 {
            if i + j < filtered.len() && filtered[i + j] != b'=' {
                vals[j] = decode_char(filtered[i + j])
                    .ok_or_else(|| format!("invalid character: {}", filtered[i + j] as char))?;
                count += 1;
            }
        }

        let block = ((vals[0] as u64) << 35)
            | ((vals[1] as u64) << 30)
            | ((vals[2] as u64) << 25)
            | ((vals[3] as u64) << 20)
            | ((vals[4] as u64) << 15)
            | ((vals[5] as u64) << 10)
            | ((vals[6] as u64) << 5)
            | (vals[7] as u64);

        output.push(((block >> 32) & 0xFF) as u8);
        if count > 2 {
            output.push(((block >> 24) & 0xFF) as u8);
        }
        if count > 4 {
            output.push(((block >> 16) & 0xFF) as u8);
        }
        if count > 5 {
            output.push(((block >> 8) & 0xFF) as u8);
        }
        if count > 7 {
            output.push((block & 0xFF) as u8);
        }

        i += 8;
    }

    Ok(output)
}

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.iter().any(|a| a == "--help") {
        println!("Usage: base32 [-d]");
        println!("Encode or decode base32 from stdin.");
        println!("  -d  decode base32 input");
        return;
    }

    let do_decode = args.iter().any(|a| a == "-d" || a == "--decode");

    let mut input = Vec::new();
    if io::stdin().read_to_end(&mut input).is_err() {
        eprintln!("base32: read error");
        process::exit(1);
    }

    if do_decode {
        let input_str = String::from_utf8_lossy(&input);
        match decode(&input_str) {
            Ok(decoded) => {
                let stdout = io::stdout();
                let mut out = stdout.lock();
                let _ = out.write_all(&decoded);
                let _ = out.flush();
            }
            Err(e) => {
                eprintln!("base32: {}", e);
                process::exit(1);
            }
        }
    } else {
        let encoded = encode(&input);
        // Wrap at 76 characters
        let stdout = io::stdout();
        let mut out = stdout.lock();
        let bytes = encoded.as_bytes();
        let mut pos = 0;
        while pos < bytes.len() {
            let end = if pos + 76 < bytes.len() {
                pos + 76
            } else {
                bytes.len()
            };
            let _ = out.write_all(&bytes[pos..end]);
            let _ = out.write_all(b"\n");
            pos = end;
        }
        let _ = out.flush();
    }
}
