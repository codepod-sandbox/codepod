//! column - columnate lists

use std::env;
use std::fs;
use std::io::{self, BufRead, BufReader, Read, Write};
use std::process;

struct Options {
    table: bool,
    separator: String,
    output_sep: String,
    files: Vec<String>,
}

fn read_lines(opts: &Options) -> Vec<String> {
    let mut lines = Vec::new();

    if opts.files.is_empty() {
        let stdin = io::stdin();
        let reader = BufReader::new(stdin.lock());
        for line in reader.lines() {
            match line {
                Ok(l) => lines.push(l),
                Err(e) => {
                    eprintln!("column: {e}");
                    process::exit(1);
                }
            }
        }
    } else {
        for file in &opts.files {
            let reader: Box<dyn Read> = if file == "-" {
                Box::new(io::stdin())
            } else {
                match fs::File::open(file) {
                    Ok(f) => Box::new(f),
                    Err(e) => {
                        eprintln!("column: {file}: {e}");
                        process::exit(1);
                    }
                }
            };
            let buf = BufReader::new(reader);
            for line in buf.lines() {
                match line {
                    Ok(l) => lines.push(l),
                    Err(e) => {
                        eprintln!("column: {e}");
                        process::exit(1);
                    }
                }
            }
        }
    }

    lines
}

fn format_table(lines: &[String], separator: &str, output_sep: &str) -> Vec<String> {
    let mut rows: Vec<Vec<String>> = Vec::new();
    let mut col_widths: Vec<usize> = Vec::new();

    for line in lines {
        if line.is_empty() {
            rows.push(Vec::new());
            continue;
        }

        let fields: Vec<String> = if separator.is_empty() {
            line.split_whitespace().map(|s| s.to_string()).collect()
        } else {
            line.split(separator).map(|s| s.to_string()).collect()
        };

        for (i, field) in fields.iter().enumerate() {
            if i >= col_widths.len() {
                col_widths.push(field.len());
            } else if field.len() > col_widths[i] {
                col_widths[i] = field.len();
            }
        }

        rows.push(fields);
    }

    let mut result = Vec::new();
    for fields in &rows {
        if fields.is_empty() {
            result.push(String::new());
            continue;
        }
        let mut parts: Vec<String> = Vec::new();
        for (i, field) in fields.iter().enumerate() {
            if i == fields.len() - 1 {
                // Last column, no padding
                parts.push(field.clone());
            } else {
                parts.push(format!("{:width$}", field, width = col_widths[i]));
            }
        }
        result.push(parts.join(output_sep));
    }

    result
}

fn format_columns(lines: &[String]) -> Vec<String> {
    // Fill columns like `ls`
    let term_width = 80;

    let max_len = lines.iter().map(|l| l.len()).max().unwrap_or(0);
    if max_len == 0 {
        return lines.to_vec();
    }

    let col_width = max_len + 2;
    let num_cols = (term_width / col_width).max(1);

    let mut result = Vec::new();
    let mut i = 0;
    while i < lines.len() {
        let mut row_parts = Vec::new();
        for c in 0..num_cols {
            let idx = i + c;
            if idx >= lines.len() {
                break;
            }
            if c == num_cols - 1 || idx + 1 >= lines.len() {
                row_parts.push(lines[idx].clone());
            } else {
                row_parts.push(format!("{:width$}", lines[idx], width = col_width));
            }
        }
        result.push(row_parts.join(""));
        i += num_cols;
    }

    result
}

fn main() {
    let args: Vec<String> = env::args().collect();

    if args.iter().any(|a| a == "--help") {
        println!("Usage: column [OPTIONS] [FILE...]");
        println!("Columnate lists.");
        println!("  -t        Create a table");
        println!("  -s DELIM  Use DELIM as column separator");
        println!("  -o SEP    Output separator (default two spaces)");
        return;
    }

    let mut opts = Options {
        table: false,
        separator: String::new(),
        output_sep: "  ".to_string(),
        files: Vec::new(),
    };

    let mut i = 1;
    while i < args.len() {
        if args[i] == "-t" {
            opts.table = true;
        } else if args[i] == "-s" {
            i += 1;
            if i >= args.len() {
                eprintln!("column: option requires an argument -- 's'");
                process::exit(1);
            }
            opts.separator = args[i].clone();
        } else if args[i] == "-o" {
            i += 1;
            if i >= args.len() {
                eprintln!("column: option requires an argument -- 'o'");
                process::exit(1);
            }
            opts.output_sep = args[i].clone();
        } else if args[i] == "--" {
            opts.files.extend_from_slice(&args[i + 1..]);
            break;
        } else {
            opts.files.push(args[i].clone());
        }
        i += 1;
    }

    let lines = read_lines(&opts);

    let output_lines = if opts.table {
        format_table(&lines, &opts.separator, &opts.output_sep)
    } else {
        let non_empty: Vec<String> = lines.into_iter().filter(|l| !l.is_empty()).collect();
        format_columns(&non_empty)
    };

    let stdout = io::stdout();
    let mut out = stdout.lock();
    for line in &output_lines {
        let _ = writeln!(out, "{line}");
    }
}
