use core::cell::UnsafeCell;
use core::sync::atomic::Ordering;
use core::sync::atomic::{AtomicBool, AtomicUsize};
use spin::{Mutex, MutexGuard};

/// Barrier (Синхронизационный барьер) - примитив синхронизации,
/// который останавливает все потоки до тех пор, пока не достигнут
/// определенное количество участников.
pub struct Barrier {
    /// Количество ожидаемых участников
    count: usize,
    /// Количество текущих участников
    current: AtomicUsize,
    /// Фаза барьера (для многократного использования)
    phase: AtomicUsize,
    /// Флаг готовности всех участников
    arrived: AtomicBool,
    /// Внутренняя защита для синхронизации
    lock: Mutex<()>,
}

impl Barrier {
    /// Создает новый барьер с указанным количеством участников
    pub fn new(count: usize) -> Self {
        Barrier {
            count,
            current: AtomicUsize::new(0),
            phase: AtomicUsize::new(0),
            arrived: AtomicBool::new(false),
            lock: Mutex::new(()),
        }
    }

    /// Ожидает до тех пор, пока не придут все участники
    pub fn arrive_and_wait(&self) {
        let mut guard = self.lock.lock();
        let current_phase = self.phase.load(Ordering::Acquire);

        // Увеличиваем счетчик прибывших
        let old_count = self.current.fetch_add(1, Ordering::AcqRel);

        if old_count + 1 == self.count {
            // Мы последний участник, сбрасываем состояние
            self.current.store(0, Ordering::Release);
            self.arrived.store(true, Ordering::Release);

            // Освобождаем других участников
            drop(guard);
        } else {
            // Ждем, пока не придут все
            drop(guard);

            while self.phase.load(Ordering::Acquire) == current_phase
                && !self.arrived.load(Ordering::Acquire)
            {
                core::hint::spin_loop();
            }
        }

        // Увеличиваем фазу для следующего цикла
        if self.arrived.load(Ordering::Acquire) {
            // Только один поток должен увеличить фазу
            let _guard = self.lock.lock();
            if self.arrived.load(Ordering::Acquire) {
                self.phase.fetch_add(1, Ordering::AcqRel);
                self.arrived.store(false, Ordering::Release);
            }
        }
    }
}

/// CountdownLatch - ловушка обратного отсчета
///
/// Потоки могут ждать, пока счетчик не достигнет нуля.
pub struct CountdownLatch {
    count: AtomicUsize,
    completed: AtomicBool,
    lock: Mutex<()>,
}

impl CountdownLatch {
    /// Создает новую ловушку с начальным значением
    pub fn new(count: usize) -> Self {
        CountdownLatch {
            count: AtomicUsize::new(count),
            completed: AtomicBool::new(false),
            lock: Mutex::new(()),
        }
    }

    /// Ждет, пока счетчик не станет 0
    pub fn wait(&self) {
        while self.count.load(Ordering::Acquire) > 0 {
            core::hint::spin_loop();
        }
    }

    /// Сигнализирует о выполнении одной задачи
    pub fn count_down(&self) {
        let old_count = self.count.fetch_sub(1, Ordering::AcqRel);

        if old_count == 1 {
            // Последняя задача завершена, помечаем как выполненное
            self.completed.store(true, Ordering::Release);
        }
    }

    /// Возвращает текущее значение счетчика
    pub fn get_count(&self) -> usize {
        self.count.load(Ordering::Acquire)
    }

    /// Проверяет, завершена ли ловушка
    pub fn is_completed(&self) -> bool {
        self.completed.load(Ordering::Acquire)
    }
}

/// CyclicBarrier - циклический барьер (повторное использование)
pub struct CyclicBarrier {
    /// Количество ожидаемых участников
    parties: usize,
    /// Текущий цикл
    cycle: AtomicUsize,
    /// Количество прибывших в текущем цикле
    count: AtomicUsize,
    /// Фаза завершения
    generation: AtomicUsize,
    /// Взаимоисключающая блокировка
    mutex: Mutex<()>,
    /// Количество сброшенных циклов
    resets: AtomicUsize,
}

impl CyclicBarrier {
    /// Создает новый циклический барьер
    pub fn new(parties: usize) -> Result<Self, &'static str> {
        if parties == 0 {
            return Err("Parties must be greater than 0");
        }

        Ok(CyclicBarrier {
            parties,
            cycle: AtomicUsize::new(0),
            count: AtomicUsize::new(parties),
            generation: AtomicUsize::new(0),
            mutex: Mutex::new(()),
            resets: AtomicUsize::new(0),
        })
    }

    /// Ожидает до завершения цикла, затем переходит к следующему
    pub fn await_barrier(&self) -> Result<(), &'static str> {
        let mut mutex_guard = self.mutex.lock();

        let current_count = self.count.fetch_sub(1, Ordering::AcqRel);

        // Проверяем, является ли текущий поток последним
        if current_count == 1 {
            // Это последний участник, сбрасываем барьер
            self.count.store(self.parties, Ordering::Release);
            self.cycle.fetch_add(1, Ordering::Release);
            self.generation.fetch_add(1, Ordering::Release);

            drop(mutex_guard);
            Ok(())
        } else {
            // Ждем, пока не придет последний участник
            let current_generation = self.generation.load(Ordering::Acquire);
            drop(mutex_guard);

            // Ждем завершения текущего цикла
            while self.generation.load(Ordering::Acquire) == current_generation {
                core::hint::spin_loop();
            }

            Ok(())
        }
    }

    /// Возвращает количество ожидаемых участников
    pub fn party_count(&self) -> usize {
        self.parties
    }

    /// Возвращает количество завершенных циклов
    pub fn get_number_of_cycles(&self) -> usize {
        self.cycle.load(Ordering::Acquire)
    }
}

// Semaphore - семафор для контроля доступа к ресурсам
pub struct Semaphore {
    /// Доступное количество единиц ресурса
    permits: AtomicUsize,
    /// Максимально возможное количество прав
    max_permits: usize,
    /// Защита для внутреннего состояния
    mutex: Mutex<()>,
}

impl Semaphore {
    /// Создает новый семафор с начальным количеством прав
    pub fn new(initial_permits: usize) -> Self {
        Semaphore {
            permits: AtomicUsize::new(initial_permits),
            max_permits: initial_permits * 2, // Максимум в 2 раза больше начального
            mutex: Mutex::new(()),
        }
    }

    /// Захватывает право доступа (уменьшает счетчик)
    pub fn acquire(&self) {
        loop {
            let current = self.permits.load(Ordering::Acquire);

            if current > 0 {
                // Пробуем атомарно уменьшить
                match self.permits.compare_exchange(
                    current,
                    current - 1,
                    Ordering::AcqRel,
                    Ordering::Acquire,
                ) {
                    Ok(_) => break,     // Успешно захвачено
                    Err(_) => continue, // Неудача, пробуем снова
                }
            } else {
                // Нет доступных прав, ждем
                core::hint::spin_loop();
            }
        }
    }

    /// Освобождает право доступа (увеличивает счетчик)
    pub fn release(&self) {
        let current = self.permits.load(Ordering::Acquire);

        if current < self.max_permits {
            self.permits.fetch_add(1, Ordering::Release);
        }
    }

    /// Пытается захватить право без блокировки
    pub fn try_acquire(&self) -> bool {
        let current = self.permits.load(Ordering::Relaxed);

        if current > 0 {
            match self.permits.compare_exchange(
                current,
                current - 1,
                Ordering::AcqRel,
                Ordering::Acquire,
            ) {
                Ok(_) => true,
                Err(_) => false,
            }
        } else {
            false
        }
    }

    /// Возвращает текущее количество доступных прав
    pub fn available_permits(&self) -> usize {
        self.permits.load(Ordering::Acquire)
    }
}

/// PhasedBarrier - барьер с фазами выполнения
pub struct PhasedBarrier {
    /// Количество участников
    participants: usize,
    /// Текущая фаза
    current_phase: AtomicUsize,
    /// Счетчик участников в текущей фазе
    phase_count: AtomicUsize,
    /// Счетчик завершенных циклов
    cycle_count: AtomicUsize,
    /// Защита доступа
    lock: Mutex<()>,
    /// Максимальное количество фаз
    max_phases: usize,
}

impl PhasedBarrier {
    /// Создает новый phased barrier
    pub fn new(participants: usize, max_phases: usize) -> Self {
        PhasedBarrier {
            participants,
            current_phase: AtomicUsize::new(0),
            phase_count: AtomicUsize::new(0),
            cycle_count: AtomicUsize::new(0),
            lock: Mutex::new(()),
            max_phases,
        }
    }

    /// Переходит к следующей фазе
    pub fn next_phase(&self) -> Result<(), &'static str> {
        let mut guard = self.lock.lock();

        let current_count = self.phase_count.fetch_add(1, Ordering::AcqRel);

        if current_count + 1 == self.participants {
            // Все участники достигли текущей фазы
            let new_phase = (self.current_phase.load(Ordering::Acquire) + 1) % self.max_phases;
            self.current_phase.store(new_phase, Ordering::Release);
            self.phase_count.store(0, Ordering::Release);
            self.cycle_count.fetch_add(1, Ordering::Release);

            drop(guard);
            Ok(())
        } else if current_count >= self.participants {
            // Ошибка - слишком много участников
            drop(guard);
            Err("Too many participants")
        } else {
            // Ожидаем других участников
            let current_phase = self.current_phase.load(Ordering::Acquire);
            drop(guard);

            while self.current_phase.load(Ordering::Acquire) == current_phase {
                core::hint::spin_loop();
            }

            Ok(())
        }
    }

    /// Получает текущую фазу
    pub fn current_phase(&self) -> usize {
        self.current_phase.load(Ordering::Acquire)
    }

    /// Получает количество завершенных циклов
    pub fn cycle_count(&self) -> usize {
        self.cycle_count.load(Ordering::Acquire)
    }
}

/// Интерфейс для работы с синхронизационными барьерами
pub struct SyncBarrierInterface;

impl SyncBarrierInterface {
    /// Создает новый барьер
    pub fn create_barrier(count: usize) -> Barrier {
        Barrier::new(count)
    }

    /// Создает новую ловушку обратного отсчета
    pub fn create_countdown_latch(count: usize) -> CountdownLatch {
        CountdownLatch::new(count)
    }

    /// Создает новый циклический барьер
    pub fn create_cyclic_barrier(parties: usize) -> Result<CyclicBarrier, &'static str> {
        CyclicBarrier::new(parties)
    }

    /// Создает новый семафор
    pub fn create_semaphore(initial_permits: usize) -> Semaphore {
        Semaphore::new(initial_permits)
    }

    /// Создает новый phased barrier
    pub fn create_phased_barrier(participants: usize, max_phases: usize) -> PhasedBarrier {
        PhasedBarrier::new(participants, max_phases)
    }
}

/// Макросы для удобства
#[macro_export]
macro_rules! sync_barrier {
    ($count:expr) => {
        $crate::sync_barriers::SyncBarrierInterface::create_barrier($count)
    };
}

#[macro_export]
macro_rules! countdown_latch {
    ($count:expr) => {
        $crate::sync_barriers::SyncBarrierInterface::create_countdown_latch($count)
    };
}

#[macro_export]
macro_rules! cyclic_barrier {
    ($parties:expr) => {
        $crate::sync_barriers::SyncBarrierInterface::create_cyclic_barrier($parties)
    };
}

#[macro_export]
macro_rules! semaphore {
    ($initial:expr) => {
        $crate::sync_barriers::SyncBarrierInterface::create_semaphore($initial)
    };
}

#[macro_export]
macro_rules! phased_barrier {
    ($participants:expr, $phases:expr) => {
        $crate::sync_barriers::SyncBarrierInterface::create_phased_barrier($participants, $phases)
    };
}

#[macro_export]
macro_rules! wait_on_barrier {
    ($barrier:expr) => {
        $barrier.arrive_and_wait()
    };
}

#[macro_export]
macro_rules! countdown {
    ($latch:expr) => {
        $latch.count_down()
    };
}

#[macro_export]
macro_rules! acquire_semaphore {
    ($sem:expr) => {
        $sem.acquire()
    };
}

#[macro_export]
macro_rules! release_semaphore {
    ($sem:expr) => {
        $sem.release()
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_barrier() {
        let barrier = Barrier::new(3);

        // Ничего не происходит до прибытия всех
        for _ in 0..2 {
            let b = barrier.clone();
            // В реальной системе тут были бы потоки
            // b.arrive_and_wait();
        }

        // После прибытия последнего участника все проходят барьер
    }

    #[test]
    fn test_countdown_latch() {
        let latch = CountdownLatch::new(2);
        assert_eq!(latch.get_count(), 2);

        latch.count_down();
        assert_eq!(latch.get_count(), 1);

        latch.count_down();
        assert_eq!(latch.get_count(), 0);
        assert!(latch.is_completed());
    }

    #[test]
    fn test_semaphore() {
        let sem = Semaphore::new(2);
        assert_eq!(sem.available_permits(), 2);

        sem.acquire();
        assert_eq!(sem.available_permits(), 1);

        sem.acquire();
        assert_eq!(sem.available_permits(), 0);

        sem.release();
        assert_eq!(sem.available_permits(), 1);
    }
}
