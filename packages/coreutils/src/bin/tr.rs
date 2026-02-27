//! tr - translate or delete characters

use std::env;
use std::io::{self, Read, Write};
use std::process;

/// Expand a character set specification, handling ranges like a-z.
fn expand_set(spec: &str) -> Vec<char> {
    let chars: Vec<char> = spec.chars().collect();
    let mut result = Vec::new();
    let mut i = 0;

    while i < chars.len() {
        if i + 2 < chars.len() && chars[i + 1] == '-' {
            let start = chars[i] as u32;
            let end = chars[i + 2] as u32;
            if start <= end {
                for code in start..=end {
                    if let Some(c) = char::from_u32(code) {
                        result.push(c);
                    }
                }
            } else {
                // Invalid range, treat literally
                result.push(chars[i]);
                result.push('-');
                result.push(chars[i + 2]);
            }
            i += 3;
        } else {
            result.push(chars[i]);
            i += 1;
        }
    }

    result
}

fn main() {
    let args: Vec<String> = env::args().collect();

    let mut delete = false;
    let mut squeeze = false;
    let mut complement = false;
    let mut positional: Vec<String> = Vec::new();

    let mut i = 1;
    while i < args.len() {
        let arg = &args[i];
        if arg.starts_with('-') && arg.len() > 1 && !arg.starts_with("--") {
            for ch in arg[1..].chars() {
                match ch {
                    'd' => delete = true,
                    's' => squeeze = true,
                    'c' | 'C' => complement = true,
                    _ => {
                        eprintln!("tr: invalid option -- '{ch}'");
                        process::exit(1);
                    }
                }
            }
        } else {
            positional.push(arg.clone());
        }
        i += 1;
    }

    if positional.is_empty() {
        eprintln!("tr: missing operand");
        process::exit(1);
    }

    let set1 = expand_set(&positional[0]);

    let mut input = String::new();
    if let Err(e) = io::stdin().read_to_string(&mut input) {
        eprintln!("tr: read error: {e}");
        process::exit(1);
    }

    let stdout = io::stdout();
    let mut out = stdout.lock();

    if delete && squeeze {
        // -ds: delete chars in SET1, then squeeze chars in SET2
        let set2 = if positional.len() > 1 {
            expand_set(&positional[1])
        } else {
            vec![]
        };
        let mut last: Option<char> = None;
        for c in input.chars() {
            let in_set1 = set1.contains(&c) != complement;
            if in_set1 {
                continue; // delete
            }
            // Squeeze consecutive chars in set2
            if set2.contains(&c) && last == Some(c) {
                continue;
            }
            last = Some(c);
            let _ = write!(out, "{c}");
        }
    } else if delete {
        for c in input.chars() {
            let in_set1 = set1.contains(&c) != complement;
            if !in_set1 {
                let _ = write!(out, "{c}");
            }
        }
    } else if squeeze && positional.len() < 2 {
        // -s with one set: squeeze consecutive repeats of chars in SET1
        let mut last: Option<char> = None;
        for c in input.chars() {
            let in_set = set1.contains(&c) != complement;
            if in_set && last == Some(c) {
                continue;
            }
            last = Some(c);
            let _ = write!(out, "{c}");
        }
    } else {
        // Translate (and optionally squeeze)
        if positional.len() < 2 {
            eprintln!("tr: missing operand");
            process::exit(1);
        }
        let set2 = expand_set(&positional[1]);
        let mut last_out: Option<char> = None;

        for c in input.chars() {
            let in_set1 = set1.contains(&c) != complement;
            let replacement = if in_set1 {
                if let Some(pos) = set1.iter().position(|&s| s == c) {
                    if pos < set2.len() {
                        set2[pos]
                    } else if !set2.is_empty() {
                        set2[set2.len() - 1]
                    } else {
                        c
                    }
                } else if complement {
                    // Complemented: char not in set1, translate to last of set2
                    if !set2.is_empty() {
                        set2[set2.len() - 1]
                    } else {
                        c
                    }
                } else {
                    c
                }
            } else {
                c
            };
            // Squeeze if -s and char is in set2
            if squeeze && set2.contains(&replacement) && last_out == Some(replacement) {
                continue;
            }
            last_out = Some(replacement);
            let _ = write!(out, "{replacement}");
        }
    }
}
