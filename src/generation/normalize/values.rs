use super::{Field, Normalizer};
use crate::{
    BridgeError, GenerateImageRequest, GenerationInput, ImageDecodePolicy, ImageInputField,
    ImageSize, InpaintImageToImageField, InvalidRequest, ModelDescriptor,
    parameters::bounds::{normalize_float, normalize_integer},
};

impl Normalizer {
    pub(super) fn normalize_operation(
        &mut self,
        operation: &mut GenerationInput,
        descriptor: &ModelDescriptor,
    ) -> Result<(), BridgeError> {
        let (strength, noise, strength_field, noise_field, limits) = match operation {
            GenerationInput::TextToImage => return Ok(()),
            GenerationInput::ImageToImage(input) => {
                if input.max_enhance
                    && !self.require_capability(
                        descriptor.support().max_enhance,
                        Field::ImageInput(ImageInputField::MaxEnhance),
                        "a model supporting Max Enhance",
                        super::NormalizationChange::Unsupported,
                    )?
                {
                    input.max_enhance = false;
                }
                (
                    &mut input.strength,
                    &mut input.noise,
                    Field::ImageInput(ImageInputField::Strength),
                    Field::ImageInput(ImageInputField::Noise),
                    self.constraints.img2img_strength,
                )
            },
            GenerationInput::Inpainting(input) => {
                if input.img2img.is_none() {
                    return Ok(());
                }
                if !self.require_capability(
                    descriptor
                        .support()
                        .inpainting
                        .is_some_and(|profile| profile.image_to_image),
                    Field::InpaintImageToImage(InpaintImageToImageField::Entry),
                    "models supporting image-to-image conditioning in inpainting",
                    super::NormalizationChange::Unsupported,
                )? {
                    input.img2img = None;
                    return Ok(());
                }
                let parameters = input
                    .img2img
                    .as_mut()
                    .expect("conditioning was present before capability validation");
                (
                    &mut parameters.strength,
                    &mut parameters.noise,
                    Field::InpaintImageToImage(InpaintImageToImageField::Strength),
                    Field::InpaintImageToImage(InpaintImageToImageField::Noise),
                    self.constraints.inpaint_strength,
                )
            },
        };
        *strength = self.normalize_f32_range(strength_field, *strength, limits.min, limits.max)?;
        let limits = self.constraints.img2img_noise;
        *noise = self.normalize_f32_range(noise_field, *noise, limits.min, limits.max)?;
        Ok(())
    }

    /// Every generation operation uses the final model canvas before output geometry is derived.
    pub(super) fn normalize_generation_canvas(
        &mut self,
        request: &mut GenerateImageRequest,
    ) -> Result<(), BridgeError> {
        request.core.size.width =
            self.normalize_image_dimension(Field::Width, request.core.size.width)?;
        request.core.size.height =
            self.normalize_image_dimension(Field::Height, request.core.size.height)?;
        let limits = self.constraints.canvas;
        self.normalize_canvas_area(
            &mut request.core.size,
            limits.multiple,
            limits.dimension.min,
        )
    }

    #[expect(
        clippy::cast_precision_loss,
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "Canvas area normalization follows positive floating point scaling and floors each bounded dimension"
    )]
    fn normalize_canvas_area(
        &mut self,
        size: &mut ImageSize,
        multiple: u32,
        minimum: u32,
    ) -> Result<(), BridgeError> {
        let original = *size;
        let max_pixels = self
            .constraints
            .canvas
            .max_pixels
            .min(ImageDecodePolicy::default().max_pixels);
        if original.pixels() > max_pixels {
            if self.policy.is_reject() {
                return Err(InvalidRequest::NumericOutOfRange {
                    field: Field::Size.into(),
                    value: original.pixels() as f64,
                    min: f64::from(minimum) * f64::from(minimum),
                    max: max_pixels as f64,
                }
                .into());
            }
            let scale = (max_pixels as f64 / original.pixels() as f64).sqrt();
            size.width = (((f64::from(original.width) * scale).floor() as u32 / multiple)
                * multiple)
                .max(minimum);
            size.height = (((f64::from(original.height) * scale).floor() as u32 / multiple)
                * multiple)
                .max(minimum);
            self.adjusted(
                Field::Width,
                f64::from(original.width),
                f64::from(size.width),
            );
            self.adjusted(
                Field::Height,
                f64::from(original.height),
                f64::from(size.height),
            );
        }
        Ok(())
    }

    pub(super) fn normalize_u32_range(
        &mut self,
        field: Field,
        value: u32,
        min: u32,
        max: u32,
    ) -> Result<u32, BridgeError> {
        let effective = normalize_integer(field.into(), value, min, max, self.policy)?;
        self.adjusted(field, f64::from(value), f64::from(effective));
        Ok(effective)
    }

    pub(super) fn normalize_f32_range(
        &mut self,
        field: Field,
        value: f32,
        min: f32,
        max: f32,
    ) -> Result<f32, BridgeError> {
        let effective = normalize_float(field.into(), value, min, max, self.policy)?;
        self.adjusted(field, f64::from(value), f64::from(effective));
        Ok(effective)
    }

    pub(super) fn normalize_image_dimension(
        &mut self,
        field_name: Field,
        value: u32,
    ) -> Result<u32, BridgeError> {
        let limits = self.constraints.canvas;
        if self.policy.is_reject()
            && (!(limits.dimension.min..=limits.dimension.max).contains(&value)
                || !value.is_multiple_of(limits.multiple))
        {
            return Err(InvalidRequest::InvalidImageDimension {
                field: field_name.into(),
                value,
                min: limits.dimension.min,
                max: limits.dimension.max,
                multiple_of: limits.multiple,
            }
            .into());
        }

        let clamped = value.clamp(limits.dimension.min, limits.dimension.max);
        let snapped = ((clamped + (limits.multiple / 2)) / limits.multiple) * limits.multiple;
        let effective = snapped.clamp(limits.dimension.min, limits.dimension.max);
        self.adjusted(field_name, f64::from(value), f64::from(effective));
        Ok(effective)
    }
}
