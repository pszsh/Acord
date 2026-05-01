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

#define PAREN (1 << 0)

#define BRACKET (1 << 1)

#define BRACE (1 << 2)

#define SINGLE (1 << 3)

#define DOUBLE (1 << 4)

#define BACKTICK (1 << 5)

#define ALL (((((PAREN | BRACKET) | BRACE) | SINGLE) | DOUBLE) | BACKTICK)

#define BASE_BOOST 0.30

#define THRESHOLD_PX 6.0

#define EVAL_RESULT_KIND 24

#define EVAL_ERROR_KIND 25

#define USER_IDENT_PALETTE_SIZE 8

#define USER_IDENT_HOP 3

/**
 * Owns the browser window's wgpu surface, iced renderer, and BrowserState.
 */
typedef struct BrowserHandle BrowserHandle;

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

void viewport_set_auto_pair_flags(struct ViewportHandle *handle, uint32_t flags);

uint32_t viewport_get_auto_pair_flags(void);

void viewport_send_command(struct ViewportHandle *handle, uint32_t command);

void viewport_set_settings_view(struct ViewportHandle *handle,
                                const char *theme_mode,
                                const char *line_indicator,
                                bool gutter_rainbow,
                                const char *auto_save_dir);

char *viewport_take_shell_action(struct ViewportHandle *handle);

/**
 * Export the note as a standalone Rust crate at `out_dir/name/`. Returns
 * a heap-allocated C string on success (the absolute path of the created
 * folder), or null on failure. Free the returned string with
 * `viewport_free_string`.
 */
char *viewport_export_crate(struct ViewportHandle *handle, const char *out_dir, const char *name);

struct BrowserHandle *browser_create(void *nsview,
                                     float width,
                                     float height,
                                     float scale,
                                     const char *notes_dir);

void browser_destroy(struct BrowserHandle *handle);

void browser_render(struct BrowserHandle *handle);

void browser_resize(struct BrowserHandle *handle, float width, float height, float scale);

void browser_mouse_event(struct BrowserHandle *handle,
                         float x,
                         float y,
                         uint8_t button,
                         bool pressed);

void browser_scroll_event(struct BrowserHandle *handle, float delta_x, float delta_y);

void browser_key_event(struct BrowserHandle *handle,
                       uint32_t key,
                       uint32_t modifiers,
                       bool pressed,
                       const char *text);

char *browser_take_pending_open(struct BrowserHandle *handle);

void browser_refresh(struct BrowserHandle *handle);

/**
 * dispatches a numeric zoom command into the browser's scale state.
 */
void browser_send_command(struct BrowserHandle *handle, uint32_t command);

uint32_t viewport_render_mode(struct ViewportHandle *handle);

#endif  /* ACORD_VIEWPORT_H */
