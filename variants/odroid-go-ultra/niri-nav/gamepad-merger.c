/*
 * gamepad-merger: Merge three evdev devices into one virtual uinput gamepad
 *
 * The ODROID Go Ultra has three separate input devices:
 *   - adc-joystick-left  (ABS_X, ABS_Y)
 *   - adc-joystick-right (ABS_RX, ABS_RY)
 *   - gpio-keys          (BTN_SOUTH, BTN_EAST, etc.)
 *
 * SDL2's GameController API needs a single unified device.
 * This daemon merges all three into one virtual gamepad via uinput.
 *
 * Axis processing:
 *   - Calibrates center at startup (reads resting position)
 *   - Uses absinfo span centered on actual resting value (fixes broken absinfo)
 *   - Normalizes to standard -32768..32767 range
 *   - 15% dead zone to eliminate drift
 *   - Y axes inverted (stick up = negative, matching SDL2/Xbox convention)
 *
 * Accepts commands on stdin from niri-nav:
 *   ACTIVATE\n   - start forwarding events (GamePassthrough entered)
 *   DEACTIVATE\n - stop forwarding events (GamePassthrough exited)
 *   EOF          - exit cleanly
 *
 * EVIOCGRAB: gpio-keys is grabbed only when ACTIVATED, preventing
 * emulators (ES-DE, RetroArch) from seeing raw unmapped events.
 * When not activated (niri-nav mode), gpio-keys is NOT grabbed so
 * niri-nav can read it directly.
 *
 * Build:
 *   gcc -o gamepad-merger gamepad-merger.c -O2 -lm
 */

#define _GNU_SOURCE

#include <errno.h>
#include <fcntl.h>
#include <linux/input.h>
#include <linux/uinput.h>
#include <math.h>
#include <poll.h>
#include <signal.h>
#include <stdbool.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>

/* Custom vendor/product for SDL2 GUID matching */
#define VIRT_VENDOR  0x4F47   /* "OG" */
#define VIRT_PRODUCT 0x5530   /* "U0" */
#define VIRT_VERSION 0x0001

/* Dead zone: 15% of half-range */
#define DEAD_ZONE 0.15

/* Standard gamepad axis output range */
#define OUT_MIN -32768
#define OUT_MAX  32767

static volatile sig_atomic_t running = 1;
static bool active = false;

static int ljoy_fd = -1;   /* left joystick */
static int rjoy_fd = -1;   /* right joystick */
static int keys_fd = -1;   /* gpio-keys */
static int uinput_fd = -1; /* virtual gamepad */

/* Axis codes for each stick */
static int ljoy_axis_x, ljoy_axis_y;
static int rjoy_axis_x, rjoy_axis_y;

/* Per-stick calibration data */
struct stick_cal {
    int center_x, center_y;
    int range_min_x, range_max_x;
    int range_min_y, range_max_y;
};

static struct stick_cal ljoy_cal, rjoy_cal;

static void sig_handler(int sig) {
    (void)sig;
    running = 0;
}

/* --- Find input device by name pattern --- */

static int open_input_by_name(const char *match, int *out_axis_x, int *out_axis_y) {
    char path[64];

    for (int i = 0; i < 32; i++) {
        snprintf(path, sizeof(path), "/dev/input/event%d", i);
        int fd = open(path, O_RDONLY | O_NONBLOCK);
        if (fd < 0)
            continue;

        char name[256] = "";
        ioctl(fd, EVIOCGNAME(sizeof(name)), name);

        if (strstr(name, match)) {
            if (out_axis_x && out_axis_y) {
                /* Detect axis pair */
                unsigned long abs_bits[(ABS_MAX + 8 * sizeof(unsigned long) - 1) /
                                       (8 * sizeof(unsigned long))] = {0};
                ioctl(fd, EVIOCGBIT(EV_ABS, sizeof(abs_bits)), abs_bits);

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
                        fprintf(stderr, "gamepad-merger: opened %s (%s) axes=%d,%d\n",
                                path, name, ax, ay);
                        return fd;
                    }
                }
            } else {
                fprintf(stderr, "gamepad-merger: opened %s (%s)\n", path, name);
                return fd;
            }
        }
        close(fd);
    }

    return -1;
}

/* --- Calibrate stick at startup --- */

static void calibrate_stick(int fd, int axis_x, int axis_y,
                             struct stick_cal *cal, const char *label) {
    struct input_absinfo info;

    if (ioctl(fd, EVIOCGABS(axis_x), &info) == 0) {
        cal->center_x = info.value;
        int span = info.maximum - info.minimum;
        cal->range_min_x = cal->center_x - span / 2;
        cal->range_max_x = cal->center_x + span / 2;
        fprintf(stderr, "gamepad-merger: %s X center=%d (range %d-%d)\n",
                label, cal->center_x, cal->range_min_x, cal->range_max_x);
    }

    if (ioctl(fd, EVIOCGABS(axis_y), &info) == 0) {
        cal->center_y = info.value;
        int span = info.maximum - info.minimum;
        cal->range_min_y = cal->center_y - span / 2;
        cal->range_max_y = cal->center_y + span / 2;
        fprintf(stderr, "gamepad-merger: %s Y center=%d (range %d-%d)\n",
                label, cal->center_y, cal->range_min_y, cal->range_max_y);
    }
}

/* --- Axis normalization and dead zone --- */

static double normalize_axis(int value, int center, int range_min, int range_max) {
    double half_lo = (double)(center - range_min);
    double half_hi = (double)(range_max - center);
    double half_range = (half_lo > half_hi) ? half_lo : half_hi;
    if (half_range < 1.0) half_range = 1.0;

    double norm = (double)(value - center) / half_range;
    if (norm < -1.0) norm = -1.0;
    if (norm >  1.0) norm =  1.0;
    return norm;
}

static int apply_deadzone_and_scale(double norm, bool invert) {
    if (invert) norm = -norm;

    double abs_val = fabs(norm);
    if (abs_val < DEAD_ZONE)
        return 0;

    double remapped = (abs_val - DEAD_ZONE) / (1.0 - DEAD_ZONE);

    int result = (int)(remapped * OUT_MAX);
    return (norm < 0.0) ? -result : result;
}

/* --- Create virtual gamepad via uinput --- */

static int create_virtual_gamepad(void) {
    int fd = open("/dev/uinput", O_WRONLY | O_NONBLOCK);
    if (fd < 0) {
        perror("gamepad-merger: open /dev/uinput");
        return -1;
    }

    /* Enable event types */
    ioctl(fd, UI_SET_EVBIT, EV_KEY);
    ioctl(fd, UI_SET_EVBIT, EV_ABS);
    ioctl(fd, UI_SET_EVBIT, EV_SYN);

    /* Face buttons */
    ioctl(fd, UI_SET_KEYBIT, BTN_SOUTH);
    ioctl(fd, UI_SET_KEYBIT, BTN_EAST);
    ioctl(fd, UI_SET_KEYBIT, BTN_NORTH);
    ioctl(fd, UI_SET_KEYBIT, BTN_WEST);

    /* Shoulder buttons */
    ioctl(fd, UI_SET_KEYBIT, BTN_TL);
    ioctl(fd, UI_SET_KEYBIT, BTN_TR);
    ioctl(fd, UI_SET_KEYBIT, BTN_TL2);
    ioctl(fd, UI_SET_KEYBIT, BTN_TR2);

    /* Menu buttons */
    ioctl(fd, UI_SET_KEYBIT, BTN_START);
    ioctl(fd, UI_SET_KEYBIT, BTN_SELECT);

    /* D-pad as buttons */
    ioctl(fd, UI_SET_KEYBIT, BTN_DPAD_UP);
    ioctl(fd, UI_SET_KEYBIT, BTN_DPAD_DOWN);
    ioctl(fd, UI_SET_KEYBIT, BTN_DPAD_LEFT);
    ioctl(fd, UI_SET_KEYBIT, BTN_DPAD_RIGHT);

    /* Axes */
    ioctl(fd, UI_SET_ABSBIT, ABS_X);
    ioctl(fd, UI_SET_ABSBIT, ABS_Y);
    ioctl(fd, UI_SET_ABSBIT, ABS_RX);
    ioctl(fd, UI_SET_ABSBIT, ABS_RY);

    struct input_absinfo std_abs = {
        .value = 0,
        .minimum = OUT_MIN,
        .maximum = OUT_MAX,
        .fuzz = 0,
        .flat = 0,
        .resolution = 0,
    };

    struct uinput_setup setup = {0};
    snprintf(setup.name, UINPUT_MAX_NAME_SIZE, "OGU Virtual Gamepad");
    setup.id.bustype = BUS_VIRTUAL;
    setup.id.vendor = VIRT_VENDOR;
    setup.id.product = VIRT_PRODUCT;
    setup.id.version = VIRT_VERSION;

    struct uinput_abs_setup abs_setup;
    int axes[] = { ABS_X, ABS_Y, ABS_RX, ABS_RY };

    for (int i = 0; i < 4; i++) {
        memset(&abs_setup, 0, sizeof(abs_setup));
        abs_setup.code = axes[i];
        abs_setup.absinfo = std_abs;
        ioctl(fd, UI_ABS_SETUP, &abs_setup);
    }

    ioctl(fd, UI_DEV_SETUP, &setup);

    if (ioctl(fd, UI_DEV_CREATE) < 0) {
        perror("gamepad-merger: UI_DEV_CREATE");
        close(fd);
        return -1;
    }

    fprintf(stderr, "gamepad-merger: virtual gamepad created (axes: %d to %d)\n",
            OUT_MIN, OUT_MAX);
    return fd;
}

/* --- Emit event to virtual gamepad --- */

static void emit(int fd, int type, int code, int value) {
    struct input_event ev = {0};
    ev.type = type;
    ev.code = code;
    ev.value = value;
    write(fd, &ev, sizeof(ev));
}

static void emit_syn(int fd) {
    emit(fd, EV_SYN, SYN_REPORT, 0);
}

/* --- Forward joystick axis events with calibration + dead zone --- */

static void forward_stick_event(struct input_event *ev, int src_axis_x,
                                 int src_axis_y, int dst_axis_x,
                                 int dst_axis_y, struct stick_cal *cal) {
    if (ev->type != EV_ABS)
        return;

    if (ev->code == src_axis_x) {
        double norm = normalize_axis(ev->value, cal->center_x,
                                      cal->range_min_x, cal->range_max_x);
        int out = apply_deadzone_and_scale(norm, false);
        emit(uinput_fd, EV_ABS, dst_axis_x, out);
        emit_syn(uinput_fd);
    } else if (ev->code == src_axis_y) {
        double norm = normalize_axis(ev->value, cal->center_y,
                                      cal->range_min_y, cal->range_max_y);
        int out = apply_deadzone_and_scale(norm, true);  /* invert Y */
        emit(uinput_fd, EV_ABS, dst_axis_y, out);
        emit_syn(uinput_fd);
    }
}

/* --- Forward button events --- */

/*
 * OGU button remapping for SDL2 GameController compatibility.
 *
 * OGU physical -> evdev code -> we emit -> SDL2 sees
 * A (right)    -> BTN_EAST   -> BTN_SOUTH -> "A" (confirm)
 * B (bottom)   -> BTN_SOUTH  -> BTN_EAST  -> "B" (back)
 * X (top)      -> BTN_NORTH  -> BTN_WEST  -> "X"
 * Y (left)     -> BTN_WEST   -> BTN_NORTH -> "Y"
 * F6/Start     -> TRIGGER_HAPPY6 -> BTN_START
 * F1/Select    -> TRIGGER_HAPPY1 -> BTN_SELECT
 */

#ifndef BTN_TRIGGER_HAPPY1
#define BTN_TRIGGER_HAPPY1 0x2c0
#endif
#ifndef BTN_TRIGGER_HAPPY2
#define BTN_TRIGGER_HAPPY2 0x2c1
#endif
#ifndef BTN_TRIGGER_HAPPY6
#define BTN_TRIGGER_HAPPY6 0x2c5
#endif

struct button_map {
    int from;
    int to;
};

static const struct button_map button_remap[] = {
    /* Face buttons: swap to match SDL2 Xbox-style expectations */
    { BTN_EAST,    BTN_SOUTH },  /* OGU A -> SDL2 A */
    { BTN_SOUTH,   BTN_EAST  },  /* OGU B -> SDL2 B */
    { BTN_NORTH,   BTN_WEST  },  /* OGU X -> SDL2 X */
    { BTN_WEST,    BTN_NORTH },  /* OGU Y -> SDL2 Y */
    /* Shoulders: pass through */
    { BTN_TL,      BTN_TL      },
    { BTN_TR,      BTN_TR      },
    { BTN_TL2,     BTN_TL2     },
    { BTN_TR2,     BTN_TR2     },
    /* Menu: remap F buttons to standard Start/Select */
    { BTN_TRIGGER_HAPPY6, BTN_START  },   /* F6 -> Start */
    { BTN_TRIGGER_HAPPY1, BTN_SELECT },   /* F1 -> Select */
    /* D-pad: pass through */
    { BTN_DPAD_UP,    BTN_DPAD_UP    },
    { BTN_DPAD_DOWN,  BTN_DPAD_DOWN  },
    { BTN_DPAD_LEFT,  BTN_DPAD_LEFT  },
    { BTN_DPAD_RIGHT, BTN_DPAD_RIGHT },
};
#define NUM_REMAP (sizeof(button_remap) / sizeof(button_remap[0]))

static void forward_key_event(struct input_event *ev) {
    if (ev->type != EV_KEY)
        return;

    for (int i = 0; i < (int)NUM_REMAP; i++) {
        if (button_remap[i].from == ev->code) {
            emit(uinput_fd, EV_KEY, button_remap[i].to, ev->value);
            emit_syn(uinput_fd);
            return;
        }
    }
}

/* --- Stdin command processing --- */

static char stdin_buf[256];
static int stdin_len = 0;

static void process_stdin(void) {
    int n = read(STDIN_FILENO, stdin_buf + stdin_len,
                 sizeof(stdin_buf) - stdin_len - 1);
    if (n <= 0) {
        running = 0;
        return;
    }
    stdin_len += n;
    stdin_buf[stdin_len] = '\0';

    char *line;
    while ((line = strchr(stdin_buf, '\n')) != NULL) {
        *line = '\0';

        if (strcmp(stdin_buf, "ACTIVATE") == 0) {
            if (!active) {
                active = true;
                /* Grab gpio-keys so emulators only see virtual gamepad */
                if (ioctl(keys_fd, EVIOCGRAB, 1) < 0)
                    perror("gamepad-merger: EVIOCGRAB on ACTIVATE");
                else
                    fprintf(stderr, "gamepad-merger: grabbed gpio-keys\n");
                fprintf(stderr, "gamepad-merger: ACTIVATED\n");
            }
        } else if (strcmp(stdin_buf, "DEACTIVATE") == 0) {
            if (active) {
                active = false;
                /* Release gpio-keys grab */
                if (ioctl(keys_fd, EVIOCGRAB, 0) < 0)
                    perror("gamepad-merger: EVIOCGRAB release");
                else
                    fprintf(stderr, "gamepad-merger: released gpio-keys grab\n");
                fprintf(stderr, "gamepad-merger: DEACTIVATED\n");
            }
        }

        int remaining = stdin_len - (int)(line - stdin_buf) - 1;
        memmove(stdin_buf, line + 1, remaining);
        stdin_len = remaining;
    }
}

/* --- Main loop --- */

int main(void) {
    signal(SIGINT, sig_handler);
    signal(SIGTERM, sig_handler);

    fprintf(stderr, "gamepad-merger: starting\n");

    /* Open the three input devices */
    ljoy_fd = open_input_by_name("left", &ljoy_axis_x, &ljoy_axis_y);
    if (ljoy_fd < 0) {
        fprintf(stderr, "gamepad-merger: ERROR: left joystick not found\n");
        return 1;
    }

    rjoy_fd = open_input_by_name("right", &rjoy_axis_x, &rjoy_axis_y);
    if (rjoy_fd < 0) {
        fprintf(stderr, "gamepad-merger: ERROR: right joystick not found\n");
        return 1;
    }

    keys_fd = open_input_by_name("gpio-keys", NULL, NULL);
    if (keys_fd < 0) {
        fprintf(stderr, "gamepad-merger: ERROR: gpio-keys not found\n");
        return 1;
    }

    /* NOTE: gpio-keys is NOT grabbed here. The grab happens on ACTIVATE
     * so that niri-nav (which also reads gpio-keys) is not blocked. */

    /* Calibrate sticks */
    calibrate_stick(ljoy_fd, ljoy_axis_x, ljoy_axis_y, &ljoy_cal, "left");
    calibrate_stick(rjoy_fd, rjoy_axis_x, rjoy_axis_y, &rjoy_cal, "right");

    /* Create virtual gamepad */
    uinput_fd = create_virtual_gamepad();
    if (uinput_fd < 0) {
        fprintf(stderr, "gamepad-merger: ERROR: failed to create virtual gamepad\n");
        return 1;
    }

    /* Make stdin non-blocking */
    int flags = fcntl(STDIN_FILENO, F_GETFL);
    fcntl(STDIN_FILENO, F_SETFL, flags | O_NONBLOCK);

    fprintf(stderr, "gamepad-merger: ready (waiting for ACTIVATE)\n");

    /* Poll all four sources: left stick, right stick, buttons, stdin */
    struct pollfd fds[4];
    fds[0].fd = ljoy_fd;
    fds[0].events = POLLIN;
    fds[1].fd = rjoy_fd;
    fds[1].events = POLLIN;
    fds[2].fd = keys_fd;
    fds[2].events = POLLIN;
    fds[3].fd = STDIN_FILENO;
    fds[3].events = POLLIN;

    while (running) {
        int ret = poll(fds, 4, 1000);
        if (ret < 0) {
            if (errno == EINTR) continue;
            break;
        }

        /* Process stdin commands */
        if (fds[3].revents & (POLLIN | POLLHUP)) {
            process_stdin();
        }

        if (!active) {
            /* Drain events when inactive so we don't build up a backlog */
            struct input_event ev;
            if (fds[0].revents & POLLIN)
                while (read(ljoy_fd, &ev, sizeof(ev)) > 0) {}
            if (fds[1].revents & POLLIN)
                while (read(rjoy_fd, &ev, sizeof(ev)) > 0) {}
            if (fds[2].revents & POLLIN)
                while (read(keys_fd, &ev, sizeof(ev)) > 0) {}
            continue;
        }

        /* Forward left stick events */
        if (fds[0].revents & POLLIN) {
            struct input_event ev;
            while (read(ljoy_fd, &ev, sizeof(ev)) == sizeof(ev)) {
                forward_stick_event(&ev, ljoy_axis_x, ljoy_axis_y,
                                    ABS_X, ABS_Y, &ljoy_cal);
            }
        }

        /* Forward right stick events */
        if (fds[1].revents & POLLIN) {
            struct input_event ev;
            while (read(rjoy_fd, &ev, sizeof(ev)) == sizeof(ev)) {
                forward_stick_event(&ev, rjoy_axis_x, rjoy_axis_y,
                                    ABS_RX, ABS_RY, &rjoy_cal);
            }
        }

        /* Forward button events */
        if (fds[2].revents & POLLIN) {
            struct input_event ev;
            while (read(keys_fd, &ev, sizeof(ev)) == sizeof(ev)) {
                forward_key_event(&ev);
            }
        }
    }

    /* Cleanup */
    fprintf(stderr, "gamepad-merger: shutting down\n");
    if (keys_fd >= 0 && active) {
        ioctl(keys_fd, EVIOCGRAB, 0);  /* release grab */
    }
    if (uinput_fd >= 0) {
        ioctl(uinput_fd, UI_DEV_DESTROY);
        close(uinput_fd);
    }
    if (ljoy_fd >= 0) close(ljoy_fd);
    if (rjoy_fd >= 0) close(rjoy_fd);
    if (keys_fd >= 0) close(keys_fd);

    return 0;
}
