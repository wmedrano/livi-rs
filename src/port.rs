use crate::event::LV2AtomSequence;

/// The type of IO for the port. Either input or output.
#[derive(Copy, Clone, Debug)]
pub enum IOType {
    // The data is an input to the plugin. Usually this corresponds to an `&`
    // and not an `&mut`.
    Input,
    // The data is an input to the plugin. Usually this corresponds to an `&mut`
    // and not an `&`.
    Output,
}

/// The data type pointed to by the port.
#[derive(Copy, Clone, Debug)]
pub enum DataType {
    /// A single f32.
    Control,

    /// An `[f32]` representing an audio signal.
    Audio,

    /// An LV2 atom sequence.
    AtomSequence,

    /// An `[f32]`..
    /// See http://lv2plug.in/ns/lv2core#CVPort.
    CV,
}

/// The type of port.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum PortType {
    /// A single `&f32`.
    ControlInput,

    /// A single `&mut f32`. This is not yet supported.
    ///
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

    /// Similar to `ControlInput`, but the type is `&[f32]`.
    CVInput,

    /// Similar to `ControlOutput`, but the type is `&mut [f32]`.
    CVOutput,
}

/// The index of the port within a plugin.
#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct PortIndex(pub usize);

/// A port represents a connection (either input or output) to a plugin.
#[derive(Clone, Debug)]
pub struct Port {
    /// The type of port.
    pub port_type: PortType,

    /// The name of the port.
    pub name: String,

    /// The default value for the port if it is a `ControlInputs`.
    pub default_value: f32,

    /// The minimum value allowed for the port.
    pub min_value: Option<f32>,

    /// The maximum value allowed for the port.
    pub max_value: Option<f32>,

    /// The index of this port within the plugin.
    pub index: PortIndex,
}

/// A `PortConnections` object with no connections.
pub type EmptyPortConnections = PortConnections<
    'static,
    std::iter::Empty<&'static [f32]>,
    std::iter::Empty<&'static mut [f32]>,
    std::iter::Empty<&'static LV2AtomSequence>,
    std::iter::Empty<&'static mut LV2AtomSequence>,
    std::iter::Empty<&'static [f32]>,
    std::iter::Empty<&'static mut [f32]>,
>;

impl EmptyPortConnections {
    /// Create a new `PortConnections` object without any connections.
    pub fn new() -> EmptyPortConnections {
        EmptyPortConnections {
            audio_inputs: std::iter::empty(),
            audio_outputs: std::iter::empty(),
            atom_sequence_inputs: std::iter::empty(),
            atom_sequence_outputs: std::iter::empty(),
            cv_inputs: std::iter::empty(),
            cv_outputs: std::iter::empty(),
        }
    }
}

impl Default for EmptyPortConnections {
    fn default() -> Self {
        Self::new()
    }
}

/// All the inputs and outputs for an instance.
pub struct PortConnections<
    'a,
    AudioInputs,
    AudioOutputs,
    AtomSequenceInputs,
    AtomSequenceOutputs,
    CVInputs,
    CVOutputs,
> where
    AudioInputs: ExactSizeIterator + Iterator<Item = &'a [f32]>,
    AudioOutputs: ExactSizeIterator + Iterator<Item = &'a mut [f32]>,
    AtomSequenceInputs: ExactSizeIterator + Iterator<Item = &'a LV2AtomSequence>,
    AtomSequenceOutputs: ExactSizeIterator + Iterator<Item = &'a mut LV2AtomSequence>,
    CVInputs: ExactSizeIterator + Iterator<Item = &'a [f32]>,
    CVOutputs: ExactSizeIterator + Iterator<Item = &'a mut [f32]>,
{
    /// The audio inputs.
    pub audio_inputs: AudioInputs,

    /// The audio outputs.
    pub audio_outputs: AudioOutputs,

    /// The events input.
    pub atom_sequence_inputs: AtomSequenceInputs,

    /// The events output.
    pub atom_sequence_outputs: AtomSequenceOutputs,

    /// The CV inputs.
    pub cv_inputs: CVInputs,

    /// The CV outputs.
    pub cv_outputs: CVOutputs,
}

impl<
        'a,
        AudioInputs,
        AudioOutputs,
        AtomSequenceInputs,
        AtomSequenceOutputs,
        CVInputs,
        CVOutputs,
    >
    PortConnections<
        'a,
        AudioInputs,
        AudioOutputs,
        AtomSequenceInputs,
        AtomSequenceOutputs,
        CVInputs,
        CVOutputs,
    >
where
    AudioInputs: ExactSizeIterator + Iterator<Item = &'a [f32]>,
    AudioOutputs: ExactSizeIterator + Iterator<Item = &'a mut [f32]>,
    AtomSequenceInputs: ExactSizeIterator + Iterator<Item = &'a LV2AtomSequence>,
    AtomSequenceOutputs: ExactSizeIterator + Iterator<Item = &'a mut LV2AtomSequence>,
    CVInputs: ExactSizeIterator + Iterator<Item = &'a [f32]>,
    CVOutputs: ExactSizeIterator + Iterator<Item = &'a mut [f32]>,
{
    /// Returns the number of ports supported by type.
    pub fn port_counts(&self) -> PortCounts {
        PortCounts {
            control_inputs: 0,
            control_outputs: 0,
            audio_inputs: self.audio_inputs.len(),
            audio_outputs: self.audio_outputs.len(),
            atom_sequence_inputs: self.atom_sequence_inputs.len(),
            atom_sequence_outputs: self.atom_sequence_outputs.len(),
            cv_inputs: self.cv_inputs.len(),
            cv_outputs: self.cv_outputs.len(),
        }
    }

    /// Create an instance of `PortConnections` with the given audio inputs.
    pub fn with_audio_inputs<I>(
        self,
        audio_inputs: I,
    ) -> PortConnections<
        'a,
        I,
        AudioOutputs,
        AtomSequenceInputs,
        AtomSequenceOutputs,
        CVInputs,
        CVOutputs,
    >
    where
        I: ExactSizeIterator + Iterator<Item = &'a [f32]>,
    {
        PortConnections {
            audio_inputs,
            audio_outputs: self.audio_outputs,
            atom_sequence_inputs: self.atom_sequence_inputs,
            atom_sequence_outputs: self.atom_sequence_outputs,
            cv_inputs: self.cv_inputs,
            cv_outputs: self.cv_outputs,
        }
    }

    /// Create an instance of `PortConnections` with the given audio outputs.
    pub fn with_audio_outputs<I>(
        self,
        audio_outputs: I,
    ) -> PortConnections<
        'a,
        AudioInputs,
        I,
        AtomSequenceInputs,
        AtomSequenceOutputs,
        CVInputs,
        CVOutputs,
    >
    where
        I: ExactSizeIterator + Iterator<Item = &'a mut [f32]>,
    {
        PortConnections {
            audio_inputs: self.audio_inputs,
            audio_outputs,
            atom_sequence_inputs: self.atom_sequence_inputs,
            atom_sequence_outputs: self.atom_sequence_outputs,
            cv_inputs: self.cv_inputs,
            cv_outputs: self.cv_outputs,
        }
    }

    /// Create an instance of `PortConnections` with the given sequence inputs.
    pub fn with_atom_sequence_inputs<I>(
        self,
        atom_sequence_inputs: I,
    ) -> PortConnections<'a, AudioInputs, AudioOutputs, I, AtomSequenceOutputs, CVInputs, CVOutputs>
    where
        I: ExactSizeIterator + Iterator<Item = &'a LV2AtomSequence>,
    {
        PortConnections {
            audio_inputs: self.audio_inputs,
            audio_outputs: self.audio_outputs,
            atom_sequence_inputs,
            atom_sequence_outputs: self.atom_sequence_outputs,
            cv_inputs: self.cv_inputs,
            cv_outputs: self.cv_outputs,
        }
    }

    /// Create an instance of `PortConnections` with the given sequence outputs.
    pub fn with_atom_sequence_outputs<I>(
        self,
        atom_sequence_outputs: I,
    ) -> PortConnections<'a, AudioInputs, AudioOutputs, AtomSequenceInputs, I, CVInputs, CVOutputs>
    where
        I: ExactSizeIterator + Iterator<Item = &'a mut LV2AtomSequence>,
    {
        PortConnections {
            audio_inputs: self.audio_inputs,
            audio_outputs: self.audio_outputs,
            atom_sequence_inputs: self.atom_sequence_inputs,
            atom_sequence_outputs,
            cv_inputs: self.cv_inputs,
            cv_outputs: self.cv_outputs,
        }
    }

    /// Create an instance of `PortConnections` with the given CV inputs.
    pub fn with_cv_inputs<I>(
        self,
        cv_inputs: I,
    ) -> PortConnections<
        'a,
        AudioInputs,
        AudioOutputs,
        AtomSequenceInputs,
        AtomSequenceOutputs,
        I,
        CVOutputs,
    >
    where
        I: ExactSizeIterator + Iterator<Item = &'a [f32]>,
    {
        PortConnections {
            audio_inputs: self.audio_inputs,
            audio_outputs: self.audio_outputs,
            atom_sequence_inputs: self.atom_sequence_inputs,
            atom_sequence_outputs: self.atom_sequence_outputs,
            cv_inputs,
            cv_outputs: self.cv_outputs,
        }
    }

    /// Create an instance of `PortConnections` with the given CV outputs.
    pub fn with_cv_outputs<I>(
        self,
        cv_outputs: I,
    ) -> PortConnections<
        'a,
        AudioInputs,
        AudioOutputs,
        AtomSequenceInputs,
        AtomSequenceOutputs,
        CVInputs,
        I,
    >
    where
        I: ExactSizeIterator + Iterator<Item = &'a mut [f32]>,
    {
        PortConnections {
            audio_inputs: self.audio_inputs,
            audio_outputs: self.audio_outputs,
            atom_sequence_inputs: self.atom_sequence_inputs,
            atom_sequence_outputs: self.atom_sequence_outputs,
            cv_inputs: self.cv_inputs,
            cv_outputs,
        }
    }
}

/// The number of ports by type.
#[derive(Copy, Clone, Debug, Default, Eq, PartialEq)]
pub struct PortCounts {
    pub control_inputs: usize,
    pub control_outputs: usize,
    pub audio_inputs: usize,
    pub audio_outputs: usize,
    pub atom_sequence_inputs: usize,
    pub atom_sequence_outputs: usize,
    pub cv_inputs: usize,
    pub cv_outputs: usize,
}

#[derive(Debug)]
struct DetailedPortValues {
    port_index: PortIndex,
    value: f32,
    minimum: f32,
    maximum: f32,
}

/// Controls holds the values of control ports. These are also known as
/// parameters.
#[derive(Debug)]
pub(crate) struct Controls {
    controls: Vec<DetailedPortValues>,
}

impl Controls {
    /// Construct a new `Controls` instance from the given ports.
    pub(crate) fn new<I>(ports: I) -> Controls
    where
        I: Iterator<Item = Port>,
    {
        let mut controls: Vec<DetailedPortValues> = ports
            .map(|p| DetailedPortValues {
                port_index: p.index,
                value: p.default_value,
                minimum: p.min_value.unwrap_or(std::f32::NEG_INFINITY),
                maximum: p.max_value.unwrap_or(std::f32::INFINITY),
            })
            .collect();
        controls.sort_by(|a, b| a.port_index.cmp(&b.port_index));
        controls.dedup_by_key(|p| p.port_index);
        Controls { controls }
    }

    /// Get the value of the control at the given index or `None` if it does not
    /// exist.
    pub fn get(&self, port: PortIndex) -> Option<f32> {
        let idx = self.port_index_to_index_in_controls(port)?;
        self.controls.get(idx).map(|p| p.value)
    }

    /// Return the number of controls.
    pub fn len(&self) -> usize {
        self.controls.len()
    }

    /// Set the value of the control at the given index. The value will be
    /// clamped to the minimum and maximum bounds and returned.
    pub fn set(&mut self, port: PortIndex, value: f32) -> Option<f32> {
        let idx = self.port_index_to_index_in_controls(port)?;
        let mut p = self.controls.get_mut(idx)?;
        let normalized_value = value.clamp(p.minimum, p.maximum);
        p.value = normalized_value;
        Some(normalized_value)
    }

    /// Get a pointer to the value of the control at the given index.
    pub fn value_ptr(&self, port: PortIndex) -> Option<*const f32> {
        let idx = self.port_index_to_index_in_controls(port)?;
        let p = self.controls.get(idx)?;
        Some(&p.value)
    }

    /// Get the index within the controls vector of the given port index.
    fn port_index_to_index_in_controls(&self, port: PortIndex) -> Option<usize> {
        self.controls
            .binary_search_by_key(&port, |p| p.port_index)
            .ok()
    }
}
