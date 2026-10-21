use std::io::Cursor;
use crate::EncodedImage;

pub(crate) fn image() -> EncodedImage {
    let mut output = Cursor::new(Vec::new());
    ::image::DynamicImage::new_rgba8(2, 2).write_to(&mut output, ::image::ImageFormat::Png).unwrap();
    EncodedImage::from_bytes(output.into_inner()).unwrap()
}
