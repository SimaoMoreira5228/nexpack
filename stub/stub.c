#define SOCKET_NAME  "/nexpack.sock"

#define SYS_read   0
#define SYS_write  1
#define SYS_open   2
#define SYS_close  3
#define SYS_socket 41
#define SYS_connect 42
#define SYS_exit   60
#define SYS_exit_group 231

#define AF_UNIX    1
#define SOCK_STREAM 1

#define BUF_SIZE   4096
#define PATH_MAX   4096

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

    char entrypoint[256] = {0};
    const char *ep_val = json_str_val(response, "entrypoint");
    if (ep_val) {
        read_json_str(ep_val, entrypoint, sizeof(entrypoint));
    }

    // TODO: Exec bwrap with the mounted bundle as root, and --unshare-all for isolation
    if (entrypoint[0]) {
        /* Would exec entrypoint; for now, signal success */
    }

    sys_exit(0);
}
