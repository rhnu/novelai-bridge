use crate::{
    PromptAnalysis, PromptDiagnostic, PromptDiagnosticCode, PromptDiagnosticSeverity,
    PromptSemanticKind, PromptSemanticSpan, PromptSyntaxProfile, PromptToken, PromptTokenKind,
    PromptWeight, lex::tokenize,
};

/// Analyzes an editable prompt without changing its source or choosing random branches.
///
/// Unbalanced emphasis operators are valid. No prompt content is logged.
#[must_use]
pub fn analyze_prompt(source: &str, profile: PromptSyntaxProfile) -> PromptAnalysis {
    let tokens = tokenize(source, profile);
    let mut analysis = PromptAnalysis::default();
    let mut weight = PromptWeight::ONE;
    let mut random: Option<RandomBranch> = None;
    for token in &tokens {
        use PromptTokenKind as K;
        let previous_weight = weight;
        match token.kind {
            K::Strengthen => weight = weight.scale(1.05),
            K::Weaken => weight = weight.scale(1.0 / 1.05),
            K::NumericWeight => apply_numeric(token, profile, &mut weight, &mut analysis),
            K::WeightReset if profile.numeric_weights => weight = PromptWeight::ONE,
            K::WeightReset => diagnostic(
                &mut analysis,
                token,
                PromptDiagnosticCode::UnsupportedNumericWeight,
            ),
            K::RandomOpen => {
                random = Some(RandomBranch {
                    entry: weight,
                    joined: None,
                    from: token.from,
                });
            },
            K::RandomSeparator => {
                if let Some(branch) = &mut random {
                    branch.join(weight);
                    weight = branch.entry;
                }
            },
            K::RandomClose => {
                if let Some(mut branch) = random.take() {
                    branch.join(weight);
                    weight = branch.joined.unwrap_or(weight);
                }
            },
            K::CharacterSeparator | K::MixSeparator => weight = PromptWeight::ONE,
            K::MixWeight if token.value.is_none_or(|value| !value.is_finite()) => {
                diagnostic(&mut analysis, token, PromptDiagnosticCode::NonFiniteWeight);
            },
            K::UncertainText => {
                diagnostic(
                    &mut analysis,
                    token,
                    PromptDiagnosticCode::UncertainTextRendering,
                );
                analysis.spans.push(PromptSemanticSpan {
                    from: token.from,
                    to: token.to,
                    kind: PromptSemanticKind::UncertainText,
                    weight: PromptWeight::Unknown,
                });
            },
            K::Text | K::Escape | K::RenderedText | K::Interaction => {
                let kind = if token.kind == K::RenderedText {
                    PromptSemanticKind::RenderedText
                } else {
                    PromptSemanticKind::Text
                };
                analysis.spans.push(PromptSemanticSpan {
                    from: token.from,
                    to: token.to,
                    kind,
                    weight,
                });
            },
            _ => {},
        }
        if weight == PromptWeight::Unknown && previous_weight != PromptWeight::Unknown {
            // Report only the operator that first makes arithmetic unrepresentable.
            diagnostic(&mut analysis, token, PromptDiagnosticCode::NonFiniteWeight);
        }
    }
    if let Some(branch) = random {
        analysis.diagnostics.push(PromptDiagnostic {
            from: branch.from,
            to: tokens.last().map_or(branch.from, |token| token.to),
            code: PromptDiagnosticCode::UnclosedRandomizer,
            severity: PromptDiagnosticSeverity::Warning,
            message: "Randomizer has no closing double bar.".to_owned(),
        });
    }
    analysis.tokens = tokens;
    analysis
}

struct RandomBranch {
    entry: PromptWeight,
    joined: Option<PromptWeight>,
    from: u32,
}

impl RandomBranch {
    fn join(&mut self, weight: PromptWeight) {
        self.joined = Some(self.joined.map_or(weight, |joined| joined.join(weight)));
    }
}

fn apply_numeric(
    token: &PromptToken,
    profile: PromptSyntaxProfile,
    weight: &mut PromptWeight,
    analysis: &mut PromptAnalysis,
) {
    if !profile.numeric_weights {
        diagnostic(
            analysis,
            token,
            PromptDiagnosticCode::UnsupportedNumericWeight,
        );
    } else if token.value.is_none_or(|value| !value.is_finite()) {
        *weight = PromptWeight::Unknown;
    } else if let Some(value) = token.value {
        if value < 0.0 && !profile.negative_weights {
            diagnostic(
                analysis,
                token,
                PromptDiagnosticCode::UnsupportedNegativeWeight,
            );
        } else {
            *weight = PromptWeight::Exact { value };
        }
    }
}

fn diagnostic(analysis: &mut PromptAnalysis, token: &PromptToken, code: PromptDiagnosticCode) {
    let message = match code {
        PromptDiagnosticCode::UnsupportedNumericWeight => {
            "Numeric emphasis is unsupported by this profile."
        },
        PromptDiagnosticCode::UnsupportedNegativeWeight => {
            "Negative emphasis is unsupported by this profile."
        },
        PromptDiagnosticCode::NonFiniteWeight => "Emphasis exceeds finite numeric representation.",
        PromptDiagnosticCode::UnclosedRandomizer => "Randomizer has no closing double bar.",
        PromptDiagnosticCode::UncertainTextRendering => {
            "Randomizer alternatives disagree about whether subsequent text is rendered literally."
        },
    };
    analysis.diagnostics.push(PromptDiagnostic {
        from: token.from,
        to: token.to,
        code,
        severity: PromptDiagnosticSeverity::Warning,
        message: message.to_owned(),
    });
}

#[cfg(test)]
mod tests;
