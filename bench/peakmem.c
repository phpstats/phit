/*
 * peakmem COMMAND [ARGS...]: runs COMMAND and reports its memory on stderr in
 * the form macOS /usr/bin/time -l uses, in bytes, so the benchmark runners
 * read Linux runs the same way:
 *
 *   N  maximum resident set size   the kernel's peak RSS (getrusage), which
 *                                  counts file pages the process maps
 *   N  peak memory footprint       the peak of the process's anonymous
 *                                  resident memory (RssAnon in
 *                                  /proc/PID/status), sampled every
 *                                  millisecond: its own allocations, without
 *                                  the file pages it maps
 *
 * A leading -l is accepted and ignored, so it can stand in for time -l.
 * The exit status is the command's.
 */
#define _GNU_SOURCE
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/resource.h>
#include <sys/wait.h>
#include <time.h>
#include <unistd.h>

static long rss_anon_kib(pid_t pid)
{
    char path[64], line[256];
    long kib = -1;
    snprintf(path, sizeof path, "/proc/%d/status", pid);
    FILE *f = fopen(path, "r");
    if (!f) return -1;
    while (fgets(line, sizeof line, f)) {
        if (strncmp(line, "RssAnon:", 8) == 0) { kib = strtol(line + 8, NULL, 10); break; }
    }
    fclose(f);
    return kib;
}

int main(int argc, char **argv)
{
    int first = 1;
    if (argc > 1 && strcmp(argv[1], "-l") == 0) first = 2;
    if (argc <= first) { fprintf(stderr, "usage: peakmem [-l] COMMAND [ARGS...]\n"); return 2; }

    pid_t pid = fork();
    if (pid < 0) { perror("fork"); return 2; }
    if (pid == 0) { execvp(argv[first], argv + first); perror(argv[first]); _exit(127); }

    long peak = 0;
    int status;
    struct rusage usage;
    struct timespec tick = {0, 1000000};
    for (;;) {
        long kib = rss_anon_kib(pid);
        if (kib > peak) peak = kib;
        pid_t done = wait4(pid, &status, WNOHANG, &usage);
        if (done == pid) break;
        if (done < 0) { perror("wait4"); return 2; }
        nanosleep(&tick, NULL);
    }
    fprintf(stderr, "%20ld  maximum resident set size\n", usage.ru_maxrss * 1024L);
    fprintf(stderr, "%20ld  peak memory footprint\n", peak * 1024L);
    return WIFEXITED(status) ? WEXITSTATUS(status) : 128 + WTERMSIG(status);
}
