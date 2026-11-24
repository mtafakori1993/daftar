//! Image and SVG loading for the reader.

use std::path::Path;

pub struct LoadedImage {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

pub fn load_image(path: &Path, max_side: u32) -> Result<LoadedImage, String> {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    if ext == "svg" {
        return load_svg(path, max_side);
    }
    let image = image::open(path).map_err(|e| format!("Cannot open image: {e}"))?;
    let image = image.to_rgba8();
    let (w, h) = image.dimensions();
    if w.max(h) <= max_side {
        Ok(LoadedImage {
            width: w,
            height: h,
            rgba: image.into_raw(),
        })
    } else {
        let scale = max_side as f32 / w.max(h) as f32;
        let nw = (w as f32 * scale).round().max(1.0) as u32;
        let nh = (h as f32 * scale).round().max(1.0) as u32;
        let resized = image::imageops::resize(
            &image,
            nw,
            nh,
            image::imageops::FilterType::Lanczos3,
        );
        Ok(LoadedImage {
            width: nw,
            height: nh,
            rgba: resized.into_raw(),
        })
    }
}

fn load_svg(path: &Path, max_side: u32) -> Result<LoadedImage, String> {
    let data = std::fs::read(path).map_err(|e| format!("Cannot read SVG: {e}"))?;
    let mut fontdb = resvg::usvg::fontdb::Database::new();
    fontdb.load_system_fonts();
    for font in daftar_text::bundled_fonts() {
        fontdb.load_font_data(font.bytes.to_vec());
    }
    let mut options = resvg::usvg::Options::default();
    options.fontdb = std::sync::Arc::new(fontdb);
    options.resources_dir = path.parent().map(|p| p.to_path_buf());
    let tree = resvg::usvg::Tree::from_data(&data, &options)
        .map_err(|e| format!("Invalid SVG: {e}"))?;
    let size = tree.size();
    let scale = (max_side as f32 / size.width().max(size.height())).min(1.0);
    let w = (size.width() * scale).ceil().max(1.0) as u32;
    let h = (size.height() * scale).ceil().max(1.0) as u32;
    let mut pixmap = tiny_skia::Pixmap::new(w, h).ok_or("Could not allocate SVG pixmap")?;
    resvg::render(
        &tree,
        tiny_skia::Transform::from_scale(scale, scale),
        &mut pixmap.as_mut(),
    );
    let rgba = pixmap
        .pixels()
        .iter()
        .flat_map(|p| { let c = p.demultiply(); [c.red(), c.green(), c.blue(), c.alpha()] })
        .collect();
    Ok(LoadedImage {
        width: w,
        height: h,
        rgba,
    })
}

pub fn image_dimensions(path: &Path) -> Option<(u32, u32)> {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    if ext == "svg" {
        let data = std::fs::read(path).ok()?;
        let tree = resvg::usvg::Tree::from_data(&data, &resvg::usvg::Options::default()).ok()?;
        let size = tree.size();
        Some((
            size.width().round().max(1.0) as u32,
            size.height().round().max(1.0) as u32,
        ))
    } else {
        image::image_dimensions(path).ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loads_generated_png() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("x.png");
        let mut image = image::RgbaImage::new(20, 10);
        image.put_pixel(0, 0, image::Rgba([255, 0, 0, 255]));
        image.save(&path).unwrap();
        let loaded = load_image(&path, 100).unwrap();
        assert_eq!((loaded.width, loaded.height), (20, 10));
        assert_eq!(image_dimensions(&path), Some((20, 10)));
    }

    #[test]
    fn renders_svg() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("x.svg");
        std::fs::write(
            &path,
            r##"<svg xmlns="http://www.w3.org/2000/svg" width="40" height="20"><rect width="40" height="20" fill="#2680EB"/></svg>"##,
        )
        .unwrap();
        let loaded = load_image(&path, 200).unwrap();
        assert_eq!((loaded.width, loaded.height), (40, 20));
        assert!(loaded.rgba.iter().any(|b| *b > 0));
    }

    #[test]
    fn rejects_missing_file() {
        assert!(load_image(std::path::Path::new("/definitely/missing.png"), 100).is_err());
    }
}