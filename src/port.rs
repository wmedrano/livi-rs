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
#[derive(Copy, Clone, Debug)]
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

    /// The index of this port within the plugin.
    pub index: PortIndex,
}

/// A `PortConnections` object with no connections.
pub type EmptyPortConnections = PortConnections<
    'static,
    std::iter::Empty<&'static f32>,
    std::iter::Empty<&'static mut f32>,
    std::iter::Empty<&'static [f32]>,
    std::iter::Empty<&'static mut [f32]>,
    std::iter::Empty<&'static LV2AtomSequence>,
    std::iter::Empty<&'static mut LV2AtomSequence>,
    std::iter::Empty<&'static [f32]>,
    std::iter::Empty<&'static mut [f32]>,
>;

impl EmptyPortConnections {
    /// Create a new `PortConnections` object without any connections.
    pub fn new(sample_count: usize) -> EmptyPortConnections {
        EmptyPortConnections {
            sample_count,
            control_inputs: std::iter::empty(),
            control_outputs: std::iter::empty(),
            audio_inputs: std::iter::empty(),
            audio_outputs: std::iter::empty(),
            atom_sequence_inputs: std::iter::empty(),
            atom_sequence_outputs: std::iter::empty(),
            cv_inputs: std::iter::empty(),
            cv_outputs: std::iter::empty(),
        }
    }
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
    CVInputs,
    CVOutputs,
> where
    ControlInputs: ExactSizeIterator + Iterator<Item = &'a f32>,
    ControlOutputs: ExactSizeIterator + Iterator<Item = &'a mut f32>,
    AudioInputs: ExactSizeIterator + Iterator<Item = &'a [f32]>,
    AudioOutputs: ExactSizeIterator + Iterator<Item = &'a mut [f32]>,
    AtomSequenceInputs: ExactSizeIterator + Iterator<Item = &'a LV2AtomSequence>,
    AtomSequenceOutputs: ExactSizeIterator + Iterator<Item = &'a mut LV2AtomSequence>,
    CVInputs: ExactSizeIterator + Iterator<Item = &'a [f32]>,
    CVOutputs: ExactSizeIterator + Iterator<Item = &'a mut [f32]>,
{
    /// The number of audio samples that will be processed.
    pub sample_count: usize,

    /// The control inputs.
    pub control_inputs: ControlInputs,

    /// The control outputs.
    pub control_outputs: ControlOutputs,

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
        ControlInputs,
        ControlOutputs,
        AudioInputs,
        AudioOutputs,
        AtomSequenceInputs,
        AtomSequenceOutputs,
        CVInputs,
        CVOutputs,
    >
    PortConnections<
        'a,
        ControlInputs,
        ControlOutputs,
        AudioInputs,
        AudioOutputs,
        AtomSequenceInputs,
        AtomSequenceOutputs,
        CVInputs,
        CVOutputs,
    >
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
    /// Create an instance of `PortConnections` with the given control inputs.
    pub fn with_control_inputs<I>(
        self,
        control_inputs: I,
    ) -> PortConnections<
        'a,
        I,
        ControlOutputs,
        AudioInputs,
        AudioOutputs,
        AtomSequenceInputs,
        AtomSequenceOutputs,
        CVInputs,
        CVOutputs,
    >
    where
        I: ExactSizeIterator + Iterator<Item = &'a f32>,
    {
        PortConnections {
            sample_count: self.sample_count,
            control_inputs,
            control_outputs: self.control_outputs,
            audio_inputs: self.audio_inputs,
            audio_outputs: self.audio_outputs,
            atom_sequence_inputs: self.atom_sequence_inputs,
            atom_sequence_outputs: self.atom_sequence_outputs,
            cv_inputs: self.cv_inputs,
            cv_outputs: self.cv_outputs,
        }
    }

    /// Create an instance `PortConnections` with the given control outputs.
    pub fn with_control_outputs<I>(
        self,
        control_outputs: I,
    ) -> PortConnections<
        'a,
        ControlInputs,
        I,
        AudioInputs,
        AudioOutputs,
        AtomSequenceInputs,
        AtomSequenceOutputs,
        CVInputs,
        CVOutputs,
    >
    where
        I: ExactSizeIterator + Iterator<Item = &'a mut f32>,
    {
        PortConnections {
            sample_count: self.sample_count,
            control_inputs: self.control_inputs,
            control_outputs,
            audio_inputs: self.audio_inputs,
            audio_outputs: self.audio_outputs,
            atom_sequence_inputs: self.atom_sequence_inputs,
            atom_sequence_outputs: self.atom_sequence_outputs,
            cv_inputs: self.cv_inputs,
            cv_outputs: self.cv_outputs,
        }
    }

    /// Create an instance of `PortConnections` with the given audio inputs.
    pub fn with_audio_inputs<I>(
        self,
        audio_inputs: I,
    ) -> PortConnections<
        'a,
        ControlInputs,
        ControlOutputs,
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
            sample_count: self.sample_count,
            control_inputs: self.control_inputs,
            control_outputs: self.control_outputs,
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
        ControlInputs,
        ControlOutputs,
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
            sample_count: self.sample_count,
            control_inputs: self.control_inputs,
            control_outputs: self.control_outputs,
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
    ) -> PortConnections<
        'a,
        ControlInputs,
        ControlOutputs,
        AudioInputs,
        AudioOutputs,
        I,
        AtomSequenceOutputs,
        CVInputs,
        CVOutputs,
    >
    where
        I: ExactSizeIterator + Iterator<Item = &'a LV2AtomSequence>,
    {
        PortConnections {
            sample_count: self.sample_count,
            control_inputs: self.control_inputs,
            control_outputs: self.control_outputs,
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
    ) -> PortConnections<
        'a,
        ControlInputs,
        ControlOutputs,
        AudioInputs,
        AudioOutputs,
        AtomSequenceInputs,
        I,
        CVInputs,
        CVOutputs,
    >
    where
        I: ExactSizeIterator + Iterator<Item = &'a mut LV2AtomSequence>,
    {
        PortConnections {
            sample_count: self.sample_count,
            control_inputs: self.control_inputs,
            control_outputs: self.control_outputs,
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
        ControlInputs,
        ControlOutputs,
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
            sample_count: self.sample_count,
            control_inputs: self.control_inputs,
            control_outputs: self.control_outputs,
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
        ControlInputs,
        ControlOutputs,
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
            sample_count: self.sample_count,
            control_inputs: self.control_inputs,
            control_outputs: self.control_outputs,
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
