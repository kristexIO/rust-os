use lazy_static::lazy_static;
use x86_64::{
    structures::{
        gdt::{Descriptor, GlobalDescriptorTable, SegmentSelector},
        tss::TaskStateSegment,
    },
    VirtAddr,
};

pub const DOUBLE_FAULT_IST_INDEX: u16 = 0;

/// Стек для перехода Ring3 → Ring0 при системных вызовах (syscall).
/// 8KB — достаточно для обработчика syscall.
const KERNEL_STACK_SIZE: usize = 4096 * 2;
#[repr(align(16))]
struct KernelStack([u8; KERNEL_STACK_SIZE]);
static mut KERNEL_STACK: KernelStack = KernelStack([0; KERNEL_STACK_SIZE]);

pub fn syscall_stack_top() -> VirtAddr {
    let stack_start = VirtAddr::from_ptr(core::ptr::addr_of!(KERNEL_STACK));
    stack_start + KERNEL_STACK_SIZE as u64
}

lazy_static! {
    static ref TSS: TaskStateSegment = {
        let mut tss = TaskStateSegment::new();

        // IST[0]: стек для обработчика Double Fault
        tss.interrupt_stack_table[DOUBLE_FAULT_IST_INDEX as usize] = {
            const STACK_SIZE: usize = 4096 * 5;
            static mut STACK: [u8; STACK_SIZE] = [0; STACK_SIZE];
            let stack_start = VirtAddr::from_ptr(core::ptr::addr_of!(STACK));
            stack_start + STACK_SIZE as u64
        };

        // RSP[0]: стек ядра при переходе Ring3 → Ring0 через syscall/interrupt
        tss.privilege_stack_table[0] = syscall_stack_top();

        tss
    };
}

/// Хранит все селекторы GDT.
pub struct Selectors {
    pub kernel_code: SegmentSelector,
    pub kernel_data: SegmentSelector,
    pub user_code: SegmentSelector,
    pub user_data: SegmentSelector,
    pub tss_selector: SegmentSelector,
}

lazy_static! {
    pub static ref GDT: (GlobalDescriptorTable, Selectors) = {
        let mut gdt = GlobalDescriptorTable::new();
        // Порядок важен для STAR MSR:
        // STAR[47:32] = kernel code selector  → kernel CS при SYSCALL
        // STAR[63:48] = user code selector - 8 → user CS при SYSRET
        let kernel_code  = gdt.add_entry(Descriptor::kernel_code_segment());
        let kernel_data  = gdt.add_entry(Descriptor::kernel_data_segment());
        let user_data    = gdt.add_entry(Descriptor::user_data_segment());
        let user_code    = gdt.add_entry(Descriptor::user_code_segment());
        let tss_selector = gdt.add_entry(Descriptor::tss_segment(&TSS));
        (gdt, Selectors { kernel_code, kernel_data, user_code, user_data, tss_selector })
    };
}

pub fn init() {
    use x86_64::instructions::{
        segmentation::{Segment, CS, DS, ES, FS, GS, SS},
        tables::load_tss,
    };

    GDT.0.load();
    unsafe {
        CS::set_reg(GDT.1.kernel_code);
        load_tss(GDT.1.tss_selector);

        let null_sel =
            x86_64::structures::gdt::SegmentSelector::new(0, x86_64::PrivilegeLevel::Ring0);
        SS::set_reg(null_sel);
        DS::set_reg(null_sel);
        ES::set_reg(null_sel);
        FS::set_reg(null_sel);
        GS::set_reg(null_sel);
    }
}
