// net/mod.rs — Сетевой стек (Ethernet, IPv4, ICMP, ARP, TCP, UDP)

pub mod arp;
pub mod dns;
pub mod ethernet;
pub mod http;
pub mod icmp;
pub mod ipv4;
pub mod rtl8139;
pub mod tcp;
pub mod udp;
pub mod utils;

pub fn init() {
    rtl8139::init();
}
