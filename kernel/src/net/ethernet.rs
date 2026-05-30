// net/ethernet.rs — Разбор Ethernet фреймов

use alloc::vec::Vec;

use crate::net::rtl8139;

pub const ETHERTYPE_ARP: u16 = 0x0806;
pub const ETHERTYPE_IP: u16 = 0x0800;

#[derive(Debug)]
pub struct EthernetFrame<'a> {
    pub dest_mac: [u8; 6],
    pub src_mac: [u8; 6],
    pub eth_type: u16,
    pub payload: &'a [u8],
}

impl<'a> EthernetFrame<'a> {
    pub fn parse(packet: &'a [u8]) -> Option<Self> {
        if packet.len() < 14 {
            return None;
        }

        let mut dest = [0; 6];
        dest.copy_from_slice(&packet[0..6]);
        let mut src = [0; 6];
        src.copy_from_slice(&packet[6..12]);

        let eth_type = u16::from_be_bytes([packet[12], packet[13]]);

        Some(EthernetFrame {
            dest_mac: dest,
            src_mac: src,
            eth_type,
            payload: &packet[14..],
        })
    }
}

pub fn handle_packet(packet: &[u8]) {
    if let Some(frame) = EthernetFrame::parse(packet) {
        match frame.eth_type {
            ETHERTYPE_ARP => crate::net::arp::handle(&frame),
            ETHERTYPE_IP => crate::net::ipv4::handle(&frame),
            _ => {}, // Игнорируем остальные протоколы (IPv6, STP, etc.)
        }
    }
}

pub fn send_frame(dest_mac: [u8; 6], eth_type: u16, payload: &[u8]) {
    let mut frame = Vec::with_capacity(14 + payload.len());
    frame.extend_from_slice(&dest_mac);
    frame.extend_from_slice(&rtl8139::get_mac());

    frame.extend_from_slice(&eth_type.to_be_bytes());

    frame.extend_from_slice(payload);

    // Минимальный размер фрейма Ethernet (без CRC) — 60 байт (46 байт payload)
    while frame.len() < 60 {
        frame.push(0);
    }

    rtl8139::send_packet(&frame);
}
