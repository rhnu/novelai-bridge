#[derive(Copy, Clone)]
pub(crate) enum Endpoint {
    GenerateImage,
    #[cfg(feature = "streaming")]
    GenerateImageStream,
    EncodeVibe,
    AugmentImage,
    Subscription,
}

impl Endpoint {
    pub(crate) const fn path(self) -> &'static str {
        match self {
            Self::GenerateImage => "/ai/generate-image",
            #[cfg(feature = "streaming")]
            Self::GenerateImageStream => "/ai/generate-image-stream",
            Self::EncodeVibe => "/ai/encode-vibe",
            Self::AugmentImage => "/ai/augment-image",
            Self::Subscription => "/user/subscription",
        }
    }
}
