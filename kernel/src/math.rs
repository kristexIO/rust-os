// Модуль для базовой математики в no_std среде
// Вместо использования libm мы реализуем простые приближения

/// Приближенное значение квадратного корня (метод Ньютона)
pub fn sqrt_approx(x: f32) -> f32 {
    if x == 0.0 {
        return 0.0;
    }
    if x < 0.0 {
        return 0.0; // Вместо NaN возвращаем 0
    }

    let mut guess = x / 2.0;
    for _ in 0..10 {
        // 10 итераций для сходимости
        guess = (guess + x / guess) / 2.0;
    }
    guess
}

/// Приближенное значение синуса с использованием ряда Тейлора
pub fn sin_approx(x: f32) -> f32 {
    let TWO_PI = 2.0 * core::f32::consts::PI;
    let x = x - (x / TWO_PI) as i32 as f32 * TWO_PI;
    let x = if x > core::f32::consts::PI {
        x - 2.0 * core::f32::consts::PI
    } else if x < -core::f32::consts::PI {
        x + 2.0 * core::f32::consts::PI
    } else {
        x
    };

    let x2 = x * x;
    x - x * x2 / 6.0 + x2 * x2 * x / 120.0 - x2 * x2 * x2 / 5040.0
}

/// Приближенное значение косинуса через синус и фазу
pub fn cos_approx(x: f32) -> f32 {
    sin_approx(x + core::f32::consts::FRAC_PI_2)
}

/// Приближенное значение тангенса
pub fn tan_approx(x: f32) -> f32 {
    sin_approx(x) / cos_approx(x)
}

// Вспомогательные функции для работы с f32
pub trait FloatExt {
    fn sqrt(self) -> Self;
    fn sin(self) -> Self;
    fn cos(self) -> Self;
    fn tan(self) -> Self;
    fn to_radians(self) -> Self;
}

impl FloatExt for f32 {
    fn sqrt(self) -> Self {
        sqrt_approx(self)
    }

    fn sin(self) -> Self {
        sin_approx(self)
    }

    fn cos(self) -> Self {
        cos_approx(self)
    }

    fn tan(self) -> Self {
        tan_approx(self)
    }

    fn to_radians(self) -> Self {
        self * core::f32::consts::PI / 180.0
    }
}
