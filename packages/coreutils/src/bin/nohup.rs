//! nohup - run a command immune to hangups (sandbox stub)

use std::env;
use std::process;

fn main() {
    let args: Vec<String> = env::args().skip(1).collect();

    if args.is_empty() {
        eprintln!("usage: nohup COMMAND [ARG]...");
        process::exit(1);
    }

    eprintln!("nohup: signal handling is not needed in this environment");
}
