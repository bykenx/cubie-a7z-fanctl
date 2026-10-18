use crate::io::FileIo;
use std::path::{Path, PathBuf};
pub const DEFAULT_HWMON_ROOT: &str = "/sys/class/hwmon";
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Device {
    pub name: String,
    pub pwm: PathBuf,
}
pub struct Discovery {
    pub selected: Option<Device>,
    pub warnings: Vec<String>,
}
pub struct HwmonScanner {
    pub root: PathBuf,
}
impl HwmonScanner {
    pub fn new(root: impl AsRef<Path>) -> Self {
        Self {
            root: root.as_ref().into(),
        }
    }
    pub fn scan(
        &self,
        io: &dyn FileIo,
        preferred: &str,
        fallback: bool,
    ) -> Result<Discovery, String> {
        let mut paths = io
            .directories(&self.root)
            .map_err(|e| format!("{}: {e}", self.root.display()))?;
        paths.sort();
        let mut devices = Vec::new();
        let mut warnings = Vec::new();
        for path in paths {
            if !path
                .file_name()
                .and_then(|s| s.to_str())
                .is_some_and(|s| s.starts_with("hwmon"))
            {
                continue;
            }
            let pwm = path.join("pwm1");
            match io.exists(&pwm) {
                Ok(true) => {}
                Ok(false) => continue,
                Err(e) => {
                    warnings.push(format!("{}: {e}", pwm.display()));
                    continue;
                }
            }
            let name = match io.read(&path.join("name")) {
                Ok(s) => s.trim().to_owned(),
                Err(e) => {
                    warnings.push(format!("{}: {e}", path.join("name").display()));
                    "<unknown>".into()
                }
            };
            devices.push(Device { name, pwm });
        }
        let selected = devices
            .iter()
            .find(|d| d.name == preferred)
            .or_else(|| if fallback { devices.first() } else { None })
            .cloned();
        if devices.len() > 1 {
            warnings.push(format!(
                "{} PWM candidates; selected {}",
                devices.len(),
                selected
                    .as_ref()
                    .map_or_else(|| "none".into(), |d| d.pwm.display().to_string())
            ));
        }
        Ok(Discovery { selected, warnings })
    }
}
