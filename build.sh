# Rust OS Build and Test Script
# Используется для локальной разработки и CI/CD

#!/bin/bash

set -e

echo "=== Rust OS Build Script ==="

# Проверяем наличие Rust nightly
if ! rustup toolchain list | grep -q nightly; then
    echo "Installing Rust nightly..."
    rustup toolchain install nightly
fi

# Устанавливаем nightly как default для этого проекта
rustup override set nightly

# Устанавливаем необходимые компоненты
echo "Installing required components..."
rustup component add rust-src --toolchain nightly
rustup component add llvm-tools-preview --toolchain nightly
rustup component add rustfmt --toolchain nightly
rustup component add clippy --toolchain nightly
rustup target add x86_64-unknown-none --toolchain nightly

# Проверяем код
echo ""
echo "=== Running cargo check ==="
cargo check --workspace

echo ""
echo "=== Checking code formatting ==="
cargo fmt --all -- --check

echo ""
echo "=== Running clippy ==="
cargo clippy --workspace -- -D clippy::correctness -D unused_must_use

# Сборка
echo ""
echo "=== Building kernel ==="
cargo build --manifest-path kernel/Cargo.toml --release

echo ""
echo "=== Building runner ==="
cargo build --manifest-path runner/Cargo.toml --release

echo ""
echo "=== Building complete OS image ==="
cargo build --release

echo ""
echo "=== Build completed successfully! ==="
echo ""
echo "To run the OS in QEMU, use: cargo run --release"
echo "To run headless tests, use: cargo run --release -p runner --features test-mode -- --test"
