use core::ptr::{read_volatile, write_volatile};

#[repr(C)]
struct HbaPort {
    command_list_base: u32,
    command_list_base_upper: u32,
    fis_base: u32,
    fis_base_upper: u32,
    interrupt_status: u32,
    interrupt_enable: u32,
    command_and_status: u32,
    reserved0: u32,
    task_file_data: u32,
    signature: u32,
    sata_status: u32,
    sata_control: u32,
    sata_error: u32,
    sata_active: u32,
    command_issue: u32,
    sata_notification: u32,
    fis_switch_control: u32,
    reserved1: [u32; 11],
    vendor: [u32; 4],
}

#[repr(C)]
struct HbaMemory {
    capability: u32,
    global_control: u32,
    interrupt_status: u32,
    ports_implemented: u32,
    version: u32,
    ccc_control: u32,
    ccc_ports: u32,
    em_location: u32,
    em_control: u32,
    capability2: u32,
    bohc: u32,
    reserved: [u8; 116],
    vendor: [u8; 96],
    ports: [HbaPort; 32],
}

#[repr(C)]
struct CommandHeader {
    flags: u16,
    prdtlen: u16,
    prdbc: u32,
    ctba: u32,
    ctbau: u32,
    reserved: [u32; 4],
}

#[repr(C)]
struct PrdtEntry {
    dba: u32,
    dbau: u32,
    reserved0: u32,
    dbc: u32, // Byte count (up to 4MB)
}

#[repr(C)]
struct CommandTable {
    cfis: [u8; 64],
    acmd: [u8; 16],
    reserved: [u8; 48],
    prdt: [PrdtEntry; 1], // Для простоты пока 1 запись
}

pub static ABAR: spin::Mutex<u64> = spin::Mutex::new(0);

pub fn init(abar: u64) {
    *ABAR.lock() = abar;
    let hba = unsafe { &mut *(abar as *mut HbaMemory) };

    // 1. Включаем AHCI режим (бит 31 в GHC)
    unsafe {
        write_volatile(
            &mut hba.global_control,
            read_volatile(&hba.global_control) | (1 << 31),
        );
    }

    // 2. Ищем подключенные диски
    let pi = unsafe { read_volatile(&hba.ports_implemented) };
    for i in 0..32 {
        if (pi & (1 << i)) != 0 {
            let port = &mut hba.ports[i];
            let ssts = unsafe { read_volatile(&port.sata_status) };

            let det = ssts & 0x0F;
            let ipm = (ssts >> 8) & 0x0F;

            if det == 3 && ipm == 1 {
                // Диск найден!
                let sig = unsafe { read_volatile(&port.signature) };
                if sig == 0x00000101 {
                    // SATA
                    init_port(port, i);
                }
            }
        }
    }
}

fn init_port(port: &mut HbaPort, _port_no: usize) {
    // 1. Останавливаем порт если он работает
    stop_port(port);

    // 2. Выделяем память под Command List и FIS
    // Используем фрейм аллокатор. Нам нужно 1KB + 256B.
    // Выделяем целый фрейм (4KB).
    if let Some(frame) = crate::frame_allocator::try_alloc_frame() {
        let phys_addr = frame.start_address().as_u64();

        // В начале фрейма — Command List (1KB)
        unsafe {
            write_volatile(&mut port.command_list_base, phys_addr as u32);
            write_volatile(&mut port.command_list_base_upper, (phys_addr >> 32) as u32);

            // Через 1KB — FIS (256B)
            let fis_addr = phys_addr + 1024;
            write_volatile(&mut port.fis_base, fis_addr as u32);
            write_volatile(&mut port.fis_base_upper, (fis_addr >> 32) as u32);

            // Обнуляем выделенную память
            let virt_offset = crate::paging::phys_mem_offset();
            let ptr = (virt_offset + phys_addr) as *mut u8;
            core::ptr::write_bytes(ptr, 0, 4096);
        }

        // 3. Запускаем порт
        start_port(port);
    }
}

fn start_port(port: &mut HbaPort) {
    unsafe {
        // Ждем пока порт перестанет быть занятым
        while (read_volatile(&port.command_and_status) & (1 << 15)) != 0 {}

        let mut cmd = read_volatile(&port.command_and_status);
        cmd |= 1 << 4; // FRE (FIS Receive Enable)
        cmd |= 1 << 0; // ST (Start)
        write_volatile(&mut port.command_and_status, cmd);
    }
}

fn stop_port(port: &mut HbaPort) {
    unsafe {
        let mut cmd = read_volatile(&port.command_and_status);
        cmd &= !(1 << 0); // ST (Start)
        cmd &= !(1 << 4); // FRE (FIS Receive Enable)
        write_volatile(&mut port.command_and_status, cmd);

        // Ждем пока биты CR и FR обнулятся
        loop {
            let status = read_volatile(&port.command_and_status);
            if (status & (1 << 15)) == 0 && (status & (1 << 14)) == 0 {
                break;
            }
        }
    }
}

pub unsafe fn read(port_no: usize, sector: u64, count: u16, buffer_phys: u64) -> bool {
    let abar = *ABAR.lock();
    if abar == 0 {
        return false;
    }
    let hba = &mut *(abar as *mut HbaMemory);
    let port = &mut hba.ports[port_no];

    // 1. Ищем свободный слот
    let slot = match find_free_slot(port) {
        Some(s) => s,
        None => return false,
    };

    let cl_addr = ((port.command_list_base_upper as u64) << 32) | (port.command_list_base as u64);
    let virt_offset = crate::paging::phys_mem_offset();
    let header = &mut *((virt_offset + cl_addr + (slot as u64 * 32)) as *mut CommandHeader);

    // 2. Настраиваем хедер (8 dwords, команда будет 5 dwords = 20 bytes)
    header.flags = (5 << 0) | (0 << 6); // 5 dwords, не атовская
    header.prdtlen = 1;

    // 3. Таблица команд (пока упрощенно — за хедером в том же фрейме или аллокация)
    // В init_port мы выделили 4KB. 0..1024 = CL, 1024..1280 = FIS.
    // Остается место под CT (Command Tables).
    let ct_addr = cl_addr + 2048; // Примерно
    header.ctba = ct_addr as u32;
    header.ctbau = (ct_addr >> 32) as u32;

    let table = &mut *((virt_offset + ct_addr) as *mut CommandTable);
    core::ptr::write_bytes(
        table as *mut _ as *mut u8,
        0,
        core::mem::size_of::<CommandTable>(),
    );

    // 4. PRDT
    table.prdt[0].dba = buffer_phys as u32;
    table.prdt[0].dbau = (buffer_phys >> 32) as u32;
    table.prdt[0].dbc = (count as u32 * 512) - 1; // 0-indexed byte count

    // 5. Command FIS (Register H2D)
    table.cfis[0] = 0x27; // Register H2D
    table.cfis[1] = 0x80; // Command bit
    table.cfis[2] = 0x25; // READ DMA EXT (LBA48)

    table.cfis[4] = sector as u8;
    table.cfis[5] = (sector >> 8) as u8;
    table.cfis[6] = (sector >> 16) as u8;
    table.cfis[7] = 0x40; // LBA mode

    table.cfis[8] = (sector >> 24) as u8;
    table.cfis[9] = (sector >> 32) as u8;
    table.cfis[10] = (sector >> 40) as u8;

    table.cfis[12] = count as u8;
    table.cfis[13] = (count >> 8) as u8;

    // 6. Запуск команды
    write_volatile(&mut port.command_issue, 1 << slot);

    // 7. Ждем завершения
    loop {
        if (read_volatile(&port.command_issue) & (1 << slot)) == 0 {
            break;
        }
        if (read_volatile(&port.interrupt_status) & (1 << 30)) != 0 {
            return false;
        } // Error
    }

    true
}

pub unsafe fn write(port_no: usize, sector: u64, count: u16, buffer_phys: u64) -> bool {
    let abar = *ABAR.lock();
    if abar == 0 {
        return false;
    }
    let hba = &mut *(abar as *mut HbaMemory);
    let port = &mut hba.ports[port_no];

    let slot = match find_free_slot(port) {
        Some(s) => s,
        None => return false,
    };

    let cl_addr = ((port.command_list_base_upper as u64) << 32) | (port.command_list_base as u64);
    let virt_offset = crate::paging::phys_mem_offset();
    let header = &mut *((virt_offset + cl_addr + (slot as u64 * 32)) as *mut CommandHeader);

    header.flags = (5 << 0) | (1 << 6); // 5 dwords, write bit set
    header.prdtlen = 1;

    let ct_addr = cl_addr + 2048;
    header.ctba = ct_addr as u32;
    header.ctbau = (ct_addr >> 32) as u32;

    let table = &mut *((virt_offset + ct_addr) as *mut CommandTable);
    core::ptr::write_bytes(
        table as *mut _ as *mut u8,
        0,
        core::mem::size_of::<CommandTable>(),
    );

    table.prdt[0].dba = buffer_phys as u32;
    table.prdt[0].dbau = (buffer_phys >> 32) as u32;
    table.prdt[0].dbc = (count as u32 * 512) - 1;

    table.cfis[0] = 0x27;
    table.cfis[1] = 0x80;
    table.cfis[2] = 0x35; // WRITE DMA EXT

    table.cfis[4] = sector as u8;
    table.cfis[5] = (sector >> 8) as u8;
    table.cfis[6] = (sector >> 16) as u8;
    table.cfis[7] = 0x40; // LBA mode

    table.cfis[8] = (sector >> 24) as u8;
    table.cfis[9] = (sector >> 32) as u8;
    table.cfis[10] = (sector >> 40) as u8;

    table.cfis[12] = count as u8;
    table.cfis[13] = (count >> 8) as u8;

    write_volatile(&mut port.command_issue, 1 << slot);

    loop {
        if (read_volatile(&port.command_issue) & (1 << slot)) == 0 {
            break;
        }
        if (read_volatile(&port.interrupt_status) & (1 << 30)) != 0 {
            return false;
        }
    }

    true
}

fn find_free_slot(port: &HbaPort) -> Option<usize> {
    let slots = unsafe { read_volatile(&port.command_issue) | read_volatile(&port.sata_active) };
    for i in 0..32 {
        if (slots & (1 << i)) == 0 {
            return Some(i);
        }
    }
    None
}
