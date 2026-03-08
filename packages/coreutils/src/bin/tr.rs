//! tr - translate or delete characters

use std::env;
use std::io::{self, Read, Write};
use std::process;

/// Expand a POSIX character class like [:lower:] into its characters.
fn expand_char_class(name: &str) -> Option<Vec<char>> {
    match name {
        "lower" => Some(('a'..='z').collect()),
        "upper" => Some(('A'..='Z').collect()),
        "digit" => Some(('0'..='9').collect()),
        "alpha" => Some(('a'..='z').chain('A'..='Z').collect()),
        "alnum" => Some(('0'..='9').chain('a'..='z').chain('A'..='Z').collect()),
        "space" => Some(vec![' ', '\t', '\n', '\r', '\x0B', '\x0C']),
        "blank" => Some(vec![' ', '\t']),
        "punct" => Some(
            (0x21u8..=0x2Fu8)
                .chain(0x3Au8..=0x40u8)
                .chain(0x5Bu8..=0x60u8)
                .chain(0x7Bu8..=0x7Eu8)
                .map(|b| b as char)
                .collect(),
        ),
        "print" => Some((0x20u8..=0x7Eu8).map(|b| b as char).collect()),
        "graph" => Some((0x21u8..=0x7Eu8).map(|b| b as char).collect()),
        "cntrl" => Some(
            (0x00u8..=0x1Fu8)
                .chain(std::iter::once(0x7Fu8))
                .map(|b| b as char)
                .collect(),
        ),
        "xdigit" => Some(('0'..='9').chain('a'..='f').chain('A'..='F').collect()),
        _ => None,
    }
}

/// Expand a character set specification, handling ranges like a-z,
/// POSIX character classes like [:lower:], and escape sequences.
fn expand_set(spec: &str) -> Vec<char> {
    let chars: Vec<char> = spec.chars().collect();
    let mut result = Vec::new();
    let mut i = 0;

    while i < chars.len() {
        // Handle POSIX character classes like [:lower:]
        if chars[i] == '[' && i + 1 < chars.len() && chars[i + 1] == ':' {
            if let Some(end) = chars[i + 2..].iter().position(|&c| c == ':') {
                let class_end = i + 2 + end;
                if class_end + 1 < chars.len() && chars[class_end + 1] == ']' {
                    let name: String = chars[i + 2..class_end].iter().collect();
                    if let Some(expanded) = expand_char_class(&name) {
                        result.extend(expanded);
                        i = class_end + 2;
                        continue;
                    }
                }
            }
        }
        // Handle backslash escape sequences
        if chars[i] == '\\' && i + 1 < chars.len() {
            let escaped = match chars[i + 1] {
                'n' => '\n',
                't' => '\t',
                'r' => '\r',
                'a' => '\x07',
                'b' => '\x08',
                'f' => '\x0C',
                'v' => '\x0B',
                '0' => '\0',
                '\\' => '\\',
                // Octal: \NNN
                c if c.is_ascii_digit() => {
                    let mut val = 0u32;
                    let mut j = i + 1;
                    while j < chars.len() && j < i + 4 && chars[j].is_ascii_digit() {
                        val = val * 8 + (chars[j] as u32 - '0' as u32);
                        j += 1;
                    }
                    i = j;
                    if let Some(ch) = char::from_u32(val) {
                        result.push(ch);
                    }
                    continue;
                }
                other => other,
            };
            result.push(escaped);
            i += 2;
        } else if i + 2 < chars.len() && chars[i + 1] == '-' {
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
