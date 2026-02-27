use codepod_shell::ast::Command;

use crate::control::{ControlFlow, RunResult, ShellError};
use crate::expand::{
    expand_braces, expand_globs, expand_words_with_splitting, restore_brace_sentinels,
};
use crate::host::HostInterface;
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
            redirects: _,
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

            // Convert env HashMap to the slice format expected by spawn.
            let env_pairs: Vec<(&str, &str)> = state
                .env
                .iter()
                .map(|(k, v)| (k.as_str(), v.as_str()))
                .collect();

            let spawn_result = host
                .spawn(cmd_name, &args, &env_pairs, &state.cwd, "")
                .map_err(|e| ShellError::HostError(e.to_string()))?;

            state.last_exit_code = spawn_result.exit_code;

            Ok(ControlFlow::Normal(RunResult {
                exit_code: spawn_result.exit_code,
                stdout: spawn_result.stdout,
                stderr: spawn_result.stderr,
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
}
