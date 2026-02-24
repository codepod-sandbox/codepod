//! gzip / gunzip - compress or decompress files
//!
//! When invoked as "gunzip", defaults to decompression mode.

use flate2::read::{GzDecoder, GzEncoder};
use flate2::Compression;
use std::env;
use std::fs;
use std::io::{self, Read, Write};
use std::path::Path;
use std::process;

struct Options {
    decompress: bool,
    to_stdout: bool,
    keep: bool,
    level: u32,
    files: Vec<String>,
}

fn parse_args() -> Options {
    let args: Vec<String> = env::args().collect();

    // Detect gunzip via argv[0]
    let prog = Path::new(&args[0])
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("gzip");
    let default_decompress = prog.contains("gunzip");

    let mut opts = Options {
        decompress: default_decompress,
        to_stdout: false,
        keep: false,
        level: 6,
        files: Vec::new(),
    };

    let mut i = 1;
    while i < args.len() {
        let arg = &args[i];
        if arg == "-d" || arg == "--decompress" {
            opts.decompress = true;
        } else if arg == "-c" || arg == "--stdout" {
            opts.to_stdout = true;
        } else if arg == "-k" || arg == "--keep" {
            opts.keep = true;
        } else if arg.starts_with('-') && arg.len() == 2 && arg.as_bytes()[1].is_ascii_digit() {
            opts.level = (arg.as_bytes()[1] - b'0') as u32;
        } else if arg.starts_with('-') && arg != "-" {
            // Parse combined flags like -dc, -ck
            for ch in arg[1..].chars() {
                match ch {
                    'd' => opts.decompress = true,
                    'c' => opts.to_stdout = true,
                    'k' => opts.keep = true,
                    '1'..='9' => opts.level = ch as u32 - '0' as u32,
                    _ => {
                        eprintln!("gzip: invalid option -- '{}'", ch);
                        process::exit(1);
                    }
                }
            }
        } else {
            opts.files.push(arg.clone());
        }
        i += 1;
    }

    opts
}

fn compress_stream<R: Read, W: Write>(input: &mut R, output: &mut W, level: u32) -> io::Result<()> {
    let mut encoder = GzEncoder::new(input, Compression::new(level));
    io::copy(&mut encoder, output)?;
    Ok(())
}

fn decompress_stream<R: Read, W: Write>(input: &mut R, output: &mut W) -> io::Result<()> {
    let mut decoder = GzDecoder::new(input);
    io::copy(&mut decoder, output)?;
    Ok(())
}

fn main() {
    let opts = parse_args();

    // No files: stdin/stdout mode
    if opts.files.is_empty() {
        let stdin = io::stdin();
        let stdout = io::stdout();
        let mut input = stdin.lock();
        let mut output = stdout.lock();

        let result = if opts.decompress {
            decompress_stream(&mut input, &mut output)
        } else {
            compress_stream(&mut input, &mut output, opts.level)
        };

        if let Err(e) = result {
            eprintln!("gzip: {}", e);
            process::exit(1);
        }
        return;
    }

    // File mode
    for file in &opts.files {
        if opts.decompress {
            decompress_file(file, &opts);
        } else {
            compress_file(file, &opts);
        }
    }
}

fn compress_file(path: &str, opts: &Options) {
    let data = match fs::read(path) {
        Ok(d) => d,
        Err(e) => {
            eprintln!("gzip: {}: {}", path, e);
            process::exit(1);
        }
    };

    let mut input = &data[..];
    let out_path = format!("{}.gz", path);

    if opts.to_stdout {
        let stdout = io::stdout();
        let mut output = stdout.lock();
        if let Err(e) = compress_stream(&mut input, &mut output, opts.level) {
            eprintln!("gzip: {}", e);
            process::exit(1);
        }
    } else {
        let mut output = Vec::new();
        if let Err(e) = compress_stream(&mut input, &mut output, opts.level) {
            eprintln!("gzip: {}", e);
            process::exit(1);
        }
        if let Err(e) = fs::write(&out_path, &output) {
            eprintln!("gzip: {}: {}", out_path, e);
            process::exit(1);
        }
        if !opts.keep {
            let _ = fs::remove_file(path);
        }
    }
}

fn decompress_file(path: &str, opts: &Options) {
    let data = match fs::read(path) {
        Ok(d) => d,
        Err(e) => {
            eprintln!("gzip: {}: {}", path, e);
            process::exit(1);
        }
    };

    let mut input = &data[..];
    let out_path = if let Some(stripped) = path.strip_suffix(".gz") {
        stripped.to_string()
    } else {
        format!("{}.out", path)
    };

    if opts.to_stdout {
        let stdout = io::stdout();
        let mut output = stdout.lock();
        if let Err(e) = decompress_stream(&mut input, &mut output) {
            eprintln!("gzip: {}", e);
            process::exit(1);
        }
    } else {
        let mut output = Vec::new();
        if let Err(e) = decompress_stream(&mut input, &mut output) {
            eprintln!("gzip: {}", e);
            process::exit(1);
        }
        if let Err(e) = fs::write(&out_path, &output) {
            eprintln!("gzip: {}: {}", out_path, e);
            process::exit(1);
        }
        if !opts.keep {
            let _ = fs::remove_file(path);
        }
    }
}
