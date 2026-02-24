//! od - octal dump

use std::env;
use std::fs::File;
use std::io::{self, Read};
use std::process;

#[derive(Clone)]
enum OutputType {
    Octal,
    Hex,
    Decimal,
    Chars,
    NamedChars,
}

#[derive(Clone)]
enum AddressRadix {
    Octal,
    Decimal,
    Hex,
    None,
}

fn format_address(offset: usize, radix: &AddressRadix) -> String {
    match radix {
        AddressRadix::Octal => format!("{:07o}", offset),
        AddressRadix::Decimal => format!("{:07}", offset),
        AddressRadix::Hex => format!("{:07x}", offset),
        AddressRadix::None => String::new(),
    }
}

fn named_char(byte: u8) -> String {
    match byte {
        0x00 => "nul".to_string(),
        0x01 => "soh".to_string(),
        0x02 => "stx".to_string(),
        0x03 => "etx".to_string(),
        0x04 => "eot".to_string(),
        0x05 => "enq".to_string(),
        0x06 => "ack".to_string(),
        0x07 => "bel".to_string(),
        0x08 => " bs".to_string(),
        0x09 => " ht".to_string(),
        0x0a => " nl".to_string(),
        0x0b => " vt".to_string(),
        0x0c => " ff".to_string(),
        0x0d => " cr".to_string(),
        0x0e => " so".to_string(),
        0x0f => " si".to_string(),
        0x10 => "dle".to_string(),
        0x11 => "dc1".to_string(),
        0x12 => "dc2".to_string(),
        0x13 => "dc3".to_string(),
        0x14 => "dc4".to_string(),
        0x15 => "nak".to_string(),
        0x16 => "syn".to_string(),
        0x17 => "etb".to_string(),
        0x18 => "can".to_string(),
        0x19 => " em".to_string(),
        0x1a => "sub".to_string(),
        0x1b => "esc".to_string(),
        0x1c => " fs".to_string(),
        0x1d => " gs".to_string(),
        0x1e => " rs".to_string(),
        0x1f => " us".to_string(),
        0x20 => " sp".to_string(),
        0x7f => "del".to_string(),
        b => format!("  {}", b as char),
    }
}

fn char_repr(byte: u8) -> String {
    match byte {
        b'\\' => " \\\\".to_string(),
        b'\0' => " \\0".to_string(),
        b'\n' => " \\n".to_string(),
        b'\t' => " \\t".to_string(),
        b'\r' => " \\r".to_string(),
        0x20..=0x7e => format!("   {}", byte as char),
        _ => format!("{:>4o}", byte),
    }
}

fn dump_data(data: &[u8], output_type: &OutputType, address_radix: &AddressRadix) {
    let bytes_per_line = 16;

    let mut offset = 0;
    while offset < data.len() {
        let end = (offset + bytes_per_line).min(data.len());
        let chunk = &data[offset..end];

        let addr = format_address(offset, address_radix);
        if !addr.is_empty() {
            print!("{}", addr);
        }

        match output_type {
            OutputType::Octal => {
                // Display as 2-byte octal words
                let mut i = 0;
                while i < chunk.len() {
                    if i + 1 < chunk.len() {
                        let word = (chunk[i + 1] as u16) << 8 | chunk[i] as u16;
                        print!(" {:06o}", word);
                    } else {
                        print!(" {:06o}", chunk[i] as u16);
                    }
                    i += 2;
                }
            }
            OutputType::Hex => {
                for &byte in chunk {
                    print!(" {:02x}", byte);
                }
            }
            OutputType::Decimal => {
                // Display as 2-byte decimal words
                let mut i = 0;
                while i < chunk.len() {
                    if i + 1 < chunk.len() {
                        let word = (chunk[i + 1] as u16) << 8 | chunk[i] as u16;
                        print!("  {:05}", word);
                    } else {
                        print!("  {:05}", chunk[i] as u16);
                    }
                    i += 2;
                }
            }
            OutputType::Chars => {
                for &byte in chunk {
                    print!("{}", char_repr(byte));
                }
            }
            OutputType::NamedChars => {
                for &byte in chunk {
                    print!(" {:>3}", named_char(byte));
                }
            }
        }

        println!();
        offset = end;
    }

    // Print final address
    let addr = format_address(data.len(), address_radix);
    if !addr.is_empty() {
        println!("{}", addr);
    }
}

fn main() {
    let args: Vec<String> = env::args().collect();

    if args.iter().any(|a| a == "--help") {
        println!("Usage: od [-A RADIX] [-t TYPE] [-N COUNT] [FILE...]");
        println!("Octal dump.");
        println!("  -A RADIX  address radix: o (octal), d (decimal), x (hex), n (none)");
        println!("  -t TYPE   output type: o (octal), x (hex), d (decimal), c (chars), a (named)");
        println!("  -N COUNT  read only COUNT bytes");
        return;
    }

    let mut output_type = OutputType::Octal;
    let mut address_radix = AddressRadix::Octal;
    let mut max_bytes: Option<usize> = None;
    let mut files: Vec<String> = Vec::new();
    let mut i = 1;

    while i < args.len() {
        if args[i] == "-A" {
            i += 1;
            if i >= args.len() {
                eprintln!("od: option requires an argument -- 'A'");
                process::exit(1);
            }
            address_radix = match args[i].as_str() {
                "o" => AddressRadix::Octal,
                "d" => AddressRadix::Decimal,
                "x" => AddressRadix::Hex,
                "n" => AddressRadix::None,
                other => {
                    eprintln!("od: invalid address radix: {}", other);
                    process::exit(1);
                }
            };
        } else if args[i] == "-t" {
            i += 1;
            if i >= args.len() {
                eprintln!("od: option requires an argument -- 't'");
                process::exit(1);
            }
            output_type = match args[i].as_str() {
                "o" | "o2" => OutputType::Octal,
                "x" | "x1" => OutputType::Hex,
                "d" | "d2" => OutputType::Decimal,
                "c" => OutputType::Chars,
                "a" => OutputType::NamedChars,
                other => {
                    eprintln!("od: invalid type: {}", other);
                    process::exit(1);
                }
            };
        } else if args[i] == "-N" {
            i += 1;
            if i >= args.len() {
                eprintln!("od: option requires an argument -- 'N'");
                process::exit(1);
            }
            max_bytes = match args[i].parse() {
                Ok(n) => Some(n),
                Err(_) => {
                    eprintln!("od: invalid count: {}", args[i]);
                    process::exit(1);
                }
            };
        } else if args[i] == "--" {
            files.extend_from_slice(&args[i + 1..]);
            break;
        } else {
            files.push(args[i].clone());
        }
        i += 1;
    }

    let mut data = Vec::new();

    if files.is_empty() {
        if let Err(e) = io::stdin().lock().read_to_end(&mut data) {
            eprintln!("od: {}", e);
            process::exit(1);
        }
    } else {
        for path in &files {
            match File::open(path) {
                Ok(mut f) => {
                    if let Err(e) = f.read_to_end(&mut data) {
                        eprintln!("od: {}: {}", path, e);
                        process::exit(1);
                    }
                }
                Err(e) => {
                    eprintln!("od: {}: {}", path, e);
                    process::exit(1);
                }
            }
        }
    }

    if let Some(max) = max_bytes {
        data.truncate(max);
    }

    dump_data(&data, &output_type, &address_radix);
}
