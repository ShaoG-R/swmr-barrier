// ============================================================================
// Platform-specific memory barrier implementations
// 平台特定的内存屏障实现
// ============================================================================

#[cfg(feature = "loom")]
mod impl_ {
    //! Loom simulation implementation
    //! Loom 模拟实现

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
        // If we only use compiler_fence here, Loom will not see a synchronization relationship.
        //
        // 注意：在 Loom 中，如果 Heavy 用了 fence，Light 必须也用 fence。
        // 如果这里只用 compiler_fence，Loom 会认为两条线程没有同步关系。
        loom::sync::atomic::fence(loom::sync::atomic::Ordering::SeqCst);
    }
}

#[cfg(not(feature = "loom"))]
mod impl_ {
    //! Real implementation using the `membarrier` crate.
    //! 使用 `membarrier` crate 的真实实现。
    //!
    //! The membarrier crate handles all platforms internally:
    //! - Linux: sys_membarrier() with mprotect() fallback
    //! - Windows: FlushProcessWriteBuffers()
    //! - Others: SeqCst fence fallback
    //!
    //! membarrier crate 内部处理所有平台：
    //! - Linux：sys_membarrier()，回退到 mprotect()
    //! - Windows：FlushProcessWriteBuffers()
    //! - 其他：SeqCst fence 回退

    #[inline]
    pub(crate) fn heavy_barrier_impl() {
        membarrier::heavy();
    }

    #[inline]
    pub(crate) fn light_barrier_impl() {
        membarrier::light();
    }
}

pub(crate) use impl_::*;