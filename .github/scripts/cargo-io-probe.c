// Diagnostic launcher: retain Socket's environment and observe real Cargo.
#include <fcntl.h>
#include <stdio.h>
#include <stdlib.h>
#include <unistd.h>

int main(int argc, char **argv) {
    (void)argc;
    const char *cargo = getenv("DIAG_REAL_CARGO");
    const char *library = getenv("DIAG_IO_LIBRARY");
    if (!cargo || !library) return 125;
    fprintf(stderr, "IO_PROBE pid=%d stdout_flags=%x stderr_flags=%x\n",
            getpid(), fcntl(1, F_GETFL), fcntl(2, F_GETFL));
    if (setenv("DYLD_INSERT_LIBRARIES", library, 1)) return 125;
    execv(cargo, argv);
    perror("IO_PROBE execv");
    return 125;
}
