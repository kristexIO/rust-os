/// TCP стек: конечный автомат (FSM) по RFC 793.
///
/// Состояния:
///   Closed → Listen → SynReceived → Established → CloseWait → LastAck → Closed
///   Closed → SynSent → Established → FinWait1 → FinWait2 → TimeWait → Closed
///
/// Активные соединения хранятся в глобальной таблице `TCP_CONNECTIONS`.
/// Каждое соединение идентифицируется `TcpKey` = (local_port, remote_ip,
/// remote_port).
///
/// Receive buffer: кольцевой буфер на `Vec<u8>` (фиксированный размер
/// RX_BUF_SIZE). Send buffer: `Vec<u8>` (простой, без sliding window пока).
///
/// Поддержка:
///   - Passive open (server listen/accept)
///   - Active open (client connect)
///   - Data transfer (send/recv)
///   - FIN/RST teardown
///   - Checksum verification/generation
///   - Retransmit timeout (простой: счётчик тиков)
use alloc::{collections::BTreeMap, vec::Vec};

use spin::Mutex;

use crate::net::utils::{htons, ntohs, process_checksum};

#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
pub struct TcpHeader {
    pub src_port: u16,
    pub dst_port: u16,
    pub seq_num: u32,
    pub ack_num: u32,
    pub data_offset_and_flags: u16, // 4 бита offset + 6 бит reserved + 6 бит flags
    pub window_size: u16,
    pub checksum: u16,
    pub urgent_ptr: u16,
}

impl TcpHeader {
    pub fn new() -> Self {
        TcpHeader {
            src_port: 0,
            dst_port: 0,
            seq_num: 0,
            ack_num: 0,
            data_offset_and_flags: 0,
            window_size: 0,
            checksum: 0,
            urgent_ptr: 0,
        }
    }

    pub fn source_port(&self) -> u16 {
        ntohs(self.src_port)
    }

    pub fn destination_port(&self) -> u16 {
        ntohs(self.dst_port)
    }

    pub fn set_source_port(&mut self, port: u16) {
        self.src_port = htons(port);
    }

    pub fn set_destination_port(&mut self, port: u16) {
        self.dst_port = htons(port);
    }

    pub fn sequence_number(&self) -> u32 {
        u32::from_be(self.seq_num)
    }

    pub fn acknowledgment_number(&self) -> u32 {
        u32::from_be(self.ack_num)
    }

    pub fn flags(&self) -> u8 {
        (u16::from_be(self.data_offset_and_flags) & 0x3F) as u8
    }

    pub fn set_flags(&mut self, flags: u8) {
        let bits = u16::from_be(self.data_offset_and_flags);
        self.data_offset_and_flags = ((bits & !0x3F) | flags as u16).to_be();
    }

    pub fn data_offset(&self) -> u8 {
        (u16::from_be(self.data_offset_and_flags) >> 12) as u8
    }

    pub fn checksum(&self) -> u16 {
        ntohs(self.checksum)
    }
}

// TCP Flags
pub const TCP_FLAG_FIN: u8 = 0x01;
pub const TCP_FLAG_SYN: u8 = 0x02;
pub const TCP_FLAG_RST: u8 = 0x04;
pub const TCP_FLAG_PSH: u8 = 0x08;
pub const TCP_FLAG_ACK: u8 = 0x10;
pub const TCP_FLAG_URG: u8 = 0x20;

#[derive(Debug)]
pub struct TcpPacket<'a> {
    pub header: TcpHeader,
    pub payload: &'a [u8],
}

impl<'a> TcpPacket<'a> {
    pub fn parse(data: &'a [u8]) -> Option<Self> {
        if data.len() < core::mem::size_of::<TcpHeader>() {
            return None;
        }

        let header_bytes = &data[0..core::mem::size_of::<TcpHeader>()];
        let header = unsafe { core::ptr::read(header_bytes.as_ptr() as *const TcpHeader) };

        let data_offset = header.data_offset();
        let header_size = (data_offset * 4) as usize;

        if data.len() < header_size {
            return None;
        }

        let payload = &data[header_size..];

        Some(TcpPacket {
            header,
            payload,
        })
    }

    pub fn calculate_checksum(&self, src_ip: &[u8; 4], dst_ip: &[u8; 4]) -> u16 {
        // Создаем pseudo-header для вычисления контрольной суммы TCP
        let mut pseudo_header = Vec::new();
        pseudo_header.extend_from_slice(src_ip);
        pseudo_header.extend_from_slice(dst_ip);
        pseudo_header.push(0); // reserved
        pseudo_header.push(6); // protocol = TCP
        let tcp_length = (core::mem::size_of::<TcpHeader>() + self.payload.len()) as u16;
        pseudo_header.extend_from_slice(&tcp_length.to_be_bytes());

        // Добавляем TCP заголовок и данные
        let tcp_without_checksum = unsafe {
            core::slice::from_raw_parts(
                &self.header as *const TcpHeader as *const u8,
                core::mem::size_of::<TcpHeader>(),
            )
        }
        .to_vec();

        let mut tcp_bytes = Vec::new();
        tcp_bytes.extend_from_slice(&tcp_without_checksum);
        tcp_bytes.splice(16..18, 0u16.to_be_bytes()); // Обнуляем поле checksum
        tcp_bytes.extend_from_slice(self.payload);

        // Вычисляем контрольную сумму
        let mut all_data = Vec::new();
        all_data.extend_from_slice(&pseudo_header);
        all_data.extend_from_slice(&tcp_bytes);

        process_checksum(&all_data)
    }

    pub fn verify_checksum(&self, src_ip: &[u8; 4], dst_ip: &[u8; 4]) -> bool {
        let header = unsafe {
            core::slice::from_raw_parts(
                &self.header as *const TcpHeader as *const u8,
                core::mem::size_of::<TcpHeader>(),
            )
        };
        let mut bytes = Vec::with_capacity(header.len() + self.payload.len());
        bytes.extend_from_slice(header);
        bytes.extend_from_slice(self.payload);
        tcp_checksum(src_ip, dst_ip, &bytes) == 0
    }
}

/// Создает TCP пакет
pub fn create_tcp_packet(
    src_port: u16,
    dst_port: u16,
    seq_num: u32,
    ack_num: u32,
    flags: u8,
    payload: &[u8],
) -> Vec<u8> {
    let mut packet = Vec::with_capacity(core::mem::size_of::<TcpHeader>() + payload.len());

    let mut header = TcpHeader::new();
    header.set_source_port(src_port);
    header.set_destination_port(dst_port);
    header.seq_num = seq_num.to_be();
    header.ack_num = ack_num.to_be();
    header.data_offset_and_flags = ((5u16 << 12) | flags as u16).to_be();
    header.window_size = htons(8192); // Стандартный размер окна

    // Заполняем пакет данными
    packet.extend_from_slice(unsafe {
        core::slice::from_raw_parts(
            &header as *const TcpHeader as *const u8,
            core::mem::size_of::<TcpHeader>(),
        )
    });
    packet.extend_from_slice(payload);

    // После добавления всех данных вычисляем контрольную сумму
    // (в реальности нужно использовать правильный алгоритм расчета)

    packet
}

// ─── TCP State Machine
// ────────────────────────────────────────────────────────

/// Состояния TCP соединения (RFC 793, рис. 6)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TcpState {
    /// Нет соединения
    Closed,
    /// Сервер ждёт входящих SYN
    Listen,
    /// Клиент отправил SYN, ждёт SYN-ACK
    SynSent,
    /// Сервер получил SYN, отправил SYN-ACK, ждёт ACK
    SynReceived,
    /// Соединение установлено — данные можно передавать
    Established,
    /// Мы получили FIN от партнёра, ждём закрытия нашей стороны
    CloseWait,
    /// Мы отправили FIN, ждём ACK
    FinWait1,
    /// Наш FIN подтверждён, ждём FIN партнёра
    FinWait2,
    /// Мы отправили наш FIN в ответ на FIN партнёра
    LastAck,
    /// Ожидание 2*MSL перед полным закрытием
    TimeWait,
}

// ─── Размеры буферов
// ──────────────────────────────────────────────────────────

const RX_BUF_SIZE: usize = 8192;
const TX_BUF_SIZE: usize = 8192;
const MSS: u16 = 1460; // Maximum Segment Size
const WINDOW_SIZE: u16 = 8192; // Advertised window size
const RETRANSMIT_TIMEOUT: u64 = 500; // тиков (≈ 500ms)

// ─── Идентификатор соединения
// ─────────────────────────────────────────────────

/// 4-tuple идентификатор TCP соединения
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct TcpKey {
    pub local_port: u16,
    pub remote_ip: [u8; 4],
    pub remote_port: u16,
}

// ─── Соединение
// ───────────────────────────────────────────────────────────────

/// Одно TCP соединение
pub struct TcpConnection {
    pub state: TcpState,
    pub local_port: u16,
    pub remote_ip: [u8; 4],
    pub remote_mac: [u8; 6],
    pub remote_port: u16,

    // Sequence numbers
    pub snd_una: u32, // Send unacknowledged
    pub snd_nxt: u32, // Send next
    pub snd_wnd: u16, // Send window (партнёра)
    pub rcv_nxt: u32, // Receive next (ожидаемый seq)
    pub rcv_wnd: u16, // Receive window (наш)
    pub iss: u32,     // Initial send sequence number

    // Буферы
    pub rx_buf: Vec<u8>,
    pub tx_buf: Vec<u8>,

    // Retransmit
    pub retransmit_tick: u64,
    pub retransmit_count: u8,

    // TimeWait: тик, когда можно закрыть
    pub timewait_deadline: u64,
}

impl TcpConnection {
    pub fn new(local_port: u16, remote_ip: [u8; 4], remote_mac: [u8; 6], remote_port: u16) -> Self {
        // ISN на основе TSC (должен быть уникальным — RFC 793)
        let isn = unsafe {
            let mut lo: u32;
            let mut hi: u32;
            core::arch::asm!("rdtsc", out("eax") lo, out("edx") hi, options(nomem, nostack));
            lo ^ hi.wrapping_mul(1337)
        };

        TcpConnection {
            state: TcpState::Closed,
            local_port,
            remote_ip,
            remote_mac,
            remote_port,
            snd_una: isn,
            snd_nxt: isn,
            snd_wnd: WINDOW_SIZE,
            rcv_nxt: 0,
            rcv_wnd: WINDOW_SIZE,
            iss: isn,
            rx_buf: Vec::new(),
            tx_buf: Vec::new(),
            retransmit_tick: 0,
            retransmit_count: 0,
            timewait_deadline: 0,
        }
    }

    // ─── Генерация пакетов ─────────────────────────────────────────────────

    fn build_segment(&self, flags: u8, payload: &[u8]) -> Vec<u8> {
        let mut tcp_bytes = Vec::with_capacity(20 + payload.len());

        // TCP header fields
        tcp_bytes.extend_from_slice(&self.local_port.to_be_bytes());
        tcp_bytes.extend_from_slice(&self.remote_port.to_be_bytes());
        tcp_bytes.extend_from_slice(&self.snd_nxt.to_be_bytes());
        tcp_bytes.extend_from_slice(&self.rcv_nxt.to_be_bytes());

        // Data offset (5 * 4 = 20 bytes) + flags
        let data_offset_flags: u16 = (5u16 << 12) | (flags as u16);
        tcp_bytes.extend_from_slice(&data_offset_flags.to_be_bytes());

        tcp_bytes.extend_from_slice(&self.rcv_wnd.to_be_bytes());
        tcp_bytes.extend_from_slice(&0u16.to_be_bytes()); // checksum placeholder
        tcp_bytes.extend_from_slice(&0u16.to_be_bytes()); // urgent pointer

        tcp_bytes.extend_from_slice(payload);

        // Вычисляем checksum через pseudo-header
        let chk = tcp_checksum(&crate::net::arp::MY_IP, &self.remote_ip, &tcp_bytes);
        tcp_bytes[16] = (chk >> 8) as u8;
        tcp_bytes[17] = (chk & 0xFF) as u8;

        tcp_bytes
    }

    /// Отправляет TCP сегмент через IPv4
    fn send_segment(&self, flags: u8, payload: &[u8]) {
        let tcp_bytes = self.build_segment(flags, payload);
        crate::net::ipv4::send_packet(
            self.remote_mac,
            self.remote_ip,
            6, // TCP protocol
            &tcp_bytes,
        );
    }

    // ─── Активное открытие (клиент) ────────────────────────────────────────

    /// Отправляет SYN → переходит в SynSent
    pub fn connect(&mut self) {
        self.state = TcpState::SynSent;
        self.snd_nxt = self.iss.wrapping_add(1);
        self.send_segment(TCP_FLAG_SYN, &[]);
        self.retransmit_tick = crate::scheduler::get_ticks() + RETRANSMIT_TIMEOUT;
    }

    // ─── Пассивное открытие (сервер) ───────────────────────────────────────

    /// Отправляет SYN-ACK в ответ на полученный SYN
    pub fn accept_syn(&mut self, client_seq: u32) {
        self.rcv_nxt = client_seq.wrapping_add(1);
        self.state = TcpState::SynReceived;
        self.snd_nxt = self.iss.wrapping_add(1);
        self.send_segment(TCP_FLAG_SYN | TCP_FLAG_ACK, &[]);
        self.retransmit_tick = crate::scheduler::get_ticks() + RETRANSMIT_TIMEOUT;
    }

    // ─── Обработка входящего пакета (FSM) ─────────────────────────────────

    /// Обрабатывает входящий TCP пакет. Главный метод FSM.
    pub fn handle_incoming(&mut self, pkt: &TcpPacket) {
        let flags = pkt.header.flags();
        let seq = pkt.header.sequence_number();
        let ack = pkt.header.acknowledgment_number();
        let has_syn = flags & TCP_FLAG_SYN != 0;
        let has_ack = flags & TCP_FLAG_ACK != 0;
        let has_fin = flags & TCP_FLAG_FIN != 0;
        let has_rst = flags & TCP_FLAG_RST != 0;
        let _has_psh = flags & TCP_FLAG_PSH != 0;

        // RST — немедленно закрываем
        if has_rst {
            self.state = TcpState::Closed;
            return;
        }

        match self.state {
            TcpState::Listen => {
                // Ожидаем SYN
                if has_syn && !has_ack {
                    self.accept_syn(seq);
                }
            },

            TcpState::SynSent => {
                // Ожидаем SYN-ACK
                if has_syn && has_ack {
                    if ack == self.snd_nxt {
                        self.rcv_nxt = seq.wrapping_add(1);
                        self.snd_una = ack;
                        self.state = TcpState::Established;
                        // Отправляем ACK
                        self.send_segment(TCP_FLAG_ACK, &[]);
                    }
                }
            },

            TcpState::SynReceived => {
                // Ожидаем ACK (завершение 3-way handshake)
                if has_ack && ack == self.snd_nxt {
                    self.snd_una = ack;
                    self.state = TcpState::Established;
                }
            },

            TcpState::Established => {
                // Обновляем окно партнёра
                self.snd_wnd = ntohs(pkt.header.window_size);

                // Обновляем snd_una если пришёл ACK
                if has_ack {
                    if self.is_valid_ack(ack) {
                        self.snd_una = ack;
                        // Сдвигаем tx_buf на количество подтверждённых байт
                        let acked = ack.wrapping_sub(self.iss.wrapping_add(1)) as usize;
                        let already_acked = self
                            .tx_buf
                            .len()
                            .saturating_sub(self.snd_nxt.wrapping_sub(self.snd_una) as usize);
                        let to_drain = acked.min(self.tx_buf.len());
                        if to_drain > already_acked {
                            let drain = to_drain - already_acked;
                            self.tx_buf.drain(..drain.min(self.tx_buf.len()));
                        }
                    }
                }

                // Принимаем данные
                if !pkt.payload.is_empty() && seq == self.rcv_nxt {
                    // Добавляем в rx_buf (с ограничением)
                    let available = RX_BUF_SIZE.saturating_sub(self.rx_buf.len());
                    let to_copy = pkt.payload.len().min(available);
                    self.rx_buf.extend_from_slice(&pkt.payload[..to_copy]);
                    self.rcv_nxt = self.rcv_nxt.wrapping_add(to_copy as u32);

                    // Отправляем ACK
                    self.send_segment(TCP_FLAG_ACK, &[]);
                }

                // FIN от партнёра
                if has_fin {
                    self.rcv_nxt = self.rcv_nxt.wrapping_add(1);
                    self.send_segment(TCP_FLAG_ACK, &[]);
                    self.state = TcpState::CloseWait;
                }
            },

            TcpState::FinWait1 => {
                if has_ack {
                    self.snd_una = ack;
                    if has_fin {
                        // Simultaneous close
                        self.rcv_nxt = self.rcv_nxt.wrapping_add(1);
                        self.send_segment(TCP_FLAG_ACK, &[]);
                        self.state = TcpState::TimeWait;
                        self.timewait_deadline = crate::scheduler::get_ticks() + 2000;
                    } else {
                        self.state = TcpState::FinWait2;
                    }
                }
            },

            TcpState::FinWait2 => {
                if has_fin {
                    self.rcv_nxt = self.rcv_nxt.wrapping_add(1);
                    self.send_segment(TCP_FLAG_ACK, &[]);
                    self.state = TcpState::TimeWait;
                    self.timewait_deadline = crate::scheduler::get_ticks() + 2000;
                }
            },

            TcpState::LastAck => {
                if has_ack {
                    self.state = TcpState::Closed;
                }
            },

            TcpState::TimeWait => {
                // Игнорируем пакеты — ждём истечения таймера
            },

            _ => {},
        }
    }

    /// Отправляет данные партнёру (добавляет в tx_buf и отправляет сегменты)
    pub fn send_data(&mut self, data: &[u8]) -> usize {
        if self.state != TcpState::Established {
            return 0;
        }

        let window = self.snd_wnd as usize;
        let in_flight = self.snd_nxt.wrapping_sub(self.snd_una) as usize;
        let can_send = window.saturating_sub(in_flight);
        let to_send = data.len().min(can_send);

        if to_send == 0 {
            // Окно закрыто — буферизуем
            let space = TX_BUF_SIZE.saturating_sub(self.tx_buf.len());
            let buffered = data.len().min(space);
            self.tx_buf.extend_from_slice(&data[..buffered]);
            return buffered;
        }

        // Разбиваем на сегменты размером MSS
        let mut sent = 0;
        while sent < to_send {
            let chunk_size = (to_send - sent).min(MSS as usize);
            let chunk = &data[sent..sent + chunk_size];

            let flags = if sent + chunk_size == to_send {
                TCP_FLAG_PSH | TCP_FLAG_ACK
            } else {
                TCP_FLAG_ACK
            };

            self.send_segment(flags, chunk);
            self.snd_nxt = self.snd_nxt.wrapping_add(chunk_size as u32);
            sent += chunk_size;
        }

        sent
    }

    /// Читает данные из rx_buf
    pub fn recv_data(&mut self, buf: &mut [u8]) -> usize {
        let to_copy = buf.len().min(self.rx_buf.len());
        buf[..to_copy].copy_from_slice(&self.rx_buf[..to_copy]);
        self.rx_buf.drain(..to_copy);
        to_copy
    }

    /// Инициирует закрытие соединения (отправляет FIN)
    pub fn close(&mut self) {
        match self.state {
            TcpState::Established => {
                self.state = TcpState::FinWait1;
                self.send_segment(TCP_FLAG_FIN | TCP_FLAG_ACK, &[]);
                self.snd_nxt = self.snd_nxt.wrapping_add(1);
            },
            TcpState::CloseWait => {
                self.state = TcpState::LastAck;
                self.send_segment(TCP_FLAG_FIN | TCP_FLAG_ACK, &[]);
                self.snd_nxt = self.snd_nxt.wrapping_add(1);
            },
            _ => {},
        }
    }

    /// Отправляет RST — немедленное закрытие без handshake
    pub fn reset(&mut self) {
        self.send_segment(TCP_FLAG_RST, &[]);
        self.state = TcpState::Closed;
    }

    /// Проверяет валидность ACK (ACK должен быть в диапазоне snd_una..snd_nxt)
    fn is_valid_ack(&self, ack: u32) -> bool {
        // Обрабатываем переполнение sequence numbers
        let una = self.snd_una;
        let nxt = self.snd_nxt;
        if una <= nxt {
            ack >= una && ack <= nxt
        } else {
            // Wrapping case
            ack >= una || ack <= nxt
        }
    }

    /// Обрабатывает retransmit timeout (вызывается периодически)
    pub fn retransmit_check(&mut self) {
        if self.state == TcpState::Closed || self.state == TcpState::TimeWait {
            return;
        }

        // TimeWait: проверяем истечение таймера
        if self.state == TcpState::TimeWait {
            if crate::scheduler::get_ticks() >= self.timewait_deadline {
                self.state = TcpState::Closed;
            }
            return;
        }

        let now = crate::scheduler::get_ticks();
        if self.retransmit_tick > 0 && now >= self.retransmit_tick {
            if self.retransmit_count < 5 {
                // Повторно отправляем последний неподтверждённый сегмент
                match self.state {
                    TcpState::SynSent => {
                        self.snd_nxt = self.iss.wrapping_add(1);
                        self.send_segment(TCP_FLAG_SYN, &[]);
                    },
                    TcpState::SynReceived => {
                        self.snd_nxt = self.iss.wrapping_add(1);
                        self.send_segment(TCP_FLAG_SYN | TCP_FLAG_ACK, &[]);
                    },
                    TcpState::FinWait1 | TcpState::LastAck => {
                        self.send_segment(TCP_FLAG_FIN | TCP_FLAG_ACK, &[]);
                    },
                    _ => {},
                }
                self.retransmit_count += 1;
                // Экспоненциальный backoff (capped at 60s)
                let backoff = RETRANSMIT_TIMEOUT << self.retransmit_count.min(6);
                self.retransmit_tick = now + backoff;
            } else {
                // Превышено число попыток — сбрасываем соединение
                self.state = TcpState::Closed;
            }
        }
    }
}

// ─── Глобальная таблица соединений
// ────────────────────────────────────────────

/// Глобальная таблица активных TCP соединений
static TCP_CONNECTIONS: Mutex<BTreeMap<TcpKey, TcpConnection>> = Mutex::new(BTreeMap::new());

/// Таблица слушающих портов (server sockets)
/// port → очередь входящих соединений (backlog)
static TCP_LISTENERS: Mutex<BTreeMap<u16, Vec<TcpKey>>> = Mutex::new(BTreeMap::new());

/// Открывает порт для прослушивания входящих соединений.
pub fn listen(port: u16) {
    TCP_LISTENERS.lock().entry(port).or_insert_with(Vec::new);
}

/// Прекращает прослушивание порта.
pub fn unlisten(port: u16) {
    TCP_LISTENERS.lock().remove(&port);
}

/// Клиентское подключение: создаёт соединение и отправляет SYN.
pub fn connect(
    local_port: u16,
    remote_ip: [u8; 4],
    remote_mac: [u8; 6],
    remote_port: u16,
) -> TcpKey {
    let key = TcpKey {
        local_port,
        remote_ip,
        remote_port,
    };
    TCP_CONNECTIONS.lock().insert(
        key,
        TcpConnection::new(local_port, remote_ip, remote_mac, remote_port),
    );
    if let Some(conn) = TCP_CONNECTIONS.lock().get_mut(&key) {
        conn.connect();
    }
    key
}

/// Отправляет данные через соединение.
pub fn send(key: &TcpKey, data: &[u8]) -> usize {
    TCP_CONNECTIONS
        .lock()
        .get_mut(key)
        .map(|c| c.send_data(data))
        .unwrap_or(0)
}

/// Читает данные из rx_buf соединения.
pub fn recv(key: &TcpKey, buf: &mut [u8]) -> usize {
    TCP_CONNECTIONS
        .lock()
        .get_mut(key)
        .map(|c| c.recv_data(buf))
        .unwrap_or(0)
}

pub fn state(key: &TcpKey) -> Option<TcpState> {
    TCP_CONNECTIONS
        .lock()
        .get(key)
        .map(|connection| connection.state)
}

pub fn remove(key: &TcpKey) {
    TCP_CONNECTIONS.lock().remove(key);
}

/// Закрывает соединение (4-way teardown).
pub fn close(key: &TcpKey) {
    if let Some(conn) = TCP_CONNECTIONS.lock().get_mut(key) {
        conn.close();
    }
}

/// Обрабатывает входящий TCP сегмент (вызывается из ipv4::handle).
pub fn handle(src_ip: &[u8; 4], src_mac: &[u8; 6], data: &[u8]) {
    let pkt = match TcpPacket::parse(data) {
        Some(p) => p,
        None => return,
    };

    // Верифицируем checksum
    if !pkt.verify_checksum(src_ip, &crate::net::arp::MY_IP) {
        return; // Отбрасываем корруптированные пакеты
    }

    let dst_port = pkt.header.destination_port();
    let src_port = pkt.header.source_port();
    let key = TcpKey {
        local_port: dst_port,
        remote_ip: *src_ip,
        remote_port: src_port,
    };

    let mut conns = TCP_CONNECTIONS.lock();

    if let Some(conn) = conns.get_mut(&key) {
        // Известное соединение — передаём пакет в FSM
        conn.handle_incoming(&pkt);
    } else {
        // Новое соединение — проверяем, есть ли listener на этом порту
        let flags = pkt.header.flags();
        if flags & TCP_FLAG_SYN != 0 && flags & TCP_FLAG_ACK == 0 {
            let mut listeners = TCP_LISTENERS.lock();
            if let Some(backlog) = listeners.get_mut(&dst_port) {
                if backlog.len() < 16 {
                    // Создаём новое соединение
                    drop(listeners);
                    let mut conn = TcpConnection::new(dst_port, *src_ip, *src_mac, src_port);
                    conn.state = TcpState::Listen;
                    conn.handle_incoming(&pkt);
                    let new_key = TcpKey {
                        local_port: dst_port,
                        remote_ip: *src_ip,
                        remote_port: src_port,
                    };
                    // Добавляем в backlog listeners
                    TCP_LISTENERS
                        .lock()
                        .get_mut(&dst_port)
                        .map(|bl| bl.push(new_key));
                    conns.insert(new_key, conn);
                }
            } else {
                // Порт не слушается — отправляем RST
                drop(conns);
                send_rst(
                    src_ip,
                    src_mac,
                    src_port,
                    dst_port,
                    pkt.header.sequence_number(),
                );
            }
        }
        // ACK/FIN без соединения → RST (или тихо игнорируем)
    }
}

/// Принимает следующее готовое соединение на слушающем порту.
/// Возвращает TcpKey если соединение в состоянии Established.
pub fn accept(port: u16) -> Option<TcpKey> {
    let mut listeners = TCP_LISTENERS.lock();
    let backlog = listeners.get_mut(&port)?;

    // Ищем первое Established соединение в backlog
    let conns = TCP_CONNECTIONS.lock();
    let pos = backlog.iter().position(|key| {
        conns
            .get(key)
            .map(|c| c.state == TcpState::Established)
            .unwrap_or(false)
    })?;

    Some(backlog.remove(pos))
}

/// Периодически проверяет retransmit timeout для всех соединений.
/// Вызывается из timer tick или отдельной kernel-задачи.
pub fn retransmit_tick() {
    let mut conns = TCP_CONNECTIONS.lock();
    for conn in conns.values_mut() {
        conn.retransmit_check();
    }

    // Убираем Closed соединения
    conns.retain(|_, c| c.state != TcpState::Closed);
}

// ─── Checksum
// ─────────────────────────────────────────────────────────────────

/// Вычисляет TCP checksum с pseudo-header (RFC 793)
pub fn tcp_checksum(src_ip: &[u8; 4], dst_ip: &[u8; 4], tcp_bytes: &[u8]) -> u16 {
    let mut pseudo = Vec::with_capacity(12 + tcp_bytes.len());
    pseudo.extend_from_slice(src_ip);
    pseudo.extend_from_slice(dst_ip);
    pseudo.push(0); // reserved
    pseudo.push(6); // TCP protocol
    let tcp_len = tcp_bytes.len() as u16;
    pseudo.extend_from_slice(&tcp_len.to_be_bytes());
    pseudo.extend_from_slice(tcp_bytes);
    process_checksum(&pseudo)
}

/// Отправляет RST в ответ на неизвестный пакет
fn send_rst(dst_ip: &[u8; 4], dst_mac: &[u8; 6], dst_port: u16, src_port: u16, seq: u32) {
    let mut rst = Vec::with_capacity(20);
    rst.extend_from_slice(&src_port.to_be_bytes());
    rst.extend_from_slice(&dst_port.to_be_bytes());
    rst.extend_from_slice(&0u32.to_be_bytes()); // seq = 0
    rst.extend_from_slice(&seq.wrapping_add(1).to_be_bytes()); // ack = incoming seq + 1
    let flags: u16 = (5u16 << 12) | TCP_FLAG_RST as u16 | TCP_FLAG_ACK as u16;
    rst.extend_from_slice(&flags.to_be_bytes());
    rst.extend_from_slice(&0u16.to_be_bytes()); // window
    rst.extend_from_slice(&0u16.to_be_bytes()); // checksum placeholder
    rst.extend_from_slice(&0u16.to_be_bytes()); // urgent

    let chk = tcp_checksum(&crate::net::arp::MY_IP, dst_ip, &rst);
    rst[16] = (chk >> 8) as u8;
    rst[17] = (chk & 0xFF) as u8;

    crate::net::ipv4::send_packet(*dst_mac, *dst_ip, 6, &rst);
}
