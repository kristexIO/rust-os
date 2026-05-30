// elf.rs — Минималистичный парсер ELF64 для загрузки программ Ring 3

use core::slice;

#[repr(C, packed)]
struct Elf64Ehdr {
    e_ident: [u8; 16],
    e_type: u16,
    e_machine: u16,
    e_version: u32,
    e_entry: u64,
    e_phoff: u64,
    e_shoff: u64,
    e_flags: u32,
    e_ehsize: u16,
    e_phentsize: u16,
    e_phnum: u16,
    e_shentsize: u16,
    e_shnum: u16,
    e_shstrndx: u16,
}

#[repr(C, packed)]
struct Elf64Phdr {
    p_type: u32,
    p_flags: u32,
    p_offset: u64,
    p_vaddr: u64,
    p_paddr: u64,
    p_filesz: u64,
    p_memsz: u64,
    p_align: u64,
}

const PT_LOAD: u32 = 1;

/// Парсит ELF файл из памяти, выделяет сегменты и загружает их (пока через
/// memcpy, так как у нас нет paging). Возвращает (entry_point, stack_top).
pub fn load_elf(elf_data: &[u8]) -> Result<u64, &'static str> {
    if elf_data.len() < core::mem::size_of::<Elf64Ehdr>() {
        return Err("File too small");
    }

    let ehdr = unsafe { &*(elf_data.as_ptr() as *const Elf64Ehdr) };

    // Проверяем Magic bytes: \x7F E L F
    if ehdr.e_ident[0..4] != [0x7F, b'E', b'L', b'F'] {
        return Err("Invalid ELF magic");
    }

    // Проверяем что это 64-бит (e_ident[4] == 2)
    if ehdr.e_ident[4] != 2 {
        return Err("Not a 64-bit ELF");
    }

    let phoff = ehdr.e_phoff as usize;
    let phnum = ehdr.e_phnum as usize;
    let phentsize = ehdr.e_phentsize as usize;

    for i in 0..phnum {
        let offset = phoff + i * phentsize;
        if offset + core::mem::size_of::<Elf64Phdr>() > elf_data.len() {
            return Err("Program header out of bounds");
        }

        let phdr = unsafe { &*(elf_data[offset..].as_ptr() as *const Elf64Phdr) };

        if phdr.p_type == PT_LOAD {
            let vaddr = phdr.p_vaddr as usize;
            let memsz = phdr.p_memsz as usize;
            let filesz = phdr.p_filesz as usize;
            let file_offset = phdr.p_offset as usize;

            if memsz == 0 {
                continue;
            }

            // КРИТИЧНО: В полноценной ОС здесь мы бы выделяли физические страницы,
            // искали свободные виртуальные адреса и маппили (mmap).
            // В нашем случае (identity mapping / bootloader map) мы просто
            // доверяем ELF-файлу и пишем прямо по указанному виртуальному адресу.
            // Пользовательский ELF должен быть скомпилирован с адресом загрузки
            // (например 0x400000), который не пересекается с ядром.
            // Защиты памяти пока нет.

            unsafe {
                let dest = slice::from_raw_parts_mut(vaddr as *mut u8, memsz);

                // 1. Копируем данные из файла
                if filesz > 0 {
                    let src = &elf_data[file_offset..file_offset + filesz];
                    dest[..filesz].copy_from_slice(src);
                }

                // 2. Очищаем BSS секцию (всё что memsz > filesz заполняем нулями)
                if memsz > filesz {
                    dest[filesz..].fill(0);
                }
            }
        }
    }

    Ok(ehdr.e_entry)
}
