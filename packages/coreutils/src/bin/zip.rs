//! zip - create zip archives (store method, no compression)

use std::env;
use std::fs;
use std::io::{self, Write};
use std::path::Path;
use std::process;

struct LocalFileHeader {
    name: String,
    data: Vec<u8>,
    crc32: u32,
}

fn crc32(data: &[u8]) -> u32 {
    let mut crc: u32 = 0xFFFFFFFF;
    for &byte in data {
        crc ^= byte as u32;
        for _ in 0..8 {
            if crc & 1 != 0 {
                crc = (crc >> 1) ^ 0xEDB88320;
            } else {
                crc >>= 1;
            }
        }
    }
    !crc
}

fn write_u16_le(out: &mut Vec<u8>, val: u16) {
    out.extend_from_slice(&val.to_le_bytes());
}

fn write_u32_le(out: &mut Vec<u8>, val: u32) {
    out.extend_from_slice(&val.to_le_bytes());
}

fn collect_files(path: &Path, recursive: bool) -> Vec<String> {
    let mut files = Vec::new();
    if path.is_file() {
        files.push(path.to_string_lossy().to_string());
    } else if path.is_dir() && recursive {
        if let Ok(entries) = fs::read_dir(path) {
            let mut entries: Vec<_> = entries.filter_map(|e| e.ok()).collect();
            entries.sort_by_key(|e| e.file_name());
            for entry in entries {
                let child_path = entry.path();
                if child_path.is_dir() {
                    files.extend(collect_files(&child_path, true));
                } else {
                    files.push(child_path.to_string_lossy().to_string());
                }
            }
        }
    }
    files
}

fn main() {
    let args: Vec<String> = env::args().collect();

    if args.iter().any(|a| a == "--help") {
        println!("Usage: zip [OPTIONS] ARCHIVE FILE...");
        println!("Create zip archives (store method).");
        println!("  -r  Recurse into directories");
        return;
    }

    let mut recursive = false;
    let mut positional: Vec<String> = Vec::new();

    for arg in args.iter().skip(1) {
        if arg == "-r" {
            recursive = true;
        } else if arg == "--" {
            continue;
        } else if arg.starts_with('-') && arg.len() > 1 {
            eprintln!("zip: unknown option: {arg}");
            process::exit(1);
        } else {
            positional.push(arg.clone());
        }
    }

    if positional.len() < 2 {
        eprintln!("zip: usage: zip [OPTIONS] ARCHIVE FILE...");
        process::exit(1);
    }

    let archive_path = &positional[0];
    let input_paths = &positional[1..];

    // Collect all files to add
    let mut all_files: Vec<String> = Vec::new();
    for path_str in input_paths {
        let p = Path::new(path_str);
        if !p.exists() {
            eprintln!("zip: {path_str}: No such file or directory");
            process::exit(1);
        }
        all_files.extend(collect_files(p, recursive));
    }

    // Read all file data
    let mut entries: Vec<LocalFileHeader> = Vec::new();
    for file_path in &all_files {
        let data = match fs::read(file_path) {
            Ok(d) => d,
            Err(e) => {
                eprintln!("zip: {file_path}: {e}");
                process::exit(1);
            }
        };
        let crc = crc32(&data);
        // Strip leading / for archive paths
        let name = if let Some(stripped) = file_path.strip_prefix('/') {
            stripped.to_string()
        } else {
            file_path.clone()
        };
        entries.push(LocalFileHeader {
            name,
            data,
            crc32: crc,
        });
    }

    // Build zip file
    let mut output: Vec<u8> = Vec::new();
    let mut offsets: Vec<u32> = Vec::new();

    // Write local file headers + data
    for entry in &entries {
        offsets.push(output.len() as u32);

        // Local file header signature
        write_u32_le(&mut output, 0x04034b50);
        // Version needed to extract (2.0)
        write_u16_le(&mut output, 20);
        // General purpose bit flag
        write_u16_le(&mut output, 0);
        // Compression method: 0 = store
        write_u16_le(&mut output, 0);
        // Last mod time
        write_u16_le(&mut output, 0);
        // Last mod date
        write_u16_le(&mut output, 0);
        // CRC-32
        write_u32_le(&mut output, entry.crc32);
        // Compressed size
        write_u32_le(&mut output, entry.data.len() as u32);
        // Uncompressed size
        write_u32_le(&mut output, entry.data.len() as u32);
        // File name length
        write_u16_le(&mut output, entry.name.len() as u16);
        // Extra field length
        write_u16_le(&mut output, 0);
        // File name
        output.extend_from_slice(entry.name.as_bytes());
        // File data
        output.extend_from_slice(&entry.data);
    }

    // Central directory
    let cd_offset = output.len() as u32;
    let mut cd_size: u32 = 0;

    for (i, entry) in entries.iter().enumerate() {
        let start = output.len();
        // Central directory header signature
        write_u32_le(&mut output, 0x02014b50);
        // Version made by
        write_u16_le(&mut output, 20);
        // Version needed to extract
        write_u16_le(&mut output, 20);
        // General purpose bit flag
        write_u16_le(&mut output, 0);
        // Compression method: store
        write_u16_le(&mut output, 0);
        // Last mod time
        write_u16_le(&mut output, 0);
        // Last mod date
        write_u16_le(&mut output, 0);
        // CRC-32
        write_u32_le(&mut output, entry.crc32);
        // Compressed size
        write_u32_le(&mut output, entry.data.len() as u32);
        // Uncompressed size
        write_u32_le(&mut output, entry.data.len() as u32);
        // File name length
        write_u16_le(&mut output, entry.name.len() as u16);
        // Extra field length
        write_u16_le(&mut output, 0);
        // File comment length
        write_u16_le(&mut output, 0);
        // Disk number start
        write_u16_le(&mut output, 0);
        // Internal file attributes
        write_u16_le(&mut output, 0);
        // External file attributes
        write_u32_le(&mut output, 0);
        // Relative offset of local header
        write_u32_le(&mut output, offsets[i]);
        // File name
        output.extend_from_slice(entry.name.as_bytes());

        cd_size += (output.len() - start) as u32;
    }

    // End of central directory
    write_u32_le(&mut output, 0x06054b50);
    // Disk number
    write_u16_le(&mut output, 0);
    // Disk with central directory
    write_u16_le(&mut output, 0);
    // Number of entries on this disk
    write_u16_le(&mut output, entries.len() as u16);
    // Total number of entries
    write_u16_le(&mut output, entries.len() as u16);
    // Size of central directory
    write_u32_le(&mut output, cd_size);
    // Offset of central directory
    write_u32_le(&mut output, cd_offset);
    // Comment length
    write_u16_le(&mut output, 0);

    if let Err(e) = fs::write(archive_path, &output) {
        eprintln!("zip: {archive_path}: {e}");
        process::exit(1);
    }

    let stderr = io::stderr();
    let mut err = stderr.lock();
    for entry in &entries {
        let _ = writeln!(err, "  adding: {}", entry.name);
    }
}
