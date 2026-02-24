//! join - join lines of two files on a common field

use std::env;
use std::fs::File;
use std::io::{self, BufRead, BufReader};
use std::process;

fn get_field(line: &str, field: usize, sep: &Option<char>) -> String {
    match sep {
        Some(c) => {
            let fields: Vec<&str> = line.split(*c).collect();
            if field <= fields.len() {
                fields[field - 1].to_string()
            } else {
                String::new()
            }
        }
        None => {
            let fields: Vec<&str> = line.split_whitespace().collect();
            if field <= fields.len() {
                fields[field - 1].to_string()
            } else {
                String::new()
            }
        }
    }
}

fn get_other_fields(line: &str, field: usize, sep: &Option<char>) -> Vec<String> {
    let fields: Vec<&str> = match sep {
        Some(c) => line.split(*c).collect(),
        None => line.split_whitespace().collect(),
    };
    let mut result = Vec::new();
    for (i, f) in fields.iter().enumerate() {
        if i + 1 != field {
            result.push(f.to_string());
        }
    }
    result
}

fn main() {
    let args: Vec<String> = env::args().collect();

    if args.iter().any(|a| a == "--help") {
        println!("Usage: join [-t CHAR] [-1 FIELD] [-2 FIELD] FILE1 FILE2");
        println!("Join lines of two sorted files on a common field.");
        println!("  -t CHAR   use CHAR as field separator");
        println!("  -1 FIELD  join on FIELD of file 1 (default 1)");
        println!("  -2 FIELD  join on FIELD of file 2 (default 1)");
        return;
    }

    let mut separator: Option<char> = None;
    let mut field1: usize = 1;
    let mut field2: usize = 1;
    let mut files: Vec<String> = Vec::new();
    let mut i = 1;

    while i < args.len() {
        if args[i] == "-t" {
            i += 1;
            if i >= args.len() {
                eprintln!("join: option requires an argument -- 't'");
                process::exit(1);
            }
            let chars: Vec<char> = args[i].chars().collect();
            if chars.is_empty() {
                eprintln!("join: separator must be a single character");
                process::exit(1);
            }
            separator = Some(chars[0]);
        } else if args[i] == "-1" {
            i += 1;
            if i >= args.len() {
                eprintln!("join: option requires an argument -- '1'");
                process::exit(1);
            }
            field1 = match args[i].parse() {
                Ok(n) if n > 0 => n,
                _ => {
                    eprintln!("join: invalid field number: {}", args[i]);
                    process::exit(1);
                }
            };
        } else if args[i] == "-2" {
            i += 1;
            if i >= args.len() {
                eprintln!("join: option requires an argument -- '2'");
                process::exit(1);
            }
            field2 = match args[i].parse() {
                Ok(n) if n > 0 => n,
                _ => {
                    eprintln!("join: invalid field number: {}", args[i]);
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

    if files.len() != 2 {
        eprintln!("join: requires exactly two files");
        process::exit(1);
    }

    let reader1: Box<dyn BufRead> = if files[0] == "-" {
        Box::new(BufReader::new(io::stdin().lock()))
    } else {
        match File::open(&files[0]) {
            Ok(f) => Box::new(BufReader::new(f)),
            Err(e) => {
                eprintln!("join: {}: {}", files[0], e);
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
                eprintln!("join: {}: {}", files[1], e);
                process::exit(1);
            }
        }
    };

    let lines1: Vec<String> = reader1.lines().map(|l| l.unwrap_or_default()).collect();
    let lines2: Vec<String> = reader2.lines().map(|l| l.unwrap_or_default()).collect();

    let out_sep = match separator {
        Some(c) => c.to_string(),
        None => " ".to_string(),
    };

    let mut j = 0;
    for line1 in &lines1 {
        let key1 = get_field(line1, field1, &separator);
        // Since files are sorted, advance j to first match or past
        while j < lines2.len() && get_field(&lines2[j], field2, &separator) < key1 {
            j += 1;
        }
        // Output all matching lines from file2
        let mut k = j;
        while k < lines2.len() {
            let key2 = get_field(&lines2[k], field2, &separator);
            if key2 != key1 {
                break;
            }
            let other1 = get_other_fields(line1, field1, &separator);
            let other2 = get_other_fields(&lines2[k], field2, &separator);
            let mut parts: Vec<String> = vec![key1.clone()];
            parts.extend(other1);
            parts.extend(other2);
            println!("{}", parts.join(&out_sep));
            k += 1;
        }
    }
}
