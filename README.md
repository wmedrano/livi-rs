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
use livi;

let mut world = livi::World::new();
// Running a plugin for less samples than MIN_BLOCK_SIZE or more samples than
// MAX_BLOCK_SIZE will fail.
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

// The size of the events buffer. This is where midi is read from.
const ATOM_SEQUENCE_SIZE: usize = 32768; // 32KiB
// port_data contains all the input and outputs for the plugin. Alternatively,
// you can create your own buffers and build ports starting with
// `EmptyPortConnections::new`. See `./examples/livi-jack.rs` for how to buidl
// ports from your own buffers.
let mut port_data = plugin.build_port_data(ATOM_SEQUENCE_SIZE)
    .expect("Could not build port data.");
let ports = port_data.as_port_connections(MAX_BLOCK_SIZE);
unsafe { instance.run(ports).unwrap() };
```

## Building, Testing, and Running

- Build - `cargo build`
- Test - `cargo test`, requires mda LV2 plugins.
- Run livi-jack - `cargo run --example livi-jack --release -- --plugin-uri=http://drobilla.net/plugins/mda/EPiano`.
