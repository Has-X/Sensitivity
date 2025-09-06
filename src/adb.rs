use anyhow::{bail, Context, Result};
use byteorder::{ByteOrder, LittleEndian};
use std::cmp;
use std::time::Duration;

use crate::usb::UsbTransport;

const MAX_PAYLOAD: usize = 1 << 20; // 1 MiB cap for safety

const fn adb_cmd(b: [u8; 4]) -> u32 {
    (b[0] as u32)
        | ((b[1] as u32) << 8)
        | ((b[2] as u32) << 16)
        | ((b[3] as u32) << 24)
}

pub const A_CNXN: u32 = adb_cmd(*b"CNXN");
pub const A_OPEN: u32 = adb_cmd(*b"OPEN");
pub const A_OKAY: u32 = adb_cmd(*b"OKAY");
pub const A_CLSE: u32 = adb_cmd(*b"CLSE");
pub const A_WRTE: u32 = adb_cmd(*b"WRTE");

#[derive(Debug, Clone)]
pub struct AdbPacket {
    pub cmd: u32,
    pub arg0: u32,
    pub arg1: u32,
    pub payload: Vec<u8>,
}

impl AdbPacket {
    pub fn new(cmd: u32, arg0: u32, arg1: u32, payload: Vec<u8>) -> Self {
        Self { cmd, arg0, arg1, payload }
    }
}

pub struct AdbConnection {
    usb: UsbTransport,
    // Some recoveries assume local-id is always 1 (matches miasst.c)
    local_id_counter: u32,
}

impl AdbConnection {
    pub fn new(usb: UsbTransport) -> Result<Self> {
        let mut conn = Self { usb, local_id_counter: 1 };
        // Small settle delay after claiming interface to reduce race on Windows
        std::thread::sleep(Duration::from_millis(200));
        conn.handshake()?;
        Ok(conn)
    }

    fn checksum(data: &[u8]) -> u32 {
        data.iter().fold(0u32, |acc, &b| acc.wrapping_add(b as u32))
    }

    fn send_packet(&mut self, pkt: &AdbPacket) -> Result<()> {
        let mut header = [0u8; 24];
        LittleEndian::write_u32(&mut header[0..4], pkt.cmd);
        LittleEndian::write_u32(&mut header[4..8], pkt.arg0);
        LittleEndian::write_u32(&mut header[8..12], pkt.arg1);
        LittleEndian::write_u32(&mut header[12..16], pkt.payload.len() as u32);
        // The original C impl sets checksum to 0; mirror that for device compatibility
        LittleEndian::write_u32(&mut header[16..20], 0);
        LittleEndian::write_u32(&mut header[20..24], pkt.cmd ^ 0xFFFF_FFFF);

        self.usb.write_all(&header)?;
        if !pkt.payload.is_empty() {
            self.usb.write_all(&pkt.payload)?;
        }
        Ok(())
    }

    fn recv_packet(&mut self) -> Result<AdbPacket> {
        let mut header = [0u8; 24];
        self.usb.read_exact(&mut header)?;
        let cmd = LittleEndian::read_u32(&header[0..4]);
        let arg0 = LittleEndian::read_u32(&header[4..8]);
        let arg1 = LittleEndian::read_u32(&header[8..12]);
        let len = LittleEndian::read_u32(&header[12..16]) as usize;
        let _cksum = LittleEndian::read_u32(&header[16..20]);
        let magic = LittleEndian::read_u32(&header[20..24]);
        if magic != (cmd ^ 0xFFFF_FFFF) {
            bail!("ADB bad magic: {:#x} vs {:#x}", magic, cmd ^ 0xFFFF_FFFF);
        }
        if len > MAX_PAYLOAD {
            bail!("ADB payload too large: {} bytes", len);
        }
        let mut payload = vec![0u8; len];
        if len > 0 {
            self.usb.read_exact(&mut payload)?;
            // Xiaomi's Mi Assistant mode sets checksum to 0 and does not verify; skip checksum validation here.
        }
        Ok(AdbPacket { cmd, arg0, arg1, payload })
    }

    fn handshake(&mut self) -> Result<()> {
        // Send CNXN host banner (match the C tool: version 0x01000001, banner "host::\0")
        let banner = b"host::\x00".to_vec();
        let pkt = AdbPacket::new(A_CNXN, 0x0100_0001, 1024 * 1024, banner);
        self.send_packet(&pkt)?;

        // Accept either CNXN or a WRTE with "sideload::" as success, mirroring miasst.c
        for _ in 0..10 {
            let reply = self.recv_packet().context("Waiting for device reply after CONNECT")?;
            match reply.cmd {
                x if x == A_CNXN => {
                    return Ok(());
                }
                x if x == A_WRTE => {
                    let s = String::from_utf8_lossy(&reply.payload);
                    if s.starts_with("sideload::") {
                        // Ack and accept as success; some recoveries present a sideload banner here.
                        // We don't yet know remote-id; the banner WRTE often uses arg0 as some id. Ack with zeros is tolerated.
                        self.send_packet(&AdbPacket::new(A_OKAY, 1, reply.arg0, Vec::new()))?;
                        return Ok(());
                    }
                }
                _ => {}
            }
        }
        bail!("Did not receive expected reply (CNXN/WRTE sideload::) from device after CONNECT");
    }

    pub fn open_service(&mut self, name: &str) -> Result<AdbStream> {
        let local_id = self.alloc_local_id();
        let mut payload = Vec::from(name.as_bytes());
        if !payload.ends_with(&[0]) {
            payload.push(0);
        }
        self.send_packet(&AdbPacket::new(A_OPEN, local_id, 0, payload))?;
        loop {
            let pkt = self.recv_packet()?;
            match pkt.cmd {
                A_OKAY => {
                    let remote_id = pkt.arg0; // remote sends its id in arg0
                    return Ok(AdbStream { conn: self, local_id, remote_id });
                }
                A_CLSE => bail!("Stream closed by device while opening {}", name),
                A_WRTE => {
                    // Some recoveries send an initial WRTE during open; ack it.
                    self.send_packet(&AdbPacket::new(A_OKAY, local_id, pkt.arg0, Vec::new()))?;
                }
                _ => {}
            }
        }
    }

    // Open sideload-host service without consuming the first WRTE request.
    // Returns the stream and an optional pending packet (first WRTE or OKAY already read).
    pub fn open_sideload(&mut self, name: &str) -> Result<(AdbStream, Option<AdbPacket>)> {
        let local_id = self.alloc_local_id();
        let mut payload = Vec::from(name.as_bytes());
        if !payload.ends_with(&[0]) { payload.push(0); }
        self.send_packet(&AdbPacket::new(A_OPEN, local_id, 0, payload))?;

        // We need the device's remote id. It can arrive in OKAY or in WRTE.arg0
        let mut remote_id: Option<u32> = None;
        loop {
            let pkt = self.recv_packet()?;
            match pkt.cmd {
                x if x == A_OKAY => {
                    remote_id = Some(pkt.arg0);
                    // Keep looping for the first WRTE; don't send any ACK here
                }
                x if x == A_WRTE => {
                    let rid = remote_id.unwrap_or(pkt.arg0);
                    let stream = AdbStream { conn: self, local_id, remote_id: rid };
                    return Ok((stream, Some(pkt)));
                }
                x if x == A_CLSE => bail!("Stream closed by device while opening sideload-host"),
                _ => { /* ignore */ }
            }
        }
    }

    // Query a short text response service using C-tool semantics: OPEN -> OKAY -> WRTE -> CLSE.
    // We do not send host OKAY/CLSE during this short exchange to mirror miasst.c exactly.
    pub fn query_text(&mut self, name: &str) -> Result<String> {
        let local_id = 1;
        let mut payload = Vec::from(name.as_bytes());
        if !payload.ends_with(&[0]) {
            payload.push(0);
        }
        self.send_packet(&AdbPacket::new(A_OPEN, local_id, 0, payload))?;

        // First receive: often OKAY, sometimes WRTE
        let first = self.recv_packet()?;
        let mut text: Option<String> = None;
        if first.cmd == A_WRTE {
            text = Some(String::from_utf8_lossy(&first.payload).to_string());
        } else if first.cmd != A_OKAY {
            // Unexpected but continue
        }

        if text.is_none() {
            // Second receive: expect WRTE
            let second = self.recv_packet()?;
            if second.cmd == A_WRTE {
                text = Some(String::from_utf8_lossy(&second.payload).to_string());
            }
        }

        // Third receive: consume CLSE (ignore)
        let _ = self.recv_packet();

        let mut s = text.unwrap_or_default();
        while s.ends_with('\n') || s.ends_with('\r') { s.pop(); }
        Ok(s)
    }

    fn alloc_local_id(&mut self) -> u32 { 1 }

    pub fn set_timeout(&mut self, dur: Duration) {
        self.usb.set_timeout(dur);
    }
}

pub struct AdbStream<'a> {
    conn: &'a mut AdbConnection,
    pub local_id: u32,
    pub remote_id: u32,
}

impl<'a> AdbStream<'a> {
    pub fn set_timeout(&mut self, dur: Duration) {
        self.conn.set_timeout(dur);
    }
    pub fn recv_raw(&mut self) -> Result<AdbPacket> {
        self.conn.recv_packet()
    }

    pub fn send_okay_mirror(&mut self, pkt_arg0: u32, pkt_arg1: u32) -> Result<()> {
        // Mirror OKAY with swapped ids like the C tool
        self.conn
            .send_packet(&AdbPacket::new(A_OKAY, pkt_arg1, pkt_arg0, Vec::new()))
    }

    pub fn send_wrte_mirror(&mut self, pkt_arg0: u32, pkt_arg1: u32, payload: Vec<u8>) -> Result<()> {
        // Mirror WRTE with swapped ids like the C tool
        self.conn
            .send_packet(&AdbPacket::new(A_WRTE, pkt_arg1, pkt_arg0, payload))
    }
    pub fn read_write_or_close(&mut self) -> Result<Option<Vec<u8>>> {
        loop {
            let pkt = self.conn.recv_packet()?;
            match pkt.cmd {
                A_WRTE => {
                    // ack and return this chunk
                    self.conn.send_packet(&AdbPacket::new(A_OKAY, self.local_id, pkt.arg0, Vec::new()))?;
                    return Ok(Some(pkt.payload));
                }
                A_OKAY => {
                    // ignore keepalive/ack
                }
                A_CLSE => return Ok(None),
                _ => {}
            }
        }
    }
    pub fn write(&mut self, data: &[u8]) -> Result<()> {
        let mut off = 0;
        while off < data.len() {
            let chunk = cmp::min(64 * 1024, data.len() - off);
            let payload = data[off..off + chunk].to_vec();
            self.conn.send_packet(&AdbPacket::new(A_WRTE, self.local_id, self.remote_id, payload))?;
            // Expect OKAY
            loop {
                let pkt = self.conn.recv_packet()?;
                match pkt.cmd {
                    A_OKAY => break,
                    A_WRTE => {
                        // Reader first, ack it
                        self.conn.send_packet(&AdbPacket::new(A_OKAY, self.local_id, pkt.arg0, Vec::new()))?;
                    }
                    A_CLSE => bail!("Stream closed by device during write"),
                    _ => {}
                }
            }
            off += chunk;
        }
        Ok(())
    }

    pub fn read_to_end(&mut self) -> Result<Vec<u8>> {
        let mut out = Vec::new();
        loop {
            let pkt = self.conn.recv_packet()?;
            match pkt.cmd {
                A_WRTE => {
                    out.extend_from_slice(&pkt.payload);
                    // Ack
                    self.conn.send_packet(&AdbPacket::new(A_OKAY, self.local_id, pkt.arg0, Vec::new()))?;
                }
                A_OKAY => {
                    // ignore
                }
                A_CLSE => {
                    // Mirror close
                    self.conn.send_packet(&AdbPacket::new(A_CLSE, self.local_id, pkt.arg0, Vec::new()))?;
                    break;
                }
                _ => {}
            }
        }
        Ok(out)
    }

    pub fn close(self) -> Result<()> {
        self.conn
            .send_packet(&AdbPacket::new(A_CLSE, self.local_id, self.remote_id, Vec::new()))
    }
}

pub fn connect(usb: UsbTransport) -> Result<AdbConnection> {
    AdbConnection::new(usb)
}
