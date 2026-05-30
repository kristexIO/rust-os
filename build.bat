@echo off
REM Rust OS Build and Test Script for Windows
REM Используется для локальной разработки и CI/CD

echo === Rust OS Build Script (Windows) ===

REM Проверяем наличие Rust nightly
rustup toolchain list | findstr /C:"nightly" >nul
if errorlevel 1 (
    echo Installing Rust nightly...
    rustup toolchain install nightly
)

REM Устанавливаем nightly как default для этого проекта
rustup override set nightly

REM Устанавливаем необходимые компоненты
echo Installing required components...
rustup component add rust-src --toolchain nightly
rustup component add llvm-tools-preview --toolchain nightly
rustup component add rustfmt --toolchain nightly
rustup component add clippy --toolchain nightly
rustup target add x86_64-unknown-none --toolchain nightly

REM Проверяем код
echo.
echo === Running cargo check ===
cargo check --workspace
if errorlevel 1 goto error

echo.
echo === Checking formatting ===
cargo fmt --all -- --check
if errorlevel 1 goto error

echo.
echo === Running clippy ===
cargo clippy --workspace -- -D clippy::correctness -D unused_must_use
if errorlevel 1 goto error

REM Сборка
echo.
echo === Building kernel ===
cargo build --manifest-path kernel/Cargo.toml --release
if errorlevel 1 goto error

echo.
echo === Building runner ===
cargo build --manifest-path runner/Cargo.toml --release
if errorlevel 1 goto error

echo.
echo === Building complete OS image ===
cargo build --release
if errorlevel 1 goto error

echo.
echo === Build completed successfully! ===
echo.
echo To run the OS in QEMU, use: cargo run --release
echo To run headless tests, use: cargo run --release -p runner --features test-mode -- --test
goto end

:error
echo.
echo === Build failed! ===
exit /b 1

:end
