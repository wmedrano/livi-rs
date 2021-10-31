# Livi

A library for hosting LV2 plugins.

Note: This is a work in progress and has not yet been full tested.

## Quickstart

Below is an example on how to run the mda EPiano plugin.

```rust
use livi;

let mut world = livi::World::new();
const MIN_BLOCK_SIZE: usize = 1;
const MAX_BLOCK_SIZE: usize = 256;
const SAMPLE_RATE: f64 = 44100.0;
world
    .initialize_block_length(MIN_BLOCK_SIZE, MAX_BLOCK_SIZE)
    .unwrap();
let plugin = world
    // This is the URI for mda EPiano. You can use the `lv2ls` command line
    // utility to see all available LV2 plugins.
    .plugin_by_uri("http://drobilla.net/plugins/mda/EPiano")
    .expect("Plugin not found.");
let mut instance = unsafe {
    plugin
        .instantiate(SAMPLE_RATE)
        .expect("Could not instantiate plugin.")
};

// Where midi events will be read from.
let input = {
    let mut s = livi::event::LV2AtomSequence::new(1024);
    let play_note_data = [0x90, 0x40, 0x7f];
    s.push_midi_event::<3>(1, world.midi_urid(), &play_note_data)
        .unwrap();
    s
};

// Where parameters can be set. We initialize to the plugin's default values.
let params: Vec<f32> = plugin
    .ports_with_type(livi::PortType::ControlInput)
    .map(|p| p.default_value)
    .collect();
// This is where the audio data will be stored.
let mut outputs = [
    vec![0.0; MAX_BLOCK_SIZE], // For mda EPiano, this is the left channel.
    vec![0.0; MAX_BLOCK_SIZE], // For mda EPiano, this is the right channel.
];

// Set up the port configuration and run the plugin!
// The results will be stored in `outputs`.
let ports = EmptyPortConnections::new(MAX_BLOCK_SIZE)
    .with_atom_sequence_inputs(std::iter::once(&input))
    .with_audio_outputs(outputs.iter_mut().map(|output| output.as_mut_slice()))
    .with_control_inputs(params.iter());
unsafe { instance.run(ports).unwrap() };
```

## Supported LV2 Features

LV2 has a simple core interface but is accompanied by extensions that can add
lots of functionality. This library aims to support as many features as possible
out of the box.

- [`http://lv2plug.in/ns/ext/urid#map`](http://lv2plug.in/ns/ext/urid#map)
- [`http://lv2plug.in/ns/ext/urid#unmap`](http://lv2plug.in/ns/ext/urid#unmap)
- [`http://lv2plug.in/ns/ext/options#options`](http://lv2plug.in/ns/ext/options#options])
- [`http://lv2plug.in/ns/ext/buf-size#boundedBlockLength`](http://lv2plug.in/ns/ext/buf-size#boundedBlockLength)
