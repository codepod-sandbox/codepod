use std::env;
use std::process;

fn main() {
    let args: Vec<String> = env::args().skip(1).collect();
    if args.is_empty() {
        for (key, val) in env::vars() {
            println!("{key}={val}");
        }
    } else {
        for name in &args {
            match env::var(name) {
                Ok(val) => println!("{val}"),
                Err(_) => process::exit(1),
            }
        }
    }
}
