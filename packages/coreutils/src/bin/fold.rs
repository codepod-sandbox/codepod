//! fold - wrap each input line to fit in specified width

use std::env;
use std::fs::File;
use std::io::{self, BufRead, BufReader, Read};
use std::process;

fn fold_reader<R: Read>(reader: R, width: usize, break_spaces: bool) {
    let buf = BufReader::new(reader);
    for line in buf.lines() {
        let line = match line {
            Ok(l) => l,
            Err(e) => {
                eprintln!("fold: {}", e);
                process::exit(1);
            }
        };

        if line.len() <= width {
            println!("{}", line);
            continue;
        }

        if break_spaces {
            fold_line_spaces(&line, width);
        } else {
            fold_line_hard(&line, width);
        }
    }
}

fn fold_line_hard(line: &str, width: usize) {
    let chars: Vec<char> = line.chars().collect();
    let mut pos = 0;
    while pos < chars.len() {
        let end = (pos + width).min(chars.len());
        let segment: String = chars[pos..end].iter().collect();
        println!("{}", segment);
        pos = end;
    }
}

fn fold_line_spaces(line: &str, width: usize) {
    let chars: Vec<char> = line.chars().collect();
    let mut pos = 0;

    while pos < chars.len() {
        if pos + width >= chars.len() {
            let segment: String = chars[pos..].iter().collect();
            println!("{}", segment);
            break;
        }

        // Look for the last space within the width
        let end = pos + width;
        let mut break_at = end;
        let mut found_space = false;
        for j in (pos..end).rev() {
            if chars[j] == ' ' {
                break_at = j + 1;
                found_space = true;
                break;
            }
        }

        if !found_space {
            // No space found, hard break
            break_at = end;
        }

        let segment: String = chars[pos..break_at].iter().collect();
        println!("{}", segment.trim_end());
        pos = break_at;
        // Skip leading spaces after a break at space
        if found_space {
            while pos < chars.len() && chars[pos] == ' ' {
                pos += 1;
            }
        }
    }
}

fn main() {
    let args: Vec<String> = env::args().collect();

    if args.iter().any(|a| a == "--help") {
        println!("Usage: fold [-w WIDTH] [-s] [FILE...]");
        println!("Wrap each input line to fit in specified width.");
        println!("  -w WIDTH  use WIDTH columns (default 80)");
        println!("  -s        break at spaces");
        return;
    }

    let mut width: usize = 80;
    let mut break_spaces = false;
    let mut files: Vec<String> = Vec::new();
    let mut i = 1;

    while i < args.len() {
        if args[i] == "-w" {
            i += 1;
            if i >= args.len() {
                eprintln!("fold: option requires an argument -- 'w'");
                process::exit(1);
            }
            width = match args[i].parse() {
                Ok(n) => n,
                Err(_) => {
                    eprintln!("fold: invalid width: {}", args[i]);
                    process::exit(1);
                }
            };
        } else if args[i].starts_with("-w") {
            let val = &args[i][2..];
            width = match val.parse() {
                Ok(n) => n,
                Err(_) => {
                    eprintln!("fold: invalid width: {}", val);
                    process::exit(1);
                }
            };
        } else if args[i] == "-s" {
            break_spaces = true;
        } else if args[i].starts_with('-') && args[i].len() > 1 && args[i] != "--" {
            // Handle combined flags like -sw20
            let flags = &args[i][1..];
            let mut j = 0;
            let chars: Vec<char> = flags.chars().collect();
            while j < chars.len() {
                match chars[j] {
                    's' => break_spaces = true,
                    'w' => {
                        let rest: String = chars[j + 1..].iter().collect();
                        if rest.is_empty() {
                            i += 1;
                            if i >= args.len() {
                                eprintln!("fold: option requires an argument -- 'w'");
                                process::exit(1);
                            }
                            width = match args[i].parse() {
                                Ok(n) => n,
                                Err(_) => {
                                    eprintln!("fold: invalid width: {}", args[i]);
                                    process::exit(1);
                                }
                            };
                        } else {
                            width = match rest.parse() {
                                Ok(n) => n,
                                Err(_) => {
                                    eprintln!("fold: invalid width: {}", rest);
                                    process::exit(1);
                                }
                            };
                        }
                        break;
                    }
                    _ => {
                        eprintln!("fold: invalid option -- '{}'", chars[j]);
                        process::exit(1);
                    }
                }
                j += 1;
            }
        } else if args[i] == "--" {
            files.extend_from_slice(&args[i + 1..]);
            break;
        } else {
            files.push(args[i].clone());
        }
        i += 1;
    }

    if files.is_empty() {
        fold_reader(io::stdin().lock(), width, break_spaces);
    } else {
        for path in &files {
            match File::open(path) {
                Ok(f) => fold_reader(f, width, break_spaces),
                Err(e) => {
                    eprintln!("fold: {}: {}", path, e);
                    process::exit(1);
                }
            }
        }
    }
}
