use crate::error::InitializeBlockLengthError;
use lv2_raw::LV2Feature;
use lv2_sys::LV2_BUF_SIZE__boundedBlockLength;
use std::convert::TryFrom;
use std::{collections::HashSet, ffi::CStr};

pub mod options;
pub mod urid_map;
pub mod worker;

pub(crate) const BOUNDED_BLOCK_LENGTH: LV2Feature = LV2Feature {
    uri: LV2_BUF_SIZE__boundedBlockLength.as_ptr().cast(),
    data: std::ptr::null_mut(),
};

pub struct Features {
    pub urid_map: urid_map::UridMap,
    pub options: options::Options,
    pub min_and_max_block_length: Option<(usize, usize)>,
}

impl Features {
    pub fn new() -> Features {
        Features {
            urid_map: urid_map::UridMap::new(),
            options: options::Options::new(),
            min_and_max_block_length: None,
        }
    }

    pub fn initialize_block_length(
        &mut self,
        min_block_length: usize,
        max_block_length: usize,
    ) -> Result<(), InitializeBlockLengthError> {
        if let Some((min_block_length, max_block_length)) = self.min_and_max_block_length {
            return Err(InitializeBlockLengthError::BlockLengthAlreadyInitialized {
                min_block_length,
                max_block_length,
            });
        }
        let min = i32::try_from(min_block_length).map_err(|_| {
            InitializeBlockLengthError::MinBlockLengthTooLarge {
                max_supported: i32::MAX as usize,
                actual: min_block_length,
            }
        })?;
        let max = i32::try_from(max_block_length).map_err(|_| {
            InitializeBlockLengthError::MaxBlockLengthTooLarge {
                max_supported: i32::MAX as usize,
                actual: max_block_length,
            }
        })?;
        self.options.set_int_option(
            &self.urid_map,
            self.urid_map.map(
                CStr::from_bytes_with_nul(b"http://lv2plug.in/ns/ext/buf-size#minBlockLength\0")
                    .unwrap(),
            ),
            min,
        );
        self.options.set_int_option(
            &self.urid_map,
            self.urid_map.map(
                CStr::from_bytes_with_nul(b"http://lv2plug.in/ns/ext/buf-size#maxBlockLength\0")
                    .unwrap(),
            ),
            max,
        );
        self.min_and_max_block_length = Some((min_block_length, max_block_length));
        Ok(())
    }

    /// Get the URIs for all supported features.
    pub fn supported_features(&self) -> HashSet<&'static str> {
        HashSet::from([
            "http://lv2plug.in/ns/ext/urid#map",
            "http://lv2plug.in/ns/ext/urid#unmap",
            "http://lv2plug.in/ns/ext/options#options",
            "http://lv2plug.in/ns/ext/buf-size#boundedBlockLength",
            "http://lv2plug.in/ns/ext/worker#schedule",
        ])
    }
}
