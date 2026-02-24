//! expand - convert tabs to spaces

use std::env;
use std::fs::File;
use std::io::{self, BufRead, BufReader, Read};
use std::process;

fn expand_reader<R: Read>(reader: R, tab_stop: usize) {
    let buf = BufReader::new(reader);
    for line in buf.lines() {
        let line = match line {
            Ok(l) => l,
            Err(e) => {
                eprintln!("expand: {}", e);
                process::exit(1);
            }
        };

        let mut col = 0;
        let mut output = String::new();
        for ch in line.chars() {
            if ch == '\t' {
                let spaces = tab_stop - (col % tab_stop);
                for _ in 0..spaces {
                    output.push(' ');
                }
                col += spaces;
            } else {
                output.push(ch);
                col += 1;
            }
        }
        println!("{}", output);
    }
}

fn main() {
    let args: Vec<String> = env::args().collect();

    if args.iter().any(|a| a == "--help") {
        println!("Usage: expand [-t N] [FILE...]");
        println!("Convert tabs to spaces.");
        println!("  -t N  set tab stop to N (default 8)");
        return;
    }

    let mut tab_stop: usize = 8;
    let mut files: Vec<String> = Vec::new();
    let mut i = 1;

    while i < args.len() {
        if args[i] == "-t" {
            i += 1;
            if i >= args.len() {
                eprintln!("expand: option requires an argument -- 't'");
                process::exit(1);
            }
            tab_stop = match args[i].parse() {
                Ok(n) if n > 0 => n,
                _ => {
                    eprintln!("expand: invalid tab stop: {}", args[i]);
                    process::exit(1);
                }
            };
        } else if args[i].starts_with("-t") {
            let val = &args[i][2..];
            tab_stop = match val.parse() {
                Ok(n) if n > 0 => n,
                _ => {
                    eprintln!("expand: invalid tab stop: {}", val);
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
        expand_reader(io::stdin().lock(), tab_stop);
    } else {
        for path in &files {
            match File::open(path) {
                Ok(f) => expand_reader(f, tab_stop),
                Err(e) => {
                    eprintln!("expand: {}: {}", path, e);
                    process::exit(1);
                }
            }
        }
    }
}
