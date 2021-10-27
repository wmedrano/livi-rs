use std::{
    collections::HashSet,
    sync::{Arc, Mutex},
};

use log::{error, info, warn};
use lv2_raw::{LV2AtomSequence, LV2Feature};

use crate::midi::Lv2AtomSequence;

mod features;
pub mod midi;

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
}

impl Features {
    fn new() -> Features {
        Features {
            urid_map: features::urid_map::UridMap::new(),
        }
    }

    fn supported_features(&self) -> HashSet<String> {
        self.iter_features()
            .map(|f| {
                unsafe { std::ffi::CStr::from_ptr(f.uri) }
                    .to_string_lossy()
                    .into_owned()
            })
            .collect()
    }

    fn iter_features(&self) -> impl Iterator<Item = &'_ LV2Feature> {
        std::iter::once(self.urid_map.as_feature())
    }

    fn iter_features_mut(&mut self) -> impl Iterator<Item = &'_ mut LV2Feature> {
        std::iter::once(self.urid_map.as_feature_mut())
    }
}

pub struct World {
    plugins: Vec<lilv::plugin::Plugin>,
    resources: Arc<Resources>,
}

impl World {
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

    pub fn urid(&self, uri: &std::ffi::CStr) -> lv2_raw::LV2Urid {
        self.resources.features.lock().unwrap().urid_map.map(uri)
    }

    pub fn midi_urid(&self) -> lv2_raw::LV2Urid {
        self.urid(
            std::ffi::CStr::from_bytes_with_nul(b"http://lv2plug.in/ns/ext/midi#MidiEvent\0")
                .unwrap(),
        )
    }
    pub fn iter_plugins(&self) -> impl '_ + Iterator<Item = Plugin> {
        self.plugins.iter().map(move |p| Plugin {
            inner: p.clone(),
            resources: self.resources.clone(),
        })
    }
}

impl Default for World {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug)]
pub enum InstantiateError {
    UnknownError,
    TooManyEventsInputs,
}

#[derive(Copy, Clone, Debug)]
pub struct PluginOptions {
    event_buffer_size: Option<usize>,
}

#[derive(Clone)]
pub struct Plugin {
    inner: lilv::plugin::Plugin,
    resources: Arc<Resources>,
}

impl Plugin {
    /// A unique identifier for the plugin.
    pub fn uri(&self) -> String {
        self.inner.uri().as_str().unwrap_or("BAD_URI").to_string()
    }

    /// The name of the plugin.
    pub fn name(&self) -> String {
        self.inner.name().as_str().unwrap_or("BAD_NAME").to_string()
    }

    /// # Safety
    /// Running plugin code is unsafe.
    pub unsafe fn instantiate(&self, sample_rate: f64) -> Result<Instance, InstantiateError> {
        let instance = self
            .inner
            .instantiate(
                sample_rate,
                self.resources.features.lock().unwrap().iter_features_mut(),
            )
            .ok_or(InstantiateError::UnknownError)?;
        let mut control_inputs = Vec::new();
        let mut audio_inputs = Vec::new();
        let mut audio_outputs = Vec::new();
        let mut events_input = None;
        for port in self.ports() {
            match port.port_type {
                PortType::ControlInput => control_inputs.push(port.index),
                PortType::ControlOutput => (),
                PortType::AudioInput => audio_inputs.push(port.index),
                PortType::AudioOutput => audio_outputs.push(port.index),
                PortType::EventsInput => {
                    if events_input.is_some() {
                        return Err(InstantiateError::TooManyEventsInputs);
                    }
                    events_input = Some(port.index);
                }
            }
        }
        Ok(Instance {
            inner: instance.activate(),
            control_inputs,
            audio_inputs,
            audio_outputs,
            events_input,
        })
    }

    /// # Safety
    /// Running plugin code is unsafe.
    pub unsafe fn instantiate_with_ports(
        &self,
        sample_rate: f64,
        buffer_size: usize,
        events_size: usize,
    ) -> Result<InstanceWithPorts, InstantiateError> {
        let instance = self.instantiate(sample_rate)?;
        let control_inputs = vec![0.0; instance.control_inputs.len()];
        let audio_inputs = vec![vec![0.0; buffer_size]; instance.audio_inputs.len()];
        let audio_outputs = vec![vec![0.0; buffer_size]; instance.audio_inputs.len()];
        Ok(InstanceWithPorts {
            instance,
            buffer_size,
            control_inputs,
            audio_inputs,
            audio_outputs,
            atom_sequence: Lv2AtomSequence::new(events_size),
        })
    }

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
                DataType::Atom
            } else {
                unreachable!("Port is not an audio or control port.")
            };
            let port_type = match (io_type, data_type) {
                (IOType::Input, DataType::Control) => PortType::ControlInput,
                (IOType::Input, DataType::Audio) => PortType::AudioInput,
                (IOType::Output, DataType::Control) => PortType::ControlOutput,
                (IOType::Output, DataType::Audio) => PortType::AudioOutput,
                (IOType::Input, DataType::Atom) => PortType::EventsInput,
                (iotype, data_type) => panic!(
                    "Port {:?} has unsupported configuration. It is an {:?} {:?} port.",
                    p, iotype, data_type
                ),
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
}

#[derive(Copy, Clone, Debug)]
enum IOType {
    // The data is an input to the plugin. Usually this corresponds to an `&`
    // and not an `&mut`.
    Input,
    // The data is an input to the plugin. Usually this corresponds to an `&mut`
    // and not an `&`.
    Output,
}

#[derive(Copy, Clone, Debug)]
enum DataType {
    /// A single f32.
    Control,
    /// An `[f32]`.
    Audio,
    /// LV2 atom.
    Atom,
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum PortType {
    /// A single `&f32`.
    ControlInput,
    /// A single `&mut f32`. This is not supported.
    ControlOutput,
    /// A `&[f32]`.
    AudioInput,
    /// And `&mut [f32]`.
    AudioOutput,
    /// LV2 events input. This is used to handle midi, among other things.
    EventsInput,
}

pub struct Port {
    pub port_type: PortType,
    pub name: String,
    pub default_value: f32,
    index: PortIndex,
}

pub struct MidiEvent<'a> {
    pub frame: usize,
    pub data: &'a [u8],
}

pub struct Ports<'a, ControlInput, AudioInput, AudioOutput> {
    pub frames: usize,
    pub control_input: ControlInput,
    pub audio_input: AudioInput,
    pub audio_output: AudioOutput,
    pub atom_sequence: Option<&'a Lv2AtomSequence>,
}

pub struct PortIndex(usize);

pub struct Instance {
    inner: lilv::instance::ActiveInstance,
    control_inputs: Vec<PortIndex>,
    audio_inputs: Vec<PortIndex>,
    audio_outputs: Vec<PortIndex>,
    events_input: Option<PortIndex>,
}

impl Instance {
    /// # Safety
    /// Running plugin code is unsafe.
    pub unsafe fn run<'a, ControlInput, AudioInput, AudioOutput>(
        &mut self,
        ports: Ports<ControlInput, AudioInput, AudioOutput>,
    ) -> Result<(), RunError>
    where
        ControlInput: ExactSizeIterator + Iterator<Item = &'a f32>,
        AudioInput: ExactSizeIterator + Iterator<Item = &'a [f32]>,
        AudioOutput: ExactSizeIterator + Iterator<Item = &'a mut [f32]>,
    {
        if ports.control_input.len() != self.control_inputs.len() {
            return Err(RunError::ControlInputSizeMismatch);
        }
        for (data, index) in ports.control_input.zip(self.control_inputs.iter()) {
            self.inner
                .instance_mut()
                .connect_port_ptr(index.0, data as *const f32 as *mut f32);
        }
        if ports.audio_input.len() != self.audio_inputs.len() {
            return Err(RunError::AudioInputSizeMismatch);
        }
        for (data, index) in ports.audio_input.zip(self.audio_inputs.iter()) {
            self.inner
                .instance_mut()
                .connect_port_ptr(index.0, data.as_ptr() as *mut f32);
        }
        if ports.audio_output.len() != self.audio_outputs.len() {
            return Err(RunError::AudioOutputSizeMismatch);
        }
        for (data, index) in ports.audio_output.zip(self.audio_outputs.iter()) {
            self.inner
                .instance_mut()
                .connect_port_ptr(index.0, data.as_mut_ptr());
        }
        if ports.atom_sequence.iter().count() != self.events_input.iter().count() {
            return Err(RunError::EventsInputSizeMismatch);
        }
        if let (Some(index), Some(sequence)) = (self.events_input.as_ref(), ports.atom_sequence) {
            self.inner
                .instance_mut()
                .connect_port_ptr(index.0, sequence.as_ptr() as *mut LV2AtomSequence);
        }
        self.inner.run(ports.frames);
        Ok(())
    }
}

#[derive(Debug)]
pub enum RunError {
    ControlInputSizeMismatch,
    AudioInputSizeMismatch,
    AudioOutputSizeMismatch,
    EventsInputSizeMismatch,
}

pub struct InstanceWithPorts {
    instance: Instance,
    buffer_size: usize,
    pub control_inputs: Vec<f32>,
    pub audio_inputs: Vec<Vec<f32>>,
    pub audio_outputs: Vec<Vec<f32>>,
    pub atom_sequence: midi::Lv2AtomSequence,
}

impl InstanceWithPorts {
    /// # Safety
    /// Running plugin code is unsafe.
    pub unsafe fn run<AudioInput, AudioOutput>(&mut self) -> Result<&'_ Vec<Vec<f32>>, RunError> {
        let atom_sequence = if self.instance.events_input.is_some() {
            Some(&self.atom_sequence)
        } else {
            None
        };
        let ports = Ports {
            frames: self.buffer_size,
            control_input: self.control_inputs.iter(),
            audio_input: self.audio_inputs.iter().map(|v| v.as_slice()),
            audio_output: self.audio_outputs.iter_mut().map(|v| v.as_mut_slice()),
            atom_sequence,
        };
        self.instance.run(ports)?;
        Ok(&self.audio_outputs)
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
