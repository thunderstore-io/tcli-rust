use std::sync::{Arc, OnceLock, RwLock};

use serde::Serialize;

static SINK: OnceLock<RwLock<Option<Arc<dyn ProgressSink>>>> = OnceLock::new();

fn get_sink_lock() -> &'static RwLock<Option<Arc<dyn ProgressSink>>> {
    SINK.get_or_init(|| RwLock::new(None))
}

/// Install a progress sink globally.
pub fn set_sink(sink: Arc<dyn ProgressSink>) {
    *get_sink_lock().write().unwrap() = Some(sink);
}

/// Clear the global progress sink.
pub fn clear_sink() {
    *get_sink_lock().write().unwrap() = None;
}

/// Emit a progress event to the current sink, if any.
pub fn emit(event: ProgressEvent) {
    if let Some(sink) = get_sink_lock().read().unwrap().as_ref() {
        sink.emit(event.clone());
    }
}

/// Start a new scope.
pub fn scope_start(id: impl Into<String>, label: impl Into<String>) {
    emit(ProgressEvent::ScopeStarted {
        id: id.into(),
        parent: None,
        label: label.into(),
        total: None,
    });
}

/// Start a new child scope under an existing parent.
pub fn scope_start_child(id: impl Into<String>, parent: impl Into<String>, label: impl Into<String>) {
    emit(ProgressEvent::ScopeStarted {
        id: id.into(),
        parent: Some(parent.into()),
        label: label.into(),
        total: None,
    });
}

/// Update progress within a scope.
pub fn scope_progress(id: impl Into<String>, current: u64, status: Option<&str>) {
    emit(ProgressEvent::ScopeProgress {
        id: id.into(),
        current,
        status: status.map(String::from),
    });
}

/// Mark a scope as completed.
pub fn scope_complete(id: impl Into<String>) {
    emit(ProgressEvent::ScopeCompleted { id: id.into() });
}

/// Mark a scope as failed.
pub fn scope_fail(id: impl Into<String>, error: impl Into<String>) {
    emit(ProgressEvent::ScopeFailed {
        id: id.into(),
        error: error.into(),
    });
}

/// Emit a log message.
pub fn log(level: LogLevel, message: impl Into<String>) {
    emit(ProgressEvent::Log {
        level,
        message: message.into(),
        scope: None,
    });
}

/// Emit an info log message.
pub fn info(message: impl Into<String>) {
    log(LogLevel::Info, message);
}

/// Emit a warning log message.
pub fn warn(message: impl Into<String>) {
    log(LogLevel::Warn, message);
}

/// Emit an error log message.
pub fn error(message: impl Into<String>) {
    log(LogLevel::Error, message);
}

/// Progress events emitted during operations.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ProgressEvent {
    /// A scoped operation has started.
    ScopeStarted {
        id: String,
        parent: Option<String>,
        label: String,
        total: Option<u64>,
    },

    /// Progress within a scope.
    ScopeProgress {
        id: String,
        current: u64,
        status: Option<String>,
    },

    /// Scope completed successfully.
    ScopeCompleted { id: String },

    /// Scope failed.
    ScopeFailed { id: String, error: String },

    /// Log message.
    Log {
        level: LogLevel,
        message: String,
        scope: Option<String>,
    },
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LogLevel {
    Debug,
    Info,
    Warn,
    Error,
}

/// Receiver for progress events.
pub trait ProgressSink: Send + Sync {
    fn emit(&self, event: ProgressEvent);
}

/// Discards all events.
pub struct VoidSink;

impl ProgressSink for VoidSink {
    fn emit(&self, _event: ProgressEvent) {}
}

/// Collects events for testing.
pub struct CollectorSink {
    events: std::sync::Mutex<Vec<ProgressEvent>>,
}

impl CollectorSink {
    pub fn new() -> Self {
        Self {
            events: std::sync::Mutex::new(Vec::new()),
        }
    }

    pub fn events(&self) -> Vec<ProgressEvent> {
        self.events.lock().unwrap().clone()
    }
}

impl ProgressSink for CollectorSink {
    fn emit(&self, event: ProgressEvent) {
        self.events.lock().unwrap().push(event);
    }
}

/// Terminal sink using indicatif for progress bars.
pub struct TerminalSink {
    multi: indicatif::MultiProgress,
    bars: std::sync::Mutex<std::collections::HashMap<String, (indicatif::ProgressBar, String)>>,
}

impl TerminalSink {
    pub fn new() -> Self {
        Self {
            multi: indicatif::MultiProgress::new(),
            bars: std::sync::Mutex::new(std::collections::HashMap::new()),
        }
    }

    fn get_style() -> indicatif::ProgressStyle {
        indicatif::ProgressStyle::with_template("{spinner:.cyan} {msg}")
            .unwrap()
            .tick_strings(&["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"])
    }
}

impl ProgressSink for TerminalSink {
    fn emit(&self, event: ProgressEvent) {
        let mut bars = self.bars.lock().unwrap();

        match event {
            ProgressEvent::ScopeStarted { id, parent: _, label, total: _ } => {
                let bar = self.multi.add(indicatif::ProgressBar::new_spinner());
                bar.set_style(Self::get_style());
                bar.set_message(label.clone());
                bar.enable_steady_tick(std::time::Duration::from_millis(80));
                bars.insert(id, (bar, label));
            }
            ProgressEvent::ScopeProgress { id, current: _, status } => {
                if let Some((bar, label)) = bars.get(&id) {
                    if let Some(s) = status {
                        bar.set_message(format!("{} ({})", label, s));
                    }
                }
            }
            ProgressEvent::ScopeCompleted { id } => {
                if let Some((bar, _)) = bars.remove(&id) {
                    bar.finish_and_clear();
                }
            }
            ProgressEvent::ScopeFailed { id, error } => {
                if let Some((bar, label)) = bars.remove(&id) {
                    bar.abandon_with_message(format!("✗ {}: {}", label, error));
                }
            }
            ProgressEvent::Log { level, message, .. } => {
                let prefix = match level {
                    LogLevel::Debug => "DEBUG",
                    LogLevel::Info => "INFO ",
                    LogLevel::Warn => "WARN ",
                    LogLevel::Error => "ERROR",
                };
                self.multi.println(format!("[{}] {}", prefix, message)).ok();
            }
        }
    }
}
