use futures_util::StreamExt;
use novelai_bridge::{
    CharacterReference, CharacterReferenceType, Client, EncodeVibeRequest, GenerateImageRequest,
    GenerateImageStreamRequest, GenerationDelivery, ImageSize, Img2ImgRequest, Model, ReferenceSet,
    StreamMode, StreamingTransport, TransportMethod, TransportRequest, VibeReference,
    VibeTransferConfig, DEFAULT_IMAGE_BASE,
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

pub fn shape(args: &RunArgs, model: Model, size: ImageSize, edits: &RequestEdits) -> TaskResult<()> {
    let (path, body, method) = match args.preset {
        Preset::Subscription => ("/user/subscription", None, TransportMethod::Get),
        Preset::Vibe => (
            "/ai/encode-vibe",
            Some(encode_vibe_request(args, model, size)?.prepare()?.encode()?),
            TransportMethod::Post,
        ),
        _ => {
            let delivery = if args.preset == Preset::Stream {
                GenerationDelivery::Stream(StreamMode::default())
            } else {
                GenerationDelivery::Images
            };
            let path = if args.preset == Preset::Stream {
                "/ai/generate-image-stream"
            } else {
                "/ai/generate-image"
            };
            (path, Some(generation_request(args, model, size)?.normalize()?.prepare()?.encode(delivery)?), TransportMethod::Post)
        }
    };
    edits.apply(TransportRequest {
        method,
        endpoint: format!("{DEFAULT_IMAGE_BASE}{path}"),
        accept: None,
        body,
    })?;
    Ok(())
}

pub async fn execute<T: StreamingTransport + ?Sized>(
    client: &Client<T>, args: &RunArgs, model: Model, size: ImageSize,
) -> TaskResult<String> {
    match args.preset {
        Preset::Subscription => Ok(render::subscription_summary(&client.get_subscription().await?)),
        Preset::Stream => {
            let stream = client.generate_stream(GenerateImageStreamRequest {
                base: generation_request(args, model, size)?,
                stream: StreamMode::default(),
            }).await?;
            let resolved_seed = stream.resolved_seed;
            let mut chunks = 0_usize;
            let mut inner = stream.into_inner();
            while let Some(item) = inner.next().await {
                let chunk = item?;
                chunks += 1;
                if chunks <= 3 {
                    println!("  chunk {chunks} type={} samp={} step={:?}", chunk.event_type, chunk.samp_ix, chunk.step_ix);
                }
            }
            Ok(format!("seed {resolved_seed}, {chunks} chunk(s)"))
        }
        Preset::Vibe => {
            let encoded = client.encode_vibe(encode_vibe_request(args, model, size)?).await?;
            let mut request = base_request(args, model, size);
            request.references = Some(ReferenceSet {
                vibe_transfer: Some(VibeTransferConfig {
                    references: vec![VibeReference { vibe_data_cache: encoded, strength: 0.6 }],
                    strength: 1.0,
                }),
                character_references: Vec::new(),
            });
            report::generation(&client.generate(request).await?, args)
        }
        _ => report::generation(&client.generate(generation_request(args, model, size)?).await?, args),
    }
}

fn encode_vibe_request(args: &RunArgs, model: Model, size: ImageSize) -> TaskResult<EncodeVibeRequest> {
    Ok(EncodeVibeRequest {
        image: inputs::source(args, size)?,
        information_extracted: 1.0,
        model,
        strict_mode: args.strict,
    })
}

fn generation_request(args: &RunArgs, model: Model, size: ImageSize) -> TaskResult<GenerateImageRequest> {
    let mut request = base_request(args, model, size);
    match args.preset {
        Preset::I2i | Preset::Infill => {
            let mut input = Img2ImgRequest::new(inputs::source(args, size)?, args.strength, 0.0);
            if args.preset == Preset::Infill {
                input.mask = Some(inputs::mask(args, size)?);
            }
            request.img2img = Some(input);
        }
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
        }
        _ => {}
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
