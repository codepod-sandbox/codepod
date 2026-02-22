use std::process::ExitCode;

use rustpython::InterpreterBuilderExt;

fn main() -> ExitCode {
    // RustPython's run() handles all CLI arg parsing: -c, -m, script paths, stdin
    rustpython::run(rustpython::InterpreterBuilder::new().init_stdlib())
}
