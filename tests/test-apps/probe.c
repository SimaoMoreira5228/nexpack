#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>
#include <fcntl.h>
#include <signal.h>
#include <setjmp.h>
#include <sys/socket.h>
#include <sys/wait.h>
#include <sys/stat.h>
#include <netinet/in.h>
#include <arpa/inet.h>
#include <errno.h>

#define MAX_PROBES 32
#define PROBE_NAME_MAX 48

static sigjmp_buf jmpbuf;
static volatile int caught_sigsys = 0;

static void sigsys_handler(int _sig)
{
    (void)_sig;
    caught_sigsys = 1;
    siglongjmp(jmpbuf, 1);
}

struct probe_result {
    char name[PROBE_NAME_MAX];
    int passed;
};

static struct probe_result results[MAX_PROBES];
static int probe_count = 0;

static void probe_begin(const char *name)
{
    if (probe_count < MAX_PROBES) {
        strncpy(results[probe_count].name, name, PROBE_NAME_MAX - 1);
        results[probe_count].name[PROBE_NAME_MAX - 1] = '\0';
        results[probe_count].passed = 0;
    }
}

static void probe_end(int passed)
{
    if (probe_count < MAX_PROBES) {
        results[probe_count].passed = passed;
            printf("  %-18s %s\n", results[probe_count].name, passed ? "PASS" : "FAIL");
    }
    probe_count++;
}

static int run_probe_sigsys(int (*fn)(void))
{
    struct sigaction sa, old;
    sa.sa_handler = sigsys_handler;
    sigemptyset(&sa.sa_mask);
    sa.sa_flags = 0;
    sigaction(SIGSYS, &sa, &old);

    caught_sigsys = 0;
    int result = 0;

    if (sigsetjmp(jmpbuf, 1) == 0) {
        result = fn();
    }

    sigaction(SIGSYS, &old, NULL);
    return caught_sigsys ? 0 : result;
}

static int probe_socket(void)
{
    int fd = socket(AF_INET, SOCK_STREAM, 0);
    if (fd >= 0) { close(fd); return 1; }
    return 0;
}

static int probe_connect(void)
{
    int fd = socket(AF_INET, SOCK_STREAM, 0);
    if (fd < 0) return 0;
    struct sockaddr_in addr;
    memset(&addr, 0, sizeof(addr));
    addr.sin_family = AF_INET;
    addr.sin_port = htons(80);
    inet_pton(AF_INET, "1.1.1.1", &addr.sin_addr);
    int ret = connect(fd, (struct sockaddr *)&addr, sizeof(addr));
    close(fd);
    return ret == 0 ? 1 : 0;
}

static int probe_bind(void)
{
    int fd = socket(AF_INET, SOCK_STREAM, 0);
    if (fd < 0) return 0;
    struct sockaddr_in addr;
    memset(&addr, 0, sizeof(addr));
    addr.sin_family = AF_INET;
    addr.sin_port = htons(0);
    addr.sin_addr.s_addr = INADDR_ANY;
    int ret = bind(fd, (struct sockaddr *)&addr, sizeof(addr));
    close(fd);
    return ret == 0 ? 1 : 0;
}

static int probe_read_etc(void)
{
    int fd = open("/etc/passwd", O_RDONLY);
    if (fd < 0) return 0;
    char buf[64];
    int n = (int)read(fd, buf, sizeof(buf) - 1);
    close(fd);
    return n > 0 ? 1 : 0;
}

static int probe_write_etc(void)
{
    int fd = open("/etc/nexpack-probe-test", O_WRONLY | O_CREAT | O_TRUNC, 0644);
    if (fd >= 0) { close(fd); unlink("/etc/nexpack-probe-test"); return 1; }
    return 0;
}

static int probe_write_tmp(void)
{
    int fd = open("/tmp/nexpack-probe-write", O_WRONLY | O_CREAT | O_TRUNC, 0644);
    if (fd < 0) return 0;
    ssize_t r = write(fd, "test", 4);
    close(fd);
    unlink("/tmp/nexpack-probe-write");
    return r == 4 ? 1 : 0;
}

static int probe_fork(void)
{
    pid_t pid = fork();
    if (pid < 0) return 0;
    if (pid == 0) _exit(42);
    int status;
    waitpid(pid, &status, 0);
    return WIFEXITED(status) && WEXITSTATUS(status) == 42 ? 1 : 0;
}

static int probe_exec_true(void)
{
    pid_t pid = fork();
    if (pid < 0) return 0;
    if (pid == 0) {
        execl("/bin/true", "true", NULL);
        _exit(1);
    }
    int status;
    waitpid(pid, &status, 0);
    return WIFEXITED(status) && WEXITSTATUS(status) == 0 ? 1 : 0;
}

static int probe_proc_self(void)
{
    char buf[256];
    ssize_t n = (ssize_t)readlink("/proc/self/exe", buf, sizeof(buf) - 1);
    if (n <= 0) return 0;
    buf[n] = '\0';
    return n > 0 ? 1 : 0;
}

static int probe_env_home(void)
{
    const char *home = getenv("HOME");
    return home && home[0] ? 1 : 0;
}

static int probe_write_home(void)
{
    const char *home = getenv("HOME");
    if (!home) return 0;
    char path[512];
    snprintf(path, sizeof(path), "%s/nexpack-probe-home-test", home);
    int fd = open(path, O_WRONLY | O_CREAT | O_TRUNC, 0644);
    if (fd < 0) return 0;
    ssize_t r = write(fd, "test", 4);
    close(fd);
    unlink(path);
    return r == 4 ? 1 : 0;
}

static int probe_pipe(void)
{
    int fds[2];
    if (pipe(fds) < 0) return 0;
    ssize_t wr = write(fds[1], "x", 1);
    char buf;
    int n = (int)read(fds[0], &buf, 1);
    close(fds[0]);
    close(fds[1]);
    return wr == 1 && n == 1 ? 1 : 0;
}

int main(void)
{
    printf("nexpack-sandbox-probe v0.1.0\n");
    printf("============================\n");
    printf("Probe               Result\n");
    printf("----               ------\n");

    probe_begin("socket");
    probe_end(run_probe_sigsys(probe_socket));

    probe_begin("connect");
    probe_end(run_probe_sigsys(probe_connect));

    probe_begin("bind");
    probe_end(run_probe_sigsys(probe_bind));

    probe_begin("read /etc");
    probe_end(run_probe_sigsys(probe_read_etc));

    probe_begin("write /etc");
    probe_end(run_probe_sigsys(probe_write_etc));

    probe_begin("write /tmp");
    probe_end(run_probe_sigsys(probe_write_tmp));

    probe_begin("fork");
    probe_end(run_probe_sigsys(probe_fork));

    probe_begin("execve");
    probe_end(run_probe_sigsys(probe_exec_true));

    probe_begin("readlink /proc");
    probe_end(run_probe_sigsys(probe_proc_self));

    probe_begin("getenv HOME");
    probe_end(run_probe_sigsys(probe_env_home));

    probe_begin("write $HOME");
    probe_end(run_probe_sigsys(probe_write_home));

    probe_begin("pipe");
    probe_end(run_probe_sigsys(probe_pipe));

    int failures = 0;
    for (int i = 0; i < probe_count; i++) {
        if (!results[i].passed) failures++;
    }

    printf("----             ------\n");
    printf("Summary: %d/%d probes passed, %d failed\n",
           probe_count - failures, probe_count, failures);

    return failures > 0 ? (failures < 128 ? failures : 127) : 0;
}
