#!/usr/bin/env python3
"""Create an independent demo image; never edit the input image."""
import argparse
from pathlib import Path
import subprocess
import tempfile


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("base_image", type=Path)
    parser.add_argument("output_image", type=Path)
    parser.add_argument("--client", required=True, type=Path)
    parser.add_argument("--module", required=True, type=Path)
    parser.add_argument("--hypercall", choices=(0, 1), type=int, default=0)
    parser.add_argument("--attach-wait", type=int, default=30,
                        help="seconds after module addresses are printed")
    parser.add_argument("--warmup-wait", type=int, default=60,
                        help="seconds client waits after request 0; both waits must be 0 for automatic unload")
    args = parser.parse_args()
    if args.attach_wait < 0:
        parser.error("--attach-wait must be nonnegative")
    if not 0 <= args.warmup_wait <= 4294967295:
        parser.error("--warmup-wait must fit an unsigned 32-bit integer")
    if args.output_image.exists():
        parser.error("output image already exists; choose a new path")
    subprocess.run(["cp", "--reflink=auto", args.base_image, args.output_image], check=True)
    # Replay any outstanding journal before offline edits, otherwise boot may
    # restore old inode metadata over the files installed below.
    result = subprocess.run(["e2fsck", "-fy", args.output_image])
    if result.returncode not in (0, 1):
        raise RuntimeError("image filesystem check failed")

    def debugfs(command):
        return subprocess.run(["debugfs", "-w", "-R", command, args.output_image],
                              check=True, capture_output=True, text=True)

    with tempfile.TemporaryDirectory(prefix="axtrace-image-") as temp:
        tmp = Path(temp)
        mode = tmp / "mode"
        mode.write_text(f"{args.hypercall}\n")
        wait = tmp / "attach-wait"
        wait.write_text(f"{args.attach_wait}\n")
        warmup = tmp / "warmup-wait"
        warmup.write_text(f"{args.warmup_wait}\n")
        if "Inode:" not in debugfs("stat /usr/lib").stdout:
            debugfs("mkdir /usr/lib")
        for source, destination in (
            (args.module, "/usr/lib/axtrace.ko"),
            (args.client, "/usr/bin/axtrace-client"),
            (Path(__file__).with_name("launch.sh"), "/etc/init.d/axebpf-integration-launch.sh"),
            (Path(__file__).with_name("guest.toml"), "/vmconfigs/axtrace.toml"),
            (mode, "/etc/axtrace-hypercall"),
            (wait, "/etc/axtrace-attach-wait"),
            (warmup, "/etc/axtrace-warmup-wait"),
        ):
            debugfs(f"rm {destination}")
            debugfs(f'write "{source.resolve()}" {destination}')
            # debugfs may exit successfully even when a command fails.
            check = tmp / "readback"
            check.unlink(missing_ok=True)
            debugfs(f"dump {destination} {check}")
            if check.read_bytes() != source.read_bytes():
                raise RuntimeError(f"image readback mismatch: {destination}")
        debugfs("set_inode_field /usr/bin/axtrace-client mode 0100755")
    print(f"Prepared {args.output_image} (hypercall={args.hypercall}, attach_wait={args.attach_wait}s, warmup_wait={args.warmup_wait}s)")


if __name__ == "__main__":
    main()
