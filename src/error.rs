use std::{error::Error, fmt};
#[derive(Debug)]
pub struct AppError(pub String);
impl fmt::Display for AppError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}
impl Error for AppError {}
