/*
 * Custom minimal init for ODROID Go Ultra (Amlogic S922X) ostree boot
 *
 * Replaces dracut - dracut's large initramfs causes CMA memory pressure
 * that kills the MIPI-DSI display on the S922X.
 *
 * This init:
 *  1. Mounts root partition (ext4)
 *  2. Auto-discovers the ostree deployment
 *  3. Bind-mounts deployment as the real root
 *  4. switch_roots to systemd
 *
 * Compiled statically with musl (~120KB), produces ~80KB initramfs.
 */
#include <stdio.h>
#include <unistd.h>
#include <fcntl.h>
#include <string.h>
#include <sys/mount.h>
#include <sys/stat.h>
#include <sys/syscall.h>
#include <errno.h>

static int dbg = -1;

static void ws(int fd, const char *s) { write(fd, s, strlen(s)); }

static void dbglog(const char *msg) {
    ws(1, msg);
    if (dbg >= 0) ws(dbg, msg);
}

static void dbgerr(const char *msg) {
    char buf[256];
    snprintf(buf, sizeof(buf), "%s: errno=%d\n", msg, errno);
    dbglog(buf);
}

struct linux_dirent64 {
    unsigned long long d_ino;
    long long d_off;
    unsigned short d_reclen;
    unsigned char d_type;
    char d_name[];
};

/* Find first non-dot directory entry in a directory, return name in buf */
static int find_first_dir(const char *path, char *buf, int bufsz) {
    int fd = open(path, O_RDONLY | O_DIRECTORY);
    if (fd < 0) return -1;
    char dbuf[4096];
    int nread;
    while ((nread = syscall(SYS_getdents64, fd, dbuf, sizeof(dbuf))) > 0) {
        int pos = 0;
        while (pos < nread) {
            struct linux_dirent64 *d = (struct linux_dirent64 *)(dbuf + pos);
            if (d->d_name[0] != '.' && (d->d_type == 4 || d->d_type == 10)) {
                int len = strlen(d->d_name);
                if (len < bufsz) {
                    memcpy(buf, d->d_name, len + 1);
                    close(fd);
                    return 0;
                }
            }
            pos += d->d_reclen;
        }
    }
    close(fd);
    return -1;
}

/* Parse ostree= from /proc/cmdline */
static int get_cmdline_param(const char *key, char *buf, int bufsz) {
    int fd = open("/proc/cmdline", O_RDONLY);
    if (fd < 0) return -1;
    char cmdline[2048];
    int n = read(fd, cmdline, sizeof(cmdline) - 1);
    close(fd);
    if (n <= 0) return -1;
    cmdline[n] = 0;
    int keylen = strlen(key);
    char *p = cmdline;
    while ((p = strstr(p, key)) != NULL) {
        if (p == cmdline || *(p - 1) == ' ') {
            p += keylen;
            int i = 0;
            while (*p && *p != ' ' && *p != '\n' && i < bufsz - 1)
                buf[i++] = *p++;
            buf[i] = 0;
            return 0;
        }
        p++;
    }
    return -1;
}

/* Auto-discover ostree deployment path under /sysroot */
static int discover_deployment(char *deploy_path, int bufsz) {
    /* Try ostree= kernel parameter first */
    char ostree_param[512];
    if (get_cmdline_param("ostree=", ostree_param, sizeof(ostree_param)) == 0) {
        dbglog("  ostree= "); dbglog(ostree_param); dbglog("\n");

        /* Resolve the symlink: /sysroot + ostree_param */
        char sympath[768];
        snprintf(sympath, sizeof(sympath), "/sysroot%s", ostree_param);
        char link[512];
        int lr = readlink(sympath, link, sizeof(link) - 1);
        if (lr > 0) {
            link[lr] = 0;
            /* Extract the deployment hash from the end of the symlink target */
            char *last = strrchr(link, '/');
            if (last) last++;
            else last = link;
            /* Find the stateroot from the ostree= path */
            /* ostree=/ostree/boot.1/<stateroot>/<hash>/0 */
            char stateroot[64] = "default";
            char *p = strstr(ostree_param, "boot.1/");
            if (p) {
                p += 7;
                int i = 0;
                while (*p && *p != '/' && i < (int)sizeof(stateroot) - 1)
                    stateroot[i++] = *p;
                stateroot[i] = 0;
            }
            snprintf(deploy_path, bufsz,
                     "/sysroot/ostree/deploy/%s/deploy/%s", stateroot, last);
            return 0;
        }
    }

    /* Fallback: auto-discover by walking the directory tree */
    dbglog("  Auto-discovering deployment...\n");

    /* /sysroot/ostree/boot.1 -> boot.1.1 (symlink) */
    /* /sysroot/ostree/boot.1.1/<stateroot>/<boothash>/0 -> deployment */
    char stateroot[128];
    if (find_first_dir("/sysroot/ostree/boot.1.1", stateroot, sizeof(stateroot)) != 0) {
        /* Try boot.1 directly */
        if (find_first_dir("/sysroot/ostree/boot.1", stateroot, sizeof(stateroot)) != 0) {
            dbglog("  ERROR: No stateroot found\n");
            return -1;
        }
    }
    dbglog("  stateroot: "); dbglog(stateroot); dbglog("\n");

    char hashdir_path[512];
    snprintf(hashdir_path, sizeof(hashdir_path),
             "/sysroot/ostree/boot.1.1/%s", stateroot);
    char boothash[128];
    if (find_first_dir(hashdir_path, boothash, sizeof(boothash)) != 0) {
        dbglog("  ERROR: No boot hash found\n");
        return -1;
    }
    dbglog("  boothash: "); dbglog(boothash); dbglog("\n");

    /* Read the "0" symlink */
    char sympath[768];
    snprintf(sympath, sizeof(sympath), "%s/%s/0", hashdir_path, boothash);
    char link[512];
    int lr = readlink(sympath, link, sizeof(link) - 1);
    if (lr <= 0) {
        dbgerr("  ERROR: readlink deployment pointer");
        return -1;
    }
    link[lr] = 0;

    /* Extract deploy hash from link target */
    char *last = strrchr(link, '/');
    if (last) last++;
    else last = link;

    snprintf(deploy_path, bufsz,
             "/sysroot/ostree/deploy/%s/deploy/%s", stateroot, last);
    return 0;
}

int main() {
    int con = open("/dev/console", O_RDWR);
    if (con < 0) con = open("/dev/tty0", O_RDWR);
    if (con >= 0) { dup2(con, 0); dup2(con, 1); dup2(con, 2); }

    mount("proc", "/proc", "proc", 0, NULL);
    mount("sysfs", "/sys", "sysfs", 0, NULL);
    mount("devtmpfs", "/dev", "devtmpfs", 0, NULL);

    dbglog("\n=== OSTREE BOOT INIT ===\n");

    /* Mount boot FAT partition for debug logging */
    mkdir("/mnt", 0755);
    mkdir("/mnt/boot", 0755);
    if (mount("/dev/mmcblk1p1", "/mnt/boot", "vfat", 0, NULL) != 0)
        mount("/dev/mmcblk0p1", "/mnt/boot", "vfat", 0, NULL);

    dbg = open("/mnt/boot/debug.txt", O_WRONLY | O_CREAT | O_TRUNC, 0644);
    dbglog("=== OSTREE BOOT INIT ===\n\n");

    /* Mount root partition */
    dbglog("Mounting root...\n");
    mkdir("/sysroot", 0755);
    int rootok = 0;
    /* Try common eMMC/SD device paths */
    const char *root_devs[] = {
        "/dev/mmcblk1p3", "/dev/mmcblk0p3",
        "/dev/mmcblk2p3", NULL
    };
    for (int i = 0; root_devs[i]; i++) {
        if (mount(root_devs[i], "/sysroot", "ext4", 0, NULL) == 0) {
            dbglog("  Mounted "); dbglog(root_devs[i]); dbglog("\n");
            rootok = 1;
            break;
        }
    }
    if (!rootok) {
        dbgerr("  FAILED to mount root");
        goto fail;
    }

    /* Discover ostree deployment */
    dbglog("Resolving ostree deployment...\n");
    char deploy_path[768];
    if (discover_deployment(deploy_path, sizeof(deploy_path)) != 0) {
        dbglog("FAILED to discover deployment\n");
        goto fail;
    }
    dbglog("Deploy: "); dbglog(deploy_path); dbglog("\n");

    /* Verify deployment exists */
    struct stat st;
    if (stat(deploy_path, &st) != 0 || !S_ISDIR(st.st_mode)) {
        dbgerr("Deployment directory not found");
        goto fail;
    }

    /* Find stateroot for var bind mount */
    /* deploy_path = /sysroot/ostree/deploy/<stateroot>/deploy/<hash>.0 */
    char var_path[768];
    {
        /* Extract stateroot from deploy_path */
        const char *p = deploy_path + strlen("/sysroot/ostree/deploy/");
        char stateroot[64];
        int i = 0;
        while (*p && *p != '/' && i < (int)sizeof(stateroot) - 1)
            stateroot[i++] = *p++;
        stateroot[i] = 0;
        snprintf(var_path, sizeof(var_path),
                 "/sysroot/ostree/deploy/%s/var", stateroot);
    }

    /* Bind mount deployment to /realroot */
    mkdir("/realroot", 0755);
    if (mount(deploy_path, "/realroot", NULL, MS_BIND, NULL) != 0) {
        dbgerr("FAILED bind mount deployment");
        goto fail;
    }
    dbglog("Bind mounted deployment to /realroot\n");

    /* Bind mount stateroot var */
    if (mount(var_path, "/realroot/var", NULL, MS_BIND, NULL) != 0) {
        dbgerr("WARN: bind mount var failed");
    } else {
        dbglog("Bind mounted var\n");
    }

    /* Move /sysroot under deployment's sysroot dir */
    if (mount("/sysroot", "/realroot/sysroot", NULL, MS_MOVE, NULL) != 0) {
        dbgerr("FAILED to move /sysroot");
        goto fail;
    }
    dbglog("Moved /sysroot to /realroot/sysroot\n");

    /* Move virtual filesystems */
    mount("/proc", "/realroot/proc", NULL, MS_MOVE, NULL);
    mount("/sys", "/realroot/sys", NULL, MS_MOVE, NULL);
    mount("/dev", "/realroot/dev", NULL, MS_MOVE, NULL);
    dbglog("Moved proc/sys/dev\n");

    /* Close debug file and unmount boot FAT */
    dbglog("\n=== SWITCHING ROOT ===\n");
    if (dbg >= 0) { sync(); close(dbg); dbg = -1; }
    umount("/mnt/boot");

    /* switch_root */
    chdir("/realroot");
    if (mount("/realroot", "/", NULL, MS_MOVE, NULL) != 0) {
        ws(1, "FATAL: mount --move /realroot / failed\n");
        goto sleep_forever;
    }
    chroot(".");
    chdir("/");

    /* Exec systemd */
    char *init_argv[] = { "/sbin/init", NULL };
    char *init_envp[] = {
        "HOME=/",
        "TERM=linux",
        "PATH=/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin",
        NULL
    };
    execve("/sbin/init", init_argv, init_envp);
    init_argv[0] = "/usr/lib/systemd/systemd";
    execve("/usr/lib/systemd/systemd", init_argv, init_envp);

    ws(1, "FATAL: execve failed\n");

sleep_forever:
    while (1) sleep(9999);
    return 0;

fail:
    if (dbg >= 0) {
        ws(dbg, "\n=== BOOT FAILED - DMESG ===\n");
        int kf = open("/dev/kmsg", O_RDONLY | O_NONBLOCK);
        if (kf >= 0) {
            char buf[4096];
            int n;
            while ((n = read(kf, buf, sizeof(buf) - 1)) > 0) {
                buf[n] = 0;
                ws(dbg, buf);
            }
            close(kf);
        }
        sync();
        close(dbg);
    }
    umount("/mnt/boot");
    ws(1, "Boot failed. Sleeping forever.\n");
    goto sleep_forever;
}
