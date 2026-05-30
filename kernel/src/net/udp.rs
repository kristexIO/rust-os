use alloc::{collections::BTreeMap, vec::Vec};

use spin::Mutex;

use crate::net::{ipv4, utils::ntohs};

#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
pub struct UdpHeader {
    pub src_port: u16,
    pub dst_port: u16,
    pub length: u16,
    pub checksum: u16,
}

pub struct UdpPacket<'a> {
    pub header: UdpHeader,
    pub payload: &'a [u8],
}

impl<'a> UdpPacket<'a> {
    pub fn parse(data: &'a [u8]) -> Option<Self> {
        if data.len() < 8 {
            return None;
        }
        let header = unsafe { core::ptr::read(data.as_ptr() as *const UdpHeader) };
        let payload = &data[8..];
        Some(UdpPacket {
            header,
            payload,
        })
    }
}

pub struct UdpSocket {
    pub local_port: u16,
    pub remote_ip: [u8; 4],
    pub remote_port: u16,
    pub rx_buf: Vec<u8>,
}

pub struct UdpKey {
    pub local_port: u16,
}

static UDP_SOCKETS: Mutex<BTreeMap<u16, UdpSocket>> = Mutex::new(BTreeMap::new());

pub fn bind(local_port: u16) {
    let sock = UdpSocket {
        local_port,
        remote_ip: [0, 0, 0, 0],
        remote_port: 0,
        rx_buf: Vec::new(),
    };
    UDP_SOCKETS.lock().insert(local_port, sock);
}

pub fn send(local_port: u16, dest_ip: [u8; 4], dest_mac: [u8; 6], dest_port: u16, data: &[u8]) {
    let total_len = 8 + data.len();
    let mut packet = Vec::with_capacity(total_len);

    packet.extend_from_slice(&local_port.to_be_bytes());
    packet.extend_from_slice(&dest_port.to_be_bytes());
    packet.extend_from_slice(&(total_len as u16).to_be_bytes());
    packet.extend_from_slice(&0u16.to_be_bytes()); // Checksum optional for IPv4 UDP

    packet.extend_from_slice(data);

    // Optional: compute pseudo-header checksum. We'll skip for zero checksum
    // fallback in IPv4 UDP.

    crate::net::ipv4::send_packet(dest_mac, dest_ip, ipv4::PROTOCOL_UDP, &packet);
}

pub fn recv(local_port: u16) -> Option<Vec<u8>> {
    let mut sockets = UDP_SOCKETS.lock();
    if let Some(sock) = sockets.get_mut(&local_port) {
        if !sock.rx_buf.is_empty() {
            let data = sock.rx_buf.clone();
            sock.rx_buf.clear();
            return Some(data);
        }
    }
    None
}

pub fn handle(_src_ip: &[u8; 4], _src_mac: &[u8; 6], data: &[u8]) {
    if let Some(pkt) = UdpPacket::parse(data) {
        let dst_port = ntohs(pkt.header.dst_port);
        let mut sockets = UDP_SOCKETS.lock();
        if let Some(sock) = sockets.get_mut(&dst_port) {
            sock.rx_buf.extend_from_slice(pkt.payload);
        }
    }
}
