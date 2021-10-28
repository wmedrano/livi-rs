use lv2_raw::LV2Atom;

/// The underlying buffer backing the data for an atom event.
type Lv2AtomEventBuffer = [u8; 16];

/// An single atom event.
#[repr(packed)]
struct Lv2AtomEvent {
    header: lv2_raw::LV2AtomEvent,
    pub buffer: Lv2AtomEventBuffer,
}

impl Lv2AtomEvent {
    /// Create a new atom event with the given time and type. The event can be filled in by setting
    /// the bytes in buffer and calling `set_size`.
    fn new(time_in_frames: i64, my_type: u32) -> Lv2AtomEvent {
        Lv2AtomEvent {
            header: lv2_raw::LV2AtomEvent {
                time_in_frames,
                body: LV2Atom {
                    size: 0,
                    mytype: my_type,
                },
            },
            buffer: Lv2AtomEventBuffer::default(),
        }
    }

    /// Set the size of the atom. Must be less than or equal to the size of the buffer.
    fn set_size(&mut self, size: usize) {
        debug_assert!(size < self.buffer.len(), "{} < {}", size, self.buffer.len());
        self.header.body.size = size as u32;
    }

    /// Return a pointer to the header of the atom.
    #[allow(unaligned_references)]
    fn as_ptr(&self) -> *const lv2_raw::LV2AtomEvent {
        &self.header
    }
}

/// An atom sequence.
pub struct LV2AtomSequence {
    buffer: Vec<lv2_raw::LV2AtomSequence>,
}

impl LV2AtomSequence {
    /// Create a new sequence that can hold about desired_capacity bytes.
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
    fn append_event(&mut self, event: &Lv2AtomEvent) {
        unsafe {
            lv2_raw::atomutils::lv2_atom_sequence_append_event(
                self.as_mut_ptr(),
                self.capacity() as u32,
                event.as_ptr(),
            )
        };
    }

    pub fn append_midi_event(
        &mut self,
        time_in_frames: i64,
        midi_uri: lv2_raw::LV2Urid,
        data: &[u8],
    ) {
        let mut event = Lv2AtomEvent::new(time_in_frames, midi_uri);
        // TODO(wmedrano): Make this an error.
        debug_assert!(data.len() <= event.buffer.len());
        event.set_size(data.len());
        event.buffer[..data.len()].copy_from_slice(data);
        self.append_event(&event);
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

impl std::fmt::Debug for LV2AtomSequence {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let capacity = self.capacity();
        f.debug_struct("Lv2AtomSequence")
            .field("capacity", &capacity)
            .finish()
    }
}
