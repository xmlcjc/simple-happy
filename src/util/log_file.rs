//! 应用日志落盘：单文件轮换 + 读取/清空。
//!
//! 设计目标与 fqapp 的 `app_log` 对齐，但实现放在 Rust 侧：
//! - `tracing` 的每条事件都会进入 `logs/app.log`，单文件超过 2 MB 轮换为
//!   `app.log.1`，只保留一份历史文件；
//! - 前端的关键失败（书源、直播、插件）通过 `log_append` 命令写入同一文件，
//!   这样用户导出的是一份完整时间线；
//! - 读取时把轮换文件与当前文件拼接，只返回最后 N 行，避免查看器卡顿。
//!
//! 隐私纪律：调用方只写入方法/阶段/耗时/错误码/标签，不写 URL 参数、Cookie、
//! 密钥与响应体。

use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

use chrono::Local;

const LOG_DIR_NAME: &str = "logs";
const LOG_FILE_NAME: &str = "app.log";
const ROTATED_FILE_NAME: &str = "app.log.1";
/// 单文件上限。2 MB 约等于 2 万行，足够覆盖一次会话的排障窗口。
const MAX_BYTES: u64 = 2 * 1024 * 1024;

fn write_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

pub fn log_dir(storage_dir: &str) -> PathBuf {
    Path::new(storage_dir).join(LOG_DIR_NAME)
}

pub fn log_path(storage_dir: &str) -> PathBuf {
    log_dir(storage_dir).join(LOG_FILE_NAME)
}

/// 追加一行日志；调用方负责给出完整行（含时间戳与级别）。
pub fn append_line(storage_dir: &str, line: &str) -> io::Result<()> {
    let _guard = write_lock().lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    let dir = log_dir(storage_dir);
    fs::create_dir_all(&dir)?;
    let path = dir.join(LOG_FILE_NAME);
    if let Ok(metadata) = fs::metadata(&path) {
        if metadata.len() >= MAX_BYTES {
            rotate(&dir);
        }
    }
    let mut file = OpenOptions::new().create(true).append(true).open(&path)?;
    file.write_all(line.as_bytes())?;
    if !line.ends_with('\n') {
        file.write_all(b"\n")?;
    }
    file.flush()
}

fn rotate(dir: &Path) {
    let current = dir.join(LOG_FILE_NAME);
    let rotated = dir.join(ROTATED_FILE_NAME);
    let _ = fs::remove_file(&rotated);
    let _ = fs::rename(&current, &rotated);
}

/// 读取最近 `limit` 行（旧文件在前），返回实际日志路径。
pub fn read_lines(storage_dir: &str, limit: usize) -> io::Result<(PathBuf, Vec<String>)> {
    let path = log_path(storage_dir);
    let mut lines: Vec<String> = Vec::new();
    if let Ok(content) = fs::read_to_string(log_dir(storage_dir).join(ROTATED_FILE_NAME)) {
        lines.extend(content.lines().map(str::to_string));
    }
    if let Ok(content) = fs::read_to_string(&path) {
        lines.extend(content.lines().map(str::to_string));
    }
    let limit = limit.clamp(1, 5000);
    if lines.len() > limit {
        lines = lines.split_off(lines.len() - limit);
    }
    Ok((path, lines))
}

/// 清空当前日志与轮换文件。
pub fn clear(storage_dir: &str) -> io::Result<()> {
    let dir = log_dir(storage_dir);
    let _ = fs::remove_file(dir.join(LOG_FILE_NAME));
    let _ = fs::remove_file(dir.join(ROTATED_FILE_NAME));
    Ok(())
}

/// 生成一行统一格式的日志：`[I] 2026-10-23 10:11:12.123 tag: message`
pub fn format_line(level: &str, tag: &str, message: &str) -> String {
    let token = match level.to_ascii_lowercase().as_str() {
        "debug" => "D",
        "warn" | "warning" => "W",
        "error" => "E",
        _ => "I",
    };
    let tag = tag.trim().replace(['\n', '\r'], " ");
    let message = message.trim().replace(['\n', '\r'], " ");
    let message: String = message.chars().take(2000).collect();
    format!(
        "[{token}] {} {}: {}",
        Local::now().format("%Y-%m-%d %H:%M:%S%.3f"),
        if tag.is_empty() { "app" } else { tag.as_str() },
        message
    )
}

/// `tracing_subscriber::fmt` 的写入端：把格式化后的记录追加到日志文件。
#[derive(Clone)]
pub struct LogWriter {
    storage_dir: String,
}

impl LogWriter {
    pub fn new(storage_dir: impl Into<String>) -> Self {
        Self {
            storage_dir: storage_dir.into(),
        }
    }
}

impl<'a> tracing_subscriber::fmt::MakeWriter<'a> for LogWriter {
    type Writer = LogFile;

    fn make_writer(&'a self) -> Self::Writer {
        LogFile {
            storage_dir: self.storage_dir.clone(),
            pending: String::new(),
        }
    }
}

pub struct LogFile {
    storage_dir: String,
    /// `tracing` 可能把一条记录分几次 `write` 写入；先攒到换行再整行落盘，
    /// 否则每条片段都会被补一个换行，日志被拆得七零八落。
    pending: String,
}

impl Write for LogFile {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        // 控制台保留一份，便于开发时直接观察；失败不影响落盘。
        let _ = io::stdout().write_all(buf);
        self.pending.push_str(&String::from_utf8_lossy(buf));
        while let Some(position) = self.pending.find('\n') {
            let line: String = self.pending.drain(..=position).collect();
            // 写入失败不能让业务线程 panic：日志只是可观测性，不是主链路。
            let _ = append_line(&self.storage_dir, line.trim_end_matches('\n'));
        }
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        if !self.pending.is_empty() {
            let line = std::mem::take(&mut self.pending);
            let _ = append_line(&self.storage_dir, line.trim_end_matches('\n'));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "reader-log-test-{name}-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn appends_reads_and_clears() {
        let dir = temp_dir("basic");
        let storage = dir.to_string_lossy().to_string();
        append_line(&storage, "[I] one").unwrap();
        append_line(&storage, "[E] two").unwrap();
        let (path, lines) = read_lines(&storage, 10).unwrap();
        assert_eq!(lines, vec!["[I] one", "[E] two"]);
        assert!(path.ends_with("app.log"));
        clear(&storage).unwrap();
        assert!(read_lines(&storage, 10).unwrap().1.is_empty());
    }

    #[test]
    fn read_limit_keeps_tail() {
        let dir = temp_dir("limit");
        let storage = dir.to_string_lossy().to_string();
        for index in 0..20 {
            append_line(&storage, &format!("line-{index}")).unwrap();
        }
        let (_, lines) = read_lines(&storage, 5).unwrap();
        assert_eq!(lines, vec!["line-15", "line-16", "line-17", "line-18", "line-19"]);
    }

    #[test]
    fn writer_joins_fragments_into_one_line() {
        let dir = temp_dir("fragments");
        let storage = dir.to_string_lossy().to_string();
        let mut writer = LogFile {
            storage_dir: storage.clone(),
            pending: String::new(),
        };
        writer.write_all(b"[I] hello ").unwrap();
        writer.write_all(b"world\n").unwrap();
        let (_, lines) = read_lines(&storage, 10).unwrap();
        assert_eq!(lines, vec!["[I] hello world"]);
    }

    #[test]
    fn format_line_is_single_line() {
        let line = format_line("warn", "live", "a\nb\r\nc");
        assert!(!line.contains('\n'));
        assert!(line.starts_with("[W] "));
    }
}
