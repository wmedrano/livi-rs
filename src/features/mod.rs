use crate::WorkerManager;
use lv2_raw::LV2Feature;
use lv2_sys::LV2_BUF_SIZE__boundedBlockLength;
use std::pin::Pin;
use std::sync::Arc;
use std::{collections::HashSet, ffi::CStr};

pub mod options;
pub mod urid_map;
pub mod worker;

/// A builder for `Features` objects.
pub struct FeaturesBuilder {
    /// The minimum block size. If plugins try to process less samples than this
    /// on a single `run` call, an error will be returned.
    pub min_block_length: usize,
    /// The maximum block size. If plugins try to process more samples than this
    /// on a single `run` call, an error will be returned.
    pub max_block_length: usize,
    /// The worker manager. Plugins will execute asynchronous work to be done by
    /// this worker manager.
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
    /// Build a new `Features` object.
    pub fn build(self, _world: &crate::World) -> Arc<Features> {
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

/// `Features` are used to provide functionality to plugins.
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

    /// Iterate over all the LV2 features.
    pub fn iter_features<'a>(
        &'a self,
        worker_feature: &'a LV2Feature,
    ) -> impl Iterator<Item = &'a LV2Feature> {
        std::iter::once(self.urid_map.as_urid_map_feature())
            .chain(std::iter::once(self.urid_map.as_urid_unmap_feature()))
            .chain(std::iter::once(self.options.as_feature()))
            .chain(std::iter::once(&self.bounded_block_length))
            .chain(std::iter::once(worker_feature))
    }

    /// The minimum allowed block length.
    pub fn min_block_length(&self) -> usize {
        self.min_block_length
    }

    /// The maximum allowed block length.
    pub fn max_block_length(&self) -> usize {
        self.max_block_length
    }

    /// The urid for the given uri.
    pub fn urid(&self, uri: &CStr) -> u32 {
        self.urid_map.map(uri)
    }

    /// The urid for midi.
    pub fn midi_urid(&self) -> lv2_raw::LV2Urid {
        self.urid(
            std::ffi::CStr::from_bytes_with_nul(b"http://lv2plug.in/ns/ext/midi#MidiEvent\0")
                .unwrap(),
        )
    }

    /// The uri for the given urid.
    pub fn uri(&self, urid: lv2_raw::LV2Urid) -> Option<&str> {
        self.urid_map.unmap(urid)
    }

    /// The worker manager. This should be run periodically to perform any
    /// asynchronous work that plugins have scheduled.
    pub fn worker_manager(&self) -> &Arc<WorkerManager> {
        &self.worker_manager
    }
}
