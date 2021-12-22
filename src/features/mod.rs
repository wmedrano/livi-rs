use crate::error::InitializeBlockLengthError;
use lv2_raw::LV2Feature;
use std::convert::TryFrom;
use std::{collections::HashSet, ffi::CStr};

pub mod options;
pub mod urid_map;
pub mod worker;

const BOUNDED_BLOCK_LENGTH_URI: &[u8] = b"http://lv2plug.in/ns/ext/buf-size#boundedBlockLength\0";

pub const BOUNDED_BLOCK_LENGTH: LV2Feature = LV2Feature {
    uri: BOUNDED_BLOCK_LENGTH_URI.as_ptr().cast(),
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
    pub fn supported_features(&self) -> HashSet<String> {
        self.iter_features()
            .map(|f| {
                unsafe { std::ffi::CStr::from_ptr(f.uri) }
                    .to_string_lossy()
                    .into_owned()
            })
            .collect()
    }

    /// Iterate over all supported features.
    pub fn iter_features(&self) -> impl Iterator<Item = &'_ LV2Feature> {
        std::iter::once(self.urid_map.as_urid_map_feature())
            .chain(std::iter::once(self.urid_map.as_urid_unmap_feature()))
            .chain(std::iter::once(self.options.as_feature()))
            .chain(std::iter::once(&BOUNDED_BLOCK_LENGTH))
    }
}
