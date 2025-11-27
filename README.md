# swmr-barrier: Asymmetric Heavy-Light Barrier

[![Crates.io](https://img.shields.io/crates/v/swmr-barrier.svg)](https://crates.io/crates/swmr-barrier)
[![License](https://img.shields.io/crates/l/swmr-barrier.svg)](https://github.com/ShaoG-R/swmr-barrier#license)
[![Docs.rs](https://docs.rs/swmr-barrier/badge.svg)](https://docs.rs/swmr-barrier)

[中文文档](./README_CN.md)

`swmr-barrier` provides an asymmetric memory barrier for **Single-Writer Multi-Reader (SWMR)** scenarios. It implements a **Heavy Barrier** for the writer (cold path) and a **Light Barrier** for readers (hot path).

On supported platforms (Linux & Windows), the Light Barrier compiles down to a mere **compiler fence** with **zero runtime instruction overhead**, while the Heavy Barrier uses OS APIs to ensure global memory visibility.

## Features

- **Zero-Cost Readers**: On supported platforms, `light_barrier()` has no runtime CPU instructions (just a compiler fence).
- **OS-Hardware Acceleration**:
  - **Linux**: Uses `membarrier` (Private Expedited).
  - **Windows**: Uses `FlushProcessWriteBuffers`.
- **Automatic Fallback**: Safely degrades to `std::sync::atomic::fence(SeqCst)` on unsupported platforms (macOS, older Linux/Windows) or if runtime initialization fails.
- **Loom Support**: Built-in support for [Loom](https://github.com/tokio-rs/loom) concurrency testing.

## Usage

Add this to your `Cargo.toml`:

```toml
[dependencies]
swmr-barrier = "0.1"
```

### Example

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

    // Writer Thread (Cold Path)
    let writer = thread::spawn(move || {
        x_writer.store(1, Ordering::Relaxed);
        
        // Heavy Barrier: Ensures X is visible before Y
        // Cost: High (IPI on Linux, System Call on Windows)
        heavy_barrier();
        
        y_writer.store(1, Ordering::Relaxed);
    });

    // Reader Thread (Hot Path)
    let reader = thread::spawn(move || {
        // Wait for Y to be set
        while y.load(Ordering::Relaxed) == 0 {
            std::hint::spin_loop();
        }

        // Light Barrier: Ensures if we see Y, we must see X
        // Cost: Zero (Compiler Fence only) on supported platforms
        light_barrier();

        let x_val = x.load(Ordering::Relaxed);
        assert_eq!(x_val, 1, "X must be 1 if Y is 1");
    });

    writer.join().unwrap();
    reader.join().unwrap();
}
```

## Platform Support

| Platform | Implementation | Overhead (Reader) | Overhead (Writer) |
|----------|----------------|-------------------|-------------------|
| **Linux** (Kernel 4.14+) | `membarrier(CMD_PRIVATE_EXPEDITED)` | **Zero** (Compiler Fence) | High (IPI Broadcast) |
| **Windows** (Vista+) | `FlushProcessWriteBuffers` | **Zero** (Compiler Fence) | High (System Call) |
| **macOS / Others** | `fence(SeqCst)` | High (CPU Fence) | High (CPU Fence) |
| **Loom** | `loom::sync::atomic::fence` | Simulated | Simulated |

*Note: On Linux, the crate automatically detects support for `membarrier` at runtime. If not supported, it falls back to `fence(SeqCst)`.*

## Loom Testing

To use with Loom, enable the `loom` feature:

```bash
cargo test --features loom
```

## License

This project is licensed under either of

 * Apache License, Version 2.0, ([LICENSE-APACHE](LICENSE-APACHE) or http://www.apache.org/licenses/LICENSE-2.0)
 * MIT license ([LICENSE-MIT](LICENSE-MIT) or http://opensource.org/licenses/MIT)

at your option.
