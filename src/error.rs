/// An error with plugin instantiation.
#[derive(Copy, Clone, Debug)]
pub enum InstantiateError {
    /// An error ocurred, but it is not known why.
    UnknownError,
}

/// An error that occurs when dealing with atom events.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum EventError {
    /// The size of the data is too large than what is supported for the event.
    DataTooLarge {
        max_supported_size: usize,
        actual_size: usize,
    },

    /// The size of the sequence has exceeded its capacity.
    SequenceFull { capacity: usize, requested: usize },
}

/// An error associated with running a plugin.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum RunError {
    /// The sample count is smaller than the minimum that is allowed. The
    /// supported size is set when initializing the `livi::World` object.
    SampleCountTooSmall { min_supported: usize, actual: usize },

    /// The sample count is larger than the maximum that is allowed. The
    /// supported size is set when initializing the `livi::World` object.
    SampleCountTooLarge { max_supported: usize, actual: usize },

    /// The number of audio inputs was different than what the plugin required.
    AudioInputsSizeMismatch { expected: usize, actual: usize },

    /// The number of samples in the audio inputs was too small to contain the
    /// number of specified samples.
    AudioInputSampleCountTooSmall { expected: usize, actual: usize },

    /// The number of audio outputs was different than what the plugin required.
    AudioOutputsSizeMismatch { expected: usize, actual: usize },

    /// The number of samples in the audio inputs was too small to contain the
    /// number of specified samples.
    AudioOutputSampleCountTooSmall { expected: usize, actual: usize },

    /// The number of atom sequence inputs was different than what the plugin
    /// required.
    AtomSequenceInputsSizeMismatch { expected: usize, actual: usize },

    /// The number of atom sequence outputs was different than what the plugin
    /// required.
    AtomSequenceOutputsSizeMismatch { expected: usize, actual: usize },

    /// The number of cv inputs was different than what the plugin required.
    CVInputsSizeMismatch { expected: usize, actual: usize },

    /// The number of cv outputs was different than what the plugin required.
    CVOutputsSizeMismatch { expected: usize, actual: usize },
}
