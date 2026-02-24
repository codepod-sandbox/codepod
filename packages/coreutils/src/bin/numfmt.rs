//! numfmt - convert numbers from/to human-readable strings

use std::env;
use std::io::{self, BufRead, Write};
use std::process;

#[derive(Clone, PartialEq)]
enum Scale {
    None,
    Iec, // 1024-based
    Si,  // 1000-based
}

struct Options {
    from: Scale,
    to: Scale,
}

const IEC_SUFFIXES: &[char] = &['K', 'M', 'G', 'T', 'P', 'E'];
const SI_SUFFIXES: &[char] = &['K', 'M', 'G', 'T', 'P', 'E'];

fn parse_human(s: &str, scale: &Scale) -> Result<f64, String> {
    let s = s.trim();
    if s.is_empty() {
        return Err("empty string".to_string());
    }

    let base: f64 = match scale {
        Scale::Iec => 1024.0,
        Scale::Si => 1000.0,
        Scale::None => return s.parse::<f64>().map_err(|e| e.to_string()),
    };

    let suffixes = match scale {
        Scale::Iec => IEC_SUFFIXES,
        Scale::Si => SI_SUFFIXES,
        Scale::None => unreachable!(),
    };

    // Check for suffix
    let last = s.chars().last().unwrap();
    if last.is_ascii_alphabetic() {
        let upper = last.to_ascii_uppercase();
        if let Some(pos) = suffixes.iter().position(|&c| c == upper) {
            let num_part = &s[..s.len() - 1];
            let num: f64 = num_part
                .parse()
                .map_err(|e: std::num::ParseFloatError| e.to_string())?;
            let multiplier = base.powi((pos + 1) as i32);
            return Ok(num * multiplier);
        }
        return Err(format!("invalid suffix: {last}"));
    }

    s.parse::<f64>().map_err(|e| e.to_string())
}

fn format_human(val: f64, scale: &Scale) -> String {
    if *scale == Scale::None {
        return format!("{val}");
    }

    let base: f64 = match scale {
        Scale::Iec => 1024.0,
        Scale::Si => 1000.0,
        Scale::None => unreachable!(),
    };

    let suffixes = match scale {
        Scale::Iec => IEC_SUFFIXES,
        Scale::Si => SI_SUFFIXES,
        Scale::None => unreachable!(),
    };

    let abs_val = val.abs();
    if abs_val < base {
        // No suffix needed
        if val == val.floor() {
            return format!("{}", val as i64);
        }
        return format!("{val}");
    }

    for (i, &suffix) in suffixes.iter().enumerate() {
        let threshold = base.powi((i + 2) as i32);
        if abs_val < threshold || i == suffixes.len() - 1 {
            let scaled = val / base.powi((i + 1) as i32);
            return format!("{scaled:.1}{suffix}");
        }
    }

    format!("{val}")
}

fn main() {
    let args: Vec<String> = env::args().collect();

    if args.iter().any(|a| a == "--help") {
        println!("Usage: numfmt [OPTIONS] [NUMBER...]");
        println!("Reformat numbers.");
        println!("  --from=iec  Parse IEC (1024-based) suffixes");
        println!("  --from=si   Parse SI (1000-based) suffixes");
        println!("  --to=iec    Format as IEC (1024-based) suffixes");
        println!("  --to=si     Format as SI (1000-based) suffixes");
        return;
    }

    let mut opts = Options {
        from: Scale::None,
        to: Scale::None,
    };
    let mut numbers: Vec<String> = Vec::new();

    for arg in args.iter().skip(1) {
        if let Some(val) = arg.strip_prefix("--from=") {
            opts.from = match val {
                "iec" => Scale::Iec,
                "si" => Scale::Si,
                "none" => Scale::None,
                _ => {
                    eprintln!("numfmt: invalid --from value: {val}");
                    process::exit(1);
                }
            };
        } else if let Some(val) = arg.strip_prefix("--to=") {
            opts.to = match val {
                "iec" => Scale::Iec,
                "si" => Scale::Si,
                "none" => Scale::None,
                _ => {
                    eprintln!("numfmt: invalid --to value: {val}");
                    process::exit(1);
                }
            };
        } else if !arg.starts_with('-') {
            numbers.push(arg.clone());
        }
    }

    let stdout = io::stdout();
    let mut out = stdout.lock();

    if numbers.is_empty() {
        // Read from stdin
        let stdin = io::stdin();
        for line in stdin.lock().lines() {
            let line = match line {
                Ok(l) => l,
                Err(e) => {
                    eprintln!("numfmt: {e}");
                    process::exit(1);
                }
            };
            for word in line.split_whitespace() {
                let val = match parse_human(word, &opts.from) {
                    Ok(v) => v,
                    Err(e) => {
                        eprintln!("numfmt: invalid number: {word}: {e}");
                        process::exit(1);
                    }
                };
                let _ = writeln!(out, "{}", format_human(val, &opts.to));
            }
        }
    } else {
        for num in &numbers {
            let val = match parse_human(num, &opts.from) {
                Ok(v) => v,
                Err(e) => {
                    eprintln!("numfmt: invalid number: {num}: {e}");
                    process::exit(1);
                }
            };
            let _ = writeln!(out, "{}", format_human(val, &opts.to));
        }
    }
}
