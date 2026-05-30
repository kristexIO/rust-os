# RustOS - A Hobby Operating System in Rust

A 64-bit hobby operating system written from scratch in Rust, featuring a GUI desktop environment, an experimental HTTP networking path, and kernel subsystem prototypes.

## 🚀 Features

### Core System
- **x86_64 Architecture**: 64-bit kernel running in Ring 0
- **Memory Management**: 
  - Paging with virtual memory support
  - Multiple allocator types (Buddy, Slab, Bump, Bitmap)
  - Copy-on-Write (CoW) support
  - Frame allocator using bootloader memory map
- **Process Management**:
  - Process and CFS data structures with vruntime tracking
  - `fork`/`exec` syscall numbers reserved; currently return `ENOSYS`
  - The live GUI/shell runtime currently executes through the cooperative async executor
  - Priority levels via nice values (-20 to +19)
  - Inter-process communication (IPC) via message passing
  - Signal handling support
- **Syscall Interface**: implemented file/IPC/time calls plus reserved process calls:
  - File I/O (open, read, write, close)
  - Process management (`exit`, `getpid`; `fork` and `exec` are not operational yet)
  - IPC (send_msg, recv_msg, kill, signal)
  - Time (gettime)
  - Memory (shm_get, shm_at)

### File System
- **Virtual File System (VFS)**: Unified interface for multiple filesystems
- **RamFS**: In-memory filesystem with full read/write support
- **Path Resolution**: Support for `.`, `..`, relative and absolute paths
- **File Permissions**: ACL-based security system

### GUI Desktop Environment
- **Window Manager**: 
  - XFCE-style compositor with window decorations
  - Taskbar with start menu, clock, and task buttons
  - Window dragging, focus management, and z-ordering
  - Dirty-rect optimization for efficient rendering
- **Applications**:
  - Terminal emulator with ANSI escape codes
  - Interactive shell with command history
  - Calculator, Paint, Games (Snake, Tetris)
  - System Monitor, Web Browser (experimental HTTP only)
- **Font Rendering**: 8x8 bitmap font rendering via font8x8 crate

### Networking
- **RTL8139 Driver**: Ethernet network card support
- **Protocol Stack**:
  - ARP (Address Resolution Protocol)
  - IPv4 (Internet Protocol)
  - ICMP (Ping support)
  - TCP (Transmission Control Protocol)
  - HTTP Client (HTTP/1.0 GET requests; HTTPS/TLS is explicitly rejected)
- **Network Commands**: `ping`, `http`, `ifconfig`

### Shell
The built-in shell supports numerous commands:
- **File System**: `ls`, `ll`, `cd`, `mkdir`, `touch`, `cat`, `rm`, `pwd`
- **System**: `help`, `echo`, `bench`, `tests`, `lspci`, `gdt`, `idt`
- **Network**: `ifconfig`, `ping`, `http`
- **Utilities**: `clear`/`cls`, command history with arrow keys, tab completion

### Security
- **ACL System**: Access Control Lists for files and resources
- **Process Isolation**: Memory protection via page tables
- **Syscall Whitelisting**: Per-process syscall filtering
- **Stack Protection Prototype**: canary manager exists; compiler/stack integration is not enforced yet
- **KASLR Prototype**: offset generation exists; kernel relocation is not enforced yet

### Drivers
- **Storage**: ATA, AHCI (SATA)
- **Input**: PS/2 Keyboard and Mouse
- **Network**: RTL8139 Ethernet
- **Display**: VirtIO GPU, Framebuffer
- **Sound**: PC Speaker
- **PCI/PCIe**: Device enumeration

## 🛠️ Recent Improvements (2026-04-12)

### ✅ Completed
1. **Fixed Unsafe Lifetime Extension**: Removed dangerous `transmute` to `'static` in kernel_main
2. **Enhanced Shell**:
   - Added `pwd` command
   - Added `ll` (long listing) command
   - Added `clear`/`cls` aliases
   - Implemented proper path resolution with `.` and `..`
   - Added directory existence validation in `cd`
   - Improved error messages across all commands
3. **Networking Path**:
   - Implemented ICMP Echo Request/Reply (ping)
   - Routed received RTL8139 frames through Ethernet/IPv4/TCP and integrated HTTP client
   - IP address parsing with validation
4. **Process API Groundwork**:
   - Reserved `sys_fork` (syscall 17) and `sys_exec` (syscall 18)
   - Both return `ENOSYS` until scheduled Ring 3 address spaces and ELF loading are connected
   - Enhanced zombie process cleanup with resource reclamation
   - Automatic periodic zombie reaping in timer interrupt
5. **Memory Leak Fixes**:
   - Proper cleanup of zombie process resources (stacks, process entries)
   - Automatic reaping every 100 timer ticks

### 🚧 In Progress
- Window destroy/cleanup when closed
- Bounds checking for GUI drawing functions
- Ring 3 program execution and CFS context switching

## 📁 Project Structure

```
rust-os/
├── kernel/                 # Main kernel code
│   └── src/
│       ├── main.rs        # Kernel entry point
│       ├── allocator.rs   # Heap allocator
│       ├── memory_allocators.rs  # Multiple allocator implementations
│       ├── process.rs     # Process management
│       ├── scheduler.rs   # CFS task scheduler
│       ├── syscall.rs     # Syscall interface
│       ├── interrupts.rs  # IDT and interrupt handlers
│       ├── paging.rs      # Virtual memory management
│       ├── vfs.rs         # Virtual file system
│       ├── ramfs.rs       # RAM filesystem
│       ├── shell.rs       # Shell implementation
│       ├── gui.rs         # GUI compositor
│       ├── graphics.rs    # Framebuffer abstraction
│       ├── window.rs      # Window management
│       ├── net/           # Networking stack
│       │   ├── rtl8139.rs # Network driver
│       │   ├── ipv4.rs    # IP protocol
│       │   ├── icmp.rs    # ICMP (ping)
│       │   ├── tcp.rs     # TCP protocol
│       │   └── http.rs    # HTTP client
│       └── driver/        # Hardware drivers
├── runner/                # QEMU runner
└── Cargo.toml            # Workspace configuration
```

## 🔨 Building and Running

### Prerequisites
- Nightly Rust toolchain
- Nightly `x86_64-unknown-none` target
- QEMU system emulator
- Build tools (make, cargo)

### Build Commands
```bash
rustup target add x86_64-unknown-none --toolchain nightly

# Build the kernel
cargo build --release -p kernel --target x86_64-unknown-none

# Run in QEMU
cargo run -p runner

# Run headless kernel tests in QEMU
cargo run --release -p runner --features test-mode -- --test
```

### Build Script
Alternatively, use the provided build scripts:
```bash
# Windows
.\build.bat

# Linux/macOS
./build.sh
```

## 🎯 Architecture Highlights

### Boot Process
1. **Bootloader** initializes hardware and passes memory map
2. **Kernel Main** initializes subsystems in order:
   - Allocator → Frame Allocator → Graphics → Terminal
   - GDT → Syscalls → Paging → IDT
   - PIC → Mouse → Interrupts enabled
   - VFS → Network → GUI Compositor
3. **Task Spawning**: PC Speaker chime, Mouse driver, Shell

### Memory Layout
- `0x0000_0000_0000_0000` - `0x0000_7FFF_FFFF_FFFF`: User space
- `0xFFFF_8000_0000_0000` - `0xFFFF_FFFF_FFFF_FFFF`: Kernel space (higher half)
- Physical memory mapped at offset from bootloader

### Task Scheduler
The CFS scheduler prototype implements:
- **vruntime**: Virtual runtime weighted by priority
- **Run Queue**: BTreeMap sorted by (vruntime, task_id)
- **Timeslicing**: Proportional to weight (nice value)

The live desktop tasks currently run on `task::Executor`; timer ticks do not yet perform CFS context switches.

## 📝 Syscall Table

| Number | Name         | Description                    |
|--------|--------------|--------------------------------|
| 0      | sys_write    | Write to file descriptor       |
| 1      | sys_exit     | Terminate process              |
| 2      | sys_getpid   | Get process ID                 |
| 3      | sys_sleep    | Sleep (busy-wait)              |
| 4      | sys_yield    | Yield CPU                      |
| 5      | sys_open     | Open file                      |
| 6      | sys_read     | Read from file descriptor      |
| 7      | sys_close    | Close file descriptor          |
| 8      | sys_mkdir    | Create directory               |
| 9      | sys_unlink   | Delete file                    |
| 10     | sys_gettime  | Get timestamp counter          |
| 11     | sys_shm_get  | Create shared memory           |
| 12     | sys_shm_at   | Attach shared memory           |
| 13     | sys_send_msg | Send IPC message               |
| 14     | sys_recv_msg | Receive IPC message            |
| 15     | sys_signal   | Set signal handler             |
| 16     | sys_kill     | Send signal to process         |
| 17     | sys_fork     | Reserved; returns `ENOSYS`    |
| 18     | sys_exec     | Reserved; returns `ENOSYS`    |

## 🐛 Known Issues & Limitations

### Current Limitations
1. **Single Core**: No SMP support yet
2. **Cooperative Multitasking**: Tasks must yield cooperatively
3. **No ELF Loader / Ring 3 Runtime**: `fork`/`exec` return `ENOSYS`
4. **Network**: QEMU user-mode networking only; browser supports HTTP but no TLS/HTTPS
5. **Filesystem**: FAT32 support incomplete
6. **No Swap**: Limited to physical RAM

### Future Work
- [ ] ELF binary loader for running actual programs
- [ ] Preemptive multitasking with proper context switching
- [ ] TLS implementation for HTTPS browser navigation
- [ ] SMP support with per-CPU run queues
- [ ] Virtual memory with swap/paging to disk
- [ ] Complete FAT32 driver with read/write support
- [ ] USB device support
- [ ] Sound card driver (beyond PC speaker)
- [ ] Multiple monitor support
- [ ] POSIX compatibility layer

## 📚 Documentation

### Code Documentation
- All public APIs are documented with Rustdoc comments
- Architecture decisions documented in code comments
- Complex algorithms explained with pseudocode

### External Resources
- OSDev Wiki: https://wiki.osdev.org/
- Intel SDM: https://www.intel.com/sdm
- AMD APM: https://www.amd.com/system/files/TechDocs/24593.pdf
- Rust OSDev Guide: https://os.phil-opp.com/

## 🤝 Contributing

This is a personal learning project, but suggestions and improvements are welcome!

### Code Style
- Follow Rust conventions and clippy lints
- Document all public APIs
- Keep unsafe blocks minimal and well-audited
- Add tests for new features where possible

## 📄 License

This project is provided as-is for educational purposes. Feel free to use the code as inspiration for your own OS projects!

## 🙏 Acknowledgments

- **Philipp Oppermann**: Excellent Rust OSDev tutorials
- **OSDev Community**: Invaluable resource for OS development
- **Rust Team**: For the amazing language and tooling
- **Crate Authors**: font8x8, x86_64, bootloader, and other crate maintainers

---

**Last Updated**: April 12, 2026  
**Version**: 0.2.0 (Development)
