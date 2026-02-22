use chrono::Utc;
use std::fs;
use std::path::Path;

const LOGS_DIR: &str = "logs";

pub fn time_bucket_15m(d: &chrono::DateTime<Utc>) -> String {
    let y = d.format("%Y");
    let month = d.format("%m");
    let day = d.format("%d");
    let h = d.format("%H");
    let min = (d.format("%M").to_string().parse::<i32>().unwrap_or(0) / 15) * 15;
    let min_str = format!("{:02}", min);
    format!("{}-{}-{}_{}-{}", y, month, day, h, min_str)
}

fn ensure_logs_dir() {
    let _ = fs::create_dir_all(LOGS_DIR);
}

pub fn append_monitor_log(line: &str, at: &chrono::DateTime<Utc>) {
    ensure_logs_dir();
    let bucket = time_bucket_15m(at);
    let filename = format!("monitor_{}.log", bucket);
    let filepath = Path::new(LOGS_DIR).join(&filename);
    if let Ok(mut f) = fs::OpenOptions::new().create(true).append(true).open(&filepath) {
            use std::io::Write;
            let _ = writeln!(f, "{}", line);
    }
}

pub fn append_monitor_log_with_timestamp(message: &str) {
    let at = Utc::now();
    let line = format!("[{}] {}", at.to_rfc3339(), message);
    append_monitor_log(&line, &at);
}
