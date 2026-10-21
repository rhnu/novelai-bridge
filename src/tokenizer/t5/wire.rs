use super::{
    TokenizerError,
    asset::{invalid_data, malformed},
};

pub(super) struct Reader<'a> {
    bytes: &'a [u8],
    position: usize,
}

impl<'a> Reader<'a> {
    pub(super) const fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, position: 0 }
    }

    pub(super) const fn is_done(&self) -> bool {
        self.position >= self.bytes.len()
    }

    pub(super) fn varint(&mut self) -> Result<u64, TokenizerError> {
        let mut value = 0u64;
        let mut shift = 0u32;
        loop {
            let byte = *self
                .bytes
                .get(self.position)
                .ok_or_else(|| malformed("truncated varint"))?;
            self.position += 1;
            if shift == 63 && byte > 1 {
                return Err(malformed("varint exceeds 64 bits"));
            }
            value |= u64::from(byte & 0x7F) << shift;
            if byte & 0x80 == 0 {
                return Ok(value);
            }
            shift += 7;
            if shift >= 64 {
                return Err(malformed("varint exceeds 64 bits"));
            }
        }
    }

    pub(super) fn take(&mut self, length: usize) -> Result<&'a [u8], TokenizerError> {
        let end = self
            .position
            .checked_add(length)
            .filter(|end| *end <= self.bytes.len())
            .ok_or_else(|| malformed("length-delimited field runs past the end of the model"))?;
        let slice = &self.bytes[self.position..end];
        self.position = end;
        Ok(slice)
    }

    pub(super) fn length_delimited(&mut self) -> Result<&'a [u8], TokenizerError> {
        let length = usize::try_from(self.varint()?)
            .map_err(|source| invalid_data("length-delimited field length", source))?;
        self.take(length)
    }

    /// Reads a tag and returns `(field number, wire type)`.
    pub(super) fn tag(&mut self) -> Result<(u64, u8), TokenizerError> {
        let key = self.varint()?;
        if key >> 3 == 0 {
            return Err(malformed("protobuf field zero"));
        }
        let wire_type = u8::try_from(key & 0x7).expect("three bits fit in u8");
        Ok((key >> 3, wire_type))
    }

    /// Skips a field whose contents are not needed.
    pub(super) fn skip(&mut self, wire_type: u8) -> Result<(), TokenizerError> {
        match wire_type {
            0 => {
                self.varint()?;
            }
            1 => {
                self.take(8)?;
            }
            2 => {
                self.length_delimited()?;
            }
            5 => {
                self.take(4)?;
            }
            other => return Err(malformed(&format!("unsupported wire type {other}"))),
        }
        Ok(())
    }
}
