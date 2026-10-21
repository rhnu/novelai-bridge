use std::{io::Cursor, path::Path};

use novelai_bridge::{EncodedImage, ImageSize};

use crate::{RunArgs, TaskResult};

pub fn source(args: &RunArgs, size: ImageSize) -> TaskResult<EncodedImage> {
    if let Some(path) = &args.image {
        return read_image(path);
    }
    let image = image::RgbImage::from_fn(size.width, size.height, |horizontal, vertical| {
        let red = u8::try_from(horizontal * 255 / size.width.max(1)).unwrap_or(255);
        let green = u8::try_from(vertical * 255 / size.height.max(1)).unwrap_or(255);
        image::Rgb([red, green, 128])
    });
    synthesized(args, "source", image)
}

pub fn mask(args: &RunArgs, size: ImageSize) -> TaskResult<EncodedImage> {
    if let Some(path) = &args.mask {
        return read_image(path);
    }
    let horizontal = size.width / 4..size.width * 3 / 4;
    let vertical = size.height / 4..size.height * 3 / 4;
    let image = image::RgbImage::from_fn(size.width, size.height, |column, row| {
        let value = if horizontal.contains(&column) && vertical.contains(&row) { 255 } else { 0 };
        image::Rgb([value, value, value])
    });
    synthesized(args, "mask", image)
}

fn synthesized(args: &RunArgs, name: &str, image: image::RgbImage) -> TaskResult<EncodedImage> {
    let mut output = Cursor::new(Vec::new());
    image::DynamicImage::ImageRgb8(image).write_to(&mut output, image::ImageFormat::Png)?;
    let bytes = output.into_inner();
    if args.save_inputs {
        let directory = args.out.as_ref().ok_or("--save-inputs requires --out")?;
        std::fs::create_dir_all(directory)?;
        let path = directory.join(format!("input-{name}.png"));
        std::fs::write(&path, &bytes)?;
        println!("wrote {}", path.display());
    }
    Ok(EncodedImage::from_bytes(bytes)?)
}

fn read_image(path: &Path) -> TaskResult<EncodedImage> {
    Ok(EncodedImage::from_bytes(std::fs::read(path)?)?)
}
