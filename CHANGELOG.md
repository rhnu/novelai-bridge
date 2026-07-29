# Changelog

## 0.3.1

- Validate complete PNG image data through `IEND` before reporting parsed metadata.
- Read PNG text chunks that appear after image data.
- Preserve bounded `zTXt` / `iTXt` decoding failures as structured metadata warnings.

## 0.3.0

- Change `Client::generate` to return `GenerateImageResult`, separating the request-level
  `resolved_seed` from individual images.
- Parse every returned PNG independently and derive per-image seeds only from embedded metadata.
- Add `GeneratedImageMetadata` and `inspect_generated_image_metadata` for non-failing, diagnosable
  output metadata inspection.
- Change `Client::generate_stream` to return `GenerateImageStream`, which exposes the request-level
  `resolved_seed` and continues to implement `Stream`.
- Add serialization support and generation-data inspection helpers for parsed PNG metadata.
- Preserve recoverable metadata diagnostics such as malformed `Comment` JSON in
  `PngMetadata::warnings`.

### Migration

- Replace direct iteration over `client.generate(...).await?` with iteration over
  `client.generate(...).await?.images`.
- Read the actual request seed from `GenerateImageResult::resolved_seed`.
- Replace `GeneratedImage::seed` field access with `GeneratedImage::seed()`. It now returns only an
  embedded per-image seed and never falls back to the request seed.
- Existing `StreamExt` consumers can continue polling the value returned by `generate_stream`;
  read its `resolved_seed` field before or while consuming chunks.
