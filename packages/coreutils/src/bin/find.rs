//! find - search for files in a directory hierarchy

use std::env;
use std::fs;
use std::path::Path;
use std::process;

struct Options {
    name_pattern: Option<String>,
    file_type: Option<char>, // 'f' for file, 'd' for directory
    max_depth: Option<usize>,
}

/// Simple glob matching supporting * and ? wildcards.
fn glob_match(pattern: &str, text: &str) -> bool {
    let p: Vec<char> = pattern.chars().collect();
    let t: Vec<char> = text.chars().collect();
    glob_match_inner(&p, &t)
}

fn glob_match_inner(pattern: &[char], text: &[char]) -> bool {
    if pattern.is_empty() {
        return text.is_empty();
    }
    if pattern[0] == '*' {
        // * matches zero or more characters
        for i in 0..=text.len() {
            if glob_match_inner(&pattern[1..], &text[i..]) {
                return true;
            }
        }
        false
    } else if text.is_empty() {
        false
    } else if pattern[0] == '?' || pattern[0] == text[0] {
        glob_match_inner(&pattern[1..], &text[1..])
    } else {
        false
    }
}

fn should_print(path: &Path, opts: &Options) -> bool {
    // Check -type filter
    if let Some(t) = opts.file_type {
        match t {
            'f' => {
                if !path.is_file() {
                    return false;
                }
            }
            'd' => {
                if !path.is_dir() {
                    return false;
                }
            }
            _ => {}
        }
    }

    // Check -name filter
    if let Some(ref pattern) = opts.name_pattern {
        if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
            if !glob_match(pattern, name) {
                return false;
            }
        } else {
            return false;
        }
    }

    true
}

fn walk(dir: &Path, opts: &Options, depth: usize) {
    if let Some(max) = opts.max_depth {
        if depth > max {
            return;
        }
    }

    if should_print(dir, opts) {
        println!("{}", dir.display());
    }

    if dir.is_dir() {
        if let Some(max) = opts.max_depth {
            if depth >= max {
                return;
            }
        }

        let mut entries: Vec<_> = match fs::read_dir(dir) {
            Ok(rd) => rd.filter_map(|e| e.ok()).collect(),
            Err(e) => {
                eprintln!("find: '{}': {}", dir.display(), e);
                return;
            }
        };
        entries.sort_by_key(|e| e.file_name());

        for entry in entries {
            let child = entry.path();
            if child.is_dir() {
                walk(&child, opts, depth + 1);
            } else if should_print(&child, opts) {
                println!("{}", child.display());
            }
        }
    }
}

fn main() {
    let args: Vec<String> = env::args().collect();
    let mut opts = Options {
        name_pattern: None,
        file_type: None,
        max_depth: None,
    };
    let mut paths: Vec<String> = Vec::new();

    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "-name" => {
                i += 1;
                if i >= args.len() {
                    eprintln!("find: missing argument to '-name'");
                    process::exit(1);
                }
                opts.name_pattern = Some(args[i].clone());
            }
            "-type" => {
                i += 1;
                if i >= args.len() {
                    eprintln!("find: missing argument to '-type'");
                    process::exit(1);
                }
                opts.file_type = args[i].chars().next();
            }
            "-maxdepth" => {
                i += 1;
                if i >= args.len() {
                    eprintln!("find: missing argument to '-maxdepth'");
                    process::exit(1);
                }
                opts.max_depth = args[i].parse().ok();
            }
            arg => {
                if !arg.starts_with('-') {
                    paths.push(arg.to_string());
                } else {
                    eprintln!("find: unknown predicate '{}'", arg);
                    process::exit(1);
                }
            }
        }
        i += 1;
    }

    if paths.is_empty() {
        paths.push(".".to_string());
    }

    for path in &paths {
        walk(Path::new(path), &opts, 0);
    }
}
