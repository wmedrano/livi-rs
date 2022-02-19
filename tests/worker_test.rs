// Integration test for a plugin "worker".
//
// Here we test a plugin that requires
// the LV2 Worker feature. We will use eg-sampler,
// a plugin that uses a worker to load a sample
// from disk for playback.
//
// Test outline:
//
// 1. Write a sample to disk, a small buffer filled with 1.0 values
// 2. Load eg-sampler plugin
// 3. Feed in single MIDI note to event buffer
// 4. Call instance.run and verify the output buffer is empty
//    (no sample has been loaded yet, so the output is silence).
// 5. Construct an LV2 Atom event instructing the sampler to
//    load the sample written to disk. Add this event to event buffer.
// 6. Call instance.run and verify output buffer is still empty
//    (the worker has not yet loaded the sample,
//    so we still expect the output to be silence).
// 7. Run the worker
// 8. Call instance.run and verify output buffer is still empty
//    (the worker response swaps in the loaded sample,
//    but this does not happen until after audio
//    processing occurs - so the outputs remain zero still).
// 9. Feed in MIDI note to event buffer and call instance.run
// 10. Verify output buffer now contains the expected sample.
//
// We confirm that the worker feature is operating as expected
// because the sampler is correctly playing back the sample
// that the worker loaded from disk.
//
// Obviously this test does not reflect reality perfectly
// since everything is running in one thread. In a real application
// the instance.run method will usually be called in the realtime
// thread while the worker will be run in a non-realtime thread.

use livi::event::{LV2AtomEventBuilder, LV2AtomSequence};
use livi::{EmptyPortConnections, Instance, WorkerManager, World};
use std::ffi::CStr;
use std::mem::size_of;
use tempfile::NamedTempFile;

const MIN_BLOCK_SIZE: usize = 1;
const MAX_BLOCK_SIZE: usize = 256;
const SAMPLE_RATE: f64 = 44100.0;
const MAX_PATH_SIZE: usize = 256;

// These structs define the message
// that we will deliver to the plugin
// in order to trigger the sample
// to be loaded. We do not talk to
// the worker directly but send
// this message into the plugin
// instance Atom event buffer.
// The plugin then communicates
// with the worker to load the sample.
#[repr(C)]
struct PatchProperty {
    key: u32,
    context: u32,
    value: lv2_sys::LV2_Atom_URID,
}

#[repr(C)]
struct PathAtom {
    atom: lv2_sys::LV2_Atom,
    body: [u8; MAX_PATH_SIZE],
}

#[repr(C)]
struct PatchValue {
    key: u32,
    context: u32,
    value: PathAtom,
}

#[repr(C)]
struct SetSamplerMessage(lv2_sys::LV2_Atom_Object_Body, PatchValue, PatchProperty);

// Some helper functions
fn run_instance_with_input_sequence(
    instance: &mut Instance,
    world: &mut World,
    input: LV2AtomSequence,
) -> [Vec<f32>; 1] {
    let mut output_events = LV2AtomSequence::new(world, 1024);
    let mut outputs = [vec![0.0; MAX_BLOCK_SIZE]];

    let ports = EmptyPortConnections::new(MAX_BLOCK_SIZE)
        .with_atom_sequence_inputs(std::iter::once(&input))
        .with_atom_sequence_outputs(std::iter::once(&mut output_events))
        .with_audio_outputs(outputs.iter_mut().map(|output| output.as_mut_slice()));

    unsafe { instance.run(ports).unwrap() };

    outputs
}

fn run_instance_with_single_midi_note_input(
    instance: &mut Instance,
    world: &mut World,
) -> [Vec<f32>; 1] {
    let input = {
        let mut s = LV2AtomSequence::new(world, 1024);
        let play_note_data = [0x90, 0x40, 0x7f];
        s.push_midi_event::<3>(1, world.midi_urid(), &play_note_data)
            .unwrap();
        s
    };
    run_instance_with_input_sequence(instance, world, input)
}

fn build_sampler_message(world: &mut World, sample_filepath: &str) -> SetSamplerMessage {
    let eg_sample_urid = world
        .urid(CStr::from_bytes_with_nul(b"http://lv2plug.in/plugins/eg-sampler#sample\0").unwrap());
    let urid_urid = world.urid(CStr::from_bytes_with_nul(lv2_sys::LV2_ATOM__URID).unwrap());
    let patch_property_urid =
        world.urid(CStr::from_bytes_with_nul(lv2_sys::LV2_PATCH__property).unwrap());
    let patch_value_urid =
        world.urid(CStr::from_bytes_with_nul(lv2_sys::LV2_PATCH__value).unwrap());
    let patch_set_urid = world.urid(CStr::from_bytes_with_nul(lv2_sys::LV2_PATCH__Set).unwrap());
    let path_urid = world.urid(CStr::from_bytes_with_nul(lv2_sys::LV2_ATOM__Path).unwrap());

    let mut path = [0_u8; MAX_PATH_SIZE];
    path[..sample_filepath.len()].copy_from_slice(sample_filepath.as_bytes());

    SetSamplerMessage(
        lv2_sys::LV2_Atom_Object_Body {
            id: 0,
            otype: patch_set_urid,
        },
        PatchValue {
            key: patch_value_urid,
            context: 0,
            value: PathAtom {
                atom: lv2_sys::LV2_Atom {
                    size: MAX_PATH_SIZE as u32,
                    type_: path_urid,
                },
                body: path,
            },
        },
        PatchProperty {
            key: patch_property_urid,
            context: 0,
            value: lv2_sys::LV2_Atom_URID {
                atom: lv2_sys::LV2_Atom {
                    size: size_of::<lv2_raw::LV2Urid>() as u32,
                    type_: urid_urid,
                },
                body: eg_sample_urid,
            },
        },
    )
}

fn assert_silence(buffers: [Vec<f32>; 1]) {
    for buffer in buffers {
        for sample in buffer {
            assert_eq!(sample, 0.0);
        }
    }
}

fn assert_not_silence(buffers: [Vec<f32>; 1]) {
    for buffer in buffers {
        for sample in buffer {
            assert_ne!(sample, 0.0);
        }
    }
}

#[test]
fn test_sampler() {
    let cwd = std::env::current_dir().unwrap();
    let mut out_file = NamedTempFile::new_in(cwd).unwrap();
    let sample = wav::bit_depth::BitDepth::ThirtyTwoFloat(vec![1.0; MAX_BLOCK_SIZE]);
    let header = wav::Header::new(wav::header::WAV_FORMAT_PCM, 1, SAMPLE_RATE as u32, 32);
    wav::write(header, &sample, &mut out_file).unwrap();

    let mut world = World::with_load_bundle("file:///usr/lib/lv2/eg-sampler.lv2/");
    world
        .initialize_block_length(MIN_BLOCK_SIZE, MAX_BLOCK_SIZE)
        .unwrap();
    let plugin = world
        .plugin_by_uri("http://lv2plug.in/plugins/eg-sampler")
        .expect("Plugin not found.");
    let mut instance = unsafe {
        plugin
            .instantiate(SAMPLE_RATE)
            .expect("Could not instantiate plugin.")
    };

    let mut worker_manager = WorkerManager::default();

    if let Some(worker) = instance.get_worker() {
        worker_manager.add_worker(worker);
    }

    let outputs = run_instance_with_single_midi_note_input(&mut instance, &mut world);
    assert_silence(outputs);

    let message = build_sampler_message(&mut world, out_file.path().to_str().unwrap());
    let object_urid = world.urid(CStr::from_bytes_with_nul(lv2_sys::LV2_ATOM__Object).unwrap());

    let input = {
        let mut sequence = LV2AtomSequence::new(&world, 1024);
        let m = &message as *const SetSamplerMessage as *const u8;
        let slice: &[u8] = unsafe { std::slice::from_raw_parts(m, size_of::<SetSamplerMessage>()) };
        let event = LV2AtomEventBuilder::<512>::new(0, object_urid, slice).unwrap();
        sequence.push_event(&event).unwrap();
        sequence
    };

    let outputs = run_instance_with_input_sequence(&mut instance, &mut world, input);
    assert_silence(outputs);

    worker_manager.run_workers();

    let outputs = run_instance_with_single_midi_note_input(&mut instance, &mut world);
    assert_silence(outputs);

    let outputs = run_instance_with_single_midi_note_input(&mut instance, &mut world);
    // There is now audio content
    // in the outputs, indicating
    // that the sample file was loaded
    // correctly by the worker.
    assert_not_silence(outputs);
}
