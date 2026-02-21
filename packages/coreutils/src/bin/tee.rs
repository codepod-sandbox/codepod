//! tee - read from stdin, write to stdout and files

use std::env;
use std::fs::OpenOptions;
use std::io::{self, BufRead, Write};
use std::process;

fn main() {
    let args: Vec<String> = env::args().collect();

    let mut append = false;
    let mut files: Vec<String> = Vec::new();

    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "-a" => append = true,
            _ => files.push(args[i].clone()),
        }
        i += 1;
    }

    // Open all output files
    let mut writers: Vec<Box<dyn Write>> = Vec::new();
    for path in &files {
        let file = OpenOptions::new()
            .create(true)
            .write(true)
            .append(append)
            .truncate(!append)
            .open(path);

        match file {
            Ok(f) => writers.push(Box::new(f)),
            Err(e) => {
                eprintln!("tee: {}: {}", path, e);
                process::exit(1);
            }
        }
    }

    let stdin = io::stdin();
    let stdout = io::stdout();
    let mut stdout_lock = stdout.lock();

    for line_result in stdin.lock().lines() {
        match line_result {
            Ok(line) => {
                let line_bytes = format!("{}\n", line);
                // Write to stdout
                if let Err(e) = stdout_lock.write_all(line_bytes.as_bytes()) {
                    eprintln!("tee: stdout: {}", e);
                    process::exit(1);
                }
                // Write to each file
                for writer in &mut writers {
                    if let Err(e) = writer.write_all(line_bytes.as_bytes()) {
                        eprintln!("tee: write error: {}", e);
                        process::exit(1);
                    }
                }
            }
            Err(e) => {
                eprintln!("tee: read error: {}", e);
                process::exit(1);
            }
        }
    }
}
