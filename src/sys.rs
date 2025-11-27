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
// 2. Linux Real Implementation (Requires Runtime Status Check)
// 2. Linux 真实实现 (需要运行时状态检查)
// ============================================================================
    else if #[cfg(target_os = "linux")] {
        use std::sync::atomic::{fence, compiler_fence, Ordering, AtomicBool};

        // Flag: Whether the system supports and has successfully registered the Private Expedited acceleration mode.
        // 标记：系统是否支持并成功注册了 Private Expedited 加速模式
        static IS_ACCELERATED: AtomicBool = AtomicBool::new(false);

        // Use ctor to automatically try registering before main.
        // 使用 ctor 在 main 之前自动尝试注册
        #[ctor::ctor]
        fn linux_auto_init() {
            let res = membarrier::cmd::RegisterPrivateExpedited.exec();
            if res.is_ok() {
                IS_ACCELERATED.store(true, Ordering::Relaxed);
            }
        }

        #[inline]
        pub(crate) fn heavy_barrier_impl() {
            // Check global flag (Relaxed Load is extremely fast on x86/ARM).
            // 检查全局标志 (Relaxed Load 在 x86/ARM 上极快)
            if IS_ACCELERATED.load(Ordering::Relaxed) {
                unsafe {
                    // 1. Call kernel to broadcast interrupts.
                    // 1. 调用内核广播中断
                    let _ = membarrier::cmd::PrivateExpedited.exec();
                }
                // 2. Prevent local reordering.
                // 2. 防止本地重排
                compiler_fence(Ordering::SeqCst);
            } else {
                // Fallback: Traditional full barrier.
                // Fallback: 传统全屏障
                fence(Ordering::SeqCst);
            }
        }

        #[inline]
        pub(crate) fn light_barrier_impl() {
            // [Critical] Light Barrier must also check the status!
            // 【关键点】Light Barrier 也必须检查状态！
            if IS_ACCELERATED.load(Ordering::Relaxed) {
                // Best Path: Only prevent compiler reordering, no CPU instructions needed.
                // 最佳路径：只防止编译器重排，无需 CPU 指令
                compiler_fence(Ordering::SeqCst);
            } else {
                // Fallback Path: Must match the fence(SeqCst) on the Heavy side.
                // Otherwise, a global total order cannot be established.
                //
                // 回退路径：必须与 Heavy 端的 fence(SeqCst) 匹配
                // 否则无法构成全局全序
                fence(Ordering::SeqCst);
            }
        }
    }

// ============================================================================
// 3. Windows Real Implementation (Assumed to be always supported)
// 3. Windows 真实实现 (假定总是支持)
// ============================================================================
    else if #[cfg(target_os = "windows")] {
        use windows_sys::Win32::System::Threading::FlushProcessWriteBuffers;
        use std::sync::atomic::{compiler_fence, Ordering};

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