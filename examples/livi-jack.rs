use log::{error, info};
use structopt::StructOpt;

/// The configuration for the backend.
#[derive(StructOpt, Debug)]
struct Configuration {
    /// The uri of the plugin to instantiate.
    /// To see the set of available plugins, use `lv2ls`.
    #[structopt(
        long = "plugin-uri",
        default_value = "http://drobilla.net/plugins/mda/EPiano"
    )]
    plugin_uri: String,

    /// The amount of debug logging to provide. Valid values are "off", "error", "warn", "info",
    /// "debug", and "trace".
    #[structopt(long = "log-level", default_value = "info")]
    log_level: log::LevelFilter,
}

fn main() {
    let config = Configuration::from_args();
    env_logger::builder().filter_level(config.log_level).init();

    let mut livi = livi::World::new();
    let plugin = livi
        .iter_plugins()
        .find(|p| p.uri() == config.plugin_uri)
        .unwrap();

    let (client, status) =
        jack::Client::new(&plugin.name(), jack::ClientOptions::NO_START_SERVER).unwrap();
    info!("Created jack client {:?} with status {:?}.", client, status);

    let midi_urid = livi.midi_urid();
    livi.initialize_block_length(client.buffer_size() as usize, client.buffer_size() as usize)
        .unwrap();
    let mut plugin_instance = unsafe { plugin.instantiate(client.sample_rate() as f64).unwrap() };

    let inputs: Vec<jack::Port<jack::AudioIn>> = plugin
        .ports_with_type(livi::PortType::AudioInput)
        .inspect(|p| info!("Initializing audio input {}.", p.name))
        .map(|p| client.register_port(&p.name, jack::AudioIn).unwrap())
        .collect();
    let mut outputs: Vec<jack::Port<jack::AudioOut>> = plugin
        .ports_with_type(livi::PortType::AudioOutput)
        .inspect(|p| info!("Initializing audio output {}.", p.name))
        .map(|p| client.register_port(&p.name, jack::AudioOut).unwrap())
        .collect();
    let controls: Vec<f32> = plugin
        .ports_with_type(livi::PortType::ControlInput)
        .inspect(|p| info!("Using {} = {}", p.name, p.default_value))
        .map(|p| p.default_value)
        .collect();
    let mut events_in = plugin
        .ports_with_type(livi::PortType::EventsInput)
        .map(|p| client.register_port(&p.name, jack::MidiIn).unwrap())
        .map(|p| (p, livi::event::LV2AtomSequence::new(4096)))
        .collect::<Vec<_>>();
    let mut events_out = plugin
        .ports_with_type(livi::PortType::EventsOutput)
        .map(|p| client.register_port(&p.name, jack::MidiOut).unwrap())
        .map(|p| (p, livi::event::LV2AtomSequence::new(4096)))
        .collect::<Vec<_>>();

    let process_handler =
        jack::ClosureProcessHandler::new(move |_: &jack::Client, ps: &jack::ProcessScope| {
            for (port, buffer) in events_in.iter_mut() {
                buffer.clear();
                for midi in port.iter(ps) {
                    buffer.append_midi_event(midi.time as i64, midi_urid, midi.bytes);
                }
            }

            let ports = livi::PortValues {
                frames: ps.n_frames() as usize,
                control_input: controls.iter(),
                audio_input: inputs.iter().map(|p| p.as_slice(ps)),
                audio_output: outputs.iter_mut().map(|p| p.as_mut_slice(ps)),
                atom_sequence_input: events_in.iter().map(|(_, e)| e),
                atom_sequence_output: events_out.iter_mut().map(|(_, e)| e),
            };
            match unsafe { plugin_instance.run(ports) } {
                Ok(()) => (),
                Err(e) => {
                    error!("Error: {:?}", e);
                    return jack::Control::Quit;
                }
            }
            for (_, _) in events_out.iter_mut() {
                unimplemented!("events cannot yet be output to midi");
            }
            jack::Control::Continue
        });

    let active_client = client.activate_async((), process_handler).unwrap();

    std::thread::park();
    drop(active_client);
}
