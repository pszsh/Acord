#include <stdarg.h>
#include <stdbool.h>
#include <stdint.h>
#include <stdlib.h>
#include "../../core/include/acord.h"


#ifndef ACORD_VIEWPORT_H
#define ACORD_VIEWPORT_H

#include <stdarg.h>
#include <stdbool.h>
#include <stdint.h>
#include <stdlib.h>

#define BASE_BOOST 0.30

#define THRESHOLD_PX 6.0

#define EVAL_RESULT_KIND 24

#define EVAL_ERROR_KIND 25

#define USER_IDENT_PALETTE_SIZE 8

#define USER_IDENT_HOP 3

typedef struct TextPos TextPos;

typedef struct ViewportHandle ViewportHandle;



struct ViewportHandle *viewport_create(void *nsview, float width, float height, float scale);

void viewport_destroy(struct ViewportHandle *handle);

void viewport_render(struct ViewportHandle *handle);

void viewport_resize(struct ViewportHandle *handle, float width, float height, float scale);

void viewport_mouse_event(struct ViewportHandle *handle,
                          float x,
                          float y,
                          uint8_t button,
                          bool pressed);

void viewport_key_event(struct ViewportHandle *handle,
                        uint32_t key,
                        uint32_t modifiers,
                        bool pressed,
                        const char *text);

void viewport_scroll_event(struct ViewportHandle *handle,
                           float x,
                           float y,
                           float delta_x,
                           float delta_y);

void viewport_set_text(struct ViewportHandle *handle, const char *text);

void viewport_set_lang(struct ViewportHandle *handle, const char *ext);

char *viewport_get_text(struct ViewportHandle *handle);

void viewport_free_string(char *s);

void viewport_set_theme(struct ViewportHandle *handle, const char *name);

void viewport_set_line_indicator(struct ViewportHandle *handle, const char *mode);

void viewport_set_gutter_rainbow(struct ViewportHandle *handle, bool enabled);

void viewport_send_command(struct ViewportHandle *handle, uint32_t command);

/**
 * Export the note as a standalone Rust crate at `out_dir/name/`. Returns
 * a heap-allocated C string on success (the absolute path of the created
 * folder), or null on failure. Free the returned string with
 * `viewport_free_string`.
 */
char *viewport_export_crate(struct ViewportHandle *handle, const char *out_dir, const char *name);

uint32_t viewport_render_mode(struct ViewportHandle *handle);

#endif  /* ACORD_VIEWPORT_H */
