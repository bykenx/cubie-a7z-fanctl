#![forbid(unsafe_code)]
use cubie_a7z_fanctl::{cli, hwmon::DEFAULT_HWMON_ROOT, io::RealIo, logging::log};
use std::{env, path::Path, process::ExitCode, sync::Arc};
fn main() -> ExitCode {
    match cli::run(
        env::args_os().skip(1),
        Arc::new(RealIo),
        Path::new(DEFAULT_HWMON_ROOT),
    ) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            log("error", e);
            ExitCode::FAILURE
        }
    }
}
