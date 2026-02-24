//! patch - apply a unified diff to files

use std::env;
use std::fs;
use std::io::{self, BufRead, BufReader, Read};
use std::path::PathBuf;
use std::process;

struct Options {
    strip: usize,
    input_file: Option<String>,
}

struct Hunk {
    old_start: usize,
    new_lines: Vec<HunkLine>,
}

enum HunkLine {
    Context(String),
    Add(String),
    Remove(()),
}

fn strip_path(path: &str, strip: usize) -> String {
    if strip == 0 {
        return path.to_string();
    }
    let mut components: Vec<&str> = path.split('/').collect();
    if components.len() > strip {
        components.drain(..strip);
    }
    components.join("/")
}

fn parse_hunk_header(line: &str) -> Option<(usize, usize)> {
    // @@ -old_start,old_count +new_start,new_count @@
    let line = line.strip_prefix("@@ ")?;
    let end = line.find(" @@")?;
    let ranges = &line[..end];
    let mut parts = ranges.split(' ');
    let old_part = parts.next()?;
    let old_part = old_part.strip_prefix('-')?;
    let old_start: usize = if let Some(comma) = old_part.find(',') {
        old_part[..comma].parse().ok()?
    } else {
        old_part.parse().ok()?
    };
    Some((old_start, 0))
}

fn apply_hunks(original: &str, hunks: &[Hunk]) -> String {
    let old_lines: Vec<&str> = original.lines().collect();
    let mut result: Vec<String> = Vec::new();
    let mut old_idx = 0;

    for hunk in hunks {
        // Copy lines before this hunk
        let hunk_start = if hunk.old_start > 0 {
            hunk.old_start - 1
        } else {
            0
        };
        while old_idx < hunk_start && old_idx < old_lines.len() {
            result.push(old_lines[old_idx].to_string());
            old_idx += 1;
        }

        for hline in &hunk.new_lines {
            match hline {
                HunkLine::Context(s) => {
                    result.push(s.clone());
                    old_idx += 1;
                }
                HunkLine::Add(s) => {
                    result.push(s.clone());
                }
                HunkLine::Remove(_) => {
                    old_idx += 1;
                }
            }
        }
    }

    // Copy remaining lines
    while old_idx < old_lines.len() {
        result.push(old_lines[old_idx].to_string());
        old_idx += 1;
    }

    let mut out = result.join("\n");
    if !out.is_empty()
        && (original.ends_with('\n')
            || hunks.iter().any(|h| {
                h.new_lines
                    .last()
                    .map(|l| matches!(l, HunkLine::Add(_)))
                    .unwrap_or(false)
            }))
    {
        out.push('\n');
    }
    out
}

fn read_input(input_file: &Option<String>) -> Vec<String> {
    let reader: Box<dyn Read> = match input_file {
        Some(path) => match fs::File::open(path) {
            Ok(f) => Box::new(f),
            Err(e) => {
                eprintln!("patch: {path}: {e}");
                process::exit(1);
            }
        },
        None => Box::new(io::stdin()),
    };

    let buf = BufReader::new(reader);
    buf.lines().map(|l| l.unwrap_or_default()).collect()
}

fn main() {
    let args: Vec<String> = env::args().collect();

    if args.iter().any(|a| a == "--help") {
        println!("Usage: patch [OPTIONS] [FILE]");
        println!("Apply a unified diff from stdin.");
        println!("  -p N  Strip N leading path components");
        return;
    }

    let mut opts = Options {
        strip: 0,
        input_file: None,
    };
    let mut target_file: Option<String> = None;
    let mut i = 1;

    while i < args.len() {
        if args[i] == "-p" {
            i += 1;
            if i >= args.len() {
                eprintln!("patch: option requires an argument -- 'p'");
                process::exit(1);
            }
            opts.strip = match args[i].parse() {
                Ok(n) => n,
                Err(_) => {
                    eprintln!("patch: invalid strip count: {}", args[i]);
                    process::exit(1);
                }
            };
        } else if let Some(rest) = args[i].strip_prefix("-p") {
            opts.strip = match rest.parse() {
                Ok(n) => n,
                Err(_) => {
                    eprintln!("patch: invalid strip count: {rest}");
                    process::exit(1);
                }
            };
        } else if args[i] == "-i" {
            i += 1;
            if i >= args.len() {
                eprintln!("patch: option requires an argument -- 'i'");
                process::exit(1);
            }
            opts.input_file = Some(args[i].clone());
        } else if !args[i].starts_with('-') {
            target_file = Some(args[i].clone());
        }
        i += 1;
    }

    let diff_lines = read_input(&opts.input_file);

    // Parse diff into per-file patches
    let mut line_idx = 0;

    while line_idx < diff_lines.len() {
        // Find next file header
        let mut file_path: Option<String> = None;

        while line_idx < diff_lines.len() {
            if diff_lines[line_idx].starts_with("+++ ") {
                let path_str = diff_lines[line_idx][4..].trim();
                // Remove timestamp if present
                let path_str = if let Some(tab_pos) = path_str.find('\t') {
                    &path_str[..tab_pos]
                } else {
                    path_str
                };
                file_path = Some(strip_path(path_str, opts.strip));
                line_idx += 1;
                break;
            }
            line_idx += 1;
        }

        let file_path = match file_path {
            Some(p) => p,
            None => break,
        };

        // Use the target_file override if provided
        let actual_path = target_file.as_deref().unwrap_or(&file_path);

        // Parse hunks
        let mut hunks: Vec<Hunk> = Vec::new();

        while line_idx < diff_lines.len() {
            if diff_lines[line_idx].starts_with("--- ") || diff_lines[line_idx].starts_with("diff ")
            {
                break;
            }

            if diff_lines[line_idx].starts_with("@@ ") {
                let (old_start, _) = match parse_hunk_header(&diff_lines[line_idx]) {
                    Some(h) => h,
                    None => {
                        eprintln!("patch: malformed hunk header: {}", diff_lines[line_idx]);
                        process::exit(1);
                    }
                };
                line_idx += 1;

                let mut hunk_lines: Vec<HunkLine> = Vec::new();

                while line_idx < diff_lines.len() {
                    let line = &diff_lines[line_idx];
                    if line.starts_with("@@ ")
                        || line.starts_with("--- ")
                        || line.starts_with("+++ ")
                        || line.starts_with("diff ")
                    {
                        break;
                    }

                    if let Some(rest) = line.strip_prefix('+') {
                        hunk_lines.push(HunkLine::Add(rest.to_string()));
                    } else if let Some(rest) = line.strip_prefix('-') {
                        let _ = rest; // consume but discard
                        hunk_lines.push(HunkLine::Remove(()));
                    } else if let Some(rest) = line.strip_prefix(' ') {
                        hunk_lines.push(HunkLine::Context(rest.to_string()));
                    } else if line.is_empty() {
                        hunk_lines.push(HunkLine::Context(String::new()));
                    } else {
                        // Treat as context
                        hunk_lines.push(HunkLine::Context(line.to_string()));
                    }

                    line_idx += 1;
                }

                hunks.push(Hunk {
                    old_start,
                    new_lines: hunk_lines,
                });
            } else {
                line_idx += 1;
            }
        }

        if hunks.is_empty() {
            continue;
        }

        eprintln!("patching file {actual_path}");

        // Read original file (may not exist)
        let original = fs::read_to_string(actual_path).unwrap_or_default();

        let result = apply_hunks(&original, &hunks);

        // Create parent directories if needed
        let path = PathBuf::from(actual_path);
        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() {
                let _ = fs::create_dir_all(parent);
            }
        }

        if let Err(e) = fs::write(actual_path, &result) {
            eprintln!("patch: {actual_path}: {e}");
            process::exit(1);
        }
    }
}
