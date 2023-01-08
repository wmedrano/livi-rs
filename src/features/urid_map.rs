use log::error;
use lv2_raw::LV2Feature;
use std::collections::HashMap;
use std::convert::TryFrom;
use std::ffi::{CStr, CString};
use std::pin::Pin;
use std::ptr::NonNull;
use std::sync::Mutex;

static URID_MAP: &[u8] = b"http://lv2plug.in/ns/ext/urid#map\0";
static URID_UNMAP: &[u8] = b"http://lv2plug.in/ns/ext/urid#unmap\0";

type MapImpl = Mutex<HashMap<CString, u32>>;

/// # Safety
/// Dereference to `uri_ptr` may be unsafe.
extern "C" fn do_map(handle: lv2_raw::LV2UridMapHandle, uri_ptr: *const i8) -> lv2_raw::LV2Urid {
    let handle: *const MapImpl = handle as *const _;
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
    let handle: *const MapImpl = handle as *const _;
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
    map: MapImpl,
    map_data: lv2_raw::LV2UridMap,
    unmap_data: lv2_sys::LV2_URID_Unmap,
    urid_map_feature: LV2Feature,
    urid_unmap_feature: LV2Feature,
    _pin: std::marker::PhantomPinned,
}

unsafe impl Send for UridMap {}

impl UridMap {
    pub fn new() -> Pin<Box<UridMap>> {
        let mut urid_map = Box::pin(UridMap {
            map: Mutex::default(),
            map_data: lv2_raw::LV2UridMap {
                handle: std::ptr::null_mut(),
                map: do_map,
            },
            unmap_data: lv2_sys::LV2_URID_Unmap {
                handle: std::ptr::null_mut(),
                unmap: Some(do_unmap),
            },
            urid_map_feature: LV2Feature {
                uri: URID_MAP.as_ptr().cast(),
                data: std::ptr::null_mut(),
            },
            urid_unmap_feature: LV2Feature {
                uri: URID_UNMAP.as_ptr().cast(),
                data: std::ptr::null_mut(),
            },
            _pin: std::marker::PhantomPinned,
        });
        let map_impl_ptr = NonNull::from(&urid_map.map);
        let map_data_ptr = NonNull::from(&urid_map.map_data);
        let unmap_data_ptr = NonNull::from(&urid_map.unmap_data);
        unsafe {
            let mut_ref_pin: Pin<&mut UridMap> = Pin::as_mut(&mut urid_map);
            let mut_ref = Pin::get_unchecked_mut(mut_ref_pin);
            mut_ref.map_data.handle = map_impl_ptr.as_ptr().cast();
            mut_ref.unmap_data.handle = map_impl_ptr.as_ptr().cast();
            mut_ref.urid_map_feature.data = map_data_ptr.as_ptr().cast();
            mut_ref.urid_unmap_feature.data = unmap_data_ptr.as_ptr().cast();
        }
        urid_map
    }

    pub fn map(&self, uri: &CStr) -> lv2_raw::LV2Urid {
        do_map(self.map_data.handle, uri.as_ptr())
    }

    pub fn unmap(&self, urid: lv2_raw::LV2Urid) -> Option<&str> {
        let ptr = do_unmap(self.unmap_data.handle, urid);
        let non_null_ptr = std::ptr::NonNull::new(ptr as *mut _)?;
        let cstr = unsafe { CStr::from_ptr(non_null_ptr.as_ptr()) };
        match cstr.to_str() {
            Ok(s) => Some(s),
            Err(e) => {
                error!("Could not convert cstr{:?} to str: {:?}", cstr, e);
                None
            }
        }
    }

    pub fn as_urid_map_feature(&self) -> &LV2Feature {
        &self.urid_map_feature
    }

    pub fn as_urid_unmap_feature(&self) -> &LV2Feature {
        &self.urid_unmap_feature
    }
}

impl std::fmt::Debug for UridMap {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("UridMap").field("map", &self.map).finish()
    }
}
