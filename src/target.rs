pub struct CompilationTarget {
    triple: target_lexicon::Triple,
    endianness: target_lexicon::Endianness,
    c_type_sizes: target_lexicon::CDataModel,
}
use thiserror::Error;

use crate::il;

// target_lexicon::triple does not implement Copy because there might be
// a CustomVendor which holds a String,
// This is quite a rare case, and EVERY other field is Copy.
//
// As such, we clone the Triple eagerly as in most cases, the Clone is acutally a Copy

pub trait ToTargetEndianBytes<const N: usize>: Copy + Sized {
    fn to_target_endian_bytes(self, target: &CompilationTarget) -> [u8; N];
}

impl ToTargetEndianBytes<4> for u32 {
    fn to_target_endian_bytes(self, target: &CompilationTarget) -> [u8; 4] {
        match target.endianness() {
            target_lexicon::Endianness::Little => self.to_le_bytes(),
            target_lexicon::Endianness::Big => self.to_be_bytes(),
        }
    }
}
impl ToTargetEndianBytes<2> for u16 {
    fn to_target_endian_bytes(self, target: &CompilationTarget) -> [u8; 2] {
        match target.endianness() {
            target_lexicon::Endianness::Little => self.to_le_bytes(),
            target_lexicon::Endianness::Big => self.to_be_bytes(),
        }
    }
}
impl ToTargetEndianBytes<8> for u64 {
    fn to_target_endian_bytes(self, target: &CompilationTarget) -> [u8; 8] {
        match target.endianness() {
            target_lexicon::Endianness::Little => self.to_le_bytes(),
            target_lexicon::Endianness::Big => self.to_be_bytes(),
        }
    }
}

const fn tlsize_to_qbe_type(tl_size: target_lexicon::Size) -> il::ILTypeData {
    match tl_size {
        target_lexicon::Size::U8 => il::ILTypeData::I8,
        target_lexicon::Size::U16 => il::ILTypeData::I16,
        target_lexicon::Size::U32 => il::ILTypeData::I32,
        target_lexicon::Size::U64 => il::ILTypeData::I64,
    }
}

#[derive(Debug, Error)]
pub enum InvalidTargetError {
    #[error("invalid pointer width: {0:?}")]
    UnsupportedPointerWidth(target_lexicon::PointerWidth),
    #[error("invalid triple: {0:?}")]
    InvalidTriple(target_lexicon::Triple),
}

impl From<target_lexicon::Triple> for InvalidTargetError {
    fn from(v: target_lexicon::Triple) -> Self {
        Self::InvalidTriple(v)
    }
}

impl CompilationTarget {
    pub fn host() -> Self {
        Self::for_triple(target_lexicon::HOST).expect("invalid host")
    }

    pub fn for_triple(triple: target_lexicon::Triple) -> Result<Self, InvalidTargetError> {
        let endianness = triple
            .endianness()
            .map_err(|_| InvalidTargetError::from(triple.clone()))?;
        let c_type_sizes = triple
            .data_model()
            .map_err(|_| InvalidTargetError::from(triple.clone()))?;

        let compilation_target = Self {
            triple,
            endianness,
            c_type_sizes,
        };
        compilation_target.check_pointer_width_min_32()?;
        Ok(compilation_target)
    }

    pub fn check_pointer_width_min_32(&self) -> Result<(), InvalidTargetError> {
        match self.pointer_width() {
            target_lexicon::PointerWidth::U16 => Err(InvalidTargetError::UnsupportedPointerWidth(
                self.pointer_width(),
            )),
            target_lexicon::PointerWidth::U32 => Ok(()),
            target_lexicon::PointerWidth::U64 => Ok(()),
        }
    }

    pub fn pointer_width(&self) -> target_lexicon::PointerWidth {
        self.triple.pointer_width().unwrap()
    }

    pub fn pointer_width_is_64(&self) -> bool {
        let w = self.pointer_width();
        match w {
            target_lexicon::PointerWidth::U16 => {
                unimplemented!("rQBE does not support 16 bit arches")
            }
            target_lexicon::PointerWidth::U32 => false,
            target_lexicon::PointerWidth::U64 => true,
        }
    }

    pub fn triple(&self) -> &target_lexicon::Triple {
        &self.triple
    }
    pub fn endianness(&self) -> target_lexicon::Endianness {
        self.endianness
    }

    pub fn c_type_sizes(&self) -> target_lexicon::CDataModel {
        self.c_type_sizes
    }
    pub fn type_word(&self) -> il::ILTypeData {
        tlsize_to_qbe_type(self.c_type_sizes.int_size())
    }
    pub fn type_halfword(&self) -> il::ILTypeData {
        tlsize_to_qbe_type(self.c_type_sizes.short_size())
    }
    pub fn type_long(&self) -> il::ILTypeData {
        tlsize_to_qbe_type(self.c_type_sizes.long_size())
    }
    pub fn type_mem(&self) -> il::ILTypeData {
        tlsize_to_qbe_type(self.c_type_sizes.pointer_width())
    }
    pub fn type_single(&self) -> il::ILTypeData {
        tlsize_to_qbe_type(self.c_type_sizes.float_size())
    }
    pub fn type_double(&self) -> il::ILTypeData {
        tlsize_to_qbe_type(self.c_type_sizes.double_size())
    }
    pub const fn type_byte(&self) -> il::ILTypeData {
        il::ILTypeData::I8
    }
}
