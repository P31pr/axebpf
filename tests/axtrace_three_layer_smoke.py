#!/usr/bin/env python3
"""Exercise one axtrace program on USER/KERNEL/HOST probes in AxVisor."""
import argparse
from pathlib import Path
import re
import subprocess
import tempfile
import time
import pexpect

p = argparse.ArgumentParser(description=__doc__)
p.add_argument('--kernel', required=True)
p.add_argument('--elf', required=True)
p.add_argument('--image', required=True, help='image with both wait windows and current eBPF/client symbols')
p.add_argument('--log', required=True)
p.add_argument('--settle-seconds', type=float, default=2,
               help='delay after module readiness before sending host commands')
args = p.parse_args()
symbols = subprocess.check_output(['aarch64-linux-gnu-nm', '-n', args.elf], text=True)
symbol = next(line.split()[2] for line in symbols.splitlines()
              if 'axtrace_submit' in line and 'axvm' in line)
with tempfile.TemporaryDirectory(prefix='axtrace-three-') as tmp:
    disk = str(Path(tmp) / 'disk.qcow2')
    subprocess.run(['qemu-img', 'create', '-f', 'qcow2', '-F', 'raw', '-b',
                    str(Path(args.image).resolve()), disk], check=True)
    with open(args.log, 'w') as log:
        child = pexpect.spawn('qemu-system-aarch64', [
            '-nographic', '-cpu', 'cortex-a72', '-machine', 'virt,virtualization=on,gic-version=3',
            '-smp', '4', '-m', '8g', '-device', 'virtio-blk-device,drive=disk0',
            '-drive', f'id=disk0,if=none,format=qcow2,file={disk}',
            '-kernel', args.kernel, '-append', 'root=/dev/vda rw init=/init',
        ], encoding='utf-8', timeout=90)
        child.logfile_read = log
        prompt = r'(?:\n|\x1b\[m)axvisor:/[^\r\n]*\$ '
        def command(text):
            child.sendline(text)
            child.expect(prompt, timeout=30)
            return child.before
        try:
            child.expect(prompt)
            command('trace verbose on')
            command('vm create /vmconfigs/axtrace.toml')
            command('trace loadsyms vm1 /vmimages/linux-qemu-aarch64.syms')
            command('trace uloadelf vm1 /usr/bin/axtrace-client /vmimages/axtrace-client.syms')
            loaded = command('trace load file /vmimages/axtrace.o shell:shell_command')
            assert 'Loaded program 0 ' in loaded, loaded
            command(f'trace hprobe {symbol} 0')
            command('trace uprobe vm1:/usr/bin/axtrace-client:demo_submit 0')
            child.sendline('vm start 1')
            child.expect(r'([0-9a-f]{16})\s+t axtrace_submit\s+\[axtrace\]')
            address = child.match.group(1)
            child.expect_exact('axtrace: MODULE_READY')
            # Let guest boot output settle before recovering the host prompt.
            time.sleep(args.settle_seconds)
            child.sendline('')
            child.expect(prompt, timeout=8)
            attached = command(f'trace kprobe vm1:0x{address} 0 --inject')
            assert 'kprobe attached:' in attached, attached
            child.expect_exact('axtrace: CLIENT_READY')
            child.expect_exact('reason=runtime_retry')
            active = command('trace list')
            assert re.search(r'/usr/bin/axtrace-client\s+demo_submit[^\n]*\bactive\b', active), active
            child.expect_exact('axtrace: DONE module retained; detach probes before manual rmmod')
            child.sendline('')
            child.expect(prompt, timeout=30)
            result = command('trace path 0')
            result = re.sub(r'\x1b\[[0-9;]*[A-Za-z]', '', result)
            # Ignore warmup request 0; it intentionally precedes activation.
            for request in (41, 42, 43):
                block = re.search(rf'request={request}\s+(.*?)(?=request=|Observed requests:)', result, re.S)
                assert block, result
                assert 'PASS: USER -> KERNEL -> HOST' in block.group(1), block.group(1)
            # Ending QEMU avoids relying on the still-unverified online detach path.
        finally:
            child.close(force=True)
print('PASS: runtime retry activated uprobe; requests 41/42/43 each have one ordered USER/KERNEL/HOST record')
