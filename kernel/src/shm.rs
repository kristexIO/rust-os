use alloc::{collections::BTreeMap, sync::Arc, vec::Vec};

use spin::Mutex;
use x86_64::structures::paging::PhysFrame;

use crate::frame_allocator;

/// Уникальный идентификатор сегмента разделяемой памяти
pub type ShmId = u32;

/// Сегмент разделяемой памяти
pub struct SharedRegion {
    pub id: ShmId,
    pub frames: Vec<PhysFrame>,
    pub size_pages: usize,
}

impl SharedRegion {
    pub fn new(id: ShmId, size_pages: usize) -> Option<Self> {
        let mut frames = Vec::with_capacity(size_pages);
        for _ in 0..size_pages {
            if let Some(frame) = frame_allocator::try_alloc_frame() {
                frames.push(frame);
            } else {
                // Если не хватило памяти, освобождаем уже выделенное
                for f in frames {
                    frame_allocator::free_frame(f);
                }
                return None;
            }
        }
        Some(SharedRegion {
            id,
            frames,
            size_pages,
        })
    }
}

pub struct ShmManager {
    regions: BTreeMap<ShmId, Arc<SharedRegion>>,
}

impl ShmManager {
    pub const fn new() -> Self {
        ShmManager {
            regions: BTreeMap::new(),
        }
    }

    /// Получает существующий или создает новый сегмент SHM
    pub fn get_or_create(&mut self, id: ShmId, size_pages: usize) -> Option<Arc<SharedRegion>> {
        if let Some(region) = self.regions.get(&id) {
            return Some(region.clone());
        }

        let region = SharedRegion::new(id, size_pages)?;
        let arc = Arc::new(region);
        self.regions.insert(id, arc.clone());
        Some(arc)
    }

    /// Удаляет сегмент (если ссылок больше нет)
    pub fn remove(&mut self, id: ShmId) {
        self.regions.remove(&id);
    }
}

pub static SHM_MANAGER: Mutex<ShmManager> = Mutex::new(ShmManager::new());

/// Системный вызов: получить ID сегмента SHM
pub fn shm_get(id: ShmId, size_pages: usize) -> Option<ShmId> {
    let mut mgr = SHM_MANAGER.lock();
    mgr.get_or_create(id, size_pages).map(|r| r.id)
}

/// Системный вызов: примаппить SHM в адресное пространство текущего процесса
/// (В реальности это должно работать через VMA и PageManager)
pub fn map_shm_to_current(id: ShmId, virt_addr: x86_64::VirtAddr) -> Result<(), &'static str> {
    let region = {
        let mgr = SHM_MANAGER.lock();
        mgr.regions.get(&id).cloned().ok_or("SHM ID not found")?
    };

    unsafe {
        let mut addr = virt_addr;
        for frame in &region.frames {
            let flags = x86_64::structures::paging::PageTableFlags::PRESENT
                | x86_64::structures::paging::PageTableFlags::WRITABLE
                | x86_64::structures::paging::PageTableFlags::USER_ACCESSIBLE;

            crate::paging::map_page(addr, *frame, flags)?;
            addr += 4096u64;
        }
    }

    Ok(())
}
