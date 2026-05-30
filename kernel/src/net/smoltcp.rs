use alloc::sync::Arc;
use spin::Mutex;
use crate::driver::NetworkInterface;

pub struct SmoltcpAdapter;

impl SmoltcpAdapter {
    pub fn poll(&mut self) {}
    pub fn iface(&mut self) -> &mut FakeIface {
        unsafe { &mut *(0 as *mut FakeIface) }
    }
}

pub struct FakeIface;
impl FakeIface {
    pub fn ip_addrs(&self) -> &[match_me::Addr] { &[] }
}

mod match_me {
    pub struct Addr;
    impl core::fmt::Display for Addr {
        fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
            write!(f, "0.0.0.0")
        }
    }
}

pub fn init_adapter(_driver: Arc<Mutex<dyn NetworkInterface>>) {}
pub fn poll_global_adapter() {}
pub fn get_adapter_mut() -> Option<&'static mut SmoltcpAdapter> { None }
