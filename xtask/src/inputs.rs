use std::{io::Cursor, path::Path};

use anyhow::Context as _;
use novelai_bridge::{EncodedImage, ImageDecodePolicy, ImageLimit, ImageSize};

use crate::{RunArgs, TaskResult};

pub fn source(args: &RunArgs, size: ImageSize) -> TaskResult<EncodedImage> {
    if let Some(path) = &args.image {
        return read_image(path);
    }
    validate_synthetic_canvas(size)?;
    let image = image::RgbImage::from_fn(size.width, size.height, |horizontal, vertical| {
        let red = u8::try_from(u64::from(horizontal) * 255 / u64::from(size.width)).unwrap_or(255);
        let green = u8::try_from(u64::from(vertical) * 255 / u64::from(size.height)).unwrap_or(255);
        image::Rgb([red, green, 128])
    });
    synthesized(args, "source", image)
}

pub fn mask(args: &RunArgs, size: ImageSize) -> TaskResult<EncodedImage> {
    if let Some(path) = &args.mask {
        return read_image(path);
    }
    validate_synthetic_canvas(size)?;
    let horizontal = size.width / 4..size.width * 3 / 4;
    let vertical = size.height / 4..size.height * 3 / 4;
    let image = image::RgbImage::from_fn(size.width, size.height, |column, row| {
        let value = if horizontal.contains(&column) && vertical.contains(&row) {
            255
        } else {
            0
        };
        image::Rgb([value, value, value])
    });
    synthesized(args, "mask", image)
}

fn synthesized(args: &RunArgs, name: &str, image: image::RgbImage) -> TaskResult<EncodedImage> {
    let mut output = Cursor::new(Vec::new());
    image::DynamicImage::ImageRgb8(image).write_to(&mut output, image::ImageFormat::Png)?;
    let bytes = output.into_inner();
    if args.save_inputs {
        let directory = args.out.as_ref().context("--save-inputs requires --out")?;
        std::fs::create_dir_all(directory)
            .with_context(|| format!("create output directory {}", directory.display()))?;
        let path = directory.join(format!("input-{name}.png"));
        std::fs::write(&path, &bytes)
            .with_context(|| format!("write synthesized input {}", path.display()))?;
        println!("wrote {}", path.display());
    }
    Ok(EncodedImage::from_bytes(bytes)?)
}

fn read_image(path: &Path) -> TaskResult<EncodedImage> {
    Ok(EncodedImage::from_bytes(
        std::fs::read(path).with_context(|| format!("read source image {}", path.display()))?,
    )?)
}

fn validate_synthetic_canvas(size: ImageSize) -> TaskResult<()> {
    anyhow::ensure!(
        size.width > 0 && size.height > 0,
        "synthetic canvas dimensions must be nonzero"
    );
    let policy = ImageDecodePolicy::default();
    policy.check_dimensions(size.width, size.height)?;
    policy.check(
        ImageLimit::DecodedBytes,
        size.pixels()
            .checked_mul(3)
            .context("synthetic canvas byte count overflow")?,
    )?;
    Ok(())
}
