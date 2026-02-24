//! nl - number lines of files

use std::env;
use std::fs::File;
use std::io::{self, BufRead, BufReader, Read};
use std::process;

#[derive(PartialEq)]
enum BodyNumbering {
    All,
    NonEmpty,
}

fn nl_reader<R: Read>(reader: R, numbering: &BodyNumbering, line_number: &mut usize) {
    let buf = BufReader::new(reader);
    for line in buf.lines() {
        let line = match line {
            Ok(l) => l,
            Err(e) => {
                eprintln!("nl: {}", e);
                process::exit(1);
            }
        };

        let should_number = match numbering {
            BodyNumbering::All => true,
            BodyNumbering::NonEmpty => !line.is_empty(),
        };

        if should_number {
            println!("{:>6}\t{}", line_number, line);
            *line_number += 1;
        } else {
            println!("      \t{}", line);
        }
    }
}

fn main() {
    let args: Vec<String> = env::args().collect();

    if args.iter().any(|a| a == "--help") {
        println!("Usage: nl [-b STYLE] [FILE...]");
        println!("Number lines of files.");
        println!("  -b a    number all lines (default)");
        println!("  -b t    number only non-empty lines");
        return;
    }

    let mut numbering = BodyNumbering::All;
    let mut files: Vec<String> = Vec::new();
    let mut i = 1;

    while i < args.len() {
        if args[i] == "-b" {
            i += 1;
            if i >= args.len() {
                eprintln!("nl: option requires an argument -- 'b'");
                process::exit(1);
            }
            match args[i].as_str() {
                "a" => numbering = BodyNumbering::All,
                "t" => numbering = BodyNumbering::NonEmpty,
                other => {
                    eprintln!("nl: invalid body numbering style: {}", other);
                    process::exit(1);
                }
            }
        } else if args[i] == "--" {
            files.extend_from_slice(&args[i + 1..]);
            break;
        } else if args[i].starts_with('-') && args[i].len() > 1 {
            eprintln!("nl: invalid option -- '{}'", &args[i][1..]);
            process::exit(1);
        } else {
            files.push(args[i].clone());
        }
        i += 1;
    }

    let mut line_number: usize = 1;

    if files.is_empty() {
        nl_reader(io::stdin().lock(), &numbering, &mut line_number);
    } else {
        for path in &files {
            match File::open(path) {
                Ok(f) => nl_reader(f, &numbering, &mut line_number),
                Err(e) => {
                    eprintln!("nl: {}: {}", path, e);
                    process::exit(1);
                }
            }
        }
    }
}
