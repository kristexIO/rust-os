use spin::Mutex;
use x86_64::instructions::port::Port;

const DATA_PORT: u16 = 0x1F0;
const SECTOR_COUNT_PORT: u16 = 0x1F2;
const LBA_LO_PORT: u16 = 0x1F3;
const LBA_MID_PORT: u16 = 0x1F4;
const LBA_HI_PORT: u16 = 0x1F5;
const DRIVE_PORT: u16 = 0x1F6;
const STATUS_COMMAND_PORT: u16 = 0x1F7;

bitflags::bitflags! {
    struct StatusFlags: u8 {
        const ERR  = 1 << 0; // Error
        const DRQ  = 1 << 3; // Data Request Ready
        const SRV  = 1 << 4; // Overlapped Mode Service Request
        const DF   = 1 << 5; // Drive Fault Error
        const RDY  = 1 << 6; // Drive Ready
        const BSY  = 1 << 7; // Busy
    }
}

pub struct AtaDrive {
    data: Port<u16>,
    sector_count: Port<u8>,
    lba_lo: Port<u8>,
    lba_mid: Port<u8>,
    lba_hi: Port<u8>,
    drive: Port<u8>,
    status_cmd: Port<u8>,
}

impl AtaDrive {
    pub const fn new() -> Self {
        AtaDrive {
            data: Port::new(DATA_PORT),
            sector_count: Port::new(SECTOR_COUNT_PORT),
            lba_lo: Port::new(LBA_LO_PORT),
            lba_mid: Port::new(LBA_MID_PORT),
            lba_hi: Port::new(LBA_HI_PORT),
            drive: Port::new(DRIVE_PORT),
            status_cmd: Port::new(STATUS_COMMAND_PORT),
        }
    }

    fn wait_ready(&mut self) -> Result<(), &'static str> {
        for _ in 0..1_000_000 {
            let status = StatusFlags::from_bits_truncate(unsafe { self.status_cmd.read() });
            if !status.contains(StatusFlags::BSY) && status.contains(StatusFlags::RDY) {
                return Ok(());
            }
        }
        Err("ATA Timeout")
    }

    fn setup_lba28(
        &mut self,
        drive_idx: u8,
        lba: u32,
        sector_count: u8,
    ) -> Result<(), &'static str> {
        self.wait_ready()?;

        // 0xE0 for Master (Drive 0), 0xF0 for Slave (Drive 1)
        let drive_sel = if drive_idx == 0 { 0xE0 } else { 0xF0 };
        let lba_bits_24_27 = ((lba >> 24) & 0x0F) as u8;

        unsafe {
            self.drive.write(drive_sel | lba_bits_24_27);
            self.sector_count.write(sector_count);
            self.lba_lo.write((lba & 0xFF) as u8);
            self.lba_mid.write(((lba >> 8) & 0xFF) as u8);
            self.lba_hi.write(((lba >> 16) & 0xFF) as u8);
        }
        Ok(())
    }

    pub fn read_sector(
        &mut self,
        lba: u32,
        sector_buf: &mut [u8; 512],
    ) -> Result<(), &'static str> {
        // Мы читаем диск 1 (Slave), так как 0 (Master) это загрузочный образ OS
        self.setup_lba28(1, lba, 1)?;

        unsafe {
            self.status_cmd.write(0x20); // 0x20 = Read Sectors with Retry
        }

        self.wait_ready()?;
        let status = StatusFlags::from_bits_truncate(unsafe { self.status_cmd.read() });
        if status.contains(StatusFlags::ERR) || status.contains(StatusFlags::DF) {
            return Err("ATA Read Sector Error");
        }

        // Ждем пока диск скажет, что данные готовы (DRQ)
        for _ in 0..10_000 {
            let s = StatusFlags::from_bits_truncate(unsafe { self.status_cmd.read() });
            if s.contains(StatusFlags::DRQ) {
                break;
            }
        }

        unsafe {
            // Читаем 256 слов по 16 бит из порта данных
            let mut ptr = sector_buf.as_mut_ptr() as *mut u16;
            for _ in 0..256 {
                ptr.write_unaligned(self.data.read());
                ptr = ptr.add(1);
            }
        }

        Ok(())
    }

    pub fn write_sector(&mut self, lba: u32, sector_buf: &[u8; 512]) -> Result<(), &'static str> {
        self.setup_lba28(1, lba, 1)?; // 디スク 1 (Slave)

        unsafe {
            self.status_cmd.write(0x30); // 0x30 = Write Sectors
        }

        self.wait_ready()?;

        // Wait for Data Request
        for _ in 0..10_000 {
            let s = StatusFlags::from_bits_truncate(unsafe { self.status_cmd.read() });
            if s.contains(StatusFlags::DRQ) {
                break;
            }
        }

        unsafe {
            // Пишем 256 слов
            let mut ptr = sector_buf.as_ptr() as *const u16;
            for _ in 0..256 {
                self.data.write(ptr.read_unaligned());
                ptr = ptr.add(1);
            }

            // Flush Cache
            self.status_cmd.write(0xE7);
        }

        self.wait_ready()?;
        let status = StatusFlags::from_bits_truncate(unsafe { self.status_cmd.read() });
        if status.contains(StatusFlags::ERR) || status.contains(StatusFlags::DF) {
            return Err("ATA Write Sector Error");
        }

        Ok(())
    }
}

pub static ATA_DRIVE: Mutex<AtaDrive> = Mutex::new(AtaDrive::new());
