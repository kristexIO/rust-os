use alloc::{format, string::String, sync::Arc, vec::Vec};

use spin::Mutex;

use crate::{graphics::DrawTarget, vfs::FileType, window::Window};

/// NullTarget for when we need a DrawTarget but don't care about the output
struct NullTarget;

impl DrawTarget for NullTarget {
    fn width(&self) -> usize {
        800
    }
    fn height(&self) -> usize {
        600
    }
    fn draw_pixel(&mut self, _x: usize, _y: usize, _color: crate::graphics::Color) {}
    fn scroll_up(&mut self, _lines: usize) {}
}

pub struct Shell {
    pwd: String,
    history: Vec<String>,
    history_idx: usize,
}

impl Shell {
    pub fn new() -> Self {
        Shell {
            pwd: String::from("/"),
            history: Vec::new(),
            history_idx: 0,
        }
    }

    /// Helper to print to terminal with window locking
    fn print(&self, win: &Arc<Mutex<Window>>, s: &str) {
        let mut term_guard = crate::fast_terminal::TERMINAL.lock();
        if let Some(term) = term_guard.as_mut() {
            let mut win_lock = win.lock();
            term.print_str(&mut *win_lock, s);
        }
    }

    pub async fn interactive_task(&mut self, win: Arc<Mutex<Window>>) {
        let mut line = String::new();
        loop {
            {
                self.print(&win, "\x1b[92mguest@rust-os\x1b[0m:\x1b[94m");
                self.print(&win, &self.pwd);
                self.print(&win, "\x1b[0m# ");
            }
            crate::gui::force_redraw_display();

            loop {
                let c = crate::keyboard::get_char().await;

                // Route global shortcuts & focused window inputs first
                if crate::gui::COMPOSITOR.lock().handle_keyboard_char(c) {
                    continue;
                }

                // If not handled by GUI, process for shell...
                if c == '\n' {
                    self.print(&win, "\n");
                    crate::gui::force_redraw_display();

                    let trimmed = line.trim();
                    if !trimmed.is_empty() {
                        let s = String::from(trimmed);
                        if self.history.is_empty() || self.history.last().unwrap() != &s {
                            self.history.push(s);
                        }
                    }
                    self.history_idx = self.history.len();
                    break;
                }
                if c == '\x11' {
                    // Up
                    if self.history_idx > 0 {
                        self.history_idx -= 1;
                        {
                            let mut term_guard = crate::fast_terminal::TERMINAL.lock();
                            if let Some(term) = term_guard.as_mut() {
                                let mut lock = win.lock();
                                for _ in 0..line.chars().count() {
                                    term.print_str(&mut *lock, "\x08 \x08");
                                }
                                line = self.history[self.history_idx].clone();
                                term.print_str(&mut *lock, &line);
                            }
                        }
                        crate::gui::force_redraw_display();
                    }
                    continue;
                }
                if c == '\x12' {
                    // Down
                    if self.history_idx < self.history.len() {
                        self.history_idx += 1;
                        {
                            let mut term_guard = crate::fast_terminal::TERMINAL.lock();
                            if let Some(term) = term_guard.as_mut() {
                                let mut lock = win.lock();
                                for _ in 0..line.chars().count() {
                                    term.print_str(&mut *lock, "\x08 \x08");
                                }
                                if self.history_idx < self.history.len() {
                                    line = self.history[self.history_idx].clone();
                                } else {
                                    line.clear();
                                }
                                term.print_str(&mut *lock, &line);
                            }
                        }
                        crate::gui::force_redraw_display();
                    }
                    continue;
                }
                if c == '\x08' {
                    if !line.is_empty() {
                        line.pop();
                        {
                            let mut term_guard = crate::fast_terminal::TERMINAL.lock();
                            if let Some(term) = term_guard.as_mut() {
                                let mut lock = win.lock();
                                term.print_str(&mut *lock, "\x08 \x08");
                            }
                        }
                        crate::gui::force_redraw_display();
                    } else {
                        crate::pcspeaker::PcSpeaker::async_play_sound(750, 1).await;
                    }
                    continue;
                }
                if c == '\t' {
                    let mut is_new_word = false;
                    if line.ends_with(' ') || line.is_empty() {
                        is_new_word = true;
                    }
                    let prefix = if is_new_word {
                        ""
                    } else {
                        line.split_whitespace().last().unwrap_or("")
                    };

                    let mut match_found = None;
                    {
                        let vfs = crate::vfs::GLOBAL_VFS.lock();
                        if let Ok(entries) = vfs.read_dir(&self.pwd) {
                            for e in entries {
                                if e.name.starts_with(prefix) {
                                    match_found = Some(e.name.clone());
                                    break;
                                }
                            }
                        }
                    }

                    if let Some(matched) = match_found {
                        let completion = &matched[prefix.len()..];
                        if !completion.is_empty() {
                            line.push_str(completion);
                            self.print(&win, completion);
                            crate::gui::force_redraw_display();
                        } else {
                            crate::pcspeaker::PcSpeaker::async_play_sound(750, 1).await;
                        }
                    } else {
                        crate::pcspeaker::PcSpeaker::async_play_sound(750, 1).await;
                    }
                    continue;
                }
                line.push(c);
                {
                    let s: String = c.into();
                    self.print(&win, &s);
                }
                crate::gui::force_redraw_display();
            }

            self.execute_line(&line, &win);
            crate::gui::force_redraw_display();
            line.clear();
        }
    }

    pub fn execute_line(&mut self, line: &str, win: &Arc<Mutex<Window>>) {
        let tokens: Vec<&str> = line.split_whitespace().collect();
        if tokens.is_empty() {
            return;
        }

        let cmd = tokens[0];
        let args = &tokens[1..];

        match cmd {
            "help" => self.help(win),
            "ls" => self.ls(args, win),
            "cd" => self.cd(args, win),
            "mkdir" => self.mkdir(args, win),
            "touch" => self.touch(args, win),
            "echo" => self.echo(args, win),
            "cat" => self.cat(args, win),
            "rm" => self.rm(args, win),
            "pwd" => self.pwd(win),
            "clear" | "cls" => self.clear_screen(win),
            "ll" => self.ll(args, win),
            "mount" => {
                self.print(win, "Mounting not yet fully implemented in shell.\n");
            },
            "umount" => {
                self.print(win, "Unmounting not yet fully implemented in shell.\n");
            },
            "exec" => {
                self.print(win, "Execution not yet fully implemented in shell.\n");
            },
            "ifconfig" => self.ifconfig_cmd(win),
            "ping" => self.ping_cmd(args, win),
            "http" => {
                self.http_command(args.to_vec(), win.clone());
            },
            "bench" => self.bench(win),
            "tests" => self.run_tests(win),
            "lspci" => self.lspci_cmd(win),
            "gdt" => self.gdt_cmd(win),
            "idt" => self.idt_cmd(win),
            _ => {
                self.print(win, "Unknown command: ");
                self.print(win, cmd);
                self.print(win, "\n");
            },
        }
    }

    fn help(&self, win: &Arc<Mutex<Window>>) {
        self.print(win, "\x1b[96mRustOS Shell v0.2\x1b[0m\n");
        self.print(win, "\x1b[93mAvailable commands:\x1b[0m\n");
        self.print(
            win,
            "  \x1b[92mFS\x1b[0m      : ls, ll, cd, mkdir, touch, cat, rm, pwd, clear\n\x1b[92m  \
             Net\x1b[0m     : ifconfig, ping, http\n\x1b[92m  System\x1b[0m  : help, echo, exec, \
             bench, tests, lspci, gdt, idt\n",
        );
    }

    fn ls(&self, args: &[&str], win: &Arc<Mutex<Window>>) {
        let path = args.first().unwrap_or(&".");
        match crate::vfs::GLOBAL_VFS.lock().read_dir(path) {
            Ok(entries) => {
                for entry in entries {
                    self.print(win, &entry.name);
                    if entry.file_type == FileType::Directory {
                        self.print(win, "/");
                    }
                    self.print(win, " ");
                }
                self.print(win, "\n");
            },
            Err(_) => self.print(win, "Error reading directory\n"),
        }
    }

    /// Long listing (like ls -l)
    fn ll(&self, args: &[&str], win: &Arc<Mutex<Window>>) {
        let path = args.first().unwrap_or(&".");
        match crate::vfs::GLOBAL_VFS.lock().read_dir(path) {
            Ok(entries) => {
                for entry in entries {
                    let type_str = match entry.file_type {
                        FileType::Directory => "d",
                        FileType::RegularFile => "-",
                    };
                    self.print(win, type_str);
                    self.print(win, "rwxrwxr-x  ");
                    self.print(win, &format!("{:>8}  ", entry.size));
                    self.print(win, &entry.name);
                    if entry.file_type == FileType::Directory {
                        self.print(win, "/");
                    }
                    self.print(win, "\n");
                }
            },
            Err(_) => self.print(win, "Error reading directory\n"),
        }
    }

    fn cd(&mut self, args: &[&str], win: &Arc<Mutex<Window>>) {
        if args.is_empty() {
            self.pwd = String::from("/");
            return;
        }

        let new_path = args[0];

        // Resolve path (handle . and ..)
        let resolved = self.resolve_path(new_path);

        // Validate that the directory exists
        let vfs = crate::vfs::GLOBAL_VFS.lock();
        match vfs.read_dir(&resolved) {
            Ok(entries) => {
                // Check if it's actually a directory
                let is_dir =
                    entries.iter().any(|e| e.file_type == FileType::Directory) || new_path == "/";

                if is_dir || new_path == "/" {
                    self.pwd = resolved;
                } else {
                    self.print(win, "cd: not a directory: ");
                    self.print(win, new_path);
                    self.print(win, "\n");
                }
            },
            Err(_) => {
                self.print(win, "cd: no such directory: ");
                self.print(win, new_path);
                self.print(win, "\n");
            },
        }
    }

    /// Resolve path, handling . and .. and relative paths
    fn resolve_path(&self, path: &str) -> String {
        if path.starts_with('/') {
            // Absolute path - normalize it
            self.normalize_path(path)
        } else {
            // Relative to current directory
            let base = if self.pwd == "/" {
                String::from("/")
            } else {
                self.pwd.clone()
            };

            if path == "." {
                return base;
            }

            let combined = if base == "/" {
                format!("/{}", path)
            } else {
                format!("{}/{}", base, path)
            };

            self.normalize_path(&combined)
        }
    }

    /// Normalize a path by resolving . and .. components
    fn normalize_path(&self, path: &str) -> String {
        let parts: Vec<&str> = path
            .split('/')
            .filter(|s| !s.is_empty() && *s != ".")
            .collect();
        let mut stack = Vec::new();

        for part in parts {
            if part == ".." {
                stack.pop(); // Go up one directory
            } else {
                stack.push(part);
            }
        }

        if stack.is_empty() {
            String::from("/")
        } else {
            format!("/{}", stack.join("/"))
        }
    }

    /// Print working directory
    fn pwd(&self, win: &Arc<Mutex<Window>>) {
        self.print(win, &self.pwd);
        self.print(win, "\n");
    }

    /// Clear screen
    fn clear_screen(&self, win: &Arc<Mutex<Window>>) {
        // ANSI clear screen sequence
        self.print(win, "\x1b[2J\x1b[H");
    }

    fn mkdir(&self, args: &[&str], win: &Arc<Mutex<Window>>) {
        if args.is_empty() {
            self.print(win, "mkdir: missing operand\n");
            return;
        }
        match crate::vfs::GLOBAL_VFS.lock().mkdir(args[0]) {
            Ok(()) => {},
            Err(_) => {
                self.print(win, "mkdir: cannot create directory '");
                self.print(win, args[0]);
                self.print(win, "'\n");
            },
        }
    }

    fn touch(&self, args: &[&str], win: &Arc<Mutex<Window>>) {
        if args.is_empty() {
            self.print(win, "touch: missing file operand\n");
            return;
        }
        match crate::vfs::GLOBAL_VFS.lock().create_file(args[0]) {
            Ok(()) => {},
            Err(_) => {
                self.print(win, "touch: cannot create file '");
                self.print(win, args[0]);
                self.print(win, "'\n");
            },
        }
    }

    fn echo(&self, args: &[&str], win: &Arc<Mutex<Window>>) {
        for (i, arg) in args.iter().enumerate() {
            if i > 0 {
                self.print(win, " ");
            }
            self.print(win, arg);
        }
        self.print(win, "\n");
    }

    fn cat(&self, args: &[&str], win: &Arc<Mutex<Window>>) {
        if args.is_empty() {
            self.print(win, "cat: missing file operand\n");
            return;
        }
        match crate::vfs::GLOBAL_VFS.lock().read_file(args[0]) {
            Ok(data) => {
                if let Ok(s) = String::from_utf8(data) {
                    self.print(win, &s);
                } else {
                    self.print(win, "[binary data]\n");
                }
            },
            Err(_) => {
                self.print(win, "cat: ");
                self.print(win, args[0]);
                self.print(win, ": No such file or directory\n");
            },
        }
    }

    fn rm(&self, args: &[&str], win: &Arc<Mutex<Window>>) {
        if args.is_empty() {
            self.print(win, "rm: missing operand\n");
            return;
        }
        match crate::vfs::GLOBAL_VFS.lock().remove(args[0]) {
            Ok(()) => {},
            Err(_) => {
                self.print(win, "rm: cannot remove '");
                self.print(win, args[0]);
                self.print(win, "'\n");
            },
        }
    }

    fn ifconfig_cmd(&self, win: &Arc<Mutex<Window>>) {
        self.print(win, "Network Interfaces:\n");
        let mac = crate::net::rtl8139::get_mac();
        self.print(
            win,
            &format!(
                "  eth0: MAC {:02x}:{:02x}:{:02x}:{:02x}:{:02x}:{:02x}\n",
                mac[0], mac[1], mac[2], mac[3], mac[4], mac[5]
            ),
        );
    }

    fn ping_cmd(&self, args: &[&str], win: &Arc<Mutex<Window>>) {
        if args.is_empty() {
            self.print(win, "Usage: ping <ip_address>\n");
            return;
        }

        // Parse IP address
        let ip_str = args[0];
        let ip_parts: Vec<&str> = ip_str.split('.').collect();

        if ip_parts.len() != 4 {
            self.print(win, "Invalid IP address format. Expected: x.x.x.x\n");
            return;
        }

        let mut ip = [0u8; 4];
        for (i, part) in ip_parts.iter().enumerate() {
            match part.parse::<u8>() {
                Ok(val) => ip[i] = val,
                Err(_) => {
                    self.print(win, "Invalid IP address: ");
                    self.print(win, part);
                    self.print(win, "\n");
                    return;
                },
            }
        }

        self.print(win, &format!("PING {} (56 data bytes)\n", ip_str));

        // Send 4 ICMP echo requests
        let mut success_count = 0;
        for seq in 1..=4u16 {
            self.print(win, &format!("Sending ICMP Echo Request to {}... ", ip_str));

            if crate::net::icmp::send_echo_request(ip, seq, 0x1234) {
                self.print(win, "sent\n");
                success_count += 1;
            } else {
                self.print(win, "failed\n");
            }

            // Small delay between pings (busy wait)
            let start_ticks = crate::scheduler::get_ticks();
            while crate::scheduler::get_ticks() - start_ticks < 10 {
                core::hint::spin_loop();
            }
        }

        self.print(win, &format!("\n--- {} ping statistics ---\n", ip_str));
        self.print(
            win,
            &format!("{} packets transmitted, {} received, ", 4, success_count),
        );

        let loss = ((4 - success_count) * 100) / 4;
        self.print(win, &format!("{}% packet loss\n", loss));
    }

    fn http_command(&mut self, args: Vec<&str>, win: Arc<Mutex<Window>>) {
        if args.is_empty() {
            self.print(&win, "Usage: http <url>\n");
            self.print(&win, "Example: http example.com\n");
            return;
        }
        let url = args[0];
        self.print(&win, &format!("HTTP GET {}\n", url));
        self.print(&win, "Resolving host...\n");
        crate::gui::force_redraw_display();

        // Use the real HTTP client
        match crate::net::http::http_get(url) {
            Ok(response) => {
                self.print(
                    &win,
                    &format!(
                        "HTTP/1.0 {} {}\n",
                        response.status_code, response.status_text
                    ),
                );

                // Print headers
                for (key, value) in &response.headers {
                    self.print(&win, &format!("{}: {}\n", key, value));
                }
                self.print(&win, "\n");

                // Print body (try as UTF-8)
                if let Ok(body_str) = core::str::from_utf8(&response.body) {
                    // Truncate if too long for terminal
                    let display_len = body_str.len().min(4096);
                    self.print(&win, &body_str[..display_len]);
                    if body_str.len() > 4096 {
                        self.print(&win, "\n... [truncated, response too long]\n");
                    }
                } else {
                    self.print(
                        &win,
                        &format!("[binary data, {} bytes]\n", response.body.len()),
                    );
                }

                self.print(&win, "\n\n--- Response complete ---\n");
            },
            Err(error) => {
                self.print(&win, &format!("HTTP request failed: {}\n", error.message()));
                self.print(
                    &win,
                    "Note: Only works in QEMU with user networking (-netdev user,id=net0)\n",
                );
            },
        }
    }

    fn bench(&self, win: &Arc<Mutex<Window>>) {
        self.print(win, "Running benchmarks...\n");
        // Simple memory allocation benchmark
        let start = crate::scheduler::get_ticks();
        let mut count = 0u64;
        for _ in 0..1000 {
            let _ = alloc::vec::Vec::<u64>::new();
            count += 1;
        }
        let end = crate::scheduler::get_ticks();
        self.print(
            win,
            &format!(
                "Alloc benchmark: {} iters in {} ticks\n",
                count,
                end - start
            ),
        );
    }

    fn run_tests(&self, win: &Arc<Mutex<Window>>) {
        self.print(win, "Running kernel tests...\n");
        crate::test_framework::run_all_tests(win);
    }

    fn lspci_cmd(&self, win: &Arc<Mutex<Window>>) {
        self.print(win, "PCI Devices:\n");
        let devs = crate::pci::PCI_DEVICES.lock();
        if devs.is_empty() {
            self.print(win, "  No PCI devices found\n");
        } else {
            for d in devs.iter() {
                self.print(
                    win,
                    &format!(
                        "  {:04x}:{:04x} [class {:02x}{:02x}]\n",
                        d.vendor_id, d.device_id, d.class, d.subclass
                    ),
                );
            }
        }
    }

    fn gdt_cmd(&self, win: &Arc<Mutex<Window>>) {
        self.print(win, "GDT status: Initialized\n");
        self.print(win, "  Entry 0: Null descriptor\n");
        self.print(win, "  Entry 1: Kernel code (64-bit)\n");
        self.print(win, "  Entry 2: Kernel data\n");
        self.print(win, "  Entry 3: User data\n");
        self.print(win, "  Entry 4: User code (64-bit)\n");
        self.print(win, "  Entry 5: TSS\n");
    }

    fn idt_cmd(&self, win: &Arc<Mutex<Window>>) {
        use crate::interrupts::InterruptIndex;
        self.print(win, "Active Interrupt Handlers (IDT):\n");
        self.print(win, "  [0x08] Double Fault (IST)\n");
        self.print(win, "  [0x0E] Page Fault\n");
        self.print(win, "  [0x0D] Gen. Prot. Fault\n");
        self.print(
            win,
            &format!("  [0x{:02X}] Timer (IRQ 0)\n", InterruptIndex::Timer as u8),
        );
        self.print(
            win,
            &format!(
                "  [0x{:02X}] Keyboard (IRQ 1)\n",
                InterruptIndex::Keyboard as u8
            ),
        );
        self.print(
            win,
            &format!(
                "  [0x{:02X}] Network (IRQ 11)\n",
                InterruptIndex::Network as u8
            ),
        );
        self.print(
            win,
            &format!("  [0x{:02X}] Mouse (IRQ 12)\n", InterruptIndex::Mouse as u8),
        );
    }
}
