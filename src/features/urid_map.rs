use lv2_raw::LV2Feature;
use std::collections::HashMap;
use std::ffi::{CStr, CString};
use std::sync::Mutex;

static URID_MAP: &[u8] = b"http://lv2plug.in/ns/ext/urid#map\0";

/// # Safety
/// Dereference to `uri_ptr` may be unsafe.
extern "C" fn do_map(handle: lv2_raw::LV2UridMapHandle, uri_ptr: *const i8) -> lv2_raw::LV2Urid {
    let handle: *const Mutex<HashMap<CString, u32>> = handle as *const _;
    let map_mutex = unsafe { &*handle };
    let mut map = map_mutex.lock().unwrap();
    let uri = unsafe { CStr::from_ptr(uri_ptr) };

    if let Some(id) = map.get(uri) {
        *id
    } else {
        let id = map.len() as u32 + 1;
        map.insert(uri.to_owned(), id);
        id
    }
}

pub struct UridMap {
    _map: Box<Mutex<HashMap<CString, u32>>>,
    _data: Box<lv2_raw::LV2UridMap>,
    feature: LV2Feature,
}

impl UridMap {
    pub fn new() -> UridMap {
        let map = Box::new(Mutex::new(HashMap::new()));
        let map_ptr = map.as_ref() as *const _ as *mut _;
        let data = Box::new(lv2_raw::LV2UridMap {
            handle: map_ptr,
            map: do_map,
        });
        let data_ptr = data.as_ref() as *const _ as *mut _;
        UridMap {
            _map: map,
            _data: data,
            feature: LV2Feature {
                uri: URID_MAP.as_ptr().cast(),
                data: data_ptr,
            },
        }
    }

    pub fn map(&self, uri: &CStr) -> u32 {
        do_map(self._data.handle, uri.as_ptr())
    }

    pub fn as_feature(&self) -> &LV2Feature {
        &self.feature
    }

    pub fn as_feature_mut(&mut self) -> &mut LV2Feature {
        &mut self.feature
    }
}
