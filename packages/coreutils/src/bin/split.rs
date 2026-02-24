//! split - split a file into pieces

use std::env;
use std::fs::File;
use std::io::{self, BufRead, BufReader, Read, Write};
use std::process;

fn suffix_for(index: usize) -> String {
    // Generate aa, ab, ..., az, ba, ..., zz
    let first = (b'a' + (index / 26) as u8) as char;
    let second = (b'a' + (index % 26) as u8) as char;
    format!("{}{}", first, second)
}

fn split_by_lines<R: Read>(reader: R, lines_per_file: usize, prefix: &str) {
    let buf = BufReader::new(reader);
    let mut file_index = 0;
    let mut line_count = 0;
    let mut current_file: Option<File> = None;

    for line in buf.lines() {
        let line = match line {
            Ok(l) => l,
            Err(e) => {
                eprintln!("split: {}", e);
                process::exit(1);
            }
        };

        if line_count == 0 || current_file.is_none() {
            let filename = format!("{}{}", prefix, suffix_for(file_index));
            current_file = match File::create(&filename) {
                Ok(f) => Some(f),
                Err(e) => {
                    eprintln!("split: {}: {}", filename, e);
                    process::exit(1);
                }
            };
            file_index += 1;
            line_count = 0;
        }

        if let Some(ref mut f) = current_file {
            if let Err(e) = writeln!(f, "{}", line) {
                eprintln!("split: {}", e);
                process::exit(1);
            }
        }

        line_count += 1;
        if line_count >= lines_per_file {
            line_count = 0;
            current_file = None;
        }
    }
}

fn split_by_bytes<R: Read>(mut reader: R, bytes_per_file: usize, prefix: &str) {
    let mut file_index = 0;
    let mut buf = vec![0u8; bytes_per_file.min(8192)];

    loop {
        let filename = format!("{}{}", prefix, suffix_for(file_index));
        let mut out = match File::create(&filename) {
            Ok(f) => f,
            Err(e) => {
                eprintln!("split: {}: {}", filename, e);
                process::exit(1);
            }
        };

        let mut written = 0;
        let mut eof = false;

        while written < bytes_per_file {
            let to_read = buf.len().min(bytes_per_file - written);
            match reader.read(&mut buf[..to_read]) {
                Ok(0) => {
                    eof = true;
                    break;
                }
                Ok(n) => {
                    if let Err(e) = out.write_all(&buf[..n]) {
                        eprintln!("split: {}", e);
                        process::exit(1);
                    }
                    written += n;
                }
                Err(e) => {
                    eprintln!("split: {}", e);
                    process::exit(1);
                }
            }
        }

        if written == 0 {
            // Remove empty file
            let _ = std::fs::remove_file(&filename);
            break;
        }

        file_index += 1;

        if eof {
            break;
        }
    }
}

fn main() {
    let args: Vec<String> = env::args().collect();

    if args.iter().any(|a| a == "--help") {
        println!("Usage: split [-l N] [-b N] [FILE [PREFIX]]");
        println!("Split a file into pieces.");
        println!("  -l N  put N lines per output file (default 1000)");
        println!("  -b N  put N bytes per output file");
        return;
    }

    let mut lines_per_file: Option<usize> = None;
    let mut bytes_per_file: Option<usize> = None;
    let mut positional: Vec<String> = Vec::new();
    let mut i = 1;

    while i < args.len() {
        if args[i] == "-l" {
            i += 1;
            if i >= args.len() {
                eprintln!("split: option requires an argument -- 'l'");
                process::exit(1);
            }
            lines_per_file = match args[i].parse() {
                Ok(n) if n > 0 => Some(n),
                _ => {
                    eprintln!("split: invalid number of lines: {}", args[i]);
                    process::exit(1);
                }
            };
        } else if args[i] == "-b" {
            i += 1;
            if i >= args.len() {
                eprintln!("split: option requires an argument -- 'b'");
                process::exit(1);
            }
            bytes_per_file = match args[i].parse() {
                Ok(n) if n > 0 => Some(n),
                _ => {
                    eprintln!("split: invalid number of bytes: {}", args[i]);
                    process::exit(1);
                }
            };
        } else if args[i] == "--" {
            positional.extend_from_slice(&args[i + 1..]);
            break;
        } else {
            positional.push(args[i].clone());
        }
        i += 1;
    }

    let input_file = positional.first().map(|s| s.as_str());
    let prefix = positional.get(1).map(|s| s.as_str()).unwrap_or("x");

    if let Some(bytes) = bytes_per_file {
        match input_file {
            Some(path) if path != "-" => match File::open(path) {
                Ok(f) => split_by_bytes(f, bytes, prefix),
                Err(e) => {
                    eprintln!("split: {}: {}", path, e);
                    process::exit(1);
                }
            },
            _ => split_by_bytes(io::stdin().lock(), bytes, prefix),
        }
    } else {
        let lines = lines_per_file.unwrap_or(1000);
        match input_file {
            Some(path) if path != "-" => match File::open(path) {
                Ok(f) => split_by_lines(f, lines, prefix),
                Err(e) => {
                    eprintln!("split: {}: {}", path, e);
                    process::exit(1);
                }
            },
            _ => split_by_lines(io::stdin().lock(), lines, prefix),
        }
    }
}
