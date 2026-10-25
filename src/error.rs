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

    /// A CV input buffer contains fewer samples than requested.
    CVInputSampleCountTooSmall { expected: usize, actual: usize },

    /// A CV output buffer contains fewer samples than requested.
    CVOutputSampleCountTooSmall { expected: usize, actual: usize },

    /// The number of cv inputs was different than what the plugin required.
    CVInputsSizeMismatch { expected: usize, actual: usize },

    /// The number of cv outputs was different than what the plugin required.
    CVOutputsSizeMismatch { expected: usize, actual: usize },
}

/// An error associated with plugin state.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StateError {
    /// The state could not be created from the plugin instance.
    CreateFailed,

    /// The state could not be loaded.
    LoadFailed,

    /// The state is for a different plugin than the instance.
    PluginMismatch { expected: String, actual: String },

    /// The state could not be serialized.
    SerializeFailed,

    /// The path is not valid UTF-8.
    InvalidPath,

    /// The state operation returned a non-success `LV2_State_Status`.
    Status(lv2_sys::LV2_State_Status),
}

impl std::error::Error for InstantiateError {}
impl std::error::Error for EventError {}
impl std::error::Error for RunError {}
impl std::error::Error for StateError {}

impl std::fmt::Display for InstantiateError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            InstantiateError::UnknownError => f.write_str("unknown error"),
        }
    }
}

impl std::fmt::Display for EventError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EventError::DataTooLarge {
                max_supported_size,
                actual_size,
            } => write!(
                f,
                "data of size {actual_size} is larger than maximum supported size of {max_supported_size}",
            ),
            EventError::SequenceFull {
                capacity,
                requested,
            } => write!(
                f,
                "sequence with capacity {capacity} is full but requested {requested}",
            ),
        }
    }
}

impl std::fmt::Display for RunError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RunError::SampleCountTooSmall {
                min_supported,
                actual,
            } => write!(
                f,
                "sample count of {actual} is less than minimum supported sample count of {min_supported}",
            ),
            RunError::SampleCountTooLarge {
                max_supported,
                actual,
            } => write!(
                f,
                "sample count of {actual} is more than maximum supported sample count of {max_supported}",
            ),
            RunError::AudioInputsSizeMismatch { expected, actual } => {
                write!(f, "expected {expected} audio inputs but found {actual}")
            }
            RunError::AudioInputSampleCountTooSmall { expected, actual } => write!(
                f,
                "audio input required at least {expected} samples but has {actual}",
            ),
            RunError::AudioOutputsSizeMismatch { expected, actual } => {
                write!(f, "expected {expected} audio outputs but found {actual}",)
            }
            RunError::AudioOutputSampleCountTooSmall { expected, actual } => write!(
                f,
                "audio output required at least {expected} samples but has {actual}",
            ),
            RunError::AtomSequenceInputsSizeMismatch { expected, actual } => write!(
                f,
                "expected {expected} atom sequence inputs but found {actual}",
            ),
            RunError::AtomSequenceOutputsSizeMismatch { expected, actual } => write!(
                f,
                "expected {expected} atom sequence outputs but found {actual}",
            ),
            RunError::CVInputSampleCountTooSmall { expected, actual } => write!(
                f,
                "cv input required at least {expected} samples but has {actual}",
            ),
            RunError::CVOutputSampleCountTooSmall { expected, actual } => write!(
                f,
                "cv output required at least {expected} samples but has {actual}",
            ),
            RunError::CVInputsSizeMismatch { expected, actual } => {
                write!(f, "expected {expected} cv inputs but found {actual}")
            }
            RunError::CVOutputsSizeMismatch { expected, actual } => write!(
                f,
                "cv output required at least {expected} samples but has {actual}",
            ),
        }
    }
}

impl std::fmt::Display for StateError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            StateError::CreateFailed => f.write_str("failed to create state from instance"),
            StateError::LoadFailed => f.write_str("failed to load state"),
            StateError::PluginMismatch { expected, actual } => write!(
                f,
                "state is for plugin {actual} but the instance is {expected}"
            ),
            StateError::SerializeFailed => f.write_str("failed to serialize state"),
            StateError::InvalidPath => f.write_str("path is not valid UTF-8"),
            StateError::Status(status) => write!(f, "state operation failed with status {status}"),
        }
    }
}
