// FatFS Wrapper — реализует трейт FileSystem поверх fatfs + нашего BlockDevice.
// Статически открываем/закрываем файловую систему при каждой операции.

use alloc::{string::String, vec::Vec};

use crate::{
    disk::BlockDevice,
    vfs::{DirEntry, FileSystem, FileType, FsError},
};

pub struct FatFsWrapper;

impl FatFsWrapper {
    pub fn new() -> Self {
        FatFsWrapper
    }
}

unsafe impl Send for FatFsWrapper {}
unsafe impl Sync for FatFsWrapper {}

/// Открывает FAT32 ФС; проверяет что диск отформатирован
fn open_fat() -> Result<fatfs::FileSystem<BlockDevice>, FsError> {
    fatfs::FileSystem::new(BlockDevice::new(), fatfs::FsOptions::new())
        .map_err(|_| FsError::InvalidPath)
}

fn strip_slash(path: &str) -> &str {
    path.trim_start_matches('/')
}

impl FileSystem for FatFsWrapper {
    fn create_file(&self, path: &str) -> Result<(), FsError> {
        let fs = open_fat()?;
        let root = fs.root_dir();
        root.create_file(strip_slash(path))
            .map(|_| ())
            .map_err(|_| FsError::AlreadyExists)
    }

    fn mkdir(&self, path: &str) -> Result<(), FsError> {
        let fs = open_fat()?;
        let root = fs.root_dir();
        root.create_dir(strip_slash(path))
            .map(|_| ())
            .map_err(|_| FsError::AlreadyExists)
    }

    fn read_file(&self, path: &str) -> Result<Vec<u8>, FsError> {
        let fs = open_fat()?;
        let root = fs.root_dir();
        let mut file = root
            .open_file(strip_slash(path))
            .map_err(|_| FsError::NotFound)?;

        let mut data: Vec<u8> = Vec::new();
        let mut buf = [0u8; 512];
        loop {
            let n = fatfs::Read::read(&mut file, &mut buf).map_err(|_| FsError::NotFound)?;
            if n == 0 {
                break;
            }
            data.extend_from_slice(&buf[..n]);
        }
        Ok(data)
    }

    fn write_file(&self, path: &str, data: &[u8]) -> Result<(), FsError> {
        let fs = open_fat()?;
        let root = fs.root_dir();
        let p = strip_slash(path);
        let mut file = root
            .open_file(p)
            .or_else(|_| root.create_file(p))
            .map_err(|_| FsError::NotFound)?;
        file.truncate().map_err(|_| FsError::NotFound)?;

        let mut written = 0;
        while written < data.len() {
            let n =
                fatfs::Write::write(&mut file, &data[written..]).map_err(|_| FsError::NotFound)?;
            if n == 0 {
                break;
            }
            written += n;
        }
        Ok(())
    }

    fn read_dir(&self, path: &str) -> Result<Vec<DirEntry>, FsError> {
        let fs = open_fat()?;
        let root = fs.root_dir();

        let p = strip_slash(path);
        let dir = if p.is_empty() {
            root
        } else {
            root.open_dir(p).map_err(|_| FsError::NotFound)?
        };

        let mut entries = Vec::new();
        for entry in dir.iter() {
            let entry = entry.map_err(|_| FsError::NotFound)?;
            let name: String = entry.file_name().chars().collect();
            if name == "." || name == ".." {
                continue;
            }
            let file_type = if entry.is_dir() {
                FileType::Directory
            } else {
                FileType::RegularFile
            };
            let size = entry.len() as usize;
            entries.push(DirEntry {
                name,
                file_type,
                size,
            });
        }
        Ok(entries)
    }

    fn remove(&self, path: &str) -> Result<(), FsError> {
        let fs = open_fat()?;
        let root = fs.root_dir();
        root.remove(strip_slash(path))
            .map_err(|_| FsError::NotFound)
    }
}
