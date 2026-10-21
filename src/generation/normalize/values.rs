use super::{Field, Normalizer};
use crate::{
    BridgeError, GenerationInput, ImageInputField, InpaintImageToImageField, InvalidRequest,
    ModelDescriptor,
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
            GenerationInput::ImageToImage(input) => (
                &mut input.strength,
                &mut input.noise,
                Field::ImageInput(ImageInputField::Strength),
                Field::ImageInput(ImageInputField::Noise),
                self.constraints.img2img_strength,
            ),
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
