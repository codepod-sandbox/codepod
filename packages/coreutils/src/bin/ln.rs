use std::env;
use std::process;

fn main() {
    let args: Vec<String> = env::args().skip(1).collect();
    let mut symbolic = false;
    let mut force = false;
    let mut paths: Vec<&str> = Vec::new();

    for arg in &args {
        match arg.as_str() {
            "-s" | "-sf" | "-fs" => {
                symbolic = true;
                if arg.contains('f') {
                    force = true;
                }
            }
            "-f" => force = true,
            a if a.starts_with('-') => {
                for ch in a[1..].chars() {
                    match ch {
                        's' => symbolic = true,
                        'f' => force = true,
                        'n' => {} // no-dereference, ignore
                        _ => {}
                    }
                }
            }
            _ => paths.push(arg.as_str()),
        }
    }

    if paths.len() != 2 {
        eprintln!("ln: usage: ln [-sf] SOURCE DEST");
        process::exit(1);
    }

    let target = paths[0];
    let link_name = paths[1];

    if force {
        let _ = std::fs::remove_file(link_name);
    }

    if symbolic {
        match create_symlink(target, link_name) {
            Ok(_) => {}
            Err(e) => {
                eprintln!("ln: {}: {e}", target);
                process::exit(1);
            }
        }
    } else {
        match std::fs::copy(target, link_name) {
            Ok(_) => {}
            Err(e) => {
                eprintln!("ln: {}: {e}", target);
                process::exit(1);
            }
        }
    }
}

fn create_symlink(target: &str, link_name: &str) -> Result<(), std::io::Error> {
    // Use raw WASI path_symlink syscall
    let ret = unsafe {
        wasi_path_symlink(
            target.as_ptr(),
            target.len(),
            3, // fd 3 is the preopened root dir
            link_name.as_ptr(),
            link_name.len(),
        )
    };
    if ret == 0 {
        Ok(())
    } else {
        Err(std::io::Error::from_raw_os_error(ret))
    }
}

#[link(wasm_import_module = "wasi_snapshot_preview1")]
extern "C" {
    #[link_name = "path_symlink"]
    fn wasi_path_symlink(
        old_path: *const u8,
        old_path_len: usize,
        fd: i32,
        new_path: *const u8,
        new_path_len: usize,
    ) -> i32;
}
