// olive_migrated.rs — Idiomatic Rust migration of tsoding/olive.c
// Source: https://github.com/tsoding/olive.c (MIT License, Alexey Kutepov)
// Compile: rustc --edition 2021 olive_migrated.rs -o olive_test_rs

const AA_RES: i32 = 2;
const DEFAULT_FONT_HEIGHT: usize = 6;
const DEFAULT_FONT_WIDTH: usize = 6;

fn olivec_rgba(r: u32, g: u32, b: u32, a: u32) -> u32 {
    ((r & 0xFF) << 0) | ((g & 0xFF) << 8) | ((b & 0xFF) << 16) | ((a & 0xFF) << 24)
}

fn olivec_red(color: u32) -> u32 {
    (color & 0x000000FF) >> 0
}

fn olivec_green(color: u32) -> u32 {
    (color & 0x0000FF00) >> 8
}

fn olivec_blue(color: u32) -> u32 {
    (color & 0x00FF0000) >> 16
}

fn olivec_alpha(color: u32) -> u32 {
    (color & 0xFF000000) >> 24
}

struct Canvas {
    pixels: Vec<u32>,
    width: usize,
    height: usize,
    stride: usize,
}

impl Canvas {
    fn new(width: usize, height: usize) -> Self {
        Self {
            pixels: vec![0u32; width * height],
            width,
            height,
            stride: width,
        }
    }

    fn pixel(&self, x: usize, y: usize) -> u32 {
        self.pixels[y * self.stride + x]
    }

    fn set_pixel(&mut self, x: usize, y: usize, val: u32) {
        self.pixels[y * self.stride + x] = val;
    }

    fn in_bounds(&self, x: i32, y: i32) -> bool {
        0 <= x && (x as usize) < self.width && 0 <= y && (y as usize) < self.height
    }

    fn checksum(&self) -> u32 {
        let mut sum: u32 = 0;
        for y in 0..self.height {
            for x in 0..self.width {
                sum = sum.wrapping_mul(31).wrapping_add(self.pixel(x, y));
            }
        }
        sum
    }
}

struct Font {
    glyphs: &'static [[[u8; DEFAULT_FONT_WIDTH]; DEFAULT_FONT_HEIGHT]; 128],
    width: usize,
    height: usize,
}

#[derive(Default)]
struct NormalizedRect {
    x1: i32,
    x2: i32,
    y1: i32,
    y2: i32,
    ox1: i32,
    ox2: i32,
    oy1: i32,
    oy2: i32,
}

fn signum(x: i32) -> i32 {
    if x > 0 { 1 } else if x < 0 { -1 } else { 0 }
}

fn normalize_rect(
    x: i32, y: i32, w: i32, h: i32,
    canvas_width: usize, canvas_height: usize,
) -> Option<NormalizedRect> {
    if w == 0 || h == 0 {
        return None;
    }

    let mut nr = NormalizedRect::default();
    nr.ox1 = x;
    nr.oy1 = y;

    nr.ox2 = nr.ox1 + signum(w) * (w.abs() - 1);
    if nr.ox1 > nr.ox2 {
        std::mem::swap(&mut nr.ox1, &mut nr.ox2);
    }
    nr.oy2 = nr.oy1 + signum(h) * (h.abs() - 1);
    if nr.oy1 > nr.oy2 {
        std::mem::swap(&mut nr.oy1, &mut nr.oy2);
    }

    if nr.ox1 >= canvas_width as i32 { return None; }
    if nr.ox2 < 0 { return None; }
    if nr.oy1 >= canvas_height as i32 { return None; }
    if nr.oy2 < 0 { return None; }

    nr.x1 = nr.ox1;
    nr.y1 = nr.oy1;
    nr.x2 = nr.ox2;
    nr.y2 = nr.oy2;

    if nr.x1 < 0 { nr.x1 = 0; }
    if nr.x2 >= canvas_width as i32 { nr.x2 = canvas_width as i32 - 1; }
    if nr.y1 < 0 { nr.y1 = 0; }
    if nr.y2 >= canvas_height as i32 { nr.y2 = canvas_height as i32 - 1; }

    Some(nr)
}

fn blend_color(c1: &mut u32, c2: u32) {
    let mut r1 = olivec_red(*c1);
    let mut g1 = olivec_green(*c1);
    let mut b1 = olivec_blue(*c1);
    let a1 = olivec_alpha(*c1);

    let r2 = olivec_red(c2);
    let g2 = olivec_green(c2);
    let b2 = olivec_blue(c2);
    let a2 = olivec_alpha(c2);

    r1 = (r1 * (255 - a2) + r2 * a2) / 255;
    if r1 > 255 { r1 = 255; }
    g1 = (g1 * (255 - a2) + g2 * a2) / 255;
    if g1 > 255 { g1 = 255; }
    b1 = (b1 * (255 - a2) + b2 * a2) / 255;
    if b1 > 255 { b1 = 255; }

    *c1 = olivec_rgba(r1, g1, b1, a1);
}

fn olivec_fill(oc: &mut Canvas, color: u32) {
    for y in 0..oc.height {
        for x in 0..oc.width {
            oc.set_pixel(x, y, color);
        }
    }
}

fn olivec_rect(oc: &mut Canvas, x: i32, y: i32, w: i32, h: i32, color: u32) {
    let nr = match normalize_rect(x, y, w, h, oc.width, oc.height) {
        Some(nr) => nr,
        None => return,
    };
    for ix in nr.x1..=nr.x2 {
        for iy in nr.y1..=nr.y2 {
            let idx = iy as usize * oc.stride + ix as usize;
            blend_color(&mut oc.pixels[idx], color);
        }
    }
}

fn olivec_frame(oc: &mut Canvas, x: i32, y: i32, w: i32, h: i32, t: usize, color: u32) {
    if t == 0 { return; }

    let mut x1 = x;
    let mut y1 = y;
    let mut x2 = x1 + signum(w) * (w.abs() - 1);
    if x1 > x2 { std::mem::swap(&mut x1, &mut x2); }
    let mut y2 = y1 + signum(h) * (h.abs() - 1);
    if y1 > y2 { std::mem::swap(&mut y1, &mut y2); }

    let t2 = (t / 2) as i32;
    // Top
    olivec_rect(oc, x1 - t2, y1 - t2, (x2 - x1 + 1) + t2 * 2, t as i32, color);
    // Left
    olivec_rect(oc, x1 - t2, y1 - t2, t as i32, (y2 - y1 + 1) + t2 * 2, color);
    // Bottom
    olivec_rect(oc, x1 - t2, y2 + t2, (x2 - x1 + 1) + t2 * 2, -(t as i32), color);
    // Right
    olivec_rect(oc, x2 + t2, y1 - t2, -(t as i32), (y2 - y1 + 1) + t2 * 2, color);
}

fn olivec_ellipse(oc: &mut Canvas, cx: i32, cy: i32, rx: i32, ry: i32, color: u32) {
    let rx1 = rx + signum(rx);
    let ry1 = ry + signum(ry);
    let nr = match normalize_rect(cx - rx1, cy - ry1, 2 * rx1, 2 * ry1, oc.width, oc.height) {
        Some(nr) => nr,
        None => return,
    };

    for y in nr.y1..=nr.y2 {
        for x in nr.x1..=nr.x2 {
            let nx = (x as f32 + 0.5 - nr.x1 as f32) / (2.0 * rx1 as f32);
            let ny = (y as f32 + 0.5 - nr.y1 as f32) / (2.0 * ry1 as f32);
            let dx = nx - 0.5;
            let dy = ny - 0.5;
            if dx * dx + dy * dy <= 0.5 * 0.5 {
                oc.set_pixel(x as usize, y as usize, color);
            }
        }
    }
}

fn olivec_circle(oc: &mut Canvas, cx: i32, cy: i32, r: i32, color: u32) {
    let r1 = r + signum(r);
    let nr = match normalize_rect(cx - r1, cy - r1, 2 * r1, 2 * r1, oc.width, oc.height) {
        Some(nr) => nr,
        None => return,
    };

    for y in nr.y1..=nr.y2 {
        for x in nr.x1..=nr.x2 {
            let mut count = 0i32;
            for sox in 0..AA_RES {
                for soy in 0..AA_RES {
                    let res1 = AA_RES + 1;
                    let dx = x * res1 * 2 + 2 + sox * 2 - res1 * cx * 2 - res1;
                    let dy = y * res1 * 2 + 2 + soy * 2 - res1 * cy * 2 - res1;
                    if dx * dx + dy * dy <= res1 * res1 * r * r * 2 * 2 {
                        count += 1;
                    }
                }
            }
            let alpha = ((color & 0xFF000000) >> 24) as i32 * count / AA_RES / AA_RES;
            let updated_color = (color & 0x00FFFFFF) | ((alpha as u32) << 24);
            let idx = y as usize * oc.stride + x as usize;
            blend_color(&mut oc.pixels[idx], updated_color);
        }
    }
}

fn olivec_line(oc: &mut Canvas, mut x1: i32, mut y1: i32, mut x2: i32, mut y2: i32, color: u32) {
    let dx = x2 - x1;
    let dy = y2 - y1;

    if dx == 0 && dy == 0 {
        if oc.in_bounds(x1, y1) {
            let idx = y1 as usize * oc.stride + x1 as usize;
            blend_color(&mut oc.pixels[idx], color);
        }
        return;
    }

    if dx.abs() > dy.abs() {
        if x1 > x2 {
            std::mem::swap(&mut x1, &mut x2);
            std::mem::swap(&mut y1, &mut y2);
        }
        for x in x1..=x2 {
            let y = dy * (x - x1) / dx + y1;
            if oc.in_bounds(x, y) {
                let idx = y as usize * oc.stride + x as usize;
                blend_color(&mut oc.pixels[idx], color);
            }
        }
    } else {
        if y1 > y2 {
            std::mem::swap(&mut x1, &mut x2);
            std::mem::swap(&mut y1, &mut y2);
        }
        for y in y1..=y2 {
            let x = dx * (y - y1) / dy + x1;
            if oc.in_bounds(x, y) {
                let idx = y as usize * oc.stride + x as usize;
                blend_color(&mut oc.pixels[idx], color);
            }
        }
    }
}

fn mix_colors2(c1: u32, c2: u32, u1: i32, det: i32) -> u32 {
    let r1 = olivec_red(c1) as i64;
    let g1 = olivec_green(c1) as i64;
    let b1 = olivec_blue(c1) as i64;
    let a1 = olivec_alpha(c1) as i64;

    let r2 = olivec_red(c2) as i64;
    let g2 = olivec_green(c2) as i64;
    let b2 = olivec_blue(c2) as i64;
    let a2 = olivec_alpha(c2) as i64;

    if det != 0 {
        let u2 = det as i64 - u1 as i64;
        let u1 = u1 as i64;
        let det = det as i64;
        let r4 = (r1 * u2 + r2 * u1) / det;
        let g4 = (g1 * u2 + g2 * u1) / det;
        let b4 = (b1 * u2 + b2 * u1) / det;
        let a4 = (a1 * u2 + a2 * u1) / det;
        return olivec_rgba(r4 as u32, g4 as u32, b4 as u32, a4 as u32);
    }
    0
}

fn mix_colors3(c1: u32, c2: u32, c3: u32, u1: i32, u2: i32, det: i32) -> u32 {
    let r1 = olivec_red(c1) as i64;
    let g1 = olivec_green(c1) as i64;
    let b1 = olivec_blue(c1) as i64;
    let a1 = olivec_alpha(c1) as i64;

    let r2 = olivec_red(c2) as i64;
    let g2 = olivec_green(c2) as i64;
    let b2 = olivec_blue(c2) as i64;
    let a2 = olivec_alpha(c2) as i64;

    let r3 = olivec_red(c3) as i64;
    let g3 = olivec_green(c3) as i64;
    let b3 = olivec_blue(c3) as i64;
    let a3 = olivec_alpha(c3) as i64;

    if det != 0 {
        let u1i = u1 as i64;
        let u2i = u2 as i64;
        let deti = det as i64;
        let u3 = deti - u1i - u2i;
        let r4 = (r1 * u1i + r2 * u2i + r3 * u3) / deti;
        let g4 = (g1 * u1i + g2 * u2i + g3 * u3) / deti;
        let b4 = (b1 * u1i + b2 * u2i + b3 * u3) / deti;
        let a4 = (a1 * u1i + a2 * u2i + a3 * u3) / deti;
        return olivec_rgba(r4 as u32, g4 as u32, b4 as u32, a4 as u32);
    }
    0
}

fn olivec_barycentric(
    x1: i32, y1: i32, x2: i32, y2: i32, x3: i32, y3: i32,
    xp: i32, yp: i32,
) -> Option<(i32, i32, i32)> {
    let det = (x1 - x3) * (y2 - y3) - (x2 - x3) * (y1 - y3);
    let u1 = (y2 - y3) * (xp - x3) + (x3 - x2) * (yp - y3);
    let u2 = (y3 - y1) * (xp - x3) + (x1 - x3) * (yp - y3);
    let u3 = det - u1 - u2;
    if (signum(u1) == signum(det) || u1 == 0)
        && (signum(u2) == signum(det) || u2 == 0)
        && (signum(u3) == signum(det) || u3 == 0)
    {
        Some((u1, u2, det))
    } else {
        None
    }
}

fn olivec_normalize_triangle(
    width: usize, height: usize,
    x1: i32, y1: i32, x2: i32, y2: i32, x3: i32, y3: i32,
) -> Option<(i32, i32, i32, i32)> {
    let mut lx = x1;
    let mut hx = x1;
    if lx > x2 { lx = x2; }
    if lx > x3 { lx = x3; }
    if hx < x2 { hx = x2; }
    if hx < x3 { hx = x3; }
    if lx < 0 { lx = 0; }
    if lx as usize >= width { return None; }
    if hx < 0 { return None; }
    if hx as usize >= width { hx = width as i32 - 1; }

    let mut ly = y1;
    let mut hy = y1;
    if ly > y2 { ly = y2; }
    if ly > y3 { ly = y3; }
    if hy < y2 { hy = y2; }
    if hy < y3 { hy = y3; }
    if ly < 0 { ly = 0; }
    if ly as usize >= height { return None; }
    if hy < 0 { return None; }
    if hy as usize >= height { hy = height as i32 - 1; }

    Some((lx, hx, ly, hy))
}

fn olivec_triangle(oc: &mut Canvas, x1: i32, y1: i32, x2: i32, y2: i32, x3: i32, y3: i32, color: u32) {
    if let Some((lx, hx, ly, hy)) = olivec_normalize_triangle(oc.width, oc.height, x1, y1, x2, y2, x3, y3) {
        for y in ly..=hy {
            for x in lx..=hx {
                if olivec_barycentric(x1, y1, x2, y2, x3, y3, x, y).is_some() {
                    let idx = y as usize * oc.stride + x as usize;
                    blend_color(&mut oc.pixels[idx], color);
                }
            }
        }
    }
}

fn olivec_triangle3c(
    oc: &mut Canvas,
    x1: i32, y1: i32, x2: i32, y2: i32, x3: i32, y3: i32,
    c1: u32, c2: u32, c3: u32,
) {
    if let Some((lx, hx, ly, hy)) = olivec_normalize_triangle(oc.width, oc.height, x1, y1, x2, y2, x3, y3) {
        for y in ly..=hy {
            for x in lx..=hx {
                if let Some((u1, u2, det)) = olivec_barycentric(x1, y1, x2, y2, x3, y3, x, y) {
                    let idx = y as usize * oc.stride + x as usize;
                    blend_color(&mut oc.pixels[idx], mix_colors3(c1, c2, c3, u1, u2, det));
                }
            }
        }
    }
}

fn olivec_triangle3z(
    oc: &mut Canvas,
    x1: i32, y1: i32, x2: i32, y2: i32, x3: i32, y3: i32,
    z1: f32, z2: f32, z3: f32,
) {
    if let Some((lx, hx, ly, hy)) = olivec_normalize_triangle(oc.width, oc.height, x1, y1, x2, y2, x3, y3) {
        for y in ly..=hy {
            for x in lx..=hx {
                if let Some((u1, u2, det)) = olivec_barycentric(x1, y1, x2, y2, x3, y3, x, y) {
                    let detf = det as f32;
                    let z = z1 * u1 as f32 / detf + z2 * u2 as f32 / detf + z3 * (det - u1 - u2) as f32 / detf;
                    oc.set_pixel(x as usize, y as usize, z.to_bits());
                }
            }
        }
    }
}

fn olivec_triangle3uv(
    oc: &mut Canvas,
    x1: i32, y1: i32, x2: i32, y2: i32, x3: i32, y3: i32,
    tx1: f32, ty1: f32, tx2: f32, ty2: f32, tx3: f32, ty3: f32,
    z1: f32, z2: f32, z3: f32,
    texture: &Canvas,
) {
    if let Some((lx, hx, ly, hy)) = olivec_normalize_triangle(oc.width, oc.height, x1, y1, x2, y2, x3, y3) {
        for y in ly..=hy {
            for x in lx..=hx {
                if let Some((u1, u2, det)) = olivec_barycentric(x1, y1, x2, y2, x3, y3, x, y) {
                    let u3 = det - u1 - u2;
                    let detf = det as f32;
                    let z = z1 * u1 as f32 / detf + z2 * u2 as f32 / detf + z3 * (det - u1 - u2) as f32 / detf;
                    let tx = tx1 * u1 as f32 / detf + tx2 * u2 as f32 / detf + tx3 * u3 as f32 / detf;
                    let ty = ty1 * u1 as f32 / detf + ty2 * u2 as f32 / detf + ty3 * u3 as f32 / detf;

                    let mut texture_x = (tx / z * texture.width as f32) as i32;
                    if texture_x < 0 { texture_x = 0; }
                    if texture_x as usize >= texture.width { texture_x = texture.width as i32 - 1; }

                    let mut texture_y = (ty / z * texture.height as f32) as i32;
                    if texture_y < 0 { texture_y = 0; }
                    if texture_y as usize >= texture.height { texture_y = texture.height as i32 - 1; }

                    oc.set_pixel(x as usize, y as usize, texture.pixel(texture_x as usize, texture_y as usize));
                }
            }
        }
    }
}

fn olivec_pixel_bilinear(sprite: &Canvas, nx: i32, ny: i32, w: i32, h: i32) -> u32 {
    let mut px = nx % w;
    let mut py = ny % h;
    let mut x1 = nx / w;
    let mut x2 = nx / w;
    let mut y1 = ny / h;
    let mut y2 = ny / h;

    if px < w / 2 {
        px += w / 2;
        x1 -= 1;
        if x1 < 0 { x1 = 0; }
    } else {
        px -= w / 2;
        x2 += 1;
        if x2 as usize >= sprite.width { x2 = sprite.width as i32 - 1; }
    }

    if py < h / 2 {
        py += h / 2;
        y1 -= 1;
        if y1 < 0 { y1 = 0; }
    } else {
        py -= h / 2;
        y2 += 1;
        if y2 as usize >= sprite.height { y2 = sprite.height as i32 - 1; }
    }

    mix_colors2(
        mix_colors2(
            sprite.pixel(x1 as usize, y1 as usize),
            sprite.pixel(x2 as usize, y1 as usize),
            px, w,
        ),
        mix_colors2(
            sprite.pixel(x1 as usize, y2 as usize),
            sprite.pixel(x2 as usize, y2 as usize),
            px, w,
        ),
        py, h,
    )
}

fn olivec_triangle3uv_bilinear(
    oc: &mut Canvas,
    x1: i32, y1: i32, x2: i32, y2: i32, x3: i32, y3: i32,
    tx1: f32, ty1: f32, tx2: f32, ty2: f32, tx3: f32, ty3: f32,
    z1: f32, z2: f32, z3: f32,
    texture: &Canvas,
) {
    if let Some((lx, hx, ly, hy)) = olivec_normalize_triangle(oc.width, oc.height, x1, y1, x2, y2, x3, y3) {
        for y in ly..=hy {
            for x in lx..=hx {
                if let Some((u1, u2, det)) = olivec_barycentric(x1, y1, x2, y2, x3, y3, x, y) {
                    let u3 = det - u1 - u2;
                    let detf = det as f32;
                    let z = z1 * u1 as f32 / detf + z2 * u2 as f32 / detf + z3 * (det - u1 - u2) as f32 / detf;
                    let tx = tx1 * u1 as f32 / detf + tx2 * u2 as f32 / detf + tx3 * u3 as f32 / detf;
                    let ty = ty1 * u1 as f32 / detf + ty2 * u2 as f32 / detf + ty3 * u3 as f32 / detf;

                    let mut texture_x = tx / z * texture.width as f32;
                    if texture_x < 0.0 { texture_x = 0.0; }
                    if texture_x >= texture.width as f32 { texture_x = (texture.width - 1) as f32; }

                    let mut texture_y = ty / z * texture.height as f32;
                    if texture_y < 0.0 { texture_y = 0.0; }
                    if texture_y >= texture.height as f32 { texture_y = (texture.height - 1) as f32; }

                    let precision = 100;
                    oc.set_pixel(
                        x as usize, y as usize,
                        olivec_pixel_bilinear(
                            texture,
                            (texture_x * precision as f32) as i32,
                            (texture_y * precision as f32) as i32,
                            precision,
                            precision,
                        ),
                    );
                }
            }
        }
    }
}

fn olivec_text(oc: &mut Canvas, text: &str, tx: i32, ty: i32, font: &Font, glyph_size: usize, color: u32) {
    for (i, ch) in text.bytes().enumerate() {
        let gx = tx + (i * font.width * glyph_size) as i32;
        let gy = ty;
        let glyph = &font.glyphs[ch as usize];
        for dy in 0..font.height {
            for dx in 0..font.width {
                let px = gx + (dx * glyph_size) as i32;
                let py = gy + (dy * glyph_size) as i32;
                if 0 <= px && (px as usize) < oc.width && 0 <= py && (py as usize) < oc.height {
                    if glyph[dy][dx] != 0 {
                        olivec_rect(oc, px, py, glyph_size as i32, glyph_size as i32, color);
                    }
                }
            }
        }
    }
}

fn olivec_sprite_blend(oc: &mut Canvas, x: i32, y: i32, w: i32, h: i32, sprite: &Canvas) {
    if sprite.width == 0 || sprite.height == 0 { return; }
    let nr = match normalize_rect(x, y, w, h, oc.width, oc.height) {
        Some(nr) => nr,
        None => return,
    };

    let xa = if w < 0 { nr.ox2 } else { nr.ox1 };
    let ya = if h < 0 { nr.oy2 } else { nr.oy1 };
    for iy in nr.y1..=nr.y2 {
        for ix in nr.x1..=nr.x2 {
            let nx = ((ix - xa) * sprite.width as i32 / w) as usize;
            let ny = ((iy - ya) * sprite.height as i32 / h) as usize;
            let src = sprite.pixel(nx, ny);
            let idx = iy as usize * oc.stride + ix as usize;
            blend_color(&mut oc.pixels[idx], src);
        }
    }
}

fn olivec_sprite_copy(oc: &mut Canvas, x: i32, y: i32, w: i32, h: i32, sprite: &Canvas) {
    if sprite.width == 0 || sprite.height == 0 { return; }
    let nr = match normalize_rect(x, y, w, h, oc.width, oc.height) {
        Some(nr) => nr,
        None => return,
    };

    let xa = if w < 0 { nr.ox2 } else { nr.ox1 };
    let ya = if h < 0 { nr.oy2 } else { nr.oy1 };
    for iy in nr.y1..=nr.y2 {
        for ix in nr.x1..=nr.x2 {
            let nx = ((ix - xa) * sprite.width as i32 / w) as usize;
            let ny = ((iy - ya) * sprite.height as i32 / h) as usize;
            oc.set_pixel(ix as usize, iy as usize, sprite.pixel(nx, ny));
        }
    }
}

fn olivec_sprite_copy_bilinear(oc: &mut Canvas, x: i32, y: i32, w: i32, h: i32, sprite: &Canvas) {
    if w <= 0 || h <= 0 { return; }
    let nr = match normalize_rect(x, y, w, h, oc.width, oc.height) {
        Some(nr) => nr,
        None => return,
    };

    for iy in nr.y1..=nr.y2 {
        for ix in nr.x1..=nr.x2 {
            let nx = ((ix - nr.ox1) as usize * sprite.width) as i32;
            let ny = ((iy - nr.oy1) as usize * sprite.height) as i32;
            oc.set_pixel(ix as usize, iy as usize, olivec_pixel_bilinear(sprite, nx, ny, w, h));
        }
    }
}

// Subcanvas: In C, olivec_subcanvas shares the parent's pixel buffer.
// In Rust, we use region-based operations directly on the parent canvas
// to achieve the same result safely (no shared mutable aliasing).

// ======== Glyph Data ========

static DEFAULT_GLYPHS: [[[u8; DEFAULT_FONT_WIDTH]; DEFAULT_FONT_HEIGHT]; 128] = {
    let mut g = [[[0u8; DEFAULT_FONT_WIDTH]; DEFAULT_FONT_HEIGHT]; 128];

    g[b'a' as usize] = [
        [0, 0, 0, 0, 0, 0], [0, 1, 1, 0, 0, 0], [0, 0, 0, 1, 0, 0],
        [0, 1, 1, 1, 0, 0], [1, 0, 0, 1, 0, 0], [0, 1, 1, 1, 0, 0],
    ];
    g[b'b' as usize] = [
        [1, 0, 0, 0, 0, 0], [1, 1, 1, 0, 0, 0], [1, 0, 0, 1, 0, 0],
        [1, 0, 0, 1, 0, 0], [1, 0, 0, 1, 0, 0], [1, 1, 1, 0, 0, 0],
    ];
    g[b'c' as usize] = [
        [0, 0, 0, 0, 0, 0], [0, 1, 1, 0, 0, 0], [1, 0, 0, 1, 0, 0],
        [1, 0, 0, 0, 0, 0], [1, 0, 0, 1, 0, 0], [0, 1, 1, 0, 0, 0],
    ];
    g[b'd' as usize] = [
        [0, 0, 0, 1, 0, 0], [0, 1, 1, 1, 0, 0], [1, 0, 0, 1, 0, 0],
        [1, 0, 0, 1, 0, 0], [1, 0, 0, 1, 0, 0], [0, 1, 1, 1, 0, 0],
    ];
    g[b'e' as usize] = [
        [0, 0, 0, 0, 0, 0], [0, 1, 1, 0, 0, 0], [1, 0, 0, 1, 0, 0],
        [1, 1, 1, 1, 0, 0], [1, 0, 0, 0, 0, 0], [0, 1, 1, 1, 0, 0],
    ];
    g[b'f' as usize] = [
        [0, 0, 1, 1, 0, 0], [0, 1, 0, 0, 0, 0], [1, 1, 1, 1, 0, 0],
        [0, 1, 0, 0, 0, 0], [0, 1, 0, 0, 0, 0], [0, 1, 0, 0, 0, 0],
    ];
    g[b'g' as usize] = [
        [0, 1, 1, 1, 0, 0], [1, 0, 0, 1, 0, 0], [1, 0, 0, 1, 0, 0],
        [0, 1, 1, 1, 0, 0], [0, 0, 0, 1, 0, 0], [0, 1, 1, 0, 0, 0],
    ];
    g[b'h' as usize] = [
        [1, 0, 0, 0, 0, 0], [1, 1, 1, 0, 0, 0], [1, 0, 0, 1, 0, 0],
        [1, 0, 0, 1, 0, 0], [1, 0, 0, 1, 0, 0], [1, 0, 0, 1, 0, 0],
    ];
    g[b'i' as usize] = [
        [0, 0, 1, 0, 0, 0], [0, 0, 0, 0, 0, 0], [0, 0, 1, 0, 0, 0],
        [0, 0, 1, 0, 0, 0], [0, 0, 1, 0, 0, 0], [0, 0, 1, 0, 0, 0],
    ];
    g[b'j' as usize] = [
        [0, 0, 1, 0, 0, 0], [0, 0, 0, 0, 0, 0], [0, 0, 1, 0, 0, 0],
        [0, 0, 1, 0, 0, 0], [1, 0, 1, 0, 0, 0], [0, 1, 1, 0, 0, 0],
    ];
    g[b'k' as usize] = [
        [1, 0, 0, 0, 0, 0], [1, 0, 0, 1, 0, 0], [1, 0, 1, 0, 0, 0],
        [1, 1, 0, 0, 0, 0], [1, 0, 1, 0, 0, 0], [1, 0, 0, 1, 0, 0],
    ];
    g[b'l' as usize] = [
        [0, 1, 1, 0, 0, 0], [0, 0, 1, 0, 0, 0], [0, 0, 1, 0, 0, 0],
        [0, 0, 1, 0, 0, 0], [0, 0, 1, 0, 0, 0], [0, 1, 1, 1, 0, 0],
    ];
    g[b'm' as usize] = [
        [0, 0, 0, 0, 0, 0], [0, 1, 0, 1, 1, 0], [1, 0, 1, 0, 1, 0],
        [1, 0, 1, 0, 1, 0], [1, 0, 1, 0, 1, 0], [1, 0, 1, 0, 1, 0],
    ];
    g[b'n' as usize] = [
        [0, 0, 0, 0, 0, 0], [0, 1, 1, 1, 0, 0], [1, 0, 0, 1, 0, 0],
        [1, 0, 0, 1, 0, 0], [1, 0, 0, 1, 0, 0], [1, 0, 0, 1, 0, 0],
    ];
    g[b'o' as usize] = [
        [0, 0, 0, 0, 0, 0], [0, 1, 1, 0, 0, 0], [1, 0, 0, 1, 0, 0],
        [1, 0, 0, 1, 0, 0], [1, 0, 0, 1, 0, 0], [0, 1, 1, 0, 0, 0],
    ];
    g[b'p' as usize] = [
        [1, 1, 1, 0, 0, 0], [1, 0, 0, 1, 0, 0], [1, 0, 0, 1, 0, 0],
        [1, 1, 1, 0, 0, 0], [1, 0, 0, 0, 0, 0], [1, 0, 0, 0, 0, 0],
    ];
    g[b'q' as usize] = [
        [0, 1, 1, 1, 0, 0], [1, 0, 0, 1, 0, 0], [1, 0, 0, 1, 0, 0],
        [0, 1, 1, 1, 0, 0], [0, 0, 0, 1, 0, 0], [0, 0, 0, 1, 0, 0],
    ];
    g[b'r' as usize] = [
        [0, 0, 0, 0, 0, 0], [1, 0, 1, 1, 0, 0], [1, 1, 0, 0, 1, 0],
        [1, 0, 0, 0, 0, 0], [1, 0, 0, 0, 0, 0], [1, 0, 0, 0, 0, 0],
    ];
    g[b's' as usize] = [
        [0, 0, 0, 0, 0, 0], [0, 1, 1, 1, 0, 0], [1, 0, 0, 0, 0, 0],
        [1, 1, 1, 1, 0, 0], [0, 0, 0, 1, 0, 0], [1, 1, 1, 0, 0, 0],
    ];
    g[b't' as usize] = [
        [0, 1, 0, 0, 0, 0], [0, 1, 0, 0, 0, 0], [1, 1, 1, 1, 0, 0],
        [0, 1, 0, 0, 0, 0], [0, 1, 0, 1, 0, 0], [0, 1, 1, 0, 0, 0],
    ];
    g[b'u' as usize] = [
        [0, 0, 0, 0, 0, 0], [1, 0, 0, 1, 0, 0], [1, 0, 0, 1, 0, 0],
        [1, 0, 0, 1, 0, 0], [1, 0, 0, 1, 0, 0], [0, 1, 1, 1, 0, 0],
    ];
    g[b'v' as usize] = [
        [0, 0, 0, 0, 0, 0], [1, 0, 0, 1, 0, 0], [1, 0, 0, 1, 0, 0],
        [1, 0, 0, 1, 0, 0], [1, 0, 0, 1, 0, 0], [0, 1, 1, 0, 0, 0],
    ];
    g[b'w' as usize] = [
        [0, 0, 0, 0, 0, 0], [1, 0, 0, 0, 1, 0], [1, 0, 1, 0, 1, 0],
        [1, 0, 1, 0, 1, 0], [1, 0, 1, 0, 1, 0], [0, 1, 1, 1, 1, 0],
    ];
    g[b'x' as usize] = [
        [0, 0, 0, 0, 0, 0], [1, 0, 1, 0, 0, 0], [1, 0, 1, 0, 0, 0],
        [0, 1, 0, 0, 0, 0], [1, 0, 1, 0, 0, 0], [1, 0, 1, 0, 0, 0],
    ];
    g[b'y' as usize] = [
        [0, 0, 0, 0, 0, 0], [1, 0, 1, 0, 0, 0], [1, 0, 1, 0, 0, 0],
        [1, 0, 1, 0, 0, 0], [0, 1, 0, 0, 0, 0], [0, 1, 0, 0, 0, 0],
    ];
    g[b'z' as usize] = [
        [0, 0, 0, 0, 0, 0], [1, 1, 1, 1, 0, 0], [0, 0, 0, 1, 0, 0],
        [0, 1, 1, 0, 0, 0], [1, 0, 0, 0, 0, 0], [1, 1, 1, 1, 0, 0],
    ];
    // Uppercase letters: all zero (same as C)
    // Digits:
    g[b'0' as usize] = [
        [0, 1, 1, 0, 0, 0], [1, 0, 0, 1, 0, 0], [1, 0, 0, 1, 0, 0],
        [1, 0, 0, 1, 0, 0], [1, 0, 0, 1, 0, 0], [0, 1, 1, 0, 0, 0],
    ];
    g[b'1' as usize] = [
        [0, 0, 1, 0, 0, 0], [0, 1, 1, 0, 0, 0], [0, 0, 1, 0, 0, 0],
        [0, 0, 1, 0, 0, 0], [0, 0, 1, 0, 0, 0], [0, 1, 1, 1, 0, 0],
    ];
    g[b'2' as usize] = [
        [0, 1, 1, 0, 0, 0], [1, 0, 0, 1, 0, 0], [0, 0, 0, 1, 0, 0],
        [0, 1, 1, 0, 0, 0], [1, 0, 0, 0, 0, 0], [1, 1, 1, 1, 0, 0],
    ];
    g[b'3' as usize] = [
        [0, 1, 1, 0, 0, 0], [1, 0, 0, 1, 0, 0], [0, 0, 1, 0, 0, 0],
        [0, 0, 0, 1, 0, 0], [1, 0, 0, 1, 0, 0], [0, 1, 1, 0, 0, 0],
    ];
    g[b'4' as usize] = [
        [0, 0, 1, 1, 0, 0], [0, 1, 0, 1, 0, 0], [1, 0, 0, 1, 0, 0],
        [1, 1, 1, 1, 1, 0], [0, 0, 0, 1, 0, 0], [0, 0, 0, 1, 0, 0],
    ];
    g[b'5' as usize] = [
        [1, 1, 1, 0, 0, 0], [1, 0, 0, 0, 0, 0], [1, 1, 1, 0, 0, 0],
        [0, 0, 0, 1, 0, 0], [1, 0, 0, 1, 0, 0], [0, 1, 1, 0, 0, 0],
    ];
    g[b'6' as usize] = [
        [0, 1, 1, 0, 0, 0], [1, 0, 0, 0, 0, 0], [1, 1, 1, 0, 0, 0],
        [1, 0, 0, 1, 0, 0], [1, 0, 0, 1, 0, 0], [0, 1, 1, 0, 0, 0],
    ];
    g[b'7' as usize] = [
        [1, 1, 1, 1, 0, 0], [0, 0, 0, 1, 0, 0], [0, 0, 1, 0, 0, 0],
        [0, 1, 0, 0, 0, 0], [0, 1, 0, 0, 0, 0], [0, 1, 0, 0, 0, 0],
    ];
    g[b'8' as usize] = [
        [0, 1, 1, 0, 0, 0], [1, 0, 0, 1, 0, 0], [0, 1, 1, 0, 0, 0],
        [1, 0, 0, 1, 0, 0], [1, 0, 0, 1, 0, 0], [0, 1, 1, 0, 0, 0],
    ];
    g[b'9' as usize] = [
        [0, 1, 1, 0, 0, 0], [1, 0, 0, 1, 0, 0], [1, 0, 0, 1, 0, 0],
        [0, 1, 1, 1, 0, 0], [0, 0, 0, 1, 0, 0], [0, 1, 1, 0, 0, 0],
    ];
    // Punctuation:
    g[b',' as usize] = [
        [0, 0, 0, 0, 0, 0], [0, 0, 0, 0, 0, 0], [0, 0, 0, 0, 0, 0],
        [0, 0, 0, 0, 0, 0], [0, 0, 0, 1, 0, 0], [0, 0, 1, 0, 0, 0],
    ];
    g[b'.' as usize] = [
        [0, 0, 0, 0, 0, 0], [0, 0, 0, 0, 0, 0], [0, 0, 0, 0, 0, 0],
        [0, 0, 0, 0, 0, 0], [0, 0, 0, 0, 0, 0], [0, 0, 1, 0, 0, 0],
    ];
    g[b'-' as usize] = [
        [0, 0, 0, 0, 0, 0], [0, 0, 0, 0, 0, 0], [0, 0, 0, 0, 0, 0],
        [1, 1, 1, 1, 0, 0], [0, 0, 0, 0, 0, 0], [0, 0, 0, 0, 0, 0],
    ];

    g
};

static DEFAULT_FONT: Font = Font {
    glyphs: &DEFAULT_GLYPHS,
    width: DEFAULT_FONT_WIDTH,
    height: DEFAULT_FONT_HEIGHT,
};

// ======== Test Harness ========

fn make_sprite(w: usize, h: usize, alpha: u32) -> Canvas {
    let mut c = Canvas::new(w, h);
    for i in 0..(w * h) {
        let r = (i % w) as u32 * 85;
        let g = (i / w) as u32 * 85;
        c.pixels[i] = olivec_rgba(r, g, 0, alpha);
    }
    c
}

fn make_sprite_semi(w: usize, h: usize) -> Canvas {
    let mut c = Canvas::new(w, h);
    for i in 0..(w * h) {
        let r = (i % w) as u32 * 85;
        let g = (i / w) as u32 * 85;
        c.pixels[i] = olivec_rgba(r, g, 128, 128);
    }
    c
}

fn test_fill() {
    println!("=== test_fill ===");
    let mut oc = Canvas::new(16, 16);
    olivec_fill(&mut oc, 0xFF0000FF);
    println!("fill_ck: 0x{:08X}", oc.checksum());
    println!("fill_px00: 0x{:08X}", oc.pixel(0, 0));
    println!("fill_px15: 0x{:08X}", oc.pixel(15, 15));
}

fn test_rect() {
    println!("=== test_rect ===");
    let mut oc = Canvas::new(32, 32);
    olivec_fill(&mut oc, 0xFF000000);
    olivec_rect(&mut oc, 4, 4, 16, 16, 0xFF0000FF);
    println!("rect_ck: 0x{:08X}", oc.checksum());
    println!("rect_in: 0x{:08X}", oc.pixel(10, 10));
    println!("rect_out: 0x{:08X}", oc.pixel(0, 0));
}

fn test_frame() {
    println!("=== test_frame ===");
    let mut oc = Canvas::new(32, 32);
    olivec_fill(&mut oc, 0xFF000000);
    olivec_frame(&mut oc, 4, 4, 24, 24, 2, 0xFF0000FF);
    println!("frame_ck: 0x{:08X}", oc.checksum());
}

fn test_circle() {
    println!("=== test_circle ===");
    let mut oc = Canvas::new(32, 32);
    olivec_fill(&mut oc, 0xFF000000);
    olivec_circle(&mut oc, 16, 16, 10, 0xFFFFFFFF);
    println!("circle_ck: 0x{:08X}", oc.checksum());
    println!("circle_center: 0x{:08X}", oc.pixel(16, 16));
}

fn test_ellipse() {
    println!("=== test_ellipse ===");
    let mut oc = Canvas::new(32, 32);
    olivec_fill(&mut oc, 0xFF000000);
    olivec_ellipse(&mut oc, 16, 16, 12, 8, 0xFF00FF00);
    println!("ellipse_ck: 0x{:08X}", oc.checksum());
    println!("ellipse_center: 0x{:08X}", oc.pixel(16, 16));
}

fn test_line() {
    println!("=== test_line ===");
    let mut oc = Canvas::new(32, 32);
    olivec_fill(&mut oc, 0xFF000000);
    olivec_line(&mut oc, 2, 2, 28, 28, 0xFFFFFFFF);
    println!("line_ck: 0x{:08X}", oc.checksum());
    println!("line_start: 0x{:08X}", oc.pixel(2, 2));
    println!("line_end: 0x{:08X}", oc.pixel(28, 28));
    // horizontal line
    let mut oc2 = Canvas::new(32, 32);
    olivec_fill(&mut oc2, 0xFF000000);
    olivec_line(&mut oc2, 0, 16, 31, 16, 0xFFFF0000);
    println!("hline_ck: 0x{:08X}", oc2.checksum());
}

fn test_triangle() {
    println!("=== test_triangle ===");
    let mut oc = Canvas::new(32, 32);
    olivec_fill(&mut oc, 0xFF000000);
    olivec_triangle(&mut oc, 2, 2, 28, 5, 15, 28, 0xFFFFFFFF);
    println!("tri_ck: 0x{:08X}", oc.checksum());
}

fn test_triangle3c() {
    println!("=== test_triangle3c ===");
    let mut oc = Canvas::new(32, 32);
    olivec_fill(&mut oc, 0xFF000000);
    olivec_triangle3c(
        &mut oc, 2, 2, 28, 5, 15, 28,
        olivec_rgba(255, 0, 0, 255),
        olivec_rgba(0, 255, 0, 255),
        olivec_rgba(0, 0, 255, 255),
    );
    println!("tri3c_ck: 0x{:08X}", oc.checksum());
}

fn test_triangle3z() {
    println!("=== test_triangle3z ===");
    let mut oc = Canvas::new(32, 32);
    // pixels already zeroed from Canvas::new
    olivec_triangle3z(&mut oc, 2, 2, 28, 5, 15, 28, 0.1, 0.5, 0.9);
    println!("tri3z_ck: 0x{:08X}", oc.checksum());
    println!("tri3z_px: 0x{:08X}", oc.pixel(15, 15));
}

fn test_triangle3uv() {
    println!("=== test_triangle3uv ===");
    let mut oc = Canvas::new(32, 32);
    olivec_fill(&mut oc, 0xFF000000);
    let texture = make_sprite(4, 4, 255);
    olivec_triangle3uv(
        &mut oc, 2, 2, 28, 5, 15, 28,
        0.0, 0.0, 1.0, 0.0, 0.5, 1.0,
        1.0, 1.0, 1.0,
        &texture,
    );
    println!("tri3uv_ck: 0x{:08X}", oc.checksum());
}

fn test_triangle3uv_bilinear() {
    println!("=== test_triangle3uv_bilinear ===");
    let mut oc = Canvas::new(32, 32);
    olivec_fill(&mut oc, 0xFF000000);
    let texture = make_sprite(4, 4, 255);
    olivec_triangle3uv_bilinear(
        &mut oc, 2, 2, 28, 5, 15, 28,
        0.0, 0.0, 1.0, 0.0, 0.5, 1.0,
        1.0, 1.0, 1.0,
        &texture,
    );
    println!("tri3uv_bi_ck: 0x{:08X}", oc.checksum());
}

fn test_blend_color() {
    println!("=== test_blend_color ===");
    let mut c1 = 0xFF000000u32;
    blend_color(&mut c1, olivec_rgba(255, 0, 0, 128));
    println!("blend_semi: 0x{:08X}", c1);

    let mut c2 = 0xFF000000u32;
    blend_color(&mut c2, olivec_rgba(0, 255, 0, 255));
    println!("blend_opaque: 0x{:08X}", c2);

    let mut c3 = olivec_rgba(100, 100, 100, 255);
    blend_color(&mut c3, olivec_rgba(200, 50, 50, 0));
    println!("blend_trans: 0x{:08X}", c3);
}

fn test_mix_colors() {
    println!("=== test_mix_colors ===");
    let red = olivec_rgba(255, 0, 0, 255);
    let blue = olivec_rgba(0, 0, 255, 255);
    let green = olivec_rgba(0, 255, 0, 255);

    println!("mix2_half: 0x{:08X}", mix_colors2(red, blue, 1, 2));
    println!("mix2_full: 0x{:08X}", mix_colors2(red, blue, 0, 2));
    println!("mix2_zero: 0x{:08X}", mix_colors2(red, blue, 2, 2));
    println!("mix3_eq: 0x{:08X}", mix_colors3(red, green, blue, 1, 1, 3));
    println!("mix2_det0: 0x{:08X}", mix_colors2(red, blue, 0, 0));
    println!("mix3_det0: 0x{:08X}", mix_colors3(red, green, blue, 0, 0, 0));
}

fn test_text() {
    println!("=== test_text ===");
    let mut oc = Canvas::new(64, 16);
    olivec_fill(&mut oc, 0xFF000000);
    olivec_text(&mut oc, "hello", 2, 2, &DEFAULT_FONT, 1, 0xFFFFFFFF);
    println!("text_ck: 0x{:08X}", oc.checksum());

    let mut oc2 = Canvas::new(64, 32);
    olivec_fill(&mut oc2, 0xFF000000);
    olivec_text(&mut oc2, "42", 2, 2, &DEFAULT_FONT, 2, 0xFF0000FF);
    println!("text2_ck: 0x{:08X}", oc2.checksum());
}

fn test_sprite_blend() {
    println!("=== test_sprite_blend ===");
    let mut oc = Canvas::new(16, 16);
    olivec_fill(&mut oc, 0xFF000000);
    let sprite = make_sprite_semi(4, 4);
    olivec_sprite_blend(&mut oc, 2, 2, 8, 8, &sprite);
    println!("spblend_ck: 0x{:08X}", oc.checksum());
    println!("spblend_px: 0x{:08X}", oc.pixel(4, 4));
}

fn test_sprite_copy() {
    println!("=== test_sprite_copy ===");
    let mut oc = Canvas::new(16, 16);
    olivec_fill(&mut oc, 0xFF000000);
    let sprite = make_sprite(4, 4, 255);
    olivec_sprite_copy(&mut oc, 2, 2, 8, 8, &sprite);
    println!("spcopy_ck: 0x{:08X}", oc.checksum());
    println!("spcopy_px: 0x{:08X}", oc.pixel(4, 4));
}

fn test_sprite_copy_bilinear() {
    println!("=== test_sprite_copy_bilinear ===");
    let mut oc = Canvas::new(16, 16);
    olivec_fill(&mut oc, 0xFF000000);
    let sprite = make_sprite(4, 4, 255);
    olivec_sprite_copy_bilinear(&mut oc, 2, 2, 8, 8, &sprite);
    println!("spbilin_ck: 0x{:08X}", oc.checksum());
    println!("spbilin_px: 0x{:08X}", oc.pixel(4, 4));
}

fn test_subcanvas() {
    println!("=== test_subcanvas ===");
    let mut parent = Canvas::new(32, 32);
    olivec_fill(&mut parent, 0xFF000000);

    // Simulate subcanvas: fill region (8,8)-(23,23) with red
    for y in 8..24 {
        for x in 8..24 {
            parent.set_pixel(x, y, 0xFF0000FF);
        }
    }
    println!("sub_ck: 0x{:08X}", parent.checksum());
    println!("sub_px00: 0x{:08X}", parent.pixel(0, 0));
    println!("sub_px88: 0x{:08X}", parent.pixel(8, 8));
    println!("sub_px23: 0x{:08X}", parent.pixel(23, 23));
    println!("sub_px24: 0x{:08X}", parent.pixel(24, 24));

    // Draw rect on sub-region (12,12)-(19,19), rect at (1,1,6,6) relative = (13,13)-(18,18)
    olivec_rect(&mut parent, 13, 13, 6, 6, 0xFF00FF00);
    println!("sub2_ck: 0x{:08X}", parent.checksum());
    println!("sub2_px1313: 0x{:08X}", parent.pixel(13, 13));
}

fn test_normalize_rect() {
    println!("=== test_normalize_rect ===");

    // normal rect
    if let Some(nr) = normalize_rect(5, 5, 10, 10, 32, 32) {
        println!("nr_ok: 1");
        println!("nr_x1: {}", nr.x1);
        println!("nr_y1: {}", nr.y1);
        println!("nr_x2: {}", nr.x2);
        println!("nr_y2: {}", nr.y2);
    } else {
        println!("nr_ok: 0");
    }

    // negative width/height
    if let Some(nr2) = normalize_rect(15, 15, -10, -10, 32, 32) {
        println!("nr2_ok: 1");
        println!("nr2_x1: {}", nr2.x1);
        println!("nr2_y1: {}", nr2.y1);
        println!("nr2_x2: {}", nr2.x2);
        println!("nr2_y2: {}", nr2.y2);
    } else {
        println!("nr2_ok: 0");
    }

    // out of bounds
    if normalize_rect(40, 40, 10, 10, 32, 32).is_some() {
        println!("nr3_ok: 1");
    } else {
        println!("nr3_ok: 0");
    }

    // empty rect (w=0)
    if normalize_rect(5, 5, 0, 10, 32, 32).is_some() {
        println!("nr4_ok: 1");
    } else {
        println!("nr4_ok: 0");
    }

    // clipped rect
    if let Some(nr5) = normalize_rect(-5, -5, 20, 20, 32, 32) {
        println!("nr5_ok: 1");
        println!("nr5_x1: {}", nr5.x1);
        println!("nr5_y1: {}", nr5.y1);
        println!("nr5_x2: {}", nr5.x2);
        println!("nr5_y2: {}", nr5.y2);
    } else {
        println!("nr5_ok: 0");
    }
}

fn test_in_bounds() {
    println!("=== test_in_bounds ===");
    let oc = Canvas::new(10, 10);
    println!("ib_00: {}", oc.in_bounds(0, 0) as i32);
    println!("ib_99: {}", oc.in_bounds(9, 9) as i32);
    println!("ib_neg: {}", oc.in_bounds(-1, 0) as i32);
    println!("ib_oob: {}", oc.in_bounds(10, 10) as i32);
    println!("ib_edge: {}", oc.in_bounds(10, 0) as i32);
}

fn test_barycentric() {
    println!("=== test_barycentric ===");
    if let Some((u1, u2, det)) = olivec_barycentric(0, 0, 10, 0, 5, 10, 5, 3) {
        println!("bary_in: 1");
        println!("bary_u1: {}", u1);
        println!("bary_u2: {}", u2);
        println!("bary_det: {}", det);
    } else {
        println!("bary_in: 0");
    }

    if olivec_barycentric(0, 0, 10, 0, 5, 10, 20, 20).is_some() {
        println!("bary_out: 1");
    } else {
        println!("bary_out: 0");
    }
}

fn test_normalize_triangle() {
    println!("=== test_normalize_triangle ===");
    if let Some((lx, hx, ly, hy)) = olivec_normalize_triangle(32, 32, 5, 5, 25, 10, 15, 25) {
        println!("ntri_ok: 1");
        println!("ntri_lx: {}", lx);
        println!("ntri_hx: {}", hx);
        println!("ntri_ly: {}", ly);
        println!("ntri_hy: {}", hy);
    } else {
        println!("ntri_ok: 0");
    }

    if olivec_normalize_triangle(32, 32, 40, 40, 50, 45, 45, 50).is_some() {
        println!("ntri2_ok: 1");
    } else {
        println!("ntri2_ok: 0");
    }
}

fn test_edge_cases() {
    println!("=== test_edge_cases ===");
    let mut oc = Canvas::new(8, 8);
    olivec_fill(&mut oc, 0xFF000000);

    // empty rect (w=0)
    olivec_rect(&mut oc, 2, 2, 0, 4, 0xFF0000FF);
    // zero-length line (single pixel)
    olivec_line(&mut oc, 4, 4, 4, 4, 0xFF00FF00);
    // fully out-of-bounds rect
    olivec_rect(&mut oc, 20, 20, 4, 4, 0xFFFF0000);
    // partially out-of-bounds line
    olivec_line(&mut oc, -5, 0, 12, 7, 0xFFFFFFFF);
    // frame with zero thickness
    olivec_frame(&mut oc, 1, 1, 6, 6, 0, 0xFFFF0000);

    println!("edge_ck: 0x{:08X}", oc.checksum());
    println!("edge_px44: 0x{:08X}", oc.pixel(4, 4));
}

fn main() {
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
    println!("olive_done");
}
