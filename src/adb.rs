// Copyright (C) 2025 HasX
// Licensed under the GNU AGPL v3.0. See LICENSE file for details.
// Website: https://hasx.dev

use anyhow::{bail, Context, Result};
use byteorder::{ByteOrder, LittleEndian};
use std::time::Duration;

use crate::usb::UsbTransport;

const MAX_PAYLOAD: usize = 1 << 20; // 1 MiB cap for safety
const HEADER_SIZE: usize = 24;

const fn adb_cmd(b: [u8; 4]) -> u32 {
    (b[0] as u32) | ((b[1] as u32) << 8) | ((b[2] as u32) << 16) | ((b[3] as u32) << 24)
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
        Self {
            cmd,
            arg0,
            arg1,
            payload,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AdbHeader {
    pub cmd: u32,
    pub arg0: u32,
    pub arg1: u32,
    pub payload_len: usize,
    pub checksum: u32,
}

pub fn encode_header(packet: &AdbPacket) -> [u8; HEADER_SIZE] {
    let mut header = [0u8; HEADER_SIZE];
    LittleEndian::write_u32(&mut header[0..4], packet.cmd);
    LittleEndian::write_u32(&mut header[4..8], packet.arg0);
    LittleEndian::write_u32(&mut header[8..12], packet.arg1);
    LittleEndian::write_u32(&mut header[12..16], packet.payload.len() as u32);
    // Xiaomi Mi Assistant recovery follows the original client and uses zero.
    LittleEndian::write_u32(&mut header[16..20], 0);
    LittleEndian::write_u32(&mut header[20..24], packet.cmd ^ 0xFFFF_FFFF);
    header
}

pub fn decode_header(bytes: &[u8; HEADER_SIZE]) -> Result<AdbHeader> {
    let cmd = LittleEndian::read_u32(&bytes[0..4]);
    let arg0 = LittleEndian::read_u32(&bytes[4..8]);
    let arg1 = LittleEndian::read_u32(&bytes[8..12]);
    let payload_len = LittleEndian::read_u32(&bytes[12..16]) as usize;
    let checksum = LittleEndian::read_u32(&bytes[16..20]);
    let magic = LittleEndian::read_u32(&bytes[20..24]);

    if magic != (cmd ^ 0xFFFF_FFFF) {
        bail!("ADB header magic mismatch for command {cmd:#x}");
    }
    if payload_len > MAX_PAYLOAD {
        bail!("ADB payload too large: {payload_len} bytes");
    }

    Ok(AdbHeader {
        cmd,
        arg0,
        arg1,
        payload_len,
        checksum,
    })
}

pub struct AdbConnection {
    usb: UsbTransport,
}

impl AdbConnection {
    pub fn new(usb: UsbTransport) -> Result<Self> {
        let mut conn = Self { usb };
        // Small settle delay after claiming interface to reduce race on Windows
        std::thread::sleep(Duration::from_millis(200));
        conn.handshake()?;
        Ok(conn)
    }

    fn send_packet(&mut self, pkt: &AdbPacket) -> Result<()> {
        let header = encode_header(pkt);
        self.usb.write_all(&header)?;
        if !pkt.payload.is_empty() {
            self.usb.write_all(&pkt.payload)?;
        }
        Ok(())
    }

    fn recv_packet(&mut self) -> Result<AdbPacket> {
        let mut bytes = [0u8; HEADER_SIZE];
        self.usb.read_exact(&mut bytes)?;
        let header = decode_header(&bytes)?;
        let mut payload = vec![0u8; header.payload_len];
        if header.payload_len > 0 {
            self.usb.read_exact(&mut payload)?;
            // Xiaomi's Mi Assistant mode sets checksum to 0 and does not verify; skip checksum validation here.
        }
        Ok(AdbPacket {
            cmd: header.cmd,
            arg0: header.arg0,
            arg1: header.arg1,
            payload,
        })
    }

    fn handshake(&mut self) -> Result<()> {
        // Send CNXN host banner (match the C tool: version 0x01000001, banner "host::\0")
        let banner = b"host::\x00".to_vec();
        let pkt = AdbPacket::new(A_CNXN, 0x0100_0001, 1024 * 1024, banner);
        self.send_packet(&pkt)?;

        // Accept either CNXN or a WRTE with "sideload::" as success, mirroring miasst.c
        for _ in 0..10 {
            let reply = self
                .recv_packet()
                .context("Waiting for device reply after CONNECT")?;
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

    pub fn open_service(&mut self, name: &str) -> Result<AdbStream<'_>> {
        let local_id = self.alloc_local_id();
        let mut payload = Vec::from(name.as_bytes());
        if !payload.ends_with(&[0]) {
            payload.push(0);
        }
        self.send_packet(&AdbPacket::new(A_OPEN, local_id, 0, payload))?;
        for _ in 0..32 {
            let pkt = self.recv_packet()?;
            match pkt.cmd {
                A_OKAY => {
                    let remote_id = pkt.arg0; // remote sends its id in arg0
                    return Ok(AdbStream {
                        conn: self,
                        local_id,
                        remote_id,
                    });
                }
                A_CLSE => bail!("Stream closed by device while opening {}", name),
                A_WRTE => {
                    // Some recoveries send an initial WRTE during open; ack it.
                    self.send_packet(&AdbPacket::new(A_OKAY, local_id, pkt.arg0, Vec::new()))?;
                }
                _ => {}
            }
        }
        bail!("Device did not open service {name} after 32 packets")
    }

    // Open sideload-host service without consuming the first WRTE request.
    // Returns the stream and an optional pending packet (first WRTE or OKAY already read).
    pub fn open_sideload(&mut self, name: &str) -> Result<(AdbStream<'_>, Option<AdbPacket>)> {
        let local_id = self.alloc_local_id();
        let mut payload = Vec::from(name.as_bytes());
        if !payload.ends_with(&[0]) {
            payload.push(0);
        }
        self.send_packet(&AdbPacket::new(A_OPEN, local_id, 0, payload))?;

        // We need the device's remote id. It can arrive in OKAY or in WRTE.arg0
        let mut remote_id: Option<u32> = None;
        for _ in 0..32 {
            let pkt = self.recv_packet()?;
            match pkt.cmd {
                x if x == A_OKAY => {
                    remote_id = Some(pkt.arg0);
                    // Keep looping for the first WRTE; don't send any ACK here
                }
                x if x == A_WRTE => {
                    let rid = remote_id.unwrap_or(pkt.arg0);
                    let stream = AdbStream {
                        conn: self,
                        local_id,
                        remote_id: rid,
                    };
                    return Ok((stream, Some(pkt)));
                }
                x if x == A_CLSE => bail!("Stream closed by device while opening sideload-host"),
                _ => { /* ignore */ }
            }
        }
        bail!("Device did not start sideload after 32 packets")
    }

    // Query a short text response service using C-tool semantics: OPEN -> OKAY -> WRTE -> CLSE.
    // We do not send host OKAY/CLSE during this short exchange to mirror miasst.c exactly.
    pub fn query_text(&mut self, name: &str) -> Result<String> {
        let local_id = self.alloc_local_id();
        let mut payload = Vec::from(name.as_bytes());
        if !payload.ends_with(&[0]) {
            payload.push(0);
        }
        self.send_packet(&AdbPacket::new(A_OPEN, local_id, 0, payload))?;

        let mut response = Vec::new();
        for _ in 0..16 {
            match self.recv_packet() {
                Ok(packet) if packet.cmd == A_WRTE => {
                    response.extend_from_slice(&packet.payload);
                }
                Ok(packet) if packet.cmd == A_CLSE => break,
                Ok(_) => {}
                // Some stock recoveries return the text but omit the final CLSE.
                Err(_) if !response.is_empty() => break,
                Err(error) => return Err(error).with_context(|| format!("Reading {name}")),
            }
        }
        if response.is_empty() {
            bail!("Device returned an empty response for {name}");
        }
        let mut s = String::from_utf8_lossy(&response).into_owned();
        while s.ends_with('\n') || s.ends_with('\r') {
            s.pop();
        }
        Ok(s)
    }

    fn alloc_local_id(&mut self) -> u32 {
        // Preserve the proven stock-recovery behavior used by Xiaomi's tool.
        1
    }

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

    pub fn send_wrte_mirror(
        &mut self,
        pkt_arg0: u32,
        pkt_arg1: u32,
        payload: Vec<u8>,
    ) -> Result<()> {
        // Mirror WRTE with swapped ids like the C tool
        self.conn
            .send_packet(&AdbPacket::new(A_WRTE, pkt_arg1, pkt_arg0, payload))
    }
    pub fn read_to_end(&mut self) -> Result<Vec<u8>> {
        let mut out = Vec::new();
        loop {
            let pkt = self.conn.recv_packet()?;
            match pkt.cmd {
                A_WRTE => {
                    out.extend_from_slice(&pkt.payload);
                    // Ack
                    self.conn.send_packet(&AdbPacket::new(
                        A_OKAY,
                        self.local_id,
                        pkt.arg0,
                        Vec::new(),
                    ))?;
                }
                A_OKAY => {
                    // ignore
                }
                A_CLSE => {
                    // Mirror close
                    self.conn.send_packet(&AdbPacket::new(
                        A_CLSE,
                        self.local_id,
                        pkt.arg0,
                        Vec::new(),
                    ))?;
                    break;
                }
                _ => {}
            }
        }
        Ok(out)
    }

    pub fn close(self) -> Result<()> {
        self.conn.send_packet(&AdbPacket::new(
            A_CLSE,
            self.local_id,
            self.remote_id,
            Vec::new(),
        ))
    }
}

pub fn connect(usb: UsbTransport) -> Result<AdbConnection> {
    AdbConnection::new(usb)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn header_round_trip_preserves_protocol_fields() {
        let packet = AdbPacket::new(A_OPEN, 12, 34, b"getdevice:\0".to_vec());
        let header = decode_header(&encode_header(&packet)).unwrap();

        assert_eq!(header.cmd, A_OPEN);
        assert_eq!(header.arg0, 12);
        assert_eq!(header.arg1, 34);
        assert_eq!(header.payload_len, packet.payload.len());
        assert_eq!(header.checksum, 0);
    }

    #[test]
    fn header_is_little_endian_and_uses_adb_magic() {
        let packet = AdbPacket::new(A_CNXN, 0x0100_0001, 1024 * 1024, Vec::new());
        let bytes = encode_header(&packet);

        assert_eq!(&bytes[0..4], b"CNXN");
        assert_eq!(LittleEndian::read_u32(&bytes[20..24]), A_CNXN ^ 0xFFFF_FFFF);
    }

    #[test]
    fn malformed_magic_is_rejected_before_payload_read() {
        let packet = AdbPacket::new(A_OKAY, 1, 2, Vec::new());
        let mut bytes = encode_header(&packet);
        bytes[20] ^= 0xff;

        assert!(decode_header(&bytes)
            .unwrap_err()
            .to_string()
            .contains("magic mismatch"));
    }

    #[test]
    fn oversized_payload_is_rejected_before_allocation() {
        let packet = AdbPacket::new(A_WRTE, 1, 2, Vec::new());
        let mut bytes = encode_header(&packet);
        LittleEndian::write_u32(&mut bytes[12..16], (MAX_PAYLOAD + 1) as u32);

        assert!(decode_header(&bytes)
            .unwrap_err()
            .to_string()
            .contains("payload too large"));
    }
}
