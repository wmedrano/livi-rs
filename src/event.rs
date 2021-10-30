use std::fmt::Debug;

use crate::error::EventError;
use lv2_raw::LV2Atom;

/// A builder for a single atom event. The max size of the data contained in the
/// event is `MAX_SIZE`.
#[repr(packed)]
pub struct LV2AtomEventBuilder<const MAX_SIZE: usize> {
    /// The atom event.
    _event: lv2_raw::LV2AtomEvent,
    /// The data for the atom event. The data is a tuple of the atom type and
    /// the atom data but it size is restricted to 16 bytes for the moment.
    _data: [u8; MAX_SIZE],
}

impl<const MAX_SIZE: usize> LV2AtomEventBuilder<MAX_SIZE> {
    /// Create a new atom event with the given time and type. The event can be
    /// filled in by setting the bytes in buffer and calling `set_size`.
    pub fn new(
        time_in_frames: i64,
        my_type: u32,
        data: &[u8],
    ) -> Result<LV2AtomEventBuilder<MAX_SIZE>, EventError> {
        let mut buffer = [0; MAX_SIZE];
        if data.len() > buffer.len() {
            return Err(EventError::DataTooLarge {
                max_supported_size: MAX_SIZE,
                actual_size: data.len(),
            });
        }
        buffer[0..data.len()].copy_from_slice(data);
        Ok(LV2AtomEventBuilder {
            _event: lv2_raw::LV2AtomEvent {
                time_in_frames,
                body: LV2Atom {
                    size: data.len() as u32,
                    mytype: my_type,
                },
            },
            _data: buffer,
        })
    }

    /// Return a pointer to the LV2AtomEvent.
    pub fn as_ptr(&self) -> *const lv2_raw::LV2AtomEvent {
        let ptr = self as *const LV2AtomEventBuilder<MAX_SIZE>;
        ptr.cast()
    }
}

/// An atom sequence.
pub struct LV2AtomSequence {
    buffer: Vec<lv2_raw::LV2AtomSequence>,
}

impl LV2AtomSequence {
    /// Create a new sequence that can hold about `desired_capacity` bytes.
    pub fn new(desired_capacity: usize) -> LV2AtomSequence {
        let len = desired_capacity / std::mem::size_of::<lv2_raw::LV2AtomSequence>();
        let mut buffer = Vec::with_capacity(len);
        buffer.resize_with(len, || lv2_raw::LV2AtomSequence {
            atom: lv2_raw::LV2Atom { size: 0, mytype: 0 },
            body: lv2_raw::LV2AtomSequenceBody { unit: 0, pad: 0 },
        });
        let mut seq = LV2AtomSequence { buffer };
        seq.clear();
        seq
    }

    /// Clear all events in the sequence.
    pub fn clear(&mut self) {
        unsafe { lv2_raw::atomutils::lv2_atom_sequence_clear(self.as_mut_ptr()) }
    }

    /// Append an event to the sequence. If there is no capacity for it, then it will not be
    /// appended.
    pub fn push_event<const MAX_SIZE: usize>(&mut self, event: &LV2AtomEventBuilder<MAX_SIZE>) {
        unsafe {
            lv2_raw::atomutils::lv2_atom_sequence_append_event(
                self.as_mut_ptr(),
                self.capacity() as u32,
                event.as_ptr(),
            )
        };
    }

    /// Push a new midi event into the sequence. The `midi_data` must be of size
    /// `MAX_SIZE` or smaller. If this is not the case, an error is returned.
    pub fn push_midi_event<const MAX_SIZE: usize>(
        &mut self,
        time_in_frames: i64,
        midi_uri: lv2_raw::LV2Urid,
        data: &[u8],
    ) -> Result<(), EventError> {
        let event = LV2AtomEventBuilder::<MAX_SIZE>::new(time_in_frames, midi_uri, data)?;
        self.push_event(&event);
        Ok(())
    }

    /// Return a pointer to the underlying data.
    pub fn as_ptr(&self) -> *const lv2_raw::LV2AtomSequence {
        self.buffer.as_ptr()
    }

    /// Return a mutable pointer to the underlying data.
    pub fn as_mut_ptr(&mut self) -> *mut lv2_raw::LV2AtomSequence {
        self.buffer.as_mut_ptr()
    }

    /// Get the capacity of the sequence.
    pub fn capacity(&self) -> usize {
        let slice: &[lv2_raw::LV2AtomSequence] = &self.buffer;
        std::mem::size_of_val(slice)
    }
}

impl Debug for LV2AtomSequence {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let capacity = self.capacity();
        f.debug_struct("Lv2AtomSequence")
            .field("capacity", &capacity)
            .finish()
    }
}
