//! truncate - shrink or extend the size of a file

use std::env;
use std::fs;
use std::io::Write;
use std::process;

enum SizeSpec {
    Absolute(u64),
    Extend(u64),
    Shrink(u64),
}

fn parse_size(s: &str) -> Result<SizeSpec, String> {
    let (mode, rest) = if let Some(stripped) = s.strip_prefix('+') {
        ('+', stripped)
    } else if let Some(stripped) = s.strip_prefix('-') {
        ('-', stripped)
    } else {
        ('=', s)
    };

    // Parse the numeric part and optional suffix
    let (num_str, multiplier) = if rest.ends_with('G') || rest.ends_with('g') {
        (&rest[..rest.len() - 1], 1024u64 * 1024 * 1024)
    } else if rest.ends_with('M') || rest.ends_with('m') {
        (&rest[..rest.len() - 1], 1024u64 * 1024)
    } else if rest.ends_with('K') || rest.ends_with('k') {
        (&rest[..rest.len() - 1], 1024u64)
    } else {
        (rest, 1u64)
    };

    let num: u64 = num_str
        .parse()
        .map_err(|_| format!("invalid size: {}", s))?;

    let bytes = num
        .checked_mul(multiplier)
        .ok_or_else(|| format!("size too large: {}", s))?;

    match mode {
        '+' => Ok(SizeSpec::Extend(bytes)),
        '-' => Ok(SizeSpec::Shrink(bytes)),
        _ => Ok(SizeSpec::Absolute(bytes)),
    }
}

fn main() {
    let args: Vec<String> = env::args().collect();

    if args.iter().any(|a| a == "--help") {
        println!("Usage: truncate -s SIZE FILE...");
        println!("Shrink or extend the size of each FILE.");
        println!("  -s SIZE  set or adjust size; SIZE may have suffix K, M, G");
        println!("           prefix + to extend, - to shrink relative to current size");
        return;
    }

    let mut size_spec: Option<SizeSpec> = None;
    let mut files: Vec<String> = Vec::new();
    let mut i = 1;

    while i < args.len() {
        if args[i] == "-s" {
            i += 1;
            if i >= args.len() {
                eprintln!("truncate: option requires an argument -- 's'");
                process::exit(1);
            }
            size_spec = match parse_size(&args[i]) {
                Ok(s) => Some(s),
                Err(e) => {
                    eprintln!("truncate: {}", e);
                    process::exit(1);
                }
            };
        } else if args[i].starts_with("-s") {
            let val = &args[i][2..];
            size_spec = match parse_size(val) {
                Ok(s) => Some(s),
                Err(e) => {
                    eprintln!("truncate: {}", e);
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

    let size_spec = match size_spec {
        Some(s) => s,
        None => {
            eprintln!("truncate: you must specify '-s SIZE'");
            process::exit(1);
        }
    };

    if files.is_empty() {
        eprintln!("truncate: missing file operand");
        process::exit(1);
    }

    for path in &files {
        // Read existing content (or empty if file doesn't exist)
        let data = match fs::read(path) {
            Ok(d) => d,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Vec::new(),
            Err(e) => {
                eprintln!("truncate: {}: {}", path, e);
                process::exit(1);
            }
        };

        let current_size = data.len() as u64;
        let new_size = match &size_spec {
            SizeSpec::Absolute(n) => *n,
            SizeSpec::Extend(n) => current_size.saturating_add(*n),
            SizeSpec::Shrink(n) => current_size.saturating_sub(*n),
        } as usize;

        // Build new content
        let mut new_data = Vec::with_capacity(new_size);
        if new_size <= data.len() {
            new_data.extend_from_slice(&data[..new_size]);
        } else {
            new_data.extend_from_slice(&data);
            new_data.resize(new_size, 0);
        }

        // Write back
        let mut f = match fs::File::create(path) {
            Ok(f) => f,
            Err(e) => {
                eprintln!("truncate: {}: {}", path, e);
                process::exit(1);
            }
        };

        if let Err(e) = f.write_all(&new_data) {
            eprintln!("truncate: {}: {}", path, e);
            process::exit(1);
        }
    }
}
