use lv2_raw::{LV2Feature, LV2Urid};
use lv2_sys::LV2_Options_Option;
use std::convert::TryFrom;
use std::{collections::HashMap, ffi::CStr};

static OPTIONS_FEATURE_URI: &[u8] = b"http://lv2plug.in/ns/ext/options#options\0";

const EMPTY_OPTION: LV2_Options_Option = LV2_Options_Option {
    context: 0,
    subject: 0,
    key: 0,
    size: 0,
    type_: 0,
    value: std::ptr::null(),
};

#[allow(clippy::vec_box)]
pub struct Options {
    data: Vec<lv2_sys::LV2_Options_Option>,
    values: HashMap<LV2Urid, Box<i32>>,
    feature: LV2Feature,
}

impl Options {
    pub fn new() -> Options {
        let mut data = vec![EMPTY_OPTION];
        let data_ptr = data.as_mut_ptr();
        Options {
            data,
            values: HashMap::new(),
            feature: LV2Feature {
                uri: OPTIONS_FEATURE_URI.as_ptr().cast(),
                data: data_ptr.cast(),
            },
        }
    }

    pub fn as_feature(&self) -> &LV2Feature {
        &self.feature
    }

    pub fn set_int_option(
        &mut self,
        urid_map: &crate::features::urid_map::UridMap,
        key: LV2Urid,
        value: i32,
    ) {
        if let Some(v) = self.values.get_mut(&key) {
            *v.as_mut() = value;
            return;
        }
        let value = Box::new(value);
        let value_ptr = value.as_ref() as *const i32;
        self.values.insert(key, value);
        self.push_option(LV2_Options_Option {
            context: 0,
            subject: 0,
            key,
            size: u32::try_from(std::mem::size_of::<i32>())
                .expect("Size exceeded capacity of u32."),
            type_: urid_map
                .map(CStr::from_bytes_with_nul(b"http://lv2plug.in/ns/ext/atom#Int\0").unwrap()),
            value: value_ptr.cast(),
        });
    }

    fn push_option(&mut self, option: LV2_Options_Option) {
        self.data.pop(); // Remove the last `EMPTY_OPTION`.
        self.data.push(option);
        self.data.push(EMPTY_OPTION);
        let data_ptr = self.data.as_mut_ptr();
        self.feature.data = data_ptr.cast();
    }
}
