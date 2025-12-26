//! Integration tests for validating real-world barrier effectiveness.
//!
//! These tests run on actual hardware (not Loom simulation) to verify
//! that heavy_barrier + light_barrier provide correct synchronization
//! across different platforms (Linux, Windows, macOS).

#![cfg(not(feature = "loom"))]

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::thread;
use swmr_barrier::{heavy_barrier, light_barrier};

/// Number of iterations for stress tests.
/// Higher values increase the chance of catching race conditions.
const ITERATIONS: usize = 100_000;

/// Number of reader threads for multi-reader tests.
const NUM_READERS: usize = 4;

/// Basic test: Verify ordering between heavy_barrier (writer) and light_barrier (reader).
///
/// Pattern:
/// - Writer: store X -> heavy_barrier -> store Y
/// - Reader: load Y -> light_barrier -> load X
///
/// Invariant: If reader sees Y=1, it must also see X=1.
#[test]
fn test_basic_ordering() {
    for _ in 0..ITERATIONS {
        let x = Arc::new(AtomicUsize::new(0));
        let y = Arc::new(AtomicUsize::new(0));

        let x_writer = x.clone();
        let y_writer = y.clone();

        let writer = thread::spawn(move || {
            x_writer.store(1, Ordering::Relaxed);
            heavy_barrier();
            y_writer.store(1, Ordering::Relaxed);
        });

        let x_reader = x.clone();
        let y_reader = y.clone();

        let reader = thread::spawn(move || {
            let r_y = y_reader.load(Ordering::Relaxed);
            light_barrier();
            let r_x = x_reader.load(Ordering::Relaxed);

            // If we see the second store (Y=1), we must see the first store (X=1)
            if r_y == 1 {
                assert_eq!(r_x, 1, "Barrier violation: saw Y=1 but X=0");
            }
        });

        writer.join().unwrap();
        reader.join().unwrap();
    }
}

/// Multi-reader test: One writer with multiple concurrent readers.
///
/// This tests the Single-Writer Multi-Reader (SWMR) scenario.
#[test]
fn test_swmr_ordering() {
    for _ in 0..(ITERATIONS / 10) {
        let x = Arc::new(AtomicUsize::new(0));
        let y = Arc::new(AtomicUsize::new(0));

        let x_writer = x.clone();
        let y_writer = y.clone();

        let writer = thread::spawn(move || {
            x_writer.store(1, Ordering::Relaxed);
            heavy_barrier();
            y_writer.store(1, Ordering::Relaxed);
        });

        let readers: Vec<_> = (0..NUM_READERS)
            .map(|_| {
                let x_reader = x.clone();
                let y_reader = y.clone();

                thread::spawn(move || {
                    let r_y = y_reader.load(Ordering::Relaxed);
                    light_barrier();
                    let r_x = x_reader.load(Ordering::Relaxed);

                    if r_y == 1 {
                        assert_eq!(r_x, 1, "SWMR barrier violation: saw Y=1 but X=0");
                    }
                })
            })
            .collect();

        writer.join().unwrap();
        for reader in readers {
            reader.join().unwrap();
        }
    }
}

/// Seqlock-like pattern test: Version + Data consistency.
///
/// This simulates a common SWMR pattern where:
/// - Writer: update data -> heavy_barrier -> increment version
/// - Reader: read version -> light_barrier -> read data -> check version
#[test]
fn test_seqlock_pattern() {
    let version = Arc::new(AtomicUsize::new(0));
    let data = Arc::new(AtomicUsize::new(0));
    let stop = Arc::new(AtomicUsize::new(0));

    let ver_writer = version.clone();
    let data_writer = data.clone();
    let stop_writer = stop.clone();

    let writer = thread::spawn(move || {
        for i in 1..=ITERATIONS {
            // Update data first
            data_writer.store(i, Ordering::Relaxed);
            heavy_barrier();
            // Then publish version
            ver_writer.store(i, Ordering::Relaxed);
        }
        stop_writer.store(1, Ordering::Relaxed);
    });

    let readers: Vec<_> = (0..NUM_READERS)
        .map(|_| {
            let ver_reader = version.clone();
            let data_reader = data.clone();
            let stop_reader = stop.clone();

            thread::spawn(move || {
                let mut reads = 0u64;
                loop {
                    let v1 = ver_reader.load(Ordering::Relaxed);
                    light_barrier();
                    let d = data_reader.load(Ordering::Relaxed);

                    // If we see version V, data must be >= V
                    // (data is stored before version is incremented)
                    if v1 > 0 {
                        assert!(
                            d >= v1,
                            "Seqlock violation: version={} but data={} (data < version)",
                            v1,
                            d
                        );
                    }

                    reads += 1;

                    if stop_reader.load(Ordering::Relaxed) == 1 {
                        break;
                    }
                }
                reads
            })
        })
        .collect();

    writer.join().unwrap();

    let total_reads: u64 = readers.into_iter().map(|r| r.join().unwrap()).sum();
    println!("Seqlock test completed with {} total reads", total_reads);
}

/// Multi-variable ordering test: Verify ordering across multiple variables.
///
/// Writer stores a, b, c in order with heavy_barrier after each.
/// Reader reads in reverse order with light_barrier between reads.
#[test]
fn test_multi_variable_ordering() {
    for _ in 0..(ITERATIONS / 10) {
        let a = Arc::new(AtomicUsize::new(0));
        let b = Arc::new(AtomicUsize::new(0));
        let c = Arc::new(AtomicUsize::new(0));

        let (a_w, b_w, c_w) = (a.clone(), b.clone(), c.clone());

        let writer = thread::spawn(move || {
            a_w.store(1, Ordering::Relaxed);
            heavy_barrier();
            b_w.store(1, Ordering::Relaxed);
            heavy_barrier();
            c_w.store(1, Ordering::Relaxed);
        });

        let (a_r, b_r, c_r) = (a.clone(), b.clone(), c.clone());

        let reader = thread::spawn(move || {
            let r_c = c_r.load(Ordering::Relaxed);
            light_barrier();
            let r_b = b_r.load(Ordering::Relaxed);
            light_barrier();
            let r_a = a_r.load(Ordering::Relaxed);

            // If we see C=1, we must see B=1 and A=1
            if r_c == 1 {
                assert_eq!(r_b, 1, "Multi-var violation: saw C=1 but B=0");
                assert_eq!(r_a, 1, "Multi-var violation: saw C=1 but A=0");
            }
            // If we see B=1, we must see A=1
            if r_b == 1 {
                assert_eq!(r_a, 1, "Multi-var violation: saw B=1 but A=0");
            }
        });

        writer.join().unwrap();
        reader.join().unwrap();
    }
}

/// Ping-pong test: Alternating reads and writes between threads.
///
/// This tests bidirectional synchronization where both sides use barriers.
#[test]
fn test_ping_pong() {
    let flag = Arc::new(AtomicUsize::new(0));
    let data = Arc::new(AtomicUsize::new(0));

    let flag_a = flag.clone();
    let data_a = data.clone();

    let flag_b = flag.clone();
    let data_b = data.clone();

    const ROUNDS: usize = 10_000;

    let thread_a = thread::spawn(move || {
        for i in 0..ROUNDS {
            // Wait for our turn (flag == 2*i)
            loop {
                let f = flag_a.load(Ordering::Relaxed);
                light_barrier();
                if f == 2 * i {
                    break;
                }
                std::hint::spin_loop();
            }

            // Write data and signal
            data_a.store(2 * i + 1, Ordering::Relaxed);
            heavy_barrier();
            flag_a.store(2 * i + 1, Ordering::Relaxed);
        }
    });

    let thread_b = thread::spawn(move || {
        for i in 0..ROUNDS {
            // Wait for our turn (flag == 2*i + 1)
            loop {
                let f = flag_b.load(Ordering::Relaxed);
                light_barrier();
                if f == 2 * i + 1 {
                    break;
                }
                std::hint::spin_loop();
            }

            // Verify data is correct
            let d = data_b.load(Ordering::Relaxed);
            assert_eq!(
                d,
                2 * i + 1,
                "Ping-pong violation at round {}: expected data={}, got {}",
                i,
                2 * i + 1,
                d
            );

            // Write our data and signal
            data_b.store(2 * i + 2, Ordering::Relaxed);
            heavy_barrier();
            flag_b.store(2 * i + 2, Ordering::Relaxed);
        }
    });

    thread_a.join().unwrap();
    thread_b.join().unwrap();
}

/// Linux-specific test: Verify that OS-accelerated barriers (membarrier) are enabled.
///
/// This test ensures that on Linux kernels 4.3+, the library successfully
/// registers and uses either MEMBARRIER_CMD_PRIVATE_EXPEDITED (4.14+) or MEMBARRIER_CMD_SHARED (4.3+)
/// for zero-cost reader barriers.
///
/// 此测试确保在 Linux 内核 4.3+ 上，库成功注册并使用
/// MEMBARRIER_CMD_PRIVATE_EXPEDITED (4.14+) 或 MEMBARRIER_CMD_SHARED (4.3+) 实现零开销读取屏障。
#[test]
#[cfg(target_os = "linux")]
fn test_linux_membarrier_acceleration_enabled() {
    assert!(
        swmr_barrier::is_accelerated(),
        "MEMBARRIER (SHARED or PRIVATE_EXPEDITED) should be supported on Linux kernel 4.3+. \
         If this test fails, either the kernel is too old (< 4.3) or membarrier registration failed."
    );
    println!("Linux membarrier acceleration is enabled (IS_ACCELERATED = true)");
}

/// Windows-specific test: Verify that FlushProcessWriteBuffers is available.
///
/// On Windows Vista and later, this should always return true.
///
/// Windows 专用测试：验证 FlushProcessWriteBuffers 是否可用。
/// 在 Windows Vista 及更高版本上，这应始终返回 true。
#[test]
#[cfg(target_os = "windows")]
fn test_windows_acceleration_enabled() {
    assert!(
        swmr_barrier::is_accelerated(),
        "acceleration should be enabled on Windows (Vista+). \
         If this fails, FlushProcessWriteBuffers was not found."
    );
    println!("Windows acceleration is enabled (IS_ACCELERATED = true)");
}
