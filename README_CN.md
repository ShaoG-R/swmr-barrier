# swmr-barrier: 非对称 Heavy-Light 屏障

[![Crates.io](https://img.shields.io/crates/v/swmr-barrier)](https://crates.io/crates/swmr-barrier)
[![Documentation](https://docs.rs/swmr-barrier/badge.svg)](https://docs.rs/swmr-barrier)
[![License](https://img.shields.io/crates/l/swmr-barrier)](LICENSE-MIT)

[English Documentation](./README.md)

`swmr-barrier` 为 **单写多读 (SWMR)** 场景提供了一种非对称内存屏障。它实现了用于写入者（冷路径）的 **Heavy Barrier (重型屏障)** 和用于读取者（热路径）的 **Light Barrier (轻型屏障)**。

在支持的平台（Linux 和 Windows）上，轻型屏障会编译为仅一个 **编译器屏障 (compiler fence)**，具有 **零运行时指令开销**，而重型屏障则利用 OS API 来确保全局内存可见性。

## 特性

- **零开销读取**：在支持的平台上，`light_barrier()` 没有运行时 CPU 指令（仅编译器屏障）。
- **OS 硬件加速**：
  - **Linux**：直接通过 `libc` 调用 `syscall(SYS_membarrier, PRIVATE_EXPEDITED)`。
  - **Windows**：使用 `FlushProcessWriteBuffers`。
- **自动回退**：在不支持的平台（macOS、旧版 Linux 内核、旧版 Windows）或运行时初始化失败时，安全退化为 `std::sync::atomic::fence(SeqCst)`。
- **Loom 支持**：内置支持 [Loom](https://github.com/tokio-rs/loom) 并发测试。

## 使用方法

在 `Cargo.toml` 中添加：

```toml
[dependencies]
swmr-barrier = "0.1"
```

### 示例

```rust
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::thread;
use swmr_barrier::{heavy_barrier, light_barrier};

fn main() {
    let x = Arc::new(AtomicUsize::new(0));
    let y = Arc::new(AtomicUsize::new(0));

    let x_writer = x.clone();
    let y_writer = y.clone();

    // 写入线程（冷路径）
    let writer = thread::spawn(move || {
        x_writer.store(1, Ordering::Relaxed);
        
        // Heavy Barrier: 确保 X 在 Y 之前可见
        // 开销：高（Linux 上为 IPI，Windows 上为系统调用）
        heavy_barrier();
        
        y_writer.store(1, Ordering::Relaxed);
    });

    // 读取线程（热路径）
    let reader = thread::spawn(move || {
        // 等待 Y 被设置
        while y.load(Ordering::Relaxed) == 0 {
            std::hint::spin_loop();
        }

        // Light Barrier: 确保如果我们看到了 Y，那么必须能看到 X
        // 开销：在支持的平台上为零（仅编译器屏障）
        light_barrier();

        let x_val = x.load(Ordering::Relaxed);
        assert_eq!(x_val, 1, "如果 Y 是 1，X 必须是 1");
    });

    writer.join().unwrap();
    reader.join().unwrap();
}
```

## 平台支持

| 平台 | 实现方式 | 开销 (读取者) | 开销 (写入者) |
|----------|----------------|-------------------|-------------------|
| **Linux** (Kernel 4.14+) | `syscall(SYS_membarrier, PRIVATE_EXPEDITED)` | **零** (编译器屏障) | 高 (IPI 广播) |
| **Linux** (旧内核) | `fence(SeqCst)` 回退 | 高 (CPU 屏障) | 高 (CPU 屏障) |
| **Windows** (Vista+) | `FlushProcessWriteBuffers` | **零** (编译器屏障) | 高 (系统调用) |
| **macOS / 其他** | `fence(SeqCst)` | 高 (CPU 屏障) | 高 (CPU 屏障) |
| **Loom** | `loom::sync::atomic::fence` | 模拟 | 模拟 |

*注意：本库直接使用 `libc` 调用 `syscall(SYS_membarrier, ...)` 系统调用，在运行时自动检测内核支持。不支持 `MEMBARRIER_CMD_PRIVATE_EXPEDITED` 的旧版 Linux 内核（< 4.14）将回退到 `fence(SeqCst)`。*

## Loom 测试

要配合 Loom 使用，请启用 `loom` 特性：

```bash
cargo test --features loom
```

## 许可证

本项目采用以下任一许可证授权：

 * Apache License, Version 2.0, ([LICENSE-APACHE](LICENSE-APACHE) or http://www.apache.org/licenses/LICENSE-2.0)
 * MIT license ([LICENSE-MIT](LICENSE-MIT) or http://opensource.org/licenses/MIT)

由你选择。
