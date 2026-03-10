//! chown - change file owner (sandbox stub)

use std::process;

fn main() {
    eprintln!("chown: operation not permitted in sandbox");
    process::exit(1);
}
