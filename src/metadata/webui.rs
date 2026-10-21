use super::{PngMetadata, png::first_chunk_value_ci, value::non_empty_string};
const KEY_PARAMETERS: &str = "parameters";

#[derive(Default)]
struct ParameterFallback {
    prompt: Option<String>,
    negative_prompt: Option<String>,
    seed: Option<i64>,
    sampler: Option<String>,
    steps: Option<u32>,
    scale: Option<f32>,
    width: Option<u32>,
    height: Option<u32>,
    model: Option<String>,
}

pub(super) fn parse_parameters_payload(parsed: &mut PngMetadata) {
    let Some(parameters) = first_chunk_value_ci(&parsed.text_chunks, KEY_PARAMETERS) else {
        return;
    };
    let fallback = parse_webui_parameters(parameters);

    parsed.prompt = parsed.prompt.take().or(fallback.prompt);
    parsed.negative_prompt = parsed.negative_prompt.take().or(fallback.negative_prompt);
    parsed.seed = parsed.seed.take().or(fallback.seed);
    parsed.sampler = parsed.sampler.take().or(fallback.sampler);
    parsed.steps = parsed.steps.take().or(fallback.steps);
    parsed.scale = parsed.scale.take().or(fallback.scale);
    parsed.width = parsed.width.take().or(fallback.width);
    parsed.height = parsed.height.take().or(fallback.height);
    parsed.model = parsed.model.take().or(fallback.model);
}

fn parse_webui_parameters(input: &str) -> ParameterFallback {
    let mut fallback = ParameterFallback::default();
    let input = input.trim();
    if input.is_empty() {
        return fallback;
    }

    let mut prompt_block = input;
    let mut options_line = "";
    if let Some(index) = input.find("\nSteps:") {
        prompt_block = input[..index].trim();
        options_line = input[index + 1..].trim();
    } else if input.starts_with("Steps:") {
        prompt_block = "";
        options_line = input;
    }

    if let Some(index) = prompt_block.find("\nNegative prompt:") {
        fallback.prompt = non_empty_string(&prompt_block[..index]);
        fallback.negative_prompt =
            non_empty_string(&prompt_block[index + "\nNegative prompt:".len()..]);
    } else if let Some(rest) = prompt_block.strip_prefix("Negative prompt:") {
        fallback.negative_prompt = non_empty_string(rest);
    } else {
        fallback.prompt = non_empty_string(prompt_block);
    }

    if options_line.is_empty()
        && let Some(line) = input
            .lines()
            .find(|line| line.trim_start().starts_with("Steps:"))
    {
        options_line = line.trim();
    }

    for segment in options_line.split(',') {
        let mut pair = segment.trim().splitn(2, ':');
        let Some(key) = pair.next() else {
            continue;
        };
        let Some(value) = pair.next() else {
            continue;
        };
        let key = key.trim().to_ascii_lowercase();
        let value = value.trim();
        match key.as_str() {
            "steps" => fallback.steps = value.parse::<u32>().ok(),
            "sampler" => fallback.sampler = non_empty_string(value),
            "cfg scale" | "cfg_scale" | "cfg" => fallback.scale = value.parse::<f32>().ok(),
            "seed" => fallback.seed = value.parse::<i64>().ok(),
            "size" => {
                if let Some((width, height)) = parse_size(value) {
                    fallback.width = Some(width);
                    fallback.height = Some(height);
                }
            },
            "model" => fallback.model = non_empty_string(value),
            _ => {},
        }
    }

    fallback
}

fn parse_size(value: &str) -> Option<(u32, u32)> {
    let mut parts = value.split('x').map(str::trim);
    let width = parts.next()?.parse::<u32>().ok()?;
    let height = parts.next()?.parse::<u32>().ok()?;
    Some((width, height))
}
