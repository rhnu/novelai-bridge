use std::io::Write;

pub fn tiff(comment: &[u8]) -> Vec<u8> {
    // A little-endian TIFF: IFD0 -> ExifIFD -> Undefined UserComment data.
    let mut bytes =
        b"II\x2a\0\x08\0\0\0\x01\0\x69\x87\x04\0\x01\0\0\0\x1a\0\0\0\0\0\0\0\x01\0\x86\x92\x07\0"
            .to_vec();
    bytes.extend_from_slice(&u32::try_from(comment.len()).unwrap().to_le_bytes());
    bytes.extend_from_slice(&44_u32.to_le_bytes());
    bytes.extend_from_slice(&[0; 4]);
    bytes.extend_from_slice(comment);
    bytes
}

pub fn ascii_comment(json: &str) -> Vec<u8> {
    let mut bytes = b"ASCII\0\0\0".to_vec();
    bytes.extend_from_slice(json.as_bytes());
    bytes
}

pub fn compressed(text: &[u8], codec: u8) -> Vec<u8> {
    match codec {
        0 => {
            let mut encoder =
                flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
            encoder.write_all(text).unwrap();
            encoder.finish().unwrap()
        },
        1 => {
            let mut encoder =
                flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
            encoder.write_all(text).unwrap();
            encoder.finish().unwrap()
        },
        _ => {
            let mut encoder =
                flate2::write::DeflateEncoder::new(Vec::new(), flate2::Compression::default());
            encoder.write_all(text).unwrap();
            encoder.finish().unwrap()
        },
    }
}

pub fn stealth_pixels(compressed: &[u8]) -> (u32, u32, Vec<u8>) {
    let mut payload = b"stealth_pngcomp".to_vec();
    payload.extend_from_slice(&u32::try_from(compressed.len() * 8).unwrap().to_be_bytes());
    payload.extend_from_slice(compressed);
    let width = 37_u32;
    let height = u32::try_from((payload.len() * 8).div_ceil(width as usize)).unwrap() + 2;
    let mut rgba = vec![255; width as usize * height as usize * 4];
    for (byte_index, byte) in payload.iter().enumerate() {
        for shift in 0..8 {
            let bit = byte_index * 8 + shift;
            let x = bit / height as usize;
            let y = bit % height as usize;
            rgba[(y * width as usize + x) * 4 + 3] = 0xFE | ((byte >> (7 - shift)) & 1);
        }
    }
    (width, height, rgba)
}

pub fn png(text: &[(&str, &str)], exif: Option<&[u8]>, stealth: Option<&str>) -> Vec<u8> {
    let (width, height, pixels) = stealth.map_or_else(
        || (2, 2, vec![255; 16]),
        |text| stealth_pixels(&compressed(text.as_bytes(), 0)),
    );
    let mut bytes = Vec::new();
    let mut encoder = png::Encoder::new(&mut bytes, width, height);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    for (key, value) in text {
        encoder
            .add_text_chunk((*key).to_owned(), (*value).to_owned())
            .unwrap();
    }
    let mut writer = encoder.write_header().unwrap();
    if let Some(exif) = exif {
        writer.write_chunk(png::chunk::eXIf, exif).unwrap();
    }
    writer.write_image_data(&pixels).unwrap();
    writer.finish().unwrap();
    bytes
}

pub fn webp(exif: &[u8]) -> Vec<u8> {
    let mut image = Vec::new();
    image::codecs::webp::WebPEncoder::new_lossless(&mut image)
        .encode(&[255; 16], 2, 2, image::ExtendedColorType::Rgba8)
        .unwrap();
    let mut bytes = b"RIFF\0\0\0\0WEBPVP8X\x0a\0\0\0\x08\0\0\0\x01\0\0\x01\0\0".to_vec();
    bytes.extend_from_slice(&image[12..]);
    bytes.extend_from_slice(b"EXIF");
    bytes.extend_from_slice(&u32::try_from(exif.len()).unwrap().to_le_bytes());
    bytes.extend_from_slice(exif);
    if !exif.len().is_multiple_of(2) {
        bytes.push(0);
    }
    let size = u32::try_from(bytes.len() - 8).unwrap().to_le_bytes();
    bytes[4..8].copy_from_slice(&size);
    bytes
}

pub fn jpeg(exif: &[u8]) -> Vec<u8> {
    let mut image = Vec::new();
    image::codecs::jpeg::JpegEncoder::new(&mut image)
        .encode(&[255; 12], 2, 2, image::ExtendedColorType::Rgb8)
        .unwrap();
    let mut bytes = image[..2].to_vec();
    bytes.extend_from_slice(&[0xFF, 0xE1]);
    bytes.extend_from_slice(&u16::try_from(exif.len() + 8).unwrap().to_be_bytes());
    bytes.extend_from_slice(b"Exif\0\0");
    bytes.extend_from_slice(exif);
    bytes.extend_from_slice(&image[2..]);
    bytes
}
