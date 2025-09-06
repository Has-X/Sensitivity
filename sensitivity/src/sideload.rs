use std::fs::File;
use std::io::{BufReader, Read, Seek, SeekFrom};
use std::path::Path;

use anyhow::{bail, Context, Result};
use indicatif::{ProgressBar, ProgressStyle};

use crate::adb::{AdbStream, A_OKAY, A_WRTE};
use crate::mi::MiClient;

pub fn sideload_zip(client: &mut MiClient, path: &Path, chunk_size: usize, validate_token: &str) -> Result<()> {
    let file = File::open(path).with_context(|| format!("Opening {}", path.display()))?;
    let total = file.metadata()?.len();
    if chunk_size == 0 || chunk_size > 1024 * 1024 {
        bail!("Invalid chunk size: {}", chunk_size);
    }

    let host_str = format!("sideload-host:{}:{}:{}:0", total, chunk_size, validate_token);
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
        if pkt.payload.len() > 8 {
            // Likely final status text; print and exit loop
            let s = String::from_utf8_lossy(&pkt.payload);
            eprintln!("{}", s);
            break;
        }
        if pkt.cmd == A_OKAY {
            // Mirror OKAY
            stream.send_okay_mirror(pkt.arg0, pkt.arg1)?;
            continue;
        }
        if pkt.cmd != A_WRTE { continue; }
        let s = String::from_utf8_lossy(&pkt.payload);
        let idx_opt = s.trim().parse::<u64>().ok();
        if let Some(idx) = idx_opt {
            let n = send_block(idx, &mut stream, pkt.arg0, pkt.arg1)?;
            if n == 0 { finished = true; }
            bytes_sent = std::cmp::min(total, (idx * chunk_size as u64) + (n as u64));
        }
        if finished { break; }
    }

    pb.finish_and_clear();
    // Attempt to explicitly close the sideload stream
    let _ = stream.close();
    std::thread::sleep(std::time::Duration::from_millis(100));
    if bytes_sent < total {
        eprintln!("Warning: sent {} of {} bytes", bytes_sent, total);
    }
    Ok(())
}
