use crate::error::Run as RunError;
use crate::event::LV2AtomSequence;
use log::{error, info, warn};
use lv2_raw::LV2Feature;
use std::convert::TryFrom;
use std::{
    collections::HashSet,
    ffi::CStr,
    sync::{Arc, Mutex},
};

/// Contains all the error types for the `livi` crate.
pub mod error;
/// Contains utility for dealing with `LV2` events.
pub mod event;
mod features;

struct Resources {
    input_port_uri: lilv::node::Node,
    output_port_uri: lilv::node::Node,
    control_port_uri: lilv::node::Node,
    audio_port_uri: lilv::node::Node,
    atom_port_uri: lilv::node::Node,
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
            features: Mutex::new(Features::new()),
        }
    }
}

struct Features {
    urid_map: features::urid_map::UridMap,
    options: features::options::Options,
    min_and_max_block_length: Option<(usize, usize)>,
}

impl Features {
    fn new() -> Features {
        Features {
            urid_map: features::urid_map::UridMap::new(),
            options: features::options::Options::new(),
            min_and_max_block_length: None,
        }
    }

    fn initialize_block_length(
        &mut self,
        min_block_length: usize,
        max_block_length: usize,
    ) -> Result<(), error::InitializeBlockLength> {
        if let Some((min_block_length, max_block_length)) = self.min_and_max_block_length {
            return Err(
                error::InitializeBlockLength::BlockLengthAlreadyInitialized {
                    min_block_length,
                    max_block_length,
                },
            );
        }
        let min = i32::try_from(min_block_length).map_err(|_| {
            error::InitializeBlockLength::MinBlockLengthTooLarge {
                max_supported: i32::MAX as usize,
                actual: min_block_length,
            }
        })?;
        let max = i32::try_from(max_block_length).map_err(|_| {
            error::InitializeBlockLength::MaxBlockLengthTooLarge {
                max_supported: i32::MAX as usize,
                actual: max_block_length,
            }
        })?;
        self.options.set_int_option(
            &self.urid_map,
            self.urid_map.map(
                CStr::from_bytes_with_nul(b"http://lv2plug.in/ns/ext/buf-size#minBlockLength\0")
                    .unwrap(),
            ),
            min,
        );
        self.options.set_int_option(
            &self.urid_map,
            self.urid_map.map(
                CStr::from_bytes_with_nul(b"http://lv2plug.in/ns/ext/buf-size#maxBlockLength\0")
                    .unwrap(),
            ),
            max,
        );
        self.min_and_max_block_length = Some((min_block_length, max_block_length));
        Ok(())
    }

    /// Get the URIs for all supported features.
    fn supported_features(&self) -> HashSet<String> {
        self.iter_features()
            .map(|f| {
                unsafe { std::ffi::CStr::from_ptr(f.uri) }
                    .to_string_lossy()
                    .into_owned()
            })
            .collect()
    }

    /// Iterate over all supported features.
    fn iter_features(&self) -> impl Iterator<Item = &'_ LV2Feature> {
        std::iter::once(self.urid_map.as_urid_map_feature())
            .chain(std::iter::once(self.urid_map.as_urid_unmap_feature()))
            .chain(std::iter::once(self.options.as_feature()))
            .chain(std::iter::once(&features::BOUNDED_BLOCK_LENGTH))
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
                    if !port.is_a(&resources.audio_port_uri) && !port.is_a(&resources.control_port_uri) && !port.is_a(&resources.atom_port_uri) {
                        error!(
                            "Port {:?}for plugin {} not a recognized data type. Supported types are Audio and Control", port, p.uri().as_str().unwrap_or("BAD_URI")
                        );
                        return false;
                    }
                }
                true
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
    ) -> Result<(), error::InitializeBlockLength> {
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
    pub unsafe fn instantiate(&self, sample_rate: f64) -> Result<Instance, error::Instantiate> {
        let features = self.resources.features.lock().unwrap();
        if features.min_and_max_block_length.is_none() {
            return Err(error::Instantiate::BlockLengthNotInitialized);
        }
        let instance = self
            .inner
            .instantiate(sample_rate, features.iter_features())
            .ok_or(error::Instantiate::UnknownError)?;
        let mut control_inputs = Vec::new();
        let mut control_outputs = Vec::new();
        let mut audio_inputs = Vec::new();
        let mut audio_outputs = Vec::new();
        let mut atom_sequence_inputs = Vec::new();
        let mut atom_sequence_outputs = Vec::new();
        for port in self.ports() {
            match port.port_type {
                PortType::ControlInput => control_inputs.push(port.index),
                PortType::ControlOutput => control_outputs.push(port.index),
                PortType::AudioInput => audio_inputs.push(port.index),
                PortType::AudioOutput => audio_outputs.push(port.index),
                PortType::AtomSequenceInput => atom_sequence_inputs.push(port.index),
                PortType::AtomSequenceOutput => atom_sequence_outputs.push(port.index),
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

/// The type of IO for the port. Either input or output.
#[derive(Copy, Clone, Debug)]
enum IOType {
    // The data is an input to the plugin. Usually this corresponds to an `&`
    // and not an `&mut`.
    Input,
    // The data is an input to the plugin. Usually this corresponds to an `&mut`
    // and not an `&`.
    Output,
}

/// The data type pointed to by the port.
#[derive(Copy, Clone, Debug)]
enum DataType {
    /// A single f32.
    Control,
    /// An `[f32]`.
    Audio,
    /// An LV2 atom sequence.
    AtomSequence,
}

/// The type of port.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum PortType {
    /// A single `&f32`.
    ControlInput,
    /// A single `&mut f32`. This is not yet supported.
    ControlOutput,
    /// An `&[f32]`.
    AudioInput,
    /// An `&mut [f32]`.
    AudioOutput,
    /// LV2 atom sequence input. This is used to handle midi, among other
    /// things.
    AtomSequenceInput,
    /// LV2 atom sequence output. This is used to output midi, among other
    /// things.
    AtomSequenceOutput,
}

/// A port represents a connection (either input or output) to a plugin.
pub struct Port {
    /// The type of port.
    pub port_type: PortType,

    /// The name of the port.
    pub name: String,

    /// The default value for the port if it is a `ControlInputs`.
    pub default_value: f32,

    /// The index of this port within the plugin.
    pub index: PortIndex,
}

/// All the inputs and outputs for an instance.
pub struct PortConnections<
    'a,
    ControlInputs,
    ControlOutputs,
    AudioInputs,
    AudioOutputs,
    AtomSequenceInputs,
    AtomSequenceOutputs,
> where
    ControlInputs: ExactSizeIterator + Iterator<Item = &'a f32>,
    ControlOutputs: ExactSizeIterator + Iterator<Item = &'a mut f32>,
    AudioInputs: ExactSizeIterator + Iterator<Item = &'a [f32]>,
    AudioOutputs: ExactSizeIterator + Iterator<Item = &'a mut [f32]>,
    AtomSequenceInputs: ExactSizeIterator + Iterator<Item = &'a LV2AtomSequence>,
    AtomSequenceOutputs: ExactSizeIterator + Iterator<Item = &'a mut LV2AtomSequence>,
{
    /// The number of audio samples that will be processed.
    pub sample_count: usize,

    /// The control inputs.
    pub control_input: ControlInputs,

    /// The control outputs.
    pub control_output: ControlOutputs,

    /// The audio inputs.
    pub audio_input: AudioInputs,

    /// The audio outputs.
    pub audio_output: AudioOutputs,

    /// The events input.
    pub atom_sequence_input: AtomSequenceInputs,

    /// The events output.
    pub atom_sequence_output: AtomSequenceOutputs,
}

/// The index of the port within a plugin.
pub struct PortIndex(pub usize);

/// An instance of a plugin that can process inputs and outputs.
pub struct Instance {
    inner: lilv::instance::ActiveInstance,
    control_inputs: Vec<PortIndex>,
    control_outputs: Vec<PortIndex>,
    audio_inputs: Vec<PortIndex>,
    audio_outputs: Vec<PortIndex>,
    atom_sequence_inputs: Vec<PortIndex>,
    atom_sequence_outputs: Vec<PortIndex>,
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
        >,
    ) -> Result<(), RunError>
    where
        ControlInputs: ExactSizeIterator + Iterator<Item = &'a f32>,
        ControlOutputs: ExactSizeIterator + Iterator<Item = &'a mut f32>,
        AudioInputs: ExactSizeIterator + Iterator<Item = &'a [f32]>,
        AudioOutputs: ExactSizeIterator + Iterator<Item = &'a mut [f32]>,
        AtomSequenceInputs: ExactSizeIterator + Iterator<Item = &'a LV2AtomSequence>,
        AtomSequenceOutputs: ExactSizeIterator + Iterator<Item = &'a mut LV2AtomSequence>,
    {
        if ports.control_input.len() != self.control_inputs.len() {
            return Err(RunError::ControlInputsSizeMismatch {
                expected: self.control_inputs.len(),
                actual: ports.control_input.len(),
            });
        }
        for (data, index) in ports.control_input.zip(self.control_inputs.iter()) {
            self.inner
                .instance_mut()
                .connect_port_ptr(index.0, data as *const f32 as *mut f32);
        }
        if ports.control_output.len() != self.control_outputs.len() {
            return Err(RunError::ControlOutputsSizeMismatch {
                expected: self.control_outputs.len(),
                actual: ports.control_output.len(),
            });
        }
        for (data, index) in ports.control_output.zip(self.control_outputs.iter()) {
            self.inner.instance_mut().connect_port_ptr(index.0, data);
        }
        if ports.audio_input.len() != self.audio_inputs.len() {
            return Err(RunError::AudioInputsSizeMismatch {
                expected: self.audio_inputs.len(),
                actual: ports.audio_input.len(),
            });
        }
        for (data, index) in ports.audio_input.zip(self.audio_inputs.iter()) {
            self.inner
                .instance_mut()
                .connect_port_ptr(index.0, data.as_ptr() as *mut f32);
        }
        if ports.audio_output.len() != self.audio_outputs.len() {
            return Err(RunError::AudioOutputsSizeMismatch {
                expected: self.audio_outputs.len(),
                actual: ports.audio_output.len(),
            });
        }
        for (data, index) in ports.audio_output.zip(self.audio_outputs.iter()) {
            self.inner
                .instance_mut()
                .connect_port_ptr(index.0, data.as_mut_ptr());
        }
        if ports.atom_sequence_input.len() != self.atom_sequence_inputs.len() {
            return Err(RunError::AtomSequenceInputsSizeMismatch {
                expected: self.atom_sequence_inputs.len(),
                actual: ports.atom_sequence_input.len(),
            });
        }
        for (data, index) in ports
            .atom_sequence_input
            .zip(self.atom_sequence_inputs.iter())
        {
            self.inner
                .instance_mut()
                .connect_port_ptr(index.0, data.as_ptr() as *mut lv2_raw::LV2AtomSequence);
        }
        if ports.atom_sequence_output.len() != self.atom_sequence_outputs.len() {
            return Err(RunError::AtomSequenceOutputsSizeMismatch {
                expected: self.atom_sequence_outputs.len(),
                actual: ports.atom_sequence_output.len(),
            });
        }
        for (data, index) in ports
            .atom_sequence_output
            .zip(self.atom_sequence_outputs.iter())
        {
            self.inner
                .instance_mut()
                .connect_port_ptr(index.0, data.as_mut_ptr());
        }
        self.inner.run(ports.sample_count);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_midi_urid_ok() {
        let world = World::new();
        assert!(world.midi_urid() > 0, "midi urid is not valid");
    }
}
