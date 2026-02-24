//! cmp - compare two files byte by byte

use std::env;
use std::fs;
use std::io::{self, Write};
use std::process;

fn main() {
    let args: Vec<String> = env::args().collect();

    if args.iter().any(|a| a == "--help") {
        println!("Usage: cmp [OPTIONS] FILE1 FILE2");
        println!("Compare two files byte by byte.");
        println!("  -l  Print all differing bytes");
        println!("  -s  Silent, exit code only");
        return;
    }

    let mut verbose = false;
    let mut silent = false;
    let mut files: Vec<String> = Vec::new();

    for arg in args.iter().skip(1) {
        if arg == "-l" {
            verbose = true;
        } else if arg == "-s" {
            silent = true;
        } else if arg == "--" {
            continue;
        } else if arg.starts_with('-') && arg.len() > 1 {
            // Combined flags
            for ch in arg[1..].chars() {
                match ch {
                    'l' => verbose = true,
                    's' => silent = true,
                    _ => {
                        eprintln!("cmp: unknown option: -{ch}");
                        process::exit(2);
                    }
                }
            }
        } else {
            files.push(arg.clone());
        }
    }

    if files.len() != 2 {
        eprintln!("cmp: usage: cmp [OPTIONS] FILE1 FILE2");
        process::exit(2);
    }

    let data1 = match fs::read(&files[0]) {
        Ok(d) => d,
        Err(e) => {
            if !silent {
                eprintln!("cmp: {}: {e}", files[0]);
            }
            process::exit(2);
        }
    };

    let data2 = match fs::read(&files[1]) {
        Ok(d) => d,
        Err(e) => {
            if !silent {
                eprintln!("cmp: {}: {e}", files[1]);
            }
            process::exit(2);
        }
    };

    let stdout = io::stdout();
    let mut out = stdout.lock();
    let min_len = data1.len().min(data2.len());
    let mut found_diff = false;
    let mut line: usize = 1;

    for i in 0..min_len {
        if data1[i] == b'\n' {
            line += 1;
        }
        if data1[i] != data2[i] {
            if !found_diff {
                found_diff = true;
                if !verbose && !silent {
                    let _ = writeln!(
                        out,
                        "{} {} differ: byte {}, line {line}",
                        files[0],
                        files[1],
                        i + 1
                    );
                    process::exit(1);
                }
            }
            if verbose && !silent {
                let _ = writeln!(out, "{:>6} {:>3o} {:>3o}", i + 1, data1[i], data2[i]);
            }
            if silent && !verbose {
                process::exit(1);
            }
        }
    }

    if data1.len() != data2.len() {
        if !silent {
            let shorter = if data1.len() < data2.len() {
                &files[0]
            } else {
                &files[1]
            };
            eprintln!("cmp: EOF on {shorter}");
        }
        process::exit(1);
    }

    if found_diff {
        process::exit(1);
    }
}
