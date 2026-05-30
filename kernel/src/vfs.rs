use alloc::{
    string::{String, ToString},
    sync::Arc,
    vec::Vec,
};

use spin::{Lazy, Mutex};

use crate::{process::Pid, security::Permissions};

/// Типы ошибок VFS.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FsError {
    NotFound,
    AlreadyExists,
    NotADirectory,
    NotAFile,
    InvalidPath,
    NotSupported,
    PermissionDenied,
}

/// Тип узла в ФС.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FileType {
    RegularFile,
    Directory,
}

/// Информация о файле/директории внутри родительской директории.
#[derive(Debug, Clone)]
pub struct DirEntry {
    pub name: String,
    pub file_type: FileType,
    pub size: usize,
}

/// Трейт виртуальной файловой системы.
pub trait FileSystem: Send + Sync {
    fn create_file(&self, path: &str) -> Result<(), FsError>;
    fn mkdir(&self, path: &str) -> Result<(), FsError>;
    fn read_file(&self, path: &str) -> Result<Vec<u8>, FsError>;
    fn write_file(&self, path: &str, data: &[u8]) -> Result<(), FsError>;
    fn read_dir(&self, path: &str) -> Result<Vec<DirEntry>, FsError>;
    fn remove(&self, path: &str) -> Result<(), FsError>;
}

/// Точка монтирования
struct MountPoint {
    /// Абсолютный префикс, например "/" или "/mnt"
    prefix: String,
    fs: Arc<dyn FileSystem>,
}

/// Объединенная VFS система
pub struct VirtualFileSystem {
    mount_points: Vec<MountPoint>,
    ramfs: Arc<crate::ramfs::RamFS>,
}

impl VirtualFileSystem {
    pub fn new() -> Self {
        let ramfs = Arc::new(crate::ramfs::create_default_ramfs());
        for path in ["/", "/home", "/etc", "/mnt", "/etc/motd"] {
            crate::security::register_file_security(path, 0, Permissions::all());
        }

        VirtualFileSystem {
            mount_points: Vec::new(),
            ramfs,
        }
    }

    /// Монтирует файловую систему
    pub fn mount(&mut self, prefix: String, fs: Arc<dyn FileSystem>) {
        self.mount_points.push(MountPoint {
            prefix,
            fs,
        });
    }

    /// Создает файл с указанным владельцем
    pub fn create_file_as(&self, path: &str, creator_pid: Pid) -> Result<(), FsError> {
        // Проверяем права на запись в родительскую директорию
        if let Some(parent_path) = self.get_parent_path(path) {
            if !crate::security::process_can_write_file(creator_pid, &parent_path) {
                return Err(FsError::PermissionDenied);
            }
        }

        // Создаем файл в ramfs (теперь с реальными данными)
        self.ramfs.create_file(path)?;

        // Регистрируем в системе безопасности
        crate::security::register_file_security(path, creator_pid, Permissions::read_write());
        Ok(())
    }

    /// Создает файл (по умолчанию для PID 0)
    pub fn create_file(&self, path: &str) -> Result<(), FsError> {
        self.create_file_as(path, 0) // PID 0 для системных операций
    }

    /// Создает директорию с указанным владельцем
    pub fn mkdir_as(&self, path: &str, creator_pid: Pid) -> Result<(), FsError> {
        // Проверяем права на запись в родительскую директорию
        if let Some(parent_path) = self.get_parent_path(path) {
            if !crate::security::process_can_write_file(creator_pid, &parent_path) {
                return Err(FsError::PermissionDenied);
            }
        }

        // Создаем директорию в ramfs
        self.ramfs.mkdir(path)?;

        // Регистрируем в системе безопасности
        crate::security::register_file_security(path, creator_pid, Permissions::read_write());
        Ok(())
    }

    /// Создает директорию (по умолчанию для PID 0)
    pub fn mkdir(&self, path: &str) -> Result<(), FsError> {
        self.mkdir_as(path, 0) // PID 0 для системных операций
    }

    /// Читает файл для указанного процесса
    pub fn read_file_as(&self, path: &str, requester_pid: Pid) -> Result<Vec<u8>, FsError> {
        // Проверяем права на чтение через глобальный менеджер безопасности
        if !crate::security::process_can_read_file(requester_pid, path) {
            return Err(FsError::PermissionDenied);
        }

        // Пытаемся прочитать из ramfs
        match self.ramfs.read_file(path) {
            Ok(data) => Ok(data),
            Err(FsError::NotFound) => {
                // Если не в ramfs, ищем по точкам монтирования
                self.find_mount_point_and_do(path, |fs, rel_path| fs.read_file(rel_path))
            },
            Err(e) => Err(e),
        }
    }

    /// Читает файл (по умолчанию для PID 0)
    pub fn read_file(&self, path: &str) -> Result<Vec<u8>, FsError> {
        self.read_file_as(path, 0)
    }

    /// Записывает файл для указанного процесса
    pub fn write_file_as(
        &self,
        path: &str,
        data: &[u8],
        requester_pid: Pid,
    ) -> Result<(), FsError> {
        // Проверяем права на запись
        if !crate::security::process_can_write_file(requester_pid, path) {
            return Err(FsError::PermissionDenied);
        }

        match self.ramfs.write_file(path, data) {
            Ok(()) => Ok(()),
            Err(FsError::NotFound) => {
                self.find_mount_point_and_do(path, |fs, rel_path| fs.write_file(rel_path, data))
            },
            Err(e) => Err(e),
        }
    }

    /// Записывает файл (по умолчанию для PID 0)
    pub fn write_file(&self, path: &str, data: &[u8]) -> Result<(), FsError> {
        self.write_file_as(path, data, 0)
    }

    /// Читает директорию для указанного процесса
    pub fn read_dir_as(&self, path: &str, requester_pid: Pid) -> Result<Vec<DirEntry>, FsError> {
        // Проверяем права на чтение директории
        if !crate::security::process_can_read_file(requester_pid, path) {
            return Err(FsError::PermissionDenied);
        }

        match self.ramfs.read_dir(path) {
            Ok(entries) => Ok(entries),
            Err(FsError::NotFound) => {
                self.find_mount_point_and_do(path, |fs, rel_path| fs.read_dir(rel_path))
            },
            Err(e) => Err(e),
        }
    }

    /// Читает директорию (по умолчанию для PID 0)
    pub fn read_dir(&self, path: &str) -> Result<Vec<DirEntry>, FsError> {
        self.read_dir_as(path, 0)
    }

    /// Удаляет файл для указанного процесса
    pub fn remove_as(&self, path: &str, requester_pid: Pid) -> Result<(), FsError> {
        // Проверяем права на удаление (требует прав на запись)
        if !crate::security::process_can_write_file(requester_pid, path) {
            return Err(FsError::PermissionDenied);
        }

        match self.ramfs.remove(path) {
            Ok(()) => Ok(()),
            Err(FsError::NotFound) => {
                self.find_mount_point_and_do(path, |fs, rel_path| fs.remove(rel_path))
            },
            Err(e) => Err(e),
        }
    }

    /// Удаляет файл (по умолчанию для PID 0)
    pub fn remove(&self, path: &str) -> Result<(), FsError> {
        self.remove_as(path, 0)
    }

    /// Размонтирует файловую систему (заглушка)
    pub fn umount(&mut self, _prefix: &str) {
        // В реальной системе это удаляет точку монтирования
        // Пока заглушка
    }

    /// Проверяет существование пути в RAMFS
    fn ramfs_path_exists(&self, path: &str) -> bool {
        self.ramfs.read_file(path).is_ok() || self.ramfs.read_dir(path).is_ok()
    }

    /// Находит точку монтирования и выполняет операцию
    fn find_mount_point_and_do<T>(
        &self,
        path: &str,
        operation: impl Fn(&Arc<dyn FileSystem>, &str) -> Result<T, FsError>,
    ) -> Result<T, FsError> {
        // Находим наиболее подходящую точку монтирования
        let mut best_match: Option<(&str, &Arc<dyn FileSystem>)> = None;
        let mut longest_prefix_len = 0usize;

        for mount_point in &self.mount_points {
            let matches_prefix = mount_point.prefix == "/"
                || path == mount_point.prefix
                || path
                    .strip_prefix(&mount_point.prefix)
                    .is_some_and(|suffix| suffix.starts_with('/'));
            if matches_prefix && mount_point.prefix.len() > longest_prefix_len {
                longest_prefix_len = mount_point.prefix.len();
                best_match = Some((&mount_point.prefix, &mount_point.fs));
            }
        }

        let (_, fs) = best_match.ok_or(FsError::NotFound)?;

        // Вычисляем относительный путь
        let rel_path = if longest_prefix_len > 1 {
            &path[longest_prefix_len..] // Относительный путь
        } else {
            path // Корневой путь
        };

        operation(fs, rel_path)
    }

    /// Получает родительский путь
    fn get_parent_path(&self, path: &str) -> Option<String> {
        let path_str = path.strip_suffix('/').unwrap_or(path);
        if let Some(last_slash) = path_str.rfind('/') {
            if last_slash > 0 {
                Some(path_str[..last_slash].to_string())
            } else {
                Some("/".to_string())
            }
        } else {
            None
        }
    }
}

/// Глобальная VFS
pub static GLOBAL_VFS: Lazy<Mutex<VirtualFileSystem>> =
    Lazy::new(|| Mutex::new(VirtualFileSystem::new()));
