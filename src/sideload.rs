use std::fs::File;
use std::io::{BufReader, Read, Seek, SeekFrom};
use std::path::Path;

use anyhow::{bail, Context, Result};
use indicatif::{ProgressBar, ProgressStyle};

use crate::adb::{AdbStream, A_CLSE, A_OKAY, A_WRTE};
use crate::mi::MiClient;

pub fn sideload_zip(
    client: &mut MiClient,
    path: &Path,
    chunk_size: usize,
    validate_token: &str,
    allow_wipe: bool,
) -> Result<()> {
    let file = File::open(path).with_context(|| format!("Opening {}", path.display()))?;
    let total = file.metadata()?.len();
    if chunk_size == 0 || chunk_size > 1024 * 1024 {
        bail!("Invalid chunk size: {}", chunk_size);
    }

    // The last field is the wipe flag. Some cross-region updates require data wipe.
    // When server indicates Erase==1, we must send ":1"; otherwise ":0" will make recovery abort.
    let host_str = format!(
        "sideload-host:{}:{}:{}:{}",
        total,
        chunk_size,
        validate_token,
        if allow_wipe { 1 } else { 0 }
    );
    let (mut stream, pending) = client.open_sideload(&host_str).context("Opening sideload-host service")?;
    // Give the device more time between requests during sideload
    // (some recoveries take >5s before first WRTE)
    stream.set_timeout(std::time::Duration::from_secs(30));

    let pb = ProgressBar::new(total);
    pb.set_style(ProgressStyle::default_bar()
        .template("{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {bytes}/{total_bytes} ({percent}%)")
        .unwrap()
        .progress_chars("=>-"));

    let mut reader = BufReader::new(file);
    let mut send_block = |index: u64, s: &mut AdbStream<'_>, pkt_arg0: u32, pkt_arg1: u32| -> Result<usize> {
        let offset = index * (chunk_size as u64);
        if offset >= total { return Ok(0); }
        let to_send = std::cmp::min(chunk_size as u64, total - offset) as usize;
        let mut buf = vec![0u8; to_send];
        reader.seek(SeekFrom::Start(offset))?;
        reader.read_exact(&mut buf)?;
        // C tool: send WRTE(arg1,arg0) with data, then OKAY(arg1,arg0)
        s.send_wrte_mirror(pkt_arg0, pkt_arg1, buf)?;
        s.send_okay_mirror(pkt_arg0, pkt_arg1)?;
        pb.inc(to_send as u64);
        Ok(to_send)
    };

    // Protocol: device sends OKAY/WRTE cycles. For WRTE, payload is ASCII block index. We mirror OKAYs and for WRTE we send the requested chunk + OKAY.
    let mut finished = false;
    let mut bytes_sent: u64 = 0;
    let mut final_status: Option<String> = None;
    // Handle pending first packet if WRTE arrived during open
    if let Some(pkt) = pending {
        if pkt.cmd == A_WRTE {
            if let Ok(idx) = String::from_utf8_lossy(&pkt.payload).trim().parse::<u64>() {
                let n = send_block(idx, &mut stream, pkt.arg0, pkt.arg1)?;
                if n == 0 { finished = true; }
                bytes_sent = std::cmp::min(total, (idx * chunk_size as u64) + (n as u64));
            }
        } else if pkt.cmd == A_OKAY {
            stream.send_okay_mirror(pkt.arg0, pkt.arg1)?;
        }
    }
    loop {
        let pkt = stream.recv_raw().context("Reading sideload request")?;
        match pkt.cmd {
            x if x == A_OKAY => {
                // Mirror OKAY
                stream.send_okay_mirror(pkt.arg0, pkt.arg1)?;
                continue;
            }
            x if x == A_WRTE => {
                // Determine if this is a block index or a final status string.
                let text = String::from_utf8_lossy(&pkt.payload);
                let trimmed = text.trim();
                if let Ok(idx) = trimmed.parse::<u64>() {
                    let n = send_block(idx, &mut stream, pkt.arg0, pkt.arg1)?;
                    if n == 0 { finished = true; }
                    bytes_sent = std::cmp::min(total, (idx * chunk_size as u64) + (n as u64));
                } else {
                    // Treat as final status message. Ack it, record, and proceed to wait for CLSE.
                    final_status = Some(trimmed.to_string());
                    eprintln!("{}", trimmed);
                    stream.send_okay_mirror(pkt.arg0, pkt.arg1)?;
                    // Do not break yet; wait for device to close the stream.
                }
            }
            x if x == A_CLSE => {
                // Device closed the stream; mirror close and exit loop.
                let _ = stream.close();
                break;
            }
            _ => { /* ignore unknown */ }
        }
        // Do not break immediately on finished; recovery will send a final status and then close.
    }

    pb.finish_and_clear();
    // If device hasn’t closed yet, attempt to explicitly close the sideload stream
    let _ = stream.close();
    std::thread::sleep(std::time::Duration::from_millis(100));
    if bytes_sent < total {
        eprintln!("Warning: sent {} of {} bytes", bytes_sent, total);
    }
    // Evaluate final status message (if any) and treat failures as errors.
    if let Some(status) = final_status.as_deref() {
        let s = status.to_ascii_lowercase();
        // Conservative failure heuristics: common stock recovery texts
        if s.contains("aborted") || s.contains("failed") || s.contains("failure") || s.contains("error") {
            bail!("Sideload reported failure: {}", status);
        }
    }
    Ok(())
}
