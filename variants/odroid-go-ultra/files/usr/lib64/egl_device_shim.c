/*
 * egl_device_shim.c - LD_PRELOAD shim to fake EGL_EXT_device_base for Mali
 *
 * Mali's EGL doesn't support EGL_EXT_device_base (which requires
 * EGL_EXT_device_enumeration + EGL_EXT_device_query). Smithay requires
 * these extensions. This shim fakes ALL required functions.
 *
 * Build: gcc -shared -fPIC -o egl_device_shim.so egl_device_shim.c -ldl
 * Use:   LD_PRELOAD=/usr/lib64/egl_device_shim.so niri --session
 */

#define _GNU_SOURCE
#include <dlfcn.h>
#include <string.h>
#include <stdio.h>
#include <stdlib.h>
#include <stdint.h>

/* EGL constants */
#define EGL_EXTENSIONS 0x3055
#define EGL_DEVICE_EXT 0x322C
#define EGL_DRM_RENDER_NODE_FILE_EXT 0x3377
#define EGL_DRM_DEVICE_FILE_EXT 0x3233

typedef void *EGLDisplay;
typedef void *EGLDeviceEXT;
typedef unsigned int EGLBoolean;
typedef int EGLint;
typedef intptr_t EGLAttrib;

static int fake_device_sentinel = 1;
#define FAKE_DEVICE ((EGLDeviceEXT)&fake_device_sentinel)

static char *augmented_exts = NULL;

static const char *(*real_eglQueryString)(EGLDisplay dpy, EGLint name) = NULL;
static void *(*real_eglGetProcAddress)(const char *name) = NULL;

static void ensure_real_funcs(void) {
    if (!real_eglQueryString)
        real_eglQueryString = dlsym(RTLD_NEXT, "eglQueryString");
    if (!real_eglGetProcAddress)
        real_eglGetProcAddress = dlsym(RTLD_NEXT, "eglGetProcAddress");
}

static const char *wrapped_eglQueryString(EGLDisplay dpy, EGLint name) {
    ensure_real_funcs();
    const char *result = real_eglQueryString(dpy, name);

    if (name == EGL_EXTENSIONS && result != NULL) {
        if (strstr(result, "EGL_EXT_device_base") != NULL)
            return result;

        free(augmented_exts);
        const char *extra = " EGL_EXT_device_base EGL_EXT_device_query EGL_EXT_device_enumeration";
        augmented_exts = malloc(strlen(result) + strlen(extra) + 1);
        if (augmented_exts) {
            strcpy(augmented_exts, result);
            strcat(augmented_exts, extra);
            return augmented_exts;
        }
    }
    return result;
}

const char *eglQueryString(EGLDisplay dpy, EGLint name) {
    return wrapped_eglQueryString(dpy, name);
}

/* EGL_EXT_device_query: eglQueryDisplayAttribEXT */
static EGLBoolean my_eglQueryDisplayAttribEXT(EGLDisplay dpy, EGLint attr, EGLAttrib *value) {
    if (attr == EGL_DEVICE_EXT && value) {
        *value = (EGLAttrib)FAKE_DEVICE;
        return 1;
    }
    return 0;
}

/* EGL_EXT_device_query: eglQueryDeviceStringEXT */
static const char *my_eglQueryDeviceStringEXT(EGLDeviceEXT device, EGLint name) {
    if (device == FAKE_DEVICE) {
        switch (name) {
            case EGL_EXTENSIONS:
                return "";
            case EGL_DRM_RENDER_NODE_FILE_EXT:
                return "/dev/dri/card0";
            case EGL_DRM_DEVICE_FILE_EXT:
                return "/dev/dri/card0";
            default:
                return NULL;
        }
    }
    return NULL;
}

/* EGL_EXT_device_query: eglQueryDeviceAttribEXT */
static EGLBoolean my_eglQueryDeviceAttribEXT(EGLDeviceEXT device, EGLint attr, EGLAttrib *value) {
    /* No device attributes to report, but return success for our fake device */
    (void)attr;
    (void)value;
    if (device == FAKE_DEVICE)
        return 0; /* EGL_FALSE - no attributes available */
    return 0;
}

/* EGL_EXT_device_enumeration: eglQueryDevicesEXT */
static EGLBoolean my_eglQueryDevicesEXT(EGLint max_devices, EGLDeviceEXT *devices, EGLint *num_devices) {
    if (num_devices)
        *num_devices = 1;
    if (devices && max_devices >= 1)
        devices[0] = FAKE_DEVICE;
    return 1; /* EGL_TRUE */
}

void *eglGetProcAddress(const char *name) {
    ensure_real_funcs();

    /* Core function shimming */
    if (strcmp(name, "eglQueryString") == 0)
        return (void *)wrapped_eglQueryString;

    /* EGL_EXT_device_query functions */
    if (strcmp(name, "eglQueryDisplayAttribEXT") == 0)
        return (void *)my_eglQueryDisplayAttribEXT;
    if (strcmp(name, "eglQueryDeviceStringEXT") == 0)
        return (void *)my_eglQueryDeviceStringEXT;
    if (strcmp(name, "eglQueryDeviceAttribEXT") == 0)
        return (void *)my_eglQueryDeviceAttribEXT;

    /* EGL_EXT_device_enumeration functions */
    if (strcmp(name, "eglQueryDevicesEXT") == 0)
        return (void *)my_eglQueryDevicesEXT;

    return real_eglGetProcAddress(name);
}
