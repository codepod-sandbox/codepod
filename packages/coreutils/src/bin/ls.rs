use std::env;
use std::fs;
use std::path::Path;
use std::process;

struct Options {
    long: bool,
    all: bool,
    one_per_line: bool,
    recursive: bool,
}

fn parse_args() -> (Options, Vec<String>) {
    let mut opts = Options {
        long: false,
        all: false,
        one_per_line: false,
        recursive: false,
    };
    let mut paths = Vec::new();

    for arg in env::args().skip(1) {
        if arg == "--" {
            break;
        }
        if arg.starts_with('-') && arg.len() > 1 {
            for ch in arg[1..].chars() {
                match ch {
                    'l' => opts.long = true,
                    'a' => opts.all = true,
                    '1' => opts.one_per_line = true,
                    'R' => opts.recursive = true,
                    _ => {
                        eprintln!("ls: invalid option -- '{}'", ch);
                        process::exit(2);
                    }
                }
            }
        } else {
            paths.push(arg);
        }
    }

    if paths.is_empty() {
        paths.push(".".to_string());
    }

    (opts, paths)
}

fn format_size(size: u64) -> String {
    format!("{:>8}", size)
}

fn format_time(modified: std::io::Result<std::time::SystemTime>) -> String {
    match modified {
        Ok(time) => {
            match time.duration_since(std::time::UNIX_EPOCH) {
                Ok(dur) => {
                    let secs = dur.as_secs();
                    // Simple date formatting: compute year, month, day, hour, minute
                    let days = secs / 86400;
                    let time_of_day = secs % 86400;
                    let hour = time_of_day / 3600;
                    let minute = (time_of_day % 3600) / 60;

                    // Days since epoch to date (simplified)
                    let mut y = 1970i64;
                    let mut remaining = days as i64;
                    loop {
                        let days_in_year = if is_leap(y) { 366 } else { 365 };
                        if remaining < days_in_year {
                            break;
                        }
                        remaining -= days_in_year;
                        y += 1;
                    }
                    let month_days: [i64; 12] = if is_leap(y) {
                        [31, 29, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
                    } else {
                        [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
                    };
                    let mut m = 0usize;
                    for (i, &md) in month_days.iter().enumerate() {
                        if remaining < md {
                            m = i;
                            break;
                        }
                        remaining -= md;
                    }
                    let day = remaining + 1;
                    let months = [
                        "Jan", "Feb", "Mar", "Apr", "May", "Jun",
                        "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
                    ];
                    format!("{} {:>2} {:02}:{:02}", months[m], day, hour, minute)
                }
                Err(_) => "            ".to_string(),
            }
        }
        Err(_) => "            ".to_string(),
    }
}

fn is_leap(y: i64) -> bool {
    (y % 4 == 0 && y % 100 != 0) || y % 400 == 0
}

fn file_type_char(metadata: &fs::Metadata) -> char {
    if metadata.is_dir() {
        'd'
    } else if metadata.is_symlink() {
        'l'
    } else {
        '-'
    }
}

fn permissions_str(metadata: &fs::Metadata) -> String {
    let ft = file_type_char(metadata);
    // WASI doesn't expose Unix permissions in a portable way,
    // so we show a placeholder based on file type.
    let perms = if metadata.is_dir() {
        "rwxr-xr-x"
    } else {
        "rw-r--r--"
    };
    format!("{}{}", ft, perms)
}

fn list_dir(path: &Path, opts: &Options, show_header: bool) -> i32 {
    let mut exit_code = 0;

    if show_header {
        println!("{}:", path.display());
    }

    let entries = match fs::read_dir(path) {
        Ok(entries) => entries,
        Err(e) => {
            eprintln!("ls: cannot access '{}': {}", path.display(), e);
            return 1;
        }
    };

    let mut names: Vec<(String, fs::Metadata)> = Vec::new();
    for entry in entries {
        match entry {
            Ok(entry) => {
                let name = entry.file_name().to_string_lossy().to_string();
                if !opts.all && name.starts_with('.') {
                    continue;
                }
                let metadata = entry.metadata().unwrap_or_else(|_| {
                    // Fallback to symlink metadata
                    fs::symlink_metadata(entry.path()).unwrap()
                });
                names.push((name, metadata));
            }
            Err(e) => {
                eprintln!("ls: error reading entry: {}", e);
                exit_code = 1;
            }
        }
    }

    names.sort_by(|a, b| a.0.to_lowercase().cmp(&b.0.to_lowercase()));

    if opts.long {
        for (name, metadata) in &names {
            let perms = permissions_str(metadata);
            let size = format_size(metadata.len());
            let time = format_time(metadata.modified());
            println!("{} {} {} {}", perms, size, time, name);
        }
    } else if opts.one_per_line {
        for (name, _) in &names {
            println!("{}", name);
        }
    } else {
        // Simple space-separated output
        let name_list: Vec<&str> = names.iter().map(|(n, _)| n.as_str()).collect();
        if !name_list.is_empty() {
            println!("{}", name_list.join("  "));
        }
    }

    if opts.recursive {
        for (name, metadata) in &names {
            if metadata.is_dir() && name != "." && name != ".." {
                println!();
                let sub = path.join(name);
                let code = list_dir(&sub, opts, true);
                if code != 0 {
                    exit_code = code;
                }
            }
        }
    }

    exit_code
}

fn main() {
    let (opts, paths) = parse_args();
    let mut exit_code = 0;
    let show_header = paths.len() > 1 || opts.recursive;

    for (i, p) in paths.iter().enumerate() {
        let path = Path::new(p);

        if !path.exists() {
            eprintln!("ls: cannot access '{}': No such file or directory", p);
            exit_code = 1;
            continue;
        }

        if path.is_file() {
            if opts.long {
                let metadata = fs::metadata(path).unwrap();
                let perms = permissions_str(&metadata);
                let size = format_size(metadata.len());
                let time = format_time(metadata.modified());
                println!("{} {} {} {}", perms, size, time, p);
            } else {
                println!("{}", p);
            }
            continue;
        }

        if i > 0 {
            println!();
        }
        let code = list_dir(path, &opts, show_header);
        if code != 0 {
            exit_code = code;
        }
    }

    process::exit(exit_code);
}
