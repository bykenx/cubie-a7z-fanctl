use crate::{config::Config, io::FileIo};
use std::path::PathBuf;
pub struct TemperatureReader;
impl TemperatureReader {
    pub fn read(io: &dyn FileIo, paths: &[PathBuf]) -> Result<(f64, PathBuf), String> {
        let mut errors = Vec::new();
        for path in paths {
            match io.read(path) {
                Ok(text) => match text.trim().parse::<i64>() {
                    Ok(raw) if (-40_000..=150_000).contains(&raw) => {
                        return Ok((raw as f64 / 1000.0, path.clone()))
                    }
                    _ => errors.push(format!(
                        "{}: invalid millidegree temperature",
                        path.display()
                    )),
                },
                Err(e) => errors.push(format!("{}: {e}", path.display())),
            }
        }
        Err(errors.join("; "))
    }
}
/// Pure interpolation; validated Config is the normal caller contract.
pub fn calculate_pwm(temp: f64, config: &Config) -> u8 {
    if !temp.is_finite() {
        return config.fail_safe_pwm;
    }
    if temp <= config.temp_min {
        return config.min_pwm;
    }
    if temp >= config.temp_max {
        return config.max_pwm;
    }
    let span = config.temp_max - config.temp_min;
    let ratio = if span.is_finite() {
        (temp - config.temp_min) / span
    } else {
        // Halve before subtraction to keep extreme but finite TOML bounds safe.
        (temp * 0.5 - config.temp_min * 0.5) / (config.temp_max * 0.5 - config.temp_min * 0.5)
    };
    (f64::from(config.min_pwm) + (f64::from(config.max_pwm) - f64::from(config.min_pwm)) * ratio)
        .round()
        .clamp(0.0, 255.0) as u8
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn extreme_finite_bounds_do_not_overflow() {
        let mut config = Config::parse(include_str!("../config/config.toml")).unwrap();
        config.temp_min = -f64::MAX;
        config.temp_max = f64::MAX;
        config.validate().unwrap();
        assert_eq!(calculate_pwm(0.0, &config), 158);
        assert!((60..=255).contains(&calculate_pwm(f64::MAX * 0.9, &config)));
    }
    #[test]
    fn linear_pwm_boundaries() {
        let config = Config::parse(include_str!("../config/config.toml")).unwrap();
        for (temp, pwm) in [
            (39.0, 60),
            (40.0, 60),
            (57.5, 158),
            (75.0, 255),
            (76.0, 255),
        ] {
            assert_eq!(calculate_pwm(temp, &config), pwm);
        }
    }
}
