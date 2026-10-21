use crate::{ImageDecodePolicy, ImageSize, MetadataWarning};

/// One carrier bit per pixel, instead of an otherwise unused decoded RGB(A) image.
pub(in crate::metadata) struct AlphaBits {
    bytes: Vec<u8>,
    stride: usize,
    channels: usize,
    size: ImageSize,
    row: Vec<u8>,
}

impl AlphaBits {
    pub(super) fn for_metadata(
        info: &png::Info<'_>,
        (color, depth): (png::ColorType, png::BitDepth),
    ) -> Option<Self> {
        // A known text carrier wins even if it contains no recognized generation fields.
        // Text after IDAT is still inspected after the same, single pixel-decoding pass.
        if info
            .uncompressed_latin1_text
            .iter()
            .any(|chunk| !chunk.keyword.trim().is_empty())
            || depth != png::BitDepth::Eight
            || u64::from(info.width) * u64::from(info.height)
                < super::super::stealth::HEADER_BITS as u64
        {
            return None;
        }
        let channels = match color {
            png::ColorType::Rgba => 4,
            png::ColorType::GrayscaleAlpha => 2,
            _ => return None,
        };
        let stride = (info.width as usize).div_ceil(8);
        Some(Self {
            bytes: vec![0; stride * info.height as usize],
            stride,
            channels,
            size: ImageSize {
                width: info.width,
                height: info.height,
            },
            row: vec![0; stride],
        })
    }

    pub(super) fn push_row(&mut self, row: png::InterlacedRow<'_>, row_number: usize) {
        self.row.fill(0);
        for (index, pixel) in row.data().chunks_exact(self.channels).enumerate() {
            self.row[index / 8] |= (pixel[self.channels - 1] & 1) << (7 - index % 8);
        }
        match row.interlace() {
            png::InterlaceInfo::Null(_) => {
                let start = row_number * self.stride;
                self.bytes[start..start + self.stride].copy_from_slice(&self.row);
            },
            png::InterlaceInfo::Adam7(info) => {
                png::expand_interlaced_row(&mut self.bytes, self.stride, &self.row, info, 1);
            },
        }
    }

    pub(in crate::metadata) fn read(
        &self,
        policy: ImageDecodePolicy,
    ) -> Result<Option<String>, MetadataWarning> {
        super::super::stealth::read_alpha(self.size, policy, |x, y| {
            (self.bytes[y * self.stride + x / 8] >> (7 - x % 8)) & 1
        })
    }
}
