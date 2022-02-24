#[test]
fn test_readme_example() {
    let world = livi::World::new();
    const SAMPLE_RATE: f64 = 44100.0;
    let features = world.build_features(livi::FeaturesBuilder::default());
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

    // Where midi events will be read from.
    let input = {
        let mut s = livi::event::LV2AtomSequence::new(&features, 1024);
        let play_note_data = [0x90, 0x40, 0x7f];
        s.push_midi_event::<3>(1, features.midi_urid(), &play_note_data)
            .unwrap();
        s
    };

    // This is where the audio data will be stored.
    let mut outputs = [
        vec![0.0; features.max_block_length()], // For mda EPiano, this is the left channel.
        vec![0.0; features.max_block_length()], // For mda EPiano, this is the right channel.
    ];

    // Set up the port configuration and run the plugin!
    // The results will be stored in `outputs`.
    let ports = livi::EmptyPortConnections::new()
        .with_atom_sequence_inputs(std::iter::once(&input))
        .with_audio_outputs(outputs.iter_mut().map(|output| output.as_mut_slice()));
    unsafe { instance.run(features.max_block_length(), ports).unwrap() };

    // Plugins may push asynchronous works to the worker. When operating in
    // Realtime, `run_workers` should be run in a separate thread.
    features.worker_manager().run_workers();
}
