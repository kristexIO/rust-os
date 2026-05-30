use core::sync::atomic::{AtomicBool, AtomicU8, Ordering};

use lazy_static::lazy_static;
use pc_keyboard::{layouts, DecodedKey, HandleControl, Keyboard, ScancodeSet1};
use spin::Mutex;
use x86_64::instructions::port::Port;

#[derive(Clone, Copy, PartialEq)]
pub enum KeyboardLayout {
    EN,
    RU,
}
pub static CURRENT_LAYOUT: AtomicU8 = AtomicU8::new(0); // 0=EN, 1=RU

pub fn get_layout() -> KeyboardLayout {
    if CURRENT_LAYOUT.load(Ordering::Relaxed) == 1 {
        KeyboardLayout::RU
    } else {
        KeyboardLayout::EN
    }
}
pub fn toggle_layout() {
    let cur = CURRENT_LAYOUT.load(Ordering::Relaxed);
    CURRENT_LAYOUT.store(1 - cur, Ordering::Relaxed);
}

fn translate_ru(c: char) -> char {
    match c {
        'q' => 'й',
        'w' => 'ц',
        'e' => 'у',
        'r' => 'к',
        't' => 'е',
        'y' => 'н',
        'u' => 'г',
        'i' => 'ш',
        'o' => 'щ',
        'p' => 'з',
        '[' => 'х',
        ']' => 'ъ',
        'a' => 'ф',
        's' => 'ы',
        'd' => 'в',
        'f' => 'а',
        'g' => 'п',
        'h' => 'р',
        'j' => 'о',
        'k' => 'л',
        'l' => 'д',
        ';' => 'ж',
        '\'' => 'э',
        'z' => 'я',
        'x' => 'ч',
        'c' => 'с',
        'v' => 'м',
        'b' => 'и',
        'n' => 'т',
        'm' => 'ь',
        ',' => 'б',
        '.' => 'ю',
        '/' => '.',
        'Q' => 'Й',
        'W' => 'Ц',
        'E' => 'У',
        'R' => 'К',
        'T' => 'Е',
        'Y' => 'Н',
        'U' => 'Г',
        'I' => 'Ш',
        'O' => 'Щ',
        'P' => 'З',
        '{' => 'Х',
        '}' => 'Ъ',
        'A' => 'Ф',
        'S' => 'Ы',
        'D' => 'В',
        'F' => 'А',
        'G' => 'П',
        'H' => 'Р',
        'J' => 'О',
        'K' => 'Л',
        'L' => 'Д',
        ':' => 'Ж',
        '"' => 'Э',
        'Z' => 'Я',
        'X' => 'Ч',
        'C' => 'С',
        'V' => 'М',
        'B' => 'И',
        'N' => 'Т',
        'M' => 'Ь',
        '<' => 'Б',
        '>' => 'Ю',
        '?' => ',',
        '`' => 'ё',
        '~' => 'Ё',
        '@' => '"',
        '#' => '№',
        '$' => ';',
        '^' => ':',
        '&' => '?',
        _ => c,
    }
}

const QUEUE_SIZE: usize = 256;

pub struct KeyQueue {
    buffer: [char; QUEUE_SIZE],
    head: usize,
    tail: usize,
}

impl KeyQueue {
    pub const fn new() -> Self {
        KeyQueue {
            buffer: ['\0'; QUEUE_SIZE],
            head: 0,
            tail: 0,
        }
    }

    pub fn push(&mut self, c: char) {
        let next_tail = (self.tail + 1) % QUEUE_SIZE;
        if next_tail != self.head {
            self.buffer[self.tail] = c;
            self.tail = next_tail;
        }
    }

    pub fn pop(&mut self) -> Option<char> {
        if self.head == self.tail {
            None
        } else {
            let c = self.buffer[self.head];
            self.head = (self.head + 1) % QUEUE_SIZE;
            Some(c)
        }
    }
}

lazy_static! {
    static ref KEYBOARD: Mutex<Keyboard<layouts::Us104Key, ScancodeSet1>> =
        Mutex::new(Keyboard::new(
            ScancodeSet1::new(),
            layouts::Us104Key,
            HandleControl::Ignore
        ));
    static ref LALT_PRESSED: AtomicBool = AtomicBool::new(false);
}

// Статическая очередь символов. Без динамической аллокации (Vec),
// чтобы избежать дедлока глобального аллокатора при работе прерываний.
pub static KEY_QUEUE: Mutex<KeyQueue> = Mutex::new(KeyQueue::new());

use core::{
    future::Future,
    pin::Pin,
    task::{Context, Poll, Waker},
};

// Глобальный Waker для асинхронного пробуждения таски шелла
static WAKER: Mutex<Option<Waker>> = Mutex::new(None);

// Внутренний обработчик прерывания IRQ1
pub fn handle_keyboard_interrupt() {
    let mut port = Port::new(0x60);
    // Читаем сырой сканкод из порта клавиатуры PS/2
    let scancode: u8 = unsafe { port.read() };

    let mut keyboard = KEYBOARD.lock();
    if let Ok(Some(key_event)) = keyboard.add_byte(scancode) {
        if key_event.code == pc_keyboard::KeyCode::LAlt {
            LALT_PRESSED.store(
                key_event.state == pc_keyboard::KeyState::Down,
                Ordering::Relaxed,
            );
        }
        if key_event.code == pc_keyboard::KeyCode::LShift
            && key_event.state == pc_keyboard::KeyState::Down
        {
            if LALT_PRESSED.load(Ordering::Relaxed) {
                toggle_layout();
                if let Some(mut comp) = crate::gui::COMPOSITOR.try_lock() {
                    comp.request_full_redraw();
                }
            }
        }
        if let Some(key) = keyboard.process_keyevent(key_event) {
            match key {
                DecodedKey::Unicode(mut character) => {
                    if get_layout() == KeyboardLayout::RU {
                        character = translate_ru(character);
                    }
                    KEY_QUEUE.lock().push(character);
                    if let Some(waker) = WAKER.lock().take() {
                        waker.wake();
                    }
                },
                DecodedKey::RawKey(k) => match k {
                    pc_keyboard::KeyCode::Tab => {
                        if LALT_PRESSED.load(Ordering::Relaxed) {
                            KEY_QUEUE.lock().push('\x13'); // Alt+Tab
                            if let Some(waker) = WAKER.lock().take() {
                                waker.wake();
                            }
                        }
                    },
                    pc_keyboard::KeyCode::F4 => {
                        if LALT_PRESSED.load(Ordering::Relaxed) {
                            KEY_QUEUE.lock().push('\x14'); // Alt+F4
                            if let Some(waker) = WAKER.lock().take() {
                                waker.wake();
                            }
                        }
                    },
                    pc_keyboard::KeyCode::ArrowLeft => {
                        if LALT_PRESSED.load(Ordering::Relaxed) {
                            KEY_QUEUE.lock().push('\x15'); // Alt+Left
                            if let Some(waker) = WAKER.lock().take() {
                                waker.wake();
                            }
                        }
                    },
                    pc_keyboard::KeyCode::ArrowRight => {
                        if LALT_PRESSED.load(Ordering::Relaxed) {
                            KEY_QUEUE.lock().push('\x16'); // Alt+Right
                            if let Some(waker) = WAKER.lock().take() {
                                waker.wake();
                            }
                        }
                    },
                    pc_keyboard::KeyCode::ArrowUp => {
                        KEY_QUEUE.lock().push('\x11');
                        if let Some(waker) = WAKER.lock().take() {
                            waker.wake();
                        }
                    },
                    pc_keyboard::KeyCode::ArrowDown => {
                        KEY_QUEUE.lock().push('\x12');
                        if let Some(waker) = WAKER.lock().take() {
                            waker.wake();
                        }
                    },
                    _ => {},
                },
            }
        }
    }
}

// Старая синхронная функция
pub fn get_char_sync() -> Option<char> {
    x86_64::instructions::interrupts::without_interrupts(|| KEY_QUEUE.lock().pop())
}

pub struct AsyncChar;

impl Future for AsyncChar {
    type Output = char;
    fn poll(self: Pin<&mut Self>, cx: &mut Context) -> Poll<char> {
        // Пробуем забрать символ (очередь проверяется быстро)
        if let Some(c) = get_char_sync() {
            Poll::Ready(c)
        } else {
            // Если символа нет, сохраняем Waker, чтобы нас разбудил IRQ1
            *WAKER.lock() = Some(cx.waker().clone());
            Poll::Pending
        }
    }
}

pub async fn get_char() -> char {
    AsyncChar.await
}
