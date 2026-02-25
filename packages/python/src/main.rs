use std::process::ExitCode;

use rustpython::InterpreterBuilderExt;

fn main() -> ExitCode {
    let config = rustpython::InterpreterBuilder::new().init_stdlib();

    #[cfg(feature = "numpy")]
    let config = config.add_native_module(numpy_rust_python::numpy_module_def(&config.ctx));

    #[cfg(feature = "pandas")]
    let config = config.add_native_module(pandas_native::module_def(&config.ctx));

    #[cfg(feature = "pil")]
    let config = config.add_native_module(pil_native::module_def(&config.ctx));

    #[cfg(feature = "matplotlib")]
    let config = config.add_native_module(matplotlib_native::module_def(&config.ctx));

    #[cfg(feature = "sklearn")]
    let config = config.add_native_module(sklearn_native::module_def(&config.ctx));

    #[cfg(feature = "sqlite3")]
    let config = config.add_native_module(sqlite3_native::module_def(&config.ctx));

    rustpython::run(config)
}
