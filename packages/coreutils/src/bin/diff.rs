use std::env;
use std::fs;
use std::io::{self, Read, Write};
use std::process;

// ---------------------------------------------------------------------------
// Options
// ---------------------------------------------------------------------------

struct Options {
    unified: bool,
    context_lines: usize, // default 3 for -u
    brief: bool,
    ignore_all_space: bool,
    ignore_space_change: bool,
    ignore_blank_lines: bool,
    ignore_case: bool,
    label1: Option<String>,
    label2: Option<String>,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            unified: false,
            context_lines: 3,
            brief: false,
            ignore_all_space: false,
            ignore_space_change: false,
            ignore_blank_lines: false,
            ignore_case: false,
            label1: None,
            label2: None,
        }
    }
}

// ---------------------------------------------------------------------------
// Edit operations from Myers diff
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq)]
enum Op {
    Equal,
    Delete, // line from file1 only
    Insert, // line from file2 only
}

// ---------------------------------------------------------------------------
// Myers diff algorithm — compute shortest edit script
// ---------------------------------------------------------------------------

fn myers_diff<'a>(a: &[&'a str], b: &[&'a str], opts: &Options) -> Vec<(Op, usize, usize)> {
    let n = a.len();
    let m = b.len();

    if n == 0 && m == 0 {
        return vec![];
    }
    if n == 0 {
        return (0..m).map(|j| (Op::Insert, 0, j)).collect();
    }
    if m == 0 {
        return (0..n).map(|i| (Op::Delete, i, 0)).collect();
    }

    let max_d = n + m;
    let size = 2 * max_d + 1;
    // v[k + offset] = furthest x on diagonal k
    let offset = max_d;
    let mut v = vec![0usize; size];
    let mut trace: Vec<Vec<usize>> = Vec::new();

    'outer: for d in 0..=max_d {
        trace.push(v.clone());
        let mut new_v = v.clone();
        let d_i = d as isize;
        let mut k = -d_i;
        while k <= d_i {
            let ki = (k + offset as isize) as usize;
            let mut x = if k == -d_i
                || (k != d_i
                    && v[(k - 1 + offset as isize) as usize]
                        < v[(k + 1 + offset as isize) as usize])
            {
                v[(k + 1 + offset as isize) as usize] // move down (insert)
            } else {
                v[(k - 1 + offset as isize) as usize] + 1 // move right (delete)
            };
            let mut y = (x as isize - k) as usize;
            // Follow diagonal (equal lines)
            while x < n && y < m && lines_equal(a[x], b[y], opts) {
                x += 1;
                y += 1;
            }
            new_v[ki] = x;
            if x >= n && y >= m {
                v = new_v;
                trace.push(v.clone());
                break 'outer;
            }
            k += 2;
        }
        v = new_v;
    }

    // Backtrack to recover the edit script
    backtrack(&trace, n, m, a, b, offset, opts)
}

fn backtrack(
    trace: &[Vec<usize>],
    n: usize,
    m: usize,
    a: &[&str],
    b: &[&str],
    offset: usize,
    opts: &Options,
) -> Vec<(Op, usize, usize)> {
    let mut ops = Vec::new();
    let mut x = n;
    let mut y = m;

    for d in (0..trace.len() - 1).rev() {
        let v = &trace[d];
        let d_i = d as isize;
        let k = x as isize - y as isize;

        let prev_k = if k == -d_i
            || (k != d_i
                && v[(k - 1 + offset as isize) as usize] < v[(k + 1 + offset as isize) as usize])
        {
            k + 1
        } else {
            k - 1
        };

        let prev_x = v[(prev_k + offset as isize) as usize];
        let prev_y = (prev_x as isize - prev_k) as usize;

        // Diagonal moves (equal lines)
        while x > prev_x && y > prev_y {
            x -= 1;
            y -= 1;
            ops.push((Op::Equal, x, y));
        }

        if d > 0 {
            if x == prev_x {
                // Insert
                y -= 1;
                ops.push((Op::Insert, x, y));
            } else {
                // Delete
                x -= 1;
                ops.push((Op::Delete, x, 0));
            }
        }

        // Handle remaining diagonal at d=0
        if d == 0 {
            while x > 0 && y > 0 && lines_equal(a[x - 1], b[y - 1], opts) {
                x -= 1;
                y -= 1;
                ops.push((Op::Equal, x, y));
            }
        }
    }

    ops.reverse();
    ops
}

fn lines_equal(a: &str, b: &str, opts: &Options) -> bool {
    if opts.ignore_all_space {
        let a_stripped: String = a.chars().filter(|c| !c.is_whitespace()).collect();
        let b_stripped: String = b.chars().filter(|c| !c.is_whitespace()).collect();
        if opts.ignore_case {
            a_stripped.eq_ignore_ascii_case(&b_stripped)
        } else {
            a_stripped == b_stripped
        }
    } else if opts.ignore_space_change {
        let a_norm = normalize_whitespace(a);
        let b_norm = normalize_whitespace(b);
        if opts.ignore_case {
            a_norm.eq_ignore_ascii_case(&b_norm)
        } else {
            a_norm == b_norm
        }
    } else if opts.ignore_case {
        a.eq_ignore_ascii_case(b)
    } else {
        a == b
    }
}

fn normalize_whitespace(s: &str) -> String {
    let mut result = String::new();
    let mut in_space = false;
    for c in s.chars() {
        if c.is_whitespace() {
            if !in_space {
                result.push(' ');
                in_space = true;
            }
        } else {
            result.push(c);
            in_space = false;
        }
    }
    result.trim().to_string()
}

// ---------------------------------------------------------------------------
// Hunk grouping — group consecutive changes with context
// ---------------------------------------------------------------------------

struct Hunk {
    start1: usize, // 1-based line number in file1
    count1: usize,
    start2: usize, // 1-based line number in file2
    count2: usize,
    lines: Vec<(Op, String)>,
}

fn group_hunks(ops: &[(Op, usize, usize)], a: &[&str], b: &[&str], ctx: usize) -> Vec<Hunk> {
    if ops.is_empty() {
        return vec![];
    }

    // Find ranges of changes with context
    let mut change_ranges: Vec<(usize, usize)> = Vec::new(); // (start_idx, end_idx) in ops
    let mut i = 0;
    while i < ops.len() {
        if ops[i].0 != Op::Equal {
            let start = i;
            while i < ops.len() && ops[i].0 != Op::Equal {
                i += 1;
            }
            change_ranges.push((start, i));
        } else {
            i += 1;
        }
    }

    if change_ranges.is_empty() {
        return vec![];
    }

    // Merge nearby changes that share context
    let mut merged: Vec<(usize, usize)> = vec![change_ranges[0]];
    for &(s, e) in &change_ranges[1..] {
        let (_, prev_e) = merged.last().unwrap();
        // If the gap between changes is <= 2*ctx, merge them
        if s - prev_e <= 2 * ctx {
            merged.last_mut().unwrap().1 = e;
        } else {
            merged.push((s, e));
        }
    }

    // Build hunks with context
    let mut hunks = Vec::new();
    for (change_start, change_end) in merged {
        // Expand to include context
        let hunk_start = change_start.saturating_sub(ctx);
        let hunk_end = (change_end + ctx).min(ops.len());

        let mut lines = Vec::new();
        let mut s1 = usize::MAX;
        let mut s2 = usize::MAX;
        let mut c1 = 0usize;
        let mut c2 = 0usize;

        for &(op, ai, bi) in &ops[hunk_start..hunk_end] {
            match op {
                Op::Equal => {
                    if s1 == usize::MAX {
                        s1 = ai;
                        s2 = bi;
                    }
                    lines.push((Op::Equal, a[ai].to_string()));
                    c1 += 1;
                    c2 += 1;
                }
                Op::Delete => {
                    if s1 == usize::MAX {
                        s1 = ai;
                        // For s2, look at bi from nearby ops
                        s2 = bi;
                    }
                    lines.push((Op::Delete, a[ai].to_string()));
                    c1 += 1;
                }
                Op::Insert => {
                    if s1 == usize::MAX {
                        s1 = ai;
                        s2 = bi;
                    }
                    lines.push((Op::Insert, b[bi].to_string()));
                    c2 += 1;
                }
            }
        }

        if s1 == usize::MAX {
            s1 = 0;
        }
        if s2 == usize::MAX {
            s2 = 0;
        }

        hunks.push(Hunk {
            start1: s1 + 1, // 1-based
            count1: c1,
            start2: s2 + 1,
            count2: c2,
            lines,
        });
    }

    hunks
}

// ---------------------------------------------------------------------------
// Output formatters
// ---------------------------------------------------------------------------

fn output_unified(out: &mut dyn Write, hunks: &[Hunk], label1: &str, label2: &str) {
    let _ = writeln!(out, "--- {label1}");
    let _ = writeln!(out, "+++ {label2}");
    for hunk in hunks {
        let _ = writeln!(
            out,
            "@@ -{},{} +{},{} @@",
            hunk.start1, hunk.count1, hunk.start2, hunk.count2
        );
        for (op, line) in &hunk.lines {
            match op {
                Op::Equal => {
                    let _ = writeln!(out, " {line}");
                }
                Op::Delete => {
                    let _ = writeln!(out, "-{line}");
                }
                Op::Insert => {
                    let _ = writeln!(out, "+{line}");
                }
            }
        }
    }
}

fn output_normal(out: &mut dyn Write, ops: &[(Op, usize, usize)], a: &[&str], b: &[&str]) {
    // Group consecutive operations into change blocks
    let mut i = 0;
    while i < ops.len() {
        if ops[i].0 == Op::Equal {
            i += 1;
            continue;
        }

        // Collect consecutive deletes and inserts
        let mut deletes: Vec<usize> = Vec::new();
        let mut inserts: Vec<usize> = Vec::new();
        while i < ops.len() && ops[i].0 != Op::Equal {
            match ops[i].0 {
                Op::Delete => deletes.push(ops[i].1),
                Op::Insert => inserts.push(ops[i].2),
                _ => {}
            }
            i += 1;
        }

        // Determine the change type and line ranges
        if !deletes.is_empty() && !inserts.is_empty() {
            // Change
            let d_range = format_range(&deletes);
            let i_range = format_range(&inserts);
            let _ = writeln!(out, "{d_range}c{i_range}");
            for &d in &deletes {
                let _ = writeln!(out, "< {}", a[d]);
            }
            let _ = writeln!(out, "---");
            for &ins in &inserts {
                let _ = writeln!(out, "> {}", b[ins]);
            }
        } else if !deletes.is_empty() {
            // Delete
            let d_range = format_range(&deletes);
            let after = if !inserts.is_empty() {
                inserts[0]
            } else {
                // Find the position in b
                deletes.last().map_or(0, |&d| {
                    // Look for the nearest equal op after this delete to get b position
                    ops.iter()
                        .find(|&&(op, ai, _)| op == Op::Equal && ai > d)
                        .map_or(b.len(), |&(_, _, bi)| bi)
                })
            };
            let _ = writeln!(out, "{}d{}", d_range, after);
            for &d in &deletes {
                let _ = writeln!(out, "< {}", a[d]);
            }
        } else if !inserts.is_empty() {
            // Add
            let i_range = format_range(&inserts);
            let after = if !deletes.is_empty() {
                deletes.last().unwrap() + 1
            } else {
                // Find position in a
                inserts.first().map_or(0, |&ins| {
                    ops.iter()
                        .find(|&&(op, _, bi)| op == Op::Equal && bi > ins)
                        .map_or(a.len(), |&(_, ai, _)| ai)
                })
            };
            let _ = writeln!(out, "{}a{}", after, i_range);
            for &ins in &inserts {
                let _ = writeln!(out, "> {}", b[ins]);
            }
        }
    }
}

fn format_range(indices: &[usize]) -> String {
    if indices.len() == 1 {
        format!("{}", indices[0] + 1)
    } else {
        format!("{},{}", indices[0] + 1, indices.last().unwrap() + 1)
    }
}

// ---------------------------------------------------------------------------
// Stdin support: read from stdin when path is "-"
// ---------------------------------------------------------------------------

fn read_input(path: &str) -> Result<String, String> {
    if path == "-" {
        let mut buf = String::new();
        io::stdin()
            .read_to_string(&mut buf)
            .map_err(|e| format!("diff: stdin: {e}"))?;
        Ok(buf)
    } else {
        fs::read_to_string(path).map_err(|e| format!("diff: {path}: {e}"))
    }
}

// ---------------------------------------------------------------------------
// Main
// ---------------------------------------------------------------------------

fn main() {
    let args: Vec<String> = env::args().skip(1).collect();
    let mut opts = Options::default();
    let mut paths: Vec<String> = Vec::new();

    let mut i = 0;
    while i < args.len() {
        let arg = &args[i];
        match arg.as_str() {
            "-u" | "--unified" => {
                opts.unified = true;
            }
            "-q" | "--brief" => {
                opts.brief = true;
            }
            "-w" | "--ignore-all-space" => {
                opts.ignore_all_space = true;
            }
            "-b" | "--ignore-space-change" => {
                opts.ignore_space_change = true;
            }
            "-B" | "--ignore-blank-lines" => {
                opts.ignore_blank_lines = true;
            }
            "-i" | "--ignore-case" => {
                opts.ignore_case = true;
            }
            "--label" => {
                i += 1;
                if i < args.len() {
                    if opts.label1.is_none() {
                        opts.label1 = Some(args[i].clone());
                    } else {
                        opts.label2 = Some(args[i].clone());
                    }
                }
            }
            _ if arg.starts_with("-U") => {
                let val = &arg[2..];
                if let Ok(n) = val.parse::<usize>() {
                    opts.unified = true;
                    opts.context_lines = n;
                }
            }
            _ if arg.starts_with("--unified=") => {
                let val = &arg["--unified=".len()..];
                if let Ok(n) = val.parse::<usize>() {
                    opts.unified = true;
                    opts.context_lines = n;
                }
            }
            _ if arg.starts_with('-') && arg != "-" => {
                // Parse combined single-char flags like -ubw
                for ch in arg[1..].chars() {
                    match ch {
                        'u' => opts.unified = true,
                        'q' => opts.brief = true,
                        'w' => opts.ignore_all_space = true,
                        'b' => opts.ignore_space_change = true,
                        'B' => opts.ignore_blank_lines = true,
                        'i' => opts.ignore_case = true,
                        _ => {
                            eprintln!("diff: invalid option -- '{ch}'");
                            process::exit(2);
                        }
                    }
                }
            }
            _ => {
                paths.push(arg.clone());
            }
        }
        i += 1;
    }

    if paths.len() != 2 {
        eprintln!("diff: usage: diff [OPTIONS] FILE1 FILE2");
        process::exit(2);
    }

    let content1 = match read_input(&paths[0]) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("{e}");
            process::exit(2);
        }
    };
    let content2 = match read_input(&paths[1]) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("{e}");
            process::exit(2);
        }
    };

    let mut lines1: Vec<&str> = content1.lines().collect();
    let mut lines2: Vec<&str> = content2.lines().collect();

    // Filter blank lines if requested
    if opts.ignore_blank_lines {
        lines1.retain(|l| !l.trim().is_empty());
        lines2.retain(|l| !l.trim().is_empty());
    }

    // Quick check: identical
    if lines1.len() == lines2.len()
        && lines1
            .iter()
            .zip(lines2.iter())
            .all(|(a, b)| lines_equal(a, b, &opts))
    {
        process::exit(0);
    }

    if opts.brief {
        let label1 = opts.label1.as_deref().unwrap_or(&paths[0]);
        let label2 = opts.label2.as_deref().unwrap_or(&paths[1]);
        println!("Files {label1} and {label2} differ");
        process::exit(1);
    }

    let ops = myers_diff(&lines1, &lines2, &opts);

    // Check if there are any actual changes
    let has_changes = ops.iter().any(|&(op, _, _)| op != Op::Equal);
    if !has_changes {
        process::exit(0);
    }

    let stdout = io::stdout();
    let mut out = stdout.lock();

    if opts.unified {
        let label1 = opts.label1.as_deref().unwrap_or(&paths[0]);
        let label2 = opts.label2.as_deref().unwrap_or(&paths[1]);
        let hunks = group_hunks(&ops, &lines1, &lines2, opts.context_lines);
        output_unified(&mut out, &hunks, label1, label2);
    } else {
        output_normal(&mut out, &ops, &lines1, &lines2);
    }

    process::exit(1);
}
