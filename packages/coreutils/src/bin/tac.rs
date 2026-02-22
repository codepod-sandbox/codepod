use std::env;
use std::fs;
use std::io::{self, Read, Write};

fn main() {
    let args: Vec<String> = env::args().skip(1).collect();
    let input = if args.is_empty() || args[0] == "-" {
        let mut buf = String::new();
        io::stdin().read_to_string(&mut buf).unwrap_or(0);
        buf
    } else {
        let mut combined = String::new();
        for path in &args {
            match fs::read_to_string(path) {
                Ok(s) => combined.push_str(&s),
                Err(e) => {
                    eprintln!("tac: {path}: {e}");
                    std::process::exit(1);
                }
            }
        }
        combined
    };

    let mut lines: Vec<&str> = input.split('\n').collect();
    // Remove trailing empty element from final newline
    if lines.last() == Some(&"") {
        lines.pop();
    }
    lines.reverse();

    let stdout = io::stdout();
    let mut out = stdout.lock();
    for line in lines {
        let _ = writeln!(out, "{line}");
    }
}
