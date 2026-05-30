use core::{
    future::Future,
    pin::Pin,
    task::{Context, Poll, Waker},
};

use spin::Mutex;
use x86_64::instructions::port::Port;

#[derive(Debug, Clone, Copy)]
pub struct MousePacket {
    pub flags: u8,
    pub x_movement: i16,
    pub y_movement: i16,
}

const MOUSE_BUF_SIZE: usize = 32;

pub struct MouseQueue {
    buffer: [MousePacket; MOUSE_BUF_SIZE],
    head: usize,
    tail: usize,
}

impl MouseQueue {
    pub const fn new() -> Self {
        MouseQueue {
            buffer: [MousePacket {
                flags: 0,
                x_movement: 0,
                y_movement: 0,
            }; MOUSE_BUF_SIZE],
            head: 0,
            tail: 0,
        }
    }

    pub fn push(&mut self, pkt: MousePacket) {
        let nt = (self.tail + 1) % MOUSE_BUF_SIZE;
        if nt != self.head {
            self.buffer[self.tail] = pkt;
            self.tail = nt;
        }
    }

    pub fn pop(&mut self) -> Option<MousePacket> {
        if self.head == self.tail {
            None
        } else {
            let pkt = self.buffer[self.head];
            self.head = (self.head + 1) % MOUSE_BUF_SIZE;
            Some(pkt)
        }
    }
}

pub static MOUSE_QUEUE: Mutex<MouseQueue> = Mutex::new(MouseQueue::new());
static WAKER: Mutex<Option<Waker>> = Mutex::new(None);

pub fn init() {
    let mut cmd_port = Port::<u8>::new(0x64);
    let mut data_port = Port::<u8>::new(0x60);

    unsafe {
        // Enable auxiliary mouse device
        cmd_port.write(0xA8);

        // Read configuration byte
        cmd_port.write(0x20);
        let mut status = data_port.read();

        // Enable IRQ12
        status |= 1 << 1;

        // Write configuration byte
        cmd_port.write(0x60);
        data_port.write(status);

        // Tell mouse to use default settings
        cmd_port.write(0xD4);
        data_port.write(0xF6);
        let _ack = data_port.read();

        // Enable data reporting
        cmd_port.write(0xD4);
        data_port.write(0xF4);
        let _ack = data_port.read();
    }
}

static MOUSE_CYCLE: Mutex<u8> = Mutex::new(0);
static MOUSE_PACKET: Mutex<[u8; 3]> = Mutex::new([0; 3]);

pub fn handle_mouse_interrupt() {
    let mut port = Port::new(0x60);
    let packet_byte: u8 = unsafe { port.read() };

    let mut cycle = MOUSE_CYCLE.lock();
    let mut packet = MOUSE_PACKET.lock();

    match *cycle {
        0 => {
            // Check sync bit
            if packet_byte & 0x08 != 0 {
                packet[0] = packet_byte;
                *cycle = 1;
            }
        },
        1 => {
            packet[1] = packet_byte;
            *cycle = 2;
        },
        2 => {
            packet[2] = packet_byte;
            *cycle = 0;

            let flags = packet[0];
            let x_mov = packet[1];
            let y_mov = packet[2];

            let mut x_final = x_mov as i16;
            let mut y_final = y_mov as i16;

            // Sign extension
            if flags & (1 << 4) != 0 {
                x_final |= 0xFF00_u16 as i16;
            }
            if flags & (1 << 5) != 0 {
                y_final |= 0xFF00_u16 as i16;
            }

            let m_pkt = MousePacket {
                flags,
                // The PS/2 mouse y axis is inverted compared to normal screen coordinates
                x_movement: x_final,
                y_movement: -y_final,
            };

            MOUSE_QUEUE.lock().push(m_pkt);
            if let Some(w) = WAKER.lock().take() {
                w.wake();
            }
        },
        _ => *cycle = 0,
    }
}

pub fn get_mouse_packet_sync() -> Option<MousePacket> {
    x86_64::instructions::interrupts::without_interrupts(|| MOUSE_QUEUE.lock().pop())
}

pub struct AsyncMouse;

impl Future for AsyncMouse {
    type Output = MousePacket;
    fn poll(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
        // КРИТИЧЕСКИ ВАЖНО: Выключаем прерывания перед взятием лока!
        x86_64::instructions::interrupts::without_interrupts(|| {
            let mut q = MOUSE_QUEUE.lock();
            if let Some(pkt) = q.pop() {
                Poll::Ready(pkt)
            } else {
                *WAKER.lock() = Some(cx.waker().clone());
                Poll::Pending
            }
        })
    }
}

pub async fn get_mouse_packet() -> MousePacket {
    AsyncMouse.await
}
