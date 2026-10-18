use crate::{
    config::Config,
    hwmon::{Device, HwmonScanner},
    io::FileIo,
    logging::{log, LogThrottle},
    temperature::{calculate_pwm, TemperatureReader},
};
use std::{io, sync::Arc, time::Duration};
pub struct FanController {
    pub config: Config,
    io: Arc<dyn FileIo>,
    scanner: HwmonScanner,
    device: Option<Device>,
    last_pwm: Option<u8>,
    reported_device: Option<Device>,
    logs: LogThrottle,
    wait: Duration,
}
impl FanController {
    pub fn new(config: Config, io: Arc<dyn FileIo>, scanner: HwmonScanner) -> Self {
        Self {
            config,
            io,
            scanner,
            device: None,
            last_pwm: None,
            reported_device: None,
            logs: LogThrottle::default(),
            wait: Duration::ZERO,
        }
    }
    fn warn(
        &mut self,
        key: &'static str,
        now: Duration,
        level: &str,
        message: impl std::fmt::Display,
    ) {
        if self.logs.ready(
            key,
            now,
            Duration::from_secs(self.config.rediscovery_interval_seconds),
        ) {
            log(level, message);
        }
    }
    pub fn wait_duration(&self) -> Duration {
        self.wait
    }
    /// Caller injects monotonic elapsed time. One tick never sleeps.
    pub fn tick(&mut self, now: Duration) -> Result<(), String> {
        self.wait = Duration::from_secs(self.config.poll_interval_seconds);
        if self.device.is_none() {
            self.last_pwm = None;
            match self.scanner.scan(
                self.io.as_ref(),
                &self.config.preferred_hwmon_name,
                self.config.allow_pwm_fallback,
            ) {
                Ok(discovery) => {
                    if self.logs.recover("scan") {
                        log("info", "hwmon scanning recovered");
                    }
                    if discovery.warnings.is_empty() {
                        self.logs.recover("scan-warnings");
                    }
                    if !discovery.warnings.is_empty() {
                        self.warn("scan-warnings", now, "warn", discovery.warnings.join("; "));
                    }
                    self.device = discovery.selected;
                }
                Err(e) => self.warn("scan", now, "warn", e),
            }
            if let Some(device) = &self.device {
                if self.reported_device.as_ref() != Some(device) {
                    log(
                        "info",
                        format!(
                            "selected hwmon name={} pwm={}",
                            device.name,
                            device.pwm.display()
                        ),
                    );
                    let enable = device.pwm.with_file_name("pwm1_enable");
                    match self.io.read(&enable) {
                        Ok(s) => log(
                            "info",
                            format!("{}={} (read-only inspection)", enable.display(), s.trim()),
                        ),
                        Err(e) if e.kind() == io::ErrorKind::NotFound => {}
                        Err(e) => log("warn", format!("{}: {e}", enable.display())),
                    }
                    self.reported_device = Some(device.clone());
                }
                if self.logs.recover("missing") {
                    log("info", "PWM device recovered");
                }
            }
        }
        let target = match TemperatureReader::read(self.io.as_ref(), &self.config.temp_paths) {
            Ok((temp, _)) => {
                if self.logs.recover("temperature") {
                    log("info", "temperature sensor recovered");
                }
                calculate_pwm(temp, &self.config)
            }
            Err(e) => {
                self.warn(
                    "temperature",
                    now,
                    "warn",
                    format!(
                        "no valid temperature; fail-safe PWM={}: {e}",
                        self.config.fail_safe_pwm
                    ),
                );
                self.config.fail_safe_pwm
            }
        };
        if self.device.is_none() {
            self.wait = Duration::from_secs(self.config.rediscovery_interval_seconds);
            self.warn(
                "missing",
                now,
                "warn",
                "no PWM device; waiting for rediscovery",
            );
            return Err("no PWM device".into());
        }
        match self.write(target) {
            Ok(()) => {
                if self.logs.recover("write") {
                    log("info", "PWM writes recovered");
                }
                Ok(())
            }
            Err(e) => {
                self.warn("write", now, "error", &e);
                self.device = None;
                self.last_pwm = None;
                self.wait = Duration::from_secs(self.config.disconnected_retry_seconds);
                Err(e)
            }
        }
    }
    fn write(&mut self, value: u8) -> Result<(), String> {
        let device = self.device.as_ref().ok_or("no PWM device")?;
        match self.io.exists(&device.pwm) {
            Ok(true) => {}
            Ok(false) => return Err(format!("{} disappeared", device.pwm.display())),
            Err(e) => return Err(Self::write_error(&device.pwm, e)),
        }
        if self.last_pwm == Some(value) {
            return Ok(());
        }
        self.io
            .write_pwm(&device.pwm, value)
            .map_err(|e| Self::write_error(&device.pwm, e))?;
        self.last_pwm = Some(value);
        log("info", format!("PWM {} <- {value}", device.pwm.display()));
        Ok(())
    }
    fn write_error(path: &std::path::Path, e: io::Error) -> String {
        let hint = if e.kind() == io::ErrorKind::PermissionDenied {
            "; service requires write access to sysfs PWM (check root permissions and sandbox)"
        } else {
            ""
        };
        format!("{}: {e}{hint}", path.display())
    }
    pub fn shutdown(&mut self) -> Result<(), String> {
        let Some(value) = self.config.shutdown_pwm else {
            return Ok(());
        };
        if self.device.is_none() {
            let discovery = self.scanner.scan(
                self.io.as_ref(),
                &self.config.preferred_hwmon_name,
                self.config.allow_pwm_fallback,
            )?;
            for warning in discovery.warnings {
                log("warn", warning);
            }
            self.device = discovery.selected;
        }
        // Always attempt a fresh shutdown write, even if a prior write used this value.
        self.last_pwm = None;
        self.write(value)
    }
}
