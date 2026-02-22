use std::env;

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.iter().any(|a| a == "-a") {
        println!("wasmsand wasmsand 0.1.0 wasm32-wasip1");
    } else {
        println!("wasmsand");
    }
}
