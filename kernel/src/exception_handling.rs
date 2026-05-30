use crate::fast_terminal::Terminal;
use crate::graphics::{Color, DrawTarget};
use x86_64::{
    registers::control::Cr2,
    structures::idt::{InterruptDescriptorTable, InterruptStackFrame, PageFaultErrorCode},
    VirtAddr,
};

/// Типы исключений
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ExceptionType {
    /// Двойная ошибка (Triple Fault)
    DoubleFault,
    /// Нарушение.page (Page Fault)
    PageFault,
    /// Нарушение прав доступа (General Protection Fault)
    GeneralProtectionFault,
    /// Нарушение границ (Bounds Check)
    BoundRangeExceeded,
    /// Недопустимая операция (Invalid Opcode)
    InvalidOpcode,
    /// Деление на ноль
    DivideByZero,
    /// Переполнение
    Overflow,
    /// Недопустимый TSS
    InvalidTss,
    /// Сегмент не присутствует
    SegmentNotPresent,
    /// Stack segment fault
    StackSegmentFault,
    /// Alignment check fault
    AlignmentCheck,
    /// Machine check exception
    MachineCheck,
    /// SIMD floating-point exception
    SimdFloatingPoint,
    /// Virtualization exception
    Virtualization,
    /// Security exception
    Security,
    /// Прочие исключения
    Other(u8),
}

/// Информация об исключении
#[derive(Debug)]
pub struct ExceptionInfo {
    /// Тип исключения
    pub exception_type: ExceptionType,
    /// Код ошибки
    pub error_code: u64,
    /// Адрес инструкции, вызвавшей исключение
    pub instruction_pointer: u64,
    /// Стек исключения
    pub stack_frame: ExceptionStackFrame,
    /// Время возникновения (в миллисекундах с загрузки)
    pub timestamp: u64,
    /// ИД процесса (если известно)
    pub process_id: Option<u64>,
}

/// Фрейм стека исключения
#[derive(Debug)]
pub struct ExceptionStackFrame {
    /// Регистр RAX
    pub rax: u64,
    /// Регистр RBX
    pub rbx: u64,
    /// Регистр RCX
    pub rcx: u64,
    /// Регистр RDX
    pub rdx: u64,
    /// Регистр RSI
    pub rsi: u64,
    /// Регистр RDI
    pub rdi: u64,
    /// Регистр RBP
    pub rbp: u64,
    /// Регистр RSP
    pub rsp: u64,
    /// Регистр R8
    pub r8: u64,
    /// Регистр R9
    pub r9: u64,
    /// Регистр R10
    pub r10: u64,
    /// Регистр R11
    pub r11: u64,
    /// Регистр R12
    pub r12: u64,
    /// Регистр R13
    pub r13: u64,
    /// Регистр R14
    pub r14: u64,
    /// Регистр R15
    pub r15: u64,
    /// Instruction Pointer
    pub rip: u64,
    /// Code Segment
    pub cs: u64,
    /// RFLAGS
    pub rflags: u64,
}

/// Обработчик исключений
pub struct ExceptionHandler {
    /// Собирает информацию об исключениях
    pub collected_exceptions: alloc::vec::Vec<ExceptionInfo>,
    /// Максимальное количество хранимых исключений
    pub max_collected: usize,
    /// Флаг, включено ли логгирование исключений
    pub logging_enabled: bool,
    /// Флаг, включена ли генерация дампов
    pub dump_generation_enabled: bool,
    /// Обработчики пользовательских исключений
    pub custom_handlers: alloc::collections::BTreeMap<ExceptionType, fn(ExceptionInfo)>,
}

impl ExceptionHandler {
    /// Создает новый обработчик исключений
    pub fn new() -> Self {
        ExceptionHandler {
            collected_exceptions: alloc::vec::Vec::new(),
            max_collected: 100, // Максимум 100 последних исключений
            logging_enabled: true,
            dump_generation_enabled: true,
            custom_handlers: alloc::collections::BTreeMap::new(),
        }
    }

    /// Регистрирует пользовательский обработчик для типа исключения
    pub fn register_custom_handler(&mut self, exc_type: ExceptionType, handler: fn(ExceptionInfo)) {
        self.custom_handlers.insert(exc_type, handler);
    }

    /// Удаляет пользовательский обработчик
    pub fn unregister_custom_handler(&mut self, exc_type: ExceptionType) {
        self.custom_handlers.remove(&exc_type);
    }

    /// Обрабатывает Page Fault
    pub fn handle_page_fault(
        &mut self,
        stack_frame: InterruptStackFrame,
        error_code: PageFaultErrorCode,
    ) {
        let fault_addr = unsafe { Cr2::read() };

        let exc_info = ExceptionInfo {
            exception_type: ExceptionType::PageFault,
            error_code: error_code.bits(),
            instruction_pointer: stack_frame.instruction_pointer.as_u64(),
            stack_frame: self.extract_stack_frame(&stack_frame),
            timestamp: crate::timer::get_uptime_ms(),
            process_id: self.get_current_process_id(),
        };

        self.log_exception(&exc_info);

        // Вызываем пользовательский обработчик, если есть
        if let Some(handler) = self.custom_handlers.get(&ExceptionType::PageFault) {
            handler(exc_info.clone());
        }

        // Обрабатываем в зависимости от типа ошибки
        if error_code.contains(PageFaultErrorCode::PROTECTION_VIOLATION) {
            // Нарушение защиты - скорее всего, попытка записи в read-only память
            self.handle_protection_violation(&exc_info, fault_addr);
        } else if error_code.contains(PageFaultErrorCode::CAUSED_BY_WRITE) {
            // Ошибка при записи
            self.handle_write_fault(&exc_info, fault_addr);
        } else {
            // Ошибка при чтении
            self.handle_read_fault(&exc_info, fault_addr);
        }
    }

    /// Обрабатывает двойную ошибку (Double Fault)
    pub fn handle_double_fault(&mut self, stack_frame: InterruptStackFrame, error_code: u64) -> ! {
        let exc_info = ExceptionInfo {
            exception_type: ExceptionType::DoubleFault,
            error_code,
            instruction_pointer: stack_frame.instruction_pointer.as_u64(),
            stack_frame: self.extract_stack_frame(&stack_frame),
            timestamp: crate::timer::get_uptime_ms(),
            process_id: self.get_current_process_id(),
        };

        self.log_exception(&exc_info);

        // Вызываем пользовательский обработчик
        if let Some(handler) = self.custom_handlers.get(&ExceptionType::DoubleFault) {
            handler(exc_info.clone());
        }

        // В двойной ошибке мы не можем дальше продолжать выполнение
        self.panic_with_exception("DOUBLE FAULT", &exc_info);
    }

    /// Обрабатывает General Protection Fault
    pub fn handle_general_protection_fault(
        &mut self,
        stack_frame: InterruptStackFrame,
        error_code: u64,
    ) {
        let exc_info = ExceptionInfo {
            exception_type: ExceptionType::GeneralProtectionFault,
            error_code,
            instruction_pointer: stack_frame.instruction_pointer.as_u64(),
            stack_frame: self.extract_stack_frame(&stack_frame),
            timestamp: crate::timer::get_uptime_ms(),
            process_id: self.get_current_process_id(),
        };

        self.log_exception(&exc_info);

        // Вызываем пользовательский обработчик
        if let Some(handler) = self
            .custom_handlers
            .get(&ExceptionType::GeneralProtectionFault)
        {
            handler(exc_info.clone());
        }

        // Для GP-исключения в пользовательском режиме можем попытаться завершить процесс
        // В ядерном режиме - критическая ошибка
        if self.is_kernel_mode(&exc_info) {
            self.panic_with_exception("GENERAL PROTECTION FAULT (KERNEL MODE)", &exc_info);
        } else {
            self.terminate_current_process(&exc_info);
        }
    }

    /// Обрабатывает другие типы исключений
    pub fn handle_other_exception(
        &mut self,
        vector: u8,
        stack_frame: InterruptStackFrame,
        error_code: Option<u64>,
    ) {
        let exception_type = match vector {
            0 => ExceptionType::DivideByZero,
            4 => ExceptionType::Overflow,
            5 => ExceptionType::BoundRangeExceeded,
            6 => ExceptionType::InvalidOpcode,
            10 => ExceptionType::InvalidTss,
            11 => ExceptionType::SegmentNotPresent,
            12 => ExceptionType::StackSegmentFault,
            17 => ExceptionType::AlignmentCheck,
            18 => ExceptionType::MachineCheck,
            19 => ExceptionType::SimdFloatingPoint,
            20 => ExceptionType::Virtualization,
            30 => ExceptionType::Security,
            _ => ExceptionType::Other(vector),
        };

        let exc_info = ExceptionInfo {
            exception_type,
            error_code: error_code.unwrap_or(0),
            instruction_pointer: stack_frame.instruction_pointer.as_u64(),
            stack_frame: self.extract_stack_frame(&stack_frame),
            timestamp: crate::timer::get_uptime_ms(),
            process_id: self.get_current_process_id(),
        };

        self.log_exception(&exc_info);

        // Вызываем пользовательский обработчик
        if let Some(handler) = self.custom_handlers.get(&exception_type) {
            handler(exc_info.clone());
        }

        // Для некоторых типов исключений применяем специальную логику
        match exception_type {
            ExceptionType::DivideByZero => {
                // Обработка деления на ноль
                self.handle_divide_by_zero(&exc_info);
            }
            ExceptionType::InvalidOpcode => {
                // Обработка недопустимого кода
                self.handle_invalid_opcode(&exc_info);
            }
            ExceptionType::AlignmentCheck => {
                // Обработка нарушения выравнивания
                self.handle_alignment_check(&exc_info);
            }
            _ => {
                // Для других исключений в пользовательском режиме - завершаем процесс
                if !self.is_kernel_mode(&exc_info) {
                    self.terminate_current_process(&exc_info);
                } else {
                    // В ядре - паника
                    self.panic_with_exception("UNHANDLED EXCEPTION", &exc_info);
                }
            }
        }
    }

    /// Извлекает информацию из стекового фрейма
    fn extract_stack_frame(&self, frame: &InterruptStackFrame) -> ExceptionStackFrame {
        // В реальной системе тут будет извлечение значений всех регистров
        // из стекового фрейма. Пока возвращаем фиктивные значения.
        ExceptionStackFrame {
            rax: 0,
            rbx: 0,
            rcx: 0,
            rdx: 0,
            rsi: 0,
            rdi: 0,
            rbp: frame.frame_pointer.as_u64(),
            rsp: frame.stack_pointer.as_u64(),
            r8: 0,
            r9: 0,
            r10: 0,
            r11: 0,
            r12: 0,
            r13: 0,
            r14: 0,
            r15: 0,
            rip: frame.instruction_pointer.as_u64(),
            cs: frame.code_segment,
            rflags: frame.flags,
        }
    }

    /// Логгирует информацию об исключении
    fn log_exception(&mut self, exc_info: &ExceptionInfo) {
        if self.logging_enabled {
            // Добавляем в историю
            if self.collected_exceptions.len() >= self.max_collected {
                self.collected_exceptions.remove(0); // Удаляем самое старое
            }

            self.collected_exceptions.push(exc_info.clone());

            // Выводим в терминал
            self.print_exception_to_display(exc_info);
        }
    }

    /// Печатает информацию об исключении на экран
    fn print_exception_to_display(&self, exc_info: &ExceptionInfo) {
        if let Some(term) = crate::fast_terminal::TERMINAL.lock().as_mut() {
            if let Some(display) = crate::graphics::GLOBAL_DISPLAY.lock().as_mut() {
                let severity_color = match exc_info.exception_type {
                    ExceptionType::PageFault => Color::new(255, 255, 0), // желтый
                    ExceptionType::GeneralProtectionFault => Color::new(255, 128, 0), // оранжевый
                    ExceptionType::DoubleFault => Color::new(255, 0, 0), // красный
                    _ => Color::new(255, 255, 255),                      // белый
                };

                term.print_str(
                    display,
                    &format!(
                        "[EXCEPTION] {}: Code=0x{:X}, IP=0x{:X}, PID={:?}\n",
                        self.exception_type_to_string(exc_info.exception_type),
                        exc_info.error_code,
                        exc_info.instruction_pointer,
                        exc_info.process_id
                    ),
                    severity_color,
                );
            }
        }
    }

    /// Преобразует тип исключения в строку
    fn exception_type_to_string(&self, exc_type: ExceptionType) -> &'static str {
        match exc_type {
            ExceptionType::DoubleFault => "DOUBLE_FAULT",
            ExceptionType::PageFault => "PAGE_FAULT",
            ExceptionType::GeneralProtectionFault => "GENERAL_PROTECTION",
            ExceptionType::BoundRangeExceeded => "BOUND_RANGE_EXCEEDED",
            ExceptionType::InvalidOpcode => "INVALID_OPCODE",
            ExceptionType::DivideByZero => "DIVIDE_BY_ZERO",
            ExceptionType::Overflow => "OVERFLOW",
            ExceptionType::InvalidTss => "INVALID_TSS",
            ExceptionType::SegmentNotPresent => "SEGMENT_NOT_PRESENT",
            ExceptionType::StackSegmentFault => "STACK_SEGMENT_FAULT",
            ExceptionType::AlignmentCheck => "ALIGNMENT_CHECK",
            ExceptionType::MachineCheck => "MACHINE_CHECK",
            ExceptionType::SimdFloatingPoint => "SIMD_FLOATING_POINT",
            ExceptionType::Virtualization => "VIRTUALIZATION",
            ExceptionType::Security => "SECURITY",
            ExceptionType::Other(v) => match v {
                0 => "DIVIDE_BY_ZERO",
                1 => "DEBUG",
                2 => "NON_MASKABLE_INTERRUPT",
                3 => "BREAKPOINT",
                4 => "OVERFLOW",
                5 => "BOUND_RANGE_EXCEEDED",
                6 => "INVALID_OPCODE",
                7 => "DEVICE_NOT_AVAILABLE",
                8 => "DOUBLE_FAULT",
                10 => "INVALID_TSS",
                11 => "SEGMENT_NOT_PRESENT",
                12 => "STACK_SEGMENT_FAULT",
                13 => "GENERAL_PROTECTION_FAULT",
                14 => "PAGE_FAULT",
                16 => "X87_FLOATING_POINT_EXCEPTION",
                17 => "ALIGNMENT_CHECK",
                18 => "MACHINE_CHECK",
                19 => "SIMD_FLOATING_POINT",
                20 => "VIRTUALIZATION",
                30 => "SECURITY_EXCEPTION",
                _ => "UNKNOWN_EXCEPTION",
            },
        }
    }

    /// Проверяет, является ли исключение в ядерном режиме
    fn is_kernel_mode(&self, _exc_info: &ExceptionInfo) -> bool {
        // В реальной системе проверяем CS регистр или уровень привилегий
        // Пока простая проверка на основе адреса IP
        true // Предполагаем, что всегда в ядре для простоты
    }

    /// Получает ID текущего процесса
    fn get_current_process_id(&self) -> Option<u64> {
        // В реальной системе: получить из TCB или PCB
        None
    }

    /// Создает дамп памяти при исключении
    fn create_memory_dump(&self, exc_info: &ExceptionInfo) {
        if self.dump_generation_enabled {
            // В реальной системе: сохранить стек, регистры, память в файл дампа
            // Пока просто логгируем
            crate::logging::log_error(&format!(
                "Memory dump would be created for exception at IP=0x{:X}",
                exc_info.instruction_pointer
            ));
        }
    }

    /// Обрабатывает нарушение защиты
    fn handle_protection_violation(&mut self, exc_info: &ExceptionInfo, fault_addr: VirtAddr) {
        // Проверяем, не является ли это COW страницей
        if crate::memory::is_cow_page(fault_addr.as_u64()) {
            // Если да, то нужно выполнить COW операцию
            if crate::memory::perform_cow_operation(fault_addr.as_u64()).is_ok() {
                // Успешно обработали COW, продолжаем выполнение
                return;
            }
        }

        // Если не COW, возможно это пользовательская программа
        if !self.is_kernel_mode(exc_info) {
            // Завершаем процесс
            self.terminate_current_process(exc_info);
        } else {
            // В ядре - критическая ошибка
            self.panic_with_exception("PROTECTION VIOLATION", exc_info);
        }
    }

    /// Обрабатывает ошибку при записи
    fn handle_write_fault(&mut self, exc_info: &ExceptionInfo, fault_addr: VirtAddr) {
        // Проверяем, является ли адрес валидным
        if !crate::memory::is_valid_address(fault_addr.as_u64()) {
            // Невалидный адрес
            if !self.is_kernel_mode(exc_info) {
                self.terminate_current_process(exc_info);
            } else {
                self.panic_with_exception("WRITE TO INVALID ADDRESS", exc_info);
            }
            return;
        }

        // Может быть попытка записи в кодовую секцию
        if crate::memory::is_code_section_address(fault_addr.as_u64()) {
            // Попытка записи в код
            if !self.is_kernel_mode(exc_info) {
                self.terminate_current_process(exc_info);
            } else {
                self.panic_with_exception("WRITE TO CODE SECTION", exc_info);
            }
            return;
        }

        // В остальных случаях - обычный page fault
        if !self.is_kernel_mode(exc_info) {
            self.terminate_current_process(exc_info);
        } else {
            self.panic_with_exception("WRITE PAGE FAULT", exc_info);
        }
    }

    /// Обрабатывает ошибку при чтении
    fn handle_read_fault(&mut self, exc_info: &ExceptionInfo, fault_addr: VirtAddr) {
        // Проверяем, является ли адрес валидным
        if !crate::memory::is_valid_address(fault_addr.as_u64()) {
            if !self.is_kernel_mode(exc_info) {
                self.terminate_current_process(exc_info);
            } else {
                self.panic_with_exception("READ FROM INVALID ADDRESS", exc_info);
            }
            return;
        }

        // Проверяем, не попытка ли это прочитать защищенную память
        if !crate::memory::is_readable_address(fault_addr.as_u64()) {
            if !self.is_kernel_mode(exc_info) {
                self.terminate_current_process(exc_info);
            } else {
                self.panic_with_exception("READ FROM PROTECTED ADDRESS", exc_info);
            }
            return;
        }

        // В остальных случаях - обычный page fault
        if !self.is_kernel_mode(exc_info) {
            self.terminate_current_process(exc_info);
        } else {
            self.panic_with_exception("READ PAGE FAULT", exc_info);
        }
    }

    /// Обрабатывает деление на ноль
    fn handle_divide_by_zero(&mut self, exc_info: &ExceptionInfo) {
        // В большинстве случаев это ошибка программы
        if !self.is_kernel_mode(exc_info) {
            self.terminate_current_process(exc_info);
        } else {
            // В ядре - это критическая ошибка
            self.panic_with_exception("DIVIDE BY ZERO", exc_info);
        }
    }

    /// Обрабатывает недопустимый опкод
    fn handle_invalid_opcode(&mut self, exc_info: &ExceptionInfo) {
        // Это почти всегда ошибка программы
        if !self.is_kernel_mode(exc_info) {
            self.terminate_current_process(exc_info);
        } else {
            // В ядре - критическая ошибка
            self.panic_with_exception("INVALID OPCODE", exc_info);
        }
    }

    /// Обрабатывает проверку выравнивания
    fn handle_alignment_check(&mut self, exc_info: &ExceptionInfo) {
        // Выравнивание проверяется только в определенных режимах
        if !self.is_kernel_mode(exc_info) {
            self.terminate_current_process(exc_info);
        } else {
            self.panic_with_exception("ALIGNMENT CHECK", exc_info);
        }
    }

    /// Завершает текущий процесс
    fn terminate_current_process(&mut self, exc_info: &ExceptionInfo) {
        crate::logging::log_error(&format!(
            "Terminating process due to exception: {} at IP=0x{:X}",
            self.exception_type_to_string(exc_info.exception_type),
            exc_info.instruction_pointer
        ));

        // В реальной системе: вызвать планировщик для переключения с ошибочного процесса
        // и пометить процесс как завершившийся с ошибкой
    }

    /// Паника с выводом информации об исключении
    fn panic_with_exception(&self, message: &str, exc_info: &ExceptionInfo) -> ! {
        // Выводим информацию об исключении
        if let Some(term) = crate::fast_terminal::TERMINAL.lock().as_mut() {
            if let Some(display) = crate::graphics::GLOBAL_DISPLAY.lock().as_mut() {
                term.print_str(
                    display,
                    &format!("\n=== KERNEL PANIC ===\n"),
                    Color::new(255, 0, 0),
                );
                term.print_str(
                    display,
                    &format!("Message: {}\n", message),
                    Color::new(255, 0, 0),
                );
                term.print_str(
                    display,
                    &format!(
                        "Exception: {}\n",
                        self.exception_type_to_string(exc_info.exception_type)
                    ),
                    Color::new(255, 255, 0),
                );
                term.print_str(
                    display,
                    &format!("Error Code: 0x{:X}\n", exc_info.error_code),
                    Color::new(255, 255, 255),
                );
                term.print_str(
                    display,
                    &format!(
                        "Instruction Pointer: 0x{:X}\n",
                        exc_info.instruction_pointer
                    ),
                    Color::new(255, 255, 255),
                );

                if let Some(pid) = exc_info.process_id {
                    term.print_str(
                        display,
                        &format!("Process ID: {}\n", pid),
                        Color::new(255, 255, 255),
                    );
                }

                term.print_str(
                    display,
                    &format!("Timestamp: {}ms\n", exc_info.timestamp),
                    Color::new(255, 255, 255),
                );
                term.print_str(
                    display,
                    &format!(
                        "Stack Frame:\n  RSP: 0x{:X}\n  RBP: 0x{:X}\n  RIP: 0x{:X}\n",
                        exc_info.stack_frame.rsp,
                        exc_info.stack_frame.rbp,
                        exc_info.stack_frame.rip
                    ),
                    Color::new(255, 255, 255),
                );
            }
        }

        // Создаем дамп памяти
        self.create_memory_dump(exc_info);

        // В реальной системе тут может быть:
        // - Сохранение дампа
        // - Перезагрузка
        // - Вход в отладочный режим
        // Пока просто паника

        crate::halt_and_catch_fire(&format!(
            "Kernel panic: {} at 0x{:X}",
            message, exc_info.instruction_pointer
        ));
    }

    /// Получает историю исключений
    pub fn get_exception_history(&self) -> &[ExceptionInfo] {
        &self.collected_exceptions
    }

    /// Очищает историю исключений
    pub fn clear_exception_history(&mut self) {
        self.collected_exceptions.clear();
    }

    /// Включает логгирование исключений
    pub fn enable_logging(&mut self) {
        self.logging_enabled = true;
    }

    /// Выключает логгирование исключений
    pub fn disable_logging(&mut self) {
        self.logging_enabled = false;
    }

    /// Включает генерацию дампов
    pub fn enable_dump_generation(&mut self) {
        self.dump_generation_enabled = true;
    }

    /// Выключает генерацию дампов
    pub fn disable_dump_generation(&mut self) {
        self.dump_generation_enabled = false;
    }

    /// Получает количество исключений определенного типа
    pub fn count_exceptions_by_type(&self, exc_type: ExceptionType) -> usize {
        self.collected_exceptions
            .iter()
            .filter(|exc| exc.exception_type == exc_type)
            .count()
    }

    /// Получает наиболее часто встречающиеся исключения
    pub fn get_most_common_exceptions(&self) -> alloc::vec::Vec<(ExceptionType, usize)> {
        use alloc::collections::BTreeMap;
        let mut counts = BTreeMap::new();

        for exc in &self.collected_exceptions {
            *counts.entry(exc.exception_type).or_insert(0) += 1;
        }

        let mut result: alloc::vec::Vec<(ExceptionType, usize)> = counts.into_iter().collect();
        result.sort_by(|a, b| b.1.cmp(&a.1)); // Сортируем по убыванию

        result
    }
}

/// Интерфейс для системы исключений
pub struct ExceptionSystemInterface;

impl ExceptionSystemInterface {
    /// Инициализирует систему обработки исключений
    pub fn init_exception_handling() -> &'static spin::Mutex<ExceptionHandler> {
        use spin::Once;
        static EXCEPTION_HANDLER: Once<spin::Mutex<ExceptionHandler>> = Once::new();

        EXCEPTION_HANDLER.call_once(|| spin::Mutex::new(ExceptionHandler::new()))
    }

    /// Регистрирует пользовательский обработчик исключения
    pub fn register_exception_handler(exc_type: ExceptionType, handler: fn(ExceptionInfo)) {
        let mut ex_handler = Self::init_exception_handling().lock();
        ex_handler.register_custom_handler(exc_type, handler);
    }

    /// Удаляет пользовательский обработчик исключения
    pub fn unregister_exception_handler(exc_type: ExceptionType) {
        let mut ex_handler = Self::init_exception_handling().lock();
        ex_handler.unregister_custom_handler(exc_type);
    }

    /// Обрабатывает Page Fault
    pub fn handle_page_fault_interrupt(
        stack_frame: InterruptStackFrame,
        error_code: PageFaultErrorCode,
    ) {
        let mut ex_handler = Self::init_exception_handling().lock();
        ex_handler.handle_page_fault(stack_frame, error_code);
    }

    /// Обрабатывает Double Fault
    pub fn handle_double_fault_interrupt(stack_frame: InterruptStackFrame, error_code: u64) -> ! {
        let mut ex_handler = Self::init_exception_handling().lock();
        ex_handler.handle_double_fault(stack_frame, error_code);
    }

    /// Обрабатывает General Protection Fault
    pub fn handle_general_protection_fault_interrupt(
        stack_frame: InterruptStackFrame,
        error_code: u64,
    ) {
        let mut ex_handler = Self::init_exception_handling().lock();
        ex_handler.handle_general_protection_fault(stack_frame, error_code);
    }

    /// Обрабатывает другие исключения
    pub fn handle_other_exception_interrupt(
        vector: u8,
        stack_frame: InterruptStackFrame,
        error_code: Option<u64>,
    ) {
        let mut ex_handler = Self::init_exception_handling().lock();
        ex_handler.handle_other_exception(vector, stack_frame, error_code);
    }

    /// Получает историю исключений
    pub fn get_exception_history() -> alloc::vec::Vec<ExceptionInfo> {
        let ex_handler = Self::init_exception_handling().lock();
        ex_handler.get_exception_history().to_vec()
    }

    /// Очищает историю исключений
    pub fn clear_exception_history() {
        let mut ex_handler = Self::init_exception_handling().lock();
        ex_handler.clear_exception_history();
    }

    /// Проверяет, были ли исключения определенного типа
    pub fn has_exceptions_of_type(exc_type: ExceptionType) -> bool {
        let ex_handler = Self::init_exception_handling().lock();
        ex_handler.count_exceptions_by_type(exc_type) > 0
    }

    /// Получает статистику исключений
    pub fn get_exception_statistics() -> ExceptionStats {
        let ex_handler = Self::init_exception_handling().lock();
        ExceptionStats {
            total_exceptions: ex_handler.get_exception_history().len(),
            most_common: ex_handler
                .get_most_common_exceptions()
                .into_iter()
                .take(5)
                .collect(),
        }
    }
}

/// Статистика исключений
pub struct ExceptionStats {
    pub total_exceptions: usize,
    pub most_common: alloc::vec::Vec<(ExceptionType, usize)>,
}

/// Обертка для IDT с поддержкой исключений
pub struct ExceptionIdtWrapper;

impl ExceptionIdtWrapper {
    /// Устанавливает обработчики исключений в IDT
    pub fn setup_exception_handlers(idt: &mut InterruptDescriptorTable) {
        use x86_64::structures::idt::PageFaultHandler;

        // Устанавливаем стандартные обработчики
        idt.divide_by_zero.set_handler_fn(divide_by_zero_handler);
        idt.debug.set_handler_fn(debug_handler);
        idt.non_maskable_interrupt.set_handler_fn(nmi_handler);
        idt.breakpoint.set_handler_fn(breakpoint_handler);
        idt.overflow.set_handler_fn(overflow_handler);
        idt.bound_range_exceeded
            .set_handler_fn(bound_range_exceeded_handler);
        idt.invalid_opcode.set_handler_fn(invalid_opcode_handler);
        idt.device_not_available
            .set_handler_fn(device_not_available_handler);
        idt.double_fault.set_handler_fn(double_fault_handler);
        idt.invalid_tss.set_handler_fn(invalid_tss_handler);
        idt.segment_not_present
            .set_handler_fn(segment_not_present_handler);
        idt.stack_segment_fault
            .set_handler_fn(stack_segment_fault_handler);
        idt.general_protection_fault
            .set_handler_fn(general_protection_handler);

        idt.page_fault.set_handler_fn(page_fault_handler);

        idt.x87_floating_point
            .set_handler_fn(x87_floating_point_handler);
        idt.alignment_check.set_handler_fn(alignment_check_handler);
        idt.machine_check.set_handler_fn(machine_check_handler);
        idt.simd_floating_point
            .set_handler_fn(simd_floating_point_handler);
        idt.virtualization.set_handler_fn(virtualization_handler);
    }
}

// Обработчики исключений
extern "x86-interrupt" fn divide_by_zero_handler(stack_frame: InterruptStackFrame) {
    ExceptionSystemInterface::handle_other_exception_interrupt(0, stack_frame, Some(0));
}

extern "x86-interrupt" fn debug_handler(stack_frame: InterruptStackFrame) {
    ExceptionSystemInterface::handle_other_exception_interrupt(1, stack_frame, None);
}

extern "x86-interrupt" fn nmi_handler(stack_frame: InterruptStackFrame) {
    ExceptionSystemInterface::handle_other_exception_interrupt(2, stack_frame, None);
}

extern "x86-interrupt" fn breakpoint_handler(stack_frame: InterruptStackFrame) {
    ExceptionSystemInterface::handle_other_exception_interrupt(3, stack_frame, None);
}

extern "x86-interrupt" fn overflow_handler(stack_frame: InterruptStackFrame) {
    ExceptionSystemInterface::handle_other_exception_interrupt(4, stack_frame, None);
}

extern "x86-interrupt" fn bound_range_exceeded_handler(stack_frame: InterruptStackFrame) {
    ExceptionSystemInterface::handle_other_exception_interrupt(5, stack_frame, None);
}

extern "x86-interrupt" fn invalid_opcode_handler(stack_frame: InterruptStackFrame) {
    ExceptionSystemInterface::handle_other_exception_interrupt(6, stack_frame, None);
}

extern "x86-interrupt" fn device_not_available_handler(stack_frame: InterruptStackFrame) {
    ExceptionSystemInterface::handle_other_exception_interrupt(7, stack_frame, None);
}

extern "x86-interrupt" fn double_fault_handler(
    stack_frame: InterruptStackFrame,
    error_code: u64,
) -> ! {
    ExceptionSystemInterface::handle_double_fault_interrupt(stack_frame, error_code);
}

extern "x86-interrupt" fn invalid_tss_handler(stack_frame: InterruptStackFrame, error_code: u64) {
    ExceptionSystemInterface::handle_other_exception_interrupt(10, stack_frame, Some(error_code));
}

extern "x86-interrupt" fn segment_not_present_handler(
    stack_frame: InterruptStackFrame,
    error_code: u64,
) {
    ExceptionSystemInterface::handle_other_exception_interrupt(11, stack_frame, Some(error_code));
}

extern "x86-interrupt" fn stack_segment_fault_handler(
    stack_frame: InterruptStackFrame,
    error_code: u64,
) {
    ExceptionSystemInterface::handle_other_exception_interrupt(12, stack_frame, Some(error_code));
}

extern "x86-interrupt" fn general_protection_handler(
    stack_frame: InterruptStackFrame,
    error_code: u64,
) {
    ExceptionSystemInterface::handle_general_protection_fault_interrupt(stack_frame, error_code);
}

extern "x86-interrupt" fn page_fault_handler(
    stack_frame: InterruptStackFrame,
    error_code: PageFaultErrorCode,
) {
    ExceptionSystemInterface::handle_page_fault_interrupt(stack_frame, error_code);
}

extern "x86-interrupt" fn x87_floating_point_handler(stack_frame: InterruptStackFrame) {
    ExceptionSystemInterface::handle_other_exception_interrupt(16, stack_frame, None);
}

extern "x86-interrupt" fn alignment_check_handler(
    stack_frame: InterruptStackFrame,
    error_code: u64,
) {
    ExceptionSystemInterface::handle_other_exception_interrupt(17, stack_frame, Some(error_code));
}

extern "x86-interrupt" fn machine_check_handler(stack_frame: InterruptStackFrame) -> ! {
    // Machine Check - критическая ошибка оборудования
    ExceptionSystemInterface::handle_other_exception_interrupt(18, stack_frame, None);
    panic!("Machine Check Exception - hardware failure");
}

extern "x86-interrupt" fn simd_floating_point_handler(stack_frame: InterruptStackFrame) {
    ExceptionSystemInterface::handle_other_exception_interrupt(19, stack_frame, None);
}

extern "x86-interrupt" fn virtualization_handler(stack_frame: InterruptStackFrame) {
    ExceptionSystemInterface::handle_other_exception_interrupt(20, stack_frame, None);
}

/// Макросы для удобства обработки исключений
#[macro_export]
macro_rules! handle_exception {
    ($type:expr, $handler:expr) => {
        $crate::exception_handling::ExceptionSystemInterface::register_exception_handler(
            $type, $handler,
        );
    };
}

#[macro_export]
macro_rules! panic_with_info {
    ($msg:expr, $exc_info:expr) => {
        $crate::exception_handling::ExceptionSystemInterface::handle_general_exception_panic(
            $msg, $exc_info,
        );
    };
}

#[macro_export]
macro_rules! log_exception {
    ($exc_info:expr) => {
        $crate::exception_handling::ExceptionHandler::log_exception(&$exc_info);
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_exception_handler_creation() {
        let handler = ExceptionHandler::new();
        assert_eq!(handler.collected_exceptions.len(), 0);
        assert_eq!(handler.max_collected, 100);
    }

    #[test]
    fn test_exception_registration() {
        let mut handler = ExceptionHandler::new();

        fn test_handler(_info: ExceptionInfo) {
            // Test handler
        }

        handler.register_custom_handler(ExceptionType::PageFault, test_handler);
        assert!(handler
            .custom_handlers
            .contains_key(&ExceptionType::PageFault));

        handler.unregister_custom_handler(ExceptionType::PageFault);
        assert!(!handler
            .custom_handlers
            .contains_key(&ExceptionType::PageFault));
    }

    #[test]
    fn test_exception_info_creation() {
        let exc_info = ExceptionInfo {
            exception_type: ExceptionType::PageFault,
            error_code: 0x0001,
            instruction_pointer: 0x12345678,
            stack_frame: ExceptionStackFrame {
                rax: 0,
                rbx: 0,
                rcx: 0,
                rdx: 0,
                rsi: 0,
                rdi: 0,
                rbp: 0,
                rsp: 0,
                r8: 0,
                r9: 0,
                r10: 0,
                r11: 0,
                r12: 0,
                r13: 0,
                r14: 0,
                r15: 0,
                rip: 0x12345678,
                cs: 0x08,
                rflags: 0x202,
            },
            timestamp: 1000,
            process_id: Some(123),
        };

        assert_eq!(exc_info.exception_type, ExceptionType::PageFault);
        assert_eq!(exc_info.error_code, 0x0001);
        assert_eq!(exc_info.instruction_pointer, 0x12345678);
    }
}
