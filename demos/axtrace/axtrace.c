// SPDX-License-Identifier: GPL-2.0
#include <linux/module.h>
#include <linux/miscdevice.h>
#include <linux/fs.h>
#include <linux/uaccess.h>

/* Shared demo protocol: x0=function, x1=request ID; x0 returns status. */
#define AXTRACE_SUBMIT 0x41585401UL
static bool use_hypercall;
module_param(use_hypercall, bool, 0400);
MODULE_PARM_DESC(use_hypercall, "Enable AXTRACE_SUBMIT after host handler is installed");

static noinline long axtrace_submit(u64 request)
{
    register unsigned long x0 asm("x0") = AXTRACE_SUBMIT;
    register unsigned long x1 asm("x1") = request;
    asm volatile("hvc #0" : "+r"(x0), "+r"(x1) : : "x2", "x3", "x4", "x5", "x6", "x7", "memory");
    return (long)x0;
}

static noinline ssize_t axtrace_write(struct file *file, const char __user *buf,
                                     size_t len, loff_t *offset)
{
    u64 request;
    long status = 0;
    if (len != sizeof(request))
        return -EINVAL;
    if (copy_from_user(&request, buf, sizeof(request)))
        return -EFAULT;
    pr_info("axtrace: KERNEL request=%llu hypercall=%d\n", request, use_hypercall);
    if (use_hypercall)
        status = axtrace_submit(request);
    pr_info("axtrace: COMPLETE request=%llu status=%ld\n", request, status);
    return status < 0 ? status : sizeof(request);
}

static const struct file_operations axtrace_ops = {
    .owner = THIS_MODULE,
    .write = axtrace_write,
};
static struct miscdevice axtrace_device = {
    .minor = MISC_DYNAMIC_MINOR,
    .name = "axtrace",
    .fops = &axtrace_ops,
    .mode = 0600,
};
static int __init axtrace_init(void)
{
    return misc_register(&axtrace_device);
}
static void __exit axtrace_exit(void)
{
    misc_deregister(&axtrace_device);
}
module_init(axtrace_init);
module_exit(axtrace_exit);
MODULE_LICENSE("GPL");
MODULE_DESCRIPTION("Minimal guest-to-hypervisor tracing demo");
