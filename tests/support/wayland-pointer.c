/* Persistent virtual pointer for the isolated Wayland tests, never shipped. */
#include <stdint.h>
#include <stdio.h>
#include <string.h>
#include <time.h>
#include <wayland-client.h>
#include "virtual-pointer.h"

static struct zwlr_virtual_pointer_manager_v1 *manager;
static void global(void *data, struct wl_registry *registry, uint32_t id,
                   const char *interface, uint32_t version) {
    (void)data;
    (void)version;
    if (!strcmp(interface, "zwlr_virtual_pointer_manager_v1"))
        manager = wl_registry_bind(registry, id,
                                   &zwlr_virtual_pointer_manager_v1_interface, 1);
}
static void removed(void *data, struct wl_registry *registry, uint32_t id) {
    (void)data;
    (void)registry;
    (void)id;
}
int main(void) {
    struct wl_display *display = wl_display_connect(NULL);
    if (!display) return 1;
    struct wl_registry *registry = wl_display_get_registry(display);
    const struct wl_registry_listener listener = {global, removed};
    wl_registry_add_listener(registry, &listener, NULL);
    if (wl_display_roundtrip(display) < 0 || !manager) return 2;
    struct zwlr_virtual_pointer_v1 *pointer =
        zwlr_virtual_pointer_manager_v1_create_virtual_pointer(manager, NULL);
    if (wl_display_roundtrip(display) < 0) return 3;
    puts("ready");
    fflush(stdout);
    char line[128];
    while (fgets(line, sizeof line, stdin)) {
        struct timespec now;
        clock_gettime(CLOCK_MONOTONIC, &now);
        uint32_t time = (uint32_t)(now.tv_sec * 1000 + now.tv_nsec / 1000000);
        unsigned x, y, width, height;
        if (sscanf(line, "move %u %u %u %u", &x, &y, &width, &height) == 4) {
            zwlr_virtual_pointer_v1_motion_absolute(pointer, time, x, y, width, height);
        } else if (!strcmp(line, "press\n") || !strcmp(line, "release\n")) {
            zwlr_virtual_pointer_v1_button(pointer, time, 0x110, line[0] == 'p');
        } else {
            return 4;
        }
        zwlr_virtual_pointer_v1_frame(pointer);
        if (wl_display_roundtrip(display) < 0) return 5;
        puts("ok");
        fflush(stdout);
    }
    zwlr_virtual_pointer_v1_destroy(pointer);
    zwlr_virtual_pointer_manager_v1_destroy(manager);
    wl_registry_destroy(registry);
    wl_display_disconnect(display);
    return 0;
}
