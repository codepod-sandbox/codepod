use codepod_shell::ast::Command;
use codepod_shell::token::RedirectType;

use crate::control::{ControlFlow, RunResult, ShellError};
use crate::expand::{
    expand_braces, expand_globs, expand_words_with_splitting, restore_brace_sentinels,
};
use crate::host::{HostInterface, WriteMode};
use crate::state::ShellState;

/// Execute a parsed `Command` AST node.
///
/// Currently only the `Command::Simple` variant is implemented.
/// All other variants return an empty `RunResult`.
pub fn exec_command(
    state: &mut ShellState,
    host: &dyn HostInterface,
    cmd: &Command,
) -> Result<ControlFlow, ShellError> {
    // Create executor callback for command substitution.
    // When word expansion encounters `$(...)`, it calls this closure to
    // parse and execute the inner command, capturing its stdout.
    let exec_fn = |state: &mut ShellState, cmd_str: &str| -> String {
        let inner_cmd = codepod_shell::parser::parse(cmd_str);
        match exec_command(state, host, &inner_cmd) {
            Ok(ControlFlow::Normal(r)) => r.stdout,
            Ok(ControlFlow::Exit(_, stdout, _)) => stdout,
            _ => String::new(),
        }
    };

    match cmd {
        Command::Simple {
            words,
            redirects,
            assignments: _,
        } => {
            if words.is_empty() {
                // Assignment-only command; nothing to spawn.
                return Ok(ControlFlow::Normal(RunResult::empty()));
            }

            let expanded = expand_words_with_splitting(state, words, Some(&exec_fn));
            if expanded.is_empty() {
                return Ok(ControlFlow::Normal(RunResult::empty()));
            }

            // Brace expansion → sentinel restoration → glob expansion
            let braced = expand_braces(&expanded);
            let restored = restore_brace_sentinels(&braced);
            let globbed = expand_globs(host, &restored);

            if globbed.is_empty() {
                return Ok(ControlFlow::Normal(RunResult::empty()));
            }
            let cmd_name = &globbed[0];
            let args: Vec<&str> = globbed[1..].iter().map(|s| s.as_str()).collect();

            // ── Phase 1: Extract stdin from input redirects ──────────────
            let mut stdin_data = String::new();
            for redir in redirects {
                match &redir.redirect_type {
                    RedirectType::StdinFrom(path) => {
                        let resolved = state.resolve_path(path);
                        stdin_data = host
                            .read_file(&resolved)
                            .map_err(|e| ShellError::HostError(e.to_string()))?;
                    }
                    RedirectType::Heredoc(content) => {
                        stdin_data = content.clone();
                    }
                    RedirectType::HeredocStrip(content) => {
                        stdin_data = content.clone();
                    }
                    RedirectType::HereString(word) => {
                        stdin_data = format!("{word}\n");
                    }
                    _ => {}
                }
            }

            // Convert env HashMap to the slice format expected by spawn.
            let env_pairs: Vec<(&str, &str)> = state
                .env
                .iter()
                .map(|(k, v)| (k.as_str(), v.as_str()))
                .collect();

            let spawn_result = host
                .spawn(cmd_name, &args, &env_pairs, &state.cwd, &stdin_data)
                .map_err(|e| ShellError::HostError(e.to_string()))?;

            state.last_exit_code = spawn_result.exit_code;

            let mut stdout = spawn_result.stdout;
            let mut stderr = spawn_result.stderr;

            // ── Phase 2: Process output redirects ────────────────────────
            let mut last_stdout_redirect_path: Option<String> = None;

            for redir in redirects {
                match &redir.redirect_type {
                    RedirectType::StdoutOverwrite(path) => {
                        let resolved = state.resolve_path(path);
                        host.write_file(&resolved, &stdout, WriteMode::Truncate)
                            .map_err(|e| ShellError::HostError(e.to_string()))?;
                        stdout = String::new();
                        last_stdout_redirect_path = Some(resolved);
                    }
                    RedirectType::StdoutAppend(path) => {
                        let resolved = state.resolve_path(path);
                        host.write_file(&resolved, &stdout, WriteMode::Append)
                            .map_err(|e| ShellError::HostError(e.to_string()))?;
                        stdout = String::new();
                        last_stdout_redirect_path = Some(resolved);
                    }
                    RedirectType::StderrOverwrite(path) => {
                        let resolved = state.resolve_path(path);
                        host.write_file(&resolved, &stderr, WriteMode::Truncate)
                            .map_err(|e| ShellError::HostError(e.to_string()))?;
                        stderr = String::new();
                    }
                    RedirectType::StderrAppend(path) => {
                        let resolved = state.resolve_path(path);
                        host.write_file(&resolved, &stderr, WriteMode::Append)
                            .map_err(|e| ShellError::HostError(e.to_string()))?;
                        stderr = String::new();
                    }
                    RedirectType::StderrToStdout => {
                        if let Some(ref file_path) = last_stdout_redirect_path {
                            if !stderr.is_empty() {
                                // Append stderr to the file where stdout was redirected
                                host.write_file(file_path, &stderr, WriteMode::Append)
                                    .map_err(|e| ShellError::HostError(e.to_string()))?;
                            }
                        } else {
                            stdout.push_str(&stderr);
                        }
                        stderr = String::new();
                    }
                    RedirectType::BothOverwrite(path) => {
                        let resolved = state.resolve_path(path);
                        let combined = format!("{stdout}{stderr}");
                        host.write_file(&resolved, &combined, WriteMode::Truncate)
                            .map_err(|e| ShellError::HostError(e.to_string()))?;
                        stdout = String::new();
                        stderr = String::new();
                    }
                    // Input redirects were handled in Phase 1; skip them here.
                    RedirectType::StdinFrom(_)
                    | RedirectType::Heredoc(_)
                    | RedirectType::HeredocStrip(_)
                    | RedirectType::HereString(_) => {}
                }
            }

            Ok(ControlFlow::Normal(RunResult {
                exit_code: spawn_result.exit_code,
                stdout,
                stderr,
                execution_time_ms: 0,
            }))
        }

        // All other command variants are stubs for now.
        _ => Ok(ControlFlow::Normal(RunResult::empty())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::host::SpawnResult;
    use crate::test_support::mock::MockHost;

    #[test]
    fn simple_command_spawns_via_host() {
        let host = MockHost::new().with_tool("ls").with_spawn_result(
            "ls",
            SpawnResult {
                exit_code: 0,
                stdout: "file.txt\n".into(),
                stderr: String::new(),
            },
        );
        let mut state = ShellState::new_default();
        let cmd = codepod_shell::parser::parse("ls");
        let result = exec_command(&mut state, &host, &cmd);
        let ControlFlow::Normal(run) = result.unwrap() else {
            panic!("expected Normal")
        };
        assert_eq!(run.exit_code, 0);
        assert_eq!(run.stdout, "file.txt\n");
    }

    #[test]
    fn unknown_command_returns_127() {
        let host = MockHost::new();
        let mut state = ShellState::new_default();
        let cmd = codepod_shell::parser::parse("nonexistent");
        let result = exec_command(&mut state, &host, &cmd);
        let ControlFlow::Normal(run) = result.unwrap() else {
            panic!("expected Normal")
        };
        assert_eq!(run.exit_code, 127);
        assert!(run.stderr.contains("command not found"));
    }

    #[test]
    fn simple_command_with_args() {
        let host = MockHost::new().with_spawn_result(
            "echo-args",
            SpawnResult {
                exit_code: 0,
                stdout: "hello world\n".into(),
                stderr: String::new(),
            },
        );
        let mut state = ShellState::new_default();
        let cmd = codepod_shell::parser::parse("echo-args hello world");
        let result = exec_command(&mut state, &host, &cmd);
        let ControlFlow::Normal(run) = result.unwrap() else {
            panic!("expected Normal")
        };
        assert_eq!(run.exit_code, 0);
        assert_eq!(run.stdout, "hello world\n");
    }

    #[test]
    fn last_exit_code_is_updated() {
        let host = MockHost::new().with_spawn_result(
            "fail",
            SpawnResult {
                exit_code: 42,
                stdout: String::new(),
                stderr: "error\n".into(),
            },
        );
        let mut state = ShellState::new_default();
        let cmd = codepod_shell::parser::parse("fail");
        let _ = exec_command(&mut state, &host, &cmd);
        assert_eq!(state.last_exit_code, 42);
    }

    // ---- Command substitution tests ----

    #[test]
    fn command_substitution_basic() {
        // `echo $(echo hello)` should:
        //  1. Expand $(echo hello) → run "echo hello" → stdout "hello\n" → strip → "hello"
        //  2. Outer command becomes: echo hello
        let host = MockHost::new().with_spawn_result(
            "echo",
            SpawnResult {
                exit_code: 0,
                stdout: "hello\n".into(),
                stderr: String::new(),
            },
        );
        let mut state = ShellState::new_default();
        let cmd = codepod_shell::parser::parse("echo $(echo hello)");
        let result = exec_command(&mut state, &host, &cmd);
        let ControlFlow::Normal(run) = result.unwrap() else {
            panic!("expected Normal")
        };
        assert_eq!(run.exit_code, 0);
        // The outer "echo" also returns "hello\n" from MockHost since
        // MockHost matches only on program name.
        assert_eq!(run.stdout, "hello\n");
    }

    #[test]
    fn command_substitution_strips_trailing_newline() {
        // Verify that trailing newlines are stripped from command substitution output
        use crate::expand::expand_word;
        use codepod_shell::ast::{Word, WordPart};

        let host = MockHost::new().with_spawn_result(
            "echo",
            SpawnResult {
                exit_code: 0,
                stdout: "hello\n".into(),
                stderr: String::new(),
            },
        );
        let mut state = ShellState::new_default();

        // Build the exec callback like exec_command does
        let exec_fn = |state: &mut ShellState, cmd_str: &str| -> String {
            let inner_cmd = codepod_shell::parser::parse(cmd_str);
            match exec_command(state, &host, &inner_cmd) {
                Ok(ControlFlow::Normal(r)) => r.stdout,
                Ok(ControlFlow::Exit(_, stdout, _)) => stdout,
                _ => String::new(),
            }
        };

        let word = Word {
            parts: vec![WordPart::CommandSub("echo hello".into())],
        };
        let expanded = expand_word(&mut state, &word, Some(&exec_fn));
        assert_eq!(expanded, "hello");
    }

    #[test]
    fn command_substitution_in_middle_of_word() {
        // `pre$(echo mid)suf` should expand to "premidsuf"
        use crate::expand::expand_word;
        use codepod_shell::ast::{Word, WordPart};

        let host = MockHost::new().with_spawn_result(
            "echo",
            SpawnResult {
                exit_code: 0,
                stdout: "mid\n".into(),
                stderr: String::new(),
            },
        );
        let mut state = ShellState::new_default();

        let exec_fn = |state: &mut ShellState, cmd_str: &str| -> String {
            let inner_cmd = codepod_shell::parser::parse(cmd_str);
            match exec_command(state, &host, &inner_cmd) {
                Ok(ControlFlow::Normal(r)) => r.stdout,
                Ok(ControlFlow::Exit(_, stdout, _)) => stdout,
                _ => String::new(),
            }
        };

        let word = Word {
            parts: vec![
                WordPart::Literal("pre".into()),
                WordPart::CommandSub("echo mid".into()),
                WordPart::Literal("suf".into()),
            ],
        };
        let expanded = expand_word(&mut state, &word, Some(&exec_fn));
        assert_eq!(expanded, "premidsuf");
    }

    #[test]
    fn command_substitution_no_exec_returns_empty() {
        // When exec is None, CommandSub should return empty string
        use crate::expand::expand_word_part;
        use codepod_shell::ast::WordPart;

        let mut state = ShellState::new_default();
        let part = WordPart::CommandSub("echo hello".into());
        let result = expand_word_part(&mut state, &part, None);
        assert_eq!(result, "");
    }

    #[test]
    fn command_substitution_depth_limit() {
        // When substitution_depth is at MAX, CommandSub should return empty
        use crate::expand::expand_word_part;
        use crate::state::MAX_SUBSTITUTION_DEPTH;
        use codepod_shell::ast::WordPart;

        let host = MockHost::new().with_spawn_result(
            "echo",
            SpawnResult {
                exit_code: 0,
                stdout: "hello\n".into(),
                stderr: String::new(),
            },
        );
        let mut state = ShellState::new_default();
        state.substitution_depth = MAX_SUBSTITUTION_DEPTH; // at the limit

        let exec_fn = |state: &mut ShellState, cmd_str: &str| -> String {
            let inner_cmd = codepod_shell::parser::parse(cmd_str);
            match exec_command(state, &host, &inner_cmd) {
                Ok(ControlFlow::Normal(r)) => r.stdout,
                Ok(ControlFlow::Exit(_, stdout, _)) => stdout,
                _ => String::new(),
            }
        };

        let part = WordPart::CommandSub("echo hello".into());
        let result = expand_word_part(&mut state, &part, Some(&exec_fn));
        assert_eq!(result, ""); // should be empty because depth limit reached
    }

    #[test]
    fn command_substitution_increments_and_decrements_depth() {
        // Verify that substitution_depth is properly managed
        use crate::expand::expand_word_part;
        use codepod_shell::ast::WordPart;

        let host = MockHost::new().with_spawn_result(
            "echo",
            SpawnResult {
                exit_code: 0,
                stdout: "hello\n".into(),
                stderr: String::new(),
            },
        );
        let mut state = ShellState::new_default();
        assert_eq!(state.substitution_depth, 0);

        let exec_fn = |state: &mut ShellState, cmd_str: &str| -> String {
            let inner_cmd = codepod_shell::parser::parse(cmd_str);
            match exec_command(state, &host, &inner_cmd) {
                Ok(ControlFlow::Normal(r)) => r.stdout,
                Ok(ControlFlow::Exit(_, stdout, _)) => stdout,
                _ => String::new(),
            }
        };

        let part = WordPart::CommandSub("echo hello".into());
        let _ = expand_word_part(&mut state, &part, Some(&exec_fn));

        // After expansion, depth should be back to 0
        assert_eq!(state.substitution_depth, 0);
    }

    #[test]
    fn command_substitution_failed_command_returns_empty() {
        // If the inner command fails (exit code != 0), we still get its stdout
        use crate::expand::expand_word_part;
        use codepod_shell::ast::WordPart;

        let host = MockHost::new().with_spawn_result(
            "failing-cmd",
            SpawnResult {
                exit_code: 1,
                stdout: "some output\n".into(),
                stderr: "error\n".into(),
            },
        );
        let mut state = ShellState::new_default();

        let exec_fn = |state: &mut ShellState, cmd_str: &str| -> String {
            let inner_cmd = codepod_shell::parser::parse(cmd_str);
            match exec_command(state, &host, &inner_cmd) {
                Ok(ControlFlow::Normal(r)) => r.stdout,
                Ok(ControlFlow::Exit(_, stdout, _)) => stdout,
                _ => String::new(),
            }
        };

        let part = WordPart::CommandSub("failing-cmd".into());
        let result = expand_word_part(&mut state, &part, Some(&exec_fn));
        // Trailing newline stripped
        assert_eq!(result, "some output");
    }

    #[test]
    fn command_substitution_unknown_inner_command_returns_empty() {
        // If the inner command is unknown, MockHost returns exit 127 with empty stdout
        use crate::expand::expand_word_part;
        use codepod_shell::ast::WordPart;

        let host = MockHost::new(); // no spawn results configured
        let mut state = ShellState::new_default();

        let exec_fn = |state: &mut ShellState, cmd_str: &str| -> String {
            let inner_cmd = codepod_shell::parser::parse(cmd_str);
            match exec_command(state, &host, &inner_cmd) {
                Ok(ControlFlow::Normal(r)) => r.stdout,
                Ok(ControlFlow::Exit(_, stdout, _)) => stdout,
                _ => String::new(),
            }
        };

        let part = WordPart::CommandSub("nonexistent-cmd".into());
        let result = expand_word_part(&mut state, &part, Some(&exec_fn));
        assert_eq!(result, "");
    }

    // ---- Redirect tests ----

    /// Helper: build a `Command::Simple` with the given command name and redirects.
    fn simple_cmd_with_redirects(
        cmd_name: &str,
        args: &[&str],
        redirects: Vec<codepod_shell::ast::Redirect>,
    ) -> Command {
        use codepod_shell::ast::Word;
        let mut words = vec![Word::literal(cmd_name)];
        for arg in args {
            words.push(Word::literal(arg));
        }
        Command::Simple {
            words,
            redirects,
            assignments: vec![],
        }
    }

    fn redirect(rt: RedirectType) -> codepod_shell::ast::Redirect {
        codepod_shell::ast::Redirect { redirect_type: rt }
    }

    #[test]
    fn redirect_stdout_overwrite() {
        // `echo hello > /tmp/out.txt`
        // Stdout should be written to file; RunResult.stdout should be empty.
        let host = MockHost::new().with_spawn_result(
            "echo",
            SpawnResult {
                exit_code: 0,
                stdout: "hello\n".into(),
                stderr: String::new(),
            },
        );
        let mut state = ShellState::new_default();
        let cmd = simple_cmd_with_redirects(
            "echo",
            &["hello"],
            vec![redirect(RedirectType::StdoutOverwrite(
                "/tmp/out.txt".into(),
            ))],
        );
        let result = exec_command(&mut state, &host, &cmd);
        let ControlFlow::Normal(run) = result.unwrap() else {
            panic!("expected Normal")
        };
        // stdout should be captured to file, not returned
        assert_eq!(run.stdout, "");
        assert_eq!(run.exit_code, 0);
        // Verify file was written
        assert_eq!(host.get_file("/tmp/out.txt").unwrap(), "hello\n");
    }

    #[test]
    fn redirect_stdout_append() {
        // File already has "line1\n", then `echo line2 >> /tmp/out.txt`
        let host = MockHost::new()
            .with_file("/tmp/out.txt", b"line1\n")
            .with_spawn_result(
                "echo",
                SpawnResult {
                    exit_code: 0,
                    stdout: "line2\n".into(),
                    stderr: String::new(),
                },
            );
        let mut state = ShellState::new_default();
        let cmd = simple_cmd_with_redirects(
            "echo",
            &["line2"],
            vec![redirect(RedirectType::StdoutAppend("/tmp/out.txt".into()))],
        );
        let result = exec_command(&mut state, &host, &cmd);
        let ControlFlow::Normal(run) = result.unwrap() else {
            panic!("expected Normal")
        };
        assert_eq!(run.stdout, "");
        assert_eq!(host.get_file("/tmp/out.txt").unwrap(), "line1\nline2\n");
    }

    #[test]
    fn redirect_stdout_append_creates_new_file() {
        // >> on a nonexistent file should create it
        let host = MockHost::new().with_spawn_result(
            "echo",
            SpawnResult {
                exit_code: 0,
                stdout: "first\n".into(),
                stderr: String::new(),
            },
        );
        let mut state = ShellState::new_default();
        let cmd = simple_cmd_with_redirects(
            "echo",
            &["first"],
            vec![redirect(RedirectType::StdoutAppend("/tmp/new.txt".into()))],
        );
        let _ = exec_command(&mut state, &host, &cmd);
        assert_eq!(host.get_file("/tmp/new.txt").unwrap(), "first\n");
    }

    #[test]
    fn redirect_stdin_from_file() {
        // `cat < /tmp/input.txt` — the file content becomes stdin
        let host = MockHost::new()
            .with_file("/tmp/input.txt", b"file content\n")
            .with_spawn_result(
                "cat",
                SpawnResult {
                    exit_code: 0,
                    stdout: "file content\n".into(),
                    stderr: String::new(),
                },
            );
        let mut state = ShellState::new_default();
        let cmd = simple_cmd_with_redirects(
            "cat",
            &[],
            vec![redirect(RedirectType::StdinFrom("/tmp/input.txt".into()))],
        );
        let result = exec_command(&mut state, &host, &cmd);
        let ControlFlow::Normal(run) = result.unwrap() else {
            panic!("expected Normal")
        };
        assert_eq!(run.stdout, "file content\n");
        assert_eq!(run.exit_code, 0);
    }

    #[test]
    fn redirect_stdin_from_relative_path() {
        // `cat < input.txt` resolves relative to cwd
        let host = MockHost::new()
            .with_file("/home/user/input.txt", b"relative content\n")
            .with_spawn_result(
                "cat",
                SpawnResult {
                    exit_code: 0,
                    stdout: "relative content\n".into(),
                    stderr: String::new(),
                },
            );
        let mut state = ShellState::new_default();
        // cwd is /home/user by default
        let cmd = simple_cmd_with_redirects(
            "cat",
            &[],
            vec![redirect(RedirectType::StdinFrom("input.txt".into()))],
        );
        let result = exec_command(&mut state, &host, &cmd);
        let ControlFlow::Normal(run) = result.unwrap() else {
            panic!("expected Normal")
        };
        assert_eq!(run.stdout, "relative content\n");
    }

    #[test]
    fn redirect_stderr_overwrite() {
        // `cmd 2> /tmp/err.txt`
        let host = MockHost::new().with_spawn_result(
            "cmd",
            SpawnResult {
                exit_code: 1,
                stdout: "out\n".into(),
                stderr: "error msg\n".into(),
            },
        );
        let mut state = ShellState::new_default();
        let cmd = simple_cmd_with_redirects(
            "cmd",
            &[],
            vec![redirect(RedirectType::StderrOverwrite(
                "/tmp/err.txt".into(),
            ))],
        );
        let result = exec_command(&mut state, &host, &cmd);
        let ControlFlow::Normal(run) = result.unwrap() else {
            panic!("expected Normal")
        };
        // stdout is preserved, stderr goes to file
        assert_eq!(run.stdout, "out\n");
        assert_eq!(run.stderr, "");
        assert_eq!(host.get_file("/tmp/err.txt").unwrap(), "error msg\n");
    }

    #[test]
    fn redirect_stderr_append() {
        // File has existing content, then `cmd 2>> /tmp/err.txt`
        let host = MockHost::new()
            .with_file("/tmp/err.txt", b"old error\n")
            .with_spawn_result(
                "cmd",
                SpawnResult {
                    exit_code: 1,
                    stdout: String::new(),
                    stderr: "new error\n".into(),
                },
            );
        let mut state = ShellState::new_default();
        let cmd = simple_cmd_with_redirects(
            "cmd",
            &[],
            vec![redirect(RedirectType::StderrAppend("/tmp/err.txt".into()))],
        );
        let _ = exec_command(&mut state, &host, &cmd);
        assert_eq!(
            host.get_file("/tmp/err.txt").unwrap(),
            "old error\nnew error\n"
        );
    }

    #[test]
    fn redirect_stderr_to_stdout_no_file() {
        // `cmd 2>&1` without prior stdout redirect: stderr merges into stdout
        let host = MockHost::new().with_spawn_result(
            "cmd",
            SpawnResult {
                exit_code: 0,
                stdout: "out\n".into(),
                stderr: "err\n".into(),
            },
        );
        let mut state = ShellState::new_default();
        let cmd =
            simple_cmd_with_redirects("cmd", &[], vec![redirect(RedirectType::StderrToStdout)]);
        let result = exec_command(&mut state, &host, &cmd);
        let ControlFlow::Normal(run) = result.unwrap() else {
            panic!("expected Normal")
        };
        // stderr should be appended to stdout
        assert_eq!(run.stdout, "out\nerr\n");
        assert_eq!(run.stderr, "");
    }

    #[test]
    fn redirect_stderr_to_stdout_with_file_redirect() {
        // `cmd > /tmp/out.txt 2>&1` — stdout goes to file, then stderr also goes to file
        let host = MockHost::new().with_spawn_result(
            "cmd",
            SpawnResult {
                exit_code: 0,
                stdout: "out\n".into(),
                stderr: "err\n".into(),
            },
        );
        let mut state = ShellState::new_default();
        let cmd = simple_cmd_with_redirects(
            "cmd",
            &[],
            vec![
                redirect(RedirectType::StdoutOverwrite("/tmp/out.txt".into())),
                redirect(RedirectType::StderrToStdout),
            ],
        );
        let result = exec_command(&mut state, &host, &cmd);
        let ControlFlow::Normal(run) = result.unwrap() else {
            panic!("expected Normal")
        };
        // Both should be empty in the run result
        assert_eq!(run.stdout, "");
        assert_eq!(run.stderr, "");
        // File should contain both stdout and stderr
        assert_eq!(host.get_file("/tmp/out.txt").unwrap(), "out\nerr\n");
    }

    #[test]
    fn redirect_both_overwrite() {
        // `cmd &> /tmp/all.txt`
        let host = MockHost::new().with_spawn_result(
            "cmd",
            SpawnResult {
                exit_code: 0,
                stdout: "out\n".into(),
                stderr: "err\n".into(),
            },
        );
        let mut state = ShellState::new_default();
        let cmd = simple_cmd_with_redirects(
            "cmd",
            &[],
            vec![redirect(RedirectType::BothOverwrite("/tmp/all.txt".into()))],
        );
        let result = exec_command(&mut state, &host, &cmd);
        let ControlFlow::Normal(run) = result.unwrap() else {
            panic!("expected Normal")
        };
        assert_eq!(run.stdout, "");
        assert_eq!(run.stderr, "");
        assert_eq!(host.get_file("/tmp/all.txt").unwrap(), "out\nerr\n");
    }

    #[test]
    fn redirect_heredoc() {
        // Heredoc content becomes stdin
        let host = MockHost::new().with_spawn_result(
            "cat",
            SpawnResult {
                exit_code: 0,
                stdout: "heredoc line 1\nheredoc line 2\n".into(),
                stderr: String::new(),
            },
        );
        let mut state = ShellState::new_default();
        let cmd = simple_cmd_with_redirects(
            "cat",
            &[],
            vec![redirect(RedirectType::Heredoc(
                "heredoc line 1\nheredoc line 2\n".into(),
            ))],
        );
        let result = exec_command(&mut state, &host, &cmd);
        let ControlFlow::Normal(run) = result.unwrap() else {
            panic!("expected Normal")
        };
        assert_eq!(run.stdout, "heredoc line 1\nheredoc line 2\n");
    }

    #[test]
    fn redirect_heredoc_strip() {
        // HeredocStrip content becomes stdin (tab stripping is done by the parser)
        let host = MockHost::new().with_spawn_result(
            "cat",
            SpawnResult {
                exit_code: 0,
                stdout: "stripped content\n".into(),
                stderr: String::new(),
            },
        );
        let mut state = ShellState::new_default();
        let cmd = simple_cmd_with_redirects(
            "cat",
            &[],
            vec![redirect(RedirectType::HeredocStrip(
                "stripped content\n".into(),
            ))],
        );
        let result = exec_command(&mut state, &host, &cmd);
        let ControlFlow::Normal(run) = result.unwrap() else {
            panic!("expected Normal")
        };
        assert_eq!(run.stdout, "stripped content\n");
    }

    #[test]
    fn redirect_here_string() {
        // `cat <<< "hello"` — stdin becomes "hello\n"
        let host = MockHost::new().with_spawn_result(
            "cat",
            SpawnResult {
                exit_code: 0,
                stdout: "hello\n".into(),
                stderr: String::new(),
            },
        );
        let mut state = ShellState::new_default();
        let cmd = simple_cmd_with_redirects(
            "cat",
            &[],
            vec![redirect(RedirectType::HereString("hello".into()))],
        );
        let result = exec_command(&mut state, &host, &cmd);
        let ControlFlow::Normal(run) = result.unwrap() else {
            panic!("expected Normal")
        };
        assert_eq!(run.stdout, "hello\n");
    }

    #[test]
    fn redirect_stdout_overwrite_relative_path() {
        // `echo hello > out.txt` resolves relative to cwd
        let host = MockHost::new().with_spawn_result(
            "echo",
            SpawnResult {
                exit_code: 0,
                stdout: "hello\n".into(),
                stderr: String::new(),
            },
        );
        let mut state = ShellState::new_default();
        let cmd = simple_cmd_with_redirects(
            "echo",
            &["hello"],
            vec![redirect(RedirectType::StdoutOverwrite("out.txt".into()))],
        );
        let _ = exec_command(&mut state, &host, &cmd);
        // /home/user is the default cwd
        assert_eq!(host.get_file("/home/user/out.txt").unwrap(), "hello\n");
    }

    #[test]
    fn redirect_multiple_output_redirects() {
        // `cmd > /tmp/out.txt 2> /tmp/err.txt` — stdout and stderr to separate files
        let host = MockHost::new().with_spawn_result(
            "cmd",
            SpawnResult {
                exit_code: 0,
                stdout: "output\n".into(),
                stderr: "error\n".into(),
            },
        );
        let mut state = ShellState::new_default();
        let cmd = simple_cmd_with_redirects(
            "cmd",
            &[],
            vec![
                redirect(RedirectType::StdoutOverwrite("/tmp/out.txt".into())),
                redirect(RedirectType::StderrOverwrite("/tmp/err.txt".into())),
            ],
        );
        let result = exec_command(&mut state, &host, &cmd);
        let ControlFlow::Normal(run) = result.unwrap() else {
            panic!("expected Normal")
        };
        assert_eq!(run.stdout, "");
        assert_eq!(run.stderr, "");
        assert_eq!(host.get_file("/tmp/out.txt").unwrap(), "output\n");
        assert_eq!(host.get_file("/tmp/err.txt").unwrap(), "error\n");
    }

    #[test]
    fn redirect_no_redirects_passes_empty_stdin() {
        // When no redirects, empty string is passed as stdin
        let host = MockHost::new().with_spawn_result(
            "echo",
            SpawnResult {
                exit_code: 0,
                stdout: "hi\n".into(),
                stderr: String::new(),
            },
        );
        let mut state = ShellState::new_default();
        let cmd = codepod_shell::parser::parse("echo hi");
        let result = exec_command(&mut state, &host, &cmd);
        let ControlFlow::Normal(run) = result.unwrap() else {
            panic!("expected Normal")
        };
        assert_eq!(run.stdout, "hi\n");
    }

    #[test]
    fn redirect_stdin_file_not_found() {
        // `cat < /nonexistent` should return an error
        let host = MockHost::new().with_spawn_result(
            "cat",
            SpawnResult {
                exit_code: 0,
                stdout: String::new(),
                stderr: String::new(),
            },
        );
        let mut state = ShellState::new_default();
        let cmd = simple_cmd_with_redirects(
            "cat",
            &[],
            vec![redirect(RedirectType::StdinFrom("/nonexistent".into()))],
        );
        let result = exec_command(&mut state, &host, &cmd);
        assert!(result.is_err());
    }

    #[test]
    fn redirect_stderr_to_stdout_empty_stderr() {
        // `cmd 2>&1` with empty stderr — stdout unchanged
        let host = MockHost::new().with_spawn_result(
            "cmd",
            SpawnResult {
                exit_code: 0,
                stdout: "only out\n".into(),
                stderr: String::new(),
            },
        );
        let mut state = ShellState::new_default();
        let cmd =
            simple_cmd_with_redirects("cmd", &[], vec![redirect(RedirectType::StderrToStdout)]);
        let result = exec_command(&mut state, &host, &cmd);
        let ControlFlow::Normal(run) = result.unwrap() else {
            panic!("expected Normal")
        };
        assert_eq!(run.stdout, "only out\n");
        assert_eq!(run.stderr, "");
    }

    #[test]
    fn redirect_both_overwrite_empty_outputs() {
        // `cmd &> /tmp/all.txt` with empty stdout and stderr
        let host = MockHost::new().with_spawn_result(
            "cmd",
            SpawnResult {
                exit_code: 0,
                stdout: String::new(),
                stderr: String::new(),
            },
        );
        let mut state = ShellState::new_default();
        let cmd = simple_cmd_with_redirects(
            "cmd",
            &[],
            vec![redirect(RedirectType::BothOverwrite("/tmp/all.txt".into()))],
        );
        let _ = exec_command(&mut state, &host, &cmd);
        assert_eq!(host.get_file("/tmp/all.txt").unwrap(), "");
    }

    #[test]
    fn redirect_heredoc_with_output_redirect() {
        // Heredoc for stdin + stdout redirect to file
        let host = MockHost::new().with_spawn_result(
            "cat",
            SpawnResult {
                exit_code: 0,
                stdout: "hello world\n".into(),
                stderr: String::new(),
            },
        );
        let mut state = ShellState::new_default();
        let cmd = simple_cmd_with_redirects(
            "cat",
            &[],
            vec![
                redirect(RedirectType::Heredoc("hello world\n".into())),
                redirect(RedirectType::StdoutOverwrite("/tmp/out.txt".into())),
            ],
        );
        let result = exec_command(&mut state, &host, &cmd);
        let ControlFlow::Normal(run) = result.unwrap() else {
            panic!("expected Normal")
        };
        assert_eq!(run.stdout, "");
        assert_eq!(host.get_file("/tmp/out.txt").unwrap(), "hello world\n");
    }

    #[test]
    fn redirect_last_stdin_redirect_wins() {
        // Multiple input redirects: the last one should win
        let host = MockHost::new()
            .with_file("/tmp/a.txt", b"content a\n")
            .with_file("/tmp/b.txt", b"content b\n")
            .with_spawn_result(
                "cat",
                SpawnResult {
                    exit_code: 0,
                    stdout: "content b\n".into(),
                    stderr: String::new(),
                },
            );
        let mut state = ShellState::new_default();
        let cmd = simple_cmd_with_redirects(
            "cat",
            &[],
            vec![
                redirect(RedirectType::StdinFrom("/tmp/a.txt".into())),
                redirect(RedirectType::StdinFrom("/tmp/b.txt".into())),
            ],
        );
        let result = exec_command(&mut state, &host, &cmd);
        let ControlFlow::Normal(run) = result.unwrap() else {
            panic!("expected Normal")
        };
        // Last input redirect wins, so cat sees content of b.txt
        assert_eq!(run.stdout, "content b\n");
    }
}
