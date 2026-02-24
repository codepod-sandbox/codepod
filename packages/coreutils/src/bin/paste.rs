//! paste - merge lines of files

use std::env;
use std::fs::File;
use std::io::{self, BufRead, BufReader};
use std::process;

fn main() {
    let args: Vec<String> = env::args().collect();

    if args.iter().any(|a| a == "--help") {
        println!("Usage: paste [-d DELIM] [-s] [FILE...]");
        println!("Merge lines of files side by side.");
        println!("  -d DELIM  use DELIM instead of tab");
        println!("  -s        serial mode (one file per line)");
        return;
    }

    let mut delimiter = "\t".to_string();
    let mut serial = false;
    let mut file_args: Vec<String> = Vec::new();
    let mut i = 1;

    while i < args.len() {
        if args[i] == "-d" {
            i += 1;
            if i >= args.len() {
                eprintln!("paste: option requires an argument -- 'd'");
                process::exit(1);
            }
            delimiter = args[i].clone();
        } else if args[i].starts_with("-d") {
            delimiter = args[i][2..].to_string();
        } else if args[i] == "-s" {
            serial = true;
        } else if args[i] == "--" {
            file_args.extend_from_slice(&args[i + 1..]);
            break;
        } else {
            file_args.push(args[i].clone());
        }
        i += 1;
    }

    if file_args.is_empty() {
        file_args.push("-".to_string());
    }

    if serial {
        paste_serial(&file_args, &delimiter);
    } else {
        paste_parallel(&file_args, &delimiter);
    }
}

fn paste_serial(file_args: &[String], delimiter: &str) {
    for path in file_args {
        let reader: Box<dyn BufRead> = if path == "-" {
            Box::new(BufReader::new(io::stdin().lock()))
        } else {
            match File::open(path) {
                Ok(f) => Box::new(BufReader::new(f)),
                Err(e) => {
                    eprintln!("paste: {}: {}", path, e);
                    process::exit(1);
                }
            }
        };

        let mut first = true;
        for line in reader.lines() {
            let line = match line {
                Ok(l) => l,
                Err(e) => {
                    eprintln!("paste: {}", e);
                    process::exit(1);
                }
            };
            if !first {
                print!("{}", delimiter);
            }
            print!("{}", line);
            first = false;
        }
        println!();
    }
}

fn paste_parallel(file_args: &[String], delimiter: &str) {
    let mut readers: Vec<Option<Box<dyn BufRead>>> = Vec::new();

    for path in file_args {
        if path == "-" {
            readers.push(Some(Box::new(BufReader::new(io::stdin().lock()))));
        } else {
            match File::open(path) {
                Ok(f) => readers.push(Some(Box::new(BufReader::new(f)))),
                Err(e) => {
                    eprintln!("paste: {}: {}", path, e);
                    process::exit(1);
                }
            }
        }
    }

    let delim_chars: Vec<char> = if delimiter.is_empty() {
        vec![]
    } else {
        delimiter.chars().collect()
    };

    loop {
        let mut any_line = false;
        let mut parts: Vec<String> = Vec::new();

        for reader_opt in readers.iter_mut() {
            if let Some(reader) = reader_opt {
                let mut line = String::new();
                match reader.read_line(&mut line) {
                    Ok(0) => {
                        parts.push(String::new());
                        *reader_opt = None;
                    }
                    Ok(_) => {
                        let trimmed = line.trim_end_matches('\n').trim_end_matches('\r');
                        parts.push(trimmed.to_string());
                        any_line = true;
                    }
                    Err(e) => {
                        eprintln!("paste: {}", e);
                        process::exit(1);
                    }
                }
            } else {
                parts.push(String::new());
            }
        }

        if !any_line {
            break;
        }

        let mut output = String::new();
        for (idx, part) in parts.iter().enumerate() {
            if idx > 0 && !delim_chars.is_empty() {
                let d = delim_chars[idx.wrapping_sub(1) % delim_chars.len()];
                output.push(d);
            }
            output.push_str(part);
        }
        println!("{}", output);
    }
}
