/// Реализация интерфеса для крейта fatfs, чтобы он мог управлять ATA диском
pub struct BlockDevice {
    current_offset: u64,
    disk_size: u64,
}

impl BlockDevice {
    pub const fn new() -> Self {
        BlockDevice {
            current_offset: 0,
            disk_size: 10 * 1024 * 1024, // 10MB size set in the QEMU image
        }
    }
}

#[derive(Debug)]
pub enum BlockError {
    Ata(&'static str),
}

// impl fatfs::IoError for BlockError {
// fn is_interrupted(&self) -> bool {
// false
// }
// fn new_unexpected_eof_error() -> Self {
// BlockError::Ata("EOF")
// }
// fn new_write_zero_error() -> Self {
// BlockError::Ata("Write zero")
// }
// }
// impl fatfs::IoBase for BlockDevice {
// type Error = BlockError;
// }

// impl fatfs::Read for BlockDevice {
// fn read(&mut self, buf: &mut [u8]) -> Result<usize, Self::Error> {
// if buf.is_empty() {
// return Ok(0);
// }
//
// let mut total_read = 0;
// let mut curr_offset = self.current_offset;
//
// while total_read < buf.len() && curr_offset < self.disk_size {
// let sector = (curr_offset / 512) as u32;
// let offset_in_sec = (curr_offset % 512) as usize;
//
// let mut sector_buf = [0u8; 512];
// ATA_DRIVE
// .lock()
// .read_sector(sector, &mut sector_buf)
// .map_err(BlockError::Ata)?;
//
// let amount = core::cmp::min(512 - offset_in_sec, buf.len() - total_read);
// let available_on_disk = (self.disk_size - curr_offset) as usize;
// let amount = core::cmp::min(amount, available_on_disk);
// if amount == 0 {
// break;
// }
//
// buf[total_read..total_read + amount]
// .copy_from_slice(&sector_buf[offset_in_sec..offset_in_sec + amount]);
//
// total_read += amount;
// curr_offset += amount as u64;
// }
//
// self.current_offset = curr_offset;
// Ok(total_read)
// }
// }

// impl fatfs::Write for BlockDevice {
// fn write(&mut self, buf: &[u8]) -> Result<usize, Self::Error> {
// if buf.is_empty() {
// return Ok(0);
// }
//
// let mut total_writ = 0;
// let mut curr_offset = self.current_offset;
//
// while total_writ < buf.len() && curr_offset < self.disk_size {
// let sector = (curr_offset / 512) as u32;
// let offset_in_sec = (curr_offset % 512) as usize;
// let amount = core::cmp::min(512 - offset_in_sec, buf.len() - total_writ);
// let available_on_disk = (self.disk_size - curr_offset) as usize;
// let amount = core::cmp::min(amount, available_on_disk);
// if amount == 0 {
// break;
// }
//
// let mut sector_buf = [0u8; 512];
// Частичная запись требует прочтения старого сектора (Read-Modify-Write)
// if amount < 512 {
// ATA_DRIVE
// .lock()
// .read_sector(sector, &mut sector_buf)
// .map_err(BlockError::Ata)?;
// }
//
// sector_buf[offset_in_sec..offset_in_sec + amount]
// .copy_from_slice(&buf[total_writ..total_writ + amount]);
// ATA_DRIVE
// .lock()
// .write_sector(sector, &sector_buf)
// .map_err(BlockError::Ata)?;
//
// total_writ += amount;
// curr_offset += amount as u64;
// }
//
// self.current_offset = curr_offset;
// Ok(total_writ)
// }
//
// fn flush(&mut self) -> Result<(), Self::Error> {
// Мы вызываем 0xE7 flush_cache при каждой записи напрямую в ATA драйвере
// Ok(())
// }
// }

// impl fatfs::Seek for BlockDevice {
// fn seek(&mut self, pos: fatfs::SeekFrom) -> Result<u64, Self::Error> {
// match pos {
// fatfs::SeekFrom::Start(bytes) => {
// self.current_offset = bytes;
// },
// fatfs::SeekFrom::End(bytes) => {
// let new_offset = self.disk_size as i64 + bytes;
// if new_offset >= 0 {
// self.current_offset = new_offset as u64;
// }
// },
// fatfs::SeekFrom::Current(bytes) => {
// let new_offset = self.current_offset as i64 + bytes;
// if new_offset >= 0 {
// self.current_offset = new_offset as u64;
// }
// },
// }
// Ok(self.current_offset)
// }
// }
