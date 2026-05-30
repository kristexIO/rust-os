/// HTTP клиент для bare-metal OS
/// Поддерживает HTTP/1.0 GET запросы через TCP стек
use alloc::{
    string::{String, ToString},
    vec::Vec,
};

use crate::net::tcp::{self, TcpState};

/// Разобранный URL
pub struct ParsedUrl {
    pub host: String,
    pub port: u16,
    pub path: String,
}

/// HTTP ответ
pub struct HttpResponse {
    pub status_code: u16,
    pub status_text: String,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HttpError {
    EmptyUrl,
    UnsupportedScheme,
    TlsUnsupported,
    InvalidAuthority,
    DnsFailed,
    ConnectTimeout,
    SendFailed,
    ResponseTimeout,
    ResponseTooLarge,
    InvalidResponse,
}

impl HttpError {
    pub fn message(self) -> &'static str {
        match self {
            HttpError::EmptyUrl => "URL is empty",
            HttpError::UnsupportedScheme => "Unsupported URL scheme",
            HttpError::TlsUnsupported => "HTTPS is not implemented yet; use an HTTP URL",
            HttpError::InvalidAuthority => "Invalid host or port in URL",
            HttpError::DnsFailed => "DNS resolution failed",
            HttpError::ConnectTimeout => "TCP connection timed out",
            HttpError::SendFailed => "Failed to send HTTP request",
            HttpError::ResponseTimeout => "Server returned no HTTP response",
            HttpError::ResponseTooLarge => "HTTP response exceeded browser limit",
            HttpError::InvalidResponse => "Invalid HTTP response",
        }
    }
}

/// Парсит URL
pub fn parse_url(url: &str) -> Result<ParsedUrl, HttpError> {
    let url = url.trim();
    if url.is_empty() {
        return Err(HttpError::EmptyUrl);
    }

    let without_proto = if let Some(without_proto) = url.strip_prefix("http://") {
        without_proto
    } else if url.starts_with("https://") {
        return Err(HttpError::TlsUnsupported);
    } else if url.contains("://") {
        return Err(HttpError::UnsupportedScheme);
    } else {
        url
    };

    // Разделяем host и path
    let (host_part, path_part) = if let Some(slash) = without_proto.find('/') {
        (&without_proto[..slash], &without_proto[slash..])
    } else {
        (without_proto, "/")
    };

    // Разделяем host и port
    let (host, port) = if let Some(colon) = host_part.rfind(':') {
        let port_str = &host_part[colon + 1..];
        let port = port_str
            .parse::<u16>()
            .map_err(|_| HttpError::InvalidAuthority)?;
        (&host_part[..colon], port)
    } else {
        (host_part, 80u16)
    };
    if host.is_empty() {
        return Err(HttpError::InvalidAuthority);
    }

    Ok(ParsedUrl {
        host: String::from(host),
        port,
        path: String::from(path_part),
    })
}

/// Простой DNS резолвер (хардкод для QEMU)
/// В QEMU user network DNS обычно 10.0.2.3
fn resolve_host(host: &str) -> Option<[u8; 4]> {
    // Для демо: хардкодим некоторые IP
    // В реальности нужен DNS клиент
    match host {
        "localhost" | "127.0.0.1" => Some([127, 0, 0, 1]),
        "example.com" => Some([93, 184, 215, 14]),
        "google.com" => Some([142, 250, 80, 46]),
        "rust-lang.org" => Some([185, 199, 108, 153]),
        "osdev.org" => Some([104, 26, 10, 140]),
        _ => {
            // Пытаемся распарсить как IP адрес
            let parts: Vec<&str> = host.split('.').collect();
            if parts.len() == 4 {
                let mut ip = [0u8; 4];
                for (i, part) in parts.iter().enumerate() {
                    ip[i] = part.parse::<u8>().ok()?;
                }
                Some(ip)
            } else {
                // Вызываем реальный DNS-резолвер!
                crate::net::dns::resolve_dns(host)
            }
        },
    }
}

fn resolve_arp(_ip: [u8; 4]) -> Option<[u8; 6]> {
    // Для QEMU user network, gateway MAC обычно 52:54:00:12:34:56.
    // Шлем всё на gateway MAC, чтобы виртуальный роутер QEMU делал NAT.
    Some([0x52, 0x54, 0x00, 0x12, 0x34, 0x56])
}

/// Выполняет HTTP GET запрос
pub fn http_get(url: &str) -> Result<HttpResponse, HttpError> {
    let parsed = parse_url(url)?;
    let dest_ip = resolve_host(&parsed.host).ok_or(HttpError::DnsFailed)?;
    let dest_mac = resolve_arp(dest_ip).ok_or(HttpError::DnsFailed)?;
    let local_port = 49152
        + (crate::gui::CLOCK_TICKS.load(core::sync::atomic::Ordering::Relaxed) % 16384) as u16;

    let key = tcp::connect(local_port, dest_ip, dest_mac, parsed.port);

    // Ждём established (упрощённо — спин с лимитом)
    let start = crate::gui::CLOCK_TICKS.load(core::sync::atomic::Ordering::Relaxed);
    while tcp::state(&key) != Some(TcpState::Established) {
        let now = crate::gui::CLOCK_TICKS.load(core::sync::atomic::Ordering::Relaxed);
        if now - start > 18 * 2 {
            tcp::remove(&key);
            return Err(HttpError::ConnectTimeout);
        }
        core::hint::spin_loop();
    }

    // Формируем HTTP GET запрос
    let mut request = String::from("GET ");
    request.push_str(&parsed.path);
    request.push_str(" HTTP/1.0\r\nHost: ");
    request.push_str(&parsed.host);
    request.push_str(
        "\r\nUser-Agent: RustOS-Browser/0.1\r\nAccept: text/html,text/plain,*/*\r\nConnection: \
         close\r\n\r\n",
    );

    // Отправляем
    if tcp::send(&key, request.as_bytes()) != request.len() {
        tcp::close(&key);
        tcp::remove(&key);
        return Err(HttpError::SendFailed);
    }

    // Read until peer closes or a short idle deadline expires.
    let rx_start = crate::gui::CLOCK_TICKS.load(core::sync::atomic::Ordering::Relaxed);
    let mut response_data = Vec::new();
    loop {
        let mut chunk = [0u8; 1024];
        let count = tcp::recv(&key, &mut chunk);
        if count != 0 {
            response_data.extend_from_slice(&chunk[..count]);
            if response_data.len() > 128 * 1024 {
                tcp::close(&key);
                tcp::remove(&key);
                return Err(HttpError::ResponseTooLarge);
            }
        }

        if !response_data.is_empty()
            && matches!(
                tcp::state(&key),
                None | Some(TcpState::CloseWait) | Some(TcpState::Closed)
            )
        {
            break;
        }

        let now = crate::gui::CLOCK_TICKS.load(core::sync::atomic::Ordering::Relaxed);
        if now - rx_start > 18 * 2 {
            if response_data.is_empty() {
                tcp::close(&key);
                tcp::remove(&key);
                return Err(HttpError::ResponseTimeout);
            }
            break;
        }
        core::hint::spin_loop();
    }

    tcp::close(&key);
    tcp::remove(&key);
    parse_http_response(&response_data).ok_or(HttpError::InvalidResponse)
}

/// Парсит HTTP ответ
fn parse_http_response(data: &[u8]) -> Option<HttpResponse> {
    // Ищем конец заголовков
    let header_end = find_subslice(data, b"\r\n\r\n")?;

    let header_section = &data[..header_end];
    let body = &data[header_end + 4..];

    // Парсим статус строку
    let first_line_end = find_subslice(header_section, b"\r\n").unwrap_or(header_section.len());
    let status_line = core::str::from_utf8(&header_section[..first_line_end]).ok()?;

    let parts: Vec<&str> = status_line.splitn(3, ' ').collect();
    if parts.len() < 2 {
        return None;
    }

    let status_code = parts[1].parse::<u16>().ok()?;
    let status_text = if parts.len() > 2 {
        String::from(parts[2])
    } else {
        String::new()
    };

    // Парсим заголовки
    let mut headers = Vec::new();
    let remaining = &header_section[first_line_end + 2..];
    for line in remaining.split(|&b| b == b'\r' || b == b'\n') {
        if line.is_empty() {
            continue;
        }
        if let Some(colon) = line.iter().position(|&b| b == b':') {
            let key = core::str::from_utf8(&line[..colon])
                .ok()?
                .trim()
                .to_string();
            let val = core::str::from_utf8(&line[colon + 1..])
                .ok()?
                .trim()
                .to_string();
            headers.push((key, val));
        }
    }

    Some(HttpResponse {
        status_code,
        status_text,
        headers,
        body: body.to_vec(),
    })
}

fn find_subslice(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() || haystack.len() < needle.len() {
        return None;
    }
    for i in 0..=(haystack.len() - needle.len()) {
        if &haystack[i..i + needle.len()] == needle {
            return Some(i);
        }
    }
    None
}
