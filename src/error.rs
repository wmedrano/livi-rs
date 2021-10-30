/// An error that occurs when initializing the block length LV2 feature.
#[derive(Debug)]
pub enum InitializeBlockLengthError {
    /// The minimum block length is too large.
    MinBlockLengthTooLarge,
    /// The maximum block length is too large.
    MaxBlockLengthTooLarge,
    /// The block length has already been initialized. It cannot be initialized
    /// again since existing plugins may have already been instantiated.
    BlockLengthAlreadyInitialized,
}

/// An error with plugin instantiation.
#[derive(Debug)]
pub enum InstantiateError {
    /// An error ocurred, but it is not known why.
    UnknownError,
    /// The plugin was found to have too many atom ports. Only up to 1 atom port
    /// is supported.
    TooManyEventsInputs,
    /// `World::initialize_block_length` has not yet been called.
    BlockLengthNotInitialized,
}
