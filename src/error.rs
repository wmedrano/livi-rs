/// An error that occurs when initializing the block length LV2 feature.
#[derive(Copy, Clone, Debug)]
pub enum InitializeBlockLengthError {
    /// The minimum block length is too large.
    MinBlockLengthTooLarge { max_supported: usize, actual: usize },
    /// The maximum block length is too large.
    MaxBlockLengthTooLarge { max_supported: usize, actual: usize },
    /// The block length has already been initialized. It cannot be initialized
    /// again since existing plugins may have already been instantiated.
    BlockLengthAlreadyInitialized {
        min_block_length: usize,
        max_block_length: usize,
    },
}

/// An error with plugin instantiation.
#[derive(Copy, Clone, Debug)]
pub enum InstantiateError {
    /// An error ocurred, but it is not known why.
    UnknownError,
    /// `World::initialize_block_length` has not yet been called.
    BlockLengthNotInitialized,
}

/// An error that occurs when dealing with atom events.
#[derive(Copy, Clone, Debug)]
pub enum EventError {
    /// The size of the data is too large than what is supported for the event.
    DataTooLarge {
        max_supported_size: usize,
        actual_size: usize,
    },

    /// The size of the sequence has exceeded its capacity.
    SequenceCapacityExceeded,
}

/// An error associated with running a plugin.
#[derive(Debug)]
pub enum RunError {
    /// The number of control inputs was different than what the plugin
    /// required.
    ControlInputsSizeMismatch { expected: usize, actual: usize },

    /// The number of control outputs was different than what the plugin
    /// required.
    ControlOutputsSizeMismatch { expected: usize, actual: usize },

    /// The number of audio inputs was different than what the plugin required.
    AudioInputsSizeMismatch { expected: usize, actual: usize },

    /// The number of audio outputs was different than what the plugin required.
    AudioOutputsSizeMismatch { expected: usize, actual: usize },

    /// The number of atom sequence inputs was different than what the plugin
    /// required.
    AtomSequenceInputsSizeMismatch { expected: usize, actual: usize },

    /// The number of atom sequence outputs was different than what the plugin
    /// required.
    AtomSequenceOutputsSizeMismatch { expected: usize, actual: usize },
}
