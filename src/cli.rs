use crate::{
    config::{Config, DEFAULT_CONFIG},
    controller::FanController,
    hwmon::HwmonScanner,
    io::FileIo,
    logging::log,
};
use std::{
    ffi::OsString,
    path::{Path, PathBuf},
    sync::Arc,
};
const VERSION: &str = concat!(env!("CARGO_PKG_NAME"), " ", env!("CARGO_PKG_VERSION"));
const HELP: &str = "Usage: cubie-a7z-fanctl [--config PATH] [--check-config | --once]\n  --config PATH    Configuration (default /etc/cubie-a7z-fanctl/config.toml)\n  --check-config   Validate without hardware access\n  --once           One control cycle; keep resulting PWM\n  --help           Show help\n  --version        Show version";
pub fn run(
    args: impl IntoIterator<Item = OsString>,
    io: Arc<dyn FileIo>,
    hwmon_root: &Path,
) -> Result<(), String> {
    let mut args = args.into_iter();
    let mut path = PathBuf::from(DEFAULT_CONFIG);
    let mut mode = None;
    let mut informational = None;
    let mut has_config = false;
    while let Some(arg) = args.next() {
        match arg.to_str() {
            Some("--config") if !has_config => {
                path = args
                    .next()
                    .map(PathBuf::from)
                    .ok_or("--config requires PATH")?;
                has_config = true;
            }
            Some("--once" | "--check-config") if mode.is_none() => {
                mode = arg.to_str().map(str::to_owned)
            }
            Some("--help" | "--version") if informational.is_none() => {
                informational = arg.to_str().map(str::to_owned)
            }
            _ => {
                return Err(format!(
                    "unknown, duplicate or conflicting argument: {}",
                    arg.to_string_lossy()
                ))
            }
        }
    }
    if let Some(info) = informational {
        if mode.is_some() || has_config {
            return Err("--help/--version cannot be combined with control options".into());
        }
        println!("{VERSION}");
        if info == "--help" {
            println!("{HELP}");
        }
        return Ok(());
    }
    let config = Config::load(&path).map_err(|e| e.to_string())?;
    if mode.as_deref() == Some("--check-config") {
        log("info", format!("{}: configuration valid", path.display()));
        return Ok(());
    }
    // The hwmon root is injectable through the library, not a production CLI option.
    let controller = FanController::new(config, io, HwmonScanner::new(hwmon_root));
    crate::service::run(controller, mode.as_deref() == Some("--once"))
}
