#!/usr/bin/env python3
"""Boot the demo and check write round trips, with optional AxVisor hosting."""
import argparse
from pathlib import Path
import re
import tempfile

import pexpect


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--kernel", required=True)
    parser.add_argument("--image", required=True)
    parser.add_argument("--log", required=True, type=Path)
    parser.add_argument("--axvisor", action="store_true")
    parser.add_argument("--attach-wait", type=int, default=0,
                        help="expected wait configured in the image")
    parser.add_argument("--warmup-wait", type=int, default=0,
                        help="expected client wait configured in the image")
    args = parser.parse_args()
    # Explicit overlay keeps the source image unchanged and avoids QEMU's
    # platform-dependent /var/tmp location for -snapshot.
    import subprocess
    with tempfile.TemporaryDirectory(prefix="axtrace-run-") as temp:
        overlay = str(Path(temp) / "disk.qcow2")
        subprocess.run(["qemu-img", "create", "-f", "qcow2", "-F", "raw", "-b",
                        str(Path(args.image).resolve()), overlay], check=True)
        cmd = ["-nographic", "-cpu", "cortex-a72", "-machine",
               "virt,gic-version=3" + (",virtualization=on" if args.axvisor else ""),
               "-smp", "4" if args.axvisor else "1", "-m", "8g" if args.axvisor else "512M",
               "-device", "virtio-blk-device,drive=disk0", "-drive",
               f"id=disk0,if=none,format=qcow2,file={overlay}", "-kernel", args.kernel,
               "-append", "root=/dev/vda rw init=/init" if args.axvisor else
               "root=/dev/vda rw init=/sbin/init nokaslr console=ttyAMA0"]
        args.log.parent.mkdir(parents=True, exist_ok=True)
        with args.log.open("w") as log:
            child = pexpect.spawn("qemu-system-aarch64", cmd, encoding="utf-8", timeout=60)
            child.logfile_read = log
            try:
                if args.axvisor:
                    child.expect(r"axvisor:/.*\$ ")
                    child.sendline("vm create /vmconfigs/axtrace.toml")
                    child.expect(r"axvisor:/.*\$ ")
                    child.sendline("vm start 1")
                if args.attach_wait > 0:
                    child.expect_exact(f"axtrace: MODULE_READY wait_seconds={args.attach_wait}")
                    # No request may run during the first half of the window.
                    result = child.expect_exact(["axtrace: USER request=", pexpect.TIMEOUT],
                                                timeout=args.attach_wait / 2)
                    assert result == 1, "client ran before the attach window elapsed"
                if args.warmup_wait > 0:
                    child.expect(r"axtrace: CLIENT_READY pid=\d+ wait_seconds=" +
                                 str(args.warmup_wait) + r";", timeout=args.attach_wait + 60)
                    result = child.expect_exact(["axtrace: REQUESTS_BEGIN", pexpect.TIMEOUT],
                                                timeout=args.warmup_wait / 2)
                    assert result == 1, "formal requests ran before the warmup wait elapsed"
                if args.attach_wait > 0 or args.warmup_wait > 0:
                    child.expect_exact("axtrace: DONE module retained; detach probes before manual rmmod",
                                       timeout=args.attach_wait + args.warmup_wait + 60)
                else:
                    child.expect_exact("axtrace: UNLOAD PASS")
            finally:
                child.close(force=True)
    text = args.log.read_text()
    assert "axtrace: PASS" in text
    if args.attach_wait > 0 or args.warmup_wait > 0:
        assert "axtrace: UNLOAD PASS" not in text
    if args.warmup_wait > 0:
        stages = ["axtrace: WARMUP request=0", "axtrace: KERNEL request=0"]
        if args.axvisor:
            stages.append("axtrace: HOST request=0 vm=1")
        stages += ["axtrace: CLIENT_READY", "axtrace: REQUESTS_BEGIN", "axtrace: USER request=41"]
        positions = [text.index(stage) for stage in stages]
        assert positions == sorted(positions), stages
    assert not re.search(r"Kernel panic|Unknown symbol|invalid module format", text)
    for request in (41, 42, 43):
        stages = [f"axtrace: USER request={request}", f"axtrace: KERNEL request={request}"]
        if args.axvisor:
            stages.append(f"axtrace: HOST request={request} vm=1")
        stages += [f"axtrace: COMPLETE request={request} status=0",
                   f"axtrace: USER_DONE request={request} bytes=8"]
        positions = [text.index(stage) for stage in stages]
        assert positions == sorted(positions), stages
    print(f"PASS: three ordered request round trips; log: {args.log}")


if __name__ == "__main__":
    main()
