# Livi

[![crates.io](https://img.shields.io/crates/v/livi.svg)](https://crates.io/crates/livi)
[![docs.rs](https://docs.rs/livi/badge.svg)](https://docs.rs/livi)

[![License: MIT](https://img.shields.io/badge/License-MIT-green.svg)](https://opensource.org/licenses/MIT)
[![Tests](https://github.com/wmedrano/livi-rs/actions/workflows/test.yml/badge.svg)](https://github.com/wmedrano/livi-rs/actions/workflows/test.yml)

A library for hosting LV2 plugins.

Note: This is a work in progress and has not yet been full tested.

## Supported LV2 Features

LV2 has a simple core interface but is accompanied by extensions that can add
lots of functionality. This library aims to support as many features as possible
out of the box.

- [`http://lv2plug.in/ns/ext/urid#map`](http://lv2plug.in/ns/ext/urid#map)
- [`http://lv2plug.in/ns/ext/urid#unmap`](http://lv2plug.in/ns/ext/urid#unmap)
- [`http://lv2plug.in/ns/ext/options#options`](http://lv2plug.in/ns/ext/options#options])
- [`http://lv2plug.in/ns/ext/buf-size#boundedBlockLength`](http://lv2plug.in/ns/ext/buf-size#boundedBlockLength)

## Quickstart

Below is an example on how to run the mda EPiano plugin.

```rust
let world = livi::World::new();
const SAMPLE_RATE: f64 = 44100.0;
let worker_manager = std::sync::Arc::new(livi::WorkerManager::default());
let features = world.build_features(livi::FeaturesBuilder {
    min_block_length: 1,
    max_block_length: 4096,
    worker_manager: worker_manager.clone(),
});
let plugin = world
    // This is the URI for mda EPiano. You can use the `lv2ls` command line
    // utility to see all available LV2 plugins.
    .plugin_by_uri("http://drobilla.net/plugins/mda/EPiano")
    .expect("Plugin not found.");
let mut instance = unsafe {
    plugin
        .instantiate(features.clone(), SAMPLE_RATE)
        .expect("Could not instantiate plugin.")
};
if let Some(worker) = instance.take_worker() {
    worker_manager.add_worker(worker);
}

// Where midi events will be read from.
let input = {
    let mut s = livi::event::LV2AtomSequence::new(&features, 1024);
    let play_note_data = [0x90, 0x40, 0x7f];
    s.push_midi_event::<3>(1, features.midi_urid(), &play_note_data)
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
    vec![0.0; features.max_block_length()], // For mda EPiano, this is the left channel.
    vec![0.0; features.max_block_length()], // For mda EPiano, this is the right channel.
];

// Set up the port configuration and run the plugin!
// The results will be stored in `outputs`.
let ports = livi::EmptyPortConnections::new(features.max_block_length())
    .with_atom_sequence_inputs(std::iter::once(&input))
    .with_audio_outputs(outputs.iter_mut().map(|output| output.as_mut_slice()))
    .with_control_inputs(params.iter());
unsafe { instance.run(ports).unwrap() };

// Plugins may push asynchronous works to the worker. When operating in
// Realtime, `run_workers` should be run in a separate thread.
worker_manager.run_workers();
```

## Building, Testing, and Running

- Build - `cargo build`
- Test - `cargo test`, requires mda LV2 plugins.
- Run livi-jack - `cargo run --example livi-jack --release -- --plugin-uri=http://drobilla.net/plugins/mda/EPiano`.
