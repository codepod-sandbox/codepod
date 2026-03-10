//! nice - run a program with modified scheduling priority (sandbox stub)

use std::env;
use std::process;

fn main() {
    let args: Vec<String> = env::args().skip(1).collect();

    if args.is_empty() {
        println!("0");
        return;
    }

    // Skip -n/--adjustment and its value
    let mut i = 0;
    let mut has_command = false;
    while i < args.len() {
        if args[i] == "-n" || args[i] == "--adjustment" {
            i += 2; // skip flag and value
        } else if args[i].starts_with("-n") || args[i].starts_with("--adjustment=") {
            i += 1; // skip combined flag
        } else {
            has_command = true;
            break;
        }
    }

    if has_command {
        eprintln!("nice: cannot set priority in this environment");
        process::exit(0);
    }

    // Only flags, no command
    println!("0");
}
