use alloc::vec::Vec;

use crate::net::udp;

pub fn resolve_dns(domain: &str) -> Option<[u8; 4]> {
    let local_port = 50000
        + (crate::gui::CLOCK_TICKS.load(core::sync::atomic::Ordering::Relaxed) % 10000) as u16;
    udp::bind(local_port);

    // QEMU Gateway MAC is always required to exit the user networks!
    let qemu_gateway_mac = [0x52, 0x54, 0x00, 0x12, 0x34, 0x56];
    let qemu_dns_ip = [10, 0, 2, 3]; // QEMU user default DNS is at 10.0.2.3

    // Construct DNS Query
    let mut packet = Vec::new();
    let tx_id: u16 = 0x1234;
    packet.extend_from_slice(&tx_id.to_be_bytes()); // ID
    packet.extend_from_slice(&0x0100u16.to_be_bytes()); // Flags: Standard query
    packet.extend_from_slice(&0x0001u16.to_be_bytes()); // QDCOUNT: 1
    packet.extend_from_slice(&0x0000u16.to_be_bytes()); // ANCOUNT: 0
    packet.extend_from_slice(&0x0000u16.to_be_bytes()); // NSCOUNT: 0
    packet.extend_from_slice(&0x0000u16.to_be_bytes()); // ARCOUNT: 0

    // Encode QNAME
    let parts: Vec<&str> = domain.split('.').collect();
    for part in parts {
        packet.push(part.len() as u8);
        packet.extend_from_slice(part.as_bytes());
    }
    packet.push(0); // Null terminator for QNAME

    // QTYPE: A (1)
    packet.extend_from_slice(&0x0001u16.to_be_bytes());
    // QCLASS: IN (1)
    packet.extend_from_slice(&0x0001u16.to_be_bytes());

    udp::send(local_port, qemu_dns_ip, qemu_gateway_mac, 53, &packet);

    // Poll for response
    let start = crate::gui::CLOCK_TICKS.load(core::sync::atomic::Ordering::Relaxed);
    loop {
        if let Some(resp) = udp::recv(local_port) {
            if resp.len() >= 12 {
                // Verify ID and response flags
                let resp_id = u16::from_be_bytes([resp[0], resp[1]]);
                let flags = u16::from_be_bytes([resp[2], resp[3]]);
                if resp_id == tx_id && (flags & 0x8000) != 0 {
                    // Quick traversal to answer section (assuming 1 question, 1+ answer)
                    let ancount = u16::from_be_bytes([resp[6], resp[7]]);
                    if ancount > 0 {
                        // Scan for the end of QNAME
                        let mut offset = 12;
                        while offset < resp.len() && resp[offset] != 0 {
                            offset += 1;
                        }
                        offset += 5; // skip 0 and QTYPE and QCLASS

                        if offset + 12 <= resp.len() {
                            // First answer starts at `offset`
                            // Check if it's a pointer to name (usually 0xC0 something)
                            let mut a_offset = offset;
                            if resp[a_offset] >= 0xC0 {
                                a_offset += 2;
                            } else {
                                while a_offset < resp.len() && resp[a_offset] != 0 {
                                    a_offset += 1;
                                }
                                a_offset += 1;
                            }

                            // A_offset now at TYPE
                            if a_offset + 10 <= resp.len() {
                                let atype =
                                    u16::from_be_bytes([resp[a_offset], resp[a_offset + 1]]);
                                let rdlength =
                                    u16::from_be_bytes([resp[a_offset + 8], resp[a_offset + 9]]);
                                if atype == 1 && rdlength == 4 && a_offset + 10 + 4 <= resp.len() {
                                    let ip_idx = a_offset + 10;
                                    return Some([
                                        resp[ip_idx],
                                        resp[ip_idx + 1],
                                        resp[ip_idx + 2],
                                        resp[ip_idx + 3],
                                    ]);
                                }
                            }
                        }
                    }
                }
            }
            break;
        }

        let now = crate::gui::CLOCK_TICKS.load(core::sync::atomic::Ordering::Relaxed);
        if now - start > 18 * 2 {
            break; // Timeout 2s
        }
        core::hint::spin_loop();
    }

    None
}
