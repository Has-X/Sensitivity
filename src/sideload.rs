// Copyright (C) 2026 HasX
// Licensed under the GNU AGPL v3.0. See LICENSE file for details.
// Website: https://hasx.dev

use std::fs::File;
use std::io::{BufReader, Read, Seek, SeekFrom};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};

use anyhow::{bail, Context, Result};
use indicatif::{ProgressBar, ProgressStyle};

use crate::adb::{AdbStream, A_CLSE, A_OKAY, A_WRTE};
use crate::mi::MiClient;

fn block_window(total: u64, chunk_size: usize, index: u64) -> Option<(u64, usize)> {
    let offset = index.checked_mul(chunk_size as u64)?;
    if offset >= total {
        return None;
    }
    let length = std::cmp::min(chunk_size as u64, total - offset) as usize;
    Some((offset, length))
}

fn sideload_host_service(
    total: u64,
    chunk_size: usize,
    validate_token: &str,
    allow_wipe: bool,
) -> Result<String> {
    if validate_token.is_empty()
        || validate_token.contains(':')
        || !validate_token
            .chars()
            .all(|character| character.is_ascii_graphic())
    {
        bail!("Validation token contains invalid protocol characters");
    }
    // The final field is strictly the wipe flag. It is not a resume offset.
    Ok(format!(
        "sideload-host:{total}:{chunk_size}:{validate_token}:{}",
        u8::from(allow_wipe)
    ))
}

pub fn sideload_zip(
    client: &mut MiClient,
    path: &Path,
    chunk_size: usize,
    validate_token: &str,
    allow_wipe: bool,
    cancel: &AtomicBool,
) -> Result<()> {
    let total = std::fs::metadata(path)
        .with_context(|| format!("Reading {}", path.display()))?
        .len();
    let progress_bar = ProgressBar::new(total);
    progress_bar.set_style(
        ProgressStyle::default_bar()
            .template(
                "{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {bytes}/{total_bytes} ({percent}%)",
            )
            .unwrap()
            .progress_chars("=>-"),
    );
    let result = sideload_zip_with_progress(
        client,
        path,
        chunk_size,
        validate_token,
        allow_wipe,
        cancel,
        |sent, _| progress_bar.set_position(sent),
    );
    if cancel.load(Ordering::Relaxed) {
        progress_bar.abandon_with_message("Cancelled");
    } else {
        progress_bar.finish_and_clear();
    }
    result
}

pub fn sideload_zip_with_progress<F>(
    client: &mut MiClient,
    path: &Path,
    chunk_size: usize,
    validate_token: &str,
    allow_wipe: bool,
    cancel: &AtomicBool,
    mut progress: F,
) -> Result<()>
where
    F: FnMut(u64, u64),
{
    let file = File::open(path).with_context(|| format!("Opening {}", path.display()))?;
    let total = file.metadata()?.len();
    if total == 0 {
        bail!("ROM package is empty: {}", path.display());
    }
    if chunk_size == 0 || chunk_size > 1024 * 1024 {
        bail!("Invalid chunk size: {}", chunk_size);
    }

    // The last field is the wipe flag. Some cross-region updates require data wipe.
    // When server indicates Erase==1, we must send ":1"; otherwise ":0" will make recovery abort.
    let host_str = sideload_host_service(total, chunk_size, validate_token, allow_wipe)?;
    let (mut stream, pending) = client
        .open_sideload(&host_str)
        .context("Opening sideload-host service")?;
    // Give the device more time between requests during sideload
    // (some recoveries take >5s before first WRTE)
    stream.set_timeout(std::time::Duration::from_secs(30));

    progress(0, total);

    let mut reader = BufReader::new(file);
    let mut send_block =
        |index: u64, s: &mut AdbStream<'_>, pkt_arg0: u32, pkt_arg1: u32| -> Result<u64> {
            let Some((offset, to_send)) = block_window(total, chunk_size, index) else {
                // Always acknowledge the device's WRTE, even if there's no more data.
                // Some recoveries request one extra block to signal completion.
                s.send_okay_mirror(pkt_arg0, pkt_arg1)?;
                return Ok(total);
            };
            let mut buf = vec![0u8; to_send];
            reader.seek(SeekFrom::Start(offset))?;
            reader.read_exact(&mut buf)?;
            // C tool: send WRTE(arg1,arg0) with data, then OKAY(arg1,arg0)
            s.send_wrte_mirror(pkt_arg0, pkt_arg1, buf)?;
            s.send_okay_mirror(pkt_arg0, pkt_arg1)?;
            let end = offset + to_send as u64;
            progress(end, total);
            Ok(end)
        };

    // Protocol: device sends OKAY/WRTE cycles. For WRTE, payload is ASCII block index. We mirror OKAYs and for WRTE we send the requested chunk + OKAY.
    let mut bytes_sent: u64 = 0;
    let mut final_status: Option<String> = None;
    // Handle pending first packet if WRTE arrived during open
    if let Some(pkt) = pending {
        if cancel.load(Ordering::Relaxed) {
            let _ = stream.close();
            bail!("Sideload cancelled by user");
        }
        if pkt.cmd == A_WRTE {
            if let Ok(idx) = String::from_utf8_lossy(&pkt.payload).trim().parse::<u64>() {
                let end = send_block(idx, &mut stream, pkt.arg0, pkt.arg1)?;
                bytes_sent = bytes_sent.max(end);
            }
        } else if pkt.cmd == A_OKAY {
            stream.send_okay_mirror(pkt.arg0, pkt.arg1)?;
        }
    }
    loop {
        if cancel.load(Ordering::Relaxed) {
            let _ = stream.close();
            bail!("Sideload cancelled by user");
        }
        // Read next packet; if the device disconnected after sending final status,
        // treat it as end-of-session instead of surfacing a transport error.
        let pkt = match stream.recv_raw() {
            Ok(p) => p,
            Err(e) => {
                if final_status.is_some() {
                    break;
                } else {
                    return Err(e).context("Reading sideload request");
                }
            }
        };
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
                    let end = send_block(idx, &mut stream, pkt.arg0, pkt.arg1)?;
                    bytes_sent = bytes_sent.max(end);
                } else {
                    // Treat as final status message. Ack it, record, and proactively end the session.
                    final_status = Some(trimmed.to_string());
                    eprintln!("{}", trimmed);
                    // Acknowledge the device's status WRTE
                    stream.send_okay_mirror(pkt.arg0, pkt.arg1)?;
                    // Break out and close from host side to avoid waiting on a CLSE that may never arrive.
                    break;
                }
            }
            x if x == A_CLSE => {
                // Device closed the stream; exit loop and mirror close after loop.
                break;
            }
            _ => { /* ignore unknown */ }
        }
        // Do not break immediately on finished; recovery will send a final status and then close.
    }

    // If device hasn’t closed yet, attempt to explicitly close the sideload stream
    let _ = stream.close();
    std::thread::sleep(std::time::Duration::from_millis(100));
    if bytes_sent < total {
        bail!("Sideload ended after {bytes_sent} of {total} bytes");
    }
    // Evaluate final status message (if any) and treat failures as errors.
    if let Some(status) = final_status.as_deref() {
        let s = status.to_ascii_lowercase();
        // Conservative failure heuristics: common stock recovery texts
        if s.contains("aborted")
            || s.contains("failed")
            || s.contains("failure")
            || s.contains("error")
        {
            bail!("Sideload reported failure: {}", status);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn block_window_handles_full_and_partial_blocks() {
        assert_eq!(block_window(10, 4, 0), Some((0, 4)));
        assert_eq!(block_window(10, 4, 1), Some((4, 4)));
        assert_eq!(block_window(10, 4, 2), Some((8, 2)));
        assert_eq!(block_window(10, 4, 3), None);
    }

    #[test]
    fn absurd_block_index_cannot_overflow_offset() {
        assert_eq!(block_window(10, 64 * 1024, u64::MAX), None);
    }

    #[test]
    fn final_host_field_is_only_the_wipe_flag() {
        assert_eq!(
            sideload_host_service(123, 65_536, "token+/=", false).unwrap(),
            "sideload-host:123:65536:token+/=:0"
        );
        assert_eq!(
            sideload_host_service(123, 65_536, "token+/=", true).unwrap(),
            "sideload-host:123:65536:token+/=:1"
        );
    }

    #[test]
    fn protocol_delimiters_are_rejected_in_tokens() {
        assert!(sideload_host_service(123, 65_536, "bad:token", false).is_err());
        assert!(sideload_host_service(123, 65_536, "", false).is_err());
    }
}
