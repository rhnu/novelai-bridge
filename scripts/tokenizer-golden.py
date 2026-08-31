"""Regenerate fixtures against pinned upstream references (no NovelAI code).

Requires: sentencepiece==0.2.2 tokenizers==0.23.1 ftfy==6.3.1 regex.
Run after `cargo xtask fetch-tokenizer-assets` from any working directory.
The CLIP Python reference is fetched from its pinned commit and executed, not vendored.
"""
import gzip
import hashlib
import html.entities
import json
from pathlib import Path
import tomllib
import urllib.request
import sentencepiece
import tokenizers

ROOT = Path(__file__).resolve().parents[1]
ASSETS = ROOT / "crates/tokenizer-assets/assets"
FIXTURES = ROOT / "tests/support/tokenizer"
SOURCES = {a["name"]: a for a in tomllib.loads((ASSETS / "SOURCES.toml").read_text())["asset"]}

# Data, not tokenizer implementation. Python 3.13 HTML5 table; PSF-2.0 attribution in CREDITS.md.
entity_data = json.dumps(html.entities.html5, ensure_ascii=False, sort_keys=True, separators=(",", ":")) + "\n"
assert hashlib.sha256(entity_data.encode()).hexdigest() == "beb2a380afc47242a690b5c60a6c6866fcf79c8e8308dc8436de246a26ae1679", "HTML5 entity data changed; review provenance before updating"
(ROOT / "src/tokenizer/clip/entities.json").write_text(entity_data, encoding="utf-8")


def asset(name):
    spec = SOURCES[name]
    raw = (ASSETS / spec["output"]).read_bytes()
    if spec["output"].endswith(".gz") and not spec.get("upstream_gzip"):
        raw = gzip.decompress(raw)
    assert hashlib.sha256(raw).hexdigest() == spec["sha256"], name
    return raw


def write(name, encode, frame, size, reference):
    previous = json.loads((FIXTURES / f"{name}.json").read_text(encoding="utf-8"))
    inputs = {case["name"]: case["input"] for case in previous["cases"]}
    inputs.update({
        "special_boundaries": "中🙂x<<|im_start|>z<unknown><|audio_start|>尾",
        "near_special": "<<not_a_token>> <<|unknown|> <<< nested >>>",
        "normalization": "é e\u0301 가 가 café",
        "html_named": "&eacute; &copy; &NotEqualTilde; &amp;lt;",
        "long_unicode": "猫🙂 café, 1girl, detailed background. " * 160,
    })
    if name != "clip":
        inputs["compatibility_normalization"] = "ＡＢＣ ﬁ ① ℌ \u200b \u00ad \u00a0 \u3000"
    spec = SOURCES[name]
    output = {"tokenizer": name, "source_url": spec["url"], "revision": spec["revision"],
              "sha256": spec["sha256"], "license": spec["license"],
              "generated_with": reference, "vocabulary_size": size, "cases": []}
    if name == "qwen":
        output["config_sha256"] = SOURCES["qwen-config"]["sha256"]
    for label, text in inputs.items():
        ids = encode(text)
        output["cases"].append({"name": label, "input": text, "content_only": ids, "model_input": frame(ids)})
    (FIXTURES / f"{name}.json").write_text(json.dumps(output, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
    print(f"generated {name}: {len(inputs)} cases, {size} entries")


clip_url = f"https://raw.githubusercontent.com/openai/CLIP/{SOURCES['clip']['revision']}/clip/simple_tokenizer.py"
clip_source = urllib.request.urlopen(clip_url).read()
namespace = {"__file__": str(ASSETS / "simple_tokenizer.py")}
exec(compile(clip_source, clip_url, "exec"), namespace)
asset("clip")
clip = namespace["SimpleTokenizer"](str(ASSETS / SOURCES["clip"]["output"]))
write("clip", clip.encode, lambda ids: [clip.encoder["<|startoftext|>"], *ids, clip.encoder["<|endoftext|>"]],
      len(clip.encoder), f"{clip_url}; ftfy 6.3.1; source sha256 {hashlib.sha256(clip_source).hexdigest()}")

t5 = sentencepiece.SentencePieceProcessor(model_proto=asset("t5"))
write("t5", t5.encode, lambda ids: [*ids, t5.eos_id()], t5.vocab_size(), f"sentencepiece {sentencepiece.__version__}")

qwen = tokenizers.Tokenizer.from_str(asset("qwen").decode())
config = json.loads(asset("qwen-config"))
for expected, entry in sorted(config["added_tokens_decoder"].items(), key=lambda pair: int(pair[0])):
    qwen.add_special_tokens([tokenizers.AddedToken(**entry)])
    assert qwen.token_to_id(entry["content"]) == int(expected)
write("qwen", lambda text: qwen.encode(text, add_special_tokens=False).ids, lambda ids: ids,
      qwen.get_vocab_size(), f"tokenizers {tokenizers.__version__}, tokenizer.json plus config added_tokens_decoder")
