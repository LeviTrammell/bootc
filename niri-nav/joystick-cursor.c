/*
 * joystick-cursor: Wayland virtual pointer daemon for analog joysticks
 *
 * Left joystick  → cursor motion (via zwlr_virtual_pointer_v1.motion)
 * Right joystick → scrolling      (via zwlr_virtual_pointer_v1.axis)
 *
 * Accepts commands on stdin from niri-nav:
 *   CLICK left\n   - left mouse button press+release
 *   CLICK right\n  - right mouse button press+release
 *   PAUSE\n        - stop cursor movement + scrolling (GamePassthrough)
 *   RESUME\n       - resume
 *   EOF            - exit cleanly
 *
 * Build:
 *   wayland-scanner client-header \
 *     /usr/share/wlr-protocols/unstable/wlr-virtual-pointer-unstable-v1.xml \
 *     vptr-client.h
 *   wayland-scanner private-code \
 *     /usr/share/wlr-protocols/unstable/wlr-virtual-pointer-unstable-v1.xml \
 *     vptr-code.c
 *   gcc -o joystick-cursor joystick-cursor.c vptr-code.c \
 *       $(pkg-config --cflags --libs wayland-client) -lm
 */

#define _GNU_SOURCE

#include <errno.h>
#include <fcntl.h>
#include <linux/input.h>
#include <math.h>
#include <poll.h>
#include <signal.h>
#include <stdbool.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/timerfd.h>
#include <unistd.h>
#include <wayland-client.h>

#include "vptr-client.h"

/* Dead zone: 15% of half-range */
#define DEAD_ZONE 0.15

/* Max cursor speed in pixels per frame at 60 Hz */
#define MAX_SPEED_X 15.0
#define MAX_SPEED_Y 22.0

/* Max scroll speed in wl_fixed units per frame */
#define MAX_SCROLL_X 2.0
#define MAX_SCROLL_Y 3.0

/* Cursor update rate */
#define FPS 60

/* Linux button codes for virtual pointer clicks */
#define BTN_LEFT_CODE   0x110
#define BTN_RIGHT_CODE  0x111

/* --- Per-stick state --- */
struct stick {
    int fd;
    int axis_code_x;    /* ABS_X or ABS_RX */
    int axis_code_y;    /* ABS_Y or ABS_RY */
    int center_x, center_y;
    int cur_x, cur_y;
    int range_min_x, range_max_x;
    int range_min_y, range_max_y;
    double accum_x, accum_y;
};

/* --- Wayland globals --- */
static struct wl_display *display;
static struct wl_seat *seat;
static struct zwlr_virtual_pointer_manager_v1 *vptr_manager;
static struct zwlr_virtual_pointer_v1 *vptr;

static struct stick left_stick = { .fd = -1 };
static struct stick right_stick = { .fd = -1 };

static bool paused = false;
static volatile sig_atomic_t running = 1;

/* --- Wayland registry --- */

static void registry_global(void *data, struct wl_registry *registry,
                            uint32_t name, const char *interface,
                            uint32_t version) {
    (void)data;
    if (strcmp(interface, wl_seat_interface.name) == 0) {
        seat = wl_registry_bind(registry, name, &wl_seat_interface, 1);
    } else if (strcmp(interface,
                      zwlr_virtual_pointer_manager_v1_interface.name) == 0) {
        vptr_manager = wl_registry_bind(
            registry, name, &zwlr_virtual_pointer_manager_v1_interface, 1);
    }
}

static void registry_global_remove(void *data, struct wl_registry *registry,
                                   uint32_t name) {
    (void)data;
    (void)registry;
    (void)name;
}

static const struct wl_registry_listener registry_listener = {
    .global = registry_global,
    .global_remove = registry_global_remove,
};

/* --- Joystick → cursor math --- */

static double normalize_axis(int value, int center, int min, int max) {
    /* Use symmetric half-range so both directions have equal max speed.
     * Don't clamp input — the ADC can exceed its reported min/max. */
    double half_lo = (double)(center - min);
    double half_hi = (double)(max - center);
    double half_range = (half_lo > half_hi) ? half_lo : half_hi;
    if (half_range < 1.0) half_range = 1.0;

    double norm = (double)(value - center) / half_range;
    if (norm < -1.0) norm = -1.0;
    if (norm >  1.0) norm =  1.0;
    return norm;
}

static double apply_curve(double norm, double max_speed) {
    double abs_val = fabs(norm);
    if (abs_val < DEAD_ZONE)
        return 0.0;

    /* Remap so just past dead zone starts at 0 */
    double remapped = (abs_val - DEAD_ZONE) / (1.0 - DEAD_ZONE);
    /* Quadratic acceleration curve */
    double speed = remapped * remapped * max_speed;

    return (norm < 0.0) ? -speed : speed;
}

/* --- Find joystick device by name pattern --- */

static int open_joystick_by_name(const char *match, int *out_axis_x, int *out_axis_y) {
    char path[64];

    for (int i = 0; i < 32; i++) {
        snprintf(path, sizeof(path), "/dev/input/event%d", i);
        int fd = open(path, O_RDONLY | O_NONBLOCK);
        if (fd < 0)
            continue;

        char name[256] = "";
        ioctl(fd, EVIOCGNAME(sizeof(name)), name);

        if (strstr(name, match)) {
            /* Find which ABS axes this device has — try X/Y first, then RX/RY */
            unsigned long abs_bits[(ABS_MAX + 8 * sizeof(unsigned long) - 1) /
                                   (8 * sizeof(unsigned long))] = {0};
            ioctl(fd, EVIOCGBIT(EV_ABS, sizeof(abs_bits)), abs_bits);

            /* Check candidate axis pairs */
            int pairs[][2] = { {ABS_X, ABS_Y}, {ABS_RX, ABS_RY}, {ABS_Z, ABS_RZ} };
            for (int p = 0; p < 3; p++) {
                int ax = pairs[p][0], ay = pairs[p][1];
                int has_x = abs_bits[ax / (8 * sizeof(unsigned long))] &
                            (1UL << (ax % (8 * sizeof(unsigned long))));
                int has_y = abs_bits[ay / (8 * sizeof(unsigned long))] &
                            (1UL << (ay % (8 * sizeof(unsigned long))));
                if (has_x && has_y) {
                    *out_axis_x = ax;
                    *out_axis_y = ay;
                    fprintf(stderr, "joystick-cursor: opened %s (%s) axes=%d,%d\n",
                            path, name, ax, ay);
                    return fd;
                }
            }
        }
        close(fd);
    }

    return -1;
}

/* --- Read resting position and range for center calibration --- */

static void calibrate_stick(struct stick *s, const char *label) {
    struct input_absinfo abs_info;

    if (ioctl(s->fd, EVIOCGABS(s->axis_code_x), &abs_info) == 0) {
        s->center_x = abs_info.value;
        s->cur_x = abs_info.value;
        /* Use absinfo range span centered on actual resting position.
         * Some ADC axes report wrong min/max signs but correct span. */
        int span_x = abs_info.maximum - abs_info.minimum;
        s->range_min_x = s->center_x - span_x / 2;
        s->range_max_x = s->center_x + span_x / 2;
        fprintf(stderr, "joystick-cursor: %s X center=%d (range %d-%d)\n",
                label, s->center_x, s->range_min_x, s->range_max_x);
    }
    if (ioctl(s->fd, EVIOCGABS(s->axis_code_y), &abs_info) == 0) {
        s->center_y = abs_info.value;
        s->cur_y = abs_info.value;
        int span_y = abs_info.maximum - abs_info.minimum;
        s->range_min_y = s->center_y - span_y / 2;
        s->range_max_y = s->center_y + span_y / 2;
        fprintf(stderr, "joystick-cursor: %s Y center=%d (range %d-%d)\n",
                label, s->center_y, s->range_min_y, s->range_max_y);
    }
}

/* --- Read evdev events for a stick --- */

static void read_stick_events(struct stick *s) {
    struct input_event ev;
    while (read(s->fd, &ev, sizeof(ev)) == sizeof(ev)) {
        if (ev.type == EV_ABS) {
            if (ev.code == s->axis_code_x)
                s->cur_x = ev.value;
            else if (ev.code == s->axis_code_y)
                s->cur_y = ev.value;
        }
    }
}

/* --- Send virtual pointer click --- */

static void send_click(uint32_t button_code) {
    if (!vptr)
        return;

    struct timespec ts;
    clock_gettime(CLOCK_MONOTONIC, &ts);
    uint32_t ms = (uint32_t)(ts.tv_sec * 1000 + ts.tv_nsec / 1000000);

    /* Press */
    zwlr_virtual_pointer_v1_button(vptr, ms, button_code, WL_POINTER_BUTTON_STATE_PRESSED);
    zwlr_virtual_pointer_v1_frame(vptr);
    wl_display_flush(display);

    /* Brief delay for apps to register the press */
    usleep(20000);

    /* Release */
    clock_gettime(CLOCK_MONOTONIC, &ts);
    ms = (uint32_t)(ts.tv_sec * 1000 + ts.tv_nsec / 1000000);
    zwlr_virtual_pointer_v1_button(vptr, ms, button_code, WL_POINTER_BUTTON_STATE_RELEASED);
    zwlr_virtual_pointer_v1_frame(vptr);
    wl_display_flush(display);
}

/* --- Process stdin commands --- */

static void process_stdin(void) {
    static char buf[256];
    static int buf_pos = 0;

    while (1) {
        ssize_t n = read(STDIN_FILENO, buf + buf_pos, sizeof(buf) - buf_pos - 1);
        if (n <= 0)
            break;
        buf_pos += n;
        buf[buf_pos] = '\0';

        /* Process complete lines */
        char *line_start = buf;
        char *newline;
        while ((newline = strchr(line_start, '\n')) != NULL) {
            *newline = '\0';

            if (strcmp(line_start, "CLICK left") == 0) {
                send_click(BTN_LEFT_CODE);
            } else if (strcmp(line_start, "CLICK right") == 0) {
                send_click(BTN_RIGHT_CODE);
            } else if (strcmp(line_start, "PAUSE") == 0) {
                paused = true;
                left_stick.accum_x = 0.0;
                left_stick.accum_y = 0.0;
                right_stick.accum_x = 0.0;
                right_stick.accum_y = 0.0;
                fprintf(stderr, "joystick-cursor: paused\n");
            } else if (strcmp(line_start, "RESUME") == 0) {
                paused = false;
                fprintf(stderr, "joystick-cursor: resumed\n");
            }

            line_start = newline + 1;
        }

        /* Move any remaining partial line to start of buffer */
        int remaining = buf_pos - (line_start - buf);
        if (remaining > 0)
            memmove(buf, line_start, remaining);
        buf_pos = remaining;
    }
}

/* --- Signal handler --- */

static void handle_signal(int sig) {
    (void)sig;
    running = 0;
}

/* --- Main --- */

int main(void) {
    fprintf(stderr, "joystick-cursor: starting\n");

    signal(SIGINT, handle_signal);
    signal(SIGTERM, handle_signal);

    /* Connect to Wayland. At boot the user service can win the race
     * against niri creating the socket, so retry instead of dying —
     * niri-nav only spawns us once. */
    for (int attempt = 0; ; attempt++) {
        display = wl_display_connect(NULL);
        if (display)
            break;
        if (attempt == 0)
            fprintf(stderr, "joystick-cursor: waiting for Wayland display...\n");
        if (attempt >= 240) { /* ~120s */
            fprintf(stderr, "joystick-cursor: gave up waiting for Wayland display\n");
            return 1;
        }
        usleep(500000);
    }

    struct wl_registry *registry = wl_display_get_registry(display);
    wl_registry_add_listener(registry, &registry_listener, NULL);
    wl_display_roundtrip(display);

    if (!seat) {
        fprintf(stderr, "joystick-cursor: no wl_seat found\n");
        return 1;
    }
    if (!vptr_manager) {
        fprintf(stderr, "joystick-cursor: no zwlr_virtual_pointer_manager_v1 found\n");
        return 1;
    }

    /* Create virtual pointer */
    vptr = zwlr_virtual_pointer_manager_v1_create_virtual_pointer(
        vptr_manager, seat);
    if (!vptr) {
        fprintf(stderr, "joystick-cursor: failed to create virtual pointer\n");
        return 1;
    }
    wl_display_flush(display);
    fprintf(stderr, "joystick-cursor: virtual pointer created\n");

    /* Open left joystick (cursor) */
    left_stick.fd = open_joystick_by_name("left", &left_stick.axis_code_x, &left_stick.axis_code_y);
    if (left_stick.fd < 0) {
        fprintf(stderr, "joystick-cursor: failed to open left joystick\n");
        return 1;
    }
    calibrate_stick(&left_stick, "left");

    /* Open right joystick (scroll) — optional */
    right_stick.fd = open_joystick_by_name("right", &right_stick.axis_code_x, &right_stick.axis_code_y);
    if (right_stick.fd >= 0) {
        calibrate_stick(&right_stick, "right");
    } else {
        fprintf(stderr, "joystick-cursor: right joystick not found (scrolling disabled)\n");
    }

    /* Create 60 Hz timer */
    int timer_fd = timerfd_create(CLOCK_MONOTONIC, TFD_NONBLOCK);
    if (timer_fd < 0) {
        perror("timerfd_create");
        return 1;
    }
    struct itimerspec its = {
        .it_interval = { .tv_sec = 0, .tv_nsec = 1000000000 / FPS },
        .it_value    = { .tv_sec = 0, .tv_nsec = 1000000000 / FPS },
    };
    timerfd_settime(timer_fd, 0, &its, NULL);

    /* Make stdin non-blocking */
    int flags = fcntl(STDIN_FILENO, F_GETFL, 0);
    fcntl(STDIN_FILENO, F_SETFL, flags | O_NONBLOCK);

    /* Get Wayland display fd for polling */
    int wl_fd = wl_display_get_fd(display);

    fprintf(stderr, "joystick-cursor: entering main loop\n");

    /* Main poll loop */
    int nfds = (right_stick.fd >= 0) ? 5 : 4;
    struct pollfd fds[5] = {
        { .fd = left_stick.fd,  .events = POLLIN },  /* 0: left stick */
        { .fd = timer_fd,       .events = POLLIN },  /* 1: 60Hz timer */
        { .fd = STDIN_FILENO,   .events = POLLIN },  /* 2: stdin cmds */
        { .fd = wl_fd,          .events = POLLIN },  /* 3: wayland */
        { .fd = right_stick.fd, .events = POLLIN },  /* 4: right stick */
    };

    while (running) {
        /* Flush pending Wayland requests before polling */
        while (wl_display_prepare_read(display) != 0)
            wl_display_dispatch_pending(display);
        wl_display_flush(display);

        int ret = poll(fds, nfds, 100);

        if (ret < 0) {
            wl_display_cancel_read(display);
            if (errno == EINTR)
                continue;
            break;
        }

        /* Compositor went away (niri restart): exit so niri-nav's
         * respawn logic can bring us back against the new socket. */
        if (fds[3].revents & (POLLHUP | POLLERR)) {
            wl_display_cancel_read(display);
            fprintf(stderr, "joystick-cursor: Wayland connection lost, exiting\n");
            break;
        }

        /* Handle Wayland events */
        if (fds[3].revents & POLLIN) {
            if (wl_display_read_events(display) < 0) {
                fprintf(stderr, "joystick-cursor: Wayland read failed, exiting\n");
                break;
            }
            wl_display_dispatch_pending(display);
        } else {
            wl_display_cancel_read(display);
        }

        /* Read left joystick events */
        if (fds[0].revents & POLLIN)
            read_stick_events(&left_stick);

        /* Read right joystick events */
        if (nfds > 4 && (fds[4].revents & POLLIN))
            read_stick_events(&right_stick);

        /* Timer tick: send cursor motion + scroll */
        if (fds[1].revents & POLLIN) {
            uint64_t expirations;
            read(timer_fd, &expirations, sizeof(expirations));

            if (!paused && vptr) {
                struct timespec ts;
                clock_gettime(CLOCK_MONOTONIC, &ts);
                uint32_t ms = (uint32_t)(ts.tv_sec * 1000 + ts.tv_nsec / 1000000);
                bool sent = false;

                /* --- Left stick: cursor motion --- */
                struct stick *ls = &left_stick;
                double dx = apply_curve(
                    normalize_axis(ls->cur_x, ls->center_x, ls->range_min_x, ls->range_max_x),
                    MAX_SPEED_X);
                double dy = -apply_curve(
                    normalize_axis(ls->cur_y, ls->center_y, ls->range_min_y, ls->range_max_y),
                    MAX_SPEED_Y);

                ls->accum_x += dx;
                ls->accum_y += dy;

                int move_x = (int)ls->accum_x;
                int move_y = (int)ls->accum_y;

                if (move_x != 0 || move_y != 0) {
                    ls->accum_x -= move_x;
                    ls->accum_y -= move_y;
                    zwlr_virtual_pointer_v1_motion(vptr, ms,
                        wl_fixed_from_int(move_x),
                        wl_fixed_from_int(move_y));
                    sent = true;
                }

                /* --- Right stick: scrolling --- */
                if (right_stick.fd >= 0) {
                    struct stick *rs = &right_stick;
                    double sx = apply_curve(
                        normalize_axis(rs->cur_x, rs->center_x, rs->range_min_x, rs->range_max_x),
                        MAX_SCROLL_X);
                    double sy = apply_curve(
                        normalize_axis(rs->cur_y, rs->center_y, rs->range_min_y, rs->range_max_y),
                        MAX_SCROLL_Y);

                    rs->accum_x += sx;
                    rs->accum_y += sy;

                    int scroll_x = (int)rs->accum_x;
                    int scroll_y = (int)rs->accum_y;

                    if (scroll_x != 0 || scroll_y != 0) {
                        rs->accum_x -= scroll_x;
                        rs->accum_y -= scroll_y;

                        zwlr_virtual_pointer_v1_axis_source(vptr,
                            WL_POINTER_AXIS_SOURCE_CONTINUOUS);
                        if (scroll_y != 0)
                            zwlr_virtual_pointer_v1_axis(vptr, ms,
                                WL_POINTER_AXIS_VERTICAL_SCROLL,
                                wl_fixed_from_int(scroll_y));
                        if (scroll_x != 0)
                            zwlr_virtual_pointer_v1_axis(vptr, ms,
                                WL_POINTER_AXIS_HORIZONTAL_SCROLL,
                                wl_fixed_from_int(scroll_x));
                        sent = true;
                    }
                }

                if (sent) {
                    zwlr_virtual_pointer_v1_frame(vptr);
                    wl_display_flush(display);
                }
            }
        }

        /* Process stdin commands */
        if (fds[2].revents & POLLIN) {
            process_stdin();
        }

        /* Stdin EOF (parent died) */
        if (fds[2].revents & POLLHUP) {
            fprintf(stderr, "joystick-cursor: stdin EOF, exiting\n");
            break;
        }
    }

    /* Cleanup */
    fprintf(stderr, "joystick-cursor: cleaning up\n");
    if (vptr)
        zwlr_virtual_pointer_v1_destroy(vptr);
    if (vptr_manager)
        zwlr_virtual_pointer_manager_v1_destroy(vptr_manager);
    if (left_stick.fd >= 0)
        close(left_stick.fd);
    if (right_stick.fd >= 0)
        close(right_stick.fd);
    close(timer_fd);
    wl_display_disconnect(display);

    return 0;
}
