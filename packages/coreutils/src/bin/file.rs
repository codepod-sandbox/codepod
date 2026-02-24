//! file - determine file type

use std::env;
use std::fs;
use std::io::{self, Write};
use std::process;

fn detect_type(data: &[u8]) -> &'static str {
    if data.is_empty() {
        return "empty";
    }

    // Check magic bytes
    if data.len() >= 8 && data[..8] == [0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A] {
        return "PNG image data";
    }
    if data.len() >= 3 && data[..2] == [0xFF, 0xD8] {
        return "JPEG image data";
    }
    if data.len() >= 6 && (data[..6] == *b"GIF87a" || data[..6] == *b"GIF89a") {
        return "GIF image data";
    }
    if data.len() >= 5 && data[..5] == *b"%PDF-" {
        return "PDF document";
    }
    if data.len() >= 2 && data[..2] == [0x1F, 0x8B] {
        return "gzip compressed data";
    }
    if data.len() >= 5 && data[..5] == *b"ustar" {
        return "POSIX tar archive";
    }
    if data.len() >= 265 && data[257..262] == *b"ustar" {
        return "POSIX tar archive";
    }
    if data.len() >= 4 && data[..4] == [0x50, 0x4B, 0x03, 0x04] {
        return "Zip archive data";
    }
    if data.len() >= 4 && data[..4] == [0x7F, 0x45, 0x4C, 0x46] {
        return "ELF";
    }
    if data.len() >= 4 && data[..4] == [0x00, 0x61, 0x73, 0x6D] {
        return "WebAssembly (wasm) binary module";
    }
    if data.len() >= 16 && data[..16] == *b"SQLite format 3\0" {
        return "SQLite 3.x database";
    }

    // Text-based detection
    let check_len = data.len().min(512);
    let prefix = &data[..check_len];

    // Check for XML
    if prefix.starts_with(b"<?xml") || prefix.starts_with(b"<\xef\xbb\xbf<?xml") {
        return "XML document";
    }
    // Check for HTML
    let lower: Vec<u8> = prefix.iter().map(|b| b.to_ascii_lowercase()).collect();
    let lower_str = String::from_utf8_lossy(&lower);
    if lower_str.contains("<!doctype html") || lower_str.contains("<html") {
        return "HTML document";
    }
    // Check for shebang
    if data.len() >= 2 && data[..2] == *b"#!" {
        let first_line_end = data
            .iter()
            .position(|&b| b == b'\n')
            .unwrap_or(check_len.min(data.len()));
        let first_line = String::from_utf8_lossy(&data[..first_line_end]);
        if first_line.contains("python") {
            return "Python script, ASCII text executable";
        }
        if first_line.contains("node") || first_line.contains("deno") || first_line.contains("bun")
        {
            return "JavaScript text executable";
        }
        return "POSIX shell script, ASCII text executable";
    }
    // Check for JSON (skip whitespace then check for { or [)
    let trimmed = prefix
        .iter()
        .copied()
        .skip_while(|b| b.is_ascii_whitespace());
    let first_byte = trimmed.clone().next();
    if first_byte == Some(b'{') || first_byte == Some(b'[') {
        // Verify it looks like JSON (all printable)
        let all_text = prefix
            .iter()
            .all(|&b| b.is_ascii_graphic() || b.is_ascii_whitespace());
        if all_text {
            return "JSON text data";
        }
    }

    // Check if all bytes are printable ASCII text
    let is_text = data
        .iter()
        .all(|&b| b.is_ascii_graphic() || b.is_ascii_whitespace());
    if is_text {
        return "ASCII text";
    }

    // Check UTF-8 text
    if std::str::from_utf8(data).is_ok() {
        return "UTF-8 Unicode text";
    }

    "data"
}

fn main() {
    let args: Vec<String> = env::args().collect();

    if args.iter().any(|a| a == "--help") {
        println!("Usage: file FILE...");
        println!("Determine file type.");
        return;
    }

    let files: Vec<&str> = args
        .iter()
        .skip(1)
        .filter(|a| !a.starts_with('-'))
        .map(|s| s.as_str())
        .collect();

    if files.is_empty() {
        eprintln!("file: missing operand");
        process::exit(1);
    }

    let stdout = io::stdout();
    let mut out = stdout.lock();

    for file in &files {
        let data = match fs::read(file) {
            Ok(d) => d,
            Err(e) => {
                eprintln!("file: {file}: {e}");
                process::exit(1);
            }
        };

        let file_type = detect_type(&data);
        let _ = writeln!(out, "{file}: {file_type}");
    }
}
