use futures_util::StreamExt;
use novelai_bridge::{
    ApiEndpoint, CharacterReference, CharacterReferenceType, Client, DEFAULT_IMAGE_BASE,
    EncodeVibeRequest, GenerateImageRequest, GenerateImageStreamRequest, GenerationDelivery,
    GenerationInput, ImageSize, ImageToImageInput, InpaintImageToImage, InpaintingInput, Model,
    ReferenceSet, StreamMode, SuggestTagsRequest, TagLanguage, Transport, UpscaleImageRequest,
    VibeReference, VibeTransferConfig,
};

use crate::{Preset, RunArgs, TaskResult, capture::RequestEdits, inputs, render, report};

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

pub fn shape(
    args: &RunArgs,
    model: Model,
    size: ImageSize,
    edits: &RequestEdits,
) -> TaskResult<()> {
    if args.preset == Preset::Tags {
        let query = tag_query(args, model);
        let mut request = ApiEndpoint::SuggestTags.request(DEFAULT_IMAGE_BASE, None);
        request.endpoint.push('?');
        request.endpoint.push_str(&query.query_string());
        edits.apply(request)?;
        return Ok(());
    }
    let (endpoint, body) = match args.preset {
        Preset::Subscription => (ApiEndpoint::Subscription, None),
        Preset::Upscale => (
            ApiEndpoint::UpscaleImage,
            Some(upscale_request(args, size)?.prepare()?.encode()?),
        ),
        Preset::Vibe => (
            ApiEndpoint::EncodeVibe,
            Some(
                encode_vibe_request(args, model, size)?
                    .prepare()?
                    .encode()?,
            ),
        ),
        _ => {
            let delivery = if args.preset == Preset::Stream {
                GenerationDelivery::Stream(StreamMode::default())
            } else {
                GenerationDelivery::Images
            };
            let endpoint = if args.preset == Preset::Stream {
                ApiEndpoint::GenerateImageStream
            } else {
                ApiEndpoint::GenerateImage
            };
            (
                endpoint,
                Some(
                    generation_request(args, model, size)?
                        .normalize()?
                        .prepare()?
                        .encode(delivery)?,
                ),
            )
        },
    };
    edits.apply(endpoint.request(DEFAULT_IMAGE_BASE, body))?;
    Ok(())
}

pub async fn execute<T: Transport + ?Sized>(
    client: &Client<T>,
    args: &RunArgs,
    model: Model,
    size: ImageSize,
) -> TaskResult<String> {
    match args.preset {
        Preset::Tags => Ok(serde_json::to_string_pretty(
            &client.suggest_tags(tag_query(args, model)).await?,
        )?),
        Preset::Upscale => report::images(
            &client.upscale(upscale_request(args, size)?).await?.images,
            args,
        ),
        Preset::Subscription => Ok(render::subscription_summary(
            &client.get_subscription().await?,
        )),
        Preset::Stream => {
            let stream = client
                .generate_stream(GenerateImageStreamRequest {
                    base: generation_request(args, model, size)?,
                    stream: StreamMode::default(),
                })
                .await?;
            let resolved_seed = stream.resolved_seed;
            let mut chunks = 0_usize;
            let mut inner = stream.into_inner();
            while let Some(item) = inner.next().await {
                let chunk = item?;
                chunks += 1;
                if chunks <= 3 {
                    println!(
                        "  chunk {chunks} type={} samp={:?}",
                        chunk.event_type().as_str(),
                        chunk.sample_index()
                    );
                }
            }
            Ok(format!("seed {resolved_seed}, {chunks} chunk(s)"))
        },
        Preset::Vibe => {
            let encoded = client
                .encode_vibe(encode_vibe_request(args, model, size)?)
                .await?;
            let mut request = base_request(args, model, size);
            request.references = Some(ReferenceSet {
                vibe_transfer: Some(VibeTransferConfig {
                    references: vec![VibeReference {
                        vibe_data_cache: encoded,
                        strength: 0.6,
                    }],
                    strength: 1.0,
                }),
                character_references: Vec::new(),
            });
            report::generation(&client.generate(request).await?, args)
        },
        _ => report::generation(
            &client
                .generate(generation_request(args, model, size)?)
                .await?,
            args,
        ),
    }
}

fn tag_query(args: &RunArgs, model: Model) -> SuggestTagsRequest {
    let mut query = SuggestTagsRequest::new(&args.prompt, model);
    query.language = if args.lang == "jp" {
        TagLanguage::Japanese
    } else {
        TagLanguage::English
    };
    query
}

fn upscale_request(args: &RunArgs, size: ImageSize) -> TaskResult<UpscaleImageRequest> {
    let mut request = UpscaleImageRequest::new(inputs::source(args, size)?);
    if let Some(model) = args.model {
        request.model = model;
    }
    Ok(request)
}

fn encode_vibe_request(
    args: &RunArgs,
    model: Model,
    size: ImageSize,
) -> TaskResult<EncodeVibeRequest> {
    Ok(EncodeVibeRequest {
        image: inputs::source(args, size)?,
        information_extracted: 1.0,
        model,
        policy: args.policy(),
    })
}

fn generation_request(
    args: &RunArgs,
    model: Model,
    size: ImageSize,
) -> TaskResult<GenerateImageRequest> {
    let mut request = base_request(args, model, size);
    match args.preset {
        Preset::I2i => {
            let mut input = ImageToImageInput::new(inputs::source(args, size)?, args.strength, 0.0);
            if args.max_enhance {
                input.max_enhance = true;
                request.core.size = input.image.size();
            }
            request.operation = GenerationInput::ImageToImage(input);
        },
        Preset::Infill => {
            let mut input =
                InpaintingInput::new(inputs::source(args, size)?, inputs::mask(args, size)?);
            input.img2img = Some(InpaintImageToImage::new(args.strength, 0.0));
            request.operation = GenerationInput::Inpainting(input);
        },
        Preset::Charref => {
            request.references = Some(ReferenceSet {
                vibe_transfer: None,
                character_references: vec![CharacterReference {
                    image: inputs::source(args, size)?,
                    reference_type: CharacterReferenceType::Character,
                    fidelity: 0.5,
                    strength: 0.6,
                }],
            });
        },
        _ => {},
    }
    Ok(request)
}

fn base_request(args: &RunArgs, model: Model, size: ImageSize) -> GenerateImageRequest {
    let mut builder = GenerateImageRequest::builder(&args.prompt, model)
        .size(size)
        .seed(args.seed)
        .n_samples(args.samples)
        .variety_boost(args.variety)
        .quality(args.quality.into())
        .transparent_background(args.transparent)
        .policy(args.policy());

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
