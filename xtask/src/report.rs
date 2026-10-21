use anyhow::Context as _;
use novelai_bridge::{GenerateImageResult, GeneratedImage, GeneratedImageMetadata};

use crate::{RunArgs, TaskResult};

pub fn generation(result: &GenerateImageResult, args: &RunArgs) -> TaskResult<String> {
    Ok(format!(
        "request seed {}, {}",
        result.resolved_seed,
        images(&result.images, args)?
    ))
}

pub fn images(images: &[GeneratedImage], args: &RunArgs) -> TaskResult<String> {
    let mut lines = vec![format!("{} image(s)", images.len())];
    for (index, image) in images.iter().enumerate() {
        let metadata = match image.metadata() {
            GeneratedImageMetadata::Parsed(parsed) => format!(
                "parsed (generation data: {}, {} warning(s))",
                parsed.has_generation_data(),
                parsed.warnings.len()
            ),
            GeneratedImageMetadata::NotPresent => "none embedded".to_owned(),
            GeneratedImageMetadata::UnsupportedFormat => "unsupported format".to_owned(),
            GeneratedImageMetadata::Invalid { .. } => "invalid".to_owned(),
        };
        lines.push(format!(
            "  [{index}] {} bytes, mime {:?}, embedded seed {:?}, metadata {metadata}",
            image.bytes().len(),
            image.mime_type(),
            image.seed(),
        ));
        if let Some(directory) = &args.out {
            let extension = image.format().extension();
            std::fs::create_dir_all(directory)
                .with_context(|| format!("create output directory {}", directory.display()))?;
            let path = directory.join(format!("probe-{index}.{extension}"));
            std::fs::write(&path, image.bytes())
                .with_context(|| format!("write generated image {}", path.display()))?;
            lines.push(format!("       wrote {}", path.display()));
        }
    }
    Ok(lines.join("\n"))
}
