use std::{
    fs::{self, OpenOptions},
    io::{self, Write},
    path::{Path, PathBuf},
};
/// Small boundary for deterministic tests and fault injection. Never creates PWM nodes.
pub trait FileIo: Send + Sync {
    fn read(&self, path: &Path) -> io::Result<String>;
    fn directories(&self, root: &Path) -> io::Result<Vec<PathBuf>>;
    fn exists(&self, path: &Path) -> io::Result<bool>;
    fn write_pwm(&self, path: &Path, value: u8) -> io::Result<()>;
}
pub struct RealIo;
impl FileIo for RealIo {
    fn read(&self, path: &Path) -> io::Result<String> {
        fs::read_to_string(path)
    }
    fn directories(&self, root: &Path) -> io::Result<Vec<PathBuf>> {
        fs::read_dir(root)?.map(|e| e.map(|e| e.path())).collect()
    }
    fn exists(&self, path: &Path) -> io::Result<bool> {
        path.try_exists()
    }
    fn write_pwm(&self, path: &Path, value: u8) -> io::Result<()> {
        // No create: a hot-unplug race must never create a fake control file.
        let mut file = OpenOptions::new().write(true).truncate(true).open(path)?;
        write_pwm_message(&mut file, value)
    }
}

// sysfs invokes store() once per write(2), not once per formatted message.
// Never send a trailing fragment as a second attribute update.
fn write_pwm_message(writer: &mut impl Write, value: u8) -> io::Result<()> {
    let payload = format!("{value}\n");
    let written = writer.write(payload.as_bytes())?;
    if written != payload.len() {
        return Err(io::Error::new(
            io::ErrorKind::WriteZero,
            format!(
                "short PWM attribute write: {written}/{} bytes",
                payload.len()
            ),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Default)]
    struct SysfsStore {
        calls: usize,
        value: Option<u8>,
    }
    impl Write for SysfsStore {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            self.calls += 1;
            let text = std::str::from_utf8(bytes).map_err(|_| io::ErrorKind::InvalidInput)?;
            // Each syscall must be one complete, valid attribute update.
            let value = text
                .strip_suffix('\n')
                .ok_or(io::ErrorKind::InvalidInput)?
                .parse::<u8>()
                .map_err(|_| io::ErrorKind::InvalidInput)?;
            self.value = Some(value);
            Ok(bytes.len())
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }
    #[test]
    fn pwm_is_a_single_complete_sysfs_store() {
        for value in [0, 60, 128, 255] {
            let mut store = SysfsStore::default();
            write_pwm_message(&mut store, value).unwrap();
            assert_eq!(store.calls, 1);
            assert_eq!(store.value, Some(value));
        }
    }
    #[test]
    fn short_store_does_not_write_a_trailing_fragment() {
        struct ShortStore {
            calls: usize,
        }
        impl Write for ShortStore {
            fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
                self.calls += 1;
                Ok(bytes.len() - 1)
            }
            fn flush(&mut self) -> io::Result<()> {
                Ok(())
            }
        }
        let mut store = ShortStore { calls: 0 };
        assert_eq!(
            write_pwm_message(&mut store, 128).unwrap_err().kind(),
            io::ErrorKind::WriteZero
        );
        assert_eq!(store.calls, 1);
    }
}
