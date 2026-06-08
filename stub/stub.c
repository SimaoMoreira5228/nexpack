#define SOCKET_NAME  "/nexpack.sock"
#define BOOTSTRAP_MAGIC "NXBT"

#define SYS_read   0
#define SYS_write  1
#define SYS_open   2
#define SYS_close  3
#define SYS_fstat  5
#define SYS_lseek  8
#define SYS_mkdir 83
#define SYS_chmod 90
#define SYS_access 21
#define SYS_socket 41
#define SYS_connect 42
#define SYS_clone 56
#define SYS_fork  57
#define SYS_execve 59
#define SYS_exit   60
#define SYS_exit_group 231
#define SYS_nanosleep 35

#define AF_UNIX    1
#define SOCK_STREAM 1
#define SIGCHLD    17
#define SEEK_SET   0
#define SEEK_END   2
#define O_RDONLY   0
#define O_WRONLY   1
#define O_CREAT    0100
#define O_TRUNC    01000

#define BUF_SIZE   4096
#define MAX_ARGS   64
#define MAX_ARG_LEN 256

struct sockaddr_un {
    unsigned short sun_family;
    char sun_path[108];
};

struct timespec {
    long tv_sec;
    long tv_nsec;
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

static long sys_open(const char *path, int flags, int mode)
{
    return sys_call(SYS_open, (long)path, (long)flags, (long)mode);
}

static long sys_lseek(int fd, long offset, int whence)
{
    return sys_call(SYS_lseek, fd, offset, whence);
}

static long sys_socket(int domain, int type, int protocol)
{
    return sys_call(SYS_socket, domain, type, protocol);
}

static long sys_connect(int fd, const struct sockaddr_un *addr,
                         unsigned long addrlen)
{
    return sys_call(SYS_connect, (long)fd, (long)addr, addrlen);
}

static long sys_execve(const char *path, char *const argv[], char *const envp[])
{
    return sys_call(SYS_execve, (long)path, (long)argv, (long)envp);
}

static long sys_access(const char *path, int mode)
{
    return sys_call(SYS_access, (long)path, (long)mode, 0);
}

static long sys_fork(void)
{
    return sys_call(SYS_fork, 0, 0, 0);
}

static long sys_mkdir(const char *path, unsigned short mode)
{
    return sys_call(SYS_mkdir, (long)path, (long)mode, 0);
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

static int read_all(int fd, char *buf, unsigned long len)
{
    unsigned long read_bytes = 0;
    while (read_bytes < len) {
        long n = sys_read(fd, buf + read_bytes, len - read_bytes);
        if (n <= 0) return -1;
        read_bytes += (unsigned long)n;
    }
    return 0;
}

static unsigned long str_len(const char *s)
{
    unsigned long n = 0;
    while (s[n]) n++;
    return n;
}

static void str_copy(const char *src, char *dst, unsigned long max)
{
    unsigned long i = 0;
    while (src[i] && i < max - 1) {
        dst[i] = src[i];
        i++;
    }
    dst[i] = '\0';
}

static int make_dir_for(const char *path)
{
    long ret = sys_mkdir(path, 0700);
    if (ret == 0 || ret == -17) return 0;
    return -1;
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

static int read_bootstrap_trailer(const char *path, unsigned long *offset_out, unsigned long *size_out)
{
    int fd = (int)sys_open(path, O_RDONLY, 0);
    if (fd < 0) return -1;

    long file_size = sys_lseek(fd, 0, SEEK_END);
    if (file_size < 20) { sys_close(fd); return -1; }

    unsigned long trailer_off = (unsigned long)file_size - 20;
    sys_lseek(fd, (long)trailer_off, SEEK_SET);

    unsigned char trailer[20];
    if (read_all(fd, (char *)trailer, 20) < 0) { sys_close(fd); return -1; }
    sys_close(fd);

    if (trailer[0] != 'N' || trailer[1] != 'X' || trailer[2] != 'B' || trailer[3] != 'T')
        return -1;

    *offset_out = (unsigned long)trailer[4]
        | ((unsigned long)trailer[5] << 8)
        | ((unsigned long)trailer[6] << 16)
        | ((unsigned long)trailer[7] << 24)
        | ((unsigned long)trailer[8] << 32)
        | ((unsigned long)trailer[9] << 40)
        | ((unsigned long)trailer[10] << 48)
        | ((unsigned long)trailer[11] << 56);

    *size_out = (unsigned long)trailer[12]
        | ((unsigned long)trailer[13] << 8)
        | ((unsigned long)trailer[14] << 16)
        | ((unsigned long)trailer[15] << 24)
        | ((unsigned long)trailer[16] << 32)
        | ((unsigned long)trailer[17] << 40)
        | ((unsigned long)trailer[18] << 48)
        | ((unsigned long)trailer[19] << 56);

    return 0;
}

static int extract_self_bootstrap(const char *bundle_path, const char *bin_dir)
{
    unsigned long boot_off, boot_size;
    if (read_bootstrap_trailer(bundle_path, &boot_off, &boot_size) != 0)
        return -1;

    int fd = (int)sys_open(bundle_path, O_RDONLY, 0);
    if (fd < 0) return -1;

    sys_lseek(fd, (long)boot_off, SEEK_SET);

    unsigned char hdr[4];
    if (read_all(fd, (char *)hdr, 4) < 0) { sys_close(fd); return -1; }
    unsigned int count = (unsigned int)hdr[0]
        | ((unsigned int)hdr[1] << 8)
        | ((unsigned int)hdr[2] << 16)
        | ((unsigned int)hdr[3] << 24);

    for (unsigned int i = 0; i < count; i++) {
        unsigned char nl[2];
        if (read_all(fd, (char *)nl, 2) < 0) { sys_close(fd); return -1; }
        unsigned int name_len = (unsigned int)nl[0] | ((unsigned int)nl[1] << 8);

        unsigned char name[256];
        if (name_len >= sizeof(name)) { sys_close(fd); return -1; }
        if (read_all(fd, (char *)name, name_len) < 0) { sys_close(fd); return -1; }
        name[name_len] = '\0';

        unsigned char dl[4];
        if (read_all(fd, (char *)dl, 4) < 0) { sys_close(fd); return -1; }
        unsigned long data_len = (unsigned long)dl[0]
            | ((unsigned long)dl[1] << 8)
            | ((unsigned long)dl[2] << 16)
            | ((unsigned long)dl[3] << 24);

        char file_path[512];
        unsigned long pi = 0;
        while (bin_dir[pi] && pi < sizeof(file_path) - 2) {
            file_path[pi] = bin_dir[pi];
            pi++;
        }
        file_path[pi++] = '/';
        unsigned long ni = 0;
        while (name[ni] && pi < sizeof(file_path) - 1) {
            file_path[pi++] = (char)name[ni++];
        }
        file_path[pi] = '\0';

        char fbuf[4096];
        unsigned long remaining = data_len;

        int wfd = (int)sys_open(file_path, O_WRONLY | O_CREAT | O_TRUNC, 0700);
        if (wfd < 0) { sys_close(fd); return -1; }

        while (remaining > 0) {
            unsigned long chunk = remaining < sizeof(fbuf) ? remaining : sizeof(fbuf);
            if (read_all(fd, fbuf, chunk) < 0) { sys_close(wfd); sys_close(fd); return -1; }
            if (write_all(wfd, fbuf, chunk) < 0) { sys_close(wfd); sys_close(fd); return -1; }
            remaining -= chunk;
        }

        sys_close(wfd);
    }

    sys_close(fd);
    return 0;
}

static int try_bootstrap_and_start_daemon(const char *bundle_path, char *envp[], char *nxpk_out, unsigned long nxpk_max)
{
    const char *home = env_get(envp, "HOME");
    if (!home) return -1;

    char bin_dir[256];
    unsigned long p = 0;

    while (home[p] && p < sizeof(bin_dir) - 1) {
        bin_dir[p] = home[p];
        p++;
    }
    const char *suffix = "/.local/share/nexpack/bin";
    unsigned long si = 0;
    while (suffix[si] && p < sizeof(bin_dir) - 1) {
        bin_dir[p++] = suffix[si++];
    }
    bin_dir[p] = '\0';

    char mkdir_buf[256];
    str_copy(home, mkdir_buf, sizeof(mkdir_buf));
    unsigned long mpi = str_len(mkdir_buf);
    const char *parts[] = { ".local", "share", "nexpack", "bin", 0 };
    for (int i = 0; parts[i]; i++) {
        mkdir_buf[mpi++] = '/';
        str_copy(parts[i], mkdir_buf + mpi, sizeof(mkdir_buf) - mpi);
        make_dir_for(mkdir_buf);
        mpi = str_len(mkdir_buf);
    }

    if (extract_self_bootstrap(bundle_path, bin_dir) != 0)
        return -1;

    char nexpackd_path[512];
    unsigned long np = 0;
    while (bin_dir[np] && np < sizeof(nexpackd_path) - 1) {
        nexpackd_path[np] = bin_dir[np];
        np++;
    }
    const char *bn = "/nexpackd";
    unsigned long bni = 0;
    while (bn[bni] && np < sizeof(nexpackd_path) - 1) {
        nexpackd_path[np++] = bn[bni++];
    }
    nexpackd_path[np] = '\0';

    unsigned long np2 = 0;
    while (bin_dir[np2] && np2 < nxpk_max - 1) {
        nxpk_out[np2] = bin_dir[np2];
        np2++;
    }
    const char *bn2 = "/nxpk";
    unsigned long bni2 = 0;
    while (bn2[bni2] && np2 < nxpk_max - 1) {
        nxpk_out[np2++] = bn2[bni2++];
    }
    nxpk_out[np2] = '\0';

    long pid = sys_fork();
    if (pid == 0) {
        char *daemon_argv[] = { nexpackd_path, 0 };
        sys_execve(nexpackd_path, daemon_argv, envp);
        sys_exit(1);
    }

    if (pid < 0) return -1;

    {
        struct timespec ts;
        ts.tv_sec = 0;
        ts.tv_nsec = 800000000;
        sys_call(SYS_nanosleep, (long)&ts, 0, 0);
    }

    return 0;
}

static int try_connect_daemon(char *sock_path)
{
    long sock = sys_socket(AF_UNIX, SOCK_STREAM, 0);
    if (sock < 0) return -1;

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
        return -1;
    }

    return (int)sock;
}

void _start_c(unsigned long *sp);

__attribute__((naked))
void _start(void)
{
    __asm__ volatile ("movq %%rsp, %%rdi; jmp _start_c" ::: "rdi");
}

void _start_c(unsigned long *sp)
{
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

    long sock = try_connect_daemon(sock_path);

    if (sock < 0) {
        char nxpk_bin[MAX_ARG_LEN] = {0};
        if (find_in_path("nxpk", nxpk_bin, sizeof(nxpk_bin), envp) == 0) {
            char *fallback_argv[MAX_ARGS + 3];
            int fi = 0;
            fallback_argv[fi++] = nxpk_bin;
            fallback_argv[fi++] = "run";
            fallback_argv[fi++] = (char *)self_path;
            for (int j = 1; j < argc && fi < MAX_ARGS + 2; j++) {
                fallback_argv[fi++] = argv[j];
            }
            fallback_argv[fi] = 0;
            sys_execve(nxpk_bin, fallback_argv, envp);
        }

        char extracted_nxpk[MAX_ARG_LEN] = {0};
        if (try_bootstrap_and_start_daemon(self_path, envp, extracted_nxpk, sizeof(extracted_nxpk)) == 0) {
            sock = try_connect_daemon(sock_path);
            if (sock >= 0) {
                sys_close((int)sock);
                char *exec_argv[MAX_ARGS + 3];
                int fi = 0;
                exec_argv[fi++] = extracted_nxpk;
                exec_argv[fi++] = "run";
                exec_argv[fi++] = (char *)self_path;
                for (int j = 1; j < argc && fi < MAX_ARGS + 2; j++) {
                    exec_argv[fi++] = argv[j];
                }
                exec_argv[fi] = 0;
                sys_execve(extracted_nxpk, exec_argv, envp);
            }
        }

        const char m[] = "nexpack: daemon not available. Install nxpk or run: nexpackd &\n";
        sys_write(2, m, str_len(m));
        sys_exit(1);
    }

    sys_exit(0);
}
