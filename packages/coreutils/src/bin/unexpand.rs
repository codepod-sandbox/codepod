//! unexpand - convert spaces to tabs

use std::env;
use std::fs::File;
use std::io::{self, BufRead, BufReader, Read};
use std::process;

fn unexpand_reader<R: Read>(reader: R, tab_stop: usize, all: bool) {
    let buf = BufReader::new(reader);
    for line in buf.lines() {
        let line = match line {
            Ok(l) => l,
            Err(e) => {
                eprintln!("unexpand: {}", e);
                process::exit(1);
            }
        };

        if all {
            println!("{}", unexpand_all(&line, tab_stop));
        } else {
            println!("{}", unexpand_leading(&line, tab_stop));
        }
    }
}

fn unexpand_leading(line: &str, tab_stop: usize) -> String {
    let mut result = String::new();
    let mut col = 0;
    let mut in_leading = true;
    let mut space_count = 0;

    for ch in line.chars() {
        if in_leading && ch == ' ' {
            space_count += 1;
            col += 1;
            if col % tab_stop == 0 {
                result.push('\t');
                space_count = 0;
            }
        } else if in_leading && ch == '\t' {
            // Already a tab; emit it and reset
            // Add any pending spaces first
            for _ in 0..space_count {
                result.push(' ');
            }
            space_count = 0;
            result.push('\t');
            col = ((col / tab_stop) + 1) * tab_stop;
        } else {
            if in_leading {
                for _ in 0..space_count {
                    result.push(' ');
                }
                space_count = 0;
                in_leading = false;
            }
            result.push(ch);
            col += 1;
        }
    }

    // Flush any remaining leading spaces
    if in_leading {
        for _ in 0..space_count {
            result.push(' ');
        }
    }

    result
}

fn unexpand_all(line: &str, tab_stop: usize) -> String {
    let mut result = String::new();
    let mut col = 0;
    let mut space_count = 0;

    for ch in line.chars() {
        if ch == ' ' {
            space_count += 1;
            col += 1;
            if col % tab_stop == 0 && space_count > 1 {
                result.push('\t');
                space_count = 0;
            }
        } else {
            for _ in 0..space_count {
                result.push(' ');
            }
            space_count = 0;
            if ch == '\t' {
                result.push('\t');
                col = ((col / tab_stop) + 1) * tab_stop;
            } else {
                result.push(ch);
                col += 1;
            }
        }
    }

    for _ in 0..space_count {
        result.push(' ');
    }

    result
}

fn main() {
    let args: Vec<String> = env::args().collect();

    if args.iter().any(|a| a == "--help") {
        println!("Usage: unexpand [-a] [-t N] [FILE...]");
        println!("Convert spaces to tabs.");
        println!("  -a    convert all runs of spaces, not just leading");
        println!("  -t N  set tab stop to N (default 8)");
        return;
    }

    let mut tab_stop: usize = 8;
    let mut all = false;
    let mut files: Vec<String> = Vec::new();
    let mut i = 1;

    while i < args.len() {
        if args[i] == "-a" {
            all = true;
        } else if args[i] == "-t" {
            i += 1;
            if i >= args.len() {
                eprintln!("unexpand: option requires an argument -- 't'");
                process::exit(1);
            }
            tab_stop = match args[i].parse() {
                Ok(n) if n > 0 => n,
                _ => {
                    eprintln!("unexpand: invalid tab stop: {}", args[i]);
                    process::exit(1);
                }
            };
        } else if args[i].starts_with("-t") {
            let val = &args[i][2..];
            tab_stop = match val.parse() {
                Ok(n) if n > 0 => n,
                _ => {
                    eprintln!("unexpand: invalid tab stop: {}", val);
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
        unexpand_reader(io::stdin().lock(), tab_stop, all);
    } else {
        for path in &files {
            match File::open(path) {
                Ok(f) => unexpand_reader(f, tab_stop, all),
                Err(e) => {
                    eprintln!("unexpand: {}: {}", path, e);
                    process::exit(1);
                }
            }
        }
    }
}
