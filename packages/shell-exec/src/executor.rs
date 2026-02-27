use codepod_shell::ast::{Command, WordPart};

use crate::control::{ControlFlow, RunResult, ShellError};
use crate::host::HostInterface;
use crate::state::ShellState;

/// Extract the literal text from a single `WordPart`.
///
/// For Task 3 we only handle `Literal` and `QuotedLiteral`; all other
/// variants are placeholders that return an empty string.
fn expand_word_part(part: &WordPart) -> String {
    match part {
        WordPart::Literal(s) | WordPart::QuotedLiteral(s) => s.clone(),
        // Placeholders for future expansion phases:
        WordPart::Variable(_)
        | WordPart::CommandSub(_)
        | WordPart::ParamExpansion { .. }
        | WordPart::ArithmeticExpansion(_)
        | WordPart::ProcessSub(_) => String::new(),
    }
}

/// Expand all parts of a `Word` into a single string by concatenating
/// the expansions of its parts.
fn expand_word(word: &codepod_shell::ast::Word) -> String {
    word.parts.iter().map(expand_word_part).collect()
}

/// Execute a parsed `Command` AST node.
///
/// For Task 3 only the `Command::Simple` variant is implemented.
/// All other variants return an empty `RunResult`.
pub fn exec_command(
    state: &mut ShellState,
    host: &dyn HostInterface,
    cmd: &Command,
) -> Result<ControlFlow, ShellError> {
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

            let expanded: Vec<String> = words.iter().map(expand_word).collect();
            let cmd_name = &expanded[0];
            let args: Vec<&str> = expanded[1..].iter().map(|s| s.as_str()).collect();

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
}
