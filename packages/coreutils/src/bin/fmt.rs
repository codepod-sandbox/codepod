//! fmt - rewrap text to a specified width

use std::env;
use std::fs::File;
use std::io::{self, BufRead, BufReader, Read};
use std::process;

fn fmt_reader<R: Read>(reader: R, width: usize) {
    let buf = BufReader::new(reader);
    let mut paragraph: Vec<String> = Vec::new();

    for line in buf.lines() {
        let line = match line {
            Ok(l) => l,
            Err(e) => {
                eprintln!("fmt: {}", e);
                process::exit(1);
            }
        };

        if line.trim().is_empty() {
            if !paragraph.is_empty() {
                output_paragraph(&paragraph, width);
                paragraph.clear();
            }
            println!();
        } else {
            paragraph.push(line);
        }
    }

    if !paragraph.is_empty() {
        output_paragraph(&paragraph, width);
    }
}

fn output_paragraph(lines: &[String], width: usize) {
    let mut words: Vec<&str> = Vec::new();
    for line in lines {
        for word in line.split_whitespace() {
            words.push(word);
        }
    }

    if words.is_empty() {
        return;
    }

    let mut current_line = String::new();
    for word in &words {
        if current_line.is_empty() {
            current_line.push_str(word);
        } else if current_line.len() + 1 + word.len() <= width {
            current_line.push(' ');
            current_line.push_str(word);
        } else {
            println!("{}", current_line);
            current_line.clear();
            current_line.push_str(word);
        }
    }
    if !current_line.is_empty() {
        println!("{}", current_line);
    }
}

fn main() {
    let args: Vec<String> = env::args().collect();

    if args.iter().any(|a| a == "--help") {
        println!("Usage: fmt [-w WIDTH] [FILE...]");
        println!("Rewrap text to fit within WIDTH columns (default 75).");
        return;
    }

    let mut width: usize = 75;
    let mut files: Vec<String> = Vec::new();
    let mut i = 1;

    while i < args.len() {
        if args[i] == "-w" {
            i += 1;
            if i >= args.len() {
                eprintln!("fmt: option requires an argument -- 'w'");
                process::exit(1);
            }
            width = match args[i].parse() {
                Ok(n) => n,
                Err(_) => {
                    eprintln!("fmt: invalid width: {}", args[i]);
                    process::exit(1);
                }
            };
        } else if args[i].starts_with("-w") {
            let val = &args[i][2..];
            width = match val.parse() {
                Ok(n) => n,
                Err(_) => {
                    eprintln!("fmt: invalid width: {}", val);
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
        fmt_reader(io::stdin().lock(), width);
    } else {
        for path in &files {
            match File::open(path) {
                Ok(f) => fmt_reader(f, width),
                Err(e) => {
                    eprintln!("fmt: {}: {}", path, e);
                    process::exit(1);
                }
            }
        }
    }
}
