use alloc::{format, sync::Arc};
/// Тестовая инфраструктура для ядра ОС
///
/// Поскольку мы работаем в no_std окружении, стандартный test framework
/// недоступен. Этот модуль предоставляет базовую инфраструктуру для тестов.
use core::fmt;

use spin::Mutex;

use crate::window::Window;

/// Результат теста
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TestResult {
    Ok,
    Failed,
}

/// Информация о тесте
pub struct TestInfo {
    pub name: &'static str,
    pub result: TestResult,
    pub message: Option<&'static str>,
}

/// Trait для тестируемых функций
pub trait Testable {
    fn run(&self) -> TestResult;
    fn name(&self) -> &'static str;
}

/// Структура для хранения теста
pub struct Test {
    pub name: &'static str,
    pub test_fn: fn() -> TestResult,
}

impl Testable for Test {
    fn run(&self) -> TestResult {
        (self.test_fn)()
    }

    fn name(&self) -> &'static str {
        self.name
    }
}

/// Макрос для создания теста
#[macro_export]
macro_rules! test_case {
    ($name:expr, $test_fn:expr) => {
        $crate::test_framework::Test {
            name: $name,
            test_fn: $test_fn,
        }
    };
}

/// Макрос для assert в тестах
#[macro_export]
macro_rules! test_assert {
    ($cond:expr) => {
        if !$cond {
            return $crate::test_framework::TestResult::Failed;
        }
    };
    ($cond:expr, $msg:expr) => {
        if !$cond {
            return $crate::test_framework::TestResult::Failed;
        }
    };
}

/// Макрос для assert_eq в тестах
#[macro_export]
macro_rules! test_assert_eq {
    ($left:expr, $right:expr) => {
        if $left != $right {
            return $crate::test_framework::TestResult::Failed;
        }
    };
}

/// Макрос для assert_ne в тестах
#[macro_export]
macro_rules! test_assert_ne {
    ($left:expr, $right:expr) => {
        if $left == $right {
            return $crate::test_framework::TestResult::Failed;
        }
    };
}

/// Runner для запуска тестов
pub struct TestRunner {
    tests: &'static [Test],
}

impl TestRunner {
    pub const fn new(tests: &'static [Test]) -> Self {
        Self {
            tests,
        }
    }

    /// Запустить все тесты
    pub fn run_all(&self) -> TestStats {
        let mut stats = TestStats {
            total: self.tests.len(),
            passed: 0,
            failed: 0,
        };

        for test in self.tests {
            let result = test.run();
            match result {
                TestResult::Ok => stats.passed += 1,
                TestResult::Failed => stats.failed += 1,
            }
        }

        stats
    }

    /// Запустить конкретный тест по имени
    pub fn run_test(&self, name: &str) -> Option<TestResult> {
        for test in self.tests {
            if test.name() == name {
                return Some(test.run());
            }
        }
        None
    }

    /// Получить список всех тестов
    pub fn list_tests(&self) -> &[Test] {
        self.tests
    }
}

/// Статистика тестов
#[derive(Debug, Clone, Copy)]
pub struct TestStats {
    pub total: usize,
    pub passed: usize,
    pub failed: usize,
}

impl TestStats {
    pub fn success_rate(&self) -> f32 {
        if self.total == 0 {
            return 0.0;
        }
        (self.passed as f32 / self.total as f32) * 100.0
    }
}

impl fmt::Display for TestStats {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "Tests: {} total, {} passed, {} failed ({:.1}% success)",
            self.total,
            self.passed,
            self.failed,
            self.success_rate()
        )
    }
}

fn write_to_terminal(win: &Arc<Mutex<Window>>, text: &str) {
    if let Some(term) = crate::fast_terminal::TERMINAL.lock().as_mut() {
        term.print_str(&mut *win.lock(), text);
    }
}

/// Run all registered tests and display individual results in the terminal.
pub fn run_all_tests(win: &Arc<Mutex<Window>>) -> TestStats {
    let runner = crate::tests::create_test_runner();
    let mut stats = TestStats {
        total: runner.list_tests().len(),
        passed: 0,
        failed: 0,
    };

    for test in runner.list_tests() {
        match test.run() {
            TestResult::Ok => {
                stats.passed += 1;
                write_to_terminal(win, &format!("[ OK ] {}\n", test.name()));
            },
            TestResult::Failed => {
                stats.failed += 1;
                write_to_terminal(win, &format!("[FAIL] {}\n", test.name()));
            },
        }
    }

    write_to_terminal(win, &format!("{}\n", stats));
    stats
}

pub fn run_headless_tests() -> TestStats {
    crate::tests::create_test_runner().run_all()
}

#[cfg(feature = "test-mode")]
pub fn exit_qemu(success: bool) -> ! {
    use x86_64::instructions::port::Port;

    unsafe {
        Port::<u32>::new(0xf4).write(if success { 0x10 } else { 0x11 });
    }
    loop {
        x86_64::instructions::hlt();
    }
}
