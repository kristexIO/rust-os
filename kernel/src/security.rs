use alloc::{
    collections::BTreeMap,
    string::{String, ToString},
};

use crate::process::Pid;

/// Права доступа к файлам и ресурсам
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Permissions {
    pub read: bool,
    pub write: bool,
    pub execute: bool,
}

impl Permissions {
    pub const fn none() -> Self {
        Permissions {
            read: false,
            write: false,
            execute: false,
        }
    }

    pub const fn all() -> Self {
        Permissions {
            read: true,
            write: true,
            execute: true,
        }
    }

    pub const fn read_only() -> Self {
        Permissions {
            read: true,
            write: false,
            execute: false,
        }
    }

    pub const fn read_write() -> Self {
        Permissions {
            read: true,
            write: true,
            execute: false,
        }
    }

    pub const fn write_only() -> Self {
        Permissions {
            read: false,
            write: true,
            execute: false,
        }
    }

    pub const fn execute_only() -> Self {
        Permissions {
            read: false,
            write: false,
            execute: true,
        }
    }
}

/// Типы владельцев ресурсов
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum OwnerType {
    User(Pid),  // PID процесса
    Group(u32), // ID группы
    System,     // Системные ресурсы
    Anonymous,  // Гостевые права
}

/// Информация о владельце ресурса
#[derive(Debug, Clone)]
pub struct ResourceOwner {
    pub owner_type: OwnerType,
    pub owner_id: u32, // Может быть PID или GroupID
}

/// Типы ресурсов в системе
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum ResourceType {
    File(String),        // Файловая система
    MemoryRegion(usize), // Область памяти
    NetworkSocket(u16),  // Сетевой сокет
    Device(String),      // Устройство
    Process(Pid),        // Процесс
}

/// Запись ACL (Access Control List)
#[derive(Debug, Clone)]
pub struct AclEntry {
    pub principal: OwnerType,
    pub permissions: Permissions,
}

/// Менеджер безопасности для конкретного ресурса
#[derive(Debug)]
pub struct SecurityDescriptor {
    pub owner: ResourceOwner,
    pub group: Option<u32>,               // Группа владельца
    pub acl: alloc::vec::Vec<AclEntry>,   // Список контроля доступа
    pub default_permissions: Permissions, // Права по умолчанию для новых ресурсов
}

impl SecurityDescriptor {
    pub fn new(owner_type: OwnerType, owner_id: u32) -> Self {
        SecurityDescriptor {
            owner: ResourceOwner {
                owner_type,
                owner_id,
            },
            group: None,
            acl: alloc::vec::Vec::new(),
            default_permissions: Permissions::none(),
        }
    }

    /// Добавляет запись в ACL
    pub fn add_permission(&mut self, principal: OwnerType, perms: Permissions) {
        // Проверяем, есть ли уже запись для этого субъекта
        if let Some(entry) = self.acl.iter_mut().find(|e| e.principal == principal) {
            // Обновляем права
            entry.permissions = perms;
        } else {
            // Добавляем новую запись
            self.acl.push(AclEntry {
                principal,
                permissions: perms,
            });
        }
    }

    /// Проверяет права доступа
    pub fn check_access(&self, requester: &OwnerType, perm_type: fn(&Permissions) -> bool) -> bool {
        // Владельцу также нужны явно заданные права: owner не является bypass ACL.
        for entry in &self.acl {
            if entry.principal == *requester {
                return perm_type(&entry.permissions);
            }
        }

        // 3. Проверяем группу (если применимо)
        // TODO: Реализовать проверку по группе
        if let OwnerType::User(requester_pid) = requester {
            if let Some(group_id) = self.group {
                // Проверяем, состоит ли пользователь в группе
                if self.user_in_group(*requester_pid, group_id) {
                    // Проверяем групповые права
                    for entry in &self.acl {
                        if let OwnerType::Group(gid) = entry.principal {
                            if gid == group_id {
                                return perm_type(&entry.permissions);
                            }
                        }
                    }
                }
            }
        }

        // 4. Права по умолчанию для остальных (обычно запрещены)
        false
    }

    /// Проверяет, состоит ли пользователь в группе (заглушка)
    fn user_in_group(&self, _user_pid: Pid, _group_id: u32) -> bool {
        // В реальной системе тут была бы проверка Memberships
        // Пока всегда возвращаем false
        false
    }

    /// Проверяет право на чтение
    pub fn can_read(&self, requester: &OwnerType) -> bool {
        self.check_access(requester, |perm| perm.read)
    }

    /// Проверяет право на запись
    pub fn can_write(&self, requester: &OwnerType) -> bool {
        self.check_access(requester, |perm| perm.write)
    }

    /// Проверяет право на выполнение
    pub fn can_execute(&self, requester: &OwnerType) -> bool {
        self.check_access(requester, |perm| perm.execute)
    }
}

/// Менеджер безопасности системы
pub struct SecurityManager {
    resources: BTreeMap<ResourceType, SecurityDescriptor>,
}

impl SecurityManager {
    pub fn new() -> Self {
        SecurityManager {
            resources: BTreeMap::new(),
        }
    }

    /// Регистрирует новый ресурс с безопасностью
    pub fn register_resource(
        &mut self,
        resource: ResourceType,
        descriptor: SecurityDescriptor,
    ) -> bool {
        self.resources.insert(resource, descriptor).is_none()
    }

    /// Проверяет доступ к ресурсу
    pub fn check_resource_access(
        &self,
        resource: &ResourceType,
        requester: &OwnerType,
        perm_type: fn(&Permissions) -> bool,
    ) -> bool {
        if let Some(descriptor) = self.resources.get(resource) {
            descriptor.check_access(requester, perm_type)
        } else {
            // Неизвестный ресурс - запрещаем доступ
            false
        }
    }

    /// Получает дескриптор безопасности для ресурса
    pub fn get_security_descriptor(&self, resource: &ResourceType) -> Option<&SecurityDescriptor> {
        self.resources.get(resource)
    }

    /// Обновляет права доступа к ресурсу
    pub fn update_permissions(
        &mut self,
        resource: &ResourceType,
        principal: OwnerType,
        perms: Permissions,
    ) -> bool {
        if let Some(desc) = self.resources.get_mut(resource) {
            desc.add_permission(principal, perms);
            true
        } else {
            false
        }
    }

    /// Проверяет права пользователя на файл
    pub fn check_file_access(
        &self,
        filepath: &str,
        requester: &OwnerType,
        perms: &Permissions,
    ) -> bool {
        let resource = ResourceType::File(filepath.to_string());

        (!perms.read || self.check_resource_access(&resource, requester, |p| p.read))
            && (!perms.write || self.check_resource_access(&resource, requester, |p| p.write))
            && (!perms.execute || self.check_resource_access(&resource, requester, |p| p.execute))
    }
}

/// Глобальный менеджер безопасности
pub static SECURITY_MANAGER: spin::Lazy<spin::Mutex<SecurityManager>> =
    spin::Lazy::new(|| spin::Mutex::new(SecurityManager::new()));

// Вспомогательные функции

/// Проверяет, имеет ли процесс права на файл
pub fn process_has_file_access(pid: Pid, filepath: &str, perms: &Permissions) -> bool {
    // PID 0 represents kernel initiated operations, including mounted filesystems.
    if pid == 0 {
        return true;
    }
    let owner_type = OwnerType::User(pid);
    SECURITY_MANAGER
        .lock()
        .check_file_access(filepath, &owner_type, perms)
}

/// Регистрирует файл с правами доступа
pub fn register_file_security(filepath: &str, owner_pid: Pid, permissions: Permissions) -> bool {
    let resource = ResourceType::File(filepath.to_string());
    let mut descriptor = SecurityDescriptor::new(OwnerType::User(owner_pid), owner_pid);
    descriptor.add_permission(OwnerType::User(owner_pid), permissions);

    SECURITY_MANAGER
        .lock()
        .register_resource(resource, descriptor)
}

/// Обновляет права доступа к файлу
pub fn update_file_permissions(filepath: &str, pid: Pid, perms: Permissions) -> bool {
    let owner_type = OwnerType::User(pid);
    let resource = ResourceType::File(filepath.to_string());

    SECURITY_MANAGER
        .lock()
        .update_permissions(&resource, owner_type, perms)
}

/// Проверяет, может ли процесс читать файл
pub fn process_can_read_file(pid: Pid, filepath: &str) -> bool {
    process_has_file_access(pid, filepath, &Permissions::read_only())
}

/// Проверяет, может ли процесс записывать файл
pub fn process_can_write_file(pid: Pid, filepath: &str) -> bool {
    process_has_file_access(pid, filepath, &Permissions::write_only())
}

/// Проверяет, может ли процесс выполнять файл
pub fn process_can_execute_file(pid: Pid, filepath: &str) -> bool {
    process_has_file_access(pid, filepath, &Permissions::execute_only())
}

/// Регистрирует ресурс памяти с правами доступа
pub fn register_memory_security(start_addr: usize, owner_pid: Pid) -> bool {
    let resource = ResourceType::MemoryRegion(start_addr);
    let permissions = Permissions::read_write();
    let mut descriptor = SecurityDescriptor::new(OwnerType::User(owner_pid), owner_pid);
    descriptor.add_permission(OwnerType::User(owner_pid), permissions);

    SECURITY_MANAGER
        .lock()
        .register_resource(resource, descriptor)
}

/// Проверяет, может ли процесс получить доступ к области памяти
pub fn process_can_access_memory(pid: Pid, mem_addr: usize) -> bool {
    let resource = ResourceType::MemoryRegion(mem_addr);
    let owner_type = OwnerType::User(pid);

    SECURITY_MANAGER
        .lock()
        .check_resource_access(&resource, &owner_type, |_| true)
}
