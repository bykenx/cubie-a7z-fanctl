use cubie_a7z_fanctl::{
    config::Config,
    controller::FanController,
    hwmon::HwmonScanner,
    io::{FileIo, RealIo},
    logging::LogThrottle,
    temperature::{calculate_pwm, TemperatureReader},
};
use std::{
    fs, io,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};
use tempfile::TempDir;
fn config() -> Config {
    Config::parse(include_str!("../config/config.toml")).unwrap()
}
struct Fixture {
    dir: TempDir,
    config: Config,
}
impl Fixture {
    fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir(dir.path().join("hwmon")).unwrap();
        fs::write(dir.path().join("temp"), "55000\n").unwrap();
        let mut config = config();
        config.temp_paths = vec![dir.path().join("temp")];
        Self { dir, config }
    }
    fn device(&self, id: &str, name: &str) -> PathBuf {
        let path = self.dir.path().join("hwmon").join(id);
        fs::create_dir_all(&path).unwrap();
        fs::write(path.join("name"), name).unwrap();
        fs::write(path.join("pwm1"), "0\n").unwrap();
        path.join("pwm1")
    }
    fn controller(&self, io: Arc<dyn FileIo>) -> FanController {
        FanController::new(
            self.config.clone(),
            io,
            HwmonScanner::new(self.dir.path().join("hwmon")),
        )
    }
}
#[test]
fn interpolation_boundaries_and_rounding() {
    let c = config();
    for (t, expected) in [
        (0.0, 60),
        (39.0, 60),
        (40.0, 60),
        (57.5, 158),
        (75.0, 255),
        (100.0, 255),
    ] {
        assert_eq!(calculate_pwm(t, &c), expected);
    }
    for n in -1000..2000 {
        let value = calculate_pwm(f64::from(n) / 10.0, &c);
        assert!((60..=255).contains(&value));
    }
    assert_eq!(calculate_pwm(f64::NAN, &c), c.fail_safe_pwm);
}
#[test]
fn invalid_configurations() {
    let text = include_str!("../config/config.toml");
    for (from, to) in [
        ("temp_min = 40.0", "temp_min = 75.0"),
        ("temp_max = 75.0", "temp_max = nan"),
        ("min_pwm = 60", "min_pwm = 256"),
        ("max_pwm = 255", "max_pwm = -1"),
        ("fail_safe_pwm = 255", "fail_safe_pwm = 999"),
        ("shutdown_pwm = 255", "shutdown_pwm = null"),
        ("shutdown_pwm = 255", "shutdown_pwm = 256"),
        ("poll_interval_seconds = 2", "poll_interval_seconds = 0"),
        (
            "rediscovery_interval_seconds = 10",
            "rediscovery_interval_seconds = 0",
        ),
        (
            "disconnected_retry_seconds = 5",
            "disconnected_retry_seconds = 0",
        ),
        ("max_pwm = 255", "max_pwm = 1"),
    ] {
        assert!(Config::parse(&text.replace(from, to)).is_err(), "{to}");
    }
    assert!(Config::parse(&(text.to_owned() + "\nunknown = true\n")).is_err());
    assert!(Config::parse(&text.replace("min_pwm = 60", "")).is_err());
    assert!(Config::parse(&text.replace("shutdown_pwm = 255", ""))
        .unwrap()
        .shutdown_pwm
        .is_none());
    let mut c = config();
    c.temp_paths.clear();
    assert!(c.validate().is_err());
    c = config();
    c.temp_paths = vec!["relative".into()];
    assert!(c.validate().is_err());
    let error = Config::load(Path::new("/nonexistent-cubie-config"))
        .unwrap_err()
        .to_string();
    assert!(error.contains("/nonexistent-cubie-config"));
}
#[test]
fn temperature_fallback_and_invalid_values() {
    let f = Fixture::new();
    let bad = f.dir.path().join("bad");
    for value in [
        "nan",
        "55000.1",
        "150001",
        "-40001",
        "",
        "9223372036854775808",
    ] {
        fs::write(&bad, value).unwrap();
        let (temp, path) = TemperatureReader::read(
            &RealIo,
            &[
                f.dir.path().join("missing"),
                bad.clone(),
                f.config.temp_paths[0].clone(),
            ],
        )
        .unwrap();
        assert_eq!(temp, 55.0);
        assert_eq!(path, f.config.temp_paths[0]);
    }
    fs::write(&bad, "40000").unwrap();
    assert_eq!(
        TemperatureReader::read(&RealIo, &[bad.clone(), f.config.temp_paths[0].clone()])
            .unwrap()
            .1,
        bad
    );
}
#[test]
fn discovery_preference_stability_and_no_fallback() {
    let f = Fixture::new();
    let fallback = f.device("hwmon1", "other");
    // With the installed defaults, an unrelated PWM device must remain untouched.
    let mut ctl = f.controller(Arc::new(RealIo));
    assert!(ctl.tick(Duration::ZERO).is_err());
    assert_eq!(fs::read_to_string(&fallback).unwrap(), "0\n");
    let preferred = f.device("hwmon9", "pwmfan");
    ctl.tick(Duration::from_secs(10)).unwrap();
    assert_eq!(fs::read_to_string(&preferred).unwrap(), "144\n");
    assert_eq!(fs::read_to_string(&fallback).unwrap(), "0\n");
    let scanner = HwmonScanner::new(f.dir.path().join("hwmon"));
    let result = scanner.scan(&RealIo, "pwmfan", true).unwrap();
    assert_eq!(result.selected.unwrap().pwm, preferred);
    assert!(!result.warnings.is_empty());
    for _ in 0..5 {
        assert_eq!(
            scanner
                .scan(&RealIo, "absent", true)
                .unwrap()
                .selected
                .unwrap()
                .pwm,
            fallback
        );
    }
    assert!(scanner
        .scan(&RealIo, "absent", false)
        .unwrap()
        .selected
        .is_none());
}
#[derive(Default)]
struct MockIo {
    writes: Mutex<Vec<(PathBuf, u8)>>,
    deny: Mutex<bool>,
}
impl FileIo for MockIo {
    fn read(&self, p: &Path) -> io::Result<String> {
        RealIo.read(p)
    }
    fn directories(&self, p: &Path) -> io::Result<Vec<PathBuf>> {
        RealIo.directories(p)
    }
    fn exists(&self, p: &Path) -> io::Result<bool> {
        RealIo.exists(p)
    }
    fn write_pwm(&self, p: &Path, v: u8) -> io::Result<()> {
        if *self.deny.lock().unwrap() {
            return Err(io::Error::from(io::ErrorKind::PermissionDenied));
        }
        RealIo.write_pwm(p, v)?;
        self.writes.lock().unwrap().push((p.into(), v));
        Ok(())
    }
}
#[test]
fn deduplication_disappearance_and_rediscovery() {
    let f = Fixture::new();
    let first = f.device("hwmon0", "pwmfan");
    let io = Arc::new(MockIo::default());
    let mut ctl = f.controller(io.clone());
    ctl.tick(Duration::ZERO).unwrap();
    ctl.tick(Duration::from_secs(2)).unwrap();
    assert_eq!(io.writes.lock().unwrap().len(), 1);
    fs::remove_file(&first).unwrap();
    assert!(ctl.tick(Duration::from_secs(4)).is_err());
    assert!(!first.exists());
    assert_eq!(ctl.wait_duration(), Duration::from_secs(5));
    let second = f.device("hwmon2", "pwmfan");
    ctl.tick(Duration::from_secs(9)).unwrap();
    assert_eq!(io.writes.lock().unwrap().last().unwrap().0, second);
    fs::remove_file(&second).unwrap();
    ctl.tick(Duration::from_secs(11)).unwrap_err();
    fs::write(&second, "0").unwrap();
    ctl.tick(Duration::from_secs(16)).unwrap();
    assert_eq!(io.writes.lock().unwrap().len(), 3);
}
#[test]
fn fail_safe_permission_recovery_and_shutdown() {
    let f = Fixture::new();
    let pwm = f.device("hwmon0", "pwmfan");
    fs::write(pwm.with_file_name("pwm1_enable"), "2\n").unwrap();
    let io = Arc::new(MockIo::default());
    let mut ctl = f.controller(io.clone());
    *io.deny.lock().unwrap() = true;
    let e = ctl.tick(Duration::ZERO).unwrap_err();
    assert!(e.contains("requires write access"));
    *io.deny.lock().unwrap() = false;
    ctl.tick(Duration::from_secs(5)).unwrap();
    fs::remove_file(&f.config.temp_paths[0]).unwrap();
    ctl.tick(Duration::from_secs(7)).unwrap();
    assert_eq!(fs::read_to_string(&pwm).unwrap(), "255\n");
    assert_eq!(
        fs::read_to_string(pwm.with_file_name("pwm1_enable")).unwrap(),
        "2\n"
    );
    ctl.shutdown().unwrap();
    assert_eq!(io.writes.lock().unwrap().len(), 3);
    ctl.config.shutdown_pwm = None;
    ctl.shutdown().unwrap();
    assert_eq!(io.writes.lock().unwrap().len(), 3);
}
#[test]
fn initial_absence_retries_and_no_file_creation() {
    let f = Fixture::new();
    let mut ctl = f.controller(Arc::new(RealIo));
    assert!(ctl.tick(Duration::ZERO).is_err());
    assert_eq!(ctl.wait_duration(), Duration::from_secs(10));
    let pwm = f.device("hwmon5", "pwmfan");
    ctl.tick(Duration::from_secs(10)).unwrap();
    fs::remove_file(&pwm).unwrap();
    assert!(RealIo.write_pwm(&pwm, 100).is_err());
    assert!(!pwm.exists());
}
#[test]
fn throttle_recovers_and_uses_monotonic_time() {
    let mut throttle = LogThrottle::default();
    let interval = Duration::from_secs(10);
    assert!(throttle.ready("x", Duration::ZERO, interval));
    assert!(!throttle.ready("x", Duration::from_secs(2), interval));
    assert!(throttle.ready("x", Duration::from_secs(10), interval));
    assert!(throttle.recover("x"));
    assert!(!throttle.recover("x"));
    assert!(throttle.ready("x", Duration::from_secs(11), interval));
}
#[test]
fn cli_help_version_validation_and_errors() {
    let binary = env!("CARGO_BIN_EXE_cubie-a7z-fanctl");
    let f = Fixture::new();
    let path = f.dir.path().join("config.toml");
    fs::write(&path, include_str!("../config/config.toml")).unwrap();
    for option in ["--help", "--version"] {
        let output = Command::new(binary).arg(option).output().unwrap();
        assert!(output.status.success());
        let text = String::from_utf8(output.stdout).unwrap();
        assert_eq!(
            text.lines().next(),
            Some(concat!(
                env!("CARGO_PKG_NAME"),
                " ",
                env!("CARGO_PKG_VERSION")
            ))
        );
    }
    assert!(Command::new(binary)
        .args(["--check-config", "--config"])
        .arg(&path)
        .output()
        .unwrap()
        .status
        .success());
    for args in [
        vec!["--bad"],
        vec!["--config"],
        vec!["--once", "--check-config"],
        vec!["--version", "--once"],
    ] {
        assert!(!Command::new(binary)
            .args(args)
            .output()
            .unwrap()
            .status
            .success());
    }
    fs::write(&path, "invalid").unwrap();
    assert!(!Command::new(binary)
        .args(["--check-config", "--config"])
        .arg(&path)
        .output()
        .unwrap()
        .status
        .success());
}
// A child test process runs the same production service loop with an injected sysfs root.
#[test]
#[ignore = "subprocess fixture only"]
fn service_child() {
    let Ok(root) = std::env::var("CUBIE_TEST_ROOT") else {
        return;
    };
    let template = include_str!("../config/config.toml");
    let begin = template.find("temp_paths").unwrap();
    let end = template.find(']').unwrap() + 1;
    let temp = Path::new(&root).join("temp");
    let text = format!(
        "{}temp_paths = [{}]{}",
        &template[..begin],
        toml::Value::String(temp.display().to_string()),
        &template[end..]
    );
    let text = text
        .replace("shutdown_pwm = 255", "shutdown_pwm = 240")
        .replace("poll_interval_seconds = 2", "poll_interval_seconds = 60");
    let path = Path::new(&root).join("child.toml");
    fs::write(&path, text).unwrap();
    let mut args = vec![std::ffi::OsString::from("--config"), path.into_os_string()];
    if std::env::var_os("CUBIE_TEST_ONCE").is_some() {
        args.push("--once".into());
    }
    let result =
        cubie_a7z_fanctl::cli::run(args, Arc::new(RealIo), &Path::new(&root).join("hwmon"));
    if let Err(e) = result {
        eprintln!("{e}");
        std::process::exit(1);
    }
}
#[test]
fn subprocess_once_preserves_pwm() {
    let f = Fixture::new();
    let pwm = f.device("hwmon0", "pwmfan");
    let status = Command::new(std::env::current_exe().unwrap())
        .args(["--ignored", "--exact", "service_child", "--nocapture"])
        .env("CUBIE_TEST_ROOT", f.dir.path())
        .env("CUBIE_TEST_ONCE", "1")
        .stdout(Stdio::null())
        .status()
        .unwrap();
    assert!(status.success());
    assert_eq!(fs::read_to_string(pwm).unwrap(), "144\n");
}
#[test]
#[cfg(unix)]
fn subprocess_signals_interrupt_long_wait() {
    for signal in ["-TERM", "-INT"] {
        let f = Fixture::new();
        let pwm = f.device("hwmon0", "pwmfan");
        let mut child = Command::new(std::env::current_exe().unwrap())
            .args(["--ignored", "--exact", "service_child", "--nocapture"])
            .env("CUBIE_TEST_ROOT", f.dir.path())
            .stdout(Stdio::null())
            .spawn()
            .unwrap();
        let start = Instant::now();
        while fs::read_to_string(&pwm).unwrap() == "0\n" {
            if start.elapsed() > Duration::from_secs(5) {
                child.kill().unwrap();
                child.wait().unwrap();
                panic!("service did not write initial PWM");
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        assert!(Command::new("kill")
            .arg(signal)
            .arg(child.id().to_string())
            .status()
            .unwrap()
            .success());
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            if let Some(status) = child.try_wait().unwrap() {
                assert!(status.success());
                break;
            }
            if Instant::now() > deadline {
                child.kill().unwrap();
                child.wait().unwrap();
                panic!("signal did not interrupt wait");
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        assert_eq!(fs::read_to_string(pwm).unwrap(), "240\n");
    }
}

#[test]
fn subprocess_once_missing_device_fails() {
    let f = Fixture::new();
    let output = Command::new(std::env::current_exe().unwrap())
        .args(["--ignored", "--exact", "service_child", "--nocapture"])
        .env("CUBIE_TEST_ROOT", f.dir.path())
        .env("CUBIE_TEST_ONCE", "1")
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("no PWM device"));
}

struct NoHardware;
impl FileIo for NoHardware {
    fn read(&self, _: &Path) -> io::Result<String> {
        panic!("hardware read in check-config")
    }
    fn directories(&self, _: &Path) -> io::Result<Vec<PathBuf>> {
        panic!("hardware scan in check-config")
    }
    fn exists(&self, _: &Path) -> io::Result<bool> {
        panic!("hardware lookup in check-config")
    }
    fn write_pwm(&self, _: &Path, _: u8) -> io::Result<()> {
        panic!("hardware write in check-config")
    }
}
#[test]
fn check_config_never_accesses_hardware() {
    let f = Fixture::new();
    let path = f.dir.path().join("check.toml");
    fs::write(&path, include_str!("../config/config.toml")).unwrap();
    cubie_a7z_fanctl::cli::run(
        vec![
            "--check-config".into(),
            "--config".into(),
            path.into_os_string(),
        ],
        Arc::new(NoHardware),
        Path::new("/unreachable-hardware"),
    )
    .unwrap();
}
