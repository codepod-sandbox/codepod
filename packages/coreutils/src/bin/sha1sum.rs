//! sha1sum - compute SHA-1 message digest

use std::env;
use std::fs::File;
use std::io::{self, BufRead, BufReader, Read};
use std::process;

/// SHA-1 initial hash values
const H_INIT: [u32; 5] = [0x67452301, 0xefcdab89, 0x98badcfe, 0x10325476, 0xc3d2e1f0];

struct Sha1 {
    state: [u32; 5],
    buffer: Vec<u8>,
    total_len: u64,
}

impl Sha1 {
    fn new() -> Self {
        Sha1 {
            state: H_INIT,
            buffer: Vec::with_capacity(64),
            total_len: 0,
        }
    }

    fn update(&mut self, data: &[u8]) {
        self.total_len += data.len() as u64;
        self.buffer.extend_from_slice(data);

        while self.buffer.len() >= 64 {
            let block: Vec<u8> = self.buffer.drain(..64).collect();
            self.process_block(&block);
        }
    }

    fn process_block(&mut self, block: &[u8]) {
        let mut w = [0u32; 80];

        // Prepare message schedule
        for i in 0..16 {
            w[i] = ((block[i * 4] as u32) << 24)
                | ((block[i * 4 + 1] as u32) << 16)
                | ((block[i * 4 + 2] as u32) << 8)
                | (block[i * 4 + 3] as u32);
        }

        for i in 16..80 {
            w[i] = (w[i - 3] ^ w[i - 8] ^ w[i - 14] ^ w[i - 16]).rotate_left(1);
        }

        let mut a = self.state[0];
        let mut b = self.state[1];
        let mut c = self.state[2];
        let mut d = self.state[3];
        let mut e = self.state[4];

        #[allow(clippy::needless_range_loop)]
        for i in 0..80 {
            let (f, k) = match i {
                0..=19 => ((b & c) | ((!b) & d), 0x5a827999u32),
                20..=39 => (b ^ c ^ d, 0x6ed9eba1u32),
                40..=59 => ((b & c) | (b & d) | (c & d), 0x8f1bbcdcu32),
                _ => (b ^ c ^ d, 0xca62c1d6u32),
            };

            let temp = a
                .rotate_left(5)
                .wrapping_add(f)
                .wrapping_add(e)
                .wrapping_add(k)
                .wrapping_add(w[i]);
            e = d;
            d = c;
            c = b.rotate_left(30);
            b = a;
            a = temp;
        }

        self.state[0] = self.state[0].wrapping_add(a);
        self.state[1] = self.state[1].wrapping_add(b);
        self.state[2] = self.state[2].wrapping_add(c);
        self.state[3] = self.state[3].wrapping_add(d);
        self.state[4] = self.state[4].wrapping_add(e);
    }

    fn finalize(mut self) -> [u8; 20] {
        let bit_len = self.total_len * 8;

        // Append padding bit
        self.buffer.push(0x80);

        // Pad to 56 mod 64 bytes
        while self.buffer.len() % 64 != 56 {
            self.buffer.push(0x00);
        }

        // Append original message length in bits as 64-bit big-endian
        self.buffer.extend_from_slice(&bit_len.to_be_bytes());

        // Process remaining blocks
        while self.buffer.len() >= 64 {
            let block: Vec<u8> = self.buffer.drain(..64).collect();
            self.process_block(&block);
        }

        let mut result = [0u8; 20];
        for (i, &val) in self.state.iter().enumerate() {
            result[i * 4] = (val >> 24) as u8;
            result[i * 4 + 1] = (val >> 16) as u8;
            result[i * 4 + 2] = (val >> 8) as u8;
            result[i * 4 + 3] = val as u8;
        }
        result
    }
}

fn sha1_reader<R: Read>(mut reader: R) -> io::Result<[u8; 20]> {
    let mut hasher = Sha1::new();
    let mut buf = [0u8; 4096];
    loop {
        match reader.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => hasher.update(&buf[..n]),
            Err(e) => return Err(e),
        }
    }
    Ok(hasher.finalize())
}

fn hex_string(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{:02x}", b)).collect()
}

fn check_file(path: &str) -> i32 {
    let reader: Box<dyn BufRead> = if path == "-" {
        Box::new(BufReader::new(io::stdin()))
    } else {
        match File::open(path) {
            Ok(f) => Box::new(BufReader::new(f)),
            Err(e) => {
                eprintln!("sha1sum: {}: {}", path, e);
                return 1;
            }
        }
    };

    let mut failures = 0;
    for line in reader.lines() {
        let line = match line {
            Ok(l) => l,
            Err(e) => {
                eprintln!("sha1sum: {}", e);
                return 1;
            }
        };
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        // Format: hash  filename  (two spaces)
        let parts: Vec<&str> = line.splitn(2, "  ").collect();
        if parts.len() != 2 {
            eprintln!("sha1sum: {}: improperly formatted checksum line", line);
            failures += 1;
            continue;
        }
        let expected_hash = parts[0];
        let filename = parts[1];

        let computed = if filename == "-" {
            sha1_reader(io::stdin())
        } else {
            match File::open(filename) {
                Ok(f) => sha1_reader(f),
                Err(e) => {
                    eprintln!("sha1sum: {}: {}", filename, e);
                    failures += 1;
                    continue;
                }
            }
        };

        match computed {
            Ok(hash) => {
                let hex = hex_string(&hash);
                if hex == expected_hash {
                    println!("{}: OK", filename);
                } else {
                    println!("{}: FAILED", filename);
                    failures += 1;
                }
            }
            Err(e) => {
                eprintln!("sha1sum: {}: {}", filename, e);
                failures += 1;
            }
        }
    }

    if failures > 0 {
        1
    } else {
        0
    }
}

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.iter().any(|a| a == "--help") {
        println!("Usage: sha1sum [-c] [FILE...]");
        println!("Compute or check SHA-1 message digests.");
        println!("  -c  check SHA-1 sums from FILE");
        return;
    }

    let check_mode = args.iter().any(|a| a == "-c" || a == "--check");
    let files: Vec<&str> = args[1..]
        .iter()
        .filter(|a| *a != "-c" && *a != "--check")
        .map(|s| s.as_str())
        .collect();

    if check_mode {
        let path = if files.is_empty() { "-" } else { files[0] };
        process::exit(check_file(path));
    }

    if files.is_empty() {
        match sha1_reader(io::stdin()) {
            Ok(hash) => println!("{}  -", hex_string(&hash)),
            Err(e) => {
                eprintln!("sha1sum: {}", e);
                process::exit(1);
            }
        }
    } else {
        let mut exit_code = 0;
        for file in &files {
            if *file == "-" {
                match sha1_reader(io::stdin()) {
                    Ok(hash) => println!("{}  -", hex_string(&hash)),
                    Err(e) => {
                        eprintln!("sha1sum: {}", e);
                        exit_code = 1;
                    }
                }
            } else {
                match File::open(file) {
                    Ok(f) => match sha1_reader(f) {
                        Ok(hash) => println!("{}  {}", hex_string(&hash), file),
                        Err(e) => {
                            eprintln!("sha1sum: {}: {}", file, e);
                            exit_code = 1;
                        }
                    },
                    Err(e) => {
                        eprintln!("sha1sum: {}: {}", file, e);
                        exit_code = 1;
                    }
                }
            }
        }
        process::exit(exit_code);
    }
}
