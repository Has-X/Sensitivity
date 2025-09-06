// Copyright (C) 2025 HasX
// Licensed under the GNU AGPL v3.0. See LICENSE file for details.
// Website: https://hasx.dev

use anyhow::{Context, Result};
use std::io::{Read, Write};
use std::net::{Shutdown, TcpListener, TcpStream};
use std::time::Duration;

fn connect(port: u16, timeout: Duration) -> Result<TcpStream> {
    let addr = format!("127.0.0.1:{}", port);
    let stream = TcpStream::connect(addr).context("connect adb server 127.0.0.1:5037")?;
    stream.set_read_timeout(Some(timeout)).ok();
    stream.set_write_timeout(Some(timeout)).ok();
    Ok(stream)
}

fn send_request(stream: &mut TcpStream, req: &str) -> Result<()> {
    let len = req.len();
    let header = format!("{:04x}", len);
    stream.write_all(header.as_bytes())?;
    stream.write_all(req.as_bytes())?;
    Ok(())
}

fn read_status(stream: &mut TcpStream) -> Result<String> {
    let mut status = [0u8; 4];
    stream.read_exact(&mut status)?;
    Ok(String::from_utf8_lossy(&status).to_string())
}

pub fn kill_adb_server(timeout: Duration) -> Result<()> {
    let mut s = match connect(5037, timeout) {
        Ok(s) => s,
        Err(e) => return Err(e),
    };
    // Try host:kill
    send_request(&mut s, "host:kill")?;
    // Read status; server may close immediately on success.
    match read_status(&mut s) {
        Ok(st) if st == "OKAY" => {}
        Ok(st) if st == "FAIL" => {
            // Read length and payload for diagnostics
            let mut len_buf = [0u8; 4];
            if s.read_exact(&mut len_buf).is_ok() {
                if let Ok(n) = usize::from_str_radix(&String::from_utf8_lossy(&len_buf), 16) {
                    let mut v = vec![0u8; n];
                    let _ = s.read_exact(&mut v);
                    let msg = String::from_utf8_lossy(&v);
                    return Err(anyhow::anyhow!(format!("adb server FAIL: {}", msg)));
                }
            }
        }
        _ => {}
    }
    let _ = s.shutdown(Shutdown::Both);
    Ok(())
}

pub fn is_running(timeout: Duration) -> bool {
    match connect(5037, timeout) {
        Ok(mut s) => {
            // Send a ping request (host:version) to confirm it's an adb server
            if send_request(&mut s, "host:version").is_ok() {
                if let Ok(st) = read_status(&mut s) {
                    return st == "OKAY" || st == "FAIL"; // both indicate a speaking server
                }
            }
            true
        }
        Err(_) => false,
    }
}

#[cfg(windows)]
pub fn kill_adb_process() {
    // Best-effort: use taskkill to terminate adb.exe if present
    let _ = std::process::Command::new("taskkill")
        .args(["/F", "/IM", "adb.exe", "/T"]) // force, by image name, include child processes
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status();
}

// Bind to 127.0.0.1:5037 to prevent adb server from reappearing during our session.
// Return the listener so the caller can keep it alive (drop releases the port).
pub fn block_port_5037() -> Option<TcpListener> {
    match TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 5037)) {
        Ok(l) => Some(l),
        Err(_) => None,
    }
}
