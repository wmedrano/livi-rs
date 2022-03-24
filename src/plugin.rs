use std::boxed::Box;
use std::fmt::Debug;
use std::sync::{Arc, Mutex};

use crate::features::Features;
use crate::port::Controls;
use crate::{
    error::{InstantiateError, RunError},
    event::LV2AtomSequence,
    features::worker,
    port::{DataType, IOType},
    CommonUris, Port, PortConnections, PortCounts, PortIndex, PortType,
};
use lv2_raw::LV2Feature;
use lv2_sys::LV2_Worker_Schedule;
use ringbuf::Producer;

/// A plugin that can be used to instantiate plugin instances.
#[derive(Clone)]
pub struct Plugin {
    pub(crate) inner: lilv::plugin::Plugin,
    pub(crate) common_uris: Arc<CommonUris>,
    port_counts: PortCounts,
}

impl Plugin {
    pub(crate) fn from_raw(plugin: lilv::plugin::Plugin, common_uris: Arc<CommonUris>) -> Plugin {
        let mut port_counts = PortCounts::default();
        for port in iter_ports_impl(&plugin, &common_uris) {
            match port.port_type {
                PortType::ControlInput => port_counts.control_inputs += 1,
                PortType::ControlOutput => port_counts.control_outputs += 1,
                PortType::AudioInput => port_counts.audio_inputs += 1,
                PortType::AudioOutput => port_counts.audio_outputs += 1,
                PortType::AtomSequenceInput => port_counts.atom_sequence_inputs += 1,
                PortType::AtomSequenceOutput => port_counts.atom_sequence_outputs += 1,
                PortType::CVInput => port_counts.cv_inputs += 1,
                PortType::CVOutput => port_counts.cv_outputs += 1,
            }
        }
        Plugin {
            inner: plugin,
            common_uris,
            port_counts,
        }
    }

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
    pub unsafe fn instantiate(
        &self,
        features: Arc<Features>,
        sample_rate: f64,
    ) -> Result<Instance, InstantiateError> {
        let min_block_size = features.min_block_length();
        let max_block_size = features.max_block_length();

        let (instance_to_worker_sender, instance_to_worker_receiver) = worker::instantiate_queue();
        let (worker_to_instance_sender, worker_to_instance_receiver) = worker::instantiate_queue();
        let mut instance_to_worker_sender = Box::new(instance_to_worker_sender);
        let instance_to_worker_sender_ptr: *mut Producer<u8> = instance_to_worker_sender.as_mut();
        let mut worker_schedule = Box::new(lv2_sys::LV2_Worker_Schedule {
            handle: instance_to_worker_sender_ptr.cast(),
            schedule_work: Some(worker::schedule_work),
        });

        let worker_schedule_ptr: *mut LV2_Worker_Schedule = worker_schedule.as_mut();
        let worker_feature = LV2Feature {
            uri: lv2_sys::LV2_WORKER__schedule.as_ptr() as *mut i8,
            data: worker_schedule_ptr.cast(),
        };

        let iter_features = features.iter_features(&worker_feature);

        let mut instance = self
            .inner
            .instantiate(sample_rate, iter_features)
            .ok_or(InstantiateError::UnknownError)?;

        let control_inputs = Controls::new(self.ports_with_type(PortType::ControlInput));
        let control_outputs = Controls::new(self.ports_with_type(PortType::ControlOutput));
        let mut audio_inputs = Vec::new();
        let mut audio_outputs = Vec::new();
        let mut atom_sequence_inputs = Vec::new();
        let mut atom_sequence_outputs = Vec::new();
        let mut cv_inputs = Vec::new();
        let mut cv_outputs = Vec::new();
        for port in self.ports() {
            match port.port_type {
                PortType::ControlInput => instance
                    .connect_port(port.index.0, control_inputs.value_ptr(port.index).unwrap()),
                PortType::ControlOutput => instance
                    .connect_port(port.index.0, control_outputs.value_ptr(port.index).unwrap()),
                PortType::AudioInput => audio_inputs.push(port.index),
                PortType::AudioOutput => audio_outputs.push(port.index),
                PortType::AtomSequenceInput => atom_sequence_inputs.push(port.index),
                PortType::AtomSequenceOutput => atom_sequence_outputs.push(port.index),
                PortType::CVInput => cv_inputs.push(port.index),
                PortType::CVOutput => cv_outputs.push(port.index),
            }
        }

        let mut inner = instance.activate();
        #[allow(clippy::mutex_atomic)]
        let is_alive = Arc::new(Mutex::new(true));

        let worker_interface =
            worker::maybe_get_worker_interface(&self.inner, &self.common_uris, &mut inner);
        if let Some(worker_interface) = worker_interface.as_ref() {
            let worker = worker::Worker::new(
                is_alive.clone(),
                *worker_interface,
                inner.instance().handle(),
                instance_to_worker_receiver,
                worker_to_instance_sender,
            );
            features.worker_manager().add_worker(worker);
        }

        Ok(Instance {
            inner,
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
            worker_interface,
            worker_to_instance_receiver,
            _worker_schedule: worker_schedule,
            _instance_to_worker_sender: instance_to_worker_sender,
            is_alive,
            _features: features,
        })
    }

    /// Iterate over all ports for the plugin.
    pub fn ports(&self) -> impl '_ + Iterator<Item = Port> {
        iter_ports_impl(&self.inner, &self.common_uris)
    }

    /// Get the number of ports for each type of port.
    pub fn port_counts(&self) -> &PortCounts {
        &self.port_counts
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
            .field("port_counts", &self.port_counts)
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
    control_inputs: Controls,
    control_outputs: Controls,
    audio_inputs: Vec<PortIndex>,
    audio_outputs: Vec<PortIndex>,
    atom_sequence_inputs: Vec<PortIndex>,
    atom_sequence_outputs: Vec<PortIndex>,
    cv_inputs: Vec<PortIndex>,
    cv_outputs: Vec<PortIndex>,
    worker_interface: Option<lv2_sys::LV2_Worker_Interface>,
    worker_to_instance_receiver: worker::WorkerMessageReceiver,
    _worker_schedule: Box<lv2_sys::LV2_Worker_Schedule>,
    _instance_to_worker_sender: Box<worker::WorkerMessageSender>,
    is_alive: Arc<Mutex<bool>>,
    _features: Arc<Features>,
}

unsafe impl Sync for Instance {}
unsafe impl Send for Instance {}

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
        AudioInputs,
        AudioOutputs,
        AtomSequenceInputs,
        AtomSequenceOutputs,
        CVInputs,
        CVOutputs,
    >(
        &mut self,
        samples: usize,
        ports: PortConnections<
            'a,
            AudioInputs,
            AudioOutputs,
            AtomSequenceInputs,
            AtomSequenceOutputs,
            CVInputs,
            CVOutputs,
        >,
    ) -> Result<(), RunError>
    where
        AudioInputs: ExactSizeIterator + Iterator<Item = &'a [f32]>,
        AudioOutputs: ExactSizeIterator + Iterator<Item = &'a mut [f32]>,
        AtomSequenceInputs: ExactSizeIterator + Iterator<Item = &'a LV2AtomSequence>,
        AtomSequenceOutputs: ExactSizeIterator + Iterator<Item = &'a mut LV2AtomSequence>,
        CVInputs: ExactSizeIterator + Iterator<Item = &'a [f32]>,
        CVOutputs: ExactSizeIterator + Iterator<Item = &'a mut [f32]>,
    {
        if samples < self.min_block_size {
            return Err(RunError::SampleCountTooSmall {
                min_supported: self.min_block_size,
                actual: samples,
            });
        }
        if samples > self.max_block_size {
            return Err(RunError::SampleCountTooLarge {
                max_supported: self.max_block_size,
                actual: samples,
            });
        }
        if ports.audio_inputs.len() != self.audio_inputs.len() {
            return Err(RunError::AudioInputsSizeMismatch {
                expected: self.audio_inputs.len(),
                actual: ports.audio_inputs.len(),
            });
        }
        for (data, index) in ports.audio_inputs.zip(self.audio_inputs.iter()) {
            if data.len() < samples {
                return Err(RunError::AudioInputSampleCountTooSmall {
                    expected: samples,
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
            if data.len() < samples {
                return Err(RunError::AudioOutputSampleCountTooSmall {
                    expected: samples,
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
            data.clear_as_chunk();
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
        self.inner.run(samples);

        if let Some(interface) = self.worker_interface.as_mut() {
            worker::handle_work_responses(
                interface,
                &mut self.worker_to_instance_receiver,
                self.inner.instance().handle(),
            );
            worker::end_run(interface, self.inner.instance().handle());
        }

        Ok(())
    }

    /// Get the value of the control port at `index`. If `index` is not a valid
    /// control port index, then `None` is returned.
    pub fn control_output(&self, index: PortIndex) -> Option<f32> {
        self.control_outputs.get(index)
    }

    /// Get the value of the control port at `index`. If `index` is not a valid
    /// control port index, then `None` is returned.
    pub fn control_input(&self, index: PortIndex) -> Option<f32> {
        self.control_inputs.get(index)
    }

    /// Set the value of the control port at `index`. If `index` is not a valid
    /// control port index, then `None` is returned. If the index is valid, then
    /// the value is returned.
    ///
    /// Note: This may be different than the passed in value in cases the input
    /// `value` is out of bounds of allowed values.
    pub fn set_control_input(&mut self, index: PortIndex, value: f32) -> Option<f32> {
        self.control_inputs.set(index, value)?;
        let ptr = self.control_inputs.value_ptr(index)?;
        unsafe { self.inner.instance_mut().connect_port(index.0, ptr) };
        Some(unsafe { *ptr })
    }

    /// Get the number of ports for a specific type of port.
    pub fn port_counts_for_type(&self, t: PortType) -> usize {
        match t {
            PortType::ControlInput => self.control_inputs.len(),
            PortType::ControlOutput => self.control_outputs.len(),
            PortType::AudioInput => self.audio_inputs.len(),
            PortType::AudioOutput => self.audio_outputs.len(),
            PortType::AtomSequenceInput => self.atom_sequence_inputs.len(),
            PortType::AtomSequenceOutput => self.atom_sequence_outputs.len(),
            PortType::CVInput => self.cv_inputs.len(),
            PortType::CVOutput => self.cv_outputs.len(),
        }
    }

    /// Get the number of ports for each type of port.
    pub fn port_counts(&self) -> PortCounts {
        PortCounts {
            control_inputs: self.port_counts_for_type(PortType::ControlInput),
            control_outputs: self.port_counts_for_type(PortType::ControlOutput),
            audio_inputs: self.port_counts_for_type(PortType::AudioInput),
            audio_outputs: self.port_counts_for_type(PortType::AudioOutput),
            atom_sequence_inputs: self.port_counts_for_type(PortType::AtomSequenceInput),
            atom_sequence_outputs: self.port_counts_for_type(PortType::AtomSequenceOutput),
            cv_inputs: self.port_counts_for_type(PortType::CVInput),
            cv_outputs: self.port_counts_for_type(PortType::CVOutput),
        }
    }
}

impl Drop for Instance {
    fn drop(&mut self) {
        let mut is_alive = self.is_alive.lock().unwrap();
        *is_alive = false;
    }
}

fn iter_ports_impl<'a>(
    plugin: &'a lilv::plugin::Plugin,
    common_uris: &'a CommonUris,
) -> impl 'a + Iterator<Item = Port> {
    plugin.iter_ports().map(move |p| {
        let io_type = if p.is_a(&common_uris.input_port_uri) {
            IOType::Input
        } else if p.is_a(&common_uris.output_port_uri) {
            IOType::Output
        } else {
            unreachable!("Port is neither input or output.")
        };
        let data_type = if p.is_a(&common_uris.audio_port_uri) {
            DataType::Audio
        } else if p.is_a(&common_uris.control_port_uri) {
            DataType::Control
        } else if p.is_a(&common_uris.atom_port_uri) {
            DataType::AtomSequence
        } else if p.is_a(&common_uris.cv_port_uri) {
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
        let range = p.range();
        Port {
            port_type,
            name: p
                .name()
                .expect("port has no name")
                .as_str()
                .unwrap_or("BAD_NAME")
                .to_string(),
            default_value: node_to_value(&range.default),
            min_value: range.minimum.map(|n| node_to_value(&Some(n))),
            max_value: range.maximum.map(|n| node_to_value(&Some(n))),
            index: PortIndex(p.index()),
        }
    })
}

fn node_to_value(maybe_node: &Option<lilv::node::Node>) -> f32 {
    let n = match maybe_node {
        Some(n) => n,
        None => return 0.0,
    };
    if n.is_float() {
        n.as_float().map(|f| f as f32).unwrap_or(0.0)
    } else if n.is_int() {
        n.as_int().unwrap_or(0) as f32
    } else if n.as_bool().unwrap_or(false) {
        1f32
    } else {
        0f32
    }
}

#[cfg(test)]
mod tests {

    #[test]
    fn output_buffer_too_small_produces_error() {
        let block_size = 1024;
        let sample_rate = 44100.0;
        let world = crate::World::new();
        let plugin = world
            .plugin_by_uri("http://drobilla.net/plugins/mda/EPiano")
            .expect("Plugin not found.");
        let features = world.build_features(crate::features::FeaturesBuilder {
            min_block_length: block_size,
            max_block_length: block_size,
            worker_manager: Default::default(),
        });
        let mut instance = unsafe {
            plugin
                .instantiate(features.clone(), sample_rate)
                .expect("Could not instantiate plugin.")
        };
        let input = crate::event::LV2AtomSequence::new(&features, 1024);
        let mut outputs_that_are_too_small = [vec![0.0; 1], vec![0.0; 1]];
        let ports = crate::EmptyPortConnections::new()
            .with_atom_sequence_inputs(std::iter::once(&input))
            .with_audio_outputs(
                outputs_that_are_too_small
                    .iter_mut()
                    .map(|output| output.as_mut_slice()),
            );
        assert_eq!(
            unsafe { instance.run(block_size, ports) },
            Err(crate::error::RunError::AudioOutputSampleCountTooSmall {
                expected: block_size,
                actual: 1
            })
        );
    }

    #[test]
    fn sample_count_smaller_than_supported_block_size_produces_error() {
        let world = crate::World::new();
        let supported_block_size = (512, 1024);
        let lower_than_supported_block_size = 256;
        let plugin = world
            .plugin_by_uri("http://drobilla.net/plugins/mda/EPiano")
            .expect("Plugin not found.");
        let features = world.build_features(crate::features::FeaturesBuilder {
            min_block_length: supported_block_size.0,
            max_block_length: supported_block_size.1,
            worker_manager: Default::default(),
        });
        let mut instance = unsafe {
            plugin
                .instantiate(features, 44100.0)
                .expect("Could not instantiate plugin.")
        };
        let ports = crate::EmptyPortConnections::new();
        assert_eq!(
            unsafe { instance.run(lower_than_supported_block_size, ports) },
            Err(crate::error::RunError::SampleCountTooSmall {
                min_supported: 512,
                actual: 256
            })
        );
    }

    #[test]
    fn sample_count_larger_than_supported_block_size_produces_error() {
        let world = crate::World::new();
        let supported_block_size = (512, 1024);
        let higher_than_supported_block_size = 2048;
        let plugin = world
            .plugin_by_uri("http://drobilla.net/plugins/mda/EPiano")
            .expect("Plugin not found.");
        let features = world.build_features(crate::features::FeaturesBuilder {
            min_block_length: supported_block_size.0,
            max_block_length: supported_block_size.1,
            worker_manager: Default::default(),
        });
        let mut instance = unsafe {
            plugin
                .instantiate(features, 44100.0)
                .expect("Could not instantiate plugin.")
        };
        let ports = crate::EmptyPortConnections::new();
        assert_eq!(
            unsafe { instance.run(higher_than_supported_block_size, ports) },
            Err(crate::error::RunError::SampleCountTooLarge {
                max_supported: 1024,
                actual: 2048,
            })
        );
    }
}
