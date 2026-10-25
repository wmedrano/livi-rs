use std::collections::HashMap;
use std::ffi::CStr;
use std::path::Path;
use std::sync::Arc;

use lilv::state::{GetPortValue, SetPortValue, Value};
use log::warn;
use lv2_raw::LV2Feature;

use crate::PortIndex;
use crate::error::StateError;
use crate::features::Features;
use crate::port::Controls;

/// The flags used when creating and restoring state.
pub(crate) fn state_flags() -> lv2_sys::LV2_State_Flags {
    lv2_sys::LV2_State_Flags(
        lv2_sys::LV2_State_Flags::LV2_STATE_IS_POD.0
            | lv2_sys::LV2_State_Flags::LV2_STATE_IS_PORTABLE.0,
    )
}

/// The features passed to state operations.
pub(crate) fn state_features(features: &Features) -> impl Iterator<Item = &LV2Feature> {
    [
        features.urid_map.as_urid_map_feature(),
        features.urid_map.as_urid_unmap_feature(),
    ]
    .into_iter()
}

/// A snapshot of a plugin's state. This includes the values of the control
/// input ports and any properties that the plugin saves through the LV2 State
/// extension.
///
/// A `State` may be created from an `Instance` with `Instance::state` or loaded
/// with `World::state_from_string`, `World::state_from_file`, or
/// `World::state_from_uri`. It can be applied to an `Instance` with
/// `Instance::restore_state`.
pub struct State {
    inner: lilv::state::State,
    features: Arc<Features>,
}

impl State {
    pub(crate) fn new(inner: lilv::state::State, features: Arc<Features>) -> State {
        State { inner, features }
    }

    /// Get the underlying `lilv::state::State`.
    pub fn raw(&self) -> &lilv::state::State {
        &self.inner
    }

    /// The features that were used to create the state. URIDs within the state
    /// are mapped with these features.
    pub fn features(&self) -> &Arc<Features> {
        &self.features
    }

    /// The URI of the plugin that the state applies to.
    pub fn plugin_uri(&self) -> String {
        self.inner
            .plugin_uri()
            .as_uri()
            .unwrap_or_default()
            .to_string()
    }

    /// The URI of the state or `None` if the state has not been saved.
    pub fn uri(&self) -> Option<String> {
        self.inner.uri()?.as_uri().map(str::to_string)
    }

    /// The label of the state.
    pub fn label(&self) -> Option<&str> {
        self.inner.label()
    }

    /// Set the label of the state.
    pub fn set_label(&mut self, label: &str) {
        self.inner.set_label(label);
    }

    /// Serialize the state to a Turtle string with the given `uri` as the
    /// subject. The state can be loaded again with `World::state_from_string`.
    ///
    /// # Errors
    /// Returns an error if the state could not be serialized.
    pub fn to_string(&self, uri: &str) -> Result<String, StateError> {
        let mut map = self.features.urid_map.lv2_urid_map();
        let mut unmap = self.features.urid_map.lv2_urid_unmap();
        self.inner
            .to_string(&mut map, &mut unmap, uri, None)
            .ok_or(StateError::SerializeFailed)
    }

    /// Save the state to `filename` within the bundle directory `dir`. The
    /// saved state is compatible with the LV2 presets extension. If `uri` is
    /// `None`, then the subject of the state will be the file itself.
    ///
    /// # Errors
    /// Returns an error if the state could not be saved.
    pub fn save(&self, dir: &Path, filename: &str, uri: Option<&str>) -> Result<(), StateError> {
        let dir = dir.to_str().ok_or(StateError::InvalidPath)?;
        let mut map = self.features.urid_map.lv2_urid_map();
        let mut unmap = self.features.urid_map.lv2_urid_unmap();
        self.inner
            .save(&mut map, &mut unmap, uri, dir, filename)
            .map_err(StateError::Status)
    }
}

impl PartialEq for State {
    fn eq(&self, other: &Self) -> bool {
        self.inner == other.inner
    }
}

impl std::fmt::Debug for State {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("State")
            .field("plugin_uri", &self.plugin_uri())
            .field("uri", &self.uri())
            .field("label", &self.label())
            .finish()
    }
}

/// Provides control input port values to lilv when creating a state.
pub(crate) struct PortValueGetter<'a> {
    pub controls: &'a Controls,
    pub symbols: &'a HashMap<String, PortIndex>,
    pub float_urid: lv2_raw::LV2Urid,
}

impl GetPortValue for PortValueGetter<'_> {
    fn get_port_value(&mut self, port_symbol: &str) -> (Value, u32, u32) {
        let value_ptr = self
            .symbols
            .get(port_symbol)
            .and_then(|index| self.controls.value_ptr(*index));
        match value_ptr {
            Some(ptr) => (
                ptr.cast(),
                std::mem::size_of::<f32>() as u32,
                self.float_urid,
            ),
            None => (std::ptr::null(), 0, 0),
        }
    }
}

/// Sets control input port values from lilv when restoring a state.
pub(crate) struct PortValueSetter<'a> {
    pub controls: &'a mut Controls,
    pub symbols: &'a HashMap<String, PortIndex>,
    pub features: &'a Features,
}

impl PortValueSetter<'_> {
    fn value_as_f32(&self, value: Value, size: u32, type_: u32) -> Option<f32> {
        let urid = |uri: &[u8]| self.features.urid(CStr::from_bytes_with_nul(uri).unwrap());
        let size = size as usize;
        // Safety: lilv guarantees that `value` points to `size` bytes of data
        // with the type `type_`.
        unsafe {
            if type_ == urid(lv2_sys::LV2_ATOM__Float) && size == size_of::<f32>() {
                Some(*value.cast::<f32>())
            } else if type_ == urid(lv2_sys::LV2_ATOM__Double) && size == size_of::<f64>() {
                Some(*value.cast::<f64>() as f32)
            } else if (type_ == urid(lv2_sys::LV2_ATOM__Int)
                || type_ == urid(lv2_sys::LV2_ATOM__Bool))
                && size == size_of::<i32>()
            {
                Some(*value.cast::<i32>() as f32)
            } else if type_ == urid(lv2_sys::LV2_ATOM__Long) && size == size_of::<i64>() {
                Some(*value.cast::<i64>() as f32)
            } else {
                None
            }
        }
    }
}

impl SetPortValue for PortValueSetter<'_> {
    fn set_port_value(&mut self, port_symbol: &str, value: Value, size: u32, type_: u32) {
        let Some(index) = self.symbols.get(port_symbol).copied() else {
            warn!("State has value for unknown control input port {port_symbol}.");
            return;
        };
        match self.value_as_f32(value, size, type_) {
            Some(v) => {
                self.controls.set(index, v);
            }
            None => warn!(
                "State has unsupported value of type {:?} for port {port_symbol}.",
                self.features.uri(type_)
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::error::StateError;
    use crate::{FeaturesBuilder, Instance, PortIndex, World};

    const EG_AMP: &str = "http://lv2plug.in/plugins/eg-amp";

    fn instantiate(world: &World, features: &std::sync::Arc<crate::Features>) -> Instance {
        unsafe {
            world
                .plugin_by_uri(EG_AMP)
                .expect("eg-amp not found")
                .instantiate(features.clone(), 44100.0)
                .expect("could not instantiate eg-amp")
        }
    }

    fn gain_index(instance: &Instance) -> PortIndex {
        instance.iter_control_inputs().next().unwrap().port_index
    }

    #[test]
    fn state_round_trips_through_string() {
        let world = World::new();
        let features = world
            .try_build_features(FeaturesBuilder::default())
            .expect("invalid feature block bounds");
        let mut instance = instantiate(&world, &features);
        let gain = gain_index(&instance);
        assert_eq!(instance.set_control_input(gain, -12.0), Some(-12.0));

        let mut state = instance.state().unwrap();
        state.set_label("quiet");
        assert_eq!(state.label(), Some("quiet"));
        assert_eq!(state.plugin_uri(), EG_AMP);
        assert_eq!(state.uri(), None);
        let serialized = state.to_string("urn:livi:test:quiet").unwrap();

        let loaded = world.state_from_string(&features, &serialized).unwrap();
        assert!(loaded == state);
        assert_eq!(loaded.uri().as_deref(), Some("urn:livi:test:quiet"));
        assert_eq!(loaded.label(), Some("quiet"));

        let mut other = instantiate(&world, &features);
        assert_eq!(other.control_input(gain), Some(0.0));
        other.restore_state(&loaded).unwrap();
        assert_eq!(other.control_input(gain), Some(-12.0));
    }

    #[test]
    fn state_round_trips_through_file() {
        let world = World::new();
        let features = world
            .try_build_features(FeaturesBuilder::default())
            .expect("invalid feature block bounds");
        let mut instance = instantiate(&world, &features);
        let gain = gain_index(&instance);
        instance.set_control_input(gain, 6.0);

        let mut state = instance.state().unwrap();
        state.set_label("loud");
        let dir = tempfile::tempdir().unwrap();
        let bundle = dir.path().join("loud.lv2");
        let uri = "urn:livi:test:loud";
        state.save(&bundle, "loud.ttl", Some(uri)).unwrap();

        let loaded = world
            .state_from_file(&features, Some(uri), &bundle.join("loud.ttl"))
            .unwrap();
        assert_eq!(loaded.label(), Some("loud"));
        let mut other = instantiate(&world, &features);
        other.restore_state(&loaded).unwrap();
        assert_eq!(other.control_input(gain), Some(6.0));
    }

    #[test]
    fn state_from_uri_loads_plugin_default_state() {
        let world = World::new();
        let features = world
            .try_build_features(FeaturesBuilder::default())
            .expect("invalid feature block bounds");
        let mut instance = instantiate(&world, &features);
        let gain = gain_index(&instance);
        instance.set_control_input(gain, 3.0);

        let default_state = world.state_from_uri(&features, EG_AMP).unwrap();
        instance.restore_state(&default_state).unwrap();
        assert_eq!(instance.control_input(gain), Some(0.0));
    }

    #[test]
    fn state_from_string_with_invalid_state_fails() {
        let world = World::new();
        let features = world
            .try_build_features(FeaturesBuilder::default())
            .expect("invalid feature block bounds");
        assert_eq!(
            world
                .state_from_string(&features, "not turtle")
                .unwrap_err(),
            StateError::LoadFailed
        );
    }

    #[test]
    fn restore_state_from_other_plugin_fails() {
        let world = World::new();
        let features = world
            .try_build_features(FeaturesBuilder::default())
            .expect("invalid feature block bounds");
        let mut instance = instantiate(&world, &features);
        let fifths = unsafe {
            world
                .plugin_by_uri("http://lv2plug.in/plugins/eg-fifths")
                .expect("eg-fifths not found")
                .instantiate(features.clone(), 44100.0)
                .expect("could not instantiate eg-fifths")
        };
        let state = fifths.state().unwrap();
        assert_eq!(
            instance.restore_state(&state).unwrap_err(),
            StateError::PluginMismatch {
                expected: EG_AMP.to_string(),
                actual: "http://lv2plug.in/plugins/eg-fifths".to_string(),
            }
        );
    }
}
