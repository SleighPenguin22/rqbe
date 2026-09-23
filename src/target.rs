pub struct CompilationTarget {
    triple: target_lexicon::Triple,
    endianness: target_lexicon::Endianness,
    c_type_sizes: target_lexicon::CDataModel,
}
use thiserror::Error;

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
    /// get the [`object::Endianness`] of this target
    pub fn object_endianness(&self) -> object::Endianness {
        match self.endianness {
            target_lexicon::Endianness::Little => object::Endianness::Little,
            target_lexicon::Endianness::Big => object::Endianness::Big,
        }
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
                unreachable!("rQBE does not support 16 bit arches")
            }
            target_lexicon::PointerWidth::U32 => false,
            target_lexicon::PointerWidth::U64 => true,
        }
    }

    pub fn empty_elf_object(&self) -> object::write::Object<'_> {
        object::write::Object::new(
            object::BinaryFormat::Elf,
            self.triple_as_object_arch(),
            self.object_endianness(),
        )
    }

    pub fn declare_text_symbol(
        &self,
        obj: &mut object::write::Object,
        name: &[u8],
    ) -> object::write::SymbolId {
        obj.add_symbol(object::write::Symbol {
            name: (*name).into(),
            value: 0,
            size: 0,
            kind: object::SymbolKind::Text,
            scope: object::SymbolScope::Unknown,
            weak: false,
            section: object::write::SymbolSection::Undefined,
            flags: object::SymbolFlags::None,
        })
    }

    pub fn define_text_symbol(
        &self,
        obj: &mut object::write::Object,
        id: object::write::SymbolId,
        section: object::write::SectionId,
        scope: object::SymbolScope,
        alignment: u64,
        data: &[u8],
    ) -> u64 {
        let s = obj.symbol_mut(id);
        s.scope = scope;
        obj.add_symbol_data(id, section, data, alignment)
    }

    pub fn triple(&self) -> &target_lexicon::Triple {
        &self.triple
    }
    pub fn triple_as_object_arch(&self) -> object::Architecture {
        match self.triple.architecture {
            target_lexicon::Architecture::Aarch64(target_lexicon::Aarch64Architecture::Aarch64) => {
                object::Architecture::Aarch64
            }
            target_lexicon::Architecture::Aarch64(
                target_lexicon::Aarch64Architecture::Aarch64be,
            ) => object::Architecture::Aarch64,
            target_lexicon::Architecture::X86_64 => object::Architecture::X86_64,
            _ => todo!("support {}", self.triple),
        }
    }

    pub fn endianness(&self) -> target_lexicon::Endianness {
        self.endianness
    }

    pub fn c_type_sizes(&self) -> target_lexicon::CDataModel {
        self.c_type_sizes
    }
}
