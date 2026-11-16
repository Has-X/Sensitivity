//! Opt-in structured support logs. Callers must pass metadata, never payloads,
//! identities, paths, URLs, command arguments, or validation tokens.
use serde_json::{json, Value};
use std::fs::{File, OpenOptions};
use std::io::Write;
use std::sync::{Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

static LOG: OnceLock<Mutex<Option<File>>> = OnceLock::new();

fn failure_metadata(error: &anyhow::Error) -> Value {
    let causes: Vec<Value> = error.chain().map(|cause| {
        if let Some(error) = cause.downcast_ref::<reqwest::Error>() {
            json!({"kind": "http", "status": error.status().map(|status| status.as_u16()),
                "timeout": error.is_timeout(), "connect": error.is_connect(), "body": error.is_body(), "decode": error.is_decode()})
        } else if let Some(error) = cause.downcast_ref::<std::io::Error>() {
            json!({"kind": "io", "code": format!("{:?}", error.kind()), "os_code": error.raw_os_error()})
        } else if let Some(error) = cause.downcast_ref::<rusb::Error>() {
            json!({"kind": "usb", "code": format!("{error:?}")})
        } else if let Some(error) = cause.downcast_ref::<serde_json::Error>() {
            json!({"kind": "json", "category": format!("{:?}", error.classify()), "line": error.line(), "column": error.column()})
        } else {
            json!({"kind": "context"})
        }
    }).collect();
    json!({"causes": causes})
}

pub fn failure(error: &anyhow::Error) {
    event("backend.failed", failure_metadata(error));
}

pub fn event(name: &'static str, details: Value) {
    let log = LOG.get_or_init(|| {
        Mutex::new(
            std::env::var_os("SENSITIVITY_LOG_FILE")
                .and_then(|path| OpenOptions::new().create(true).append(true).open(path).ok()),
        )
    });
    if let Ok(mut guard) = log.lock() {
        if let Some(file) = guard.as_mut() {
            if file
                .metadata()
                .map(|m| m.len() > 5 * 1024 * 1024)
                .unwrap_or(true)
            {
                return;
            }
            let record = json!({"time_ms": SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_millis(),
                "event": name, "details": details});
            let _ = writeln!(file, "{record}");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn failure_metadata_keeps_error_codes_without_private_messages() {
        let error = anyhow::Error::new(std::io::Error::new(
            std::io::ErrorKind::PermissionDenied,
            "private-user-path-and-token",
        ))
        .context("private-url-and-serial");
        let metadata = failure_metadata(&error).to_string();
        assert!(metadata.contains("PermissionDenied"));
        assert!(!metadata.contains("private-"));
        let error = anyhow::Error::new(rusb::Error::InvalidParam).context("private-identity");
        let metadata = failure_metadata(&error).to_string();
        assert!(metadata.contains("InvalidParam"));
        assert!(!metadata.contains("private-identity"));
    }
}
