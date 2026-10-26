use tauri::image::Image;

/// A small status ring in the layer colour. Windows tray icons are 16–32 px,
/// so a ring reads better than any glyph.
pub fn icon(color: &str) -> Image<'static> {
    let (red, green, blue) = rgb(color);
    let size = 32u32;
    let mut rgba = vec![0u8; (size * size * 4) as usize];
    let center = (size as f32 - 1.0) / 2.0;

    for y in 0..size {
        for x in 0..size {
            let dx = x as f32 - center;
            let dy = y as f32 - center;
            let distance = (dx * dx + dy * dy).sqrt();
            let index = ((y * size + x) * 4) as usize;
            let pixel = if distance <= 5.0 {
                [14, 15, 19, 255]
            } else if distance <= 11.5 {
                [red, green, blue, 255]
            } else if distance <= 13.5 {
                [10, 12, 16, 255]
            } else {
                [0, 0, 0, 0]
            };
            rgba[index..index + 4].copy_from_slice(&pixel);
        }
    }

    Image::new_owned(rgba, size, size)
}

pub fn short(value: &str, max: usize) -> String {
    if value.chars().count() <= max {
        value.to_string()
    } else {
        let mut out: String = value.chars().take(max.saturating_sub(1)).collect();
        out.push('…');
        out
    }
}

fn rgb(color: &str) -> (u8, u8, u8) {
    let hex = color.trim().trim_start_matches('#');
    if hex.len() == 6 {
        if let (Ok(r), Ok(g), Ok(b)) = (
            u8::from_str_radix(&hex[0..2], 16),
            u8::from_str_radix(&hex[2..4], 16),
            u8::from_str_radix(&hex[4..6], 16),
        ) {
            return (r, g, b);
        }
    }
    (125, 211, 252)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn short_never_exceeds_the_limit() {
        assert_eq!(short("macrokey", 20), "macrokey");
        assert_eq!(short("macrokey", 5), "macr…");
        assert_eq!(short("macrokey", 1), "…");
    }
}
