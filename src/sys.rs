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
    }

// ============================================================================
// 2. Linux Real Implementation (Direct libc)
// 2. Linux 真实实现 (直接使用 libc)
// ============================================================================
    else if #[cfg(target_os = "linux")] {
        use std::sync::atomic::{fence, compiler_fence, Ordering, AtomicBool};
        use libc::{syscall, c_int, c_long};

        // --------------------------------------------------------------------
        // Constants definition (from linux/membarrier.h)
        // 手动定义常量，防止某些 libc 版本缺失
        // --------------------------------------------------------------------
        // System call number should be provided by libc, but commands are constants.
        // 系统调用号通常由 libc 提供，但命令常量是固定的。
        const SYS_MEMBARRIER: c_long = libc::SYS_membarrier;

        const MEMBARRIER_CMD_QUERY: c_int = 0;
        const MEMBARRIER_CMD_PRIVATE_EXPEDITED: c_int = 8;
        const MEMBARRIER_CMD_REGISTER_PRIVATE_EXPEDITED: c_int = 16;

        // --------------------------------------------------------------------
        // State Management
        // --------------------------------------------------------------------
        // Flag: Whether Private Expedited mode is successfully registered.
        // 标志：私有加速模式是否已成功注册。
        static IS_ACCELERATED: AtomicBool = AtomicBool::new(false);

        // --------------------------------------------------------------------
        // Initialization (runs before main)
        // 初始化 (在 main 之前运行)
        // --------------------------------------------------------------------
        #[ctor::ctor]
        fn linux_auto_init() {
            unsafe {
                // Step 1: Check kernel support (Query)
                // 第一步：检查内核支持 (查询)
                let supported_mask = syscall(SYS_MEMBARRIER, MEMBARRIER_CMD_QUERY, 0, 0);
                if supported_mask < 0 {
                    return;
                }

                // Check if PRIVATE_EXPEDITED (8) is supported
                if (supported_mask as c_int & MEMBARRIER_CMD_PRIVATE_EXPEDITED) == 0 {
                    return;
                }

                // Step 2: Register
                // 第二步：注册
                // This tells the kernel to track this process for IPI barriers.
                // 告诉内核为当前进程追踪 IPI 屏障。
                let res = syscall(SYS_MEMBARRIER, MEMBARRIER_CMD_REGISTER_PRIVATE_EXPEDITED, 0, 0);

                if res == 0 {
                    IS_ACCELERATED.store(true, Ordering::Relaxed);
                }
            }
        }

        // --------------------------------------------------------------------
        // Barrier Implementations
        // --------------------------------------------------------------------

        #[inline]
        pub(crate) fn heavy_barrier_impl() {
            // Check if we are in accelerated mode
            // 检查是否处于加速模式
            if IS_ACCELERATED.load(Ordering::Relaxed) {
                unsafe {
                    // Trigger the IPI barrier
                    // 触发 IPI 屏障
                    let ret = syscall(SYS_MEMBARRIER, MEMBARRIER_CMD_PRIVATE_EXPEDITED, 0, 0);

                    // Safety net: if syscall fails (unlikely after registration), fallback.
                    // 安全网：如果 syscall 失败（注册后不太可能发生），回退。
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
            // CRITICAL: We must match the strategy of heavy_barrier.
            // If heavy barrier falls back to fence(SeqCst), we MUST also use fence(SeqCst).
            // A compiler_fence is only sufficient if the other side triggered a real hardware barrier (IPI).
            //
            // 关键：必须与 heavy_barrier 策略匹配。
            // 如果 heavy 屏障回退到了 fence(SeqCst)，我们也必须用 fence(SeqCst)。
            // 只有当另一端触发了真实的硬件屏障（IPI）时，编译器屏障才是足够的。
            if IS_ACCELERATED.load(Ordering::Relaxed) {
                compiler_fence(Ordering::SeqCst);
            } else {
                fence(Ordering::SeqCst);
            }
        }

        /// Returns whether OS-accelerated barriers (membarrier) are in use.
        /// 返回是否正在使用 OS 加速屏障（membarrier）。
        #[inline]
        pub(crate) fn is_accelerated_impl() -> bool {
            IS_ACCELERATED.load(Ordering::Relaxed)
        }
    }

// ============================================================================
// 3. Windows Real Implementation (Assumed to be always supported)
// 3. Windows 真实实现 (假定总是支持)
// ============================================================================
    else if #[cfg(target_os = "windows")] {
        use windows_sys::Win32::System::Threading::FlushProcessWriteBuffers;
        use core::sync::atomic::{compiler_fence, Ordering};

        // Windows Vista / Server 2008 and later support this API.
        // Unless you are running Windows XP, no runtime downgrade check is needed.
        //
        // Windows Vista / Server 2008 之后都支持此 API。
        // 除非你在跑 Windows XP，否则不需要做运行时降级检查。

        #[inline]
        pub(crate) fn heavy_barrier_impl() {
            unsafe { FlushProcessWriteBuffers(); }
            compiler_fence(Ordering::SeqCst);
        }

        #[inline]
        pub(crate) fn light_barrier_impl() {
            // Since Windows supports FlushProcessWriteBuffers,
            // the reader side only needs a compiler barrier.
            //
            // Windows 既然支持 FlushProcessWriteBuffers，
            // 那么读侧只需要编译器屏障即可。
            compiler_fence(Ordering::SeqCst);
        }
    }

// ============================================================================
// 4. Other Platforms / Fallback
// 4. 其他平台 / Fallback
// ============================================================================
    else {
        use std::sync::atomic::{fence, Ordering};

        #[inline]
        pub(crate) fn heavy_barrier_impl() {
            fence(Ordering::SeqCst);
        }

        #[inline]
        pub(crate) fn light_barrier_impl() {
            // No OS acceleration, both Reader and Writer must use heavy barriers.
            // 没有 OS 加速，读写两端都必须是重屏障
            fence(Ordering::SeqCst);
        }
    }
}
