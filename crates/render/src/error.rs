use std::fmt;

// Runtime: std::runtime_error; DeviceLost: render::DeviceLost; Logic: std::logic_error (a frame the renderer refuses).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    Runtime(String),
    DeviceLost(String),
    Logic(String),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Runtime(s) | Error::DeviceLost(s) | Error::Logic(s) => f.write_str(s),
        }
    }
}

impl std::error::Error for Error {}

pub type Result<T> = std::result::Result<T, Error>;

pub(crate) fn runtime(what: impl Into<String>) -> Error {
    Error::Runtime(what.into())
}

pub(crate) fn check<T>(result: ash::prelude::VkResult<T>, what: &str) -> Result<T> {
    result.map_err(|r| Error::Runtime(format!("{what}: {}", r.as_raw())))
}
