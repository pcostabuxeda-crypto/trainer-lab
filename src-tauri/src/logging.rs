//! Logs estructurats amb `tracing`, cap a tres llocs:
//! - fitxer diari a la carpeta de logs de l'app (`%LOCALAPPDATA%\cat.trainerlab.app\logs`),
//! - consola (només en desenvolupament),
//! - un buffer circular en memòria que la UI consulta (`get_logs`) i rep en directe (`log`).

use std::collections::VecDeque;
use std::path::Path;
use std::sync::{Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter};
use tracing::field::{Field, Visit};
use tracing::{Event, Level, Subscriber};
use tracing_subscriber::layer::{Context, SubscriberExt};
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::{Layer, filter::LevelFilter, fmt};
use ts_rs::TS;

const CAPACITY: usize = 2000;

static BUFFER: Mutex<VecDeque<LogEntry>> = Mutex::new(VecDeque::new());
static APP: OnceLock<AppHandle> = OnceLock::new();

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
#[ts(export)]
pub enum LogLevel {
    Trace,
    Debug,
    Info,
    Warn,
    Error,
}

#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct LogEntry {
    /// Mil·lisegons des de l'època Unix
    #[ts(type = "number")]
    pub ts: u64,
    pub level: LogLevel,
    pub target: String,
    pub message: String,
}

/// Capa de `tracing` que desa cada esdeveniment al buffer i l'envia a la UI.
struct UiLayer;

impl<S: Subscriber> Layer<S> for UiLayer {
    fn on_event(&self, event: &Event<'_>, _ctx: Context<'_, S>) {
        let mut v = MessageVisitor(String::new());
        event.record(&mut v);
        let meta = event.metadata();
        let entry = LogEntry {
            ts: SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0),
            level: match *meta.level() {
                Level::TRACE => LogLevel::Trace,
                Level::DEBUG => LogLevel::Debug,
                Level::INFO => LogLevel::Info,
                Level::WARN => LogLevel::Warn,
                Level::ERROR => LogLevel::Error,
            },
            target: meta.target().to_string(),
            message: v.0,
        };
        push(entry.clone());
        if let Some(app) = APP.get() {
            let _ = app.emit("log", entry);
        }
    }
}

/// Junta el `message` i els camps extra (`clau=valor`) en una sola línia.
struct MessageVisitor(String);

impl Visit for MessageVisitor {
    fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
        if !self.0.is_empty() {
            self.0.push(' ');
        }
        if field.name() == "message" {
            self.0.push_str(&format!("{value:?}"));
        } else {
            self.0.push_str(&format!("{}={value:?}", field.name()));
        }
    }
    fn record_str(&mut self, field: &Field, value: &str) {
        if field.name() == "message" {
            if !self.0.is_empty() {
                self.0.push(' ');
            }
            self.0.push_str(value);
        } else {
            self.record_debug(field, &value);
        }
    }
}

fn push(entry: LogEntry) {
    let mut buf = BUFFER.lock().unwrap_or_else(|e| e.into_inner());
    if buf.len() == CAPACITY {
        buf.pop_front();
    }
    buf.push_back(entry);
}

/// Últimes `limit` entrades amb nivell >= `min_level`, de la més antiga a la més nova.
pub fn recent(limit: usize, min_level: LogLevel) -> Vec<LogEntry> {
    let buf = BUFFER.lock().unwrap_or_else(|e| e.into_inner());
    let mut out: Vec<LogEntry> = buf.iter().rev().filter(|e| e.level >= min_level).take(limit).cloned().collect();
    out.reverse();
    out
}

pub fn init(app: &AppHandle, log_dir: &Path) {
    let _ = APP.set(app.clone());
    let file = tracing_appender::rolling::daily(log_dir, "trainer-lab.log");
    let level = if cfg!(debug_assertions) { LevelFilter::DEBUG } else { LevelFilter::INFO };
    let console = cfg!(debug_assertions).then(fmt::layer);
    tracing_subscriber::registry()
        .with(fmt::layer().with_ansi(false).with_writer(file))
        .with(console)
        .with(UiLayer)
        .with(level)
        .init();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ring_buffer_keeps_last_entries_and_filters() {
        for i in 0..CAPACITY + 5 {
            let level = if i % 2 == 0 { LogLevel::Info } else { LogLevel::Error };
            push(LogEntry { ts: i as u64, level, target: "t".into(), message: i.to_string() });
        }
        let all = recent(usize::MAX, LogLevel::Trace);
        assert_eq!(all.len(), CAPACITY);
        assert_eq!(all.last().unwrap().ts, (CAPACITY + 4) as u64);
        let errors = recent(3, LogLevel::Error);
        assert!(errors.iter().all(|e| e.level == LogLevel::Error));
        assert_eq!(errors.len(), 3);
        assert!(errors[0].ts < errors[2].ts);
    }
}
