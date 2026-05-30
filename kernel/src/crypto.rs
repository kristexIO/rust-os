/// Минимальный криптографический модуль (генератор случайных чисел)
pub mod random {
    pub struct RandomGen {
        state: u64,
    }

    impl RandomGen {
        /// Создаёт новый генератор с заданным зерном (entropy)
        pub fn new(seed: u64) -> Self {
            RandomGen {
                state: seed,
            }
        }

        /// Линейный конгруэнтный генератор (LCG) для простоты
        pub fn next_u32(&mut self) -> u32 {
            self.state = self.state.wrapping_mul(6364136223846793005).wrapping_add(1);
            (self.state >> 32) as u32
        }

        pub fn next_u64(&mut self) -> u64 {
            let low = self.next_u32() as u64;
            let high = self.next_u32() as u64;
            (high << 32) | low
        }
    }
}
