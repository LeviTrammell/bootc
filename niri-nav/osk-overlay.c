/*
 * osk-overlay: GTK4 + gtk-layer-shell on-screen keyboard overlay
 *
 * Renders a QWERTY grid as a bottom layer-shell surface with a highlighted
 * key. Reads "POS row col shift\n" commands from stdin to update the
 * highlight position and shift state. Exits on stdin EOF (parent died).
 *
 * Build:
 *   gcc -o osk-overlay osk-overlay.c \
 *     $(pkg-config --cflags --libs gtk4 gtk4-layer-shell-0)
 */

#include <gtk/gtk.h>
#include <gtk4-layer-shell.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

/* QWERTY layout: 4 rows */
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

#define NUM_ROWS 4
static const int row_lens[] = { 10, 10, 9, 10 };

/* State */
static int cur_row = 1, cur_col = 0, cur_shift = 0;
static GtkWidget *labels[NUM_ROWS][10];
static GtkCssProvider *provider;

static void update_highlight(void) {
    const char **layout = cur_shift ? shifted_rows : normal_rows;

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

static gboolean on_stdin(GIOChannel *source, GIOCondition cond, gpointer data) {
    (void)data;

    if (cond & (G_IO_HUP | G_IO_ERR)) {
        /* Parent died, exit cleanly */
        g_application_quit(G_APPLICATION(data));
        return FALSE;
    }

    gchar *line = NULL;
    gsize len = 0;
    GIOStatus status = g_io_channel_read_line(source, &line, &len, NULL, NULL);

    if (status == G_IO_STATUS_NORMAL && line) {
        int row, col, shift;
        if (sscanf(line, "POS %d %d %d", &row, &col, &shift) == 3) {
            if (row >= 0 && row < NUM_ROWS && col >= 0 && col < row_lens[row]) {
                cur_row = row;
                cur_col = col;
                cur_shift = shift;
                update_highlight();
            }
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

    GtkWindow *window = GTK_WINDOW(gtk_application_window_new(app));

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
    gtk_css_provider_load_from_string(provider,
        "window { background: rgba(0,0,0,0.85); }"
        "label { color: #d8dee9; font-size: 16px; font-family: monospace;"
        "  min-width: 28px; min-height: 28px; padding: 2px 4px; }"
        "label.highlight { background: #88c0d0; color: #2e3440;"
        "  border-radius: 4px; font-weight: bold; }");
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

    const char **layout = cur_shift ? shifted_rows : normal_rows;
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

    gtk_window_present(window);
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
