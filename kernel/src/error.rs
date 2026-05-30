/// Kernel-specific error types.
///
/// This module defines a consistent error handling strategy for the kernel.
/// All kernel subsystems should use this error type for recoverable errors.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KernelError {
    /// Out of memory condition.
    OutOfMemory,
    /// Invalid input or parameter.
    InvalidInput,
    /// Resource not found.
    NotFound,
    /// Resource already exists.
    AlreadyExists,
    /// Operation not supported.
    NotSupported,
    /// Permission denied.
    PermissionDenied,
    /// Device or resource busy.
    Busy,
    /// Timeout occurred.
    Timeout,
    /// Internal kernel error.
    Internal,
    /// Filesystem specific errors.
    Fs(FsError),
    /// Memory allocation errors.
    Mem(MemError),
    /// Process related errors.
    Proc(ProcError),
    /// Scheduler errors.
    Sched(SchedError),
    /// Interrupt related errors.
    Int(IntError),
    /// Driver related errors.
    Drv(DrvError),
}

impl KernelError {
    /// Convert to a static string for logging or display.
    pub fn as_str(self) -> &'static str {
        match self {
            KernelError::OutOfMemory => "Out of memory",
            KernelError::InvalidInput => "Invalid input",
            KernelError::NotFound => "Not found",
            KernelError::AlreadyExists => "Already exists",
            KernelError::NotSupported => "Not supported",
            KernelError::PermissionDenied => "Permission denied",
            KernelError::Busy => "Busy",
            KernelError::Timeout => "Timeout",
            KernelError::Internal => "Internal error",
            KernelError::Fs(e) => e.as_str(),
            KernelError::Mem(e) => e.as_str(),
            KernelError::Proc(e) => e.as_str(),
            KernelError::Sched(e) => e.as_str(),
            KernelError::Int(e) => e.as_str(),
            KernelError::Drv(e) => e.as_str(),
        }
    }
}

/// Filesystem specific errors.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FsError {
    /// File or directory not found.
    NotFound,
    /// File or directory already exists.
    AlreadyExists,
    /// Not a directory.
    NotADir,
    /// Is a directory (when expecting a file).
    IsADir,
    /// Invalid filename or path.
    InvalidName,
    /// Read-only filesystem.
    ReadOnly,
    /// Filesystem full.
    Full,
    /// Corrupted filesystem.
    Corrupted,
    /// Unsupported operation.
    Unsupported,
}

impl FsError {
    pub fn as_str(self) -> &'static str {
        match self {
            FsError::NotFound => "File or directory not found",
            FsError::AlreadyExists => "File or directory already exists",
            FsError::NotADir => "Not a directory",
            FsError::IsADir => "Is a directory",
            FsError::InvalidName => "Invalid name",
            FsError::ReadOnly => "Read-only filesystem",
            FsError::Full => "Filesystem full",
            FsError::Corrupted => "Corrupted filesystem",
            FsError::Unsupported => "Unsupported operation",
        }
    }
}

/// Memory management errors.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MemError {
    /// Out of physical memory.
    OutOfMemory,
    /// Invalid address or alignment.
    InvalidAddress,
    /// Page table allocation failed.
    PageTableAllocFailed,
    /// Invalid memory region.
    InvalidRegion,
    /// Protection fault.
    ProtectionFault,
    /// Page fault in kernel space.
    KernelPageFault,
}

impl MemError {
    pub fn as_str(self) -> &'static str {
        match self {
            MemError::OutOfMemory => "Out of physical memory",
            MemError::InvalidAddress => "Invalid address or alignment",
            MemError::PageTableAllocFailed => "Page table allocation failed",
            MemError::InvalidRegion => "Invalid memory region",
            MemError::ProtectionFault => "Protection fault",
            MemError::KernelPageFault => "Page fault in kernel space",
        }
    }
}

/// Process related errors.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProcError {
    /// Process not found.
    NotFound,
    /// Invalid process state.
    InvalidState,
    /// Exceeded maximum number of processes.
    LimitExceeded,
    /// Invalid executable format.
    InvalidBinary,
    /// Missing required capability.
    MissingCapability,
}

impl ProcError {
    pub fn as_str(self) -> &'static str {
        match self {
            ProcError::NotFound => "Process not found",
            ProcError::InvalidState => "Invalid process state",
            ProcError::LimitExceeded => "Exceeded maximum number of processes",
            ProcError::InvalidBinary => "Invalid executable format",
            ProcError::MissingCapability => "Missing required capability",
        }
    }
}

/// Scheduler errors.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SchedError {
    /// Invalid task state.
    InvalidState,
    /// Task not found.
    NotFound,
    /// Scheduler not initialized.
    NotInitialized,
    /// Invalid priority.
    InvalidPriority,
}

impl SchedError {
    pub fn as_str(self) -> &'static str {
        match self {
            SchedError::InvalidState => "Invalid task state",
            SchedError::NotFound => "Task not found",
            SchedError::NotInitialized => "Scheduler not initialized",
            SchedError::InvalidPriority => "Invalid priority",
        }
    }
}

/// Interrupt related errors.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IntError {
    /// Invalid interrupt number.
    InvalidIrq,
    /// Interrupt handler not registered.
    HandlerNotRegistered,
    /// Spurious interrupt.
    Spurious,
}

impl IntError {
    pub fn as_str(self) -> &'static str {
        match self {
            IntError::InvalidIrq => "Invalid interrupt number",
            IntError::HandlerNotRegistered => "Interrupt handler not registered",
            IntError::Spurious => "Spurious interrupt",
        }
    }
}

/// Driver related errors.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DrvError {
    /// Device not found.
    NotFound,
    /// Device already registered.
    AlreadyRegistered,
    /// Device busy.
    Busy,
    /// Invalid device configuration.
    InvalidConfig,
    /// Operation failed.
    Failed,
}

impl DrvError {
    pub fn as_str(self) -> &'static str {
        match self {
            DrvError::NotFound => "Device not found",
            DrvError::AlreadyRegistered => "Device already registered",
            DrvError::Busy => "Device busy",
            DrvError::InvalidConfig => "Invalid device configuration",
            DrvError::Failed => "Operation failed",
        }
    }
}

// Implement From conversions for easier error propagation
impl From<FsError> for KernelError {
    fn from(err: FsError) -> Self {
        KernelError::Fs(err)
    }
}

impl From<MemError> for KernelError {
    fn from(err: MemError) -> Self {
        KernelError::Mem(err)
    }
}

impl From<ProcError> for KernelError {
    fn from(err: ProcError) -> Self {
        KernelError::Proc(err)
    }
}

impl From<SchedError> for KernelError {
    fn from(err: SchedError) -> Self {
        KernelError::Sched(err)
    }
}

impl From<IntError> for KernelError {
    fn from(err: IntError) -> Self {
        KernelError::Int(err)
    }
}

impl From<DrvError> for KernelError {
    fn from(err: DrvError) -> Self {
        KernelError::Drv(err)
    }
}

// For compatibility with existing code that uses &'static str, we provide a
// conversion Note: This is lossy and should be avoided in new code.
impl From<KernelError> for &'static str {
    fn from(err: KernelError) -> Self {
        err.as_str()
    }
}
