//! tree - list directory contents in a tree-like format

use std::env;
use std::fs;
use std::io::{self, Write};
use std::path::Path;
use std::process;

struct Options {
    max_depth: Option<usize>,
    dirs_only: bool,
}

struct Stats {
    dirs: usize,
    files: usize,
}

fn walk_tree(
    path: &Path,
    prefix: &str,
    depth: usize,
    opts: &Options,
    stats: &mut Stats,
    out: &mut dyn Write,
) {
    if let Some(max) = opts.max_depth {
        if depth >= max {
            return;
        }
    }

    let mut entries: Vec<_> = match fs::read_dir(path) {
        Ok(rd) => rd.filter_map(|e| e.ok()).collect(),
        Err(e) => {
            let _ = writeln!(out, "{prefix}[error opening dir: {e}]");
            return;
        }
    };

    entries.sort_by_key(|e| e.file_name());

    if opts.dirs_only {
        entries.retain(|e| e.file_type().map(|ft| ft.is_dir()).unwrap_or(false));
    }

    let count = entries.len();
    for (i, entry) in entries.iter().enumerate() {
        let is_last = i == count - 1;
        let connector = if is_last {
            "\u{2514}\u{2500}\u{2500}"
        } else {
            "\u{251c}\u{2500}\u{2500}"
        };
        let name = entry.file_name();
        let name_str = name.to_string_lossy();
        let is_dir = entry.file_type().map(|ft| ft.is_dir()).unwrap_or(false);

        let _ = writeln!(out, "{prefix}{connector} {name_str}");

        if is_dir {
            stats.dirs += 1;
            let child_prefix = if is_last {
                format!("{prefix}    ")
            } else {
                format!("{prefix}\u{2502}   ")
            };
            walk_tree(&entry.path(), &child_prefix, depth + 1, opts, stats, out);
        } else {
            stats.files += 1;
        }
    }
}

fn main() {
    let args: Vec<String> = env::args().collect();

    if args.iter().any(|a| a == "--help") {
        println!("Usage: tree [OPTIONS] [DIR]");
        println!("List directory contents in a tree-like format.");
        println!("  -L N  Descend only N levels deep");
        println!("  -d    List directories only");
        return;
    }

    let mut opts = Options {
        max_depth: None,
        dirs_only: false,
    };
    let mut dir = String::from(".");
    let mut i = 1;

    while i < args.len() {
        if args[i] == "-L" {
            i += 1;
            if i >= args.len() {
                eprintln!("tree: option requires an argument -- 'L'");
                process::exit(1);
            }
            opts.max_depth = match args[i].parse::<usize>() {
                Ok(n) if n > 0 => Some(n),
                _ => {
                    eprintln!("tree: invalid level: {}", args[i]);
                    process::exit(1);
                }
            };
        } else if args[i] == "-d" {
            opts.dirs_only = true;
        } else if !args[i].starts_with('-') {
            dir = args[i].clone();
        } else {
            eprintln!("tree: unknown option: {}", args[i]);
            process::exit(1);
        }
        i += 1;
    }

    let path = Path::new(&dir);
    if !path.is_dir() {
        eprintln!("tree: {dir}: not a directory");
        process::exit(1);
    }

    let stdout = io::stdout();
    let mut out = stdout.lock();
    let _ = writeln!(out, "{dir}");

    let mut stats = Stats { dirs: 0, files: 0 };
    walk_tree(path, "", 0, &opts, &mut stats, &mut out);

    if opts.dirs_only {
        let _ = writeln!(out, "\n{} directories", stats.dirs);
    } else {
        let _ = writeln!(out, "\n{} directories, {} files", stats.dirs, stats.files);
    }
}
