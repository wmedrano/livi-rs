use lv2_raw::LV2Feature;
use std::collections::HashMap;
use std::convert::TryFrom;
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
        return *id;
    }
    let id = u32::try_from(map.len()).expect("URID space has exceeded capacity for u32.") + 1;
    map.insert(uri.to_owned(), id);
    id
}

extern "C" fn do_unmap(handle: lv2_sys::LV2_URID_Map_Handle, urid: lv2_raw::LV2Urid) -> *const i8 {
    let handle: *const Mutex<HashMap<CString, lv2_raw::LV2Urid>> = handle as *const _;
    let map_mutex = unsafe { &*handle };
    let map = map_mutex.lock().unwrap();
    for (uri, id) in map.iter() {
        if *id == urid {
            return uri.as_ptr();
        }
    }
    std::ptr::null()
}

pub struct UridMap {
    _map: Box<Mutex<HashMap<CString, u32>>>,
    map_data: Box<lv2_raw::LV2UridMap>,
    _unmap_data: Box<lv2_sys::LV2_URID_Unmap>,
    urid_map_feature: LV2Feature,
    urid_unmap_feature: LV2Feature,
}

unsafe impl Send for UridMap {}

impl UridMap {
    pub fn new() -> UridMap {
        let map = Box::new(Mutex::new(HashMap::new()));
        let map_ptr = map.as_ref() as *const _ as *mut _;
        let map_data = Box::new(lv2_raw::LV2UridMap {
            handle: map_ptr,
            map: do_map,
        });
        let map_data_ptr = map_data.as_ref() as *const _ as *mut _;
        let unmap_data = Box::new(lv2_sys::LV2_URID_Unmap {
            handle: map_ptr,
            unmap: Some(do_unmap),
        });
        let unmap_data_ptr = unmap_data.as_ref() as *const _ as *mut _;
        UridMap {
            _map: map,
            map_data,
            _unmap_data: unmap_data,
            urid_map_feature: LV2Feature {
                uri: URID_MAP.as_ptr().cast(),
                data: map_data_ptr,
            },
            urid_unmap_feature: LV2Feature {
                uri: b"http://lv2plug.in/ns/ext/urid#unmap\0".as_ptr().cast(),
                data: unmap_data_ptr,
            },
        }
    }

    pub fn map(&self, uri: &CStr) -> u32 {
        do_map(self.map_data.handle, uri.as_ptr())
    }

    pub fn as_urid_map_feature(&self) -> &LV2Feature {
        &self.urid_map_feature
    }

    pub fn as_urid_unmap_feature(&self) -> &LV2Feature {
        &self.urid_unmap_feature
    }
}
