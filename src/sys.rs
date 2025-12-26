use cfg_if::cfg_if;

// ============================================================================
// 1. Loom Simulation Implementation
// 1. Loom 模拟实现
// ============================================================================
cfg_if! {
    if #[cfg(feature = "loom")] {
        #[inline]
        pub(crate) fn heavy_barrier_impl() {
            // Loom cannot simulate "IPI Interrupts" or "Cache Coherency".
            // We can only establish a Happens-Before relationship using standard Atomic Fences.
            //
            // Loom 无法模拟 "IPI 中断" 或 "Cache Coherency"，
            // 只能通过标准的 Atomic Fence 建立 Happens-Before 关系。
            loom::sync::atomic::fence(loom::sync::atomic::Ordering::SeqCst);
        }

        #[inline]
        pub(crate) fn light_barrier_impl() {
            // Note: In Loom, if Heavy uses a fence, Light must also use a fence.
            // If we only use compiler_fence here, Loom will not see a synchronization relationship between the two threads.
            //
            // 注意：在 Loom 中，如果 Heavy 用了 fence，Light 必须也用 fence。
            // 如果这里只用 compiler_fence，Loom 会认为两条线程没有同步关系。
            loom::sync::atomic::fence(loom::sync::atomic::Ordering::SeqCst);
        }

        #[inline]
        pub(crate) fn is_accelerated_impl() -> bool {
            false
        }
    }

// ============================================================================
// 2. Linux Real Implementation (Direct libc)
// 2. Linux 真实实现 (直接使用 libc)
// ============================================================================
    else if #[cfg(target_os = "linux")] {
        use core::sync::atomic::{fence, compiler_fence, Ordering, AtomicI32};
        use libc::{syscall, c_int, c_long};

        // --------------------------------------------------------------------
        // Constants definition (from linux/membarrier.h)
        // 手动定义常量，防止某些 libc 版本缺失
        // --------------------------------------------------------------------
        // System call number should be provided by libc, but commands are constants.
        // 系统调用号通常由 libc 提供，但命令常量是固定的。
        const SYS_MEMBARRIER: c_long = libc::SYS_membarrier;

        const MEMBARRIER_CMD_QUERY: c_int = 0;
        const MEMBARRIER_CMD_SHARED: c_int = 1;
        const MEMBARRIER_CMD_PRIVATE_EXPEDITED: c_int = 8;
        const MEMBARRIER_CMD_REGISTER_PRIVATE_EXPEDITED: c_int = 16;

        // --------------------------------------------------------------------
        // State Management
        // --------------------------------------------------------------------
        // Store the membarrier command to use (0 = disabled/fallback, 1 = SHARED, 8 = PRIVATE_EXPEDITED)
        // 存储要使用的 membarrier 命令 (0 = 禁用/回退, 1 = SHARED, 8 = PRIVATE_EXPEDITED)
        static MEMBARRIER_CMD: AtomicI32 = AtomicI32::new(0);

        // --------------------------------------------------------------------
        // Initialization (runs before main)
        // 初始化 (在 main 之前运行)
        // --------------------------------------------------------------------
        #[used]
        #[unsafe(link_section = ".init_array")]
        static __INIT: extern "C" fn() = linux_auto_init;

        extern "C" fn linux_auto_init() {
            unsafe {
                // Step 1: Check kernel support (Query)
                // 第一步：检查内核支持 (查询)
                let supported_mask = syscall(SYS_MEMBARRIER, MEMBARRIER_CMD_QUERY, 0, 0);
                if supported_mask < 0 {
                    return;
                }

                // Strategy 1: PRIVATE_EXPEDITED (Linux 4.14+)
                // Best performance, requires registration.
                // 策略 1: PRIVATE_EXPEDITED (Linux 4.14+)
                // 性能最佳，需要注册。
                if (supported_mask as c_int & MEMBARRIER_CMD_PRIVATE_EXPEDITED) != 0 {
                    let res = syscall(SYS_MEMBARRIER, MEMBARRIER_CMD_REGISTER_PRIVATE_EXPEDITED, 0, 0);
                    if res == 0 {
                        MEMBARRIER_CMD.store(MEMBARRIER_CMD_PRIVATE_EXPEDITED, Ordering::Relaxed);
                        return;
                    }
                }

                // Strategy 2: SHARED (Linux 4.3+)
                // Fallback for older kernels. Slower than PRIVATE_EXPEDITED but still asymmetric (good for readers).
                // 策略 2: SHARED (Linux 4.3+)
                // 旧内核的回退方案。比 PRIVATE_EXPEDITED 慢，但在读侧依然是非对称的（对读者友好）。
                if (supported_mask as c_int & MEMBARRIER_CMD_SHARED) != 0 {
                    MEMBARRIER_CMD.store(MEMBARRIER_CMD_SHARED, Ordering::Relaxed);
                    return;
                }
            }
        }

        // --------------------------------------------------------------------
        // Barrier Implementations
        // --------------------------------------------------------------------

        #[inline]
        pub(crate) fn heavy_barrier_impl() {
            let cmd = MEMBARRIER_CMD.load(Ordering::Relaxed);

            // Check if we are in accelerated mode
            // 检查是否处于加速模式
            if cmd != 0 {
                unsafe {
                    // Trigger the IPI barrier (PRIVATE_EXPEDITED or SHARED)
                    // 触发 IPI 屏障 (PRIVATE_EXPEDITED 或 SHARED)
                    let ret = syscall(SYS_MEMBARRIER, cmd, 0, 0);

                    // Safety net
                    // 安全网
                    if ret != 0 {
                        fence(Ordering::SeqCst);
                    }
                }
                // Prevent compiler reordering locally
                // 防止本地编译器重排
                compiler_fence(Ordering::SeqCst);
            } else {
                // Fallback: Standard heavy fence
                // 回退：标准全屏障
                fence(Ordering::SeqCst);
            }
        }

        #[inline]
        pub(crate) fn light_barrier_impl() {
            // CRITICAL: Match the heavy_barrier strategy.
            // 关键：必须与 heavy_barrier 策略匹配。
            if MEMBARRIER_CMD.load(Ordering::Relaxed) != 0 {
                compiler_fence(Ordering::SeqCst);
            } else {
                fence(Ordering::SeqCst);
            }
        }

        /// Returns whether OS-accelerated barriers (membarrier) are in use.
        /// 返回是否正在使用 OS 加速屏障（membarrier）。
        #[inline]
        pub(crate) fn is_accelerated_impl() -> bool {
            MEMBARRIER_CMD.load(Ordering::Relaxed) != 0
        }
    }

// ============================================================================
// 3. Windows Implementation
// 3. Windows 实现
// ============================================================================
    else if #[cfg(target_os = "windows")] {
        use windows_sys::Win32::System::LibraryLoader::{GetModuleHandleA, GetProcAddress};
        use core::sync::atomic::{compiler_fence, fence, AtomicBool, AtomicPtr, Ordering};
        use core::ffi::c_void;

        // --------------------------------------------------------------------
        // State Management
        // --------------------------------------------------------------------
        static IS_ACCELERATED: AtomicBool = AtomicBool::new(false);
        static MB_FN_PTR: AtomicPtr<c_void> = AtomicPtr::new(core::ptr::null_mut());

        // Function signature for FlushProcessWriteBuffers
        type FnFlushProcessWriteBuffers = unsafe extern "system" fn();

        // --------------------------------------------------------------------
        // Initialization (runs before main)
        // 初始化 (在 main 之前运行)
        // --------------------------------------------------------------------
        // On Windows MSVC, .CRT$XCU is the section for C++ dynamic initializers.
        // Rust uses this for its own pre-main code.
        #[used]
        #[unsafe(link_section = ".CRT$XCU")]
        static __INIT: extern "C" fn() = windows_auto_init;

        extern "C" fn windows_auto_init() {
            unsafe {
                // 1. Get readable handle to Kernel32.dll (already loaded)
                let h_kernel32 = GetModuleHandleA(b"kernel32.dll\0".as_ptr());
                if h_kernel32.is_null() {
                    return;
                }

                // 2. Try to find FlushProcessWriteBuffers
                // It is available on Vista / Server 2008 and later.
                if let Some(func_ptr) = GetProcAddress(h_kernel32, b"FlushProcessWriteBuffers\0".as_ptr()) {
                    // Store the function pointer
                    // Transmute the FARPROC to *mut c_void for storage
                    MB_FN_PTR.store(func_ptr as *mut c_void, Ordering::Relaxed);

                    // Enable acceleration
                    IS_ACCELERATED.store(true, Ordering::Relaxed);
                }
            }
        }

        #[inline]
        pub(crate) fn heavy_barrier_impl() {
            // Check if we have the accelerated function
            if IS_ACCELERATED.load(Ordering::Relaxed) {
                unsafe {
                    let ptr = MB_FN_PTR.load(Ordering::Relaxed);
                    if !ptr.is_null() {
                        let func: FnFlushProcessWriteBuffers = core::mem::transmute(ptr);
                        func();
                    }
                }
                compiler_fence(Ordering::SeqCst);
            } else {
                // Fallback for XP / Server 2003 or if detection failed
                fence(Ordering::SeqCst);
            }
        }

        #[inline]
        pub(crate) fn light_barrier_impl() {
            if IS_ACCELERATED.load(Ordering::Relaxed) {
                compiler_fence(Ordering::SeqCst);
            } else {
                fence(Ordering::SeqCst);
            }
        }

        /// Returns whether OS-accelerated barriers are in use.
        #[inline]
        pub(crate) fn is_accelerated_impl() -> bool {
            IS_ACCELERATED.load(Ordering::Relaxed)
        }
    }

// ============================================================================
// 4. Other Platforms / Fallback
// 4. 其他平台 / Fallback
// ============================================================================
    else {
        use core::sync::atomic::{fence, Ordering};

        #[inline]
        pub(crate) fn heavy_barrier_impl() {
            fence(Ordering::SeqCst);
        }

        #[inline]
        pub(crate) fn light_barrier_impl() {
            // No OS acceleration, both Reader and Writer must use heavy barriers.
            // 没有 OS 加速，读写两端都必须是重屏障
            // 没有 OS 加速，读写两端都必须是重屏障
            fence(Ordering::SeqCst);
        }

        #[inline]
        pub(crate) fn is_accelerated_impl() -> bool {
            false
        }
    }
}
