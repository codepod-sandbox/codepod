use std::env;
use std::fs;
use std::process;

fn main() {
    let mut exit_code = 0;
    for arg in env::args().skip(1) {
        if let Err(e) = fs::remove_dir(&arg) {
            eprintln!("rmdir: failed to remove '{arg}': {e}");
            exit_code = 1;
        }
    }
    process::exit(exit_code);
}
