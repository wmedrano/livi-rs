use lv2_raw::LV2Feature;

pub mod options;
pub mod urid_map;

const BOUNDED_BLOCK_LENGTH_URI: &[u8] = b"http://lv2plug.in/ns/ext/buf-size#boundedBlockLength\0";

pub const BOUNDED_BLOCK_LENGTH: LV2Feature = LV2Feature {
    uri: BOUNDED_BLOCK_LENGTH_URI.as_ptr().cast(),
    data: std::ptr::null_mut(),
};
