use core::ffi::c_void;
use std::boxed::Box;
use std::fmt::Debug;
use std::sync::{Arc, Mutex};

use crate::{
    error::{InstantiateError, RunError},
    event::LV2AtomSequence,
    features::worker,
    port::{DataType, IOType},
    Port, PortConnections, PortCounts, PortIndex, PortType, Resources,
};
use lv2_raw::LV2Feature;

/// A plugin that can be used to instantiate plugin instances.
#[derive(Clone)]
pub struct Plugin {
    pub(crate) inner: lilv::plugin::Plugin,
    pub(crate) resources: Arc<Resources>,
    port_counts: PortCounts,
}

impl Plugin {
    pub(crate) fn from_raw(plugin: lilv::plugin::Plugin, resources: Arc<Resources>) -> Plugin {
        let mut port_counts = PortCounts::default();
        for port in iter_ports_impl(&plugin, &resources) {
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
            resources,
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
    ///
    /// # Panics
    /// Panics if the world resource mutex could not be locked.
    pub unsafe fn instantiate(&self, sample_rate: f64) -> Result<Instance, InstantiateError> {
        let features = self.resources.features.lock().unwrap();
        let (min_block_size, max_block_size) = features
            .min_and_max_block_length
            .ok_or(InstantiateError::BlockLengthNotInitialized)?;

        let (instance_to_worker_sender, instance_to_worker_receiver) = worker::instantiate_queue();
        let (worker_to_instance_sender, worker_to_instance_receiver) = worker::instantiate_queue();
        let instance_to_worker_sender = Box::new(instance_to_worker_sender);
        let instance_to_worker_sender = Box::into_raw(instance_to_worker_sender);
        let worker_schedule = Box::new(lv2_sys::LV2_Worker_Schedule {
            handle: instance_to_worker_sender as *mut c_void,
            schedule_work: Some(worker::schedule_work),
        });
        let instance_to_worker_sender = Box::from_raw(instance_to_worker_sender);

        let worker_schedule = Box::into_raw(worker_schedule);
        let worker_feature = LV2Feature {
            uri: lv2_sys::LV2_WORKER__schedule.as_ptr() as *mut i8,
            data: worker_schedule as *mut c_void,
        };
        let worker_schedule = Box::from_raw(worker_schedule);

        let features = features
            .iter_features()
            .chain(std::iter::once(&worker_feature));

        let instance = self
            .inner
            .instantiate(sample_rate, features)
            .ok_or(InstantiateError::UnknownError)?;

        let mut inner = instance.activate();

        let worker_interface = worker::maybe_get_worker_interface(&mut inner);

        #[allow(clippy::mutex_atomic)]
        let is_alive = Arc::new(Mutex::new(true));
        let worker = worker_interface.map(|interface| {
            worker::Worker::new(
                is_alive.clone(),
                interface,
                inner.instance().handle(),
                instance_to_worker_receiver,
                worker_to_instance_sender,
            )
        });

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
            worker,
            worker_to_instance_receiver,
            _worker_schedule: worker_schedule,
            _instance_to_worker_sender: instance_to_worker_sender,
            is_alive,
        })
    }

    /// Iterate over all ports for the plugin.
    pub fn ports(&self) -> impl '_ + Iterator<Item = Port> {
        iter_ports_impl(&self.inner, &self.resources)
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
    control_inputs: Vec<PortIndex>,
    control_outputs: Vec<PortIndex>,
    audio_inputs: Vec<PortIndex>,
    audio_outputs: Vec<PortIndex>,
    atom_sequence_inputs: Vec<PortIndex>,
    atom_sequence_outputs: Vec<PortIndex>,
    cv_inputs: Vec<PortIndex>,
    cv_outputs: Vec<PortIndex>,
    worker: Option<worker::Worker>,
    worker_to_instance_receiver: worker::WorkerMessageReceiver,
    _worker_schedule: Box<lv2_sys::LV2_Worker_Schedule>,
    _instance_to_worker_sender: Box<worker::WorkerMessageSender>,
    is_alive: Arc<Mutex<bool>>,
}

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
        self.inner.run(ports.sample_count);

        let worker_interface = worker::maybe_get_worker_interface(&mut self.inner);
        if let Some(mut interface) = worker_interface {
            worker::handle_work_responses(
                &mut interface,
                &mut self.worker_to_instance_receiver,
                self.inner.instance().handle(),
            );
            worker::end_run(&mut interface, self.inner.instance().handle());
        }

        Ok(())
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

    /// Returns (transferring ownership of) the worker
    /// for this plugin instance. There is only one worker
    /// and you can only retrieve it once. Grab the worker
    /// before you send the plugin instance to the realtime
    /// thread and hold onto it to perform work asynchronously.
    pub fn get_worker(&mut self) -> Option<worker::Worker> {
        self.worker.take()
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
    resources: &'a Resources,
) -> impl 'a + Iterator<Item = Port> {
    plugin.iter_ports().map(move |p| {
        let io_type = if p.is_a(&resources.input_port_uri) {
            IOType::Input
        } else if p.is_a(&resources.output_port_uri) {
            IOType::Output
        } else {
            unreachable!("Port is neither input or output.")
        };
        let data_type = if p.is_a(&resources.audio_port_uri) {
            DataType::Audio
        } else if p.is_a(&resources.control_port_uri) {
            DataType::Control
        } else if p.is_a(&resources.atom_port_uri) {
            DataType::AtomSequence
        } else if p.is_a(&resources.cv_port_uri) {
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
        let input = crate::event::LV2AtomSequence::new(&world, 1024);
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
        let ports = crate::EmptyPortConnections::new(lower_than_supported_block_size);
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
        let ports = crate::EmptyPortConnections::new(higher_than_supported_block_size);
        assert_eq!(
            unsafe { instance.run(ports) },
            Err(crate::error::RunError::SampleCountTooLarge {
                max_supported: 1024,
                actual: 2048,
            })
        );
    }
}
