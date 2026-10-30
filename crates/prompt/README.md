# novelai-bridge-prompt

Lossless NovelAI image-prompt analysis for native Rust and `wasm32-unknown-unknown`.
The crate depends only on Serde at runtime: it has no SDK, network, image, tokenizer,
filesystem or randomness dependency. Optional `typegen` derives `ts_rs::TS` for the
complete public contract; callers choose where and when to export declarations.

```rust
use novelai_bridge_prompt::{analyze_prompt, PromptSyntaxProfile};

// Applications should obtain this profile from novelai-bridge's model descriptor.
let profile = PromptSyntaxProfile {
    numeric_weights: true,
    randomizer: true,
    ..Default::default()
};
let analysis = analyze_prompt("||1.5::|0.5::|::||rain::", profile);
assert!(analysis.diagnostics.is_empty());
```

The full SDK re-exports this API and owns the model-to-profile mapping through
`model.descriptor().prompt().syntax`. This crate knows capabilities, not model names.
The default profile enables no syntax and treats input as ordinary text. SDK-supported
NovelAI models enable bracket emphasis; numeric syntax on bracket-only profiles produces
unsupported-feature warnings, while plain/default profiles produce no such warnings.

## Analysis contract

`PromptAnalysis` contains lossless tokens, text semantic spans and typed warning
diagnostics. Every `from`/`to` is a half-open UTF-16 code-unit range in the original
input, suitable for browser editors. Concatenating token `text` reproduces the input,
including escapes, whitespace and unknown text. Ordinary quotation marks do not
create string literals. Unmatched emphasis brackets are valid operators.

`{` and `]` multiply subsequent emphasis by 1.05; `}` and `[` divide it by 1.05.
Numeric `n::` sets emphasis to `n`, clearing previous bracket/numeric emphasis;
bare `::` resets it to one. Unsupported numeric syntax produces typed warnings
without applying unsupported effects. Values are not rounded into display levels.

Randomizer `||` sections use single-bar alternatives. Each alternative starts from
the same incoming emphasis, and their outgoing weights join into an enclosing
`PromptWeight::Range`. A subsequent reset restores an exact value. Ranges describe
bounds rather than claiming every intermediate value is attainable. Arithmetic
outside finite `f64` representation yields `Unknown` and a warning. The documented
grammar has no nested randomizers; scanning is iterative and linear in source length.

V3 mixing `|` and trailing `:weight` produce `MixSeparator` and `MixWeight` tokens.
Mixing weights describe independent conditioning vectors, so they remain separate
from semantic spans' emphasis weights. Modern single bars delimit character fields
and reset emphasis for the next field. `source#`, `target#` and `mutual#` prefixes
are recognized at word/tag boundaries when the profile enables them.

`Text:` is recognized ignoring ASCII case, except `Text::`, at the start or after
whitespace, comma, period, colon, square/curly brackets or Japanese `、` / `。`.
Field and alternative beginnings are also prompt boundaries. This follows the official
frontend's exported `TEXT_REGEX` boundary rule. The following region is literal rendered
text: brackets, numerical weights, quotes and comment-like text have no emphasis meaning.
Randomizer boundaries remain structural; a modern single bar outside a randomizer starts
another character field. Escapes preserve the escaped character literally.

Rendering state is local to each randomizer alternative. If alternatives disagree about
rendering after their closing delimiter, subsequent text uses `UncertainText` tokens/spans,
an `UncertainTextRendering` warning and `Unknown` emphasis, rather than claiming one
interpretation. If every alternative enters rendering, subsequent text is `RenderedText`.
This conservative analysis follows the documented random substitution and the official
frontend's structural split order; it does not claim to validate undocumented service behavior.

Application extensions such as comments are deliberately absent. A sequential extension
scanner can use `PromptLexicalState::default()` and `advance(rest, previous, profile)`,
which returns the consumed UTF-8 byte length. Skip extension comments before committing
the state. Its `rendering()` and `uncertain_rendering()` distinguish protected literal
text from ambiguous rendering; `in_randomizer()` reports structural state. The analyzer
uses this same state implementation. Pure `is_text_rendering_marker`,
`is_character_separator` and `is_randomizer_delimiter` predicates are also available.
These cursor APIs take the remaining source and preceding Unicode scalar rather than
an external range. Analysis token, span and diagnostic ranges always remain UTF-16.

## Provider references

- [Strengthening and weakening](https://docs.novelai.net/en/image/strengthening-weakening/)
- [V3 prompt mixing](https://docs.novelai.net/en/image/promptmixing/)
- [Character fields and action tags](https://docs.novelai.net/en/image/multiplecharacters/)
- [Randomizer branches](https://docs.novelai.net/en/image/promptrandomizer/)
- [Text rendering](https://docs.novelai.net/en/image/textrendering/)

The analyzer supplies editor semantics; it does not expand randomizers, rewrite prompts
or validate service responses. Model-specific capability selection belongs to the SDK.
