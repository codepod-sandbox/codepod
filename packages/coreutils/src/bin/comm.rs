//! comm - compare two sorted files line by line

use std::env;
use std::fs::File;
use std::io::{self, BufRead, BufReader};
use std::process;

fn main() {
    let args: Vec<String> = env::args().collect();

    if args.iter().any(|a| a == "--help") {
        println!("Usage: comm [-1] [-2] [-3] FILE1 FILE2");
        println!("Compare two sorted files line by line.");
        println!("  -1  suppress column 1 (lines unique to FILE1)");
        println!("  -2  suppress column 2 (lines unique to FILE2)");
        println!("  -3  suppress column 3 (lines common to both)");
        return;
    }

    let mut suppress1 = false;
    let mut suppress2 = false;
    let mut suppress3 = false;
    let mut files: Vec<String> = Vec::new();
    let mut i = 1;

    while i < args.len() {
        let arg = &args[i];
        if arg.starts_with('-') && arg.len() > 1 && arg != "--" {
            for ch in arg[1..].chars() {
                match ch {
                    '1' => suppress1 = true,
                    '2' => suppress2 = true,
                    '3' => suppress3 = true,
                    _ => {
                        eprintln!("comm: invalid option -- '{}'", ch);
                        process::exit(1);
                    }
                }
            }
        } else if arg == "--" {
            files.extend_from_slice(&args[i + 1..]);
            break;
        } else {
            files.push(arg.clone());
        }
        i += 1;
    }

    if files.len() != 2 {
        eprintln!("comm: requires exactly two files");
        process::exit(1);
    }

    let reader1: Box<dyn BufRead> = if files[0] == "-" {
        Box::new(BufReader::new(io::stdin().lock()))
    } else {
        match File::open(&files[0]) {
            Ok(f) => Box::new(BufReader::new(f)),
            Err(e) => {
                eprintln!("comm: {}: {}", files[0], e);
                process::exit(1);
            }
        }
    };

    let reader2: Box<dyn BufRead> = if files[1] == "-" {
        Box::new(BufReader::new(io::stdin().lock()))
    } else {
        match File::open(&files[1]) {
            Ok(f) => Box::new(BufReader::new(f)),
            Err(e) => {
                eprintln!("comm: {}: {}", files[1], e);
                process::exit(1);
            }
        }
    };

    let lines1: Vec<String> = reader1.lines().map(|l| l.unwrap_or_default()).collect();
    let lines2: Vec<String> = reader2.lines().map(|l| l.unwrap_or_default()).collect();

    let mut i1 = 0;
    let mut i2 = 0;

    // Build column prefixes
    let col2_prefix = if !suppress1 { "\t" } else { "" };
    let col3_prefix = if !suppress1 && !suppress2 {
        "\t\t"
    } else if !suppress1 || !suppress2 {
        "\t"
    } else {
        ""
    };

    while i1 < lines1.len() && i2 < lines2.len() {
        match lines1[i1].cmp(&lines2[i2]) {
            std::cmp::Ordering::Less => {
                if !suppress1 {
                    println!("{}", lines1[i1]);
                }
                i1 += 1;
            }
            std::cmp::Ordering::Greater => {
                if !suppress2 {
                    println!("{}{}", col2_prefix, lines2[i2]);
                }
                i2 += 1;
            }
            std::cmp::Ordering::Equal => {
                if !suppress3 {
                    println!("{}{}", col3_prefix, lines1[i1]);
                }
                i1 += 1;
                i2 += 1;
            }
        }
    }

    while i1 < lines1.len() {
        if !suppress1 {
            println!("{}", lines1[i1]);
        }
        i1 += 1;
    }

    while i2 < lines2.len() {
        if !suppress2 {
            println!("{}{}", col2_prefix, lines2[i2]);
        }
        i2 += 1;
    }
}
