use crate::PortCounts;

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

    /// The number of ports was different than what the plugin required.
    PortMismatch {
        expected: PortCounts,
        actual: PortCounts,
    },

    /// The number of samples in the audio inputs was too small to contain the
    /// number of specified samples.
    AudioInputSampleCountTooSmall { expected: usize, actual: usize },

    /// The number of samples in the audio inputs was too small to contain the
    /// number of specified samples.
    AudioOutputSampleCountTooSmall { expected: usize, actual: usize },
}

impl std::error::Error for InstantiateError {}
impl std::error::Error for EventError {}
impl std::error::Error for RunError {}

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
                "data of size {} is larger than maximum supported size of {}",
                actual_size, max_supported_size
            ),
            EventError::SequenceFull {
                capacity,
                requested,
            } => write!(
                f,
                "sequence with capacity {} is full but requested {}",
                capacity, requested
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
                "sample count of {} is less than minimum supported sample count of {}",
                actual, min_supported
            ),
            RunError::SampleCountTooLarge {
                max_supported,
                actual,
            } => write!(
                f,
                "sample count of {} is more than maximum supported sample count of {}",
                actual, max_supported
            ),
            RunError::AudioInputSampleCountTooSmall { expected, actual } => write!(
                f,
                "audio input required at least {} samples but has {}",
                expected, actual
            ),
            RunError::AudioOutputSampleCountTooSmall { expected, actual } => write!(
                f,
                "audio output required at least {} samples but has {}",
                expected, actual
            ),
            RunError::PortMismatch { expected, actual } => {
                write!(f, "expected ports {:?} but got {:?}", expected, actual)
            }
        }
    }
}
