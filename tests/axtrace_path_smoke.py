#!/usr/bin/env python3
"""Verify trace path against a real host-only eBPF capture, not a complete path."""
import argparse
from pathlib import Path
import re
import subprocess
import tempfile
import pexpect

p = argparse.ArgumentParser(description=__doc__)
p.add_argument('--kernel', required=True)
p.add_argument('--elf', required=True)
p.add_argument('--image', required=True, help='HVC-enabled image with axtrace.o and retained module')
p.add_argument('--log', required=True)
args = p.parse_args()
symbols = subprocess.check_output(['aarch64-linux-gnu-nm', '-n', args.elf], text=True)
symbol = next(line.split()[2] for line in symbols.splitlines()
              if 'axtrace_submit' in line and 'axvm' in line)
with tempfile.TemporaryDirectory(prefix='axtrace-path-') as tmp:
    disk = str(Path(tmp) / 'disk.qcow2')
    subprocess.run(['qemu-img', 'create', '-f', 'qcow2', '-F', 'raw', '-b',
                    str(Path(args.image).resolve()), disk], check=True)
    with open(args.log, 'w') as log:
        child = pexpect.spawn('qemu-system-aarch64', [
            '-nographic', '-cpu', 'cortex-a72', '-machine', 'virt,virtualization=on,gic-version=3',
            '-smp', '4', '-m', '8g', '-device', 'virtio-blk-device,drive=disk0',
            '-drive', f'id=disk0,if=none,format=qcow2,file={disk}',
            '-kernel', args.kernel, '-append', 'root=/dev/vda rw init=/init',
        ], encoding='utf-8', timeout=60)
        child.logfile_read = log
        prompt = r'(?:\n|\x1b\[m)axvisor:/[^\r\n]*\$ '
        def command(text):
            child.sendline(text)
            child.expect(prompt)
            return child.before
        try:
            child.expect(prompt)
            assert 'not loaded' in command('trace path 999999')
            loaded = command('trace load file /vmimages/axtrace.o shell:shell_command')
            # Fresh VM: first program loaded is ID 0.
            assert 'No eBPF records' in command('trace path 0'), loaded
            command(f'trace hprobe {symbol} 0')
            command('vm create /vmconfigs/axtrace.toml')
            child.sendline('vm start 1')
            child.expect_exact('axtrace: DONE module retained; detach probes before manual rmmod')
            child.sendline('')
            child.expect(prompt)
            result = command('trace path 0')
            result = re.sub(r'\x1b\[[0-9;]*[A-Za-z]', '', result)
            for request in (41, 42, 43):
                assert f'request={request}' in result, result
            assert len(re.findall(r'HOST\s+first_ns=\d+ last_ns=\d+ hits=1', result)) == 3, result
            assert result.count('INCOMPLETE') == 3, result
            assert 'PASS: USER -> KERNEL -> HOST' not in result, result
            # QEMU is terminated below; no guest module is unloaded with a live probe.
        finally:
            child.close(force=True)
print('PASS: real map contains three HOST records; missing USER/KERNEL correctly reported')
