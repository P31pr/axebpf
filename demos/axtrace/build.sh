#!/bin/sh
# Use a dedicated kernel source tree at commit fd95357fd8c6.
set -eu
kernel_source=$(realpath "${1:?usage: sh build.sh KERNEL_SOURCE GUEST_KERNEL_IMAGE}")
kernel_image=$(realpath "${2:?guest kernel image required}")
demo_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
python3 - "$kernel_image" "$kernel_source/.config" <<'PY'
from pathlib import Path
import sys
import zlib
data = Path(sys.argv[1]).read_bytes()
start = data.index(b"IKCFG_ST") + len(b"IKCFG_ST")
config = zlib.decompress(data[start:], 16 + zlib.MAX_WBITS)
assert b"CONFIG_MODULES=y\n" in config
assert b"# CONFIG_MODVERSIONS is not set\n" in config
Path(sys.argv[2]).write_bytes(config)
PY
make -C "$kernel_source" ARCH=arm64 CROSS_COMPILE=aarch64-linux-gnu- \
    LOCALVERSION=-gfd95357fd8c6-dirty olddefconfig modules_prepare -j8
# Original Module.symvers was not distributed. MODVERSIONS is disabled;
# the smoke test checks actual symbol resolution by loading the module.
make -C "$kernel_source" ARCH=arm64 CROSS_COMPILE=aarch64-linux-gnu- \
    LOCALVERSION=-gfd95357fd8c6-dirty M="$demo_dir" KBUILD_MODPOST_WARN=1 modules -j8
aarch64-linux-gnu-gcc -static -O2 -g -Wall -Wextra \
    -o "$demo_dir/axtrace-client" "$demo_dir/client.c"
