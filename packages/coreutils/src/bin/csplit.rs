//! csplit - split a file into sections determined by context lines

use std::env;
use std::fs;
use std::io::Write;
use std::process;

enum Pattern {
    Regex { pattern: String, skip: bool },
    Repeat { count: usize },
}

fn matches_line(pattern: &str, line: &str) -> bool {
    regex_match(pattern, line)
}

fn regex_match(pattern: &str, text: &str) -> bool {
    let pat_chars: Vec<char> = pattern.chars().collect();

    if pat_chars.is_empty() {
        return true;
    }

    let anchored_start = pat_chars[0] == '^';
    let start = if anchored_start { 1 } else { 0 };

    let anchored_end = pat_chars.last() == Some(&'$');
    let end = if anchored_end {
        pat_chars.len() - 1
    } else {
        pat_chars.len()
    };

    let pat = &pat_chars[start..end];

    if anchored_start {
        return match_at(pat, text, 0, anchored_end);
    }

    for i in 0..=text.len() {
        if match_at(pat, text, i, anchored_end) {
            return true;
        }
    }
    false
}

fn match_at(pat: &[char], text: &str, start: usize, anchored_end: bool) -> bool {
    let text_chars: Vec<char> = text.chars().collect();
    let mut pi = 0;
    let mut ti = start;

    while pi < pat.len() {
        if pi + 1 < pat.len() && pat[pi + 1] == '*' {
            let ch = pat[pi];
            pi += 2;
            let save = ti;
            while ti < text_chars.len() && char_matches(ch, text_chars[ti]) {
                ti += 1;
            }
            while ti >= save {
                if match_at(&pat[pi..], text, ti, anchored_end) {
                    return true;
                }
                if ti == save {
                    break;
                }
                ti -= 1;
            }
            return false;
        }

        if ti >= text_chars.len() {
            return false;
        }

        if !char_matches(pat[pi], text_chars[ti]) {
            return false;
        }

        pi += 1;
        ti += 1;
    }

    if anchored_end {
        ti == text_chars.len()
    } else {
        true
    }
}

fn char_matches(pat: char, ch: char) -> bool {
    if pat == '.' {
        return true;
    }
    pat == ch
}

fn parse_patterns(args: &[String]) -> Vec<Pattern> {
    let mut patterns = Vec::new();

    for arg in args {
        if arg.starts_with('/') && arg.ends_with('/') && arg.len() > 1 {
            let inner = &arg[1..arg.len() - 1];
            patterns.push(Pattern::Regex {
                pattern: inner.to_string(),
                skip: false,
            });
        } else if arg.starts_with('%') && arg.ends_with('%') && arg.len() > 1 {
            let inner = &arg[1..arg.len() - 1];
            patterns.push(Pattern::Regex {
                pattern: inner.to_string(),
                skip: true,
            });
        } else if arg.starts_with('{') && arg.ends_with('}') {
            let inner = &arg[1..arg.len() - 1];
            match inner.parse::<usize>() {
                Ok(n) => patterns.push(Pattern::Repeat { count: n }),
                Err(_) => {
                    eprintln!("csplit: invalid repeat count: {arg}");
                    process::exit(1);
                }
            }
        } else {
            match arg.parse::<usize>() {
                Ok(_n) => {
                    patterns.push(Pattern::Regex {
                        pattern: arg.clone(),
                        skip: false,
                    });
                }
                Err(_) => {
                    eprintln!("csplit: invalid pattern: {arg}");
                    process::exit(1);
                }
            }
        }
    }

    patterns
}

fn main() {
    let args: Vec<String> = env::args().collect();

    if args.iter().any(|a| a == "--help") {
        println!("Usage: csplit [OPTIONS] FILE PATTERN...");
        println!("Split FILE into sections determined by PATTERN(s).");
        println!("  -f PREFIX  Use PREFIX instead of 'xx'");
        println!("  /REGEX/    Split at lines matching REGEX");
        println!("  %REGEX%    Skip to line matching REGEX");
        println!("  {{N}}        Repeat previous pattern N times");
        return;
    }

    let mut prefix = String::from("xx");
    let mut positional: Vec<String> = Vec::new();
    let mut i = 1;

    while i < args.len() {
        if args[i] == "-f" {
            i += 1;
            if i >= args.len() {
                eprintln!("csplit: option requires an argument -- 'f'");
                process::exit(1);
            }
            prefix = args[i].clone();
        } else {
            positional.push(args[i].clone());
        }
        i += 1;
    }

    if positional.len() < 2 {
        eprintln!("csplit: usage: csplit [OPTIONS] FILE PATTERN...");
        process::exit(1);
    }

    let file = &positional[0];
    let content = match fs::read_to_string(file) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("csplit: {file}: {e}");
            process::exit(1);
        }
    };

    let lines: Vec<&str> = content.lines().collect();
    let patterns = parse_patterns(&positional[1..]);

    // Expand repeat patterns
    let mut expanded: Vec<&Pattern> = Vec::new();
    for (i, pat) in patterns.iter().enumerate() {
        match pat {
            Pattern::Repeat { count } => {
                if i == 0 {
                    eprintln!("csplit: repeat count with no preceding pattern");
                    process::exit(1);
                }
                for _ in 0..*count {
                    expanded.push(&patterns[i - 1]);
                }
            }
            other => expanded.push(other),
        }
    }

    let stdout = std::io::stdout();
    let mut out = stdout.lock();
    let mut file_index = 0;
    let mut line_idx = 0;

    for pat in &expanded {
        match pat {
            Pattern::Regex { pattern, skip } => {
                let found = lines
                    .iter()
                    .enumerate()
                    .skip(line_idx)
                    .find(|(_, line)| matches_line(pattern, line))
                    .map(|(i, _)| i);

                let split_at = match found {
                    Some(pos) => pos,
                    None => lines.len(),
                };

                if *skip {
                    line_idx = split_at;
                } else {
                    let section: String = lines[line_idx..split_at]
                        .iter()
                        .map(|l| format!("{l}\n"))
                        .collect();
                    let filename = format!("{prefix}{file_index:02}");
                    if let Err(e) = fs::write(&filename, &section) {
                        eprintln!("csplit: {filename}: {e}");
                        process::exit(1);
                    }
                    let _ = writeln!(out, "{}", section.len());
                    file_index += 1;
                    line_idx = split_at;
                }
            }
            Pattern::Repeat { .. } => unreachable!(),
        }
    }

    // Write remaining lines
    if line_idx < lines.len() {
        let section: String = lines[line_idx..].iter().map(|l| format!("{l}\n")).collect();
        let filename = format!("{prefix}{file_index:02}");
        if let Err(e) = fs::write(&filename, &section) {
            eprintln!("csplit: {filename}: {e}");
            process::exit(1);
        }
        let _ = writeln!(out, "{}", section.len());
    } else {
        let filename = format!("{prefix}{file_index:02}");
        if let Err(e) = fs::write(&filename, "") {
            eprintln!("csplit: {filename}: {e}");
            process::exit(1);
        }
        let _ = writeln!(out, "0");
    }
}
