use log::{error, info};

fn main() {
    env_logger::builder()
        .filter_level(log::LevelFilter::Info)
        .init();
    let (client, status) =
        jack::Client::new("livi-jack", jack::ClientOptions::NO_START_SERVER).unwrap();
    info!("Created jack client {:?} with status {:?}.", client, status);

    let livi = livi::World::new();
    let midi_urid =
        livi.urid(&std::ffi::CString::new("http://lv2plug.in/ns/ext/midi#MidiEvent").unwrap());
    let plugin = livi
        .iter_plugins()
        .find(|p| p.uri() == "http://drobilla.net/plugins/mda/EPiano")
        .unwrap();
    let mut plugin_instance = unsafe { plugin.instantiate(client.sample_rate() as f64).unwrap() };

    let inputs: Vec<jack::Port<jack::AudioIn>> = plugin
        .ports()
        .filter(|p| p.port_type == livi::PortType::AudioInput)
        .inspect(|p| info!("Initializing audio input {}.", p.name))
        .map(|p| client.register_port(&p.name, jack::AudioIn).unwrap())
        .collect();
    let mut outputs: Vec<jack::Port<jack::AudioOut>> = plugin
        .ports()
        .filter(|p| p.port_type == livi::PortType::AudioOutput)
        .inspect(|p| info!("Initializing audio output {}.", p.name))
        .map(|p| client.register_port(&p.name, jack::AudioOut).unwrap())
        .collect();
    let controls: Vec<f32> = plugin
        .ports()
        .filter(|p| p.port_type == livi::PortType::ControlInput)
        .inspect(|p| info!("Using {} = {}", p.name, p.default_value))
        .map(|p| p.default_value)
        .collect();
    let mut events = livi::event::LV2AtomSequence::new(4096);
    let midi = client.register_port("midi", jack::MidiIn).unwrap();

    let process_handler =
        jack::ClosureProcessHandler::new(move |_: &jack::Client, ps: &jack::ProcessScope| {
            events.clear();
            for midi in midi.iter(ps) {
                events.append_midi_event(midi.time as i64, midi_urid, midi.bytes);
            }
            let ports = livi::PortValues {
                frames: ps.n_frames() as usize,
                control_input: controls.iter(),
                audio_input: inputs.iter().map(|p| p.as_slice(ps)),
                audio_output: outputs.iter_mut().map(|p| p.as_mut_slice(ps)),
                atom_sequence: Some(&events),
            };
            match unsafe { plugin_instance.run(ports) } {
                Ok(()) => jack::Control::Continue,
                Err(e) => {
                    error!("Error: {:?}", e);
                    jack::Control::Quit
                }
            }
        });

    let active_client = client.activate_async((), process_handler).unwrap();

    std::thread::park();
    drop(active_client);
}
