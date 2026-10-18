use crate::error::AppError;
use serde::Deserialize;
use std::{
    fs,
    path::{Path, PathBuf},
};
pub const DEFAULT_CONFIG: &str = "/etc/cubie-a7z-fanctl/config.toml";
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    pub temp_paths: Vec<PathBuf>,
    pub temp_min: f64,
    pub temp_max: f64,
    pub min_pwm: u8,
    pub max_pwm: u8,
    pub poll_interval_seconds: u64,
    pub rediscovery_interval_seconds: u64,
    pub disconnected_retry_seconds: u64,
    pub fail_safe_pwm: u8,
    pub shutdown_pwm: Option<u8>,
    pub preferred_hwmon_name: String,
    pub allow_pwm_fallback: bool,
}
impl Config {
    pub fn load(path: &Path) -> Result<Self, AppError> {
        let text =
            fs::read_to_string(path).map_err(|e| AppError(format!("{}: {e}", path.display())))?;
        Self::parse(&text).map_err(|e| AppError(format!("{}: {e}", path.display())))
    }
    pub fn parse(text: &str) -> Result<Self, AppError> {
        let config: Self = toml::from_str(text).map_err(|e| AppError(e.to_string()))?;
        config.validate()?;
        Ok(config)
    }
    pub fn validate(&self) -> Result<(), AppError> {
        let problem =
            if self.temp_paths.is_empty() || self.temp_paths.iter().any(|p| !p.is_absolute()) {
                Some("temp_paths must contain at least one absolute path")
            } else if !self.temp_min.is_finite()
                || !self.temp_max.is_finite()
                || self.temp_min >= self.temp_max
            {
                Some("temp_min and temp_max must be finite and temp_min < temp_max")
            } else if self.min_pwm > self.max_pwm {
                Some("min_pwm must be <= max_pwm")
            } else if self.poll_interval_seconds == 0
                || self.rediscovery_interval_seconds == 0
                || self.disconnected_retry_seconds == 0
            {
                Some("all polling intervals must be greater than zero")
            } else if self.preferred_hwmon_name.trim().is_empty() {
                Some("preferred_hwmon_name must not be empty")
            } else {
                None
            };
        match problem {
            Some(s) => Err(AppError(s.into())),
            None => Ok(()),
        }
    }
}
