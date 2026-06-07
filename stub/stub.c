#define SOCKET_NAME  "/nexpack.sock"

#define SYS_read   0
#define SYS_write  1
#define SYS_open   2
#define SYS_close  3
#define SYS_access 21
#define SYS_socket 41
#define SYS_connect 42
#define SYS_execve 59
#define SYS_exit   60
#define SYS_exit_group 231

#define AF_UNIX    1
#define SOCK_STREAM 1

#define BUF_SIZE   4096
#define MAX_ARGS   64
#define MAX_ARG_LEN 256

struct sockaddr_un {
    unsigned short sun_family;
    char sun_path[108];
};

static long sys_call(long number, long arg1, long arg2, long arg3)
{
    long ret;
    __asm__ volatile (
        "syscall"
        : "=a" (ret)
        : "a" (number), "D" (arg1), "S" (arg2), "d" (arg3)
        : "rcx", "r11", "memory"
    );
    return ret;
}

static long sys_write(int fd, const char *buf, unsigned long len)
{
    return sys_call(SYS_write, fd, (long)buf, len);
}

static long sys_read(int fd, char *buf, unsigned long len)
{
    return sys_call(SYS_read, fd, (long)buf, len);
}

static long sys_close(int fd)
{
    return sys_call(SYS_close, fd, 0, 0);
}

static long sys_socket(int domain, int type, int protocol)
{
    return sys_call(SYS_socket, domain, type, protocol);
}

static long sys_connect(int fd, const struct sockaddr_un *addr,
                         unsigned long addrlen)
{
    return sys_call(SYS_connect, fd, (long)addr, addrlen);
}

static long sys_execve(const char *path, char *const argv[], char *const envp[])
{
    return sys_call(SYS_execve, (long)path, (long)argv, (long)envp);
}

static long sys_access(const char *path, int mode)
{
    return sys_call(SYS_access, (long)path, (long)mode, 0);
}

static void sys_exit(int code)
{
    sys_call(SYS_exit_group, code, 0, 0);
    sys_call(SYS_exit, code, 0, 0);
}

static int write_all(int fd, const char *buf, unsigned long len)
{
    unsigned long written = 0;
    while (written < len) {
        long n = sys_write(fd, buf + written, len - written);
        if (n < 0) return -1;
        written += (unsigned long)n;
    }
    return 0;
}

static int read_line(int fd, char *buf, unsigned long bufsz)
{
    unsigned long i = 0;
    while (i < bufsz - 1) {
        char c;
        long n = sys_read(fd, &c, 1);
        if (n <= 0) break;
        if (c == '\n') break;
        buf[i++] = c;
    }
    buf[i] = '\0';
    return (int)i;
}

static unsigned long str_len(const char *s)
{
    unsigned long n = 0;
    while (s[n]) n++;
    return n;
}

static const char *env_get(char *envp[], const char *name)
{
    unsigned long name_len = str_len(name);
    if (!envp) return 0;
    for (int i = 0; envp[i]; i++) {
        const char *e = envp[i];
        unsigned long j;
        for (j = 0; j < name_len && e[j]; j++) {
            if (e[j] != name[j]) break;
        }
        if (j == name_len && e[j] == '=') {
            return e + j + 1;
        }
    }
    return 0;
}

static const char *json_str_val(const char *json, const char *key)
{
    unsigned long key_len = str_len(key);
    const char *p = json;
    while (*p) {
        if (*p == '"') {
            p++;
            unsigned long j;
            for (j = 0; j < key_len && p[j] && p[j] == key[j]; j++);
            if (j == key_len && p[j] == '"') {
                p += j;
                if (p[0] == '"' && p[1] == ':') {
                    p += 2;
                    while (*p == ' ' || *p == '\t') p++;
                    if (*p == '"') {
                        return p + 1;
                    }
                }
            }
        }
        p++;
    }
    return 0;
}

static int read_json_str(const char *start, char *out, int max)
{
    int i = 0;
    while (start[i] && start[i] != '"' && i < max - 1) {
        out[i] = start[i];
        i++;
    }
    out[i] = '\0';
    return i;
}

static const char *json_arr_val(const char *json, const char *key)
{
    unsigned long key_len = str_len(key);
    const char *p = json;
    while (*p) {
        if (*p == '"') {
            p++;
            unsigned long j;
            for (j = 0; j < key_len && p[j] && p[j] == key[j]; j++);
            if (j == key_len && p[j] == '"') {
                p += j;
                if (p[0] == '"' && p[1] == ':') {
                    p += 2;
                    while (*p == ' ' || *p == '\t') p++;
                    if (*p == '[') {
                        return p + 1;
                    }
                }
            }
        }
        p++;
    }
    return 0;
}

static int read_arr_string(const char **pp, char *out, int max)
{
    const char *p = *pp;
    while (*p == ' ' || *p == '\t' || *p == '\n') p++;
    if (*p != '"') return -1;
    p++;
    int i = 0;
    while (*p && *p != '"' && i < max - 1) {
        if (*p == '\\') {
            p++;
            if (*p) {
                out[i++] = *p;
                p++;
            }
        } else {
            out[i++] = *p;
            p++;
        }
    }
    if (*p != '"') return -1;
    out[i] = '\0';
    p++;
    while (*p == ' ' || *p == '\t' || *p == '\n') p++;
    if (*p == ',') {
        p++;
        while (*p == ' ' || *p == '\t' || *p == '\n') p++;
    }
    *pp = p;
    return 0;
}

static int find_in_path(const char *name, char *out, int max, char *envp[])
{
    const char *path_str = env_get(envp, "PATH");
    if (!path_str) return -1;

    const char *start = path_str;
    const char *end;
    while (*start) {
        end = start;
        while (*end && *end != ':') end++;

        unsigned long dir_len = (unsigned long)(end - start);
        unsigned long name_len = str_len(name);
        if (dir_len + 1 + name_len + 1 < (unsigned long)max) {
            unsigned long k;
            for (k = 0; k < dir_len; k++) out[k] = start[k];
            out[k++] = '/';
            for (unsigned long m = 0; m < name_len; m++) out[k++] = name[m];
            out[k] = '\0';
            if (sys_access(out, 0) == 0) return 0;
        }

        if (*end == '\0') break;
        start = end + 1;
    }
    return -1;
}

void _start(void)
{
    unsigned long *sp;
    __asm__ volatile ("mov %%rsp, %0" : "=r"(sp));

    int argc = (int)sp[0];
    char **argv = (char **)(sp + 1);
    char **envp = argv + argc + 1;

    const char *self_path = argv[0];
    const char *runtime_dir = env_get(envp, "XDG_RUNTIME_DIR");

    if (!runtime_dir || !runtime_dir[0])
        runtime_dir = "/tmp";

    char sock_path[128];
    unsigned long n = 0;

    while (runtime_dir[n] && n < sizeof(sock_path) - sizeof(SOCKET_NAME) - 1) {
        sock_path[n] = runtime_dir[n];
        n++;
    }

    const char *sn = SOCKET_NAME;
    while (*sn && n < sizeof(sock_path) - 1) {
        sock_path[n++] = *sn++;
    }
    sock_path[n] = '\0';

    long sock = sys_socket(AF_UNIX, SOCK_STREAM, 0);
    if (sock < 0) {
        const char m[] = "nexpack: socket() failed\n";
        sys_write(2, m, str_len(m));
        sys_exit(1);
    }

    struct sockaddr_un addr;
    addr.sun_family = AF_UNIX;

    unsigned long i;
    for (i = 0; i < sizeof(addr.sun_path) - 1 && sock_path[i]; i++)
        addr.sun_path[i] = sock_path[i];
    addr.sun_path[i] = '\0';

    if (sys_connect((int)sock, &addr,
                    (unsigned long)&((struct sockaddr_un *)0)->sun_path
                    + i + 1) < 0) {
        sys_close((int)sock);
        const char m[] = "nexpack: daemon not available. Run: nexpackd &\n";
        sys_write(2, m, str_len(m));
        sys_exit(1);
    }

    char request[BUF_SIZE];
    unsigned long pos = 0;
    {
        const char *p = "{\"method\":\"mount\",\"bundle\":\"";
        while (*p && pos < BUF_SIZE - 3) {
            request[pos++] = *p++;
        }

        const char *p2 = self_path;
        while (*p2 && pos < BUF_SIZE - 3) {
            if (*p2 == '"' || *p2 == '\\')
                request[pos++] = '\\';
            request[pos++] = *p2++;
        }
        request[pos++] = '"';
        request[pos++] = '}';
        request[pos++] = '\n';
    }

    if (write_all((int)sock, request, pos) < 0) {
        sys_close((int)sock);
        sys_exit(1);
    }

    char response[BUF_SIZE];
    if (read_line((int)sock, response, BUF_SIZE) < 0) {
        sys_close((int)sock);
        sys_exit(1);
    }

    sys_close((int)sock);

    char bwrap_bin[MAX_ARG_LEN] = {0};
    char bwrap_args_buf[MAX_ARGS][MAX_ARG_LEN];
    char *bwrap_argv[MAX_ARGS + 1];
    int nargs = 0;

    const char *arr_start = json_arr_val(response, "bwrap_args");
    if (arr_start) {
        const char *p = arr_start;
        while (*p != ']' && *p != '\0' && nargs < MAX_ARGS) {
            if (read_arr_string(&p, bwrap_args_buf[nargs], MAX_ARG_LEN) == 0) {
                bwrap_argv[nargs] = bwrap_args_buf[nargs];
                nargs++;
            } else {
                break;
            }
        }
    }

    if (nargs > 0) {
        char *final_argv[MAX_ARGS + 2];
        int ai = 0;

        if (find_in_path("bwrap", bwrap_bin, sizeof(bwrap_bin), envp) != 0) {
            const char m[] = "nexpack: bwrap not found in PATH\n";
            sys_write(2, m, str_len(m));
            sys_exit(1);
        }
        final_argv[ai++] = bwrap_bin;

        for (int j = 0; j < nargs; j++) {
            final_argv[ai++] = bwrap_argv[j];
        }

        for (int j = 1; j < argc; j++) {
            if (ai < MAX_ARGS + 1) {
                final_argv[ai++] = argv[j];
            }
        }
        final_argv[ai] = 0;

        sys_execve(bwrap_bin, final_argv, envp);

        const char m[] = "nexpack: exec bwrap failed\n";
        sys_write(2, m, str_len(m));
        sys_exit(1);
    }

    char entrypoint[256] = {0};
    const char *ep_val = json_str_val(response, "entrypoint");
    if (ep_val) {
        read_json_str(ep_val, entrypoint, sizeof(entrypoint));
    }

    if (entrypoint[0]) {
        const char m1[] = "nexpack: ";
        const char m2[] = " mounted, no sandbox args from daemon\n";
        sys_write(2, m1, str_len(m1));
        sys_write(2, entrypoint, str_len(entrypoint));
        sys_write(2, m2, str_len(m2));
    }

    sys_exit(0);
}
