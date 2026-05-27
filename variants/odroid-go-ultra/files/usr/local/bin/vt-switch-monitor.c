/*
 * vt-switch-monitor: Watches gpio-keys for L1+R1+Select held 500ms, then runs chvt.
 *
 * Runs on TTY2 (emulation side) to switch back to TTY1 (desktop).
 * On TTY1, niri-nav handles the same combo to switch to TTY2.
 *
 * Uses poll() with timeout to detect the held duration even without
 * new events arriving.
 *
 * Build: gcc -o vt-switch-monitor vt-switch-monitor.c
 */

#define _GNU_SOURCE
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>
#include <fcntl.h>
#include <dirent.h>
#include <time.h>
#include <poll.h>
#include <linux/input.h>
#include <linux/input-event-codes.h>

/* OGU gpio-keys evdev codes: L1=BTN_TL, R1=BTN_TR, F6=BTN_TRIGGER_HAPPY6 */
#define HOLD_MS 500

static int find_gpio_keys(void)
{
    DIR *dir = opendir("/sys/class/input");
    if (!dir)
        return -1;

    struct dirent *ent;
    while ((ent = readdir(dir)) != NULL) {
        if (strncmp(ent->d_name, "event", 5) != 0)
            continue;

        char path[256];
        snprintf(path, sizeof(path), "/sys/class/input/%s/device/name", ent->d_name);

        FILE *f = fopen(path, "r");
        if (!f)
            continue;

        char name[128] = {0};
        fgets(name, sizeof(name), f);
        fclose(f);

        /* Strip trailing newline */
        char *nl = strchr(name, '\n');
        if (nl) *nl = '\0';

        if (strcasestr(name, "gpio-keys") || strcasestr(name, "odroid") ||
            strcasestr(name, "go ultra")) {
            char devpath[64];
            snprintf(devpath, sizeof(devpath), "/dev/input/%s", ent->d_name);
            closedir(dir);
            int fd = open(devpath, O_RDONLY);
            if (fd >= 0)
                fprintf(stderr, "vt-switch-monitor: opened %s (%s)\n", devpath, name);
            return fd;
        }
    }

    closedir(dir);
    return -1;
}

static long elapsed_ms(struct timespec *start)
{
    struct timespec now;
    clock_gettime(CLOCK_MONOTONIC, &now);
    return (now.tv_sec - start->tv_sec) * 1000 +
           (now.tv_nsec - start->tv_nsec) / 1000000;
}

int main(void)
{
    int fd = find_gpio_keys();
    if (fd < 0) {
        fprintf(stderr, "vt-switch-monitor: no gpio-keys device found\n");
        return 1;
    }

    int l1 = 0, r1 = 0, sel = 0;
    int combo_active = 0;
    struct timespec combo_start = {0};

    struct pollfd pfd = { .fd = fd, .events = POLLIN };

    for (;;) {
        /* If combo is active, poll with timeout for remaining hold time */
        int timeout_ms = -1; /* block indefinitely */
        if (combo_active) {
            long remaining = HOLD_MS - elapsed_ms(&combo_start);
            if (remaining <= 0) {
                /* Combo held long enough - fire! */
                fprintf(stderr, "vt-switch-monitor: combo fired, switching to TTY1\n");
                system("sudo /usr/bin/chvt 1");
                combo_active = 0;
                l1 = r1 = sel = 0;
                continue;
            }
            timeout_ms = (int)remaining;
        }

        int ret = poll(&pfd, 1, timeout_ms);
        if (ret < 0)
            break; /* error */

        if (ret == 0) {
            /* Timeout - combo held long enough */
            if (combo_active) {
                fprintf(stderr, "vt-switch-monitor: combo fired, switching to TTY1\n");
                system("sudo /usr/bin/chvt 1");
                combo_active = 0;
                l1 = r1 = sel = 0;
            }
            continue;
        }

        /* Read events */
        struct input_event ev;
        if (read(fd, &ev, sizeof(ev)) != sizeof(ev))
            continue;

        if (ev.type != EV_KEY)
            continue;
        if (ev.value == 2) /* autorepeat */
            continue;

        int pressed = (ev.value == 1);
        switch (ev.code) {
        case BTN_TL:             l1 = pressed;  break;
        case BTN_TR:             r1 = pressed;  break;
        case BTN_TRIGGER_HAPPY6: sel = pressed; break;
        default: continue;
        }

        if (l1 && r1 && sel) {
            if (!combo_active) {
                combo_active = 1;
                clock_gettime(CLOCK_MONOTONIC, &combo_start);
            }
        } else {
            combo_active = 0;
        }
    }

    close(fd);
    return 0;
}
