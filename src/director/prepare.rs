use super::{DirectorCommand, RunDirectorToolRequest, wire::DirectorRequestBody};
use crate::{
    BridgeError, EncodeError, EncodeTarget, ImagePreparation, ImagePreprocessor, ImageSize,
    InvalidRequest, RequestField, ValidationAdjustment, ValidationPolicy,
    parameters::bounds::normalize_integer,
};

/// A validated director operation with its encoded source image.
pub struct PreparedDirector {
    command: DirectorCommand,
    source_size: ImageSize,
    image_size: ImageSize,
    body: DirectorRequestBody,
    adjustments: Vec<ValidationAdjustment>,
}

impl RunDirectorToolRequest {
    /// Normalizes the command and prepares a reusable payload without network I/O.
    ///
    /// # Errors
    ///
    /// Returns an invalid prompt or numeric parameter error.
    pub fn prepare(self) -> Result<PreparedDirector, BridgeError> { self.prepare_inner(None) }

    /// Prepares a Director source using a caller-provided pixel transformer.
    ///
    /// # Errors
    /// Returns command validation, media-budget or image-preparation failures.
    pub fn prepare_with(
        self,
        preprocessor: &dyn ImagePreprocessor,
    ) -> Result<PreparedDirector, BridgeError> {
        self.prepare_inner(Some(preprocessor))
    }

    fn prepare_inner(
        self,
        preprocessor: Option<&dyn ImagePreprocessor>,
    ) -> Result<PreparedDirector, BridgeError> {
        let mut adjustments = Vec::new();
        let command = match self.command {
            DirectorCommand::Emotion { prompt, defry } => {
                let prompt = prompt.trim().to_owned();
                if prompt.is_empty() {
                    return Err(InvalidRequest::RequiredFieldForContext {
                        context: "emotion".to_owned(),
                        field: RequestField::DirectorPrompt,
                    }
                    .into());
                }
                DirectorCommand::Emotion {
                    prompt,
                    defry: resolve_defry(defry, self.policy, &mut adjustments)?,
                }
            },
            DirectorCommand::Colorize { prompt, defry } => DirectorCommand::Colorize {
                prompt: prompt
                    .map(|text| text.trim().to_owned())
                    .filter(|text| !text.is_empty()),
                defry: resolve_defry(defry, self.policy, &mut adjustments)?,
            },
            command => command,
        };
        let (prompt, defry) = match &command {
            DirectorCommand::Emotion { prompt, defry } => (Some(prompt.clone()), Some(*defry)),
            DirectorCommand::Colorize { prompt, defry } => (prompt.clone(), Some(*defry)),
            _ => (None, None),
        };
        let source_size = self.image.size();
        let operation = ImagePreparation::director(source_size);
        let image =
            crate::image::prepare_image(preprocessor, RequestField::Image, &self.image, operation)?;
        let image_size = image.size();
        let body = DirectorRequestBody {
            req_type: command.wire_name(),
            width: image_size.width,
            height: image_size.height,
            image: image.to_base64(),
            prompt,
            defry,
        };
        Ok(PreparedDirector {
            command,
            source_size,
            image_size,
            body,
            adjustments,
        })
    }
}

impl PreparedDirector {
    /// Numeric changes made under normalize policy.
    #[must_use]
    pub fn adjustments(&self) -> &[ValidationAdjustment] { &self.adjustments }

    /// Effective command used by the payload.
    #[must_use]
    pub const fn command(&self) -> &DirectorCommand { &self.command }

    /// Actual processing dimensions used for both the payload and pricing.
    #[must_use]
    pub const fn image_size(&self) -> ImageSize { self.image_size }

    /// Original input dimensions before Director Tools resizing.
    #[must_use]
    pub const fn source_size(&self) -> ImageSize { self.source_size }

    /// Estimates this prepared command using its actual processing dimensions.
    #[must_use]
    pub fn estimate_anlas_cost(&self, context: crate::AnlasPricingContext) -> u32 {
        crate::estimate_director_tool_cost(self.image_size, &self.command, context)
    }

    /// Encodes the prepared operation without repeating normalization.
    ///
    /// # Errors
    ///
    /// Returns JSON request encoding failures.
    pub fn encode(&self) -> Result<Vec<u8>, BridgeError> {
        serde_json::to_vec(&self.body)
            .map_err(|source| EncodeError::new(EncodeTarget::JsonRequest, Box::new(source)).into())
    }
}

fn resolve_defry(
    value: u8,
    policy: ValidationPolicy,
    adjustments: &mut Vec<ValidationAdjustment>,
) -> Result<u8, InvalidRequest> {
    let normalized =
        normalize_integer(RequestField::DirectorDefry, u32::from(value), 0, 5, policy)?;
    if u32::from(value) != normalized {
        adjustments.push(ValidationAdjustment {
            field: RequestField::DirectorDefry,
            from: f64::from(value),
            to: f64::from(normalized),
        });
    }
    Ok(u8::try_from(normalized).expect("defry is bounded to five"))
}

impl std::fmt::Debug for PreparedDirector {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PreparedDirector")
            .field("command", &self.command)
            .field("source_size", &self.source_size)
            .field("image_size", &self.image_size)
            .field("adjustments", &self.adjustments)
            .finish_non_exhaustive()
    }
}

#[cfg(test)]
mod tests;
