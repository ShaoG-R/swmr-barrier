mod sys;

/// **Heavy Barrier**
///
/// Used for the cold path (Writer).
///
/// * **Best Case**: Calls OS API to forcibly flush all CPU caches (Linux PrivateExpedited / Windows FlushProcessWriteBuffers).
/// * **Fallback**: Degrades to `fence(Ordering::SeqCst)`.
///
/// ---
///
/// **重型屏障 (Heavy Barrier)**
///
/// 用于冷路径（Writer）。
///
/// * **最佳情况**：调用 OS API 强制刷新所有 CPU 缓存 (Linux PrivateExpedited / Windows FlushProcessWriteBuffers)。
/// * **回退情况**：退化为 `fence(Ordering::SeqCst)`。
#[inline]
pub fn heavy_barrier() {
    sys::heavy_barrier_impl();
}

/// **Light Barrier**
///
/// Used for the hot path (Reader).
///
/// * **Best Case**: Generates only a `compiler_fence(SeqCst)`. Runtime overhead is practically zero.
/// * **Fallback**: If the system does not support heavy barrier optimization, it must degrade to `fence(Ordering::SeqCst)` for safety.
///
/// ---
///
/// **轻型屏障 (Light Barrier)**
///
/// 用于热路径（Reader）。
///
/// * **最佳情况**：仅产生一个 `compiler_fence(SeqCst)`。运行时开销几乎为 0。
/// * **回退情况**：如果系统不支持重型屏障优化，必须退化为 `fence(Ordering::SeqCst)` 以保证安全。
#[inline]
pub fn light_barrier() {
    sys::light_barrier_impl();
}