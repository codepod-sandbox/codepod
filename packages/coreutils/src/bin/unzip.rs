//! unzip - extract zip archives

use std::env;
use std::fs;
use std::io::{self, Read, Write};
use std::path::PathBuf;
use std::process;

use flate2::read::DeflateDecoder;

struct ZipEntry {
    name: String,
    method: u16,
    compressed_size: u32,
    uncompressed_size: u32,
    offset: u32,
}

fn read_u16_le(data: &[u8], offset: usize) -> u16 {
    u16::from_le_bytes([data[offset], data[offset + 1]])
}

fn read_u32_le(data: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes([
        data[offset],
        data[offset + 1],
        data[offset + 2],
        data[offset + 3],
    ])
}

fn find_eocd(data: &[u8]) -> Option<usize> {
    // End of central directory signature: 0x06054b50
    if data.len() < 22 {
        return None;
    }
    let start = if data.len() > 65557 {
        data.len() - 65557
    } else {
        0
    };
    (start..=data.len() - 22)
        .rev()
        .find(|&i| read_u32_le(data, i) == 0x06054b50)
}

fn parse_central_directory(data: &[u8]) -> Result<Vec<ZipEntry>, String> {
    let eocd_pos = find_eocd(data).ok_or("not a zip file: end of central directory not found")?;

    let num_entries = read_u16_le(data, eocd_pos + 10) as usize;
    let cd_offset = read_u32_le(data, eocd_pos + 16) as usize;

    let mut entries = Vec::new();
    let mut pos = cd_offset;

    for _ in 0..num_entries {
        if pos + 46 > data.len() {
            return Err("truncated central directory".to_string());
        }

        let sig = read_u32_le(data, pos);
        if sig != 0x02014b50 {
            return Err("invalid central directory header".to_string());
        }

        let method = read_u16_le(data, pos + 10);
        let compressed_size = read_u32_le(data, pos + 20);
        let uncompressed_size = read_u32_le(data, pos + 24);
        let name_len = read_u16_le(data, pos + 28) as usize;
        let extra_len = read_u16_le(data, pos + 30) as usize;
        let comment_len = read_u16_le(data, pos + 32) as usize;
        let local_offset = read_u32_le(data, pos + 42);

        if pos + 46 + name_len > data.len() {
            return Err("truncated file name in central directory".to_string());
        }

        let name = String::from_utf8_lossy(&data[pos + 46..pos + 46 + name_len]).to_string();

        entries.push(ZipEntry {
            name,
            method,
            compressed_size,
            uncompressed_size,
            offset: local_offset,
        });

        pos += 46 + name_len + extra_len + comment_len;
    }

    Ok(entries)
}

fn extract_entry(data: &[u8], entry: &ZipEntry) -> Result<Vec<u8>, String> {
    let offset = entry.offset as usize;

    if offset + 30 > data.len() {
        return Err("truncated local file header".to_string());
    }

    let sig = read_u32_le(data, offset);
    if sig != 0x04034b50 {
        return Err("invalid local file header".to_string());
    }

    let name_len = read_u16_le(data, offset + 26) as usize;
    let extra_len = read_u16_le(data, offset + 28) as usize;
    let data_start = offset + 30 + name_len + extra_len;
    let data_end = data_start + entry.compressed_size as usize;

    if data_end > data.len() {
        return Err("truncated file data".to_string());
    }

    let compressed = &data[data_start..data_end];

    match entry.method {
        0 => {
            // Store
            Ok(compressed.to_vec())
        }
        8 => {
            // Deflate
            let mut decoder = DeflateDecoder::new(compressed);
            let mut result = Vec::with_capacity(entry.uncompressed_size as usize);
            decoder
                .read_to_end(&mut result)
                .map_err(|e| format!("deflate error: {e}"))?;
            Ok(result)
        }
        _ => Err(format!("unsupported compression method: {}", entry.method)),
    }
}

fn list_entries(entries: &[ZipEntry]) {
    let stdout = io::stdout();
    let mut out = stdout.lock();
    let _ = writeln!(out, "  Length      Method   Compressed  Name");
    let _ = writeln!(out, "---------  ----------  ----------  ----");

    let mut total_size: u64 = 0;
    let mut total_compressed: u64 = 0;

    for entry in entries {
        let method = match entry.method {
            0 => "Stored",
            8 => "Deflated",
            _ => "Unknown",
        };
        let _ = writeln!(
            out,
            "{:>9}  {:>10}  {:>10}  {}",
            entry.uncompressed_size, method, entry.compressed_size, entry.name
        );
        total_size += entry.uncompressed_size as u64;
        total_compressed += entry.compressed_size as u64;
    }

    let _ = writeln!(out, "---------              ----------  -------");
    let _ = writeln!(
        out,
        "{:>9}              {:>10}  {} files",
        total_size,
        total_compressed,
        entries.len()
    );
}

fn main() {
    let args: Vec<String> = env::args().collect();

    if args.iter().any(|a| a == "--help") {
        println!("Usage: unzip [OPTIONS] ARCHIVE");
        println!("Extract files from a zip archive.");
        println!("  -l      List contents");
        println!("  -d DIR  Extract to directory");
        return;
    }

    let mut list_mode = false;
    let mut dest_dir: Option<String> = None;
    let mut archive: Option<String> = None;
    let mut i = 1;

    while i < args.len() {
        if args[i] == "-l" {
            list_mode = true;
        } else if args[i] == "-d" {
            i += 1;
            if i >= args.len() {
                eprintln!("unzip: option requires an argument -- 'd'");
                process::exit(1);
            }
            dest_dir = Some(args[i].clone());
        } else if !args[i].starts_with('-') {
            archive = Some(args[i].clone());
        }
        i += 1;
    }

    let archive = match archive {
        Some(a) => a,
        None => {
            eprintln!("unzip: missing archive operand");
            process::exit(1);
        }
    };

    let data = match fs::read(&archive) {
        Ok(d) => d,
        Err(e) => {
            eprintln!("unzip: {archive}: {e}");
            process::exit(1);
        }
    };

    let entries = match parse_central_directory(&data) {
        Ok(e) => e,
        Err(e) => {
            eprintln!("unzip: {archive}: {e}");
            process::exit(1);
        }
    };

    if list_mode {
        list_entries(&entries);
        return;
    }

    let base_dir = dest_dir.unwrap_or_else(|| ".".to_string());

    for entry in &entries {
        // Skip directory entries
        if entry.name.ends_with('/') {
            let dir_path = PathBuf::from(&base_dir).join(&entry.name);
            if let Err(e) = fs::create_dir_all(&dir_path) {
                eprintln!("unzip: {}: {e}", entry.name);
                process::exit(1);
            }
            continue;
        }

        let file_data = match extract_entry(&data, entry) {
            Ok(d) => d,
            Err(e) => {
                eprintln!("unzip: {}: {e}", entry.name);
                process::exit(1);
            }
        };

        let out_path = PathBuf::from(&base_dir).join(&entry.name);

        // Create parent directories
        if let Some(parent) = out_path.parent() {
            if !parent.exists() {
                if let Err(e) = fs::create_dir_all(parent) {
                    eprintln!("unzip: {}: {e}", parent.display());
                    process::exit(1);
                }
            }
        }

        if let Err(e) = fs::write(&out_path, &file_data) {
            eprintln!("unzip: {}: {e}", out_path.display());
            process::exit(1);
        }

        let stderr = io::stderr();
        let mut err = stderr.lock();
        let _ = writeln!(err, "  extracting: {}", entry.name);
    }
}
