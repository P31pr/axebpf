#!/bin/sh
# Run by the guest's existing /etc/inittab.
exec >/dev/console 2>&1
mount -t proc proc /proc 2>/dev/null || true
mount -t sysfs sysfs /sys 2>/dev/null || true
mount -t devtmpfs devtmpfs /dev 2>/dev/null || true
mode=$(cat /etc/axtrace-hypercall)
wait_seconds=30
warmup_wait=60
if [ -f /etc/axtrace-attach-wait ]; then
    wait_seconds=$(cat /etc/axtrace-attach-wait)
fi
if [ -f /etc/axtrace-warmup-wait ]; then
    warmup_wait=$(cat /etc/axtrace-warmup-wait)
fi
case "$wait_seconds" in
    ''|*[!0-9]*) echo 'axtrace: invalid attach wait'; exit 1 ;;
esac
case "$warmup_wait" in
    ''|*[!0-9]*) echo 'axtrace: invalid warmup wait'; exit 1 ;;
esac
echo "axtrace: BOOT hypercall=$mode"
insmod /usr/lib/axtrace.ko use_hypercall="$mode" || exit 1
cat /proc/modules
grep ' axtrace_' /proc/kallsyms
echo "axtrace: MODULE_READY wait_seconds=$wait_seconds"
if [ "$wait_seconds" -gt 0 ]; then
    sleep "$wait_seconds" || exit 1
fi
/usr/bin/axtrace-client --warmup-wait "$warmup_wait" || exit 1
if [ "$wait_seconds" -gt 0 ] || [ "$warmup_wait" -gt 0 ]; then
    # A guest kprobe may still point into module text. Do not unload it here.
    echo 'axtrace: DONE module retained; detach probes before manual rmmod'
    exit 0
fi
rmmod axtrace || exit 1
echo 'axtrace: UNLOAD PASS'
