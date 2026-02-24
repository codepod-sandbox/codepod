//! strings - find printable strings in files

use std::env;
use std::fs::File;
use std::io::{self, Read};
use std::process;

fn strings_reader<R: Read>(mut reader: R, min_len: usize) {
    let mut buf = Vec::new();
    if let Err(e) = reader.read_to_end(&mut buf) {
        eprintln!("strings: {}", e);
        process::exit(1);
    }

    let mut current = String::new();

    for &byte in &buf {
        if (0x20..0x7f).contains(&byte) || byte == b'\t' {
            current.push(byte as char);
        } else {
            if current.len() >= min_len {
                println!("{}", current);
            }
            current.clear();
        }
    }

    if current.len() >= min_len {
        println!("{}", current);
    }
}

fn main() {
    let args: Vec<String> = env::args().collect();

    if args.iter().any(|a| a == "--help") {
        println!("Usage: strings [-n N] [FILE...]");
        println!("Print sequences of printable characters (at least N long, default 4).");
        return;
    }

    let mut min_len: usize = 4;
    let mut files: Vec<String> = Vec::new();
    let mut i = 1;

    while i < args.len() {
        if args[i] == "-n" {
            i += 1;
            if i >= args.len() {
                eprintln!("strings: option requires an argument -- 'n'");
                process::exit(1);
            }
            min_len = match args[i].parse() {
                Ok(n) if n > 0 => n,
                _ => {
                    eprintln!("strings: invalid minimum length: {}", args[i]);
                    process::exit(1);
                }
            };
        } else if args[i].starts_with("-n") {
            let val = &args[i][2..];
            min_len = match val.parse() {
                Ok(n) if n > 0 => n,
                _ => {
                    eprintln!("strings: invalid minimum length: {}", val);
                    process::exit(1);
                }
            };
        } else if args[i] == "--" {
            files.extend_from_slice(&args[i + 1..]);
            break;
        } else {
            files.push(args[i].clone());
        }
        i += 1;
    }

    if files.is_empty() {
        strings_reader(io::stdin().lock(), min_len);
    } else {
        for path in &files {
            match File::open(path) {
                Ok(f) => strings_reader(f, min_len),
                Err(e) => {
                    eprintln!("strings: {}: {}", path, e);
                    process::exit(1);
                }
            }
        }
    }
}
