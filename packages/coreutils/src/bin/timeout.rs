//! timeout - run a command with a time limit (sandbox stub)
//!
//! In a WASM sandbox, actual timeout enforcement is delegated to the
//! sandbox runtime. This stub accepts the standard syntax for compatibility
//! so that scripts using `timeout` do not break.

use std::env;
use std::process;

fn main() {
    let args: Vec<String> = env::args().collect();

    if args.iter().any(|a| a == "--help") {
        println!("Usage: timeout DURATION COMMAND [ARG...]");
        println!("Run a command with a time limit.");
        println!("DURATION may have a suffix: s (seconds), m (minutes), h (hours).");
        println!("Note: In the WASM sandbox, timeout enforcement is delegated to the runtime.");
        return;
    }

    if args.len() < 3 {
        eprintln!("timeout: missing operand");
        process::exit(1);
    }

    // Validate duration argument (args[1])
    let duration_str = &args[1];
    let (num_str, _suffix) = if duration_str.ends_with('s')
        || duration_str.ends_with('m')
        || duration_str.ends_with('h')
    {
        let (n, s) = duration_str.split_at(duration_str.len() - 1);
        (n, s)
    } else {
        (duration_str.as_str(), "s")
    };

    if num_str.parse::<f64>().is_err() {
        eprintln!("timeout: invalid time interval: {duration_str}");
        process::exit(1);
    }

    eprintln!("timeout: note: timeout enforcement delegated to sandbox runtime");

    // In a real system we would exec the command. In the sandbox we just exit.
}
