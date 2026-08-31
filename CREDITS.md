# Credits

This project was informed by the following open-source projects:

- [`Aaalice_NAI_Launcher`](https://github.com/Aaalice233/Aaalice_NAI_Launcher)
  Referenced for `VibeTransfer` and `Precise Reference` behavior.
- [`novelai-sdk`](https://github.com/caru-ini/novelai-sdk)
  Referenced for overall crate structure and basic architecture.
- [`NekoAI-JS`](https://github.com/Nya-Foundation/NekoAI-JS)
  Referenced for API wrapping approach and request-building structure.

This repository is an independent implementation and is not affiliated with, or endorsed by, the projects above or by NovelAI.

## Tokenizers

- [OpenAI CLIP](https://github.com/openai/CLIP/tree/d05afc436d78f1c48dc0dbf8e5980a9d471f35f6), MIT:
  `clip/bpe_simple_vocab_16e6.txt.gz`, unmodified upstream bytes. The Python `simple_tokenizer.py`
  at the same commit generates reference fixtures; it is not vendored. Rust is an independent
  implementation with a documented ftfy deviation. Copyright (c) 2021 OpenAI; the permission
  notice is retained in `crates/tokenizer-assets/assets/LICENSE-CLIP`.
- [google-t5/t5-base](https://huggingface.co/google-t5/t5-base/tree/a9723ea7f1b39c1eae772870f3b547bf6ef7e6c1),
  Apache-2.0: `spiece.model`, losslessly gzip-wrapped, otherwise unchanged.
  [Google SentencePiece](https://github.com/google/sentencepiece), Apache-2.0, supplies format/
  normalization reference documentation and the Python 0.2.2 golden runtime; no C++ implementation
  was copied into the crate.
- [Qwen/Qwen3.5-35B-A3B](https://huggingface.co/Qwen/Qwen3.5-35B-A3B/tree/59d61f3ce65a6d9863b86d2e96597125219dc754),
  Apache-2.0: `tokenizer.json` and `tokenizer_config.json`, losslessly gzip-wrapped, otherwise unchanged.
  Fixtures use Hugging Face tokenizers 0.23.1 and explicitly install config-only added tokens.
- Asset URLs, exact revisions, SHA-256 and download dates are recorded in
  [SOURCES.toml](crates/tokenizer-assets/assets/SOURCES.toml). Assets are distributed by
  `novelai-bridge-tokenizer-assets`, with both license texts and source notices in its package.
  Apache-2.0 terms are also retained in [LICENSE](LICENSE).
  Binary distributors enabling bundled assets must retain these notices and the CLIP MIT notice.
- `src/tokenizer/clip/entities.json` contains the HTML5 named-entity data from
  [CPython v3.13.2, Lib/html/entities.py](https://github.com/python/cpython/blob/v3.13.2/Lib/html/entities.py),
  PSF-2.0. Modification: serialize only the `html5` mapping as sorted UTF-8 JSON; no Python code is
  embedded. SHA-256: `beb2a380afc47242a690b5c60a6c6866fcf79c8e8308dc8436de246a26ae1679`.
  Copyright (c) 2001-2024 Python Software Foundation; All Rights Reserved.
  Full upstream license: [LICENSE-PYTHON](src/tokenizer/clip/LICENSE-PYTHON).
