// net/ipv4.rs — Internet Protocol version 4

use alloc::vec::Vec;

use crate::net::{
    ethernet::{send_frame, EthernetFrame, ETHERTYPE_IP},
    utils::process_checksum,
};

pub const PROTOCOL_ICMP: u8 = 1;
pub const PROTOCOL_TCP: u8 = 6;
pub const PROTOCOL_UDP: u8 = 17;

#[derive(Debug)]
pub struct Ipv4Packet<'a> {
    pub src_ip: [u8; 4],
    pub dest_ip: [u8; 4],
    pub protocol: u8,
    pub payload: &'a [u8],
}

pub fn handle(frame: &EthernetFrame) {
    let data = frame.payload;
    if data.len() < 20 {
        return;
    }

    let version_ihl = data[0];
    let version = version_ihl >> 4;
    let ihl = version_ihl & 0x0F;

    if version != 4 {
        return;
    }

    let header_len = (ihl as usize) * 4;
    if data.len() < header_len {
        return;
    }

    let protocol = data[9];

    let mut src_ip = [0; 4];
    src_ip.copy_from_slice(&data[12..16]);
    let mut dest_ip = [0; 4];
    dest_ip.copy_from_slice(&data[16..20]);

    let packet = Ipv4Packet {
        src_ip,
        dest_ip,
        protocol,
        payload: &data[header_len..],
    };

    if packet.dest_ip == crate::net::arp::MY_IP || packet.dest_ip == [255, 255, 255, 255] {
        match packet.protocol {
            PROTOCOL_ICMP => crate::net::icmp::handle(&packet, &frame.src_mac),
            PROTOCOL_TCP => {
                crate::net::tcp::handle(&packet.src_ip, &frame.src_mac, packet.payload);
            },
            PROTOCOL_UDP => {
                crate::net::udp::handle(&packet.src_ip, &frame.src_mac, packet.payload);
            },
            _ => {},
        }
    }
}

pub fn send_packet(dest_mac: [u8; 6], dest_ip: [u8; 4], protocol: u8, payload: &[u8]) {
    let mut packet = Vec::with_capacity(20 + payload.len());

    // В IPv4 header:
    // Version = 4, IHL = 5 (20 bytes) -> 0x45
    packet.push(0x45);
    // DSCP / ECN
    packet.push(0x00);
    // Total Length
    let len = 20 + payload.len() as u16;
    packet.extend_from_slice(&len.to_be_bytes());
    // Identification
    packet.push(0x00);
    packet.push(0x00);
    // Flags / Fragment Offset
    packet.push(0x40);
    packet.push(0x00); // DF (Don't Fragment)
                       // TTL
    packet.push(64);
    // Protocol
    packet.push(protocol);
    // Checksum (initially 0)
    packet.push(0x00);
    packet.push(0x00);
    // Source IP
    packet.extend_from_slice(&crate::net::arp::MY_IP);
    // Dest IP
    packet.extend_from_slice(&dest_ip);

    // Calculate Checksum over header
    let chk = process_checksum(&packet[0..20]);
    packet[10] = (chk >> 8) as u8;
    packet[11] = (chk & 0xFF) as u8;

    packet.extend_from_slice(payload);

    send_frame(dest_mac, ETHERTYPE_IP, &packet);
}
