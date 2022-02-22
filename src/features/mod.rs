use crate::WorkerManager;
use lv2_raw::LV2Feature;
use lv2_sys::LV2_BUF_SIZE__boundedBlockLength;
use std::pin::Pin;
use std::sync::Arc;
use std::{collections::HashSet, ffi::CStr};

pub mod options;
pub mod urid_map;
pub mod worker;

pub struct FeaturesBuilder {
    pub min_block_length: usize,
    pub max_block_length: usize,
    pub worker_manager: Arc<WorkerManager>,
}

impl Default for FeaturesBuilder {
    fn default() -> FeaturesBuilder {
        FeaturesBuilder {
            min_block_length: 1,
            max_block_length: 4096,
            worker_manager: Arc::default(),
        }
    }
}

impl FeaturesBuilder {
    pub(crate) fn build(self, _world: &crate::World) -> Arc<Features> {
        let mut features = Features {
            urid_map: urid_map::UridMap::new(),
            options: options::Options::new(),
            min_block_length: self.min_block_length,
            max_block_length: self.max_block_length,
            bounded_block_length: LV2Feature {
                uri: LV2_BUF_SIZE__boundedBlockLength.as_ptr().cast(),
                data: std::ptr::null_mut(),
            },
            worker_manager: self.worker_manager,
        };
        features.options.set_int_option(
            &features.urid_map,
            features.urid_map.map(
                CStr::from_bytes_with_nul(b"http://lv2plug.in/ns/ext/buf-size#minBlockLength\0")
                    .unwrap(),
            ),
            self.min_block_length as i32,
        );
        features.options.set_int_option(
            &features.urid_map,
            features.urid_map.map(
                CStr::from_bytes_with_nul(b"http://lv2plug.in/ns/ext/buf-size#maxBlockLength\0")
                    .unwrap(),
            ),
            self.max_block_length as i32,
        );
        Arc::new(features)
    }
}

pub struct Features {
    urid_map: Pin<Box<urid_map::UridMap>>,
    options: options::Options,
    bounded_block_length: LV2Feature,
    min_block_length: usize,
    max_block_length: usize,
    worker_manager: Arc<WorkerManager>,
}

impl Features {
    /// Get the URIs for all supported features.
    pub fn supported_features() -> HashSet<&'static str> {
        HashSet::from([
            "http://lv2plug.in/ns/ext/urid#map",
            "http://lv2plug.in/ns/ext/urid#unmap",
            "http://lv2plug.in/ns/ext/options#options",
            "http://lv2plug.in/ns/ext/buf-size#boundedBlockLength",
            "http://lv2plug.in/ns/ext/worker#schedule",
        ])
    }

    pub fn iter_features(&self) -> impl Iterator<Item = &LV2Feature> {
        let features = std::iter::once(self.urid_map.as_urid_map_feature())
            .chain(std::iter::once(self.urid_map.as_urid_unmap_feature()))
            .chain(std::iter::once(self.options.as_feature()))
            .chain(std::iter::once(&self.bounded_block_length));
        features
    }

    pub fn min_block_length(&self) -> usize {
        self.min_block_length
    }

    pub fn max_block_length(&self) -> usize {
        self.max_block_length
    }

    pub fn urid(&self, uri: &CStr) -> u32 {
        self.urid_map.map(uri)
    }

    pub fn midi_urid(&self) -> lv2_raw::LV2Urid {
        self.urid(
            std::ffi::CStr::from_bytes_with_nul(b"http://lv2plug.in/ns/ext/midi#MidiEvent\0")
                .unwrap(),
        )
    }

    pub fn uri(&self, urid: lv2_raw::LV2Urid) -> Option<&str> {
        self.urid_map.unmap(urid)
    }

    pub(crate) fn worker_manager(&self) -> &WorkerManager {
        &self.worker_manager
    }
}
