#!/bin/bash
set -e

# Arguments
# $1: Path to the test binary

TEST_BINARY="$1"
if [ -z "$TEST_BINARY" ]; then
    echo "Usage: $0 <path-to-test-binary>"
    exit 1
fi

# Resolve absolute path
TEST_BINARY=$(readlink -f "$TEST_BINARY")

echo "Preparing QEMU environment for $TEST_BINARY..."

# 1. Download Alpine 3.7 Kernel (Linux 4.9.65)
# We use 'vanilla' flavor which is more compatible with standard QEMU boot than 'virt' on older versions
# Note: Netboot directory is missing for 3.7, so we extract from ISO.
ISO_NAME="alpine-vanilla-3.7.3-x86_64.iso"
ISO_URL="http://dl-cdn.alpinelinux.org/alpine/v3.7/releases/x86_64/$ISO_NAME"

if [ ! -f "vmlinuz-vanilla" ]; then
    if [ ! -f "$ISO_NAME" ]; then
        echo "Downloading Alpine ISO ($ISO_NAME)..."
        curl -sL -o "$ISO_NAME" "$ISO_URL"
    fi

    echo "Extracting kernel from ISO..."
    # 7z is pre-installed on GitHub Actions ubuntu-latest runners
    if command -v 7z >/dev/null 2>&1; then
        7z e "$ISO_NAME" "boot/vmlinuz-vanilla" -y > /dev/null
    else
        # Fallback to mount (requires sudo)
        echo "7z not found, using sudo mount..."
        mkdir -p mnt_iso
        sudo mount -o loop "$ISO_NAME" mnt_iso
        cp mnt_iso/boot/vmlinuz-vanilla .
        sudo umount mnt_iso
        rmdir mnt_iso
    fi
    
    if [ ! -f "vmlinuz-vanilla" ]; then
        echo "Error: Failed to extract vmlinuz-vanilla"
        exit 1
    fi
fi

# 2. Create Initramfs
# We create a minimal initramfs that contains:
# - A static busybox (for basic shell/commands)
# - The test binary
# - An init script
mkdir -p initramfs
cd initramfs

# Copy busybox (assuming busybox-static is installed on host)
# If not, we can download a static busybox binary.
if [ -f "/bin/busybox" ]; then
    cp /bin/busybox ./busybox
else
    # Fallback to downloading static busybox
    echo "Downloading busybox..."
    curl -sL -o busybox "https://www.busybox.net/downloads/binaries/1.35.0-x86_64-linux-musl/busybox"
    chmod +x busybox
fi

# Copy test binary
cp "$TEST_BINARY" ./test_binary

# Create init script
cat > init <<EOF
#!/busybox sh
/busybox mount -t proc proc /proc
/busybox mount -t sysfs sys /sys

echo "========================================"
echo "STARTING TEST in QEMU (Kernel 4.9)"
echo "========================================"

# Run the test
/test_binary
EXIT_CODE=\$?

echo "========================================"
echo "TEST FINISHED with exit code: \$EXIT_CODE"
echo "========================================"

# Force poweroff
/busybox poweroff -f
EOF

chmod +x init

# Pack initramfs
find . | cpio -o -H newc | gzip > ../initramfs.img
cd ..

# 3. Run QEMU
echo "Booting QEMU..."
# -kernel: The Linux kernel
# -initrd: Our packed filesystem
# -nographic: Output to console
# -append: Kernel parameters (console redirection)
# -monitor none: Disable QEMU monitor
# -no-reboot: Exit QEMU when guest reboots/shuts down
qemu-system-x86_64 \
    -kernel vmlinuz-vanilla \
    -initrd initramfs.img \
    -nographic \
    -append "console=ttyS0 panic=1 init=/init ramdisk_size=102400" \
    -monitor none \
    -no-reboot

echo "QEMU exited."
