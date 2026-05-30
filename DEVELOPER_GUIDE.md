# Rust-OS Developer Guide

Welcome to the `Rust-OS` Release Candidate! This OS is written from scratch in `no_std` Rust using `bootloader_api`, featuring a fully custom graphical environment.

## 1. System Architecture

The OS is divided into distinct layers:
*   **Kernel Core (`main.rs`, `memory.rs`, `interrupts.rs`)**: Handles hardware initialization, memory allocation (bump allocator with linked lists), and interrupt descriptors.
*   **Graphical Compositor (`gui.rs`, `graphics.rs`, `window.rs`)**: A custom Window Manager that utilizes a Dirty-Rectangle rendering system to redraw only the changing pixels. It supports alpha-blending for glassmorphism effects and provides native micro-animations.
*   **Application Layer (`apps/`)**: Contains all GUI applications. Applications operate within an `AppState` enum schema.
*   **Theme Engine (`theme.rs`)**: Centralizes the OS color palette to allow real-time theme swapping (Dark, Light, Ocean, Matrix).

## 2. Adding a New GUI Application

To add a new application to the OS, follow these logical steps:

1.  **Define App Data**: Open `kernel/src/apps/mod.rs` (or `kernel/src/apps.rs` depending on modularity step) and add your app to the `AppKind` enum.
2.  **Define State**: Add a new variant to the `AppState` enum detailing the state your app holds (e.g., `MyApp { clicks: u32 }`).
3.  **Initialize App**: In `AppState::new(kind: AppKind)`, instantiate your default state.
4.  **Register Window Size**: Update `app_window_size(kind)` to define the default resolution of your application.
5.  **Build the Renderer**: Create `fn render_my_app(win: &mut Window, state: &AppState)`. Use `win.buffer` and `fill_buf`/`draw_str_buf` to paint pixels.
6.  **Handle Inputs**: Create `my_app_handle_click` and route mouse coordinates, checking bounds for buttons.
7.  **Wire to Main Loop**: Inside `render_app` and `app_handle_click`, map your `AppState` variant to your handler functions.

## 3. Build System & Boot Errors

We utilize `cargo-bootimage` and QEMU to emulate the OS.

*   `build.bat` / `build.sh`: These scripts automate running `cargo bootimage` and launching QEMU with the resulting disk image.

**Common Boot Errors:**
*   *LNK or linker issues*: Ensure your `cargo` environment specifies the exact `x86_64-blog_os` custom target file, and verify `/MD` build tools are synchronized if interacting with foreign crates.
*   *Page Faults in GUI*: Ensure your app doesn't attempt to write to buffer indices `>= win.width * win.height`. The compositor will trap invalid memory accesses if bounds checking isn't strict.

## 4. Debugging & Benchmarks

*   **Logging**: The kernel provides serial logging. Output routes directly to the terminal stdout through the emulation layer. Use standard `log::info!` or the underlying serial print macros.
*   **System Stats**: Memory utilization and tick rates can be dynamically monitored via the built-in `SysInfo` or `TaskManager` applications.
*   **Performance Tuning**: If rendering appears slow:
    *   Verify `opt-level = 2` or `3` in `Cargo.toml`. The `blit_from_buffer` function heavily relies on auto-vectorization (SIMD) to flush changes efficiently.
    *   Minimize `COMPOSITOR.lock().request_full_redraw()`. Full screen flushes operate on 3MB of memory and should be avoided unless switching themes or desktops. Rely entirely on dirty rectangle rendering (`mark_dirty()`).

---

*Note on Project Cleanup: Ensure root-level logs, crash dump `errors.txt` files, and `[0-9]+` iteration backups are periodically scrubbed to keep the repository immaculate before commit hooks.*
