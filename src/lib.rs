//! livi is a library for hosting LV2 plugins in Rust.
//! ```
//! use livi;
//!
//! let mut world = livi::World::new();
//! const MIN_BLOCK_SIZE: usize = 1;
//! const MAX_BLOCK_SIZE: usize = 256;
//! const SAMPLE_RATE: f64 = 44100.0;
//! world
//!     .initialize_block_length(MIN_BLOCK_SIZE, MAX_BLOCK_SIZE)
//!     .unwrap();
//! let plugin = world
//!     .plugin_by_uri("http://drobilla.net/plugins/mda/EPiano")
//!     .expect("Plugin not found.");
//! let mut instance = unsafe {
//!     plugin
//!         .instantiate(SAMPLE_RATE)
//!         .expect("Could not instantiate plugin.")
//! };
//! let input = {
//!     let mut s = livi::event::LV2AtomSequence::new(1024);
//!     let play_note_data = [0x90, 0x40, 0x7f];
//!     s.push_midi_event::<3>(1, world.midi_urid(), &play_note_data)
//!         .unwrap();
//!     s
//! };
//! let params: Vec<f32> = plugin
//!     .ports_with_type(livi::PortType::ControlInput)
//!     .map(|p| p.default_value)
//!     .collect();
//! let mut outputs = [vec![0.0; MAX_BLOCK_SIZE], vec![0.0; MAX_BLOCK_SIZE]];
//! let ports = livi::EmptyPortConnections::new(MAX_BLOCK_SIZE)
//!     .with_atom_sequence_inputs(std::iter::once(&input))
//!     .with_audio_outputs(outputs.iter_mut().map(|output| output.as_mut_slice()))
//!     .with_control_inputs(params.iter());
//! unsafe { instance.run(ports).unwrap() };
//! ```
use crate::error::InitializeBlockLengthError;
use crate::features::Features;
use log::{debug, error, info, warn};
use std::sync::{Arc, Mutex};

pub use plugin::{Instance, Plugin};
pub use port::{EmptyPortConnections, Port, PortConnections, PortCounts, PortIndex, PortType};

/// Contains all the error types for the `livi` crate.
pub mod error;
/// Contains utility for dealing with `LV2` events.
pub mod event;
mod features;
mod plugin;
mod port;

/// Contains all plugins.
pub struct World {
    resources: Arc<Resources>,
    livi_plugins: Vec<Plugin>,
}

impl World {
    /// Create a new world that includes all plugins that are found and are
    /// supported.  Plugins that are not supported will be listed with a `warn!`
    /// message.
    ///
    /// # Panics
    /// Panics if the world resources mutex could not be locked.
    #[must_use]
    pub fn new() -> World {
        World::with_plugin_predicate(|_| true)
    }

    /// Creates a new world that includes all plugins that are found and return
    /// `true` for `predicate.
    #[must_use]
    pub fn with_plugin_predicate<P>(predicate: P) -> World
    where
        P: Fn(&Plugin) -> bool,
    {
        let world = lilv::World::with_load_all();
        let resources = Arc::new(Resources::new(&world));
        let supported_features = resources.features.lock().unwrap().supported_features();
        info!(
            "Creating World with supported features {:?}",
            supported_features
        );
        let plugins: Vec<Plugin> = world
            .plugins()
            .into_iter()
            .filter(|p| {
                let is_supported = p
                    .required_features()
                    .into_iter()
                    .all(|f| supported_features.contains(f.as_uri().unwrap_or("")));
                if !is_supported {
                    warn!(
                        "Plugin {} requires unsupported features: {:?}",
                        p.uri().as_uri().unwrap_or("BAD_URI"),
                        p.required_features()
                    );
                }
                is_supported
            })
            .filter(|p| {
                if p.name().as_str().is_none() {
                    error!("Plugin {:?} did not return a string name.", p);
                    return false;
                }
                if p.uri().as_str().is_none() {
                    error!("Plugin {:?} did not return a valid uri.", p);
                    return false;
                }
                true
            })
            .filter(|p| {
                for port in p.iter_ports() {
                    for class in port.classes() {
                        if class != resources.input_port_uri
                            && class != resources.output_port_uri
                            && class != resources.audio_port_uri
                            && class != resources.control_port_uri
                            && class != resources.atom_port_uri
                            && class != resources.cv_port_uri
                        {
                            error!("Port class {:?} is not supported.", class);
                            return false;
                        }
                    }
                    if !port.is_a(&resources.input_port_uri)
                        && !port.is_a(&resources.output_port_uri)
                    {
                        error!(
                            "Port {:?} for plugin {} is neither an input or output.",
                            port,
                            p.uri().as_str().unwrap_or("BAD_URI")
                        );
                        return false;
                    }
                    if !port.is_a(&resources.audio_port_uri) && !port.is_a(&resources.control_port_uri) && !port.is_a(&resources.atom_port_uri) && !port.is_a(&resources.cv_port_uri) {
                        error!(
                            "Port {:?}for plugin {} not a recognized data type. Supported types are Audio and Control", port, p.uri().as_str().unwrap_or("BAD_URI")
                        );
                        return false;
                    }
                }
                true
            })
            .map(|p| Plugin::from_raw(p, resources.clone()))
            .filter(|p| {
                let keep = predicate(p);
                if !keep {
                    debug!("Ignoring plugin {} due to predicate.", p.uri());
                }
                keep
            })
            .inspect(|p| info!("Found plugin {}: {}", p.name(), p.uri()))
            .collect();
        World {
            resources,
            livi_plugins: plugins,
        }
    }

    /// Get the URID of a URI. This value is only guaranteed to be valid for
    /// instances spawned from this world. It is not guaranteed to be stable
    /// across different runs.
    ///
    /// # Panics
    /// Panics if the world resource mutex could not be locked.
    #[must_use]
    pub fn urid(&self, uri: &std::ffi::CStr) -> lv2_raw::LV2Urid {
        self.resources.features.lock().unwrap().urid_map.map(uri)
    }

    /// The URID for midi events.
    ///
    /// # Panics
    /// Panics if a `CStr` could not be built for the Midi URI. This behavior is
    /// well tested and of negligible risk.
    #[must_use]
    pub fn midi_urid(&self) -> lv2_raw::LV2Urid {
        self.urid(
            std::ffi::CStr::from_bytes_with_nul(b"http://lv2plug.in/ns/ext/midi#MidiEvent\0")
                .unwrap(),
        )
    }

    /// Iterate through all plugins.
    pub fn iter_plugins(&self) -> impl '_ + ExactSizeIterator + Iterator<Item = Plugin> {
        self.livi_plugins.iter().cloned()
    }

    /// Return the plugin given a URI or `None` if it does not exist.
    #[must_use]
    pub fn plugin_by_uri(&self, uri: &str) -> Option<Plugin> {
        self.iter_plugins().find(|p| p.uri() == uri)
    }

    /// Initialize the block length. This is the minimum and maximum number of
    /// samples that are processed per `run` method. This must be called before
    /// any plugins are instantiated and may only be called once.
    ///
    /// # Errors
    /// Returns an error if the block lengths are invalid.
    ///
    /// # Panics
    /// Panics if the world resource mutex could not be locked.
    pub fn initialize_block_length(
        &mut self,
        min_block_length: usize,
        max_block_length: usize,
    ) -> Result<(), InitializeBlockLengthError> {
        self.resources
            .features
            .lock()
            .unwrap()
            .initialize_block_length(min_block_length, max_block_length)
    }
}

impl Default for World {
    fn default() -> Self {
        Self::new()
    }
}

struct Resources {
    input_port_uri: lilv::node::Node,
    output_port_uri: lilv::node::Node,
    control_port_uri: lilv::node::Node,
    audio_port_uri: lilv::node::Node,
    atom_port_uri: lilv::node::Node,
    cv_port_uri: lilv::node::Node,
    features: Mutex<Features>,
}

impl Resources {
    fn new(world: &lilv::World) -> Resources {
        Resources {
            input_port_uri: world.new_uri("http://lv2plug.in/ns/lv2core#InputPort"),
            output_port_uri: world.new_uri("http://lv2plug.in/ns/lv2core#OutputPort"),
            control_port_uri: world.new_uri("http://lv2plug.in/ns/lv2core#ControlPort"),
            audio_port_uri: world.new_uri("http://lv2plug.in/ns/lv2core#AudioPort"),
            atom_port_uri: world.new_uri("http://lv2plug.in/ns/ext/atom#AtomPort"),
            cv_port_uri: world.new_uri("http://lv2plug.in/ns/lv2core#CVPort"),
            features: Mutex::new(Features::new()),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use crate::event::LV2AtomSequence;

    use super::*;

    const MIN_BLOCK_SIZE: usize = 1;
    const MAX_BLOCK_SIZE: usize = 256;
    const SAMPLE_RATE: f64 = 44100.0;

    #[test]
    fn test_midi_urid_ok() {
        let world = World::new();
        assert!(world.midi_urid() > 0, "midi urid is not valid");
    }

    #[test]
    fn test_mda_epiano() {
        let mut world = World::new();
        world
            .initialize_block_length(MIN_BLOCK_SIZE, MAX_BLOCK_SIZE)
            .unwrap();
        let plugin = world
            .plugin_by_uri("http://drobilla.net/plugins/mda/EPiano")
            .expect("Plugin not found.");
        assert_eq!(
            *plugin.port_counts(),
            PortCounts {
                control_inputs: 12,
                control_outputs: 0,
                audio_inputs: 0,
                audio_outputs: 2,
                atom_sequence_inputs: 1,
                atom_sequence_outputs: 0,
                cv_inputs: 0,
                cv_outputs: 0,
            }
        );
        let mut instance = unsafe {
            plugin
                .instantiate(SAMPLE_RATE)
                .expect("Could not instantiate plugin.")
        };
        assert_eq!(
            instance.port_counts(),
            PortCounts {
                control_inputs: 12,
                control_outputs: 0,
                audio_inputs: 0,
                audio_outputs: 2,
                atom_sequence_inputs: 1,
                atom_sequence_outputs: 0,
                cv_inputs: 0,
                cv_outputs: 0,
            }
        );
        let input = {
            let mut s = LV2AtomSequence::new(&world, 1024);
            let play_note_data = [0x90, 0x40, 0x7f];
            s.push_midi_event::<3>(1, world.midi_urid(), &play_note_data)
                .unwrap();
            s
        };
        let params: Vec<f32> = plugin
            .ports_with_type(PortType::ControlInput)
            .map(|p| p.default_value)
            .collect();
        let mut outputs = [vec![0.0; MAX_BLOCK_SIZE], vec![0.0; MAX_BLOCK_SIZE]];
        for block_size in MIN_BLOCK_SIZE..MAX_BLOCK_SIZE {
            let ports = EmptyPortConnections::new(block_size)
                .with_atom_sequence_inputs(std::iter::once(&input))
                .with_audio_outputs(outputs.iter_mut().map(|output| output.as_mut_slice()))
                .with_control_inputs(params.iter());
            unsafe { instance.run(ports).unwrap() };
        }
    }

    #[test]
    fn test_with_filter() {
        let uri = "http://drobilla.net/plugins/mda/EPiano";

        // EPiano only.
        let world = World::with_plugin_predicate(|p| p.uri() == uri);
        assert!(world.plugin_by_uri(uri).is_some());
        assert_eq!(world.iter_plugins().count(), 1);

        // No EPiano.
        assert!(World::with_plugin_predicate(|p| p.uri() != uri)
            .plugin_by_uri(uri)
            .is_none());

        // Empty.
        assert_eq!(
            World::with_plugin_predicate(|_| false)
                .iter_plugins()
                .count(),
            0
        );
    }

    #[test]
    fn test_supported_features() {
        let want: HashSet<String> = [
            "http://lv2plug.in/ns/ext/urid#map",
            "http://lv2plug.in/ns/ext/urid#unmap",
            "http://lv2plug.in/ns/ext/options#options",
            "http://lv2plug.in/ns/ext/buf-size#boundedBlockLength",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect();
        assert_eq!(
            want,
            World::new()
                .resources
                .features
                .lock()
                .unwrap()
                .supported_features()
        );
    }
}
