#include <errno.h>
#include <fcntl.h>
#include <inttypes.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <limits.h>
#include <sys/syscall.h>
#include <time.h>
#include <unistd.h>

__attribute__((noinline))
ssize_t demo_submit(uint64_t request, int fd)
{
    return write(fd, &request, sizeof(request));
}
int main(int argc, char **argv)
{
    unsigned int wait_seconds = 60;
    if (argc != 1) {
        char *end;
        if (argc != 3 || strcmp(argv[1], "--warmup-wait") != 0 ||
            argv[2][0] < '0' || argv[2][0] > '9') {
            fprintf(stderr, "usage: axtrace-client [--warmup-wait SECONDS]\n");
            return 1;
        }
        errno = 0;
        unsigned long value = strtoul(argv[2], &end, 10);
        if (errno || *end || value > UINT_MAX) {
            fprintf(stderr, "invalid warmup wait\n");
            return 1;
        }
        wait_seconds = (unsigned int)value;
    }
    int fd = open("/dev/axtrace", O_WRONLY);
    if (fd < 0) { perror("open axtrace"); return 1; }
    if (wait_seconds > 0) {
        /* Request 0 touches the actual probed function in this same process.
         * Its records are warmup data, not part of requests 41..43. */
        puts("axtrace: WARMUP request=0");
        fflush(stdout);
        if (demo_submit(0, fd) != sizeof(uint64_t)) {
            perror("warmup write axtrace"); close(fd); return 1;
        }
        printf("axtrace: CLIENT_READY pid=%ld wait_seconds=%u; check trace list for active uprobe\n",
               (long)getpid(), wait_seconds);
        fflush(stdout);
        /* The current uprobe observer retries at __arm64_sys_nanosleep.
         * libc sleep/nanosleep may use clock_nanosleep instead, so invoke
         * the specific syscall after touching the target code page. */
        struct timespec remaining = { .tv_sec = wait_seconds, .tv_nsec = 0 };
        struct timespec interrupted;
        while (syscall(SYS_nanosleep, &remaining, &interrupted) == -1) {
            if (errno != EINTR) {
                perror("SYS_nanosleep"); close(fd); return 1;
            }
            remaining = interrupted;
        }
    }
    puts("axtrace: REQUESTS_BEGIN first=41 last=43");
    fflush(stdout);
    for (uint64_t request = 41; request <= 43; request++) {
        printf("axtrace: USER request=%" PRIu64 "\n", request);
        fflush(stdout);
        ssize_t result = demo_submit(request, fd);
        if (result != sizeof(request)) { perror("write axtrace"); close(fd); return 1; }
        printf("axtrace: USER_DONE request=%" PRIu64 " bytes=%zd\n", request, result);
    }
    errno = 0;
    if (write(fd, "x", 1) != -1 || errno != EINVAL) {
        fprintf(stderr, "short write was not rejected with EINVAL\n");
        close(fd); return 1;
    }
    close(fd);
    puts("axtrace: PASS");
    return 0;
}
