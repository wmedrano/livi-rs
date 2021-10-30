use crate::error;
use lv2_raw::LV2Atom;
use std::convert::TryFrom;
use std::fmt::Debug;
use std::marker::PhantomData;

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
    ///
    /// # Errors
    /// Returns an error if the size of the buffer is greater than `MAX_SIZE`.
    pub fn new(
        time_in_frames: i64,
        my_type: lv2_raw::LV2Urid,
        data: &[u8],
    ) -> Result<LV2AtomEventBuilder<MAX_SIZE>, error::Event> {
        let mut buffer = [0; MAX_SIZE];
        if data.len() > buffer.len() {
            return Err(error::Event::DataTooLarge {
                max_supported_size: MAX_SIZE,
                actual_size: data.len(),
            });
        }
        buffer[0..data.len()].copy_from_slice(data);
        Ok(LV2AtomEventBuilder {
            _event: lv2_raw::LV2AtomEvent {
                time_in_frames,
                body: LV2Atom {
                    size: u32::try_from(data.len()).expect("Size exceeds u32 capacity."),
                    mytype: my_type,
                },
            },
            _data: buffer,
        })
    }

    /// Create a new midi event.
    ///
    /// This is equivalent to `new` but exists to make it more obvious how to
    /// build midi events.
    ///
    /// # Errors
    /// Returns an error if data cannot fit within `MAX_SIZE`.
    pub fn new_midi(
        time_in_frames: i64,
        midi_uri: lv2_raw::LV2Urid,
        data: &[u8],
    ) -> Result<LV2AtomEventBuilder<MAX_SIZE>, error::Event> {
        LV2AtomEventBuilder::<MAX_SIZE>::new(time_in_frames, midi_uri, data)
    }

    /// Return a pointer to the `LV2AtomEvent` that is immediately followed by
    /// its data.
    #[must_use]
    pub fn as_ptr(&self) -> *const lv2_raw::LV2AtomEvent {
        let ptr = self as *const LV2AtomEventBuilder<MAX_SIZE>;
        ptr.cast()
    }
}

/// An atom sequence.
pub struct LV2AtomSequence {
    buffer: Vec<u8>,
}

impl LV2AtomSequence {
    /// Create a new sequence of `size` bytes.
    #[must_use]
    pub fn new(size: usize) -> LV2AtomSequence {
        let mut seq = LV2AtomSequence {
            buffer: vec![0; size],
        };
        seq.clear();
        seq
    }

    /// Clear all events in the sequence.
    pub fn clear(&mut self) {
        unsafe { lv2_raw::atomutils::lv2_atom_sequence_clear(self.as_mut_ptr()) }
    }

    /// Append an event to the sequence. If there is no capacity for it, then it
    /// will not be appended.
    ///
    /// # Errors
    /// Returns an error if the event could not be pushed to the sequence.
    pub fn push_event<const MAX_SIZE: usize>(
        &mut self,
        event: &LV2AtomEventBuilder<MAX_SIZE>,
    ) -> Result<(), error::Event> {
        let new_event_ptr = unsafe {
            lv2_raw::atomutils::lv2_atom_sequence_append_event(
                self.as_mut_ptr(),
                u32::try_from(self.capacity()).expect("Size exceeds capacity of u32."),
                event.as_ptr(),
            )
        };
        if new_event_ptr.is_null() {
            Err(error::Event::SequenceCapacityExceeded)
        } else {
            Ok(())
        }
    }

    /// Push a new midi event into the sequence. The `midi_data` must be of size
    /// `MAX_SIZE` or smaller. If this is not the case, an error is returned.
    ///
    /// # Errors
    /// Returns an error if the midi data is too large.
    pub fn push_midi_event<const MAX_SIZE: usize>(
        &mut self,
        time_in_frames: i64,
        midi_uri: lv2_raw::LV2Urid,
        data: &[u8],
    ) -> Result<(), error::Event> {
        let event = LV2AtomEventBuilder::<MAX_SIZE>::new_midi(time_in_frames, midi_uri, data)?;
        self.push_event(&event)
    }

    /// Return a pointer to the underlying data.
    #[must_use]
    pub fn as_ptr(&self) -> *const lv2_raw::LV2AtomSequence {
        self.buffer.as_ptr().cast()
    }

    /// Return a mutable pointer to the underlying data.
    #[must_use]
    pub fn as_mut_ptr(&mut self) -> *mut lv2_raw::LV2AtomSequence {
        self.buffer.as_mut_ptr().cast()
    }

    /// Get the capacity of the sequence.
    #[must_use]
    pub fn capacity(&self) -> usize {
        self.buffer.len()
    }

    /// Iterate over all events (and event data) in the sequence.
    pub fn iter(&self) -> LV2AtomSequenceIter<'_> {
        let body = unsafe { &self.as_ptr().as_ref().unwrap().body };
        let size = unsafe { self.as_ptr().as_ref().unwrap().atom.size };
        let begin = unsafe { lv2_raw::lv2_atom_sequence_begin(body) };
        LV2AtomSequenceIter {
            _sequence: PhantomData,
            body,
            size,
            next: begin,
        }
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

/// An iterator of an `LV2AtomSequence`.
#[derive(Clone)]
pub struct LV2AtomSequenceIter<'a> {
    _sequence: PhantomData<&'a LV2AtomSequence>,
    body: *const lv2_raw::LV2AtomSequenceBody,
    size: u32,
    next: *const lv2_raw::LV2AtomEvent,
}

impl<'a> Iterator for LV2AtomSequenceIter<'a> {
    type Item = LV2AtomEventWithData<'a>;

    fn next(&mut self) -> Option<LV2AtomEventWithData<'a>> {
        let is_end = unsafe { lv2_raw::lv2_atom_sequence_is_end(self.body, self.size, self.next) };
        if is_end {
            return None;
        }
        let next_ptr = self.next;
        let next = unsafe { next_ptr.as_ref() }?;
        let data_ptr: *const u8 = unsafe { next_ptr.offset(1) }.cast();
        let data_size = next.body.size as usize;
        self.next = unsafe { lv2_raw::lv2_atom_sequence_next(self.next) };
        Some(LV2AtomEventWithData {
            event: next,
            data: unsafe { std::slice::from_raw_parts(data_ptr, data_size) },
        })
    }
}

impl<'a> Debug for LV2AtomSequenceIter<'a> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_list().entries(self.clone()).finish()
    }
}

/// Contains an `LV2AtomEvent` and its data.
///
/// # Note
/// This type can not usually be used as a direct substitute for `LV2AtomEvent`
/// since it does not guarantee that `event` and `data` are linked together
/// properly in terms of pointers and data layout.
pub struct LV2AtomEventWithData<'a> {
    pub event: &'a lv2_raw::LV2AtomEvent,
    pub data: &'a [u8],
}

impl<'a> Debug for LV2AtomEventWithData<'a> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LV2AtomEventWithData")
            .field("time_in_frames", &self.event.time_in_frames)
            .field("my_type", &self.event.body.mytype)
            .field("size", &self.event.body.size)
            .field("data", &self.data)
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sequence() {
        let mut sequence = LV2AtomSequence::new(4096);
        let event = LV2AtomEventBuilder::<8>::new(0, 0, &[0, 1, 2, 3, 4, 5, 6, 7]).unwrap();
        for _ in 0..10 {
            sequence.push_event(&event).unwrap();
        }
        assert_eq!(10, sequence.iter().count());
        for event in sequence.iter() {
            assert_eq!(event.data, &[0, 1, 2, 3, 4, 5, 6, 7]);
        }
    }
}
