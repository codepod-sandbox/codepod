//! nice - run a program with modified scheduling priority
//!
//! Usage: nice [-n N] command [args...]
//!
//! The -n adjustment value (0–19) is forwarded to the host scheduler so the
//! child process runs at the requested epoch quantum. Unlike OS nice(), the
//! value is not additive — it sets the absolute priority directly.
//!
//! Without a command, prints the current niceness (always 0 in WASM, because
//! priority is set at sandbox-creation time by the host).

use std::env;
use codepod_process::Command;

fn main() {
    let args: Vec<String> = env::args().skip(1).collect();

    if args.is_empty() {
        println!("0");
        return;
    }

    // Parse -n <value> / -n<value> / --adjustment=<value>
    let mut nice: u8 = 0;
    let mut i = 0;
    while i < args.len() {
        if (args[i] == "-n" || args[i] == "--adjustment") && i + 1 < args.len() {
            nice = args[i + 1].parse::<i32>().unwrap_or(0).clamp(0, 19) as u8;
            i += 2;
        } else if let Some(val) = args[i].strip_prefix("-n") {
            nice = val.parse::<i32>().unwrap_or(0).clamp(0, 19) as u8;
            i += 1;
        } else if let Some(val) = args[i].strip_prefix("--adjustment=") {
            nice = val.parse::<i32>().unwrap_or(0).clamp(0, 19) as u8;
            i += 1;
        } else {
            break;
        }
    }

    if i >= args.len() {
        println!("0");
        return;
    }

    let status = Command::new(&args[i])
        .args(&args[i + 1..])
        .nice(nice)
        .status()
        .unwrap_or_else(|e| {
            eprintln!("nice: {}: {e}", args[i]);
            std::process::exit(127);
        });

    std::process::exit(status.code().unwrap_or(1));
}
