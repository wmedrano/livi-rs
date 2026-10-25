// Integration test for a plugin "worker".
//
// Here we test a plugin that requires
// the LV2 Worker feature. We will use x42 midimap,
// a plugin that uses a worker to load a MIDI
// mapping config file from disk.
//
// Test outline:
//
// 1. Write a config file to disk with a single rule that
//    maps MIDI note 64 to MIDI note 72.
// 2. Load midimap plugin
// 3. Call instance.run and verify there are no active rules
//    (no config has been loaded yet).
// 4. Construct an LV2 Atom event instructing midimap to
//    load the config written to disk. Add this event to event buffer.
// 5. Call instance.run and verify there are still no active rules
//    (the worker has not yet loaded the config).
// 6. Run the worker
// 7. Call instance.run and verify there are still no active rules
//    (the worker response swaps in the loaded config,
//    but this does not happen until after MIDI
//    processing occurs - so no rules are applied yet).
// 8. Feed in MIDI note 64 to event buffer and call instance.run
// 9. Verify there is now one active rule and that the
//    output contains the mapped MIDI note 72.
//
// We confirm that the worker feature is operating as expected
// because midimap is correctly applying the rule
// that the worker loaded from disk.
//
// Obviously this test does not reflect reality perfectly
// since everything is running in one thread. In a real application
// the instance.run method will usually be called in the realtime
// thread while the worker will be run in a non-realtime thread.

use livi::event::{LV2AtomEventBuilder, LV2AtomSequence};
use livi::{EmptyPortConnections, Features, Instance, PortIndex, World};
use std::ffi::CStr;
use std::io::Write;
use std::mem::size_of;
use tempfile::NamedTempFile;

const MIN_BLOCK_SIZE: usize = 1;
const MAX_BLOCK_SIZE: usize = 256;
const SAMPLE_RATE: f64 = 44100.0;
const MAX_PATH_SIZE: usize = 256;
const RULE_COUNT_PORT: PortIndex = PortIndex(2);
const NOTE_ON_IN: [u8; 3] = [0x90, 64, 0x7f];
const NOTE_ON_MAPPED: [u8; 3] = [0x90, 72, 0x7f];

// These structs define the message
// that we will deliver to the plugin
// in order to trigger the config
// to be loaded. We do not talk to
// the worker directly but send
// this message into the plugin
// instance Atom event buffer.
// The plugin then communicates
// with the worker to load the config.
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
struct SetConfigMessage(lv2_sys::LV2_Atom_Object_Body, PatchValue, PatchProperty);

// Some helper functions

/// Run the instance and return the MIDI events it output.
fn run_instance_with_input_sequence(
    instance: &mut Instance,
    features: &Features,
    input: LV2AtomSequence,
) -> Vec<Vec<u8>> {
    let mut output_events = LV2AtomSequence::new(features, 1024);

    let ports = EmptyPortConnections::new()
        .with_atom_sequence_inputs(std::iter::once(&input))
        .with_atom_sequence_outputs(std::iter::once(&mut output_events));

    unsafe { instance.run(MAX_BLOCK_SIZE, ports).unwrap() };

    output_events
        .iter()
        .filter(|e| e.event.body.type_ == features.midi_urid())
        .map(|e| e.data.to_vec())
        .collect()
}

fn run_instance_with_single_midi_note_input(
    instance: &mut Instance,
    features: &Features,
) -> Vec<Vec<u8>> {
    let input = {
        let mut s = LV2AtomSequence::new(features, 1024);
        s.push_midi_event::<3>(1, features.midi_urid(), &NOTE_ON_IN)
            .unwrap();
        s
    };
    run_instance_with_input_sequence(instance, features, input)
}

fn build_set_config_message(features: &Features, config_filepath: &str) -> SetConfigMessage {
    let cfgfile_urid = features.urid(c"http://gareus.org/oss/lv2/midimap#cfgfile");
    let urid_urid = features.urid(CStr::from_bytes_with_nul(lv2_sys::LV2_ATOM__URID).unwrap());
    let patch_property_urid =
        features.urid(CStr::from_bytes_with_nul(lv2_sys::LV2_PATCH__property).unwrap());
    let patch_value_urid =
        features.urid(CStr::from_bytes_with_nul(lv2_sys::LV2_PATCH__value).unwrap());
    let patch_set_urid = features.urid(CStr::from_bytes_with_nul(lv2_sys::LV2_PATCH__Set).unwrap());
    let path_urid = features.urid(CStr::from_bytes_with_nul(lv2_sys::LV2_ATOM__Path).unwrap());

    let mut path = [0_u8; MAX_PATH_SIZE];
    path[..config_filepath.len()].copy_from_slice(config_filepath.as_bytes());

    SetConfigMessage(
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
                body: cfgfile_urid,
            },
        },
    )
}

#[test]
fn test_midimap() {
    let cwd = std::env::current_dir().unwrap();
    let mut config_file = NamedTempFile::new_in(cwd).unwrap();
    write!(config_file, "midimap v1\nNOTE 64 ANY | SAME 72 SAME\n").unwrap();
    config_file.flush().unwrap();

    let world = World::with_load_bundle("file:///usr/lib/lv2/midimap.lv2/");
    let plugin = world
        .plugin_by_uri("http://gareus.org/oss/lv2/midimap")
        .expect("Plugin not found.");
    let features = world.build_features(livi::FeaturesBuilder {
        min_block_length: MIN_BLOCK_SIZE,
        max_block_length: MAX_BLOCK_SIZE,
    });
    let mut instance = unsafe {
        plugin
            .instantiate(features.clone(), SAMPLE_RATE)
            .expect("Could not instantiate plugin.")
    };

    run_instance_with_single_midi_note_input(&mut instance, &features);
    assert_eq!(instance.control_output(RULE_COUNT_PORT), Some(0.0));

    let message = build_set_config_message(&features, config_file.path().to_str().unwrap());
    let object_urid = features.urid(CStr::from_bytes_with_nul(lv2_sys::LV2_ATOM__Object).unwrap());

    let input = {
        let mut sequence = LV2AtomSequence::new(&features, 1024);
        let m = &message as *const SetConfigMessage as *const u8;
        let slice: &[u8] = unsafe { std::slice::from_raw_parts(m, size_of::<SetConfigMessage>()) };
        let event = LV2AtomEventBuilder::<512>::new(0, object_urid, slice).unwrap();
        sequence.push_event(&event).unwrap();
        sequence
    };

    run_instance_with_input_sequence(&mut instance, &features, input);
    assert_eq!(instance.control_output(RULE_COUNT_PORT), Some(0.0));

    // Run the worker manually to ensure it has run before inspecting the outputs.
    features.worker_manager().run_workers();

    // The worker response swaps in the loaded config, but
    // this does not happen until after MIDI processing
    // occurs - so no rules are active yet.
    let outputs = run_instance_with_single_midi_note_input(&mut instance, &features);
    assert_eq!(instance.control_output(RULE_COUNT_PORT), Some(0.0));
    assert_ne!(outputs, vec![NOTE_ON_MAPPED.to_vec()]);

    // The rule loaded by the worker is now applied
    // to the MIDI events, indicating that the
    // config file was loaded correctly by the worker.
    let outputs = run_instance_with_single_midi_note_input(&mut instance, &features);
    assert_eq!(instance.control_output(RULE_COUNT_PORT), Some(1.0));
    assert_eq!(outputs, vec![NOTE_ON_MAPPED.to_vec()]);
}
