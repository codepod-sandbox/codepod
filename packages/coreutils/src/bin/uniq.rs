//! uniq - report or omit repeated lines

use std::env;
use std::fs::File;
use std::io::{self, BufRead, BufReader};
use std::process;

struct Options {
    count: bool,
    only_duplicates: bool,
    only_unique: bool,
}

fn read_lines<R: io::Read>(reader: R) -> io::Result<Vec<String>> {
    let buf = BufReader::new(reader);
    buf.lines().collect()
}

fn process_lines(lines: &[String], opts: &Options) {
    if lines.is_empty() {
        return;
    }

    // Group adjacent identical lines
    let mut groups: Vec<(usize, &str)> = Vec::new();
    let mut current = &lines[0] as &str;
    let mut count: usize = 1;

    for line in &lines[1..] {
        if line == current {
            count += 1;
        } else {
            groups.push((count, current));
            current = line;
            count = 1;
        }
    }
    groups.push((count, current));

    for (cnt, line) in &groups {
        let is_dup = *cnt > 1;
        // -d: only print duplicated lines
        if opts.only_duplicates && !is_dup {
            continue;
        }
        // -u: only print unique lines (that appeared exactly once)
        if opts.only_unique && is_dup {
            continue;
        }

        if opts.count {
            println!("{:>7} {}", cnt, line);
        } else {
            println!("{}", line);
        }
    }
}

fn main() {
    let args: Vec<String> = env::args().collect();

    let mut opts = Options {
        count: false,
        only_duplicates: false,
        only_unique: false,
    };
    let mut files: Vec<String> = Vec::new();

    for arg in &args[1..] {
        if arg == "--" {
            continue;
        }
        if arg.starts_with('-') && arg.len() > 1 && !arg.starts_with("--") {
            for ch in arg[1..].chars() {
                match ch {
                    'c' => opts.count = true,
                    'd' => opts.only_duplicates = true,
                    'u' => opts.only_unique = true,
                    _ => {
                        eprintln!("uniq: invalid option -- '{}'", ch);
                        process::exit(1);
                    }
                }
            }
        } else {
            files.push(arg.clone());
        }
    }

    let lines = if files.is_empty() || files[0] == "-" {
        let stdin = io::stdin();
        match read_lines(stdin.lock()) {
            Ok(l) => l,
            Err(e) => {
                eprintln!("uniq: {}", e);
                process::exit(1);
            }
        }
    } else {
        match File::open(&files[0]) {
            Ok(f) => match read_lines(f) {
                Ok(l) => l,
                Err(e) => {
                    eprintln!("uniq: {}: {}", files[0], e);
                    process::exit(1);
                }
            },
            Err(e) => {
                eprintln!("uniq: {}: {}", files[0], e);
                process::exit(1);
            }
        }
    };

    process_lines(&lines, &opts);
}
