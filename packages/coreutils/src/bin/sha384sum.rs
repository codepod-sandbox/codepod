//! sha384sum - compute SHA-384 message digest

use std::env;
use std::fs::File;
use std::io::{self, BufRead, BufReader, Read};
use std::process;

/// SHA-384 initial hash values (first 64 bits of fractional parts of square roots of 9th-16th primes)
const H_INIT: [u64; 8] = [
    0xcbbb9d5dc1059ed8,
    0x629a292a367cd507,
    0x9159015a3070dd17,
    0x152fecd8f70e5939,
    0x67332667ffc00b31,
    0x8eb44a8768581511,
    0xdb0c2e0d64f98fa7,
    0x47b5481dbefa4fa4,
];

/// SHA-384/512 round constants (first 64 bits of fractional parts of cube roots of first 80 primes)
const K: [u64; 80] = [
    0x428a2f98d728ae22,
    0x7137449123ef65cd,
    0xb5c0fbcfec4d3b2f,
    0xe9b5dba58189dbbc,
    0x3956c25bf348b538,
    0x59f111f1b605d019,
    0x923f82a4af194f9b,
    0xab1c5ed5da6d8118,
    0xd807aa98a3030242,
    0x12835b0145706fbe,
    0x243185be4ee4b28c,
    0x550c7dc3d5ffb4e2,
    0x72be5d74f27b896f,
    0x80deb1fe3b1696b1,
    0x9bdc06a725c71235,
    0xc19bf174cf692694,
    0xe49b69c19ef14ad2,
    0xefbe4786384f25e3,
    0x0fc19dc68b8cd5b5,
    0x240ca1cc77ac9c65,
    0x2de92c6f592b0275,
    0x4a7484aa6ea6e483,
    0x5cb0a9dcbd41fbd4,
    0x76f988da831153b5,
    0x983e5152ee66dfab,
    0xa831c66d2db43210,
    0xb00327c898fb213f,
    0xbf597fc7beef0ee4,
    0xc6e00bf33da88fc2,
    0xd5a79147930aa725,
    0x06ca6351e003826f,
    0x142929670a0e6e70,
    0x27b70a8546d22ffc,
    0x2e1b21385c26c926,
    0x4d2c6dfc5ac42aed,
    0x53380d139d95b3df,
    0x650a73548baf63de,
    0x766a0abb3c77b2a8,
    0x81c2c92e47edaee6,
    0x92722c851482353b,
    0xa2bfe8a14cf10364,
    0xa81a664bbc423001,
    0xc24b8b70d0f89791,
    0xc76c51a30654be30,
    0xd192e819d6ef5218,
    0xd69906245565a910,
    0xf40e35855771202a,
    0x106aa07032bbd1b8,
    0x19a4c116b8d2d0c8,
    0x1e376c085141ab53,
    0x2748774cdf8eeb99,
    0x34b0bcb5e19b48a8,
    0x391c0cb3c5c95a63,
    0x4ed8aa4ae3418acb,
    0x5b9cca4f7763e373,
    0x682e6ff3d6b2b8a3,
    0x748f82ee5defb2fc,
    0x78a5636f43172f60,
    0x84c87814a1f0ab72,
    0x8cc702081a6439ec,
    0x90befffa23631e28,
    0xa4506cebde82bde9,
    0xbef9a3f7b2c67915,
    0xc67178f2e372532b,
    0xca273eceea26619c,
    0xd186b8c721c0c207,
    0xeada7dd6cde0eb1e,
    0xf57d4f7fee6ed178,
    0x06f067aa72176fba,
    0x0a637dc5a2c898a6,
    0x113f9804bef90dae,
    0x1b710b35131c471b,
    0x28db77f523047d84,
    0x32caab7b40c72493,
    0x3c9ebe0a15c9bebc,
    0x431d67c49c100d4c,
    0x4cc5d4becb3e42b6,
    0x597f299cfc657e2a,
    0x5fcb6fab3ad6faec,
    0x6c44198c4a475817,
];

struct Sha384 {
    state: [u64; 8],
    buffer: Vec<u8>,
    total_len: u128,
}

impl Sha384 {
    fn new() -> Self {
        Sha384 {
            state: H_INIT,
            buffer: Vec::with_capacity(128),
            total_len: 0,
        }
    }

    fn update(&mut self, data: &[u8]) {
        self.total_len += data.len() as u128;
        self.buffer.extend_from_slice(data);

        while self.buffer.len() >= 128 {
            let block: Vec<u8> = self.buffer.drain(..128).collect();
            self.process_block(&block);
        }
    }

    fn process_block(&mut self, block: &[u8]) {
        let mut w = [0u64; 80];

        // Prepare message schedule
        for i in 0..16 {
            w[i] = ((block[i * 8] as u64) << 56)
                | ((block[i * 8 + 1] as u64) << 48)
                | ((block[i * 8 + 2] as u64) << 40)
                | ((block[i * 8 + 3] as u64) << 32)
                | ((block[i * 8 + 4] as u64) << 24)
                | ((block[i * 8 + 5] as u64) << 16)
                | ((block[i * 8 + 6] as u64) << 8)
                | (block[i * 8 + 7] as u64);
        }

        for i in 16..80 {
            let s0 = w[i - 15].rotate_right(1) ^ w[i - 15].rotate_right(8) ^ (w[i - 15] >> 7);
            let s1 = w[i - 2].rotate_right(19) ^ w[i - 2].rotate_right(61) ^ (w[i - 2] >> 6);
            w[i] = w[i - 16]
                .wrapping_add(s0)
                .wrapping_add(w[i - 7])
                .wrapping_add(s1);
        }

        let mut a = self.state[0];
        let mut b = self.state[1];
        let mut c = self.state[2];
        let mut d = self.state[3];
        let mut e = self.state[4];
        let mut f = self.state[5];
        let mut g = self.state[6];
        let mut h = self.state[7];

        for i in 0..80 {
            let s1 = e.rotate_right(14) ^ e.rotate_right(18) ^ e.rotate_right(41);
            let ch = (e & f) ^ ((!e) & g);
            let temp1 = h
                .wrapping_add(s1)
                .wrapping_add(ch)
                .wrapping_add(K[i])
                .wrapping_add(w[i]);
            let s0 = a.rotate_right(28) ^ a.rotate_right(34) ^ a.rotate_right(39);
            let maj = (a & b) ^ (a & c) ^ (b & c);
            let temp2 = s0.wrapping_add(maj);

            h = g;
            g = f;
            f = e;
            e = d.wrapping_add(temp1);
            d = c;
            c = b;
            b = a;
            a = temp1.wrapping_add(temp2);
        }

        self.state[0] = self.state[0].wrapping_add(a);
        self.state[1] = self.state[1].wrapping_add(b);
        self.state[2] = self.state[2].wrapping_add(c);
        self.state[3] = self.state[3].wrapping_add(d);
        self.state[4] = self.state[4].wrapping_add(e);
        self.state[5] = self.state[5].wrapping_add(f);
        self.state[6] = self.state[6].wrapping_add(g);
        self.state[7] = self.state[7].wrapping_add(h);
    }

    fn finalize(mut self) -> [u8; 48] {
        let bit_len = self.total_len * 8;

        // Append padding bit
        self.buffer.push(0x80);

        // Pad to 112 mod 128 bytes
        while self.buffer.len() % 128 != 112 {
            self.buffer.push(0x00);
        }

        // Append original message length in bits as 128-bit big-endian
        self.buffer.extend_from_slice(&bit_len.to_be_bytes());

        // Process remaining blocks
        while self.buffer.len() >= 128 {
            let block: Vec<u8> = self.buffer.drain(..128).collect();
            self.process_block(&block);
        }

        // SHA-384 uses only the first 6 of 8 state words (48 bytes)
        let mut result = [0u8; 48];
        for i in 0..6 {
            let val = self.state[i];
            result[i * 8] = (val >> 56) as u8;
            result[i * 8 + 1] = (val >> 48) as u8;
            result[i * 8 + 2] = (val >> 40) as u8;
            result[i * 8 + 3] = (val >> 32) as u8;
            result[i * 8 + 4] = (val >> 24) as u8;
            result[i * 8 + 5] = (val >> 16) as u8;
            result[i * 8 + 6] = (val >> 8) as u8;
            result[i * 8 + 7] = val as u8;
        }
        result
    }
}

fn sha384_reader<R: Read>(mut reader: R) -> io::Result<[u8; 48]> {
    let mut hasher = Sha384::new();
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
                eprintln!("sha384sum: {}: {}", path, e);
                return 1;
            }
        }
    };

    let mut failures = 0;
    for line in reader.lines() {
        let line = match line {
            Ok(l) => l,
            Err(e) => {
                eprintln!("sha384sum: {}", e);
                return 1;
            }
        };
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let parts: Vec<&str> = line.splitn(2, "  ").collect();
        if parts.len() != 2 {
            eprintln!("sha384sum: {}: improperly formatted checksum line", line);
            failures += 1;
            continue;
        }
        let expected_hash = parts[0];
        let filename = parts[1];

        let computed = if filename == "-" {
            sha384_reader(io::stdin())
        } else {
            match File::open(filename) {
                Ok(f) => sha384_reader(f),
                Err(e) => {
                    eprintln!("sha384sum: {}: {}", filename, e);
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
                eprintln!("sha384sum: {}: {}", filename, e);
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
        println!("Usage: sha384sum [-c] [FILE...]");
        println!("Compute or check SHA-384 message digests.");
        println!("  -c  check SHA-384 sums from FILE");
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
        match sha384_reader(io::stdin()) {
            Ok(hash) => println!("{}  -", hex_string(&hash)),
            Err(e) => {
                eprintln!("sha384sum: {}", e);
                process::exit(1);
            }
        }
    } else {
        let mut exit_code = 0;
        for file in &files {
            if *file == "-" {
                match sha384_reader(io::stdin()) {
                    Ok(hash) => println!("{}  -", hex_string(&hash)),
                    Err(e) => {
                        eprintln!("sha384sum: {}", e);
                        exit_code = 1;
                    }
                }
            } else {
                match File::open(file) {
                    Ok(f) => match sha384_reader(f) {
                        Ok(hash) => println!("{}  {}", hex_string(&hash), file),
                        Err(e) => {
                            eprintln!("sha384sum: {}: {}", file, e);
                            exit_code = 1;
                        }
                    },
                    Err(e) => {
                        eprintln!("sha384sum: {}: {}", file, e);
                        exit_code = 1;
                    }
                }
            }
        }
        process::exit(exit_code);
    }
}
