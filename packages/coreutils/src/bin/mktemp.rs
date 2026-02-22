use std::fs;

fn main() {
    // Generate a pseudo-random temp filename.
    // Use the address of a stack variable as entropy (not cryptographic, just unique).
    let mut name = String::from("/tmp/tmp.");
    let val: usize = 0;
    let addr = &val as *const usize as usize;
    for i in 0..8 {
        let c = (((addr >> (i * 4)) & 0xF) as u8).wrapping_add(b'a');
        name.push(c as char);
    }
    // Create the file
    if let Err(e) = fs::write(&name, "") {
        eprintln!("mktemp: {e}");
        std::process::exit(1);
    }
    println!("{name}");
}
