use std::fmt::Debug;
use std::sync::Arc;

use crate::{
    error::{InstantiateError, RunError},
    event::LV2AtomSequence,
    port::{DataType, IOType},
    Port, PortConnections, PortIndex, PortType, Resources,
};

/// A plugin that can be used to instantiate plugin instances.
#[derive(Clone)]
pub struct Plugin {
    pub(crate) inner: lilv::plugin::Plugin,
    pub(crate) resources: Arc<Resources>,
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
        let (min_block_size, max_block_size) = features
            .min_and_max_block_length
            .ok_or(InstantiateError::BlockLengthNotInitialized)?;
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
            min_block_size,
            max_block_size,
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

    pub fn build_port_data(&self, atom_sequence_size: usize) -> Option<crate::port::PortData> {
        let features = self.resources.features.lock().unwrap();
        let (_, max_block_size) = features.min_and_max_block_length?;
        Some(crate::port::PortData {
            control_inputs: self
                .ports_with_type(PortType::ControlInput)
                .map(|p| p.default_value)
                .collect(),
            control_outputs: self
                .ports_with_type(PortType::ControlOutput)
                .map(|p| p.default_value)
                .collect(),
            audio_inputs: crate::port::Channels::new(
                self.ports_with_type(PortType::AudioInput).count(),
                max_block_size,
            ),
            audio_outputs: crate::port::Channels::new(
                self.ports_with_type(PortType::AudioOutput).count(),
                max_block_size,
            ),
            atom_sequence_inputs: self
                .ports_with_type(PortType::AtomSequenceInput)
                .map(|_| crate::event::LV2AtomSequence::new(atom_sequence_size))
                .collect(),
            atom_sequence_outputs: self
                .ports_with_type(PortType::AtomSequenceOutput)
                .map(|_| crate::event::LV2AtomSequence::new(atom_sequence_size))
                .collect(),
            cv_inputs: crate::port::Channels::new(
                self.ports_with_type(PortType::CVInput).count(),
                max_block_size,
            ),
            cv_outputs: crate::port::Channels::new(
                self.ports_with_type(PortType::CVOutput).count(),
                max_block_size,
            ),
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
    min_block_size: usize,
    max_block_size: usize,
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
        let sample_count = ports.sample_count;
        if sample_count < self.min_block_size {
            return Err(RunError::SampleCountTooSmall {
                min_supported: self.min_block_size,
                actual: sample_count,
            });
        }
        if sample_count > self.max_block_size {
            return Err(RunError::SampleCountTooLarge {
                max_supported: self.max_block_size,
                actual: sample_count,
            });
        }
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
            if data.len() < sample_count {
                return Err(RunError::AudioInputSampleCountTooSmall {
                    expected: sample_count,
                    actual: data.len(),
                });
            }
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
            if data.len() < sample_count {
                return Err(RunError::AudioOutputSampleCountTooSmall {
                    expected: sample_count,
                    actual: data.len(),
                });
            }
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

    #[test]
    fn output_buffer_too_small_produces_error() {
        let block_size = 1024;
        let sample_rate = 44100.0;
        let mut world = crate::World::new();
        world
            .initialize_block_length(block_size, block_size)
            .unwrap();
        let plugin = world
            .plugin_by_uri("http://drobilla.net/plugins/mda/EPiano")
            .expect("Plugin not found.");
        let mut instance = unsafe {
            plugin
                .instantiate(sample_rate)
                .expect("Could not instantiate plugin.")
        };
        let input = crate::event::LV2AtomSequence::new(1024);
        let params: Vec<f32> = plugin
            .ports_with_type(crate::PortType::ControlInput)
            .map(|p| p.default_value)
            .collect();
        let mut outputs_that_are_too_small = [vec![0.0; 1], vec![0.0; 1]];
        let ports = crate::EmptyPortConnections::new(block_size)
            .with_atom_sequence_inputs(std::iter::once(&input))
            .with_audio_outputs(
                outputs_that_are_too_small
                    .iter_mut()
                    .map(|output| output.as_mut_slice()),
            )
            .with_control_inputs(params.iter());
        assert_eq!(
            unsafe { instance.run(ports) },
            Err(crate::error::RunError::AudioOutputSampleCountTooSmall {
                expected: block_size,
                actual: 1
            })
        );
    }

    #[test]
    fn sample_count_smaller_than_supported_block_size_produces_error() {
        let mut world = crate::World::new();
        let supported_block_size = (512, 1024);
        let lower_than_supported_block_size = 256;
        world
            .initialize_block_length(supported_block_size.0, supported_block_size.1)
            .unwrap();
        let plugin = world
            .plugin_by_uri("http://drobilla.net/plugins/mda/EPiano")
            .expect("Plugin not found.");
        let mut instance = unsafe {
            plugin
                .instantiate(44100.0)
                .expect("Could not instantiate plugin.")
        };
        let mut port_data = plugin.build_port_data(1024).unwrap();
        let ports = port_data.as_port_connections(lower_than_supported_block_size);
        assert_eq!(
            unsafe { instance.run(ports) },
            Err(crate::error::RunError::SampleCountTooSmall {
                min_supported: 512,
                actual: 256
            })
        );
    }

    #[test]
    fn sample_count_larger_than_supported_block_size_produces_error() {
        let mut world = crate::World::new();
        let supported_block_size = (512, 1024);
        let higher_than_supported_block_size = 2048;
        world
            .initialize_block_length(supported_block_size.0, supported_block_size.1)
            .unwrap();
        let plugin = world
            .plugin_by_uri("http://drobilla.net/plugins/mda/EPiano")
            .expect("Plugin not found.");
        let mut instance = unsafe {
            plugin
                .instantiate(44100.0)
                .expect("Could not instantiate plugin.")
        };
        let mut port_data = plugin.build_port_data(1024).unwrap();
        let ports = port_data.as_port_connections(higher_than_supported_block_size);
        assert_eq!(
            unsafe { instance.run(ports) },
            Err(crate::error::RunError::SampleCountTooLarge {
                max_supported: 1024,
                actual: 2048,
            })
        );
    }

    #[test]
    fn build_port_data_has_correct_number_of_ports() {
        let mut world = crate::World::new();
        let block_size = 512;
        world
            .initialize_block_length(block_size, block_size)
            .unwrap();
        let plugin = world
            .plugin_by_uri("http://drobilla.net/plugins/mda/EPiano")
            .expect("Plugin not found.");
        let port_data = plugin.build_port_data(1024).unwrap();
        // The results below can be verified with: `lv2info http://drobilla.net/plugins/mda/EPiano`
        assert_eq!(port_data.control_inputs.len(), 12);
        assert_eq!(port_data.control_outputs.len(), 0);
        assert_eq!(port_data.audio_inputs.channels(), 0);
        assert_eq!(port_data.audio_outputs.channels(), 2);
        assert_eq!(port_data.atom_sequence_inputs.len(), 1);
        assert_eq!(port_data.atom_sequence_outputs.len(), 0);
        assert_eq!(port_data.cv_inputs.channels(), 0);
        assert_eq!(port_data.cv_outputs.channels(), 0);
    }

    #[test]
    fn build_port_data_contains_proper_buffer_sizes() {
        let mut world = crate::World::new();
        let block_size = 512;
        world
            .initialize_block_length(block_size, block_size)
            .unwrap();
        let plugin = world
            .plugin_by_uri("http://drobilla.net/plugins/mda/EPiano")
            .expect("Plugin not found.");
        let port_data = plugin.build_port_data(1024).unwrap();
        for buffer in port_data.audio_outputs.iter() {
            assert_eq!(buffer.len(), 512);
        }
        for sequence in port_data.atom_sequence_outputs.iter() {
            assert_eq!(sequence.size(), 1024);
        }
    }
}
