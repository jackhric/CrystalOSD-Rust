use std::fs::File;
use std::io::Write;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Level {
    Error,
    Warn,
    Info,
    Debug,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Subsystem {
    Boot,
    Opening,
    Clock,
    Menus,
    Audio,
    Assets,
    Render,
    App,
}

pub fn level_name(level: Level) -> &'static str {
    match level {
        Level::Error => "error",
        Level::Warn => "warn",
        Level::Info => "info",
        Level::Debug => "debug",
    }
}

pub fn subsystem_name(subsystem: Subsystem) -> &'static str {
    match subsystem {
        Subsystem::Boot => "boot",
        Subsystem::Opening => "opening",
        Subsystem::Clock => "clock",
        Subsystem::Menus => "menus",
        Subsystem::Audio => "audio",
        Subsystem::Assets => "assets",
        Subsystem::Render => "render",
        Subsystem::App => "app",
    }
}

#[derive(Default)]
struct Inner {
    file: Option<File>,
    last_error: String,
}

pub struct Log {
    inner: Mutex<Inner>,
    frame: AtomicU64,
}

impl Log {
    pub fn get() -> &'static Log {
        static LOG: OnceLock<Log> = OnceLock::new();
        LOG.get_or_init(|| Log { inner: Mutex::new(Inner::default()), frame: AtomicU64::new(0) })
    }

    pub fn open(&self, path: &Path) {
        self.lock().file = File::create(path).ok();
    }

    pub fn set_frame(&self, frame: u64) {
        self.frame.store(frame, Ordering::SeqCst);
    }

    pub fn write(&self, level: Level, subsystem: Subsystem, message: &str) {
        let mut inner = self.lock();
        let line = format!("[f{} {} {}] {}", self.frame.load(Ordering::SeqCst), subsystem_name(subsystem), level_name(level), message);
        if level == Level::Error || level == Level::Warn {
            eprintln!("{line}");
            if level == Level::Error {
                inner.last_error = line.clone();
            }
        }
        if let Some(file) = &mut inner.file {
            let _ = writeln!(file, "{line}");
            let _ = file.flush();
        }
    }

    pub fn last_error(&self) -> String {
        self.lock().last_error.clone()
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Inner> {
        self.inner.lock().unwrap_or_else(|e| e.into_inner())
    }
}

pub fn log(level: Level, subsystem: Subsystem, message: &str) {
    Log::get().write(level, subsystem, message);
}

#[macro_export]
macro_rules! log {
    ($level:ident, $subsystem:ident, $($arg:tt)*) => {
        $crate::log($crate::Level::$level, $crate::Subsystem::$subsystem, &format!($($arg)*))
    };
}
