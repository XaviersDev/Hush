// Renders app.png / app.ico and a contact sheet of all overlay styles.
use hush::icons;
use tiny_skia::*;

fn main() {
    let out = std::env::args().nth(1).unwrap_or_else(|| "assets".into());
    icons::render_app(64).save_png(format!("{out}/app.png")).unwrap();

    let sizes = [16u32, 20, 24, 32, 40, 48, 64, 128, 256];
    // classic BMP entries, PNG only for 256: every tool (windres, NSIS, Explorer) reads this
    let pngs: Vec<Vec<u8>> = sizes.iter().map(|&s| {
        let pm = icons::render_app(s);
        if s >= 256 {
            return pm.encode_png().unwrap();
        }
        let mut b = Vec::new();
        b.extend_from_slice(&40u32.to_le_bytes());
        b.extend_from_slice(&(s as i32).to_le_bytes());
        b.extend_from_slice(&((s * 2) as i32).to_le_bytes());
        b.extend_from_slice(&1u16.to_le_bytes());
        b.extend_from_slice(&32u16.to_le_bytes());
        b.extend_from_slice(&[0u8; 24]);
        for y in (0..s).rev() {
            for x in 0..s {
                let p = pm.pixel(x, y).unwrap().demultiply();
                b.extend_from_slice(&[p.blue(), p.green(), p.red(), p.alpha()]);
            }
        }
        let row = ((s + 31) / 32 * 4) as usize;
        b.extend(std::iter::repeat(0u8).take(row * s as usize));
        b
    }).collect();
    let mut ico = Vec::new();
    ico.extend_from_slice(&[0, 0, 1, 0]);
    ico.extend_from_slice(&(sizes.len() as u16).to_le_bytes());
    let mut offset = 6 + 16 * sizes.len() as u32;
    for (s, p) in sizes.iter().zip(&pngs) {
        let b = if *s >= 256 { 0 } else { *s as u8 };
        ico.extend_from_slice(&[b, b, 0, 0, 1, 0, 32, 0]);
        ico.extend_from_slice(&(p.len() as u32).to_le_bytes());
        ico.extend_from_slice(&offset.to_le_bytes());
        offset += p.len() as u32;
    }
    for p in &pngs {
        ico.extend_from_slice(p);
    }
    std::fs::write(format!("{out}/app.ico"), ico).unwrap();

    // contact sheet on dark and light backgrounds
    let cell = 96u32;
    let sizes2 = [cell, 40];
    let mut sheet = Pixmap::new(cell * 10 + 20, cell * 4 + 150).unwrap();
    sheet.fill(Color::from_rgba8(30, 30, 34, 255));
    let mut light = Paint::default();
    light.set_color_rgba8(235, 235, 235, 255);
    sheet.fill_rect(Rect::from_xywh(0.0, (cell * 2) as f32, sheet.width() as f32, (cell * 2) as f32).unwrap(), &light, Transform::identity(), None);
    for st in 0..icons::STYLE_COUNT {
        for (row, muted) in [false, true].iter().enumerate() {
            for band in 0..2 {
                let icon = icons::render(st, *muted, sizes2[0] - 16);
                let x = 10 + st as u32 * cell + 8;
                let y = band * cell * 2 + row as u32 * cell + 8;
                sheet.draw_pixmap(x as i32, y as i32, icon.as_ref(), &PixmapPaint::default(), Transform::identity(), None);
            }
            let small = icons::render(st, *muted, 40);
            sheet.draw_pixmap((10 + st as u32 * cell + 28) as i32, (cell * 4 + 10 + row as u32 * 44) as i32, small.as_ref(), &PixmapPaint::default(), Transform::identity(), None);
        }
    }
    for (i, (badge, muted)) in [(true, false), (true, true), (false, false), (false, true)].iter().enumerate() {
        for (j, s) in [16u32, 20, 24, 32].iter().enumerate() {
            let t = icons::render_tray(*badge, *muted, *s);
            sheet.draw_pixmap(10 + i as i32 * 150 + j as i32 * 34, (cell * 4 + 100) as i32, t.as_ref(), &PixmapPaint::default(), Transform::identity(), None);
        }
    }
    sheet.save_png(format!("{out}/sheet.png")).unwrap();
}
