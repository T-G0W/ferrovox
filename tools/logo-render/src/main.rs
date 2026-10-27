use resvg::{tiny_skia, usvg};
use std::path::Path;

fn render(svg: &Path, width: u32) -> tiny_skia::Pixmap {
    let mut opt = usvg::Options::default();
    opt.fontdb_mut().load_system_fonts();
    let data = std::fs::read(svg).expect("SVG lesen");
    let tree = usvg::Tree::from_data(&data, &opt).expect("SVG parsen");
    let size = tree.size();
    let scale = width as f32 / size.width();
    let height = (size.height() * scale).round() as u32;
    let mut pixmap = tiny_skia::Pixmap::new(width, height).expect("Pixmap");
    resvg::render(&tree, tiny_skia::Transform::from_scale(scale, scale), &mut pixmap.as_mut());
    pixmap
}

fn write_ico(path: &Path, images: &[(u32, Vec<u8>)]) {
    let mut out = Vec::new();
    out.extend_from_slice(&0u16.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&(images.len() as u16).to_le_bytes());
    let mut offset = 6 + 16 * images.len() as u32;
    for (size, png) in images {
        let dim = if *size >= 256 { 0u8 } else { *size as u8 };
        out.push(dim);
        out.push(dim);
        out.push(0);
        out.push(0);
        out.extend_from_slice(&1u16.to_le_bytes());
        out.extend_from_slice(&32u16.to_le_bytes());
        out.extend_from_slice(&(png.len() as u32).to_le_bytes());
        out.extend_from_slice(&offset.to_le_bytes());
        offset += png.len() as u32;
    }
    for (_, png) in images {
        out.extend_from_slice(png);
    }
    std::fs::write(path, out).expect("ICO schreiben");
}

fn main() {
    let assets = Path::new("assets");
    let logo = assets.join("logo.svg");

    let mut ico_images = Vec::new();
    for size in [16u32, 24, 32, 48, 64, 128, 256] {
        let png = render(&logo, size).encode_png().expect("PNG");
        ico_images.push((size, png));
    }
    write_ico(&assets.join("icon.ico"), &ico_images);

    render(&logo, 512).save_png(assets.join("logo-512.png")).expect("PNG speichern");
    render(&logo, 128).save_png(assets.join("logo-128.png")).expect("PNG speichern");
    render(&assets.join("wordmark.svg"), 1200)
        .save_png(assets.join("wordmark.png"))
        .expect("PNG speichern");
    println!("Logo-Assets erzeugt.");
}
