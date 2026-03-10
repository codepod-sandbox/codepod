//! chgrp - change group ownership (sandbox stub)

use std::process;

fn main() {
    eprintln!("chgrp: operation not permitted in sandbox");
    process::exit(1);
}
