//! sed - stream editor
//!
//! Supports: s/pattern/replacement/[flags], d, p, q, c, y, w, h, H, g, G, x,
//! line-number and /pattern/ addressing, ranges, negation (!), -n flag,
//! -e expressions, multiple commands via ; and { } blocks.
//!
//! Pattern matching uses a built-in BRE (Basic Regular Expression) engine.

use std::env;
use std::fs::File;
use std::io::{self, BufRead, BufReader, Read, Write};
use std::process;

// ---------------------------------------------------------------------------
// BRE regex engine
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
enum BreToken {
    Literal(char),
    Dot,
    Anchor(AnchorKind),
    Class { chars: Vec<char>, negated: bool },
    Repeat(Box<BreToken>, RepeatKind),
    Group(Vec<BreToken>, usize), // sub-pattern, group index (1-based)
}

#[derive(Debug, Clone)]
enum AnchorKind {
    Start,
    End,
}

#[derive(Debug, Clone)]
enum RepeatKind {
    Star,     // *
    Plus,     // \+
    Question, // \?
}

/// Parse a BRE pattern string into a token list.
fn parse_bre(pattern: &str) -> Vec<BreToken> {
    let chars: Vec<char> = pattern.chars().collect();
    let mut tokens = Vec::new();
    let mut group_counter: usize = 0;
    parse_bre_inner(&chars, &mut 0, &mut tokens, &mut group_counter, false);
    tokens
}

fn parse_bre_inner(
    chars: &[char],
    pos: &mut usize,
    tokens: &mut Vec<BreToken>,
    group_counter: &mut usize,
    in_group: bool,
) {
    while *pos < chars.len() {
        // Check for end-of-group
        if in_group && *pos + 1 < chars.len() && chars[*pos] == '\\' && chars[*pos + 1] == ')' {
            *pos += 2;
            return;
        }

        let ch = chars[*pos];
        match ch {
            '\\' if *pos + 1 < chars.len() => {
                let next = chars[*pos + 1];
                match next {
                    '(' => {
                        // Start of group
                        *pos += 2;
                        *group_counter += 1;
                        let gid = *group_counter;
                        let mut sub = Vec::new();
                        parse_bre_inner(chars, pos, &mut sub, group_counter, true);
                        let tok = BreToken::Group(sub, gid);
                        // Check for quantifier after group
                        let tok = maybe_quantifier(chars, pos, tok);
                        tokens.push(tok);
                    }
                    '+' => {
                        *pos += 2;
                        if let Some(prev) = tokens.pop() {
                            tokens.push(BreToken::Repeat(Box::new(prev), RepeatKind::Plus));
                        }
                    }
                    '?' => {
                        *pos += 2;
                        if let Some(prev) = tokens.pop() {
                            tokens.push(BreToken::Repeat(Box::new(prev), RepeatKind::Question));
                        }
                    }
                    't' => {
                        *pos += 2;
                        let tok = BreToken::Literal('\t');
                        let tok = maybe_quantifier(chars, pos, tok);
                        tokens.push(tok);
                    }
                    'n' => {
                        *pos += 2;
                        let tok = BreToken::Literal('\n');
                        let tok = maybe_quantifier(chars, pos, tok);
                        tokens.push(tok);
                    }
                    _ => {
                        // Escaped literal (e.g. \\, \., \[, etc.)
                        *pos += 2;
                        let tok = BreToken::Literal(next);
                        let tok = maybe_quantifier(chars, pos, tok);
                        tokens.push(tok);
                    }
                }
            }
            '^' if tokens.is_empty() => {
                *pos += 1;
                tokens.push(BreToken::Anchor(AnchorKind::Start));
            }
            '$' if *pos + 1 >= chars.len()
                || (in_group
                    && *pos + 2 < chars.len()
                    && chars[*pos + 1] == '\\'
                    && chars[*pos + 2] == ')') =>
            {
                *pos += 1;
                tokens.push(BreToken::Anchor(AnchorKind::End));
            }
            '.' => {
                *pos += 1;
                let tok = BreToken::Dot;
                let tok = maybe_quantifier(chars, pos, tok);
                tokens.push(tok);
            }
            '*' => {
                *pos += 1;
                if let Some(prev) = tokens.pop() {
                    tokens.push(BreToken::Repeat(Box::new(prev), RepeatKind::Star));
                }
            }
            '[' => {
                *pos += 1;
                let (class_chars, negated) = parse_char_class(chars, pos);
                let tok = BreToken::Class {
                    chars: class_chars,
                    negated,
                };
                let tok = maybe_quantifier(chars, pos, tok);
                tokens.push(tok);
            }
            _ => {
                *pos += 1;
                let tok = BreToken::Literal(ch);
                let tok = maybe_quantifier(chars, pos, tok);
                tokens.push(tok);
            }
        }
    }
}

fn maybe_quantifier(chars: &[char], pos: &mut usize, tok: BreToken) -> BreToken {
    if *pos < chars.len() {
        match chars[*pos] {
            '*' => {
                *pos += 1;
                return BreToken::Repeat(Box::new(tok), RepeatKind::Star);
            }
            '\\' if *pos + 1 < chars.len() => match chars[*pos + 1] {
                '+' => {
                    *pos += 2;
                    return BreToken::Repeat(Box::new(tok), RepeatKind::Plus);
                }
                '?' => {
                    *pos += 2;
                    return BreToken::Repeat(Box::new(tok), RepeatKind::Question);
                }
                _ => {}
            },
            _ => {}
        }
    }
    tok
}

fn parse_char_class(chars: &[char], pos: &mut usize) -> (Vec<char>, bool) {
    let mut result = Vec::new();
    let mut negated = false;

    if *pos < chars.len() && chars[*pos] == '^' {
        negated = true;
        *pos += 1;
    }

    // A ] right after [ (or [^) is treated as a literal
    if *pos < chars.len() && chars[*pos] == ']' {
        result.push(']');
        *pos += 1;
    }

    while *pos < chars.len() && chars[*pos] != ']' {
        // Check for POSIX classes like [:digit:]
        if chars[*pos] == '[' && *pos + 1 < chars.len() && chars[*pos + 1] == ':' {
            *pos += 2; // skip [:
            let start = *pos;
            while *pos < chars.len()
                && !(chars[*pos] == ':' && *pos + 1 < chars.len() && chars[*pos + 1] == ']')
            {
                *pos += 1;
            }
            let class_name: String = chars[start..*pos].iter().collect();
            if *pos + 1 < chars.len() {
                *pos += 2; // skip :]
            }
            expand_posix_class(&class_name, &mut result);
            continue;
        }

        // Check for range: a-z
        if *pos + 2 < chars.len() && chars[*pos + 1] == '-' && chars[*pos + 2] != ']' {
            let from = chars[*pos];
            let to = chars[*pos + 2];
            let lo = from.min(to);
            let hi = from.max(to);
            for c in lo..=hi {
                result.push(c);
            }
            *pos += 3;
        } else {
            result.push(chars[*pos]);
            *pos += 1;
        }
    }

    if *pos < chars.len() && chars[*pos] == ']' {
        *pos += 1; // skip closing ]
    }

    (result, negated)
}

fn expand_posix_class(name: &str, result: &mut Vec<char>) {
    match name {
        "digit" => {
            for c in '0'..='9' {
                result.push(c);
            }
        }
        "alpha" => {
            for c in 'a'..='z' {
                result.push(c);
            }
            for c in 'A'..='Z' {
                result.push(c);
            }
        }
        "alnum" => {
            for c in '0'..='9' {
                result.push(c);
            }
            for c in 'a'..='z' {
                result.push(c);
            }
            for c in 'A'..='Z' {
                result.push(c);
            }
        }
        "upper" => {
            for c in 'A'..='Z' {
                result.push(c);
            }
        }
        "lower" => {
            for c in 'a'..='z' {
                result.push(c);
            }
        }
        "space" => {
            result.extend_from_slice(&[' ', '\t', '\n', '\r']);
        }
        "blank" => {
            result.extend_from_slice(&[' ', '\t']);
        }
        "punct" => {
            for c in '!'..='/' {
                result.push(c);
            }
            for c in ':'..='@' {
                result.push(c);
            }
            for c in '['..='`' {
                result.push(c);
            }
            for c in '{'..='~' {
                result.push(c);
            }
        }
        _ => {}
    }
}

/// Represents a match result with captured groups.
#[derive(Clone, Debug)]
struct MatchResult {
    start: usize,
    end: usize,
    groups: Vec<(usize, usize)>, // (start, end) for each group (index 0 = group 1)
}

/// Try to match tokens starting at position `start` within `text`.
/// Returns the end position and captured groups if successful.
fn match_tokens(
    text: &str,
    text_start: usize,
    tokens: &[BreToken],
    tok_idx: usize,
    groups: &mut Vec<(usize, usize)>,
) -> Option<usize> {
    let text_len = text.len();

    if tok_idx >= tokens.len() {
        return Some(text_start);
    }

    let token = &tokens[tok_idx];
    match token {
        BreToken::Anchor(AnchorKind::Start) => {
            if text_start == 0 {
                match_tokens(text, text_start, tokens, tok_idx + 1, groups)
            } else {
                None
            }
        }
        BreToken::Anchor(AnchorKind::End) => {
            if text_start == text_len {
                match_tokens(text, text_start, tokens, tok_idx + 1, groups)
            } else {
                None
            }
        }
        BreToken::Literal(ch) => {
            if text_start < text_len {
                // Get character at text_start
                let actual = text[text_start..].chars().next()?;
                if actual == *ch {
                    match_tokens(
                        text,
                        text_start + actual.len_utf8(),
                        tokens,
                        tok_idx + 1,
                        groups,
                    )
                } else {
                    None
                }
            } else {
                None
            }
        }
        BreToken::Dot => {
            if text_start < text_len {
                let actual = text[text_start..].chars().next()?;
                if actual != '\n' {
                    match_tokens(
                        text,
                        text_start + actual.len_utf8(),
                        tokens,
                        tok_idx + 1,
                        groups,
                    )
                } else {
                    None
                }
            } else {
                None
            }
        }
        BreToken::Class { chars, negated } => {
            if text_start < text_len {
                let actual = text[text_start..].chars().next()?;
                let in_class = chars.contains(&actual);
                let matches = if *negated { !in_class } else { in_class };
                if matches {
                    match_tokens(
                        text,
                        text_start + actual.len_utf8(),
                        tokens,
                        tok_idx + 1,
                        groups,
                    )
                } else {
                    None
                }
            } else {
                None
            }
        }
        BreToken::Repeat(inner, kind) => {
            let min = match kind {
                RepeatKind::Star => 0,
                RepeatKind::Plus => 1,
                RepeatKind::Question => 0,
            };
            let max = match kind {
                RepeatKind::Question => 1,
                _ => usize::MAX,
            };

            // Greedy: try to match as many as possible, then backtrack
            let mut positions = vec![text_start];
            let mut current = text_start;
            let mut count = 0;

            while count < max {
                let mut temp_groups = groups.clone();
                if let Some(next_pos) = match_single_token(text, current, inner, &mut temp_groups) {
                    if next_pos == current {
                        // Avoid infinite loop on zero-width match
                        break;
                    }
                    current = next_pos;
                    count += 1;
                    positions.push(current);
                } else {
                    break;
                }
            }

            if count < min {
                return None;
            }

            // Try from most to least (greedy)
            for i in (min..positions.len()).rev() {
                let saved_groups = groups.clone();
                if let Some(end) = match_tokens(text, positions[i], tokens, tok_idx + 1, groups) {
                    return Some(end);
                }
                *groups = saved_groups;
            }

            None
        }
        BreToken::Group(sub_tokens, gid) => {
            let saved_groups = groups.clone();
            let group_start = text_start;

            // Ensure groups vec is large enough
            while groups.len() < *gid {
                groups.push((0, 0));
            }

            if let Some(group_end) = match_tokens(text, group_start, sub_tokens, 0, groups) {
                // Record group
                if *gid <= groups.len() {
                    groups[*gid - 1] = (group_start, group_end);
                }
                match_tokens(text, group_end, tokens, tok_idx + 1, groups)
            } else {
                *groups = saved_groups;
                None
            }
        }
    }
}

fn match_single_token(
    text: &str,
    pos: usize,
    token: &BreToken,
    groups: &mut Vec<(usize, usize)>,
) -> Option<usize> {
    // Match a single instance of the token (unwrapping repeats for inner)
    let tokens = [token.clone()];
    match_tokens(text, pos, &tokens, 0, groups)
}

/// Find the first (leftmost) match of a BRE pattern in text.
fn bre_find(text: &str, tokens: &[BreToken]) -> Option<MatchResult> {
    if tokens.is_empty() {
        return Some(MatchResult {
            start: 0,
            end: 0,
            groups: vec![],
        });
    }

    // Check if pattern is anchored at start
    let anchored_start = matches!(tokens.first(), Some(BreToken::Anchor(AnchorKind::Start)));

    if anchored_start {
        let mut groups = Vec::new();
        if let Some(end) = match_tokens(text, 0, tokens, 0, &mut groups) {
            return Some(MatchResult {
                start: 0,
                end,
                groups,
            });
        }
        return None;
    }

    // Try matching at each position
    for start in 0..=text.len() {
        let mut groups = Vec::new();
        if let Some(end) = match_tokens(text, start, tokens, 0, &mut groups) {
            return Some(MatchResult { start, end, groups });
        }
    }
    None
}

/// Check if a BRE pattern matches anywhere in the text.
fn bre_matches(text: &str, tokens: &[BreToken]) -> bool {
    bre_find(text, tokens).is_some()
}

/// Find all non-overlapping matches.
fn bre_find_all(text: &str, tokens: &[BreToken]) -> Vec<MatchResult> {
    let mut results = Vec::new();
    let mut search_start = 0;

    while search_start <= text.len() {
        let sub = &text[search_start..];
        // We need to find from the beginning of the sub-string, but adjust positions
        let mut found = false;
        for start in 0..=sub.len() {
            let mut groups = Vec::new();
            if let Some(end) = match_tokens(text, search_start + start, tokens, 0, &mut groups) {
                results.push(MatchResult {
                    start: search_start + start,
                    end,
                    groups,
                });
                if end == search_start + start {
                    search_start = end + 1; // avoid infinite loop
                } else {
                    search_start = end;
                }
                found = true;
                break;
            }
        }
        if !found {
            break;
        }
    }
    results
}

// ---------------------------------------------------------------------------
// sed data structures
// ---------------------------------------------------------------------------

#[derive(Clone)]
enum Address {
    None,
    Line(usize),
    Last,
    Pattern(Vec<BreToken>),
    Range(Box<Address>, Box<Address>),
    Negated(Box<Address>),
}

#[derive(Clone)]
enum SedCmd {
    Substitute {
        pattern: Vec<BreToken>,
        replacement: String,
        global: bool,
        print: bool,
        ignore_case: bool,
        nth: usize, // 0 = first (default), N = Nth occurrence
        write_file: Option<String>,
    },
    Delete,
    Print,
    Quit,
    AppendText(String),
    InsertText(String),
    ChangeText(String),
    Transliterate(Vec<char>, Vec<char>),
    WriteFile(String),
    HoldCopy,   // h - copy pattern to hold
    HoldAppend, // H - append pattern to hold
    GetCopy,    // g - copy hold to pattern
    GetAppend,  // G - append hold to pattern
    Exchange,   // x - exchange hold and pattern
    Block(Vec<Rule>),
}

#[derive(Clone)]
struct Rule {
    address: Address,
    command: SedCmd,
}

// ---------------------------------------------------------------------------
// Parsing
// ---------------------------------------------------------------------------

fn unescape_replacement(s: &str) -> String {
    // We don't unescape the replacement here — we handle & and \1 at apply time.
    // But we do handle \\n -> \n and \\t -> \t literal escapes.
    let mut result = String::new();
    let chars: Vec<char> = s.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == '\\' && i + 1 < chars.len() {
            match chars[i + 1] {
                'n' => {
                    result.push('\n');
                    i += 2;
                }
                't' => {
                    result.push('\t');
                    i += 2;
                }
                _ => {
                    // Keep as-is for later processing (\1, \2, etc. and \\)
                    result.push('\\');
                    result.push(chars[i + 1]);
                    i += 2;
                }
            }
        } else {
            result.push(chars[i]);
            i += 1;
        }
    }
    result
}

fn parse_substitute(s: &str) -> Option<SedCmd> {
    if s.len() < 2 {
        return None;
    }
    let delim = s.chars().next()?;
    let rest = &s[delim.len_utf8()..];

    // Find second delimiter (respecting backslash escapes)
    let second = find_unescaped_delim(rest, delim)?;
    let pattern_str = rest[..second].to_string();
    let rest = &rest[second + delim.len_utf8()..];

    // Find third delimiter
    let (replacement_raw, flags_str) = if let Some(third) = find_unescaped_delim(rest, delim) {
        (rest[..third].to_string(), &rest[third + delim.len_utf8()..])
    } else {
        (rest.to_string(), "")
    };

    let replacement = unescape_replacement(&replacement_raw);

    let mut global = false;
    let mut print = false;
    let mut ignore_case = false;
    let mut nth: usize = 0;
    let mut write_file: Option<String> = None;

    let flags_chars: Vec<char> = flags_str.chars().collect();
    let mut fi = 0;
    while fi < flags_chars.len() {
        match flags_chars[fi] {
            'g' => global = true,
            'p' => print = true,
            'i' | 'I' => ignore_case = true,
            'w' => {
                // Rest is filename
                let fname: String = flags_chars[fi + 1..].iter().collect();
                write_file = Some(fname.trim().to_string());
                break;
            }
            c if c.is_ascii_digit() => {
                // Nth occurrence
                let mut num_str = String::new();
                while fi < flags_chars.len() && flags_chars[fi].is_ascii_digit() {
                    num_str.push(flags_chars[fi]);
                    fi += 1;
                }
                nth = num_str.parse().unwrap_or(0);
                continue; // don't increment fi again
            }
            _ => {}
        }
        fi += 1;
    }

    let tokens = parse_bre(&pattern_str);

    Some(SedCmd::Substitute {
        pattern: tokens,
        replacement,
        global,
        print,
        ignore_case,
        nth,
        write_file,
    })
}

fn find_unescaped_delim(s: &str, delim: char) -> Option<usize> {
    let chars: Vec<char> = s.chars().collect();
    let mut i = 0;
    let mut byte_offset = 0;
    while i < chars.len() {
        if chars[i] == '\\' {
            byte_offset += chars[i].len_utf8();
            i += 1;
            if i < chars.len() {
                byte_offset += chars[i].len_utf8();
                i += 1;
            }
            continue;
        }
        if chars[i] == delim {
            return Some(byte_offset);
        }
        byte_offset += chars[i].len_utf8();
        i += 1;
    }
    None
}

fn parse_address(s: &str) -> (Address, &str) {
    if s.is_empty() {
        return (Address::None, s);
    }

    let ch = s.as_bytes()[0];

    if ch == b'$' {
        let rest = &s[1..];
        let addr = Address::Last;
        return finish_address(addr, rest);
    }

    if ch.is_ascii_digit() {
        let end = s.find(|c: char| !c.is_ascii_digit()).unwrap_or(s.len());
        let n: usize = s[..end].parse().unwrap_or(0);
        let rest = &s[end..];
        let addr = Address::Line(n);
        return finish_address(addr, rest);
    }

    if ch == b'/' {
        let rest = &s[1..];
        if let Some(end) = rest.find('/') {
            let pattern = rest[..end].to_string();
            let after = &rest[end + 1..];
            let tokens = parse_bre(&pattern);
            let addr = Address::Pattern(tokens);
            return finish_address(addr, after);
        }
    }

    (Address::None, s)
}

fn finish_address(addr: Address, rest: &str) -> (Address, &str) {
    if let Some(after_comma) = rest.strip_prefix(',') {
        let (addr2, rest2) = parse_address(after_comma);
        (Address::Range(Box::new(addr), Box::new(addr2)), rest2)
    } else {
        (addr, rest)
    }
}

fn parse_script(script: &str) -> Vec<Rule> {
    let mut rules = Vec::new();
    let chars: Vec<char> = script.chars().collect();
    let mut pos = 0;
    parse_commands(&chars, &mut pos, &mut rules, false);
    rules
}

fn parse_commands(chars: &[char], pos: &mut usize, rules: &mut Vec<Rule>, _in_block: bool) {
    while *pos < chars.len() {
        // Skip whitespace and semicolons
        while *pos < chars.len()
            && (chars[*pos] == ' '
                || chars[*pos] == '\t'
                || chars[*pos] == ';'
                || chars[*pos] == '\n')
        {
            *pos += 1;
        }
        if *pos >= chars.len() {
            break;
        }

        if chars[*pos] == '}' {
            *pos += 1;
            return; // end of block
        }

        // Parse address
        let remaining: String = chars[*pos..].iter().collect();
        let (address, after_addr) = parse_address(&remaining);
        let consumed = remaining.len() - after_addr.len();
        *pos += consumed;

        // Skip whitespace
        while *pos < chars.len() && (chars[*pos] == ' ' || chars[*pos] == '\t') {
            *pos += 1;
        }

        if *pos >= chars.len() {
            break;
        }

        // Check for negation
        let (address, _negated) = if *pos < chars.len() && chars[*pos] == '!' {
            *pos += 1;
            // Skip whitespace after !
            while *pos < chars.len() && (chars[*pos] == ' ' || chars[*pos] == '\t') {
                *pos += 1;
            }
            (Address::Negated(Box::new(address)), true)
        } else {
            (address, false)
        };

        if *pos >= chars.len() {
            break;
        }

        // Parse command
        let cmd_char = chars[*pos];
        match cmd_char {
            's' => {
                *pos += 1;
                let remaining: String = chars[*pos..].iter().collect();
                if let Some(sub) = parse_substitute(&remaining) {
                    // Figure out how much was consumed
                    let sub_consumed = measure_substitute(&remaining);
                    *pos += sub_consumed;
                    rules.push(Rule {
                        address,
                        command: sub,
                    });
                }
            }
            'd' => {
                *pos += 1;
                rules.push(Rule {
                    address,
                    command: SedCmd::Delete,
                });
            }
            'p' => {
                *pos += 1;
                rules.push(Rule {
                    address,
                    command: SedCmd::Print,
                });
            }
            'q' => {
                *pos += 1;
                rules.push(Rule {
                    address,
                    command: SedCmd::Quit,
                });
            }
            'a' => {
                *pos += 1;
                let text = parse_text_arg(chars, pos);
                rules.push(Rule {
                    address,
                    command: SedCmd::AppendText(text),
                });
            }
            'i' => {
                *pos += 1;
                let text = parse_text_arg(chars, pos);
                rules.push(Rule {
                    address,
                    command: SedCmd::InsertText(text),
                });
            }
            'c' => {
                *pos += 1;
                let text = parse_text_arg(chars, pos);
                rules.push(Rule {
                    address,
                    command: SedCmd::ChangeText(text),
                });
            }
            'y' => {
                *pos += 1;
                if *pos < chars.len() {
                    let delim = chars[*pos];
                    *pos += 1;
                    let from = collect_until(chars, pos, delim);
                    if *pos < chars.len() && chars[*pos] == delim {
                        *pos += 1;
                    }
                    let to = collect_until(chars, pos, delim);
                    if *pos < chars.len() && chars[*pos] == delim {
                        *pos += 1;
                    }
                    let from_chars: Vec<char> = from.chars().collect();
                    let to_chars: Vec<char> = to.chars().collect();
                    rules.push(Rule {
                        address,
                        command: SedCmd::Transliterate(from_chars, to_chars),
                    });
                }
            }
            'w' => {
                *pos += 1;
                // Skip one space
                if *pos < chars.len() && chars[*pos] == ' ' {
                    *pos += 1;
                }
                let filename = collect_until_end_of_cmd(chars, pos);
                rules.push(Rule {
                    address,
                    command: SedCmd::WriteFile(filename.trim().to_string()),
                });
            }
            'h' => {
                *pos += 1;
                rules.push(Rule {
                    address,
                    command: SedCmd::HoldCopy,
                });
            }
            'H' => {
                *pos += 1;
                rules.push(Rule {
                    address,
                    command: SedCmd::HoldAppend,
                });
            }
            'g' => {
                *pos += 1;
                rules.push(Rule {
                    address,
                    command: SedCmd::GetCopy,
                });
            }
            'G' => {
                *pos += 1;
                rules.push(Rule {
                    address,
                    command: SedCmd::GetAppend,
                });
            }
            'x' => {
                *pos += 1;
                rules.push(Rule {
                    address,
                    command: SedCmd::Exchange,
                });
            }
            '{' => {
                *pos += 1;
                let mut block_rules = Vec::new();
                parse_commands(chars, pos, &mut block_rules, true);
                rules.push(Rule {
                    address,
                    command: SedCmd::Block(block_rules),
                });
            }
            _ => {
                *pos += 1;
            }
        }
    }
}

fn parse_text_arg(chars: &[char], pos: &mut usize) -> String {
    // Handle a\ text or a\text
    if *pos < chars.len() && chars[*pos] == '\\' {
        *pos += 1;
    }
    // Collect until end of command (semicolon, newline, or end of input)
    // But also skip leading whitespace after backslash only if there's nothing before
    let mut text = String::new();
    while *pos < chars.len() && chars[*pos] != ';' && chars[*pos] != '\n' && chars[*pos] != '}' {
        text.push(chars[*pos]);
        *pos += 1;
    }
    text
}

fn collect_until(chars: &[char], pos: &mut usize, delim: char) -> String {
    let mut result = String::new();
    while *pos < chars.len() && chars[*pos] != delim {
        if chars[*pos] == '\\' && *pos + 1 < chars.len() {
            result.push(chars[*pos]);
            result.push(chars[*pos + 1]);
            *pos += 2;
        } else {
            result.push(chars[*pos]);
            *pos += 1;
        }
    }
    result
}

fn collect_until_end_of_cmd(chars: &[char], pos: &mut usize) -> String {
    let mut result = String::new();
    while *pos < chars.len() && chars[*pos] != ';' && chars[*pos] != '\n' && chars[*pos] != '}' {
        result.push(chars[*pos]);
        *pos += 1;
    }
    result
}

fn measure_substitute(s: &str) -> usize {
    // Measure how many characters the s/pat/repl/flags consumes
    if s.is_empty() {
        return 0;
    }
    let chars: Vec<char> = s.chars().collect();
    let delim = chars[0];
    let mut i = 1;
    let mut delim_count = 0;

    while i < chars.len() && delim_count < 2 {
        if chars[i] == '\\' && i + 1 < chars.len() {
            i += 2;
            continue;
        }
        if chars[i] == delim {
            delim_count += 1;
        }
        i += 1;
    }

    // Now consume flags
    while i < chars.len()
        && chars[i] != ';'
        && chars[i] != '\n'
        && chars[i] != '}'
        && chars[i] != ' '
    {
        if chars[i] == 'w' {
            // w flag: rest until end of command is filename
            i += 1;
            while i < chars.len() && chars[i] != ';' && chars[i] != '\n' && chars[i] != '}' {
                i += 1;
            }
            break;
        }
        i += 1;
    }

    // Count bytes consumed
    let consumed: usize = chars[..i].iter().map(|c| c.len_utf8()).sum();
    consumed
}

// ---------------------------------------------------------------------------
// Address matching
// ---------------------------------------------------------------------------

fn address_matches_simple(addr: &Address, line_num: usize, line: &str, is_last: bool) -> bool {
    match addr {
        Address::None => true,
        Address::Line(n) => line_num == *n,
        Address::Last => is_last,
        Address::Pattern(tokens) => bre_matches(line, tokens),
        Address::Negated(inner) => !address_matches_simple(inner, line_num, line, is_last),
        Address::Range(..) => false, // handled by range state
    }
}

// ---------------------------------------------------------------------------
// Substitution
// ---------------------------------------------------------------------------

fn build_replacement(text: &str, matched: &MatchResult, replacement: &str) -> String {
    let mut result = String::new();
    let rchars: Vec<char> = replacement.chars().collect();
    let mut i = 0;
    while i < rchars.len() {
        if rchars[i] == '&' {
            result.push_str(&text[matched.start..matched.end]);
            i += 1;
        } else if rchars[i] == '\\' && i + 1 < rchars.len() {
            let next = rchars[i + 1];
            if next.is_ascii_digit() && next != '0' {
                let gid = (next as usize) - ('0' as usize);
                if gid <= matched.groups.len() {
                    let (gs, ge) = matched.groups[gid - 1];
                    result.push_str(&text[gs..ge]);
                }
                i += 2;
            } else if next == '\\' {
                result.push('\\');
                i += 2;
            } else if next == '&' {
                result.push('&');
                i += 2;
            } else {
                result.push(rchars[i + 1]);
                i += 2;
            }
        } else {
            result.push(rchars[i]);
            i += 1;
        }
    }
    result
}

fn apply_substitute(
    line: &str,
    tokens: &[BreToken],
    replacement: &str,
    global: bool,
    ignore_case: bool,
    nth: usize,
) -> (String, bool) {
    // For case-insensitive matching, we lowercase the line for matching
    // but use original positions for replacement
    let match_line;
    let match_tokens;
    if ignore_case {
        match_line = line.to_lowercase();
        // Re-parse the pattern with lowercase (since literal chars need to be lowercase)
        let lc_pat = lowercase_bre_tokens(tokens);
        match_tokens = lc_pat;
    } else {
        match_line = line.to_string();
        match_tokens = tokens.to_vec();
    };

    if global {
        // Replace all occurrences
        let matches = bre_find_all(&match_line, &match_tokens);
        if matches.is_empty() {
            return (line.to_string(), false);
        }
        let mut result = String::new();
        let mut last_end = 0;
        for m in &matches {
            result.push_str(&line[last_end..m.start]);
            result.push_str(&build_replacement(line, m, replacement));
            last_end = m.end;
        }
        result.push_str(&line[last_end..]);
        (result, true)
    } else if nth > 0 {
        // Replace the Nth occurrence only
        let matches = bre_find_all(&match_line, &match_tokens);
        if matches.len() < nth {
            return (line.to_string(), false);
        }
        let m = &matches[nth - 1];
        let mut result = String::new();
        result.push_str(&line[..m.start]);
        result.push_str(&build_replacement(line, m, replacement));
        result.push_str(&line[m.end..]);
        (result, true)
    } else {
        // Replace first occurrence
        if let Some(m) = bre_find(&match_line, &match_tokens) {
            let mut result = String::new();
            result.push_str(&line[..m.start]);
            result.push_str(&build_replacement(line, &m, replacement));
            result.push_str(&line[m.end..]);
            (result, true)
        } else {
            (line.to_string(), false)
        }
    }
}

fn lowercase_bre_tokens(tokens: &[BreToken]) -> Vec<BreToken> {
    tokens
        .iter()
        .map(|t| match t {
            BreToken::Literal(c) => {
                let lc: Vec<char> = c.to_lowercase().collect();
                BreToken::Literal(lc[0])
            }
            BreToken::Repeat(inner, kind) => {
                let inner_lc = lowercase_bre_tokens(&[(**inner).clone()]);
                BreToken::Repeat(Box::new(inner_lc.into_iter().next().unwrap()), kind.clone())
            }
            BreToken::Group(sub, gid) => BreToken::Group(lowercase_bre_tokens(sub), *gid),
            BreToken::Class { chars, negated } => {
                let lc_chars: Vec<char> = chars.iter().flat_map(|c| c.to_lowercase()).collect();
                BreToken::Class {
                    chars: lc_chars,
                    negated: *negated,
                }
            }
            other => other.clone(),
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Execution
// ---------------------------------------------------------------------------

struct SedState {
    hold_space: String,
    /// For each range-address rule index, whether we are currently inside the range.
    range_active: Vec<bool>,
}

fn run_sed(input: &mut dyn Read, rules: &[Rule], suppress: bool) {
    let reader = BufReader::new(input);
    let lines: Vec<String> = reader.lines().map_while(Result::ok).collect();
    let total = lines.len();

    let range_count = count_ranges(rules);
    let mut state = SedState {
        hold_space: String::new(),
        range_active: vec![false; range_count],
    };

    for (i, line) in lines.iter().enumerate() {
        let line_num = i + 1;
        let is_last = line_num == total;
        let mut current = line.clone();
        let mut deleted = false;
        let mut print_count: usize = 0; // number of explicit p commands matched
        let mut sub_print = false; // s///p flag matched
        let mut append_text: Vec<String> = Vec::new();
        let mut insert_text: Vec<String> = Vec::new();
        let mut change_text: Option<String> = None;
        let mut quit = false;

        let mut range_idx = 0;
        execute_rules(
            rules,
            line_num,
            is_last,
            &mut current,
            &mut deleted,
            &mut print_count,
            &mut sub_print,
            &mut append_text,
            &mut insert_text,
            &mut change_text,
            &mut quit,
            &mut state,
            &mut range_idx,
        );

        if let Some(text) = change_text {
            // c\ replaces the line entirely; suppress original output
            for t in &insert_text {
                println!("{}", t);
            }
            println!("{}", text);
        } else if !deleted {
            for t in &insert_text {
                println!("{}", t);
            }
            // Print explicit p copies
            for _ in 0..print_count {
                println!("{}", current);
            }
            // Print default output (unless suppressed; s///p overrides suppression)
            if !suppress || (sub_print && print_count == 0) {
                println!("{}", current);
            }
            for t in &append_text {
                println!("{}", t);
            }
        }

        if quit {
            return;
        }
    }
}

fn count_ranges(rules: &[Rule]) -> usize {
    let mut count = 0;
    for rule in rules {
        if matches!(rule.address, Address::Range(..)) {
            count += 1;
        }
        if let SedCmd::Block(ref sub_rules) = rule.command {
            count += count_ranges(sub_rules);
        }
    }
    count
}

fn rule_matches(
    addr: &Address,
    line_num: usize,
    line: &str,
    is_last: bool,
    state: &mut SedState,
    range_idx: &mut usize,
) -> bool {
    match addr {
        Address::Range(start, end) => {
            let idx = *range_idx;
            *range_idx += 1;

            if state.range_active.len() <= idx {
                state.range_active.resize(idx + 1, false);
            }

            if state.range_active[idx] {
                // We're in the range — check if this line matches the end
                if address_matches_simple(end, line_num, line, is_last) {
                    state.range_active[idx] = false;
                }
                true
            } else if address_matches_simple(start, line_num, line, is_last) {
                // Start of range
                state.range_active[idx] = true;
                // Check if end also matches on same line
                if address_matches_simple(end, line_num, line, is_last) {
                    state.range_active[idx] = false;
                }
                true
            } else {
                false
            }
        }
        Address::Negated(inner) => !rule_matches(inner, line_num, line, is_last, state, range_idx),
        other => address_matches_simple(other, line_num, line, is_last),
    }
}

#[allow(clippy::too_many_arguments)]
fn execute_rules(
    rules: &[Rule],
    line_num: usize,
    is_last: bool,
    current: &mut String,
    deleted: &mut bool,
    print_count: &mut usize,
    sub_print: &mut bool,
    append_text: &mut Vec<String>,
    insert_text: &mut Vec<String>,
    change_text: &mut Option<String>,
    quit: &mut bool,
    state: &mut SedState,
    range_idx: &mut usize,
) {
    for rule in rules {
        if *deleted {
            // Still need to advance range_idx for range addresses
            if matches!(rule.address, Address::Range(..)) {
                *range_idx += 1;
            }
            continue;
        }

        if !rule_matches(&rule.address, line_num, current, is_last, state, range_idx) {
            continue;
        }

        match &rule.command {
            SedCmd::Substitute {
                pattern,
                replacement,
                global,
                print,
                ignore_case,
                nth,
                write_file,
                ..
            } => {
                let (new_line, changed) =
                    apply_substitute(current, pattern, replacement, *global, *ignore_case, *nth);
                *current = new_line;
                if changed && *print {
                    *sub_print = true;
                }
                if changed {
                    if let Some(fname) = write_file {
                        write_line_to_file(fname, current);
                    }
                }
            }
            SedCmd::Delete => {
                *deleted = true;
            }
            SedCmd::Print => {
                *print_count += 1;
            }
            SedCmd::Quit => {
                *quit = true;
            }
            SedCmd::AppendText(text) => {
                append_text.push(text.clone());
            }
            SedCmd::InsertText(text) => {
                insert_text.push(text.clone());
            }
            SedCmd::ChangeText(text) => {
                *change_text = Some(text.clone());
            }
            SedCmd::Transliterate(from, to) => {
                let mut new_line = String::new();
                for ch in current.chars() {
                    if let Some(idx) = from.iter().position(|&c| c == ch) {
                        if idx < to.len() {
                            new_line.push(to[idx]);
                        } else {
                            new_line.push(ch);
                        }
                    } else {
                        new_line.push(ch);
                    }
                }
                *current = new_line;
            }
            SedCmd::WriteFile(fname) => {
                write_line_to_file(fname, current);
            }
            SedCmd::HoldCopy => {
                state.hold_space = current.clone();
            }
            SedCmd::HoldAppend => {
                state.hold_space.push('\n');
                state.hold_space.push_str(current);
            }
            SedCmd::GetCopy => {
                *current = state.hold_space.clone();
            }
            SedCmd::GetAppend => {
                current.push('\n');
                current.push_str(&state.hold_space);
            }
            SedCmd::Exchange => {
                std::mem::swap(current, &mut state.hold_space);
            }
            SedCmd::Block(sub_rules) => {
                execute_rules(
                    sub_rules,
                    line_num,
                    is_last,
                    current,
                    deleted,
                    print_count,
                    sub_print,
                    append_text,
                    insert_text,
                    change_text,
                    quit,
                    state,
                    range_idx,
                );
            }
        }
    }
}

fn write_line_to_file(fname: &str, line: &str) {
    if let Ok(mut f) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(fname)
    {
        let _ = writeln!(f, "{}", line);
    }
}

// ---------------------------------------------------------------------------
// Main
// ---------------------------------------------------------------------------

fn main() {
    let args: Vec<String> = env::args().collect();

    let mut suppress = false;
    let mut scripts: Vec<String> = Vec::new();
    let mut files: Vec<String> = Vec::new();

    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "-n" => suppress = true,
            "-e" => {
                i += 1;
                if i >= args.len() {
                    eprintln!("sed: option requires an argument -- 'e'");
                    process::exit(1);
                }
                scripts.push(args[i].clone());
            }
            arg => {
                if scripts.is_empty() && !arg.starts_with('-') {
                    scripts.push(arg.to_string());
                } else {
                    files.push(arg.to_string());
                }
            }
        }
        i += 1;
    }

    if scripts.is_empty() {
        eprintln!("sed: no script given");
        process::exit(1);
    }

    let mut rules = Vec::new();
    for script in &scripts {
        rules.extend(parse_script(script));
    }

    if files.is_empty() {
        let stdin = io::stdin();
        let mut lock = stdin.lock();
        run_sed(&mut lock, &rules, suppress);
    } else {
        for file in &files {
            match File::open(file) {
                Ok(mut f) => run_sed(&mut f, &rules, suppress),
                Err(e) => {
                    eprintln!("sed: {}: {}", file, e);
                    process::exit(1);
                }
            }
        }
    }
}
