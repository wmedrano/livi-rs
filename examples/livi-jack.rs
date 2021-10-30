use log::{debug, error, info};
use std::convert::TryFrom;
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

struct Processor {
    plugin: livi::Instance,
    midi_urid: lv2_raw::LV2Urid,
    audio_inputs: Vec<jack::Port<jack::AudioIn>>,
    audio_outputs: Vec<jack::Port<jack::AudioOut>>,
    control_inputs: Vec<f32>,
    control_outputs: Vec<f32>,
    event_inputs: Vec<(jack::Port<jack::MidiIn>, livi::event::LV2AtomSequence)>,
    event_outputs: Vec<(jack::Port<jack::MidiOut>, livi::event::LV2AtomSequence)>,
}

impl jack::ProcessHandler for Processor {
    fn process(&mut self, _: &jack::Client, ps: &jack::ProcessScope) -> jack::Control {
        for (port, buffer) in &mut self.event_inputs.iter_mut() {
            buffer.clear();
            for midi in port.iter(ps) {
                const MAX_SUPPORTED_MIDI_SIZE: usize = 32;
                match buffer.push_midi_event::<MAX_SUPPORTED_MIDI_SIZE>(
                    i64::from(midi.time),
                    self.midi_urid,
                    midi.bytes,
                ) {
                    Ok(_) => (),
                    Err(e) => {
                        // This should be a warning, but we don't want to
                        // hurt performance for something that may not be an
                        // issue that the user can fix.
                        debug!("Failed to push midi event: {:?}", e);
                    }
                }
            }
        }

        let ports = livi::PortConnections {
            frames: ps.n_frames() as usize,
            control_input: self.control_inputs.iter(),
            control_output: self.control_outputs.iter_mut(),
            audio_input: self.audio_inputs.iter().map(|p| p.as_slice(ps)),
            audio_output: self.audio_outputs.iter_mut().map(|p| p.as_mut_slice(ps)),
            atom_sequence_input: self.event_inputs.iter().map(|(_, e)| e),
            atom_sequence_output: self.event_outputs.iter_mut().map(|(_, e)| e),
        };
        match unsafe { self.plugin.run(ports) } {
            Ok(()) => (),
            Err(e) => {
                error!("Error: {:?}", e);
                return jack::Control::Quit;
            }
        }
        for (dst, src) in &mut self.event_outputs.iter_mut() {
            let mut writer = dst.writer(ps);
            for event in src.iter() {
                if event.event.body.mytype != self.midi_urid {
                    debug!(
                        "Found non-midi event with URID: {}",
                        event.event.body.mytype
                    );
                    continue;
                }
                let jack_event = jack::RawMidi {
                    time: u32::try_from(event.event.time_in_frames).unwrap(),
                    bytes: event.data,
                };
                match writer.write(&jack_event) {
                    Ok(()) => (),
                    Err(e) => debug!("Failed to write midi event: {:?}", e),
                }
            }
        }
        jack::Control::Continue
    }
}

fn main() {
    let config = Configuration::from_args();
    env_logger::builder().filter_level(config.log_level).init();

    let mut livi = livi::World::new();
    let plugin = livi
        .iter_plugins()
        .find(|p| p.uri() == config.plugin_uri)
        .unwrap_or_else(|| panic!("Could not find plugin with URI {}", config.plugin_uri));

    let (client, status) =
        jack::Client::new(&plugin.name(), jack::ClientOptions::NO_START_SERVER).unwrap();
    info!("Created jack client {:?} with status {:?}.", client, status);

    let midi_urid = livi.midi_urid();
    livi.initialize_block_length(client.buffer_size() as usize, client.buffer_size() as usize)
        .unwrap();
    #[allow(clippy::cast_precision_loss)]
    let plugin_instance = unsafe { plugin.instantiate(client.sample_rate() as f64).unwrap() };

    let audio_inputs: Vec<jack::Port<jack::AudioIn>> = plugin
        .ports_with_type(livi::PortType::AudioInput)
        .inspect(|p| info!("Initializing audio input {}.", p.name))
        .map(|p| client.register_port(&p.name, jack::AudioIn).unwrap())
        .collect();
    let audio_outputs: Vec<jack::Port<jack::AudioOut>> = plugin
        .ports_with_type(livi::PortType::AudioOutput)
        .inspect(|p| info!("Initializing audio output {}.", p.name))
        .map(|p| client.register_port(&p.name, jack::AudioOut).unwrap())
        .collect();
    let control_inputs: Vec<f32> = plugin
        .ports_with_type(livi::PortType::ControlInput)
        .inspect(|p| info!("Using {:?}{} = {}", p.port_type, p.name, p.default_value))
        .map(|p| p.default_value)
        .collect();
    let control_outputs: Vec<f32> = plugin
        .ports_with_type(livi::PortType::ControlOutput)
        .inspect(|p| info!("Using {:?}{} = {}", p.port_type, p.name, p.default_value))
        .map(|p| p.default_value)
        .collect();
    let event_inputs = plugin
        .ports_with_type(livi::PortType::EventsInput)
        .map(|p| client.register_port(&p.name, jack::MidiIn).unwrap())
        .map(|p| (p, livi::event::LV2AtomSequence::new(4096)))
        .collect::<Vec<_>>();
    let event_outputs = plugin
        .ports_with_type(livi::PortType::EventsOutput)
        .map(|p| client.register_port(&p.name, jack::MidiOut).unwrap())
        .map(|p| (p, livi::event::LV2AtomSequence::new(4096)))
        .collect::<Vec<_>>();
    let process_handler = Processor {
        plugin: plugin_instance,
        midi_urid,
        audio_inputs,
        audio_outputs,
        control_inputs,
        control_outputs,
        event_inputs,
        event_outputs,
    };

    let active_client = client.activate_async((), process_handler).unwrap();

    std::thread::park();
    drop(active_client);
}
