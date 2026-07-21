#define SYS_write 1
#define SYS_exit 60
#define SYS_exit_group 231
#define SYS_getpid 39
#define SYS_getcwd 79
#define SYS_open 2
#define SYS_close 3
#define SYS_read 0
#define SYS_lseek 8

#define O_RDONLY 0
#define O_WRONLY 1
#define O_CREAT 0100
#define O_TRUNC 01000
#define SEEK_SET 0

static long sys_call(long n, long a1, long a2, long a3)
{
    long ret;
    __asm__ volatile("syscall" : "=a"(ret) : "a"(n), "D"(a1), "S"(a2), "d"(a3) : "rcx", "r11", "memory");
    return ret;
}

static void print(const char *s)
{
    unsigned long n = 0;
    while (s[n]) n++;
    sys_call(SYS_write, 1, (long)s, n);
}

static void print_dec(long v)
{
    char buf[32];
    int i = 0;
    if (v < 0) { print("-"); v = -v; }
    if (v == 0) { print("0"); return; }
    while (v > 0 && i < 30) { buf[i++] = '0' + (v % 10); v /= 10; }
    while (i > 0) sys_call(SYS_write, 1, (long)(buf + --i), 1);
}

static void print_dec_nl(long v)
{
    print_dec(v);
    print("\n");
}

static long open_read(const char *path)
{
    return sys_call(SYS_open, (long)path, O_RDONLY, 0);
}

static long open_write(const char *path)
{
    return sys_call(SYS_open, (long)path, O_WRONLY | O_CREAT | O_TRUNC, 0644);
}

static long read_fd(int fd, char *buf, unsigned long len)
{
    return sys_call(SYS_read, fd, (long)buf, len);
}

static int str_eq(const char *a, const char *b)
{
    while (*a && *b && *a == *b) { a++; b++; }
    return *a == *b;
}

static unsigned long str_len(const char *s)
{
    unsigned long n = 0;
    while (s[n]) n++;
    return n;
}

static const char *find_env(unsigned long *sp, const char *name)
{
    int argc = (int)sp[0];
    char **envp = (char **)(sp + 1) + argc + 1;
    unsigned long name_len = str_len(name);
    for (int i = 0; envp[i]; i++) {
        unsigned long j;
        for (j = 0; j < name_len && envp[i][j]; j++) {
            if (envp[i][j] != name[j]) break;
        }
        if (j == name_len && envp[i][j] == '=')
            return envp[i] + j + 1;
    }
    return 0;
}

void _start_c(unsigned long *sp);

__attribute__((naked))
void _start(void)
{
    __asm__ volatile("movq %%rsp, %%rdi; jmp _start_c" ::: "rdi");
}

void _start_c(unsigned long *sp)
{
    int argc = (int)sp[0];
    char **argv = (char **)(sp + 1);

    print("=== nexpack test: simple ===\n");

    print("PID: ");
    print_dec_nl(sys_call(SYS_getpid, 0, 0, 0));

    print("Args:");
    for (int i = 0; i < argc; i++) {
        print(" ");
        print(argv[i]);
    }
    print("\n");

    char cwd[256];
    long ret = sys_call(SYS_getcwd, (long)cwd, 256, 0);
    if (ret >= 0) {
        print("CWD: ");
        print(cwd);
        print("\n");
    }

    print("Env: HOME=");
    const char *home = find_env(sp, "HOME");
    print(home ? home : "(unset)");
    print("\n");

    print("Env: PATH=");
    const char *path = find_env(sp, "PATH");
    print(path ? path : "(unset)");
    print("\n");

    print("Marker: /tmp/nexpack-simple-marker ... ");
    int fd = (int)open_write("/tmp/nexpack-simple-marker");
    if (fd >= 0) {
        const char *msg = "nexpack-test-simple was here\n";
        sys_call(SYS_write, fd, (long)msg, str_len(msg));
        sys_call(SYS_close, fd, 0, 0);

        char buf[64];
        fd = (int)open_read("/tmp/nexpack-simple-marker");
        if (fd >= 0) {
            long n = read_fd(fd, buf, sizeof(buf) - 1);
            if (n > 0) {
                buf[n] = '\0';
                if (str_eq(buf, "nexpack-test-simple was here\n")) {
                    print("OK\n");
                } else {
                    print("CONTENT MISMATCH\n");
                }
            }
            sys_call(SYS_close, fd, 0, 0);
        }
    } else {
        print("FAIL\n");
    }

    print("Output: Hello from Nexpack!\n");
    print("Exit: 0 (success)\n");

    sys_call(SYS_exit_group, 0, 0, 0);
    sys_call(SYS_exit, 0, 0, 0);
}
