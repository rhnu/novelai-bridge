# NovelAI Anlas estimation formula

> Source note: The coefficients and pricing rules below were obtained from NovelAI's official
> [image-generation website](https://novelai.net/image) on 2026-08-21. NovelAI may change its
> pricing, so this document is an estimate for that retrieval date rather than a billing guarantee.

This document describes the generation-cost estimate used by `novelai-bridge` for the current NAI
3, NAI 4, NAI 4.5, and NAI 5 model families. Legacy pricing paths are outside the scope of this
formula. Director and upscale operations use a separate pricing rule.

## Per-image cost

Let:

- `P = width × height`, the output pixel count;
- `steps`, the requested sampling step count;
- `strength`, the image-to-image or inpainting strength, defaulting to `1.0`.

The base cost is:

$$
\text{base} = \left\lceil
2.951823174884865 \times 10^{-6} \times P
+ 5.753298233447344 \times 10^{-7} \times P \times \text{steps}
\right\rceil
$$

The estimator then applies the SMEA and model multipliers:

$$
\text{adjusted\_base}
= \text{base} \times \text{smea\_multiplier} \times \text{model\_multiplier}
$$

Finally, the price of one image is:

$$
\text{per\_image}
= \max\left(\left\lceil
\text{adjusted\_base} \times \text{strength}
\right\rceil, 2\right)
$$

The rounding order is significant: round `base` first, apply the multipliers and strength, then
round the final per-image value.

### SMEA multiplier

| `smea` | `smea_dyn` | Multiplier |
| ------ | ---------- | ---------: |
| false  | false      | 1.0        |
| true   | false      | 1.2        |
| true   | true       | 1.4        |
| false  | true       | 1.0        |

### Model multiplier

| Model family | Multiplier |
| ------------ | ---------: |
| NAI 3, NAI 4, NAI 4.5 | 1.0 |
| NAI 5 | 1.5 |

The sampler does not change this formula for these model families. It remains part of the request
shape for API compatibility.

### Strength selection

- Text-to-image uses `1.0`.
- Image-to-image without an inpainting mask uses `img2img_strength`.
- Inpainting uses `inpaint_img2img_strength` when supplied, otherwise `1.0`.
- A non-finite strength is treated as `1.0`.

If `per_image` exceeds 140 Anlas, the estimate is marked as too expensive because the web client
does not accept that per-image price.

## Number of priced images

The requested image count is first clamped to the current generation API range:

$$
\text{api\_samples} = \operatorname{clamp}(n_{\text{samples}}, 1, 4)
$$

The resolution-dependent pricing limit is:

$$
\text{resolution\_limit}(P) =
\begin{cases}
8 & P \le 360{,}448 \\
6 & P \le 409{,}600 \\
4 & P \le 3{,}145{,}728 \\
0 & \text{otherwise}
\end{cases}
$$

The number retained for generation and per-image surcharges is:

$$
\text{priced\_samples}
= \min(\text{api\_samples}, \text{resolution\_limit}(P))
$$

Because the current API accepts at most four images, the effective cap is four for supported
resolutions up to 3,145,728 pixels. A larger canvas produces zero priced samples.

## Opus first-image allowance

One priced image is free when all of the following conditions hold:

- the account has `tier >= 3` and an active subscription;
- the allowance has not already been used for the request context;
- there are no chargeable character references;
- `P <= 1,048,576`;
- `steps <= 28`;
- `priced_samples > 0`;
- for a model with a V5 usage limit, the V5 usage allowance is not negative.

Let `free_first_image` be `1` when these conditions hold and `0` otherwise. Then:

$$
\text{billable\_samples}
= \max(\text{priced\_samples} - \text{free\_first\_image}, 0)
$$

The allowance reduces only the generation-image count. Character-reference and vibe-related
surcharges are calculated separately.

## Additional charges

The generation component is:

$$
\text{generation} = \text{per\_image} \times \text{billable\_samples}
$$

When character references are supported for the model and request, their surcharge is:

$$
\text{character\_reference}
= 5 \times \text{chargeable\_character\_references}
\times \text{priced\_samples}
$$

The chargeable count is zero when the model does not support character references, or when an
inpainting request uses a model that does not support character references with an inpaint mask.

For models that support encoded vibe references:

$$
\text{vibe\_encoding} = 2 \times \text{pending\_vibe\_encodes}
$$

$$
\text{vibe\_reference\_overage}
= 2 \times \max(\text{vibe\_reference\_count} - 4, 0)
$$

For unsupported models, both vibe charges are zero.

## Total estimate

The final estimate is:

$$
\text{total}
= \text{generation}
+ \text{character\_reference}
+ \text{vibe\_encoding}
+ \text{vibe\_reference\_overage}
$$

The estimator exposes these components separately so callers can display a breakdown and can
distinguish a zero-cost allowance from a request rejected because its per-image price is above the
web client's limit.
