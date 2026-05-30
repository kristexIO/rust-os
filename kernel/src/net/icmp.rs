// net/icmp.rs — Internet Control Message Protocol

use alloc::vec::Vec;

use spin::Mutex;

use crate::net::{
    ipv4::{send_packet, Ipv4Packet, PROTOCOL_ICMP},
    utils::process_checksum,
};

const ICMP_TYPE_ECHO_REPLY: u8 = 0;
const ICMP_TYPE_ECHO_REQUEST: u8 = 8;

/// Структура для хранения информации о полученном ping-ответе
#[derive(Clone, Copy)]
pub struct PingReply {
    pub from_ip: [u8; 4],
    pub sequence: u16,
    pub time_ms: u32,
}

const PING_QUEUE_SIZE: usize = 16;

pub struct PingQueue {
    buffer: [Option<PingReply>; PING_QUEUE_SIZE],
    head: usize,
    tail: usize,
}

impl PingQueue {
    pub const fn new() -> Self {
        PingQueue {
            buffer: [None; PING_QUEUE_SIZE],
            head: 0,
            tail: 0,
        }
    }

    pub fn push(&mut self, reply: PingReply) {
        let next_tail = (self.tail + 1) % PING_QUEUE_SIZE;
        if next_tail != self.head {
            self.buffer[self.tail] = Some(reply);
            self.tail = next_tail;
        }
    }

    pub fn pop(&mut self) -> Option<PingReply> {
        if self.head == self.tail {
            None
        } else {
            let reply = self.buffer[self.head];
            self.head = (self.head + 1) % PING_QUEUE_SIZE;
            reply
        }
    }
}

pub static PING_REPLIES: Mutex<PingQueue> = Mutex::new(PingQueue::new());

/// Send an ICMP Echo Request to the specified IP address
pub fn send_echo_request(dest_ip: [u8; 4], sequence: u16, identifier: u16) -> bool {
    // Need MAC address - for now use broadcast
    let broadcast_mac = [0xff, 0xff, 0xff, 0xff, 0xff, 0xff];

    // Build ICMP Echo Request
    let mut icmp_data = Vec::with_capacity(8 + 32); // 8 bytes header + 32 bytes payload

    // Type: Echo Request
    icmp_data.push(ICMP_TYPE_ECHO_REQUEST);
    // Code
    icmp_data.push(0);
    // Checksum (placeholder)
    icmp_data.push(0);
    icmp_data.push(0);
    // Identifier
    icmp_data.push(((identifier >> 8) & 0xFF) as u8);
    icmp_data.push((identifier & 0xFF) as u8);
    // Sequence
    icmp_data.push(((sequence >> 8) & 0xFF) as u8);
    icmp_data.push((sequence & 0xFF) as u8);

    // Payload (32 bytes of data)
    for i in 0..32 {
        icmp_data.push(i as u8);
    }

    // Calculate checksum
    let checksum = process_checksum(&icmp_data);
    icmp_data[2] = ((checksum >> 8) & 0xFF) as u8;
    icmp_data[3] = (checksum & 0xFF) as u8;

    // Send the packet
    send_packet(broadcast_mac, dest_ip, PROTOCOL_ICMP, &icmp_data);
    true
}

pub fn handle(packet: &Ipv4Packet, src_mac: &[u8; 6]) {
    let data = packet.payload;
    if data.len() < 8 {
        return;
    }

    let icmp_type = data[0];
    let _code = data[1];

    if icmp_type == ICMP_TYPE_ECHO_REQUEST {
        // Build Reply
        let mut reply = Vec::with_capacity(data.len());
        reply.push(ICMP_TYPE_ECHO_REPLY);
        reply.push(0); // Code
        reply.push(0); // Checksum high
        reply.push(0); // Checksum low

        reply.extend_from_slice(&data[4..]); // Identifier, Sequence, Payload

        let chk = process_checksum(&reply);
        reply[2] = (chk >> 8) as u8;
        reply[3] = (chk & 0xFF) as u8;

        send_packet(*src_mac, packet.src_ip, PROTOCOL_ICMP, &reply);

        crate::println!(
            "[net] Ping request from {}.{}.{}.{}. Sent reply.\n",
            packet.src_ip[0],
            packet.src_ip[1],
            packet.src_ip[2],
            packet.src_ip[3]
        );
    } else if icmp_type == ICMP_TYPE_ECHO_REPLY {
        // Получен Echo Reply - сохраняем в очередь
        if data.len() >= 8 {
            let _identifier = ((data[4] as u16) << 8) | (data[5] as u16);
            let sequence = ((data[6] as u16) << 8) | (data[7] as u16);
            let reply = PingReply {
                from_ip: packet.src_ip,
                sequence,
                time_ms: 0, // Will be set by caller
            };
            PING_REPLIES.lock().push(reply);
        }
    }
}
