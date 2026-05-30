# Instructions to add fork/exec to syscall.rs

## Step 1: Add to dispatch table (around line 294)
Find this line:
```
        16 => sys_kill(a1 as u32, a2 as u8),
        _ => ENOSYS,
```

Replace with:
```
        16 => sys_kill(a1 as u32, a2 as u8),
        17 => sys_fork(),
        18 => sys_exec(a1 as *const u8, a2 as usize, a3 as *const u8, a4 as usize),
        _ => ENOSYS,
```

## Step 2: Add implementations at the end of the file (after sys_kill function)

Add these functions:

```rust
/// sys_fork() — creates a new process as a copy of the current one
unsafe fn sys_fork() -> i64 {
    let parent_pid = crate::scheduler::CURRENT_PID[0].load(core::sync::atomic::Ordering::Relaxed);
    
    if let Some(parent_proc) = crate::process::find_process(parent_pid) {
        let parent_guard = parent_proc.lock();
        let child_name = alloc::format!("{}-child", parent_guard.name);
        drop(parent_guard);
        
        // Create child process
        let child_proc = crate::process::create_child_process(child_name, parent_pid);
        let child_pid = child_proc.lock().pid;
        
        // In a real implementation, we would copy the parent's memory space,
        // page tables, file descriptors, etc. For now, just return the child PID.
        child_pid as i64
    } else {
        EPERM
    }
}

/// sys_exec(path, path_len, args, args_len) — execute a new program
unsafe fn sys_exec(path_ptr: *const u8, path_len: usize, _args_ptr: *const u8, _args_len: usize) -> i64 {
    let path = match user_str(path_ptr, path_len) {
        Some(p) => alloc::string::String::from(p),
        None => return EFAULT,
    };
    
    // Validate that the file exists
    if crate::vfs::GLOBAL_VFS.lock().read_file(&path).is_err() {
        return ENOENT;
    }
    
    // Get current process
    let current_pid = crate::scheduler::CURRENT_PID[0].load(core::sync::atomic::Ordering::Relaxed);
    if let Some(proc_arc) = crate::process::find_process(current_pid) {
        let mut proc_guard = proc_arc.lock();
        
        // In a real implementation, we would:
        // 1. Load the ELF binary from the path
        // 2. Create a new memory space
        // 3. Map the ELF into the address space
        // 4. Set up the stack with arguments
        // 5. Update the entry point
        // For now, just log the attempt
        proc_guard.entry_point = Some(x86_64::VirtAddr::new(0x400000));
        
        0 // Success
    } else {
        EPERM
    }
}
```
