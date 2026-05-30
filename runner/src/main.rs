use std::{
    path::PathBuf,
    process::{self, Command},
};

fn main() {
    let test_mode = std::env::args().any(|arg| arg == "--test");
    // Cargo подставляет путь к скомпилированному бинарнику ядра.
    // Формат: CARGO_BIN_FILE_{DEP_UPPERCASE}_{BIN_NAME}
    let kernel_path = PathBuf::from(env!("CARGO_BIN_FILE_KERNEL_kernel"));

    // Кладём образ в target/ workspace'а (runner/Cargo.toml → ../target)
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let workspace = manifest.parent().expect("no workspace root");
    let disk_image = workspace.join("target").join("disk.img");

    // BiosBoot упаковывает kernel ELF + SeaBIOS loader в сырой MBR-образ.
    // В отличие от bootloader 0.9, новый загрузчик корректно работает с QEMU 7+/8+.
    bootloader::BiosBoot::new(&kernel_path)
        .create_disk_image(&disk_image)
        .expect("не удалось создать образ диска");

    // Файл хранения (для FAT32)
    let storage_image = workspace.join("target").join("storage.img");
    if !storage_image.exists() {
        println!("[runner] Creating empty 10MB storage.img...");
        let empty_data = vec![0u8; 10 * 1024 * 1024]; // 10 Мегабайт
        std::fs::write(&storage_image, empty_data).expect("Не удалось создать пустой диск");
    }

    println!("[runner] Disk image (OS): {}", disk_image.display());
    println!(
        "[runner] Storage image (FAT32): {}",
        storage_image.display()
    );

    // Запускаем QEMU с образом как сырым диском.
    // Используем стандартный pc (i440fx) чипсет — он корректно читает MBR.
    let mut qemu = Command::new("qemu-system-x86_64");

    // Диск 0 (Master) - Загрузочный образ OS
    qemu.arg("-drive").arg(format!(
        "format=raw,file={},index=0,media=disk",
        disk_image.display()
    ));

    // Диск 1 (Slave) - Наш виртуальный жесткий диск данных
    qemu.arg("-drive").arg(format!(
        "format=raw,file={},index=1,media=disk",
        storage_image.display()
    ));

    qemu.arg("-device")
        .arg("rtl8139,netdev=net0")
        .arg("-netdev")
        .arg("user,id=net0");

    qemu.arg("-serial").arg("stdio");

    if test_mode {
        qemu.arg("-display")
            .arg("none")
            .arg("-no-reboot")
            .arg("-no-shutdown")
            .arg("-device")
            .arg("isa-debug-exit,iobase=0xf4,iosize=0x04");
    }

    let status = qemu.status().expect("не удалось запустить QEMU");
    if test_mode && status.code() == Some(33) {
        process::exit(0);
    }
    process::exit(status.code().unwrap_or(1));
}
