use std::env;
use std::process;

/// Match STRING against anchored BRE REGEX.
/// Returns (output_string, matched). If the pattern has \( \) group,
/// output is the captured text; otherwise output is the match length.
fn expr_match(string: &str, pattern: &str) -> (String, bool) {
    // Convert BRE to ERE: \( → (, \) → ), \+ → +, etc.
    let mut ere = String::new();
    let chars: Vec<char> = pattern.chars().collect();
    let mut i = 0;
    let mut has_group = false;
    while i < chars.len() {
        if chars[i] == '\\' && i + 1 < chars.len() {
            match chars[i + 1] {
                '(' => {
                    ere.push('(');
                    has_group = true;
                }
                ')' => ere.push(')'),
                '+' => ere.push('+'),
                '?' => ere.push('?'),
                '{' => ere.push('{'),
                '}' => ere.push('}'),
                '|' => ere.push('|'),
                other => {
                    ere.push('\\');
                    ere.push(other);
                }
            }
            i += 2;
        } else {
            ere.push(chars[i]);
            i += 1;
        }
    }

    // Use the same simple_regex_match that works for our grep/sed
    // Pattern is anchored at start
    let full_re = format!("^{ere}");
    match simple_regex_match(string, &full_re) {
        Some((full_match, group1)) => {
            if has_group {
                let g = group1.unwrap_or_default();
                let matched = !g.is_empty();
                (g, matched)
            } else {
                let len = full_match.len();
                (len.to_string(), len > 0)
            }
        }
        None => {
            if has_group {
                (String::new(), false)
            } else {
                ("0".to_string(), false)
            }
        }
    }
}

/// Minimal regex matcher for expr. Returns (full_match, optional_group1).
/// Supports: . * + ? ^ [] [^] \d \w and one capture group.
fn simple_regex_match(input: &str, pattern: &str) -> Option<(String, Option<String>)> {
    let input_chars: Vec<char> = input.chars().collect();
    let pat_chars: Vec<char> = pattern.chars().collect();

    let mut best_end = None;
    let mut group_range: Option<(usize, usize)> = None;

    fn try_match(
        pat: &[char],
        pi: usize,
        input: &[char],
        ii: usize,
        group: &mut Option<(usize, usize)>,
        group_start: Option<usize>,
    ) -> Option<usize> {
        if pi >= pat.len() {
            return Some(ii);
        }

        let c = pat[pi];

        // Handle ^ anchor
        if c == '^' {
            return try_match(pat, pi + 1, input, ii, group, group_start);
        }

        // Handle ( for capture group
        if c == '(' {
            return try_match(pat, pi + 1, input, ii, group, Some(ii));
        }

        // Handle ) for capture group
        if c == ')' {
            if let Some(start) = group_start {
                *group = Some((start, ii));
            }
            return try_match(pat, pi + 1, input, ii, group, None);
        }

        // Check for quantifier after current atom
        let (atom_len, matches_char): (usize, Box<dyn Fn(char) -> bool>) =
            if c == '\\' && pi + 1 < pat.len() {
                match pat[pi + 1] {
                    'd' => (2, Box::new(|ch: char| ch.is_ascii_digit())),
                    'w' => (
                        2,
                        Box::new(|ch: char| ch.is_ascii_alphanumeric() || ch == '_'),
                    ),
                    's' => (2, Box::new(|ch: char| ch.is_ascii_whitespace())),
                    lit => (2, Box::new(move |ch: char| ch == lit)),
                }
            } else if c == '.' {
                (1, Box::new(|_: char| true))
            } else if c == '[' {
                // Parse character class
                let mut j = pi + 1;
                let negated = j < pat.len() && pat[j] == '^';
                if negated {
                    j += 1;
                }
                let mut ranges = Vec::new();
                while j < pat.len() && pat[j] != ']' {
                    let start = pat[j];
                    j += 1;
                    if j + 1 < pat.len() && pat[j] == '-' && pat[j + 1] != ']' {
                        let end = pat[j + 1];
                        ranges.push((start, end));
                        j += 2;
                    } else {
                        ranges.push((start, start));
                    }
                }
                if j < pat.len() {
                    j += 1;
                } // skip ]
                let class_len = j - pi;
                (
                    class_len,
                    Box::new(move |ch: char| {
                        let in_class = ranges.iter().any(|&(s, e)| ch >= s && ch <= e);
                        in_class != negated
                    }),
                )
            } else {
                (1, Box::new(move |ch: char| ch == c))
            };

        let next_pi = pi + atom_len;
        let quantifier = if next_pi < pat.len() {
            pat[next_pi]
        } else {
            '\0'
        };

        match quantifier {
            '*' => {
                // Greedy: try matching as many as possible, then backtrack
                let quant_next = next_pi + 1;
                let mut count = 0;
                while ii + count < input.len() && matches_char(input[ii + count]) {
                    count += 1;
                }
                // Try from longest to shortest
                for k in (0..=count).rev() {
                    let mut g = *group;
                    if let Some(end) =
                        try_match(pat, quant_next, input, ii + k, &mut g, group_start)
                    {
                        *group = g;
                        return Some(end);
                    }
                }
                None
            }
            '+' => {
                let quant_next = next_pi + 1;
                let mut count = 0;
                while ii + count < input.len() && matches_char(input[ii + count]) {
                    count += 1;
                }
                if count == 0 {
                    return None;
                }
                for k in (1..=count).rev() {
                    let mut g = *group;
                    if let Some(end) =
                        try_match(pat, quant_next, input, ii + k, &mut g, group_start)
                    {
                        *group = g;
                        return Some(end);
                    }
                }
                None
            }
            '?' => {
                let quant_next = next_pi + 1;
                // Try with match first
                if ii < input.len() && matches_char(input[ii]) {
                    let mut g = *group;
                    if let Some(end) =
                        try_match(pat, quant_next, input, ii + 1, &mut g, group_start)
                    {
                        *group = g;
                        return Some(end);
                    }
                }
                // Try without match
                try_match(pat, quant_next, input, ii, group, group_start)
            }
            _ => {
                // No quantifier — must match exactly one
                if ii < input.len() && matches_char(input[ii]) {
                    try_match(pat, next_pi, input, ii + 1, group, group_start)
                } else {
                    None
                }
            }
        }
    }

    let mut gr: Option<(usize, usize)> = None;
    if let Some(end) = try_match(&pat_chars, 0, &input_chars, 0, &mut gr, None) {
        best_end = Some(end);
        group_range = gr;
    }

    best_end.map(|end| {
        let full: String = input_chars[..end].iter().collect();
        let group1 = group_range.map(|(s, e)| input_chars[s..e].iter().collect());
        (full, group1)
    })
}

fn main() {
    let args: Vec<String> = env::args().skip(1).collect();
    if args.is_empty() {
        eprintln!("expr: missing operand");
        process::exit(2);
    }

    // Handle "length STRING"
    if args.len() == 2 && args[0] == "length" {
        println!("{}", args[1].len());
        return;
    }

    // Handle "match STRING REGEX" (GNU extension)
    if args.len() == 3 && args[0] == "match" {
        let (result, matched) = expr_match(&args[1], &args[2]);
        println!("{result}");
        if !matched {
            process::exit(1);
        }
        return;
    }

    // Handle "substr STRING POS LENGTH"
    if args.len() == 4 && args[0] == "substr" {
        let pos: usize = args[2].parse().unwrap_or(0);
        let len: usize = args[3].parse().unwrap_or(0);
        if pos == 0 || len == 0 || pos > args[1].len() {
            println!();
            process::exit(1);
        }
        let substr: String = args[1].chars().skip(pos - 1).take(len).collect();
        if substr.is_empty() {
            println!();
            process::exit(1);
        }
        println!("{substr}");
        return;
    }

    // Handle "index STRING CHARS"
    if args.len() == 3 && args[0] == "index" {
        let search_chars: Vec<char> = args[2].chars().collect();
        for (i, ch) in args[1].chars().enumerate() {
            if search_chars.contains(&ch) {
                println!("{}", i + 1);
                return;
            }
        }
        println!("0");
        process::exit(1);
    }

    // Handle binary operations: expr A OP B
    if args.len() == 3 {
        let left = &args[0];
        let op = &args[1];
        let right = &args[2];

        // Handle : (match) operator
        if op == ":" {
            let (result, matched) = expr_match(left, right);
            println!("{result}");
            if !matched {
                process::exit(1);
            }
            return;
        }

        // Try integer operations
        if let (Ok(l), Ok(r)) = (left.parse::<i64>(), right.parse::<i64>()) {
            let result = match op.as_str() {
                "+" => l + r,
                "-" => l - r,
                "*" => l * r,
                "/" => {
                    if r == 0 {
                        eprintln!("expr: division by zero");
                        process::exit(2);
                    }
                    l / r
                }
                "%" => {
                    if r == 0 {
                        eprintln!("expr: division by zero");
                        process::exit(2);
                    }
                    l % r
                }
                "<" => {
                    if l < r {
                        1
                    } else {
                        0
                    }
                }
                "<=" => {
                    if l <= r {
                        1
                    } else {
                        0
                    }
                }
                ">" => {
                    if l > r {
                        1
                    } else {
                        0
                    }
                }
                ">=" => {
                    if l >= r {
                        1
                    } else {
                        0
                    }
                }
                "=" => {
                    if l == r {
                        1
                    } else {
                        0
                    }
                }
                "!=" => {
                    if l != r {
                        1
                    } else {
                        0
                    }
                }
                _ => {
                    eprintln!("expr: unknown operator: {op}");
                    process::exit(2);
                }
            };
            println!("{result}");
            if result == 0 {
                process::exit(1);
            }
            return;
        }

        // String comparison
        let result = match op.as_str() {
            "=" => {
                if left == right {
                    1
                } else {
                    0
                }
            }
            "!=" => {
                if left != right {
                    1
                } else {
                    0
                }
            }
            _ => {
                eprintln!("expr: non-integer argument");
                process::exit(2);
            }
        };
        println!("{result}");
        if result == 0 {
            process::exit(1);
        }
        return;
    }

    // Single arg: print it (non-zero string = true)
    if args.len() == 1 {
        println!("{}", args[0]);
        if args[0].is_empty() || args[0] == "0" {
            process::exit(1);
        }
        return;
    }

    eprintln!("expr: syntax error");
    process::exit(2);
}
