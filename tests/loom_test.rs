#[cfg(feature = "loom")]
use loom::sync::atomic::{AtomicUsize, Ordering};
#[cfg(feature = "loom")]
use loom::thread;
#[cfg(feature = "loom")]
use std::sync::Arc;
#[cfg(feature = "loom")]
use swmr_barrier::{heavy_barrier, light_barrier};

#[test]
#[cfg(feature = "loom")]
fn test_heavy_light_barrier_ordering() {
    loom::model(|| {
        let x = Arc::new(AtomicUsize::new(0));
        let y = Arc::new(AtomicUsize::new(0));

        let x1 = x.clone();
        let y1 = y.clone();

        // Thread 1 (Writer): Store X -> Heavy Barrier -> Store Y
        // This represents the "Cold Path" which pays the heavy cost.
        thread::spawn(move || {
            x1.store(1, Ordering::Relaxed);
            heavy_barrier();
            y1.store(1, Ordering::Relaxed);
        });

        let x2 = x.clone();
        let y2 = y.clone();

        // Thread 2 (Reader): Load Y -> Light Barrier -> Load X
        // This represents the "Hot Path" which is optimized.
        thread::spawn(move || {
            let r1 = y2.load(Ordering::Relaxed);
            
            // If light_barrier() works correctly, it ensures that if we see Y=1,
            // we must also see X=1.
            light_barrier();
            
            let r2 = x2.load(Ordering::Relaxed);

            // Invariant: If we observed the effect of the second store (Y=1),
            // we must also observe the effect of the first store (X=1).
            if r1 == 1 {
                assert_eq!(r2, 1, "Violation: saw Y=1 but X=0 (Reader observed reordering!)");
            }
        });
    });
}
