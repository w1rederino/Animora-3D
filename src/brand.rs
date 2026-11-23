//! The Animora app icon, drawn in code: a bouncing orange ball with its
//! motion arc on a dark blue rounded square.

/// Renders the icon at `size`×`size` pixels (RGBA, unmultiplied alpha).
pub fn icon_rgba(size: u32) -> Vec<u8> {
    let s = size as f32;
    let mut out = vec![0u8; (size * size * 4) as usize];
    // 4×4 supersampling for smooth edges
    let ss = 4;
    for y in 0..size {
        for x in 0..size {
            let mut acc = [0f32; 4];
            for sy in 0..ss {
                for sx in 0..ss {
                    let px = (x as f32 + (sx as f32 + 0.5) / ss as f32) / s;
                    let py = (y as f32 + (sy as f32 + 0.5) / ss as f32) / s;
                    let c = shade(px, py);
                    // premultiply for averaging
                    acc[0] += c[0] * c[3];
                    acc[1] += c[1] * c[3];
                    acc[2] += c[2] * c[3];
                    acc[3] += c[3];
                }
            }
            let n = (ss * ss) as f32;
            let a = acc[3] / n;
            let i = ((y * size + x) * 4) as usize;
            if a > 0.0 {
                out[i] = (acc[0] / n / a * 255.0).clamp(0.0, 255.0) as u8;
                out[i + 1] = (acc[1] / n / a * 255.0).clamp(0.0, 255.0) as u8;
                out[i + 2] = (acc[2] / n / a * 255.0).clamp(0.0, 255.0) as u8;
            }
            out[i + 3] = (a * 255.0).clamp(0.0, 255.0) as u8;
        }
    }
    out
}

/// Colour (r, g, b, a in 0..1) at a point of the unit square.
fn shade(x: f32, y: f32) -> [f32; 4] {
    // rounded square background
    let m = 0.04;
    let r = 0.2;
    let qx = (x - 0.5).abs() - (0.5 - m - r);
    let qy = (y - 0.5).abs() - (0.5 - m - r);
    let d = (qx.max(0.0).powi(2) + qy.max(0.0).powi(2)).sqrt() + qx.max(qy).min(0.0) - r;
    if d > 0.0 {
        return [0.0; 4];
    }
    let t = y;
    let mut c = [0.16 + 0.10 * (1.0 - t), 0.22 + 0.12 * (1.0 - t), 0.34 + 0.16 * (1.0 - t)];
    // ground line
    if (y - 0.80).abs() < 0.012 && x > 0.14 && x < 0.86 {
        c = [0.55, 0.62, 0.72];
    }
    // dotted motion arc: two bounces
    let arc = |x: f32| -> f32 {
        if x < 0.5 {
            let u = (x - 0.15) / 0.35;
            0.74 - 0.48 * (1.0 - (2.0 * u - 1.0).powi(2))
        } else {
            let u = (x - 0.5) / 0.35;
            0.74 - 0.28 * (1.0 - (2.0 * u - 1.0).powi(2))
        }
    };
    if x > 0.15 && x < 0.85 {
        let ay = arc(x);
        let along = ((x - 0.15) / 0.035).fract();
        if (y - ay).abs() < 0.018 && along < 0.55 {
            c = [1.0, 0.72, 0.35];
        }
    }
    // the ball, slightly squashed, at the top of the first bounce
    let (bx, by) = (0.33, 0.30);
    let (rx, ry) = (0.13, 0.12);
    let e = ((x - bx) / rx).powi(2) + ((y - by) / ry).powi(2);
    if e < 1.0 {
        let l = (1.0 - (((x - bx + 0.05) / rx).powi(2) + ((y - by + 0.05) / ry).powi(2)) * 0.6).clamp(0.0, 1.0);
        c = [0.80 + 0.2 * l, 0.30 + 0.35 * l, 0.18 + 0.25 * l];
        if e > 0.82 {
            c = [c[0] * 0.8, c[1] * 0.8, c[2] * 0.8];
        }
    }
    // small blue joint at the landing spot
    let jd = ((x - 0.68).powi(2) + (y - 0.74).powi(2)).sqrt();
    if (jd - 0.045).abs() < 0.012 {
        c = [0.45, 0.70, 1.0];
    }
    [c[0], c[1], c[2], 1.0]
}

/// Classic 32-bit DIB icon entry (BITMAPINFOHEADER + BGRA bottom-up + AND mask).
fn dib_entry(s: u32) -> Vec<u8> {
    let rgba = icon_rgba(s);
    let mut out = Vec::new();
    out.extend_from_slice(&40u32.to_le_bytes());
    out.extend_from_slice(&(s as i32).to_le_bytes());
    out.extend_from_slice(&((s * 2) as i32).to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&32u16.to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes());
    out.extend_from_slice(&(s * s * 4).to_le_bytes());
    out.extend_from_slice(&[0u8; 16]);
    for y in (0..s).rev() {
        for x in 0..s {
            let i = ((y * s + x) * 4) as usize;
            out.extend_from_slice(&[rgba[i + 2], rgba[i + 1], rgba[i], rgba[i + 3]]);
        }
    }
    let mask_row = s.div_ceil(32) * 4;
    out.extend(std::iter::repeat_n(0u8, (mask_row * s) as usize));
    out
}

/// Writes a Windows .ico file: sizes 16–128 as bitmaps, 256 as PNG.
pub fn write_ico(path: &std::path::Path) -> Result<(), String> {
    let sizes = [16u32, 24, 32, 48, 64, 128, 256];
    let mut pngs = Vec::new();
    for &s in &sizes {
        if s < 256 {
            pngs.push(dib_entry(s));
            continue;
        }
        let img = image::RgbaImage::from_raw(s, s, icon_rgba(s)).ok_or(crate::tr!("Bildfehler", "Image error"))?;
        let mut buf = std::io::Cursor::new(Vec::new());
        img.write_to(&mut buf, image::ImageFormat::Png).map_err(|e| e.to_string())?;
        pngs.push(buf.into_inner());
    }
    let mut out = Vec::new();
    out.extend_from_slice(&0u16.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&(sizes.len() as u16).to_le_bytes());
    let mut offset = 6 + 16 * sizes.len() as u32;
    for (s, png) in sizes.iter().zip(&pngs) {
        let dim = if *s >= 256 { 0u8 } else { *s as u8 };
        out.extend_from_slice(&[dim, dim, 0, 0]);
        out.extend_from_slice(&1u16.to_le_bytes());
        out.extend_from_slice(&32u16.to_le_bytes());
        out.extend_from_slice(&(png.len() as u32).to_le_bytes());
        out.extend_from_slice(&offset.to_le_bytes());
        offset += png.len() as u32;
    }
    for png in &pngs {
        out.extend_from_slice(png);
    }
    std::fs::write(path, out).map_err(|e| e.to_string())
}
