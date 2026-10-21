use super::{Field, Normalizer};
use crate::{
    BridgeError, ImageSize, Img2ImgRequest, InvalidRequest,
    generation::ImageInputField,
    parameters::bounds::{normalize_float, normalize_integer},
};

impl Normalizer {
    pub(super) fn normalize_img2img(
        &mut self,
        img2img: Option<&mut Img2ImgRequest>,
    ) -> Result<(), BridgeError> {
        let Some(img2img) = img2img else {
            return Ok(());
        };

        let limits = self.constraints;
        let strength = if img2img.is_infill() {
            limits.inpaint_strength
        } else {
            limits.img2img_strength
        };
        img2img.strength = self.normalize_f32_range(
            Field::ImageInput(ImageInputField::Strength),
            img2img.strength,
            strength.min,
            strength.max,
        )?;
        img2img.noise = self.normalize_f32_range(
            Field::ImageInput(ImageInputField::Noise),
            img2img.noise,
            limits.img2img_noise.min,
            limits.img2img_noise.max,
        )?;
        Ok(())
    }

    pub(super) fn normalize_u32_range(
        &mut self,
        field: Field,
        value: u32,
        min: u32,
        max: u32,
    ) -> Result<u32, BridgeError> {
        let effective = normalize_integer(&field.to_string(), value, min, max, self.reject)?;
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
        let effective = normalize_float(&field.to_string(), value, min, max, self.reject)?;
        self.adjusted(field, f64::from(value), f64::from(effective));
        Ok(effective)
    }

    pub(super) fn normalize_image_dimension(
        &mut self,
        field_name: Field,
        value: u32,
    ) -> Result<u32, BridgeError> {
        let limits = self.constraints.canvas;
        if self.reject
            && (!(limits.dimension.min..=limits.dimension.max).contains(&value)
                || !value.is_multiple_of(limits.multiple))
        {
            return Err(InvalidRequest::InvalidImageDimension {
                field: field_name.to_string(),
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

    pub(super) fn normalize_canvas_area(
        &mut self,
        size: ImageSize,
    ) -> Result<ImageSize, BridgeError> {
        let limits = self.constraints.canvas;
        if size.pixels() <= limits.max_pixels {
            return Ok(size);
        }
        if self.reject {
            return Err(InvalidRequest::InvalidImageDimension {
                field: Field::Size.to_string(),
                value: size.width,
                min: limits.dimension.min,
                max: limits.dimension.max,
                multiple_of: limits.multiple,
            }
            .into());
        }

        let mut shrunk = size;
        while shrunk.pixels() > limits.max_pixels {
            if (shrunk.width >= shrunk.height && shrunk.width > limits.dimension.min)
                || shrunk.height == limits.dimension.min
            {
                shrunk.width -= limits.multiple;
            } else {
                shrunk.height -= limits.multiple;
            }
        }
        self.adjusted(Field::Width, f64::from(size.width), f64::from(shrunk.width));
        self.adjusted(
            Field::Height,
            f64::from(size.height),
            f64::from(shrunk.height),
        );
        Ok(shrunk)
    }
}
