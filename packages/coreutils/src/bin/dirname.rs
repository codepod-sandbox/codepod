//! dirname - strip last component from file name

use std::env;
use std::process;

fn main() {
    let args: Vec<String> = env::args().collect();

    if args.len() < 2 {
        eprintln!("dirname: missing operand");
        process::exit(1);
    }

    let path = &args[1];

    // Remove trailing slashes
    let trimmed = path.trim_end_matches('/');

    if trimmed.is_empty() {
        // Path was all slashes
        println!("/");
        return;
    }

    match trimmed.rfind('/') {
        Some(0) => {
            // The slash is at the root
            println!("/");
        }
        Some(pos) => {
            // Return everything up to the last slash
            let dir = &trimmed[..pos];
            // Handle case where dir would be empty (shouldn't happen since pos > 0)
            if dir.is_empty() {
                println!("/");
            } else {
                println!("{}", dir);
            }
        }
        None => {
            // No slash in the path, directory is "."
            println!(".");
        }
    }
}
