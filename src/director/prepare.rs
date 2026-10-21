use crate::{BridgeError, EncodeError, EncodeTarget, ImageSize, InvalidRequest};
use crate::generation::bounds::normalize_integer;
use super::{DirectorCommand, RunDirectorToolRequest, wire::DirectorRequestBody};

/// A validated director operation with its encoded source image.
#[derive(Debug)]
pub struct PreparedDirector {
    command: DirectorCommand,
    image_size: ImageSize,
    body: DirectorRequestBody,
}

impl RunDirectorToolRequest {
    /// Normalizes the command and prepares a reusable payload without network I/O.
    ///
    /// # Errors
    ///
    /// Returns an invalid prompt or numeric parameter error.
    pub fn prepare(self) -> Result<PreparedDirector, BridgeError> {
        let command = match self.command {
            DirectorCommand::Emotion { prompt, defry } => {
                let prompt = prompt.trim().to_owned();
                if prompt.is_empty() {
                    return Err(InvalidRequest::RequiredFieldForContext {
                        context: "emotion".to_owned(), field: "command.prompt".to_owned(),
                    }.into());
                }
                DirectorCommand::Emotion { prompt, defry: resolve_defry(defry, self.strict_mode)? }
            }
            DirectorCommand::Colorize { prompt, defry } => DirectorCommand::Colorize {
                prompt: prompt.map(|text| text.trim().to_owned()).filter(|text| !text.is_empty()),
                defry: resolve_defry(defry, self.strict_mode)?,
            },
            command => command,
        };
        let (prompt, defry) = match &command {
            DirectorCommand::Emotion { prompt, defry } => (Some(prompt.clone()), Some(*defry)),
            DirectorCommand::Colorize { prompt, defry } => (prompt.clone(), Some(*defry)),
            _ => (None, None),
        };
        let image_size = self.image.size();
        let body = DirectorRequestBody {
            req_type: command.wire_name(),
            width: image_size.width,
            height: image_size.height,
            image: self.image.to_base64(),
            prompt,
            defry,
        };
        Ok(PreparedDirector { command, image_size, body })
    }
}

impl PreparedDirector {
    /// Effective command used by the payload.
    #[must_use]
    pub const fn command(&self) -> &DirectorCommand { &self.command }

    /// Source dimensions used for both the payload and pricing.
    #[must_use]
    pub const fn image_size(&self) -> ImageSize { self.image_size }

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

fn resolve_defry(value: u8, reject: bool) -> Result<u8, InvalidRequest> {
    let value = normalize_integer("command.defry", u32::from(value), 0, 5, reject)?;
    Ok(u8::try_from(value).expect("defry is bounded to five"))
}

#[cfg(test)]
mod tests;
