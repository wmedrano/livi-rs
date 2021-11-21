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
use crate::event::LV2AtomSequence;
use crate::features::Features;
use error::{InitializeBlockLengthError, InstantiateError, RunError};
use log::{debug, error, info, warn};
use port::{DataType, IOType};
use std::{
    fmt::Debug,
    sync::{Arc, Mutex},
};

pub use port::{EmptyPortConnections, Port, PortConnections, PortIndex, PortType};

/// Contains all the error types for the `livi` crate.
pub mod error;
/// Contains utility for dealing with `LV2` events.
pub mod event;
mod features;
mod port;

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

/// Contains all plugins.
pub struct World {
    plugins: Vec<lilv::plugin::Plugin>,
    resources: Arc<Resources>,
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
        let plugins = world
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
            .filter(|p| {
                let keep = predicate(&Plugin{ inner: p.clone(), resources: resources.clone()});
                if !keep {
                    debug!("Ignoring plugin {} due to predicate.", p.uri().as_str().unwrap_or("BAD_URI"));
                }
                keep
            })
            .inspect(|p| info!("Found plugin {}: {}", p.name().as_str().unwrap_or("BAD_NAME"), p.uri().as_str().unwrap_or("BAD_URI")))
            .collect();
        World { plugins, resources }
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
        self.plugins.iter().map(move |p| Plugin {
            inner: p.clone(),
            resources: self.resources.clone(),
        })
    }

    /// Return the plugin given a URI or `None` if it does not exist.
    #[must_use]
    pub fn plugin_by_uri(&self, uri: &str) -> Option<Plugin> {
        self.plugins
            .iter()
            .find(|p| p.uri().as_str() == Some(uri))
            .map(|p| Plugin {
                inner: p.clone(),
                resources: self.resources.clone(),
            })
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

/// A plugin that can be used to instantiate plugin instances.
#[derive(Clone)]
pub struct Plugin {
    inner: lilv::plugin::Plugin,
    resources: Arc<Resources>,
}

impl Plugin {
    /// A unique identifier for the plugin.
    #[must_use]
    pub fn uri(&self) -> String {
        self.inner.uri().as_str().unwrap_or("BAD_URI").to_string()
    }

    /// The name of the plugin.
    #[must_use]
    pub fn name(&self) -> String {
        self.inner.name().as_str().unwrap_or("BAD_NAME").to_string()
    }

    /// Create a new instance of the plugin.
    ///
    /// # Errors
    /// Returns an error if the plugin could not be instantiated.
    ///
    /// # Safety
    /// Running plugin code is unsafe.
    ///
    /// # Panics
    /// Panics if the world resource mutex could not be locked.
    pub unsafe fn instantiate(&self, sample_rate: f64) -> Result<Instance, InstantiateError> {
        let features = self.resources.features.lock().unwrap();
        if features.min_and_max_block_length.is_none() {
            return Err(InstantiateError::BlockLengthNotInitialized);
        }
        let instance = self
            .inner
            .instantiate(sample_rate, features.iter_features())
            .ok_or(InstantiateError::UnknownError)?;
        let mut control_inputs = Vec::new();
        let mut control_outputs = Vec::new();
        let mut audio_inputs = Vec::new();
        let mut audio_outputs = Vec::new();
        let mut atom_sequence_inputs = Vec::new();
        let mut atom_sequence_outputs = Vec::new();
        let mut cv_inputs = Vec::new();
        let mut cv_outputs = Vec::new();
        for port in self.ports() {
            match port.port_type {
                PortType::ControlInput => control_inputs.push(port.index),
                PortType::ControlOutput => control_outputs.push(port.index),
                PortType::AudioInput => audio_inputs.push(port.index),
                PortType::AudioOutput => audio_outputs.push(port.index),
                PortType::AtomSequenceInput => atom_sequence_inputs.push(port.index),
                PortType::AtomSequenceOutput => atom_sequence_outputs.push(port.index),
                PortType::CVInput => cv_inputs.push(port.index),
                PortType::CVOutput => cv_outputs.push(port.index),
            }
        }
        Ok(Instance {
            inner: instance.activate(),
            control_inputs,
            control_outputs,
            audio_inputs,
            audio_outputs,
            atom_sequence_inputs,
            atom_sequence_outputs,
            cv_inputs,
            cv_outputs,
        })
    }

    /// Iterate over all ports for the plugin.
    pub fn ports(&self) -> impl '_ + Iterator<Item = Port> {
        self.inner.iter_ports().map(move |p| {
            let io_type = if p.is_a(&self.resources.input_port_uri) {
                IOType::Input
            } else if p.is_a(&self.resources.output_port_uri) {
                IOType::Output
            } else {
                unreachable!("Port is neither input or output.")
            };
            let data_type = if p.is_a(&self.resources.audio_port_uri) {
                DataType::Audio
            } else if p.is_a(&self.resources.control_port_uri) {
                DataType::Control
            } else if p.is_a(&self.resources.atom_port_uri) {
                DataType::AtomSequence
            } else if p.is_a(&self.resources.cv_port_uri) {
                DataType::CV
            } else {
                unreachable!("Port is not an audio, control, or atom sequence port.")
            };
            let port_type = match (io_type, data_type) {
                (IOType::Input, DataType::Control) => PortType::ControlInput,
                (IOType::Output, DataType::Control) => PortType::ControlOutput,
                (IOType::Input, DataType::Audio) => PortType::AudioInput,
                (IOType::Output, DataType::Audio) => PortType::AudioOutput,
                (IOType::Input, DataType::AtomSequence) => PortType::AtomSequenceInput,
                (IOType::Output, DataType::AtomSequence) => PortType::AtomSequenceOutput,
                (IOType::Input, DataType::CV) => PortType::CVInput,
                (IOType::Output, DataType::CV) => PortType::CVOutput,
            };
            Port {
                port_type,
                name: p
                    .name()
                    .expect("port has no name")
                    .as_str()
                    .unwrap_or("BAD_NAME")
                    .to_string(),
                default_value: p
                    .range()
                    .default
                    .map_or(0.0, |n| n.as_float().unwrap_or(0.0)),
                index: PortIndex(p.index()),
            }
        })
    }

    /// Return all ports with the given type.
    pub fn ports_with_type(&self, port_type: PortType) -> impl '_ + Iterator<Item = Port> {
        self.ports().filter(move |p| p.port_type == port_type)
    }
}

impl Debug for Plugin {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let ports = PortsDebug { plugin: self };
        f.debug_struct("Plugin")
            .field("uri", &self.uri())
            .field("name", &self.name())
            .field("ports", &ports)
            .finish()
    }
}

struct PortsDebug<'a> {
    plugin: &'a Plugin,
}

impl<'a> Debug for PortsDebug<'a> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_list().entries(self.plugin.ports()).finish()
    }
}

/// An instance of a plugin that can process inputs and outputs.
pub struct Instance {
    inner: lilv::instance::ActiveInstance,
    control_inputs: Vec<PortIndex>,
    control_outputs: Vec<PortIndex>,
    audio_inputs: Vec<PortIndex>,
    audio_outputs: Vec<PortIndex>,
    atom_sequence_inputs: Vec<PortIndex>,
    atom_sequence_outputs: Vec<PortIndex>,
    cv_inputs: Vec<PortIndex>,
    cv_outputs: Vec<PortIndex>,
}

impl Instance {
    /// Run the plugin for a given number of samples.
    ///
    /// # Safety
    /// Running plugin code is unsafe.
    ///
    /// # Errors
    /// Returns an error if the plugin could not be run.
    pub unsafe fn run<
        'a,
        ControlInputs,
        ControlOutputs,
        AudioInputs,
        AudioOutputs,
        AtomSequenceInputs,
        AtomSequenceOutputs,
        CVInputs,
        CVOutputs,
    >(
        &mut self,
        ports: PortConnections<
            'a,
            ControlInputs,
            ControlOutputs,
            AudioInputs,
            AudioOutputs,
            AtomSequenceInputs,
            AtomSequenceOutputs,
            CVInputs,
            CVOutputs,
        >,
    ) -> Result<(), RunError>
    where
        ControlInputs: ExactSizeIterator + Iterator<Item = &'a f32>,
        ControlOutputs: ExactSizeIterator + Iterator<Item = &'a mut f32>,
        AudioInputs: ExactSizeIterator + Iterator<Item = &'a [f32]>,
        AudioOutputs: ExactSizeIterator + Iterator<Item = &'a mut [f32]>,
        AtomSequenceInputs: ExactSizeIterator + Iterator<Item = &'a LV2AtomSequence>,
        AtomSequenceOutputs: ExactSizeIterator + Iterator<Item = &'a mut LV2AtomSequence>,
        CVInputs: ExactSizeIterator + Iterator<Item = &'a [f32]>,
        CVOutputs: ExactSizeIterator + Iterator<Item = &'a mut [f32]>,
    {
        if ports.control_inputs.len() != self.control_inputs.len() {
            return Err(RunError::ControlInputsSizeMismatch {
                expected: self.control_inputs.len(),
                actual: ports.control_inputs.len(),
            });
        }
        for (data, index) in ports.control_inputs.zip(self.control_inputs.iter()) {
            self.inner.instance_mut().connect_port(index.0, data);
        }
        if ports.control_outputs.len() != self.control_outputs.len() {
            return Err(RunError::ControlOutputsSizeMismatch {
                expected: self.control_outputs.len(),
                actual: ports.control_outputs.len(),
            });
        }
        for (data, index) in ports.control_outputs.zip(self.control_outputs.iter()) {
            self.inner.instance_mut().connect_port_mut(index.0, data);
        }
        if ports.audio_inputs.len() != self.audio_inputs.len() {
            return Err(RunError::AudioInputsSizeMismatch {
                expected: self.audio_inputs.len(),
                actual: ports.audio_inputs.len(),
            });
        }
        for (data, index) in ports.audio_inputs.zip(self.audio_inputs.iter()) {
            self.inner
                .instance_mut()
                .connect_port(index.0, data.as_ptr());
        }
        if ports.audio_outputs.len() != self.audio_outputs.len() {
            return Err(RunError::AudioOutputsSizeMismatch {
                expected: self.audio_outputs.len(),
                actual: ports.audio_outputs.len(),
            });
        }
        for (data, index) in ports.audio_outputs.zip(self.audio_outputs.iter()) {
            self.inner
                .instance_mut()
                .connect_port_mut(index.0, data.as_mut_ptr());
        }
        if ports.atom_sequence_inputs.len() != self.atom_sequence_inputs.len() {
            return Err(RunError::AtomSequenceInputsSizeMismatch {
                expected: self.atom_sequence_inputs.len(),
                actual: ports.atom_sequence_inputs.len(),
            });
        }
        for (data, index) in ports
            .atom_sequence_inputs
            .zip(self.atom_sequence_inputs.iter())
        {
            self.inner
                .instance_mut()
                .connect_port(index.0, data.as_ptr());
        }
        if ports.atom_sequence_outputs.len() != self.atom_sequence_outputs.len() {
            return Err(RunError::AtomSequenceOutputsSizeMismatch {
                expected: self.atom_sequence_outputs.len(),
                actual: ports.atom_sequence_outputs.len(),
            });
        }
        for (data, index) in ports
            .atom_sequence_outputs
            .zip(self.atom_sequence_outputs.iter())
        {
            self.inner
                .instance_mut()
                .connect_port_mut(index.0, data.as_mut_ptr());
        }
        if ports.cv_inputs.len() != self.cv_inputs.len() {
            return Err(RunError::CVInputsSizeMismatch {
                expected: self.cv_inputs.len(),
                actual: ports.cv_inputs.len(),
            });
        }
        for (data, index) in ports.cv_inputs.zip(self.cv_inputs.iter()) {
            self.inner
                .instance_mut()
                .connect_port(index.0, data.as_ptr());
        }
        if ports.cv_outputs.len() != self.cv_outputs.len() {
            return Err(RunError::CVOutputsSizeMismatch {
                expected: self.cv_outputs.len(),
                actual: ports.cv_outputs.len(),
            });
        }
        for (data, index) in ports.cv_outputs.zip(self.cv_outputs.iter()) {
            self.inner
                .instance_mut()
                .connect_port_mut(index.0, data.as_mut_ptr());
        }
        self.inner.run(ports.sample_count);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

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
        let mut instance = unsafe {
            plugin
                .instantiate(SAMPLE_RATE)
                .expect("Could not instantiate plugin.")
        };
        let input = {
            let mut s = LV2AtomSequence::new(1024);
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
