//! tr - translate or delete characters

use std::env;
use std::io::{self, Read, Write};
use std::process;

/// Expand a character set specification, handling ranges like a-z.
fn expand_set(spec: &str) -> Vec<char> {
    let chars: Vec<char> = spec.chars().collect();
    let mut result = Vec::new();
    let mut i = 0;

    while i < chars.len() {
        if i + 2 < chars.len() && chars[i + 1] == '-' {
            let start = chars[i] as u32;
            let end = chars[i + 2] as u32;
            if start <= end {
                for code in start..=end {
                    if let Some(c) = char::from_u32(code) {
                        result.push(c);
                    }
                }
            } else {
                // Invalid range, treat literally
                result.push(chars[i]);
                result.push('-');
                result.push(chars[i + 2]);
            }
            i += 3;
        } else {
            result.push(chars[i]);
            i += 1;
        }
    }

    result
}

fn main() {
    let args: Vec<String> = env::args().collect();

    let mut delete = false;
    let mut positional: Vec<String> = Vec::new();

    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "-d" => delete = true,
            _ => positional.push(args[i].clone()),
        }
        i += 1;
    }

    if delete {
        if positional.is_empty() {
            eprintln!("tr: missing operand");
            process::exit(1);
        }
        let set1 = expand_set(&positional[0]);

        let mut input = String::new();
        if let Err(e) = io::stdin().read_to_string(&mut input) {
            eprintln!("tr: read error: {}", e);
            process::exit(1);
        }

        let stdout = io::stdout();
        let mut out = stdout.lock();
        for c in input.chars() {
            if !set1.contains(&c) {
                if let Err(e) = write!(out, "{}", c) {
                    eprintln!("tr: write error: {}", e);
                    process::exit(1);
                }
            }
        }
    } else {
        if positional.len() < 2 {
            eprintln!("tr: missing operand");
            if positional.is_empty() {
                eprintln!("Usage: tr [-d] SET1 [SET2]");
            }
            process::exit(1);
        }
        let set1 = expand_set(&positional[0]);
        let set2 = expand_set(&positional[1]);

        let mut input = String::new();
        if let Err(e) = io::stdin().read_to_string(&mut input) {
            eprintln!("tr: read error: {}", e);
            process::exit(1);
        }

        let stdout = io::stdout();
        let mut out = stdout.lock();
        for c in input.chars() {
            let replacement = if let Some(pos) = set1.iter().position(|&s| s == c) {
                // If set2 is shorter, use the last char of set2
                if pos < set2.len() {
                    set2[pos]
                } else if !set2.is_empty() {
                    set2[set2.len() - 1]
                } else {
                    c
                }
            } else {
                c
            };
            if let Err(e) = write!(out, "{}", replacement) {
                eprintln!("tr: write error: {}", e);
                process::exit(1);
            }
        }
    }
}
