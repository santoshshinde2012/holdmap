/* Run a command and print its peak resident set size in KiB on stderr, like GNU time's %M.
 * A tiny parent keeps the reading honest: Linux carries the spawning process's own peak
 * into the child's at exec, so measuring from a large process (Python) overstates it.
 * Build: cc -O2 -o peak-rss scripts/peak-rss.c */
#include <stdio.h>
#include <sys/resource.h>
#include <sys/wait.h>
#include <unistd.h>

int main(int argc, char **argv) {
    if (argc < 2) {
        fprintf(stderr, "usage: peak-rss COMMAND [ARG...]\n");
        return 2;
    }
    pid_t pid = fork();
    if (pid < 0) return 2;
    if (pid == 0) {
        execvp(argv[1], argv + 1);
        _exit(127);
    }
    int status = 0;
    struct rusage ru;
    if (wait4(pid, &status, 0, &ru) < 0) return 2;
#ifdef __APPLE__
    long kib = ru.ru_maxrss / 1024; /* bytes on macOS */
#else
    long kib = ru.ru_maxrss; /* KiB on Linux */
#endif
    fprintf(stderr, "peak_rss_kib=%ld\n", kib);
    return WIFEXITED(status) ? WEXITSTATUS(status) : 128 + WTERMSIG(status);
}
