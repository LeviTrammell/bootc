/*
 * osk-overlay: GTK4 + gtk-layer-shell on-screen keyboard overlay
 *
 * Renders a QWERTY grid as a bottom layer-shell surface with a highlighted
 * key. Runs persistently as a child of niri-nav:
 *
 *   stdin:  "POS row col shift\n"  update highlight position
 *           "SHOW\n" / "HIDE\n"    manual summon / dismiss
 *           EOF                    exit (parent died)
 *   stdout: "IM ACTIVE\n"          a text field gained focus (auto-show)
 *           "IM INACTIVE\n"        the text field lost focus (auto-hide)
 *           "IM UNAVAILABLE\n"     another IM client owns the seat;
 *                                  manual SHOW/HIDE still works
 *
 * The auto show/hide comes from zwp_input_method_v2: the compositor
 * activates us whenever a text-input-v3 client (Zen, any GTK app...)
 * focuses an editable field — the phone/PSP keyboard experience.
 * niri-nav reacts to the stdout lines by entering/leaving TextEntry.
 *
 * Build:
 *   wayland-scanner client-header protocols/input-method-unstable-v2.xml im2-client.h
 *   wayland-scanner private-code  protocols/input-method-unstable-v2.xml im2-code.c
 *   gcc -o osk-overlay osk-overlay.c im2-code.c \
 *     $(pkg-config --cflags --libs gtk4 gtk4-layer-shell-0 wayland-client)
 */

#include <gtk/gtk.h>
#include <gtk4-layer-shell.h>
#include <gdk/wayland/gdkwayland.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#include "im2-client.h"

/* Layouts: 4 rows each. Layer 0 = lowercase, 1 = shifted, 2 = symbols
 * (URL chars + digits). MUST match KeyboardGrid in niri-nav. */
static const char *normal_rows[] = {
    "1234567890",
    "qwertyuiop",
    "asdfghjkl",
    "zxcvbnm.-/",
};
static const char *shifted_rows[] = {
    "!@#$%^&*()",
    "QWERTYUIOP",
    "ASDFGHJKL",
    "ZXCVBNM:_?",
};
static const char *symbol_rows[] = {
    ":/?.-_~=&+",
    "@#$%^*()[]",
    "'\"`;,!<>\\",
    "1234567890",
};

#define NUM_ROWS 4
#define NUM_LAYERS 3
static const int row_lens[] = { 10, 10, 9, 10 };

static const char **layer_rows(int layer) {
    switch (layer) {
    case 1:  return shifted_rows;
    case 2:  return symbol_rows;
    default: return normal_rows;
    }
}

/* State */
static int cur_row = 1, cur_col = 0, cur_layer = 0;
static GtkWidget *labels[NUM_ROWS][10];
static GtkCssProvider *provider;
static GtkWindow *window;

/* Input-method state */
static struct zwp_input_method_manager_v2 *im_manager;
static struct zwp_input_method_v2 *input_method;
static struct wl_seat *im_seat;
static gboolean im_pending_active = FALSE;
static gboolean im_active = FALSE;
static gboolean im_unavailable = FALSE;

static void report(const char *line) {
    fprintf(stdout, "%s\n", line);
    fflush(stdout);
}

static void update_highlight(void) {
    const char **layout = layer_rows(cur_layer);

    for (int r = 0; r < NUM_ROWS; r++) {
        for (int c = 0; c < row_lens[r]; c++) {
            char ch[2] = { layout[r][c], '\0' };
            gtk_label_set_text(GTK_LABEL(labels[r][c]), ch);

            if (r == cur_row && c == cur_col) {
                gtk_widget_add_css_class(labels[r][c], "highlight");
            } else {
                gtk_widget_remove_css_class(labels[r][c], "highlight");
            }
        }
    }
}

/* --- input-method-v2 listener: the auto show/hide sensor --- */

static void im_activate(void *data, struct zwp_input_method_v2 *im) {
    (void)data; (void)im;
    im_pending_active = TRUE;
}

static void im_deactivate(void *data, struct zwp_input_method_v2 *im) {
    (void)data; (void)im;
    im_pending_active = FALSE;
}

static void im_surrounding_text(void *data, struct zwp_input_method_v2 *im,
                                const char *text, uint32_t cursor, uint32_t anchor) {
    (void)data; (void)im; (void)text; (void)cursor; (void)anchor;
}

static void im_text_change_cause(void *data, struct zwp_input_method_v2 *im,
                                 uint32_t cause) {
    (void)data; (void)im; (void)cause;
}

static void im_content_type(void *data, struct zwp_input_method_v2 *im,
                            uint32_t hint, uint32_t purpose) {
    (void)data; (void)im; (void)hint; (void)purpose;
}

static void im_done(void *data, struct zwp_input_method_v2 *im) {
    (void)data; (void)im;
    if (im_pending_active == im_active)
        return;
    im_active = im_pending_active;
    if (im_active) {
        gtk_widget_set_visible(GTK_WIDGET(window), TRUE);
        report("IM ACTIVE");
    } else {
        gtk_widget_set_visible(GTK_WIDGET(window), FALSE);
        report("IM INACTIVE");
    }
}

static void im_unavailable_cb(void *data, struct zwp_input_method_v2 *im) {
    (void)data; (void)im;
    im_unavailable = TRUE;
    report("IM UNAVAILABLE");
}

static const struct zwp_input_method_v2_listener im_listener = {
    .activate = im_activate,
    .deactivate = im_deactivate,
    .surrounding_text = im_surrounding_text,
    .text_change_cause = im_text_change_cause,
    .content_type = im_content_type,
    .done = im_done,
    .unavailable = im_unavailable_cb,
};

/* --- registry: bind the IM manager off GDK's wl_display --- */

static void registry_global(void *data, struct wl_registry *registry,
                            uint32_t name, const char *interface,
                            uint32_t version) {
    (void)data; (void)version;
    if (strcmp(interface, zwp_input_method_manager_v2_interface.name) == 0) {
        im_manager = wl_registry_bind(
            registry, name, &zwp_input_method_manager_v2_interface, 1);
    }
}

static void registry_global_remove(void *data, struct wl_registry *registry,
                                   uint32_t name) {
    (void)data; (void)registry; (void)name;
}

static const struct wl_registry_listener registry_listener = {
    .global = registry_global,
    .global_remove = registry_global_remove,
};

static void setup_input_method(void) {
    GdkDisplay *gdisplay = gdk_display_get_default();
    if (!GDK_IS_WAYLAND_DISPLAY(gdisplay)) {
        report("IM UNAVAILABLE");
        return;
    }
    struct wl_display *display = gdk_wayland_display_get_wl_display(gdisplay);

    GdkSeat *gseat = gdk_display_get_default_seat(gdisplay);
    im_seat = gseat ? gdk_wayland_seat_get_wl_seat(gseat) : NULL;

    struct wl_registry *registry = wl_display_get_registry(display);
    wl_registry_add_listener(registry, &registry_listener, NULL);
    wl_display_roundtrip(display);

    if (!im_manager || !im_seat) {
        report("IM UNAVAILABLE");
        return;
    }

    input_method = zwp_input_method_manager_v2_get_input_method(
        im_manager, im_seat);
    zwp_input_method_v2_add_listener(input_method, &im_listener, NULL);
    wl_display_flush(display);
    /* Events dispatch through GDK's main loop from here on. */
}

/* --- stdin commands from niri-nav --- */

static gboolean on_stdin(GIOChannel *source, GIOCondition cond, gpointer data) {
    if (cond & (G_IO_HUP | G_IO_ERR)) {
        /* Parent died, exit cleanly */
        g_application_quit(G_APPLICATION(data));
        return FALSE;
    }

    gchar *line = NULL;
    gsize len = 0;
    GIOStatus status = g_io_channel_read_line(source, &line, &len, NULL, NULL);

    if (status == G_IO_STATUS_NORMAL && line) {
        int row, col, layer;
        if (sscanf(line, "POS %d %d %d", &row, &col, &layer) == 3) {
            if (row >= 0 && row < NUM_ROWS && col >= 0 && col < row_lens[row] &&
                layer >= 0 && layer < NUM_LAYERS) {
                cur_row = row;
                cur_col = col;
                cur_layer = layer;
                update_highlight();
            }
        } else if (strncmp(line, "SHOW", 4) == 0) {
            gtk_widget_set_visible(GTK_WIDGET(window), TRUE);
        } else if (strncmp(line, "HIDE", 4) == 0) {
            gtk_widget_set_visible(GTK_WIDGET(window), FALSE);
        }
        g_free(line);
    } else if (status == G_IO_STATUS_EOF) {
        g_application_quit(G_APPLICATION(data));
        return FALSE;
    }

    return TRUE;
}

static void activate(GtkApplication *app, gpointer user_data) {
    (void)user_data;

    window = GTK_WINDOW(gtk_application_window_new(app));

    /* Layer shell setup */
    gtk_layer_init_for_window(window);
    gtk_layer_set_layer(window, GTK_LAYER_SHELL_LAYER_OVERLAY);
    gtk_layer_set_anchor(window, GTK_LAYER_SHELL_EDGE_LEFT, TRUE);
    gtk_layer_set_anchor(window, GTK_LAYER_SHELL_EDGE_RIGHT, TRUE);
    gtk_layer_set_anchor(window, GTK_LAYER_SHELL_EDGE_BOTTOM, TRUE);
    gtk_layer_set_namespace(window, "osk-overlay");
    gtk_layer_set_keyboard_mode(window, GTK_LAYER_SHELL_KEYBOARD_MODE_NONE);

    /* CSS */
    provider = gtk_css_provider_new();
    /* Opaque background: page content bleeding through a translucent
     * overlay makes thin glyphs (. , ' -) unreadable on this panel. */
    gtk_css_provider_load_from_string(provider,
        "window { background: #16161e; }"
        "label { color: #e5e9f0; font-size: 20px; font-family: monospace;"
        "  min-width: 36px; min-height: 34px; padding: 3px 5px;"
        "  background: #232334; border-radius: 6px; }"
        "label.highlight { background: #88c0d0; color: #2e3440;"
        "  font-weight: bold; }");
    gtk_style_context_add_provider_for_display(
        gdk_display_get_default(),
        GTK_STYLE_PROVIDER(provider),
        GTK_STYLE_PROVIDER_PRIORITY_APPLICATION);

    /* Build grid */
    GtkWidget *grid = gtk_grid_new();
    gtk_grid_set_column_homogeneous(GTK_GRID(grid), FALSE);
    gtk_grid_set_row_homogeneous(GTK_GRID(grid), TRUE);
    gtk_grid_set_column_spacing(GTK_GRID(grid), 2);
    gtk_grid_set_row_spacing(GTK_GRID(grid), 2);
    gtk_widget_set_halign(grid, GTK_ALIGN_CENTER);
    gtk_widget_set_margin_start(grid, 4);
    gtk_widget_set_margin_end(grid, 4);
    gtk_widget_set_margin_top(grid, 4);
    gtk_widget_set_margin_bottom(grid, 4);

    const char **layout = layer_rows(cur_layer);
    for (int r = 0; r < NUM_ROWS; r++) {
        for (int c = 0; c < row_lens[r]; c++) {
            char ch[2] = { layout[r][c], '\0' };
            labels[r][c] = gtk_label_new(ch);
            gtk_grid_attach(GTK_GRID(grid), labels[r][c], c, r, 1, 1);
        }
    }

    gtk_window_set_child(window, grid);
    update_highlight();

    /* Watch stdin for position updates */
    GIOChannel *stdin_channel = g_io_channel_unix_new(fileno(stdin));
    g_io_add_watch(stdin_channel, G_IO_IN | G_IO_HUP | G_IO_ERR,
                   on_stdin, app);
    g_io_channel_unref(stdin_channel);

    /* Present once so the layer surface exists, then start hidden and
     * wait for IM activation or a SHOW command. */
    gtk_window_present(window);
    gtk_widget_set_visible(GTK_WIDGET(window), FALSE);

    /* Keep running while hidden (no visible window would otherwise
     * end the app's activity). */
    g_application_hold(G_APPLICATION(app));

    setup_input_method();
}

int main(int argc, char *argv[]) {
    GtkApplication *app = gtk_application_new(
        "com.ogu.osk.overlay",
        G_APPLICATION_NON_UNIQUE);
    g_signal_connect(app, "activate", G_CALLBACK(activate), NULL);
    int status = g_application_run(G_APPLICATION(app), argc, argv);
    g_object_unref(app);
    return status;
}
