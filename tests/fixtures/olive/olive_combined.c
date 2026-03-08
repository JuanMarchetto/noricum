/* olive_combined.c — tsoding/olive.c 2D graphics library + test harness
 * Source: https://github.com/tsoding/olive.c (MIT License, Alexey Kutepov)
 * Compile: gcc -std=gnu11 -Wall -o olive_test olive_combined.c -lm
 */

#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <math.h>

#define OLIVEC_IMPLEMENTATION

// Copyright 2022 Alexey Kutepov <reximkut@gmail.com>
//
// Permission is hereby granted, free of charge, to any person obtaining
// a copy of this software and associated documentation files (the
// "Software"), to deal in the Software without restriction, including
// without limitation the rights to use, copy, modify, merge, publish,
// distribute, sublicense, and/or sell copies of the Software, and to
// permit persons to whom the Software is furnished to do so, subject to
// the following conditions:
//
// The above copyright notice and this permission notice shall be
// included in all copies or substantial portions of the Software.
//
// THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND,
// EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF
// MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND
// NONINFRINGEMENT. IN NO EVENT SHALL THE AUTHORS OR COPYRIGHT HOLDERS BE
// LIABLE FOR ANY CLAIM, DAMAGES OR OTHER LIABILITY, WHETHER IN AN ACTION
// OF CONTRACT, TORT OR OTHERWISE, ARISING FROM, OUT OF OR IN CONNECTION
// WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE SOFTWARE.

#ifndef OLIVE_C_
#define OLIVE_C_

#include <stddef.h>
#include <stdint.h>
#include <stdbool.h>

#ifndef OLIVECDEF
#define OLIVECDEF static inline
#endif

#ifndef OLIVEC_AA_RES
#define OLIVEC_AA_RES 2
#endif

#define OLIVEC_SWAP(T, a, b) do { T t = a; a = b; b = t; } while (0)
#define OLIVEC_SIGN(T, x) ((T)((x) > 0) - (T)((x) < 0))
#define OLIVEC_ABS(T, x) (OLIVEC_SIGN(T, x)*(x))

typedef struct {
    size_t width, height;
    const char *glyphs;
} Olivec_Font;

#define OLIVEC_DEFAULT_FONT_HEIGHT 6
#define OLIVEC_DEFAULT_FONT_WIDTH 6
// TODO: allocate proper descender and acender areas for the default font
static char olivec_default_glyphs[128][OLIVEC_DEFAULT_FONT_HEIGHT][OLIVEC_DEFAULT_FONT_WIDTH] = {
    ['a'] = {
        {0, 0, 0, 0, 0},
        {0, 1, 1, 0, 0},
        {0, 0, 0, 1, 0},
        {0, 1, 1, 1, 0},
        {1, 0, 0, 1, 0},
        {0, 1, 1, 1, 0},
    },
    ['b'] = {
        {1, 0, 0, 0, 0},
        {1, 1, 1, 0, 0},
        {1, 0, 0, 1, 0},
        {1, 0, 0, 1, 0},
        {1, 0, 0, 1, 0},
        {1, 1, 1, 0, 0},
    },
    ['c'] = {
        {0, 0, 0, 0, 0},
        {0, 1, 1, 0, 0},
        {1, 0, 0, 1, 0},
        {1, 0, 0, 0, 0},
        {1, 0, 0, 1, 0},
        {0, 1, 1, 0, 0},
    },
    ['d'] = {
        {0, 0, 0, 1, 0},
        {0, 1, 1, 1, 0},
        {1, 0, 0, 1, 0},
        {1, 0, 0, 1, 0},
        {1, 0, 0, 1, 0},
        {0, 1, 1, 1, 0},
    },
    ['e'] = {
        {0, 0, 0, 0, 0},
        {0, 1, 1, 0, 0},
        {1, 0, 0, 1, 0},
        {1, 1, 1, 1, 0},
        {1, 0, 0, 0, 0},
        {0, 1, 1, 1, 0},
    },
    ['f'] = {
        {0, 0, 1, 1, 0},
        {0, 1, 0, 0, 0},
        {1, 1, 1, 1, 0},
        {0, 1, 0, 0, 0},
        {0, 1, 0, 0, 0},
        {0, 1, 0, 0, 0},
    },
    ['g'] = {
        {0, 1, 1, 1, 0},
        {1, 0, 0, 1, 0},
        {1, 0, 0, 1, 0},
        {0, 1, 1, 1, 0},
        {0, 0, 0, 1, 0},
        {0, 1, 1, 0, 0},
    },
    ['h'] = {
        {1, 0, 0, 0, 0},
        {1, 1, 1, 0, 0},
        {1, 0, 0, 1, 0},
        {1, 0, 0, 1, 0},
        {1, 0, 0, 1, 0},
        {1, 0, 0, 1, 0},
    },
    ['i'] = {
        {0, 0, 1, 0, 0},
        {0, 0, 0, 0, 0},
        {0, 0, 1, 0, 0},
        {0, 0, 1, 0, 0},
        {0, 0, 1, 0, 0},
        {0, 0, 1, 0, 0},
    },
    ['j'] = {
        {0, 0, 1, 0, 0},
        {0, 0, 0, 0, 0},
        {0, 0, 1, 0, 0},
        {0, 0, 1, 0, 0},
        {1, 0, 1, 0, 0},
        {0, 1, 1, 0, 0},
    },
    ['k'] = {
        {1, 0, 0, 0, 0},
        {1, 0, 0, 1, 0},
        {1, 0, 1, 0, 0},
        {1, 1, 0, 0, 0},
        {1, 0, 1, 0, 0},
        {1, 0, 0, 1, 0},
    },
    ['l'] = {
        {0, 1, 1, 0, 0},
        {0, 0, 1, 0, 0},
        {0, 0, 1, 0, 0},
        {0, 0, 1, 0, 0},
        {0, 0, 1, 0, 0},
        {0, 1, 1, 1, 0},
    },
    ['m'] = {
        {0, 0, 0, 0, 0},
        {0, 1, 0, 1, 1},
        {1, 0, 1, 0, 1},
        {1, 0, 1, 0, 1},
        {1, 0, 1, 0, 1},
        {1, 0, 1, 0, 1},
    },
    ['n'] = {
        {0, 0, 0, 0, 0},
        {0, 1, 1, 1, 0},
        {1, 0, 0, 1, 0},
        {1, 0, 0, 1, 0},
        {1, 0, 0, 1, 0},
        {1, 0, 0, 1, 0},
    },
    ['o'] = {
        {0, 0, 0, 0, 0},
        {0, 1, 1, 0, 0},
        {1, 0, 0, 1, 0},
        {1, 0, 0, 1, 0},
        {1, 0, 0, 1, 0},
        {0, 1, 1, 0, 0},
    },
    ['p'] = {
        {1, 1, 1, 0, 0},
        {1, 0, 0, 1, 0},
        {1, 0, 0, 1, 0},
        {1, 1, 1, 0, 0},
        {1, 0, 0, 0, 0},
        {1, 0, 0, 0, 0},
    },
    ['q'] = {
        {0, 1, 1, 1, 0},
        {1, 0, 0, 1, 0},
        {1, 0, 0, 1, 0},
        {0, 1, 1, 1, 0},
        {0, 0, 0, 1, 0},
        {0, 0, 0, 1, 0},
    },
    ['r'] = {
        {0, 0, 0, 0, 0},
        {1, 0, 1, 1, 0},
        {1, 1, 0, 0, 1},
        {1, 0, 0, 0, 0},
        {1, 0, 0, 0, 0},
        {1, 0, 0, 0, 0},
    },
    ['s'] = {
        {0, 0, 0, 0, 0},
        {0, 1, 1, 1, 0},
        {1, 0, 0, 0, 0},
        {1, 1, 1, 1, 0},
        {0, 0, 0, 1, 0},
        {1, 1, 1, 0, 0},
    },
    ['t'] = {
        {0, 1, 0, 0, 0},
        {0, 1, 0, 0, 0},
        {1, 1, 1, 1, 0},
        {0, 1, 0, 0, 0},
        {0, 1, 0, 1, 0},
        {0, 1, 1, 0, 0},
    },
    ['u'] = {
        {0, 0, 0, 0, 0},
        {1, 0, 0, 1, 0},
        {1, 0, 0, 1, 0},
        {1, 0, 0, 1, 0},
        {1, 0, 0, 1, 0},
        {0, 1, 1, 1, 0},
    },
    ['v'] = {
        {0, 0, 0, 0, 0},
        {1, 0, 0, 1, 0},
        {1, 0, 0, 1, 0},
        {1, 0, 0, 1, 0},
        {1, 0, 0, 1, 0},
        {0, 1, 1, 0, 0},
    },
    ['w'] = {
        {0, 0, 0, 0, 0},
        {1, 0, 0, 0, 1},
        {1, 0, 1, 0, 1},
        {1, 0, 1, 0, 1},
        {1, 0, 1, 0, 1},
        {0, 1, 1, 1, 1},
    },
    ['x'] = {
        {0, 0, 0, 0, 0},
        {1, 0, 1, 0, 0},
        {1, 0, 1, 0, 0},
        {0, 1, 0, 0, 0},
        {1, 0, 1, 0, 0},
        {1, 0, 1, 0, 0},
    },
    ['y'] = {
        {0, 0, 0, 0, 0},
        {1, 0, 1, 0, 0},
        {1, 0, 1, 0, 0},
        {1, 0, 1, 0, 0},
        {0, 1, 0, 0, 0},
        {0, 1, 0, 0, 0},
    },
    ['z'] = {
        {0, 0, 0, 0, 0},
        {1, 1, 1, 1, 0},
        {0, 0, 0, 1, 0},
        {0, 1, 1, 0, 0},
        {1, 0, 0, 0, 0},
        {1, 1, 1, 1, 0},
    },

    ['A'] = {0},
    ['B'] = {0},
    ['C'] = {0},
    ['D'] = {0},
    ['E'] = {0},
    ['F'] = {0},
    ['G'] = {0},
    ['H'] = {0},
    ['I'] = {0},
    ['J'] = {0},
    ['K'] = {0},
    ['L'] = {0},
    ['M'] = {0},
    ['N'] = {0},
    ['O'] = {0},
    ['P'] = {0},
    ['Q'] = {0},
    ['R'] = {0},
    ['S'] = {0},
    ['T'] = {0},
    ['U'] = {0},
    ['V'] = {0},
    ['W'] = {0},
    ['X'] = {0},
    ['Y'] = {0},
    ['Z'] = {0},

    ['0'] = {
        {0, 1, 1, 0, 0},
        {1, 0, 0, 1, 0},
        {1, 0, 0, 1, 0},
        {1, 0, 0, 1, 0},
        {1, 0, 0, 1, 0},
        {0, 1, 1, 0, 0},
    },
    ['1'] = {
        {0, 0, 1, 0, 0},
        {0, 1, 1, 0, 0},
        {0, 0, 1, 0, 0},
        {0, 0, 1, 0, 0},
        {0, 0, 1, 0, 0},
        {0, 1, 1, 1, 0},
    },
    ['2'] = {
        {0, 1, 1, 0, 0},
        {1, 0, 0, 1, 0},
        {0, 0, 0, 1, 0},
        {0, 1, 1, 0, 0},
        {1, 0, 0, 0, 0},
        {1, 1, 1, 1, 0},
    },
    ['3'] = {
        {0, 1, 1, 0, 0},
        {1, 0, 0, 1, 0},
        {0, 0, 1, 0, 0},
        {0, 0, 0, 1, 0},
        {1, 0, 0, 1, 0},
        {0, 1, 1, 0, 0},
    },
    ['4'] = {
        {0, 0, 1, 1, 0},
        {0, 1, 0, 1, 0},
        {1, 0, 0, 1, 0},
        {1, 1, 1, 1, 1},
        {0, 0, 0, 1, 0},
        {0, 0, 0, 1, 0},
    },
    ['5'] = {
        {1, 1, 1, 0, 0},
        {1, 0, 0, 0, 0},
        {1, 1, 1, 0, 0},
        {0, 0, 0, 1, 0},
        {1, 0, 0, 1, 0},
        {0, 1, 1, 0, 0},
    },
    ['6'] = {
        {0, 1, 1, 0, 0},
        {1, 0, 0, 0, 0},
        {1, 1, 1, 0, 0},
        {1, 0, 0, 1, 0},
        {1, 0, 0, 1, 0},
        {0, 1, 1, 0, 0},
    },
    ['7'] = {
        {1, 1, 1, 1, 0},
        {0, 0, 0, 1, 0},
        {0, 0, 1, 0, 0},
        {0, 1, 0, 0, 0},
        {0, 1, 0, 0, 0},
        {0, 1, 0, 0, 0},
    },
    ['8'] = {
        {0, 1, 1, 0, 0},
        {1, 0, 0, 1, 0},
        {0, 1, 1, 0, 0},
        {1, 0, 0, 1, 0},
        {1, 0, 0, 1, 0},
        {0, 1, 1, 0, 0},

    },
    ['9'] = {
        {0, 1, 1, 0, 0},
        {1, 0, 0, 1, 0},
        {1, 0, 0, 1, 0},
        {0, 1, 1, 1, 0},
        {0, 0, 0, 1, 0},
        {0, 1, 1, 0, 0},
    },

    [','] = {
        {0, 0, 0, 0, 0},
        {0, 0, 0, 0, 0},
        {0, 0, 0, 0, 0},
        {0, 0, 0, 0, 0},
        {0, 0, 0, 1, 0},
        {0, 0, 1, 0, 0},
    },

    ['.'] = {
        {0, 0, 0, 0, 0},
        {0, 0, 0, 0, 0},
        {0, 0, 0, 0, 0},
        {0, 0, 0, 0, 0},
        {0, 0, 0, 0, 0},
        {0, 0, 1, 0, 0},
    },
    ['-'] = {
        {0, 0, 0, 0, 0},
        {0, 0, 0, 0, 0},
        {0, 0, 0, 0, 0},
        {1, 1, 1, 1, 0},
        {0, 0, 0, 0, 0},
        {0, 0, 0, 0, 0},
    },
};

static Olivec_Font olivec_default_font = {
    .glyphs = &olivec_default_glyphs[0][0][0],
    .width = OLIVEC_DEFAULT_FONT_WIDTH,
    .height = OLIVEC_DEFAULT_FONT_HEIGHT,
};

// WARNING! Always initialize your Canvas with a color that has Non-Zero Alpha Channel!
// A lot of functions use `olivec_blend_color()` function to blend with the Background
// which preserves the original Alpha of the Background. So you may easily end up with
// a result that is perceptually transparent if the Alpha is Zero.
typedef struct {
    uint32_t *pixels;
    size_t width;
    size_t height;
    size_t stride;
} Olivec_Canvas;

#define OLIVEC_CANVAS_NULL ((Olivec_Canvas) {0})
#define OLIVEC_PIXEL(oc, x, y) (oc).pixels[(y)*(oc).stride + (x)]

OLIVECDEF Olivec_Canvas olivec_canvas(uint32_t *pixels, size_t width, size_t height, size_t stride);
OLIVECDEF Olivec_Canvas olivec_subcanvas(Olivec_Canvas oc, int x, int y, int w, int h);
OLIVECDEF bool olivec_in_bounds(Olivec_Canvas oc, int x, int y);
OLIVECDEF void olivec_blend_color(uint32_t *c1, uint32_t c2);
OLIVECDEF void olivec_fill(Olivec_Canvas oc, uint32_t color);
OLIVECDEF void olivec_rect(Olivec_Canvas oc, int x, int y, int w, int h, uint32_t color);
OLIVECDEF void olivec_frame(Olivec_Canvas oc, int x, int y, int w, int h, size_t thiccness, uint32_t color);
OLIVECDEF void olivec_circle(Olivec_Canvas oc, int cx, int cy, int r, uint32_t color);
OLIVECDEF void olivec_ellipse(Olivec_Canvas oc, int cx, int cy, int rx, int ry, uint32_t color);
// TODO: lines with different thiccness
OLIVECDEF void olivec_line(Olivec_Canvas oc, int x1, int y1, int x2, int y2, uint32_t color);
OLIVECDEF bool olivec_normalize_triangle(size_t width, size_t height, int x1, int y1, int x2, int y2, int x3, int y3, int *lx, int *hx, int *ly, int *hy);
OLIVECDEF bool olivec_barycentric(int x1, int y1, int x2, int y2, int x3, int y3, int xp, int yp, int *u1, int *u2, int *det);
OLIVECDEF void olivec_triangle(Olivec_Canvas oc, int x1, int y1, int x2, int y2, int x3, int y3, uint32_t color);
OLIVECDEF void olivec_triangle3c(Olivec_Canvas oc, int x1, int y1, int x2, int y2, int x3, int y3, uint32_t c1, uint32_t c2, uint32_t c3);
OLIVECDEF void olivec_triangle3z(Olivec_Canvas oc, int x1, int y1, int x2, int y2, int x3, int y3, float z1, float z2, float z3);
OLIVECDEF void olivec_triangle3uv(Olivec_Canvas oc, int x1, int y1, int x2, int y2, int x3, int y3, float tx1, float ty1, float tx2, float ty2, float tx3, float ty3, float z1, float z2, float z3, Olivec_Canvas texture);
OLIVECDEF void olivec_triangle3uv_bilinear(Olivec_Canvas oc, int x1, int y1, int x2, int y2, int x3, int y3, float tx1, float ty1, float tx2, float ty2, float tx3, float ty3, float z1, float z2, float z3, Olivec_Canvas texture);
OLIVECDEF void olivec_text(Olivec_Canvas oc, const char *text, int x, int y, Olivec_Font font, size_t size, uint32_t color);
OLIVECDEF void olivec_sprite_blend(Olivec_Canvas oc, int x, int y, int w, int h, Olivec_Canvas sprite);
OLIVECDEF void olivec_sprite_copy(Olivec_Canvas oc, int x, int y, int w, int h, Olivec_Canvas sprite);
OLIVECDEF void olivec_sprite_copy_bilinear(Olivec_Canvas oc, int x, int y, int w, int h, Olivec_Canvas sprite);
OLIVECDEF uint32_t olivec_pixel_bilinear(Olivec_Canvas sprite, int nx, int ny, int w, int h);

typedef struct {
    // Safe ranges to iterate over.
    int x1, x2;
    int y1, y2;

    // Original uncut ranges some parts of which may be outside of the canvas boundaries.
    int ox1, ox2;
    int oy1, oy2;
} Olivec_Normalized_Rect;

// The point of this function is to produce two ranges x1..x2 and y1..y2 that are guaranteed to be safe to iterate over the canvas of size pixels_width by pixels_height without any boundary checks.
//
// Olivec_Normalized_Rect nr = {0};
// if (olivec_normalize_rect(x, y, w, h, WIDTH, HEIGHT, &nr)) {
//     for (int x = nr.x1; x <= nr.x2; ++x) {
//         for (int y = nr.y1; y <= nr.y2; ++y) {
//             OLIVEC_PIXEL(oc, x, y) = 0x69696969;
//         }
//     }
// } else {
//     // Rectangle is invisible cause it's completely out-of-bounds
// }
OLIVECDEF bool olivec_normalize_rect(int x, int y, int w, int h,
                                     size_t canvas_width, size_t canvas_height,
                                     Olivec_Normalized_Rect *nr);

#endif // OLIVE_C_

#ifdef OLIVEC_IMPLEMENTATION

OLIVECDEF Olivec_Canvas olivec_canvas(uint32_t *pixels, size_t width, size_t height, size_t stride)
{
    Olivec_Canvas oc = {
        .pixels = pixels,
        .width  = width,
        .height = height,
        .stride = stride,
    };
    return oc;
}

OLIVECDEF bool olivec_normalize_rect(int x, int y, int w, int h,
                                     size_t canvas_width, size_t canvas_height,
                                     Olivec_Normalized_Rect *nr)
{
    // No need to render empty rectangle
    if (w == 0) return false;
    if (h == 0) return false;

    nr->ox1 = x;
    nr->oy1 = y;

    // Convert the rectangle to 2-points representation
    nr->ox2 = nr->ox1 + OLIVEC_SIGN(int, w)*(OLIVEC_ABS(int, w) - 1);
    if (nr->ox1 > nr->ox2) OLIVEC_SWAP(int, nr->ox1, nr->ox2);
    nr->oy2 = nr->oy1 + OLIVEC_SIGN(int, h)*(OLIVEC_ABS(int, h) - 1);
    if (nr->oy1 > nr->oy2) OLIVEC_SWAP(int, nr->oy1, nr->oy2);

    // Cull out invisible rectangle
    if (nr->ox1 >= (int) canvas_width) return false;
    if (nr->ox2 < 0) return false;
    if (nr->oy1 >= (int) canvas_height) return false;
    if (nr->oy2 < 0) return false;

    nr->x1 = nr->ox1;
    nr->y1 = nr->oy1;
    nr->x2 = nr->ox2;
    nr->y2 = nr->oy2;

    // Clamp the rectangle to the boundaries
    if (nr->x1 < 0) nr->x1 = 0;
    if (nr->x2 >= (int) canvas_width) nr->x2 = (int) canvas_width - 1;
    if (nr->y1 < 0) nr->y1 = 0;
    if (nr->y2 >= (int) canvas_height) nr->y2 = (int) canvas_height - 1;

    return true;
}

OLIVECDEF Olivec_Canvas olivec_subcanvas(Olivec_Canvas oc, int x, int y, int w, int h)
{
    Olivec_Normalized_Rect nr = {0};
    if (!olivec_normalize_rect(x, y, w, h, oc.width, oc.height, &nr)) return OLIVEC_CANVAS_NULL;
    oc.pixels = &OLIVEC_PIXEL(oc, nr.x1, nr.y1);
    oc.width = nr.x2 - nr.x1 + 1;
    oc.height = nr.y2 - nr.y1 + 1;
    return oc;
}

// TODO: custom pixel formats
// Maybe we can store pixel format info in Olivec_Canvas
#define OLIVEC_RED(color)   (((color)&0x000000FF)>>(8*0))
#define OLIVEC_GREEN(color) (((color)&0x0000FF00)>>(8*1))
#define OLIVEC_BLUE(color)  (((color)&0x00FF0000)>>(8*2))
#define OLIVEC_ALPHA(color) (((color)&0xFF000000)>>(8*3))
#define OLIVEC_RGBA(r, g, b, a) ((((r)&0xFF)<<(8*0)) | (((g)&0xFF)<<(8*1)) | (((b)&0xFF)<<(8*2)) | (((a)&0xFF)<<(8*3)))

OLIVECDEF void olivec_blend_color(uint32_t *c1, uint32_t c2)
{
    uint32_t r1 = OLIVEC_RED(*c1);
    uint32_t g1 = OLIVEC_GREEN(*c1);
    uint32_t b1 = OLIVEC_BLUE(*c1);
    uint32_t a1 = OLIVEC_ALPHA(*c1);

    uint32_t r2 = OLIVEC_RED(c2);
    uint32_t g2 = OLIVEC_GREEN(c2);
    uint32_t b2 = OLIVEC_BLUE(c2);
    uint32_t a2 = OLIVEC_ALPHA(c2);

    r1 = (r1*(255 - a2) + r2*a2)/255; if (r1 > 255) r1 = 255;
    g1 = (g1*(255 - a2) + g2*a2)/255; if (g1 > 255) g1 = 255;
    b1 = (b1*(255 - a2) + b2*a2)/255; if (b1 > 255) b1 = 255;

    *c1 = OLIVEC_RGBA(r1, g1, b1, a1);
}

OLIVECDEF void olivec_fill(Olivec_Canvas oc, uint32_t color)
{
    for (size_t y = 0; y < oc.height; ++y) {
        for (size_t x = 0; x < oc.width; ++x) {
            OLIVEC_PIXEL(oc, x, y) = color;
        }
    }
}

OLIVECDEF void olivec_rect(Olivec_Canvas oc, int x, int y, int w, int h, uint32_t color)
{
    Olivec_Normalized_Rect nr = {0};
    if (!olivec_normalize_rect(x, y, w, h, oc.width, oc.height, &nr)) return;
    for (int x = nr.x1; x <= nr.x2; ++x) {
        for (int y = nr.y1; y <= nr.y2; ++y) {
            olivec_blend_color(&OLIVEC_PIXEL(oc, x, y), color);
        }
    }
}

OLIVECDEF void olivec_frame(Olivec_Canvas oc, int x, int y, int w, int h, size_t t, uint32_t color)
{
    if (t == 0) return; // Nothing to render

    // Convert the rectangle to 2-points representation
    int x1 = x;
    int y1 = y;
    int x2 = x1 + OLIVEC_SIGN(int, w)*(OLIVEC_ABS(int, w) - 1);
    if (x1 > x2) OLIVEC_SWAP(int, x1, x2);
    int y2 = y1 + OLIVEC_SIGN(int, h)*(OLIVEC_ABS(int, h) - 1);
    if (y1 > y2) OLIVEC_SWAP(int, y1, y2);

    olivec_rect(oc, x1 - t/2, y1 - t/2, (x2 - x1 + 1) + t/2*2, t, color);  // Top
    olivec_rect(oc, x1 - t/2, y1 - t/2, t, (y2 - y1 + 1) + t/2*2, color);  // Left
    olivec_rect(oc, x1 - t/2, y2 + t/2, (x2 - x1 + 1) + t/2*2, -t, color); // Bottom
    olivec_rect(oc, x2 + t/2, y1 - t/2, -t, (y2 - y1 + 1) + t/2*2, color); // Right
}

OLIVECDEF void olivec_ellipse(Olivec_Canvas oc, int cx, int cy, int rx, int ry, uint32_t color)
{
    Olivec_Normalized_Rect nr = {0};
    int rx1 = rx + OLIVEC_SIGN(int, rx);
    int ry1 = ry + OLIVEC_SIGN(int, ry);
    if (!olivec_normalize_rect(cx - rx1, cy - ry1, 2*rx1, 2*ry1, oc.width, oc.height, &nr)) return;

    for (int y = nr.y1; y <= nr.y2; ++y) {
        for (int x = nr.x1; x <= nr.x2; ++x) {
            float nx = (x + 0.5 - nr.x1)/(2.0f*rx1);
            float ny = (y + 0.5 - nr.y1)/(2.0f*ry1);
            float dx = nx - 0.5;
            float dy = ny - 0.5;
            if (dx*dx + dy*dy <= 0.5*0.5) {
                OLIVEC_PIXEL(oc, x, y) = color;
            }
        }
    }
}

OLIVECDEF void olivec_circle(Olivec_Canvas oc, int cx, int cy, int r, uint32_t color)
{
    Olivec_Normalized_Rect nr = {0};
    int r1 = r + OLIVEC_SIGN(int, r);
    if (!olivec_normalize_rect(cx - r1, cy - r1, 2*r1, 2*r1, oc.width, oc.height, &nr)) return;

    for (int y = nr.y1; y <= nr.y2; ++y) {
        for (int x = nr.x1; x <= nr.x2; ++x) {
            int count = 0;
            for (int sox = 0; sox < OLIVEC_AA_RES; ++sox) {
                for (int soy = 0; soy < OLIVEC_AA_RES; ++soy) {
                    // TODO: switch to 64 bits to make the overflow less likely
                    // Also research the probability of overflow
                    int res1 = (OLIVEC_AA_RES + 1);
                    int dx = (x*res1*2 + 2 + sox*2 - res1*cx*2 - res1);
                    int dy = (y*res1*2 + 2 + soy*2 - res1*cy*2 - res1);
                    if (dx*dx + dy*dy <= res1*res1*r*r*2*2) count += 1;
                }
            }
            uint32_t alpha = ((color&0xFF000000)>>(3*8))*count/OLIVEC_AA_RES/OLIVEC_AA_RES;
            uint32_t updated_color = (color&0x00FFFFFF)|(alpha<<(3*8));
            olivec_blend_color(&OLIVEC_PIXEL(oc, x, y), updated_color);
        }
    }
}

OLIVECDEF bool olivec_in_bounds(Olivec_Canvas oc, int x, int y)
{
    return 0 <= x && x < (int) oc.width && 0 <= y && y < (int) oc.height;
}

// TODO: AA for line
OLIVECDEF void olivec_line(Olivec_Canvas oc, int x1, int y1, int x2, int y2, uint32_t color)
{
    int dx = x2 - x1;
    int dy = y2 - y1;

    // If both of the differences are 0 there will be a division by 0 below.
    if (dx == 0 && dy == 0) {
        if (olivec_in_bounds(oc, x1, y1)) {
            olivec_blend_color(&OLIVEC_PIXEL(oc, x1, y1), color);
        }
        return;
    }

    if (OLIVEC_ABS(int, dx) > OLIVEC_ABS(int, dy)) {
        if (x1 > x2) {
            OLIVEC_SWAP(int, x1, x2);
            OLIVEC_SWAP(int, y1, y2);
        }

        for (int x = x1; x <= x2; ++x) {
            int y = dy*(x - x1)/dx + y1;
            // TODO: move boundary checks out side of the loops in olivec_draw_line
            if (olivec_in_bounds(oc, x, y)) {
                olivec_blend_color(&OLIVEC_PIXEL(oc, x, y), color);
            }
        }
    } else {
        if (y1 > y2) {
            OLIVEC_SWAP(int, x1, x2);
            OLIVEC_SWAP(int, y1, y2);
        }

        for (int y = y1; y <= y2; ++y) {
            int x = dx*(y - y1)/dy + x1;
            // TODO: move boundary checks out side of the loops in olivec_draw_line
            if (olivec_in_bounds(oc, x, y)) {
                olivec_blend_color(&OLIVEC_PIXEL(oc, x, y), color);
            }
        }
    }
}

OLIVECDEF uint32_t mix_colors2(uint32_t c1, uint32_t c2, int u1, int det)
{
    // TODO: estimate how much overflows are an issue in integer only environment
    int64_t r1 = OLIVEC_RED(c1);
    int64_t g1 = OLIVEC_GREEN(c1);
    int64_t b1 = OLIVEC_BLUE(c1);
    int64_t a1 = OLIVEC_ALPHA(c1);

    int64_t r2 = OLIVEC_RED(c2);
    int64_t g2 = OLIVEC_GREEN(c2);
    int64_t b2 = OLIVEC_BLUE(c2);
    int64_t a2 = OLIVEC_ALPHA(c2);

    if (det != 0) {
        int u2 = det - u1;
        int64_t r4 = (r1*u2 + r2*u1)/det;
        int64_t g4 = (g1*u2 + g2*u1)/det;
        int64_t b4 = (b1*u2 + b2*u1)/det;
        int64_t a4 = (a1*u2 + a2*u1)/det;

        return OLIVEC_RGBA(r4, g4, b4, a4);
    }

    return 0;
}

OLIVECDEF uint32_t mix_colors3(uint32_t c1, uint32_t c2, uint32_t c3, int u1, int u2, int det)
{
    // TODO: estimate how much overflows are an issue in integer only environment
    int64_t r1 = OLIVEC_RED(c1);
    int64_t g1 = OLIVEC_GREEN(c1);
    int64_t b1 = OLIVEC_BLUE(c1);
    int64_t a1 = OLIVEC_ALPHA(c1);

    int64_t r2 = OLIVEC_RED(c2);
    int64_t g2 = OLIVEC_GREEN(c2);
    int64_t b2 = OLIVEC_BLUE(c2);
    int64_t a2 = OLIVEC_ALPHA(c2);

    int64_t r3 = OLIVEC_RED(c3);
    int64_t g3 = OLIVEC_GREEN(c3);
    int64_t b3 = OLIVEC_BLUE(c3);
    int64_t a3 = OLIVEC_ALPHA(c3);

    if (det != 0) {
        int u3 = det - u1 - u2;
        int64_t r4 = (r1*u1 + r2*u2 + r3*u3)/det;
        int64_t g4 = (g1*u1 + g2*u2 + g3*u3)/det;
        int64_t b4 = (b1*u1 + b2*u2 + b3*u3)/det;
        int64_t a4 = (a1*u1 + a2*u2 + a3*u3)/det;

        return OLIVEC_RGBA(r4, g4, b4, a4);
    }

    return 0;
}

// NOTE: we imply u3 = det - u1 - u2
OLIVECDEF bool olivec_barycentric(int x1, int y1, int x2, int y2, int x3, int y3, int xp, int yp, int *u1, int *u2, int *det)
{
    *det = ((x1 - x3)*(y2 - y3) - (x2 - x3)*(y1 - y3));
    *u1  = ((y2 - y3)*(xp - x3) + (x3 - x2)*(yp - y3));
    *u2  = ((y3 - y1)*(xp - x3) + (x1 - x3)*(yp - y3));
    int u3 = *det - *u1 - *u2;
    return (
               (OLIVEC_SIGN(int, *u1) == OLIVEC_SIGN(int, *det) || *u1 == 0) &&
               (OLIVEC_SIGN(int, *u2) == OLIVEC_SIGN(int, *det) || *u2 == 0) &&
               (OLIVEC_SIGN(int, u3) == OLIVEC_SIGN(int, *det) || u3 == 0)
           );
}

OLIVECDEF bool olivec_normalize_triangle(size_t width, size_t height, int x1, int y1, int x2, int y2, int x3, int y3, int *lx, int *hx, int *ly, int *hy)
{
    *lx = x1;
    *hx = x1;
    if (*lx > x2) *lx = x2;
    if (*lx > x3) *lx = x3;
    if (*hx < x2) *hx = x2;
    if (*hx < x3) *hx = x3;
    if (*lx < 0) *lx = 0;
    if ((size_t) *lx >= width) return false;;
    if (*hx < 0) return false;;
    if ((size_t) *hx >= width) *hx = width-1;

    *ly = y1;
    *hy = y1;
    if (*ly > y2) *ly = y2;
    if (*ly > y3) *ly = y3;
    if (*hy < y2) *hy = y2;
    if (*hy < y3) *hy = y3;
    if (*ly < 0) *ly = 0;
    if ((size_t) *ly >= height) return false;;
    if (*hy < 0) return false;;
    if ((size_t) *hy >= height) *hy = height-1;

    return true;
}

OLIVECDEF void olivec_triangle3c(Olivec_Canvas oc, int x1, int y1, int x2, int y2, int x3, int y3,
                                 uint32_t c1, uint32_t c2, uint32_t c3)
{
    int lx, hx, ly, hy;
    if (olivec_normalize_triangle(oc.width, oc.height, x1, y1, x2, y2, x3, y3, &lx, &hx, &ly, &hy)) {
        for (int y = ly; y <= hy; ++y) {
            for (int x = lx; x <= hx; ++x) {
                int u1, u2, det;
                if (olivec_barycentric(x1, y1, x2, y2, x3, y3, x, y, &u1, &u2, &det)) {
                    olivec_blend_color(&OLIVEC_PIXEL(oc, x, y), mix_colors3(c1, c2, c3, u1, u2, det));
                }
            }
        }
    }
}

OLIVECDEF void olivec_triangle3z(Olivec_Canvas oc, int x1, int y1, int x2, int y2, int x3, int y3, float z1, float z2, float z3)
{
    int lx, hx, ly, hy;
    if (olivec_normalize_triangle(oc.width, oc.height, x1, y1, x2, y2, x3, y3, &lx, &hx, &ly, &hy)) {
        for (int y = ly; y <= hy; ++y) {
            for (int x = lx; x <= hx; ++x) {
                int u1, u2, det;
                if (olivec_barycentric(x1, y1, x2, y2, x3, y3, x, y, &u1, &u2, &det)) {
                    float z = z1*u1/det + z2*u2/det + z3*(det - u1 - u2)/det;
                    OLIVEC_PIXEL(oc, x, y) = *(uint32_t*)&z;
                }
            }
        }
    }
}

OLIVECDEF void olivec_triangle3uv(Olivec_Canvas oc, int x1, int y1, int x2, int y2, int x3, int y3, float tx1, float ty1, float tx2, float ty2, float tx3, float ty3, float z1, float z2, float z3, Olivec_Canvas texture)
{
    int lx, hx, ly, hy;
    if (olivec_normalize_triangle(oc.width, oc.height, x1, y1, x2, y2, x3, y3, &lx, &hx, &ly, &hy)) {
        for (int y = ly; y <= hy; ++y) {
            for (int x = lx; x <= hx; ++x) {
                int u1, u2, det;
                if (olivec_barycentric(x1, y1, x2, y2, x3, y3, x, y, &u1, &u2, &det)) {
                    int u3 = det - u1 - u2;
                    float z = z1*u1/det + z2*u2/det + z3*(det - u1 - u2)/det;
                    float tx = tx1*u1/det + tx2*u2/det + tx3*u3/det;
                    float ty = ty1*u1/det + ty2*u2/det + ty3*u3/det;

                    int texture_x = tx/z*texture.width;
                    if (texture_x < 0) texture_x = 0;
                    if ((size_t) texture_x >= texture.width) texture_x = texture.width - 1;

                    int texture_y = ty/z*texture.height;
                    if (texture_y < 0) texture_y = 0;
                    if ((size_t) texture_y >= texture.height) texture_y = texture.height - 1;
                    OLIVEC_PIXEL(oc, x, y) = OLIVEC_PIXEL(texture, (int)texture_x, (int)texture_y);
                }
            }
        }
    }
}

OLIVECDEF void olivec_triangle3uv_bilinear(Olivec_Canvas oc, int x1, int y1, int x2, int y2, int x3, int y3, float tx1, float ty1, float tx2, float ty2, float tx3, float ty3, float z1, float z2, float z3, Olivec_Canvas texture)
{
    int lx, hx, ly, hy;
    if (olivec_normalize_triangle(oc.width, oc.height, x1, y1, x2, y2, x3, y3, &lx, &hx, &ly, &hy)) {
        for (int y = ly; y <= hy; ++y) {
            for (int x = lx; x <= hx; ++x) {
                int u1, u2, det;
                if (olivec_barycentric(x1, y1, x2, y2, x3, y3, x, y, &u1, &u2, &det)) {
                    int u3 = det - u1 - u2;
                    float z = z1*u1/det + z2*u2/det + z3*(det - u1 - u2)/det;
                    float tx = tx1*u1/det + tx2*u2/det + tx3*u3/det;
                    float ty = ty1*u1/det + ty2*u2/det + ty3*u3/det;

                    float texture_x = tx/z*texture.width;
                    if (texture_x < 0) texture_x = 0;
                    if (texture_x >= (float) texture.width) texture_x = texture.width - 1;

                    float texture_y = ty/z*texture.height;
                    if (texture_y < 0) texture_y = 0;
                    if (texture_y >= (float) texture.height) texture_y = texture.height - 1;

                    int precision = 100;
                    OLIVEC_PIXEL(oc, x, y) = olivec_pixel_bilinear(
                                                 texture,
                                                 texture_x*precision, texture_y*precision,
                                                 precision, precision);
                }
            }
        }
    }
}

// TODO: AA for triangle
OLIVECDEF void olivec_triangle(Olivec_Canvas oc, int x1, int y1, int x2, int y2, int x3, int y3, uint32_t color)
{
    int lx, hx, ly, hy;
    if (olivec_normalize_triangle(oc.width, oc.height, x1, y1, x2, y2, x3, y3, &lx, &hx, &ly, &hy)) {
        for (int y = ly; y <= hy; ++y) {
            for (int x = lx; x <= hx; ++x) {
                int u1, u2, det;
                if (olivec_barycentric(x1, y1, x2, y2, x3, y3, x, y, &u1, &u2, &det)) {
                    olivec_blend_color(&OLIVEC_PIXEL(oc, x, y), color);
                }
            }
        }
    }
}

OLIVECDEF void olivec_text(Olivec_Canvas oc, const char *text, int tx, int ty, Olivec_Font font, size_t glyph_size, uint32_t color)
{
    for (size_t i = 0; *text; ++i, ++text) {
        int gx = tx + i*font.width*glyph_size;
        int gy = ty;
        const char *glyph = &font.glyphs[(*text)*sizeof(char)*font.width*font.height];
        for (int dy = 0; (size_t) dy < font.height; ++dy) {
            for (int dx = 0; (size_t) dx < font.width; ++dx) {
                int px = gx + dx*glyph_size;
                int py = gy + dy*glyph_size;
                if (0 <= px && px < (int) oc.width && 0 <= py && py < (int) oc.height) {
                    if (glyph[dy*font.width + dx]) {
                        olivec_rect(oc, px, py, glyph_size, glyph_size, color);
                    }
                }
            }
        }
    }
}

OLIVECDEF void olivec_sprite_blend(Olivec_Canvas oc, int x, int y, int w, int h, Olivec_Canvas sprite)
{
    if (sprite.width == 0) return;
    if (sprite.height == 0) return;

    Olivec_Normalized_Rect nr = {0};
    if (!olivec_normalize_rect(x, y, w, h, oc.width, oc.height, &nr)) return;

    int xa = nr.ox1;
    if (w < 0) xa = nr.ox2;
    int ya = nr.oy1;
    if (h < 0) ya = nr.oy2;
    for (int y = nr.y1; y <= nr.y2; ++y) {
        for (int x = nr.x1; x <= nr.x2; ++x) {
            size_t nx = (x - xa)*((int) sprite.width)/w;
            size_t ny = (y - ya)*((int) sprite.height)/h;
            olivec_blend_color(&OLIVEC_PIXEL(oc, x, y), OLIVEC_PIXEL(sprite, nx, ny));
        }
    }
}

OLIVECDEF void olivec_sprite_copy(Olivec_Canvas oc, int x, int y, int w, int h, Olivec_Canvas sprite)
{
    if (sprite.width == 0) return;
    if (sprite.height == 0) return;

    // TODO: consider introducing flip parameter instead of relying on negative width and height
    // Similar to how SDL_RenderCopyEx does that
    Olivec_Normalized_Rect nr = {0};
    if (!olivec_normalize_rect(x, y, w, h, oc.width, oc.height, &nr)) return;

    int xa = nr.ox1;
    if (w < 0) xa = nr.ox2;
    int ya = nr.oy1;
    if (h < 0) ya = nr.oy2;
    for (int y = nr.y1; y <= nr.y2; ++y) {
        for (int x = nr.x1; x <= nr.x2; ++x) {
            size_t nx = (x - xa)*((int) sprite.width)/w;
            size_t ny = (y - ya)*((int) sprite.height)/h;
            OLIVEC_PIXEL(oc, x, y) = OLIVEC_PIXEL(sprite, nx, ny);
        }
    }
}

// TODO: olivec_pixel_bilinear does not check for out-of-bounds
// But maybe it shouldn't. Maybe it's a responsibility of the caller of the function.
OLIVECDEF uint32_t olivec_pixel_bilinear(Olivec_Canvas sprite, int nx, int ny, int w, int h)
{
    int px = nx%w;
    int py = ny%h;

    int x1 = nx/w, x2 = nx/w;
    int y1 = ny/h, y2 = ny/h;
    if (px < w/2) {
        // left
        px += w/2;
        x1 -= 1;
        if (x1 < 0) x1 = 0;
    } else {
        // right
        px -= w/2;
        x2 += 1;
        if ((size_t) x2 >= sprite.width) x2 = sprite.width - 1;
    }

    if (py < h/2) {
        // top
        py += h/2;
        y1 -= 1;
        if (y1 < 0) y1 = 0;
    } else {
        // bottom
        py -= h/2;
        y2 += 1;
        if ((size_t) y2 >= sprite.height) y2 = sprite.height - 1;
    }

    return mix_colors2(mix_colors2(OLIVEC_PIXEL(sprite, x1, y1),
                                   OLIVEC_PIXEL(sprite, x2, y1),
                                   px, w),
                       mix_colors2(OLIVEC_PIXEL(sprite, x1, y2),
                                   OLIVEC_PIXEL(sprite, x2, y2),
                                   px, w),
                       py, h);
}

OLIVECDEF void olivec_sprite_copy_bilinear(Olivec_Canvas oc, int x, int y, int w, int h, Olivec_Canvas sprite)
{
    // TODO: support negative size in olivec_sprite_copy_bilinear()
    if (w <= 0) return;
    if (h <= 0) return;

    Olivec_Normalized_Rect nr = {0};
    if (!olivec_normalize_rect(x, y, w, h, oc.width, oc.height, &nr)) return;

    for (int y = nr.y1; y <= nr.y2; ++y) {
        for (int x = nr.x1; x <= nr.x2; ++x) {
            size_t nx = (x - nr.ox1)*sprite.width;
            size_t ny = (y - nr.oy1)*sprite.height;
            OLIVEC_PIXEL(oc, x, y) = olivec_pixel_bilinear(sprite, nx, ny, w, h);
        }
    }
}

#endif // OLIVEC_IMPLEMENTATION

// TODO: Benchmarking
// TODO: SIMD implementations
// TODO: bezier curves
// TODO: olivec_ring
// TODO: fuzzer
// TODO: Stencil

/* ======== Test Harness ======== */

static uint32_t canvas_checksum(Olivec_Canvas oc) {
    uint32_t sum = 0;
    for (size_t y = 0; y < oc.height; y++)
        for (size_t x = 0; x < oc.width; x++)
            sum = sum * 31 + OLIVEC_PIXEL(oc, x, y);
    return sum;
}

static Olivec_Canvas make_canvas(size_t w, size_t h) {
    uint32_t *pixels = (uint32_t *)calloc(w * h, sizeof(uint32_t));
    return olivec_canvas(pixels, w, h, w);
}

static void free_canvas(Olivec_Canvas oc) {
    free(oc.pixels);
}

static void test_fill(void) {
    printf("=== test_fill ===\n");
    Olivec_Canvas oc = make_canvas(16, 16);
    olivec_fill(oc, 0xFF0000FF);
    printf("fill_ck: 0x%08X\n", canvas_checksum(oc));
    printf("fill_px00: 0x%08X\n", OLIVEC_PIXEL(oc, 0, 0));
    printf("fill_px15: 0x%08X\n", OLIVEC_PIXEL(oc, 15, 15));
    free_canvas(oc);
}

static void test_rect(void) {
    printf("=== test_rect ===\n");
    Olivec_Canvas oc = make_canvas(32, 32);
    olivec_fill(oc, 0xFF000000);
    olivec_rect(oc, 4, 4, 16, 16, 0xFF0000FF);
    printf("rect_ck: 0x%08X\n", canvas_checksum(oc));
    printf("rect_in: 0x%08X\n", OLIVEC_PIXEL(oc, 10, 10));
    printf("rect_out: 0x%08X\n", OLIVEC_PIXEL(oc, 0, 0));
    free_canvas(oc);
}

static void test_frame(void) {
    printf("=== test_frame ===\n");
    Olivec_Canvas oc = make_canvas(32, 32);
    olivec_fill(oc, 0xFF000000);
    olivec_frame(oc, 4, 4, 24, 24, 2, 0xFF0000FF);
    printf("frame_ck: 0x%08X\n", canvas_checksum(oc));
    free_canvas(oc);
}

static void test_circle(void) {
    printf("=== test_circle ===\n");
    Olivec_Canvas oc = make_canvas(32, 32);
    olivec_fill(oc, 0xFF000000);
    olivec_circle(oc, 16, 16, 10, 0xFFFFFFFF);
    printf("circle_ck: 0x%08X\n", canvas_checksum(oc));
    printf("circle_center: 0x%08X\n", OLIVEC_PIXEL(oc, 16, 16));
    free_canvas(oc);
}

static void test_ellipse(void) {
    printf("=== test_ellipse ===\n");
    Olivec_Canvas oc = make_canvas(32, 32);
    olivec_fill(oc, 0xFF000000);
    olivec_ellipse(oc, 16, 16, 12, 8, 0xFF00FF00);
    printf("ellipse_ck: 0x%08X\n", canvas_checksum(oc));
    printf("ellipse_center: 0x%08X\n", OLIVEC_PIXEL(oc, 16, 16));
    free_canvas(oc);
}

static void test_line(void) {
    printf("=== test_line ===\n");
    Olivec_Canvas oc = make_canvas(32, 32);
    olivec_fill(oc, 0xFF000000);
    olivec_line(oc, 2, 2, 28, 28, 0xFFFFFFFF);
    printf("line_ck: 0x%08X\n", canvas_checksum(oc));
    printf("line_start: 0x%08X\n", OLIVEC_PIXEL(oc, 2, 2));
    printf("line_end: 0x%08X\n", OLIVEC_PIXEL(oc, 28, 28));
    /* horizontal line */
    Olivec_Canvas oc2 = make_canvas(32, 32);
    olivec_fill(oc2, 0xFF000000);
    olivec_line(oc2, 0, 16, 31, 16, 0xFFFF0000);
    printf("hline_ck: 0x%08X\n", canvas_checksum(oc2));
    free_canvas(oc);
    free_canvas(oc2);
}

static void test_triangle(void) {
    printf("=== test_triangle ===\n");
    Olivec_Canvas oc = make_canvas(32, 32);
    olivec_fill(oc, 0xFF000000);
    olivec_triangle(oc, 2, 2, 28, 5, 15, 28, 0xFFFFFFFF);
    printf("tri_ck: 0x%08X\n", canvas_checksum(oc));
    free_canvas(oc);
}

static void test_triangle3c(void) {
    printf("=== test_triangle3c ===\n");
    Olivec_Canvas oc = make_canvas(32, 32);
    olivec_fill(oc, 0xFF000000);
    olivec_triangle3c(oc, 2, 2, 28, 5, 15, 28,
                      OLIVEC_RGBA(255, 0, 0, 255),
                      OLIVEC_RGBA(0, 255, 0, 255),
                      OLIVEC_RGBA(0, 0, 255, 255));
    printf("tri3c_ck: 0x%08X\n", canvas_checksum(oc));
    free_canvas(oc);
}

static void test_triangle3z(void) {
    printf("=== test_triangle3z ===\n");
    Olivec_Canvas oc = make_canvas(32, 32);
    memset(oc.pixels, 0, 32 * 32 * sizeof(uint32_t));
    olivec_triangle3z(oc, 2, 2, 28, 5, 15, 28, 0.1f, 0.5f, 0.9f);
    printf("tri3z_ck: 0x%08X\n", canvas_checksum(oc));
    printf("tri3z_px: 0x%08X\n", OLIVEC_PIXEL(oc, 15, 15));
    free_canvas(oc);
}

static void test_triangle3uv(void) {
    printf("=== test_triangle3uv ===\n");
    Olivec_Canvas oc = make_canvas(32, 32);
    olivec_fill(oc, 0xFF000000);

    uint32_t tex_pixels[16];
    for (int i = 0; i < 16; i++) {
        uint32_t r = (i % 4) * 85;
        uint32_t g = (i / 4) * 85;
        tex_pixels[i] = OLIVEC_RGBA(r, g, 0, 255);
    }
    Olivec_Canvas texture = olivec_canvas(tex_pixels, 4, 4, 4);

    olivec_triangle3uv(oc, 2, 2, 28, 5, 15, 28,
                       0.0f, 0.0f, 1.0f, 0.0f, 0.5f, 1.0f,
                       1.0f, 1.0f, 1.0f, texture);
    printf("tri3uv_ck: 0x%08X\n", canvas_checksum(oc));
    free_canvas(oc);
}

static void test_triangle3uv_bilinear(void) {
    printf("=== test_triangle3uv_bilinear ===\n");
    Olivec_Canvas oc = make_canvas(32, 32);
    olivec_fill(oc, 0xFF000000);

    uint32_t tex_pixels[16];
    for (int i = 0; i < 16; i++) {
        uint32_t r = (i % 4) * 85;
        uint32_t g = (i / 4) * 85;
        tex_pixels[i] = OLIVEC_RGBA(r, g, 0, 255);
    }
    Olivec_Canvas texture = olivec_canvas(tex_pixels, 4, 4, 4);

    olivec_triangle3uv_bilinear(oc, 2, 2, 28, 5, 15, 28,
                                0.0f, 0.0f, 1.0f, 0.0f, 0.5f, 1.0f,
                                1.0f, 1.0f, 1.0f, texture);
    printf("tri3uv_bi_ck: 0x%08X\n", canvas_checksum(oc));
    free_canvas(oc);
}

static void test_blend_color(void) {
    printf("=== test_blend_color ===\n");
    /* blend semi-transparent red onto opaque black */
    uint32_t c1 = 0xFF000000;
    olivec_blend_color(&c1, OLIVEC_RGBA(255, 0, 0, 128));
    printf("blend_semi: 0x%08X\n", c1);

    /* blend fully opaque green onto opaque black */
    uint32_t c2 = 0xFF000000;
    olivec_blend_color(&c2, OLIVEC_RGBA(0, 255, 0, 255));
    printf("blend_opaque: 0x%08X\n", c2);

    /* blend fully transparent onto color */
    uint32_t c3 = OLIVEC_RGBA(100, 100, 100, 255);
    olivec_blend_color(&c3, OLIVEC_RGBA(200, 50, 50, 0));
    printf("blend_trans: 0x%08X\n", c3);
}

static void test_mix_colors(void) {
    printf("=== test_mix_colors ===\n");
    uint32_t red   = OLIVEC_RGBA(255, 0, 0, 255);
    uint32_t blue  = OLIVEC_RGBA(0, 0, 255, 255);
    uint32_t green = OLIVEC_RGBA(0, 255, 0, 255);

    printf("mix2_half: 0x%08X\n", mix_colors2(red, blue, 1, 2));
    printf("mix2_full: 0x%08X\n", mix_colors2(red, blue, 0, 2));
    printf("mix2_zero: 0x%08X\n", mix_colors2(red, blue, 2, 2));
    printf("mix3_eq: 0x%08X\n", mix_colors3(red, green, blue, 1, 1, 3));
    printf("mix2_det0: 0x%08X\n", mix_colors2(red, blue, 0, 0));
    printf("mix3_det0: 0x%08X\n", mix_colors3(red, green, blue, 0, 0, 0));
}

static void test_text(void) {
    printf("=== test_text ===\n");
    Olivec_Canvas oc = make_canvas(64, 16);
    olivec_fill(oc, 0xFF000000);
    olivec_text(oc, "hello", 2, 2, olivec_default_font, 1, 0xFFFFFFFF);
    printf("text_ck: 0x%08X\n", canvas_checksum(oc));
    /* text with larger glyph size */
    Olivec_Canvas oc2 = make_canvas(64, 32);
    olivec_fill(oc2, 0xFF000000);
    olivec_text(oc2, "42", 2, 2, olivec_default_font, 2, 0xFF0000FF);
    printf("text2_ck: 0x%08X\n", canvas_checksum(oc2));
    free_canvas(oc);
    free_canvas(oc2);
}

static void test_sprite_blend(void) {
    printf("=== test_sprite_blend ===\n");
    Olivec_Canvas oc = make_canvas(16, 16);
    olivec_fill(oc, 0xFF000000);

    uint32_t spr_pixels[16];
    for (int i = 0; i < 16; i++) {
        uint32_t r = (i % 4) * 85;
        uint32_t g = (i / 4) * 85;
        spr_pixels[i] = OLIVEC_RGBA(r, g, 128, 128);
    }
    Olivec_Canvas sprite = olivec_canvas(spr_pixels, 4, 4, 4);
    olivec_sprite_blend(oc, 2, 2, 8, 8, sprite);
    printf("spblend_ck: 0x%08X\n", canvas_checksum(oc));
    printf("spblend_px: 0x%08X\n", OLIVEC_PIXEL(oc, 4, 4));
    free_canvas(oc);
}

static void test_sprite_copy(void) {
    printf("=== test_sprite_copy ===\n");
    Olivec_Canvas oc = make_canvas(16, 16);
    olivec_fill(oc, 0xFF000000);

    uint32_t spr_pixels[16];
    for (int i = 0; i < 16; i++) {
        uint32_t r = (i % 4) * 85;
        uint32_t g = (i / 4) * 85;
        spr_pixels[i] = OLIVEC_RGBA(r, g, 0, 255);
    }
    Olivec_Canvas sprite = olivec_canvas(spr_pixels, 4, 4, 4);
    olivec_sprite_copy(oc, 2, 2, 8, 8, sprite);
    printf("spcopy_ck: 0x%08X\n", canvas_checksum(oc));
    printf("spcopy_px: 0x%08X\n", OLIVEC_PIXEL(oc, 4, 4));
    free_canvas(oc);
}

static void test_sprite_copy_bilinear(void) {
    printf("=== test_sprite_copy_bilinear ===\n");
    Olivec_Canvas oc = make_canvas(16, 16);
    olivec_fill(oc, 0xFF000000);

    uint32_t spr_pixels[16];
    for (int i = 0; i < 16; i++) {
        uint32_t r = (i % 4) * 85;
        uint32_t g = (i / 4) * 85;
        spr_pixels[i] = OLIVEC_RGBA(r, g, 0, 255);
    }
    Olivec_Canvas sprite = olivec_canvas(spr_pixels, 4, 4, 4);
    olivec_sprite_copy_bilinear(oc, 2, 2, 8, 8, sprite);
    printf("spbilin_ck: 0x%08X\n", canvas_checksum(oc));
    printf("spbilin_px: 0x%08X\n", OLIVEC_PIXEL(oc, 4, 4));
    free_canvas(oc);
}

static void test_subcanvas(void) {
    printf("=== test_subcanvas ===\n");
    Olivec_Canvas parent = make_canvas(32, 32);
    olivec_fill(parent, 0xFF000000);

    Olivec_Canvas sub = olivec_subcanvas(parent, 8, 8, 16, 16);
    olivec_fill(sub, 0xFF0000FF);

    printf("sub_ck: 0x%08X\n", canvas_checksum(parent));
    printf("sub_px00: 0x%08X\n", OLIVEC_PIXEL(parent, 0, 0));
    printf("sub_px88: 0x%08X\n", OLIVEC_PIXEL(parent, 8, 8));
    printf("sub_px23: 0x%08X\n", OLIVEC_PIXEL(parent, 23, 23));
    printf("sub_px24: 0x%08X\n", OLIVEC_PIXEL(parent, 24, 24));

    /* draw rect on subcanvas */
    Olivec_Canvas sub2 = olivec_subcanvas(parent, 12, 12, 8, 8);
    olivec_rect(sub2, 1, 1, 6, 6, 0xFF00FF00);
    printf("sub2_ck: 0x%08X\n", canvas_checksum(parent));
    printf("sub2_px1313: 0x%08X\n", OLIVEC_PIXEL(parent, 13, 13));
    free_canvas(parent);
}

static void test_normalize_rect(void) {
    printf("=== test_normalize_rect ===\n");
    Olivec_Normalized_Rect nr = {0};

    /* normal rect */
    int ok = olivec_normalize_rect(5, 5, 10, 10, 32, 32, &nr);
    printf("nr_ok: %d\n", ok);
    printf("nr_x1: %d\n", nr.x1);
    printf("nr_y1: %d\n", nr.y1);
    printf("nr_x2: %d\n", nr.x2);
    printf("nr_y2: %d\n", nr.y2);

    /* negative width/height */
    Olivec_Normalized_Rect nr2 = {0};
    int ok2 = olivec_normalize_rect(15, 15, -10, -10, 32, 32, &nr2);
    printf("nr2_ok: %d\n", ok2);
    printf("nr2_x1: %d\n", nr2.x1);
    printf("nr2_y1: %d\n", nr2.y1);
    printf("nr2_x2: %d\n", nr2.x2);
    printf("nr2_y2: %d\n", nr2.y2);

    /* out of bounds */
    Olivec_Normalized_Rect nr3 = {0};
    int ok3 = olivec_normalize_rect(40, 40, 10, 10, 32, 32, &nr3);
    printf("nr3_ok: %d\n", ok3);

    /* empty rect */
    Olivec_Normalized_Rect nr4 = {0};
    int ok4 = olivec_normalize_rect(5, 5, 0, 10, 32, 32, &nr4);
    printf("nr4_ok: %d\n", ok4);

    /* clipped rect */
    Olivec_Normalized_Rect nr5 = {0};
    int ok5 = olivec_normalize_rect(-5, -5, 20, 20, 32, 32, &nr5);
    printf("nr5_ok: %d\n", ok5);
    printf("nr5_x1: %d\n", nr5.x1);
    printf("nr5_y1: %d\n", nr5.y1);
    printf("nr5_x2: %d\n", nr5.x2);
    printf("nr5_y2: %d\n", nr5.y2);
}

static void test_in_bounds(void) {
    printf("=== test_in_bounds ===\n");
    Olivec_Canvas oc = make_canvas(10, 10);
    printf("ib_00: %d\n", olivec_in_bounds(oc, 0, 0));
    printf("ib_99: %d\n", olivec_in_bounds(oc, 9, 9));
    printf("ib_neg: %d\n", olivec_in_bounds(oc, -1, 0));
    printf("ib_oob: %d\n", olivec_in_bounds(oc, 10, 10));
    printf("ib_edge: %d\n", olivec_in_bounds(oc, 10, 0));
    free_canvas(oc);
}

static void test_barycentric(void) {
    printf("=== test_barycentric ===\n");
    int u1, u2, det;
    int inside = olivec_barycentric(0, 0, 10, 0, 5, 10, 5, 3, &u1, &u2, &det);
    printf("bary_in: %d\n", inside);
    printf("bary_u1: %d\n", u1);
    printf("bary_u2: %d\n", u2);
    printf("bary_det: %d\n", det);

    int outside = olivec_barycentric(0, 0, 10, 0, 5, 10, 20, 20, &u1, &u2, &det);
    printf("bary_out: %d\n", outside);
}

static void test_normalize_triangle(void) {
    printf("=== test_normalize_triangle ===\n");
    int lx, hx, ly, hy;
    int ok = olivec_normalize_triangle(32, 32, 5, 5, 25, 10, 15, 25, &lx, &hx, &ly, &hy);
    printf("ntri_ok: %d\n", ok);
    printf("ntri_lx: %d\n", lx);
    printf("ntri_hx: %d\n", hx);
    printf("ntri_ly: %d\n", ly);
    printf("ntri_hy: %d\n", hy);

    /* triangle fully outside */
    int ok2 = olivec_normalize_triangle(32, 32, 40, 40, 50, 45, 45, 50, &lx, &hx, &ly, &hy);
    printf("ntri2_ok: %d\n", ok2);
}

static void test_edge_cases(void) {
    printf("=== test_edge_cases ===\n");
    Olivec_Canvas oc = make_canvas(8, 8);
    olivec_fill(oc, 0xFF000000);

    /* empty rect (w=0) — should be no-op */
    olivec_rect(oc, 2, 2, 0, 4, 0xFF0000FF);
    /* zero-length line (single pixel) */
    olivec_line(oc, 4, 4, 4, 4, 0xFF00FF00);
    /* fully out-of-bounds rect */
    olivec_rect(oc, 20, 20, 4, 4, 0xFFFF0000);
    /* partially out-of-bounds line */
    olivec_line(oc, -5, 0, 12, 7, 0xFFFFFFFF);
    /* frame with zero thickness — should be no-op */
    olivec_frame(oc, 1, 1, 6, 6, 0, 0xFFFF0000);

    printf("edge_ck: 0x%08X\n", canvas_checksum(oc));
    printf("edge_px44: 0x%08X\n", OLIVEC_PIXEL(oc, 4, 4));
    free_canvas(oc);
}

int main(void) {
    test_fill();
    test_rect();
    test_frame();
    test_circle();
    test_ellipse();
    test_line();
    test_triangle();
    test_triangle3c();
    test_triangle3z();
    test_triangle3uv();
    test_triangle3uv_bilinear();
    test_blend_color();
    test_mix_colors();
    test_text();
    test_sprite_blend();
    test_sprite_copy();
    test_sprite_copy_bilinear();
    test_subcanvas();
    test_normalize_rect();
    test_in_bounds();
    test_barycentric();
    test_normalize_triangle();
    test_edge_cases();
    printf("olive_done\n");
    return 0;
}
