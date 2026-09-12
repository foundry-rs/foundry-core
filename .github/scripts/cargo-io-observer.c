// macOS-only observation; preserve every return value and errno. Never log data.
#include <errno.h>
#include <fcntl.h>
#include <stdio.h>
#include <sys/stat.h>
#include <sys/uio.h>
#include <unistd.h>

static _Thread_local int reporting;

__attribute__((constructor)) static void init(void) {
    char message[160];
    int n = snprintf(message, sizeof(message),
                     "IO_OBSERVER loaded pid=%d stdout_flags=%x stderr_flags=%x\n",
                     getpid(), fcntl(1, F_GETFL), fcntl(2, F_GETFL));
    write(2, message, (size_t)n);
}

static void observe(const char *operation, int fd, int error) {
    if (reporting || error != EAGAIN) return;
    reporting = 1;
    struct stat st = {0};
    fstat(fd, &st);
    int flags = fcntl(fd, F_GETFL);
    char message[256];
    int n = snprintf(message, sizeof(message),
        "IO_OBSERVER EAGAIN pid=%d operation=%s fd=%d flags=%x nonblocking=%d kind=%s\n",
        getpid(), operation, fd, flags, flags >= 0 && (flags & O_NONBLOCK) != 0,
        S_ISFIFO(st.st_mode) ? "pipe" : S_ISSOCK(st.st_mode) ? "socket" :
        S_ISREG(st.st_mode) ? "file" : "other");
    write(2, message, (size_t)n);
    reporting = 0;
}

static ssize_t observed_write(int fd, const void *data, size_t size) {
    // dyld leaves this image's calls bound to the original implementation.
    ssize_t result = write(fd, data, size);
    int error = errno;
    if (result < 0) observe("write", fd, error);
    errno = error;
    return result;
}

static ssize_t observed_writev(int fd, const struct iovec *data, int count) {
    ssize_t result = writev(fd, data, count);
    int error = errno;
    if (result < 0) observe("writev", fd, error);
    errno = error;
    return result;
}

#define INTERPOSE(replacement, original) \
    __attribute__((used)) static struct { const void *new_fn; const void *old_fn; } \
    interpose_##original __attribute__((section("__DATA,__interpose"))) = \
    { (const void *)(replacement), (const void *)(original) }
INTERPOSE(observed_write, write);
INTERPOSE(observed_writev, writev);
