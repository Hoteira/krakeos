use crate::alloc::{format, vec::Vec};
use crate::wasm::common::{indices::MemIdx, reader::span::Span};
use core::fmt::{Debug, Formatter};

#[derive(Clone)]
pub struct DataSegment {
    pub init: Vec<u8>,
    pub mode: DataMode,
}

#[derive(Clone)]
pub enum DataMode {
    Passive,
    Active(DataModeActive),
}

#[derive(Clone)]
pub struct DataModeActive {
    pub memory_idx: MemIdx,
    pub offset: Span,
}

impl Debug for DataSegment {
    fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("DataSegment")
            .field("init_len", &self.init.len())
            .field("mode", &self.mode)
            .finish()
    }
}

impl Debug for DataMode {
    fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
        match self {
            DataMode::Passive => f.debug_struct("Passive").finish(),
            DataMode::Active(active_data_mode) => {
                f.debug_struct("Active")
                    .field("offset", &active_data_mode.offset)
                    .finish()
            }
        }
    }
}
