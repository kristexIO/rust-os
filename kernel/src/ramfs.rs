use alloc::{
    collections::BTreeMap,
    string::{String, ToString},
    sync::Arc,
    vec::Vec,
};

use spin::RwLock;

use crate::vfs::{DirEntry, FileSystem, FileType, FsError};

/// Индексный дескриптор. Использует RwLock для безопасного конкурентного
/// доступа к содержимому. RwLock позволяет множеству читателей читать файл
/// одновременно без блокировки, но эксклюзивно блокирует при записи. Это
/// критично для высокой скорости в многопоточной ОС.
pub enum Inode {
    /// Файл хранится как непрерывный кусок памяти. `Vec<u8>` обеспечивает O(1)
    /// доступ по индексу.
    File(RwLock<Vec<u8>>),
    /// Директория: использует BTreeMap для O(log N) поиска детей и
    /// лексикографической сортировки.
    Directory(RwLock<BTreeMap<String, Arc<Inode>>>),
}

impl Inode {
    pub fn new_file() -> Arc<Self> {
        Arc::new(Inode::File(RwLock::new(Vec::new())))
    }

    pub fn new_dir() -> Arc<Self> {
        Arc::new(Inode::Directory(RwLock::new(BTreeMap::new())))
    }
}

/// Высокооптимизированная In-Memory файловая система.
pub struct RamFS {
    root: Arc<Inode>,
}

impl RamFS {
    pub fn new() -> Self {
        RamFS {
            root: Inode::new_dir(),
        }
    }

    /// Парсер путей. Разделяет путь по `/` и игнорирует пустые части (решает
    /// проблему двойных слешей).
    fn split_path(path: &str) -> Vec<&str> {
        path.split('/').filter(|s| !s.is_empty()).collect()
    }

    /// Находит Inode по абсолютному пути O(Depth * log(N_CHILDREN)).
    fn resolve(&self, path: &str) -> Result<Arc<Inode>, FsError> {
        let parts = Self::split_path(path);
        let mut current = Arc::clone(&self.root);

        for part in parts {
            let next = match &*current {
                Inode::Directory(dir) => dir.read().get(part).cloned(),
                Inode::File(_) => return Err(FsError::NotADirectory),
            };
            current = next.ok_or(FsError::NotFound)?;
        }
        Ok(current)
    }

    /// Находит родительский Inode директории и возвращает его вместе с именем
    /// искомого файла.
    fn resolve_parent(&self, path: &str) -> Result<(Arc<Inode>, String), FsError> {
        let mut parts = Self::split_path(path);
        if parts.is_empty() {
            return Err(FsError::InvalidPath);
        }
        let name = match parts.pop() {
            Some(n) => n.to_string(),
            None => return Err(FsError::InvalidPath), /* Теоретически невозможно, но на всякий
                                                       * случай */
        };

        let mut current = Arc::clone(&self.root);
        for part in parts {
            let next = match &*current {
                Inode::Directory(dir) => dir.read().get(part).cloned(),
                Inode::File(_) => return Err(FsError::NotADirectory),
            };
            current = next.ok_or(FsError::NotFound)?;
        }
        Ok((current, name))
    }
}

impl FileSystem for RamFS {
    fn create_file(&self, path: &str) -> Result<(), FsError> {
        let (parent, name) = self.resolve_parent(path)?;
        if let Inode::Directory(dir) = &*parent {
            let mut writer = dir.write(); // Эксклюзивная блокировка директории на запись
            if writer.contains_key(&name) {
                return Err(FsError::AlreadyExists);
            }
            writer.insert(name, Inode::new_file());
            Ok(())
        } else {
            Err(FsError::NotADirectory)
        }
    }

    fn mkdir(&self, path: &str) -> Result<(), FsError> {
        let (parent, name) = self.resolve_parent(path)?;
        if let Inode::Directory(dir) = &*parent {
            let mut writer = dir.write();
            if writer.contains_key(&name) {
                return Err(FsError::AlreadyExists);
            }
            writer.insert(name, Inode::new_dir());
            Ok(())
        } else {
            Err(FsError::NotADirectory)
        }
    }

    fn read_file(&self, path: &str) -> Result<Vec<u8>, FsError> {
        let node = self.resolve(path)?;
        if let Inode::File(file) = &*node {
            // Быстрое не-монопольное чтение через RwLock (позволяет параллельное чтение из
            // других мест)
            let reader = file.read();
            // Возвращаем копию данных (передача больших блоков за O(N) через memcpy)
            Ok(reader.clone())
        } else {
            Err(FsError::NotAFile)
        }
    }

    fn write_file(&self, path: &str, data: &[u8]) -> Result<(), FsError> {
        let node = match self.resolve(path) {
            Ok(n) => n,
            Err(FsError::NotFound) => {
                // Если нет файла, создаем
                self.create_file(path)?;
                self.resolve(path)?
            },
            Err(e) => return Err(e),
        };

        if let Inode::File(file) = &*node {
            let mut writer = file.write(); // Эксклюзивный лок
            writer.clear();
            writer.extend_from_slice(data); // O(N) оптимизированное копирование
            Ok(())
        } else {
            Err(FsError::NotAFile)
        }
    }

    fn read_dir(&self, path: &str) -> Result<Vec<DirEntry>, FsError> {
        let node = if Self::split_path(path).is_empty() {
            Arc::clone(&self.root)
        } else {
            self.resolve(path)?
        };

        if let Inode::Directory(dir) = &*node {
            let reader = dir.read();
            let mut entries = Vec::with_capacity(reader.len());
            for (name, child) in reader.iter() {
                let (file_type, size) = match &**child {
                    Inode::File(f) => (FileType::RegularFile, f.read().len()),
                    Inode::Directory(d) => (FileType::Directory, d.read().len()),
                };
                entries.push(DirEntry {
                    name: name.clone(),
                    file_type,
                    size,
                });
            }
            Ok(entries)
        } else {
            Err(FsError::NotAFile)
        }
    }

    fn remove(&self, path: &str) -> Result<(), FsError> {
        let (parent, name) = self.resolve_parent(path)?;
        if let Inode::Directory(dir) = &*parent {
            let mut writer = dir.write();
            if writer.remove(&name).is_some() {
                Ok(())
            } else {
                Err(FsError::NotFound)
            }
        } else {
            Err(FsError::NotADirectory)
        }
    }
}

pub fn create_default_ramfs() -> RamFS {
    let fs = RamFS::new();
    // Создаем базовую файловую иерархию при старте
    let _ = fs.mkdir("/home");
    let _ = fs.mkdir("/etc");
    let _ = fs.mkdir("/mnt");
    let _ = fs.create_file("/etc/motd");
    let _ = fs.write_file("/etc/motd", b"Welcome to High-Performance Rust OS!");
    fs
}
