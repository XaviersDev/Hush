// Mic glyphs and overlay styles. Everything is drawn in a 100x100 box.

use tiny_skia::*;

pub const STYLE_COUNT: usize = 10;

pub fn style_name(i: usize) -> &'static str {
    use crate::i18n::{k, tr};
    tr([
        k::STYLE_0, k::STYLE_1, k::STYLE_2, k::STYLE_3, k::STYLE_4,
        k::STYLE_5, k::STYLE_6, k::STYLE_7, k::STYLE_8, k::STYLE_9,
    ][i % STYLE_COUNT])
}

const LIGHT: (u8, u8, u8) = (0xEC, 0xE6, 0xDE);
const LIVE: (u8, u8, u8) = (0x34, 0xC7, 0x6F);
const MUTED: (u8, u8, u8) = (0xF0, 0x4E, 0x5A);
const NEON_ON: (u8, u8, u8) = (0x3B, 0xE4, 0xFF);
const NEON_OFF: (u8, u8, u8) = (0xFF, 0x4D, 0x6D);

fn rgba(c: (u8, u8, u8), a: u8) -> Color {
    Color::from_rgba8(c.0, c.1, c.2, a)
}

fn paint(c: Color) -> Paint<'static> {
    let mut p = Paint::default();
    p.set_color(c);
    p.anti_alias = true;
    p
}

fn stroke(w: f32) -> Stroke {
    Stroke { width: w, line_cap: LineCap::Round, line_join: LineJoin::Round, ..Default::default() }
}

pub fn rounded_rect(x: f32, y: f32, w: f32, h: f32, r: f32) -> Path {
    let r = r.min(w / 2.0).min(h / 2.0);
    let k = 0.552_284_8 * r;
    let mut pb = PathBuilder::new();
    pb.move_to(x + r, y);
    pb.line_to(x + w - r, y);
    pb.cubic_to(x + w - r + k, y, x + w, y + r - k, x + w, y + r);
    pb.line_to(x + w, y + h - r);
    pb.cubic_to(x + w, y + h - r + k, x + w - r + k, y + h, x + w - r, y + h);
    pb.line_to(x + r, y + h);
    pb.cubic_to(x + r - k, y + h, x, y + h - r + k, x, y + h - r);
    pb.line_to(x, y + r);
    pb.cubic_to(x, y + r - k, x + r - k, y, x + r, y);
    pb.close();
    pb.finish().unwrap()
}

fn circle(cx: f32, cy: f32, r: f32) -> Path {
    PathBuilder::from_circle(cx, cy, r).unwrap()
}

fn line(x1: f32, y1: f32, x2: f32, y2: f32) -> Path {
    let mut pb = PathBuilder::new();
    pb.move_to(x1, y1);
    pb.line_to(x2, y2);
    pb.finish().unwrap()
}

// lower half of a circle, left to right
fn cradle(cx: f32, cy: f32, r: f32) -> Path {
    let k = 0.552_284_8 * r;
    let mut pb = PathBuilder::new();
    pb.move_to(cx - r, cy);
    pb.cubic_to(cx - r, cy + k, cx - k, cy + r, cx, cy + r);
    pb.cubic_to(cx + k, cy + r, cx + r, cy + k, cx + r, cy);
    pb.finish().unwrap()
}

enum Prim {
    Fill(Path),
    Stroke(Path, f32),
}

fn mic(filled: bool) -> Vec<Prim> {
    let body = rounded_rect(37.0, 12.0, 26.0, 48.0, 13.0);
    vec![
        if filled { Prim::Fill(body) } else { Prim::Stroke(body, 6.0) },
        Prim::Stroke(cradle(50.0, 45.0, 23.5), 6.0),
        Prim::Stroke(line(50.0, 68.5, 50.0, 85.0), 6.0),
        Prim::Stroke(line(37.0, 86.0, 63.0, 86.0), 6.0),
    ]
}

fn studio_mic() -> Vec<Prim> {
    vec![
        Prim::Fill(rounded_rect(33.0, 7.0, 34.0, 52.0, 17.0)),
        Prim::Stroke(cradle(50.0, 40.0, 27.0), 5.5),
        Prim::Stroke(line(50.0, 67.0, 50.0, 84.0), 5.5),
        Prim::Stroke(line(34.0, 87.0, 66.0, 87.0), 6.0),
    ]
}

fn draw(pm: &mut Pixmap, prims: &[Prim], c: Color, extra: f32, t: Transform) {
    let p = paint(c);
    for prim in prims {
        match prim {
            Prim::Fill(path) => {
                pm.fill_path(path, &p, FillRule::Winding, t, None);
                if extra > 0.0 {
                    pm.stroke_path(path, &p, &stroke(extra), t, None);
                }
            }
            Prim::Stroke(path, w) => pm.stroke_path(path, &p, &stroke(w + extra), t, None),
        }
    }
}

fn halo() -> Color {
    Color::from_rgba8(0, 0, 0, 105)
}

// slash across the glyph, with a gap cut out around it
fn slash(pm: &mut Pixmap, c: Color, knock: Option<Color>, t: Transform, w: f32) {
    let l = line(23.0, 15.0, 77.0, 87.0);
    let mut k = match knock {
        Some(kc) => paint(kc),
        None => {
            let mut p = Paint::default();
            p.anti_alias = true;
            p.blend_mode = BlendMode::Clear;
            p
        }
    };
    k.anti_alias = true;
    pm.stroke_path(&l, &k, &stroke(w + 8.0), t, None);
    pm.stroke_path(&l, &paint(c), &stroke(w), t, None);
}

fn sub(t: Transform, f: f32) -> Transform {
    t.pre_translate(50.0, 50.0).pre_scale(f, f).pre_translate(-50.0, -50.0)
}

fn glow(pm: &mut Pixmap, c: (u8, u8, u8), a: u8, cx: f32, cy: f32, r: f32, t: Transform) {
    let stops = vec![GradientStop::new(0.0, rgba(c, a)), GradientStop::new(1.0, rgba(c, 0))];
    if let Some(shader) =
        RadialGradient::new(Point::from_xy(cx, cy), Point::from_xy(cx, cy), r, stops, SpreadMode::Pad, Transform::identity())
    {
        let mut p = Paint::default();
        p.shader = shader;
        p.anti_alias = true;
        pm.fill_path(&circle(cx, cy, r), &p, FillRule::Winding, t, None);
    }
}

/// Renders an overlay style into a square pixmap.
pub fn render(style: usize, muted: bool, size: u32) -> Pixmap {
    let mut pm = Pixmap::new(size.max(1), size.max(1)).unwrap();
    let t = Transform::from_scale(size as f32 / 100.0, size as f32 / 100.0);
    let fg = if muted { MUTED } else { LIGHT };
    match style % STYLE_COUNT {
        0 | 1 => {
            let g = mic(style == 1);
            draw(&mut pm, &g, halo(), 4.0, t);
            if muted {
                slash(&mut pm, halo(), None, t, 10.0);
            }
            draw(&mut pm, &g, rgba(fg, 255), 0.0, t);
            if muted {
                slash(&mut pm, rgba(fg, 255), None, t, 6.0);
            }
        }
        2 => {
            let bg = if muted { MUTED } else { LIVE };
            pm.fill_path(&circle(50.0, 50.0, 47.0), &paint(rgba(bg, 255)), FillRule::Winding, t, None);
            pm.stroke_path(&circle(50.0, 50.0, 47.0), &paint(Color::from_rgba8(0, 0, 0, 60)), &stroke(2.0), t, None);
            let st = sub(t, 0.58);
            draw(&mut pm, &mic(true), Color::WHITE, 0.0, st);
            if muted {
                slash(&mut pm, Color::WHITE, Some(rgba(bg, 255)), st, 7.0);
            }
        }
        3 => {
            let c = if muted { MUTED } else { LIVE };
            pm.fill_path(&circle(50.0, 50.0, 46.5), &paint(Color::from_rgba8(17, 17, 19, 232)), FillRule::Winding, t, None);
            pm.stroke_path(&circle(50.0, 50.0, 46.5), &paint(Color::from_rgba8(255, 255, 255, 26)), &stroke(2.5), t, None);
            let st = sub(t, 0.56);
            draw(&mut pm, &mic(false), rgba(c, 255), 0.6, st);
            if muted {
                slash(&mut pm, rgba(c, 255), Some(Color::from_rgba8(17, 17, 19, 255)), st, 7.0);
            }
        }
        4 => {
            glow(&mut pm, fg, 70, 50.0, 50.0, 50.0, t);
            pm.fill_path(&circle(50.0, 50.0, 42.0), &paint(Color::from_rgba8(12, 12, 14, 150)), FillRule::Winding, t, None);
            pm.stroke_path(&circle(50.0, 50.0, 42.0), &paint(rgba(fg, 255)), &stroke(3.5), t, None);
            let st = sub(t, 0.5);
            draw(&mut pm, &mic(false), rgba(fg, 255), 1.0, st);
            if muted {
                slash(&mut pm, rgba(fg, 255), None, st, 7.0);
            }
        }
        5 => {
            pm.fill_path(&rounded_rect(3.0, 3.0, 94.0, 94.0, 24.0), &paint(Color::from_rgba8(18, 18, 20, 228)), FillRule::Winding, t, None);
            pm.stroke_path(&rounded_rect(3.0, 3.0, 94.0, 94.0, 24.0), &paint(Color::from_rgba8(255, 255, 255, 22)), &stroke(2.0), t, None);
            let st = sub(t, 0.6);
            draw(&mut pm, &mic(true), rgba(fg, 255), 0.0, st);
            if muted {
                slash(&mut pm, rgba(fg, 255), Some(Color::from_rgba8(18, 18, 20, 255)), st, 6.5);
            }
        }
        6 => {
            let c = if muted { MUTED } else { LIVE };
            glow(&mut pm, c, 95, 50.0, 50.0, 50.0, t);
            pm.fill_path(&circle(50.0, 50.0, 25.0), &paint(rgba(c, 255)), FillRule::Winding, t, None);
            pm.stroke_path(&circle(50.0, 50.0, 25.0), &paint(Color::from_rgba8(0, 0, 0, 70)), &stroke(2.0), t, None);
            if muted {
                pm.stroke_path(&line(39.0, 50.0, 61.0, 50.0), &paint(Color::WHITE), &stroke(7.0), t, None);
            }
        }
        7 => {
            let c = if muted { MUTED } else { LIVE };
            let hs: [f32; 5] = if muted { [8.0; 5] } else { [30.0, 56.0, 80.0, 56.0, 30.0] };
            let mut bars = Vec::new();
            for (i, h) in hs.iter().enumerate() {
                let x = 18.0 + i as f32 * 16.0;
                bars.push(Prim::Stroke(line(x, 50.0 - h / 2.0, x, 50.0 + h / 2.0), 9.0));
            }
            draw(&mut pm, &bars, halo(), 4.0, t);
            draw(&mut pm, &bars, rgba(c, 255), 0.0, t);
        }
        8 => {
            let g = studio_mic();
            draw(&mut pm, &g, halo(), 4.0, t);
            if muted {
                slash(&mut pm, halo(), None, t, 10.0);
            }
            draw(&mut pm, &g, rgba(fg, 255), 0.0, t);
            let mut cut = Paint::default();
            cut.anti_alias = true;
            cut.blend_mode = BlendMode::Clear;
            for y in [20.0, 29.0, 38.0] {
                pm.stroke_path(&line(41.0, y, 59.0, y), &cut, &stroke(3.2), t, None);
            }
            if muted {
                slash(&mut pm, rgba(fg, 255), None, t, 6.0);
            }
        }
        _ => {
            let c = if muted { NEON_OFF } else { NEON_ON };
            let g = mic(false);
            let st = sub(t, 0.84);
            draw(&mut pm, &g, Color::from_rgba8(0, 0, 0, 70), 7.0, st);
            for (extra, a) in [(14.0, 22u8), (9.0, 40), (5.0, 70)] {
                draw(&mut pm, &g, rgba(c, a), extra, st);
                if muted {
                    pm.stroke_path(&line(23.0, 15.0, 77.0, 87.0), &paint(rgba(c, a)), &stroke(6.0 + extra), st, None);
                }
            }
            draw(&mut pm, &g, rgba(c, 255), 0.0, st);
            draw(&mut pm, &g, Color::from_rgba8(255, 255, 255, 150), -4.0, st);
            if muted {
                pm.stroke_path(&line(23.0, 15.0, 77.0, 87.0), &paint(rgba(c, 255)), &stroke(6.0), st, None);
                pm.stroke_path(&line(23.0, 15.0, 77.0, 87.0), &paint(Color::from_rgba8(255, 255, 255, 150)), &stroke(2.0), st, None);
            }
        }
    }
    pm
}

/// Tray icon. badge = colored disc with a white mic, otherwise just a colored glyph.
pub fn render_tray(badge: bool, muted: bool, size: u32) -> Pixmap {
    let mut pm = Pixmap::new(size.max(1), size.max(1)).unwrap();
    let t = Transform::from_scale(size as f32 / 100.0, size as f32 / 100.0);
    let c = if muted { MUTED } else { LIVE };
    if badge {
        pm.fill_path(&circle(50.0, 50.0, 49.0), &paint(rgba(c, 255)), FillRule::Winding, t, None);
        let st = sub(t, 0.66);
        draw(&mut pm, &mic(true), Color::WHITE, 1.5, st);
        if muted {
            slash(&mut pm, Color::WHITE, Some(rgba(c, 255)), st, 8.0);
        }
    } else {
        let st = sub(t, 1.06);
        draw(&mut pm, &mic(true), rgba(c, 255), 3.0, st);
        if muted {
            slash(&mut pm, rgba(c, 255), None, st, 9.0);
        }
    }
    pm
}

/// App icon: black disc, white ring, white mic.
pub fn render_app(size: u32) -> Pixmap {
    let mut pm = Pixmap::new(size, size).unwrap();
    let t = Transform::from_scale(size as f32 / 100.0, size as f32 / 100.0);
    let white = (0xFF, 0xFF, 0xFF);
    pm.fill_path(&circle(50.0, 50.0, 48.5), &paint(Color::from_rgba8(10, 10, 10, 255)), FillRule::Winding, t, None);
    glow(&mut pm, white, 26, 50.0, 50.0, 44.0, t);
    let ring_w = if size <= 20 { 7.0 } else if size <= 32 { 5.5 } else { 4.0 };
    pm.stroke_path(&circle(50.0, 50.0, 40.0), &paint(rgba(white, 255)), &stroke(ring_w), t, None);
    let st = sub(t, 0.52);
    let extra = if size <= 24 { 4.0 } else { 0.5 };
    draw(&mut pm, &mic(true), rgba(white, 255), extra, st);
    pm
}

/// Premultiplied RGBA to premultiplied BGRA, in place.
pub fn to_bgra(data: &mut [u8]) {
    for px in data.chunks_exact_mut(4) {
        px.swap(0, 2);
    }
}
