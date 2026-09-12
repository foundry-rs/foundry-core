// Diagnostic launcher: retain Socket's environment and observe real Cargo.
#include <fcntl.h>
#include <stdio.h>
#include <stdlib.h>
#include <errno.h>
#include <sys/wait.h>
#include <unistd.h>

int main(int argc, char **argv) {
    (void)argc;
    const char *cargo = getenv("DIAG_REAL_CARGO");
    const char *library = getenv("DIAG_IO_LIBRARY");
    if (!cargo || !library) return 125;
    fprintf(stderr, "IO_PROBE pid=%d stdout_flags=%x stderr_flags=%x\n",
            getpid(), fcntl(1, F_GETFL), fcntl(2, F_GETFL));
    pid_t child = fork();
    if (child < 0) return 125;
    if (!child) {
        if (setenv("DYLD_INSERT_LIBRARIES", library, 1)) _exit(125);
        argv[0] = (char *)cargo;
        execv(cargo, argv);
        perror("IO_PROBE execv");
        _exit(125);
    }
    int status;
    while (waitpid(child, &status, 0) < 0) if (errno != EINTR) return 125;
    fprintf(stderr, "IO_PROBE child=%d raw_status=%d\n", child, status);
    return WIFEXITED(status) ? WEXITSTATUS(status) :
        WIFSIGNALED(status) ? 128 + WTERMSIG(status) : 125;
}
