// Copyright (C) 2026 Chromatic
// Licensed under the GNU AGPL v3.0. See LICENSE file for details.
// Website: https://chromatic.hu

use anyhow::{bail, Context, Result};
use std::io::{Read, Write};
use std::net::{Shutdown, TcpStream};
use std::time::{Duration, Instant};

const ADB_SERVER_PORT: u16 = 5037;
const DEFAULT_TIMEOUT: Duration = Duration::from_secs(3);

#[derive(Debug, Clone, serde::Serialize)]
pub struct AdbServerDeviceInfo {
    pub index: usize,
    pub transport_id: u64,
    pub state: String,
    pub product: Option<String>,
    pub model: Option<String>,
    pub device: Option<String>,
    pub recovery_device: String,
}

#[derive(Debug, Clone)]
pub struct AdbServerTransport {
    transport_id: u64,
    timeout: Duration,
}

pub struct AdbServerSideloadStream {
    stream: TcpStream,
}

struct ListedDevice {
    transport_id: u64,
    state: String,
    product: Option<String>,
    model: Option<String>,
    device: Option<String>,
}

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

fn read_status_text(stream: &mut TcpStream) -> Result<String> {
    let mut status = [0u8; 4];
    stream.read_exact(&mut status)?;
    Ok(String::from_utf8_lossy(&status).to_string())
}

fn read_hex_length(stream: &mut TcpStream) -> Result<usize> {
    let mut length = [0u8; 4];
    stream.read_exact(&mut length)?;
    usize::from_str_radix(&String::from_utf8_lossy(&length), 16)
        .context("invalid length from adb server")
}

fn read_length_prefixed(stream: &mut TcpStream) -> Result<Vec<u8>> {
    let length = read_hex_length(stream)?;
    if length > 1024 * 1024 {
        bail!("adb server response is too large: {length} bytes");
    }
    let mut payload = vec![0; length];
    stream.read_exact(&mut payload)?;
    Ok(payload)
}

fn expect_okay(stream: &mut TcpStream, request: &str) -> Result<()> {
    let sensitive_service = request.starts_with("sideload-host:");
    let request_label = if sensitive_service {
        "sideload-host service"
    } else {
        request
    };
    match read_status_text(stream)?.as_str() {
        "OKAY" => Ok(()),
        "FAIL" => {
            if sensitive_service {
                bail!("adb server rejected {request_label}");
            }
            let message = read_length_prefixed(stream)
                .map(|value| String::from_utf8_lossy(&value).into_owned())
                .unwrap_or_else(|_| "unknown adb server failure".to_owned());
            bail!("adb server rejected {request_label}: {message}")
        }
        status => bail!("unexpected adb server status {status:?} for {request_label}"),
    }
}

fn host_request(request: &str, timeout: Duration) -> Result<TcpStream> {
    let mut stream = connect(ADB_SERVER_PORT, timeout)?;
    send_request(&mut stream, request)?;
    expect_okay(&mut stream, request)?;
    Ok(stream)
}

fn read_stream_text(mut stream: TcpStream, context: &str) -> Result<String> {
    let mut output = Vec::new();
    match stream.read_to_end(&mut output) {
        Ok(_) => {}
        // Windows reports a socket timeout as an unclassified OS error on
        // some toolchains. Once the recovery returned text, a later timeout
        // only means this short service omitted its final close.
        Err(error)
            if !output.is_empty()
                && (matches!(
                    error.kind(),
                    std::io::ErrorKind::TimedOut | std::io::ErrorKind::WouldBlock
                ) || error.raw_os_error() == Some(10060)) => {}
        Err(error) => return Err(error).with_context(|| context.to_owned()),
    }
    let text = String::from_utf8(output).context("adb server returned non-UTF-8 text")?;
    Ok(text.trim_matches(['\0', '\r', '\n']).to_owned())
}

fn parse_device_line(line: &str) -> Option<ListedDevice> {
    let mut fields = line.split_ascii_whitespace();
    let _serial = fields.next()?;
    let state = fields.next()?.to_owned();
    let mut transport_id = None;
    let mut product = None;
    let mut model = None;
    let mut device = None;
    for field in fields {
        let Some((key, value)) = field.split_once(':') else {
            continue;
        };
        match key {
            "transport_id" => transport_id = value.parse().ok(),
            "product" => product = Some(value.to_owned()),
            "model" => model = Some(value.to_owned()),
            "device" => device = Some(value.to_owned()),
            _ => {}
        }
    }
    Some(ListedDevice {
        transport_id: transport_id?,
        state,
        product,
        model,
        device,
    })
}

pub fn discover_mi_recoveries(timeout: Duration) -> Result<Vec<AdbServerDeviceInfo>> {
    let mut stream = host_request("host:devices-l", timeout)?;
    let payload = read_length_prefixed(&mut stream)?;
    let listing = String::from_utf8(payload).context("adb device list is not UTF-8")?;
    let mut recoveries = Vec::new();
    for line in listing.lines() {
        let Some(device_info) = parse_device_line(line) else {
            continue;
        };
        if device_info.state != "sideload" && device_info.state != "recovery" {
            continue;
        }
        let transport_id = device_info.transport_id;
        let transport = AdbServerTransport::new(transport_id);
        let recovery_device = transport
            .query_text_with_timeout("getdevice:", timeout)
            .with_context(|| format!("probing ADB transport {transport_id} as Mi Recovery"))?;
        if recovery_device.is_empty() {
            continue;
        }
        recoveries.push(AdbServerDeviceInfo {
            index: recoveries.len(),
            transport_id,
            state: device_info.state,
            product: device_info.product,
            model: device_info.model,
            device: device_info.device,
            recovery_device,
        });
    }
    Ok(recoveries)
}

impl AdbServerTransport {
    pub fn new(transport_id: u64) -> Self {
        Self {
            transport_id,
            timeout: DEFAULT_TIMEOUT,
        }
    }

    fn open_service_with_timeout(&self, service: &str, timeout: Duration) -> Result<TcpStream> {
        let mut stream =
            host_request(&format!("host:transport-id:{}", self.transport_id), timeout)?;
        send_request(&mut stream, service)?;
        expect_okay(&mut stream, service)?;
        Ok(stream)
    }

    fn query_text_with_timeout(&self, service: &str, timeout: Duration) -> Result<String> {
        let stream = self.open_service_with_timeout(service, timeout)?;
        read_stream_text(stream, &format!("reading {service} through adb server"))
    }

    pub fn query_text(&self, service: &str) -> Result<String> {
        self.query_text_with_timeout(service, self.timeout)
    }

    pub fn command(&self, service: &str) -> Result<()> {
        let stream = self.open_service_with_timeout(service, self.timeout)?;
        let _ = read_stream_text(stream, &format!("running {service} through adb server"));
        Ok(())
    }

    pub fn open_sideload(&self, service: &str) -> Result<AdbServerSideloadStream> {
        let stream = self.open_service_with_timeout(service, Duration::from_secs(30))?;
        Ok(AdbServerSideloadStream { stream })
    }

    pub fn set_timeout(&mut self, timeout: Duration) {
        self.timeout = timeout;
    }
}

impl AdbServerSideloadStream {
    pub fn set_timeout(&mut self, timeout: Duration) {
        self.stream.set_read_timeout(Some(timeout)).ok();
        self.stream.set_write_timeout(Some(timeout)).ok();
    }

    pub fn read_command(&mut self) -> Result<[u8; 8]> {
        let mut command = [0; 8];
        self.stream
            .read_exact(&mut command)
            .context("reading sideload command through adb server")?;
        Ok(command)
    }

    pub fn write_block(&mut self, block: &[u8]) -> Result<()> {
        self.stream
            .write_all(block)
            .context("writing sideload block through adb server")
    }

    pub fn close(self) {
        let _ = self.stream.shutdown(Shutdown::Both);
    }
}

pub fn kill_adb_server(timeout: Duration) -> Result<()> {
    let mut s = connect(ADB_SERVER_PORT, timeout)?;
    // Try host:kill
    send_request(&mut s, "host:kill")?;
    // Read status; server may close immediately on success.
    match read_status_text(&mut s) {
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
    wait_until_stopped(timeout)?;
    Ok(())
}

fn wait_until_stopped(timeout: Duration) -> Result<()> {
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        if !is_running(Duration::from_millis(100)) {
            return Ok(());
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    anyhow::bail!("ADB server did not release port 5037 within {timeout:?}")
}

pub fn is_running(timeout: Duration) -> bool {
    match connect(ADB_SERVER_PORT, timeout) {
        Ok(mut s) => {
            // Send a ping request (host:version) to confirm it's an adb server
            if send_request(&mut s, "host:version").is_ok() {
                if let Ok(st) = read_status_text(&mut s) {
                    return st == "OKAY" || st == "FAIL"; // both indicate a speaking server
                }
            }
            false
        }
        Err(_) => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn device_listing_parser_ignores_the_serial_and_reads_transport_metadata() {
        let parsed = parse_device_line(
            "private-serial\tsideload product:garnet model:xiaomi_for_arm64 device:garnet transport_id:16",
        )
        .unwrap();

        assert_eq!(parsed.transport_id, 16);
        assert_eq!(parsed.state, "sideload");
        assert_eq!(parsed.product.as_deref(), Some("garnet"));
        assert_eq!(parsed.model.as_deref(), Some("xiaomi_for_arm64"));
        assert_eq!(parsed.device.as_deref(), Some("garnet"));
    }

    #[test]
    fn device_listing_parser_requires_a_transport_id() {
        assert!(parse_device_line("serial\tdevice product:sweet").is_none());
    }

    #[test]
    fn sideload_service_errors_never_echo_the_validation_token() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let message = b"secret-token-in-server-error";
            stream.write_all(b"FAIL").unwrap();
            stream
                .write_all(format!("{:04x}", message.len()).as_bytes())
                .unwrap();
            stream.write_all(message).unwrap();
        });
        let mut stream = TcpStream::connect(address).unwrap();
        let error = expect_okay(
            &mut stream,
            "sideload-host:123:65536:secret-token-in-request:0",
        )
        .unwrap_err()
        .to_string();
        server.join().unwrap();

        assert!(error.contains("sideload-host service"));
        assert!(!error.contains("secret-token"));
    }
}
