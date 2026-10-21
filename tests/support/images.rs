use std::io::Cursor;

use base64::Engine;
use image::{DynamicImage, ImageFormat, Rgba, RgbaImage};

pub fn create_zip_with_single_png() -> Vec<u8> {
    use std::io::Write;

    let mut cursor = Cursor::new(Vec::new());
    let mut writer = zip::ZipWriter::new(&mut cursor);
    let options = zip::write::SimpleFileOptions::default();
    writer
        .start_file("image.png", options)
        .expect("in-memory test ZIP operation succeeds");
    writer
        .write_all(&create_png_bytes(2, 2))
        .expect("in-memory test ZIP operation succeeds");
    writer
        .finish()
        .expect("in-memory test ZIP operation succeeds");
    cursor.into_inner()
}

pub fn create_zip_with_two_images() -> Vec<u8> {
    use std::io::Write;

    let mut cursor = Cursor::new(Vec::new());
    let mut writer = zip::ZipWriter::new(&mut cursor);
    let options = zip::write::SimpleFileOptions::default();
    writer
        .start_file("first.png", options)
        .expect("in-memory test ZIP operation succeeds");
    writer
        .write_all(&create_png_bytes(2, 2))
        .expect("in-memory test ZIP operation succeeds");
    writer
        .start_file("second.png", options)
        .expect("in-memory test ZIP operation succeeds");
    writer
        .write_all(&create_png_bytes(3, 2))
        .expect("in-memory test ZIP operation succeeds");
    writer
        .finish()
        .expect("in-memory test ZIP operation succeeds");
    cursor.into_inner()
}

pub fn create_png_with_comment(comment: &serde_json::Value) -> Vec<u8> {
    create_png_with_comment_text(&comment.to_string())
}

pub fn create_png_with_comment_text(comment: &str) -> Vec<u8> {
    let mut output = Vec::new();
    let mut encoder = png::Encoder::new(&mut output, 2, 2);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    encoder
        .add_text_chunk("Comment".to_owned(), comment.to_owned())
        .expect("add comment");
    let mut writer = encoder.write_header().expect("write header");
    writer
        .write_image_data(&[255_u8; 2 * 2 * 4])
        .expect("write image");
    drop(writer);
    output
}

pub fn create_zip_with_pngs(images: &[(&str, Vec<u8>)]) -> Vec<u8> {
    use std::io::Write;

    let mut cursor = Cursor::new(Vec::new());
    let mut writer = zip::ZipWriter::new(&mut cursor);
    let options = zip::write::SimpleFileOptions::default();
    for (name, bytes) in images {
        writer.start_file(*name, options).expect("start image");
        writer.write_all(bytes).expect("write image");
    }
    writer.finish().expect("finish zip");
    cursor.into_inner()
}

pub fn create_png_base64(width: u32, height: u32) -> String {
    base64::engine::general_purpose::STANDARD.encode(create_png_bytes(width, height))
}

pub fn create_png_bytes(width: u32, height: u32) -> Vec<u8> {
    let image = RgbaImage::from_pixel(width, height, Rgba([255, 255, 255, 255]));
    let mut cursor = Cursor::new(Vec::new());
    DynamicImage::ImageRgba8(image)
        .write_to(&mut cursor, ImageFormat::Png)
        .expect("failed to encode png");
    cursor.into_inner()
}
