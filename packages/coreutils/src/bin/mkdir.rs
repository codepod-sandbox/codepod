use std::env;
use std::fs;
use std::path::Path;
use std::process;

fn main() {
    let mut create_parents = false;
    let mut dirs: Vec<String> = Vec::new();

    for arg in env::args().skip(1) {
        if arg == "--" {
            break;
        }
        if arg.starts_with('-') && arg.len() > 1 {
            for ch in arg[1..].chars() {
                match ch {
                    'p' => create_parents = true,
                    _ => {
                        eprintln!("mkdir: invalid option -- '{}'", ch);
                        process::exit(1);
                    }
                }
            }
        } else {
            dirs.push(arg);
        }
    }

    if dirs.is_empty() {
        eprintln!("mkdir: missing operand");
        process::exit(1);
    }

    let mut exit_code = 0;

    for dir in &dirs {
        let path = Path::new(dir);
        let result = if create_parents {
            fs::create_dir_all(path)
        } else {
            fs::create_dir(path)
        };

        if let Err(e) = result {
            eprintln!("mkdir: cannot create directory '{}': {}", dir, e);
            exit_code = 1;
        }
    }

    process::exit(exit_code);
}
