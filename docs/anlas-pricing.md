# NovelAI Anlas estimation formula

> Source note: The coefficients and pricing rules below were obtained from NovelAI's official
> [image-generation website](https://novelai.net/image) on 2026-08-21. NovelAI may change its
> pricing, so this document is an estimate for that retrieval date rather than a billing guarantee.

This document describes the generation-cost estimate used by `novelai-bridge` for the current NAI
3, NAI 4, NAI 4.5, and NAI 5 model families. Legacy pricing paths are outside the scope of this
formula. Director and upscale operations use a separate pricing rule.

The independent `estimate_anlas_cost(AnlasEstimateInput)` function remains available without the
`generation` feature. Request and client helpers first normalize the editable request using its
`ValidationPolicy`, then project its effective values into that same formula. A normalized plan's
pricing helpers reuse the plan directly. The selected `GenerationInput` supplies one operation;
its source or inpainting strength is projected into the corresponding `PricingMode`.

The independent calculator evaluates pricing fields directly. Request helpers reject non-finite
values and apply model capabilities and range policy before estimating.

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

The sampler does not change this formula for these model families. It belongs to the generation request and is absent from the independent pricing input.

### Strength selection

- `PricingMode::TextToImage` uses `1.0`.
- `PricingMode::ImageToImage { strength }` uses the source-image strength.
- `PricingMode::Inpainting { strength }` uses the masked-generation strength.
- A non-finite strength is treated as `1.0`.

If `per_image` exceeds 140 Anlas, the estimate is marked as too expensive because the web client
does not accept that per-image price.

## Number of priced images

The requested image count is first clamped to the current generation API range:

$$
\text{api\_samples} = \operatorname{clamp}(n_{\text{samples}}, 1, 4)
$$

The effective resolution limit is the selected model's sample maximum when `P` is within its
pricing pixel ceiling, and zero otherwise:

$$
\text{resolution\_limit}(P) =
\begin{cases}
\text{model\_sample\_maximum} & P \le \text{model\_pixel\_ceiling} \\
0 & \text{otherwise}
\end{cases}
$$

The number retained for generation and per-image surcharges is:

$$
\text{priced\_samples}
= \min(\text{api\_samples}, \text{resolution\_limit}(P))
$$

Current model descriptors cap generation at four images. Requests beyond the pricing pixel
ceiling produce zero priced samples in the independent calculator; a zero total in that case
does not mean the request qualifies for free generation. Editable generation requests also apply
the descriptor's per-axis canvas constraints during normalization.

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

## V5 Opus generation allowance

> Source note: measured on 2026-08-27 against a live Opus account (`tier: 3`, `active: true`).

For a model whose `descriptor().pricing().opus_allowance` is `OpusAllowance::Metered` — currently
the NAI 5 family — the free first image is paid for out of a separate allowance rather than Anlas.
`GET /user/subscription` reports it as a `usage` object:

```json
"usage": { "percent": 100, "purchasedPercent": 0, "isNegative": false, "timeUntilNextPercent": 7888 }
```

The object is absent for accounts that do not receive the allowance, which is why
`SubscriptionInfo::v5_usage` is optional. A free-tier account (`tier: 0`) returns no
`usage` key at all.

The current [official schema](https://image.novelai.net/docs/doc.json) defines integer
`percent` values that can exceed 100 and a separate `purchasedPercent`. The library exposes
these as `percent` and `purchased_percent` without a display cap; a missing purchase field
means zero. Negative, fractional and incorrectly typed percentages are rejected.
`SubscriptionInfo::raw_response` retains all response fields, including unknown extensions.
Its Debug summary omits this JSON because it can contain private account and payment data.

Field behaviour (the measured examples below predate purchased allowance):

- `percent` is the **remaining** allowance as a whole percentage, not the amount consumed.
  Values above 100 remain available to callers; apply a cap only when displaying a bounded meter.
- `purchasedPercent` is a separate purchased allowance, exposed as `purchased_percent`.
- `isNegative` marks the allowance as overdrawn. It is the only field the estimator reads: an
  overdrawn allowance removes the free first image for models that carry the usage limit, which
  is exactly the condition in the section above.
- `timeUntilNextPercent` is a **rate constant** — the number of seconds needed to recharge one
  percent. The official schema also allows zero when refill is paused above the cap.
  It is not a countdown: it held at `7888` across eight samples over ~35 minutes,
  including across a `percent` change. At that rate one percent returns in ~2h11m and a full
  0 → 100 recharge takes ~9.1 days. Present it as a rate, never as a deadline.

Measured consumption, from twelve consecutive `nai-diffusion-5-full` generations at 832×1216 and
23 steps with `n_samples: 1`:

| Observation | Result |
| ----------- | ------ |
| Anlas balance | unchanged across all twelve generations |
| `percent` | `100` until the ninth generation, then `99` for the rest |
| `isNegative` | `false` throughout |
| `timeUntilNextPercent` | `7888` throughout |

So one such generation costs roughly 0.11%–0.12% of the allowance — about 8–9 generations per
percent, or on the order of 800–900 free generations at a full meter. The crate does not model
this drawdown: a caller pricing several requests in one run gets one free image per request, which
holds until the allowance is exhausted. Callers that batch heavily should read `percent` and warn
when the allowance is nearly spent.

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

Pending encoding calls are priced when the model implements encode-vibe. The active-reference
overage additionally follows reference availability for the selected operation and the
character-reference precedence rule. For unsupported models, both vibe charges are zero.
Normalized generation requests already contain encoded vibe data, so their projected
`pending_vibe_encodes` is zero.

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
