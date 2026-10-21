use serde::Serialize;

use crate::GeneratedImage;

/// Complete image collection returned by a Director Tools request.
///
/// Image sources retain provider entry identities. Their order alone does not establish
/// roles such as masked, generated or blended background-removal variants.
#[derive(Clone, Debug, Serialize, PartialEq)]
pub struct RunDirectorToolResult {
    /// Every validated image in response order.
    pub images: Vec<GeneratedImage>,
}
