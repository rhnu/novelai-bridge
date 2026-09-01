//! Request presets and response reporting.

use std::{io::Cursor, path::Path};

use base64::{Engine, engine::general_purpose::STANDARD};
use futures_util::StreamExt;
use novelai_bridge::{
    BridgeError, CharacterReference, CharacterReferenceType, Client, EncodeVibeRequest,
    EncodedImage, GenerateImageRequest, GenerateImageStreamRequest, GeneratedImage,
    GeneratedImageMetadata, ImageSize, Img2ImgRequest, Model, StreamMode, StreamingTransport,
    Transport, VibeReference, VibeTransferConfig,
};

use crate::{Preset, RunArgs, render};

/// Prints the capability registry for every model the crate knows about.
pub fn print_capabilities() {
    for model in Model::ALL {
        let descriptor = model.descriptor();
        println!("{}", model.as_str());
        println!("  inpainting id       {:?}", model.inpainting_id());
        println!("  vibe profile        {:?}", descriptor.support().vibe);
        println!("  can encode vibe     {}", descriptor.can_encode_vibe());
        println!(
            "  character reference {:?}",
            descriptor.support().character_reference
        );
        println!("  wire profile        {:?}", descriptor.wire());
        println!(
            "  params_version      {}",
            descriptor.wire().params_version()
        );
        let defaults = descriptor.defaults();
        println!(
            "  factory sampling    steps {} / scale {}",
            defaults.sampling.steps, defaults.sampling.scale
        );
        println!(
            "  max characters      {}",
            descriptor.prompt().max_characters()
        );
        println!("  prompt tokens       {}", descriptor.prompt().token_limit);
        println!(
            "  tokenizer           {}",
            descriptor.prompt().tokenizer.as_str()
        );
        println!("  sampling support    {:?}", descriptor.sampling());
        println!("  pricing profile     {:?}", descriptor.pricing());
        println!("  streaming           {}", descriptor.support().streaming);
        if let Some(sigma) = descriptor.sampling().variety_sigma(832, 1216) {
            println!("  variety sigma @832x1216  {sigma}");
        }
        println!();
    }
}

/// Builds and runs the selected preset, returning a printable report.
pub async fn execute<T: Transport + StreamingTransport>(
    client: &Client<T>,
    args: &RunArgs,
    model: Model,
    size: ImageSize,
) -> Result<String, BridgeError> {
    match args.preset {
        Preset::Subscription => {
            let info = client.get_subscription().await?;
            Ok(render::subscription_summary(&info))
        }
        Preset::Stream => {
            let request = GenerateImageStreamRequest {
                base: base_request(args, model, size),
                stream: StreamMode::default(),
            };
            let stream = client.generate_stream(request).await?;
            let resolved_seed = stream.resolved_seed;
            let mut chunks = 0_usize;
            let mut inner = stream.into_inner();
            while let Some(item) = inner.next().await {
                let chunk = item?;
                chunks += 1;
                if chunks <= 3 {
                    println!(
                        "  chunk {} type={} samp={} step={:?}",
                        chunks, chunk.event_type, chunk.samp_ix, chunk.step_ix
                    );
                }
            }
            Ok(format!("seed {resolved_seed}, {chunks} chunk(s)"))
        }
        Preset::Vibe => {
            let source = load_or_synthesize_source(args, size)?;
            let encoded = client
                .encode_vibe(EncodeVibeRequest {
                    image: source,
                    information_extracted: 1.0,
                    model,
                    strict_mode: args.strict,
                })
                .await?;
            println!("  encoded vibe: {} base64 chars", encoded.to_base64().len());

            let mut request = base_request(args, model, size);
            request.references = Some(novelai_bridge::ReferenceSet {
                vibe_transfer: Some(VibeTransferConfig {
                    references: vec![VibeReference {
                        vibe_data_cache: encoded,
                        strength: 0.6,
                    }],
                    strength: 1.0,
                }),
                character_references: Vec::new(),
            });
            report(&client.generate(request).await?, args)
        }
        preset => {
            let mut request = base_request(args, model, size);
            match preset {
                Preset::I2i => {
                    request.img2img = Some(Img2ImgRequest::new(
                        load_or_synthesize_source(args, size)?,
                        args.strength,
                        0.0,
                    ));
                }
                Preset::Infill => {
                    let mut img2img = Img2ImgRequest::new(
                        load_or_synthesize_source(args, size)?,
                        args.strength,
                        0.0,
                    );
                    img2img.mask = Some(load_or_synthesize_mask(args, size)?);
                    request.img2img = Some(img2img);
                }
                Preset::Charref => {
                    request.references = Some(novelai_bridge::ReferenceSet {
                        vibe_transfer: None,
                        character_references: vec![CharacterReference {
                            image: load_or_synthesize_source(args, size)?,
                            reference_type: CharacterReferenceType::Character,
                            fidelity: 0.5,
                            strength: 0.6,
                        }],
                    });
                }
                _ => {}
            }
            report(&client.generate(request).await?, args)
        }
    }
}

fn base_request(args: &RunArgs, model: Model, size: ImageSize) -> GenerateImageRequest {
    let mut builder = GenerateImageRequest::builder(&args.prompt, model)
        .size(size)
        .seed(args.seed)
        .n_samples(args.samples)
        .variety_boost(args.variety)
        .quality(args.quality.into())
        .transparent_background(args.transparent)
        .strict_mode(args.strict);

    if let Some(negative) = args.negative.as_ref() {
        builder = builder.negative_prompt(negative);
    }
    if let Some(steps) = args.steps {
        builder = builder.steps(steps);
    }
    if let Some(scale) = args.scale {
        builder = builder.scale(scale);
    }
    builder.build()
}

fn report(
    result: &novelai_bridge::GenerateImageResult,
    args: &RunArgs,
) -> Result<String, BridgeError> {
    let mut lines = vec![format!(
        "request seed {}, {} image(s)",
        result.resolved_seed,
        result.images.len()
    )];

    for (index, image) in result.images.iter().enumerate() {
        lines.push(format!(
            "  [{index}] {} bytes, mime {:?}, embedded seed {:?}, metadata {}",
            image.bytes.len(),
            image.mime_type,
            image.seed(),
            describe_metadata(&image.metadata)
        ));
        if let Some(dir) = args.out.as_ref() {
            lines.push(format!("       {}", save(dir, index, image)?));
        }
    }

    Ok(lines.join("\n"))
}

fn describe_metadata(metadata: &GeneratedImageMetadata) -> String {
    match metadata {
        GeneratedImageMetadata::Parsed(parsed) => format!(
            "parsed (generation data: {}, {} warning(s))",
            parsed.has_generation_data(),
            parsed.warnings.len()
        ),
        GeneratedImageMetadata::NotPresent => "none embedded".to_owned(),
        GeneratedImageMetadata::UnsupportedFormat => "unsupported format".to_owned(),
        GeneratedImageMetadata::Invalid { .. } => "invalid".to_owned(),
    }
}

fn save(dir: &Path, index: usize, image: &GeneratedImage) -> Result<String, BridgeError> {
    let path = dir.join(format!("probe-{index}.png"));
    std::fs::create_dir_all(dir)
        .and_then(|()| std::fs::write(&path, &image.bytes))
        .map_or_else(
            |error| Ok(format!("could not write {}: {error}", path.display())),
            |()| Ok(format!("wrote {}", path.display())),
        )
}

fn load_or_synthesize_source(args: &RunArgs, size: ImageSize) -> Result<EncodedImage, BridgeError> {
    args.image.as_ref().map_or_else(
        || EncodedImage::from_base64(&save_input(args, "source", encode_png(gradient(size)))),
        |path| read_image(path),
    )
}

fn load_or_synthesize_mask(args: &RunArgs, size: ImageSize) -> Result<EncodedImage, BridgeError> {
    args.mask.as_ref().map_or_else(
        || EncodedImage::from_base64(&save_input(args, "mask", encode_png(center_mask(size)))),
        |path| read_image(path),
    )
}

/// Writes a synthesized input alongside the outputs so results can be diffed against it.
fn save_input(args: &RunArgs, name: &str, data_url: String) -> String {
    if !args.save_inputs {
        return data_url;
    }
    let Some(dir) = args.out.as_ref() else {
        println!("  ! --save-inputs needs --out, skipping");
        return data_url;
    };

    let payload = data_url
        .split_once(',')
        .map_or(data_url.as_str(), |(_, b)| b);
    let path = dir.join(format!("input-{name}.png"));
    match STANDARD
        .decode(payload)
        .map_err(|error| error.to_string())
        .and_then(|bytes| {
            std::fs::create_dir_all(dir)
                .and_then(|()| std::fs::write(&path, bytes))
                .map_err(|error| error.to_string())
        }) {
        Ok(()) => println!("  wrote {}", path.display()),
        Err(error) => println!("  ! could not write {}: {error}", path.display()),
    }
    data_url
}

fn read_image(path: &Path) -> Result<EncodedImage, BridgeError> {
    let bytes = std::fs::read(path).map_err(|source| {
        novelai_bridge::TransportError::new(
            novelai_bridge::TransportOperation::SendRequest,
            Some(path.display().to_string()),
            Box::new(source),
        )
    })?;
    EncodedImage::from_bytes(bytes)
}

/// A diagonal gradient, so a returned image visibly differs from its source.
fn gradient(size: ImageSize) -> image::RgbImage {
    image::RgbImage::from_fn(size.width, size.height, |x, y| {
        let u = u8::try_from(x * 255 / size.width.max(1)).unwrap_or(255);
        let v = u8::try_from(y * 255 / size.height.max(1)).unwrap_or(255);
        image::Rgb([u, v, 128])
    })
}

/// White marks the region to repaint; `NovelAI` treats black as untouched.
fn center_mask(size: ImageSize) -> image::RgbImage {
    let (x0, x1) = (size.width / 4, size.width * 3 / 4);
    let (y0, y1) = (size.height / 4, size.height * 3 / 4);
    image::RgbImage::from_fn(size.width, size.height, |x, y| {
        if (x0..x1).contains(&x) && (y0..y1).contains(&y) {
            image::Rgb([255, 255, 255])
        } else {
            image::Rgb([0, 0, 0])
        }
    })
}

fn encode_png(buffer: image::RgbImage) -> String {
    let mut bytes = Vec::new();
    let _ = image::DynamicImage::ImageRgb8(buffer)
        .write_to(&mut Cursor::new(&mut bytes), image::ImageFormat::Png);
    format!("data:image/png;base64,{}", STANDARD.encode(&bytes))
}
