// net/arp.rs — Address Resolution Protocol

use alloc::vec::Vec;

use crate::net::{
    ethernet::{send_frame, EthernetFrame, ETHERTYPE_ARP},
    rtl8139,
};

const HW_TYPE_ETHERNET: u16 = 1;
const ARP_OPCODE_REQUEST: u16 = 1;
const ARP_OPCODE_REPLY: u16 = 2;

// Хардкодим IP-адрес нашей ОС (10.0.2.15 стандарт QEMU User Network)
pub const MY_IP: [u8; 4] = [10, 0, 2, 15];

pub fn handle(frame: &EthernetFrame) {
    let data = frame.payload;
    if data.len() < 28 {
        return;
    }

    let hw_type = u16::from_be_bytes([data[0], data[1]]);
    let proto_type = u16::from_be_bytes([data[2], data[3]]);

    if hw_type != HW_TYPE_ETHERNET || proto_type != crate::net::ethernet::ETHERTYPE_IP {
        return;
    }

    let hw_size = data[4];
    let proto_size = data[5];
    if hw_size != 6 || proto_size != 4 {
        return;
    }

    let opcode = u16::from_be_bytes([data[6], data[7]]);

    let mut sender_mac = [0; 6];
    sender_mac.copy_from_slice(&data[8..14]);
    let mut sender_ip = [0; 4];
    sender_ip.copy_from_slice(&data[14..18]);

    let mut target_ip = [0; 4];
    target_ip.copy_from_slice(&data[24..28]);

    // Если нас спрашивают (Request) и наш IP
    if opcode == ARP_OPCODE_REQUEST && target_ip == MY_IP {
        send_reply(&sender_mac, &sender_ip);
    }
    // В полноценном стеке тут еще должна обновляться ARP-таблица,
    // но для пинга из локальной сети нам пока хватит ответа.
}

fn send_reply(target_mac: &[u8; 6], target_ip: &[u8; 4]) {
    let mut packet = Vec::with_capacity(28);

    // HW Type: Ethernet (1)
    packet.extend_from_slice(&HW_TYPE_ETHERNET.to_be_bytes());

    // Proto: IPv4 (0x0800)
    packet.extend_from_slice(&crate::net::ethernet::ETHERTYPE_IP.to_be_bytes());

    // Sizes
    packet.push(6); // MAC sz
    packet.push(4); // IP sz

    // Opcode: Reply (2)
    packet.extend_from_slice(&ARP_OPCODE_REPLY.to_be_bytes());

    // Sender MAC & IP
    packet.extend_from_slice(&rtl8139::get_mac());
    packet.extend_from_slice(&MY_IP);

    // Target MAC & IP
    packet.extend_from_slice(target_mac);
    packet.extend_from_slice(target_ip);

    send_frame(*target_mac, ETHERTYPE_ARP, &packet);
}
