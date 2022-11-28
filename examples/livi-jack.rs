/// livi-jack hosts an LV2 plugin on JACK!
///
/// Run with: `cargo run --release -- --plugin-uri=${PLUGIN_URI}`
use livi::event::LV2AtomSequence;
use log::{debug, error, info, warn};
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

    /// If ports should automatticaly be connected.
    #[structopt(long = "autoconnect", parse(try_from_str), default_value = "true")]
    autoconnect: bool,
}

fn main() {
    let config = Configuration::from_args();
    env_logger::builder().filter_level(config.log_level).init();

    let livi = livi::World::new();
    let plugin = livi
        .plugin_by_uri(&config.plugin_uri)
        .unwrap_or_else(|| panic!("Could not find plugin with URI {}", config.plugin_uri));

    let (client, status) =
        jack::Client::new(&plugin.name(), jack::ClientOptions::NO_START_SERVER).unwrap();
    info!("Created jack client {:?} with status {:?}.", client, status);

    let process_handler = Processor::new(&livi, plugin, &client);
    if config.autoconnect {
        process_handler.autoconnect(&client);
    }

    // Keep reference to client to prevent it from dropping.
    let _active_client = client.activate_async((), process_handler).unwrap();
    std::thread::park();
}

struct Processor {
    plugin: livi::Instance,
    midi_urid: lv2_raw::LV2Urid,
    audio_inputs: Vec<jack::Port<jack::AudioIn>>,
    audio_outputs: Vec<jack::Port<jack::AudioOut>>,
    event_inputs: Vec<(jack::Port<jack::MidiIn>, LV2AtomSequence)>,
    event_outputs: Vec<(jack::Port<jack::MidiOut>, LV2AtomSequence)>,
    cv_inputs: Vec<jack::Port<jack::AudioIn>>,
    cv_outputs: Vec<jack::Port<jack::AudioOut>>,
}

impl Processor {
    fn new(world: &livi::World, plugin: livi::Plugin, client: &jack::Client) -> Processor {
        let buffer_size = client.buffer_size() as usize;
        let features = world.build_features(livi::FeaturesBuilder {
            min_block_length: buffer_size,
            max_block_length: buffer_size,
        });
        #[allow(clippy::cast_precision_loss)]
        let plugin_instance = unsafe {
            plugin
                .instantiate(features.clone(), client.sample_rate() as f64)
                .unwrap()
        };

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
        const EVENT_BUFFER_SIZE: usize = 262_144; // ~262KiB
        let event_inputs = plugin
            .ports_with_type(livi::PortType::AtomSequenceInput)
            .map(|p| client.register_port(&p.name, jack::MidiIn).unwrap())
            .map(|p| (p, LV2AtomSequence::new(&features, EVENT_BUFFER_SIZE)))
            .collect::<Vec<_>>();
        let event_outputs = plugin
            .ports_with_type(livi::PortType::AtomSequenceOutput)
            .map(|p| client.register_port(&p.name, jack::MidiOut).unwrap())
            .map(|p| (p, LV2AtomSequence::new(&features, EVENT_BUFFER_SIZE)))
            .collect::<Vec<_>>();
        let cv_inputs: Vec<jack::Port<jack::AudioIn>> = plugin
            .ports_with_type(livi::PortType::CVInput)
            .inspect(|p| info!("Initializing cv input {}.", p.name))
            .map(|p| {
                client
                    .register_port(&format!("CV: {}", p.name), jack::AudioIn)
                    .unwrap()
            })
            .collect();
        let cv_outputs: Vec<jack::Port<jack::AudioOut>> = plugin
            .ports_with_type(livi::PortType::CVOutput)
            .inspect(|p| info!("Initializing cv output {}.", p.name))
            .map(|p| {
                client
                    .register_port(&format!("CV: {}", p.name), jack::AudioOut)
                    .unwrap()
            })
            .collect();
        Processor {
            plugin: plugin_instance,
            midi_urid: features.midi_urid(),
            audio_inputs,
            audio_outputs,
            event_inputs,
            event_outputs,
            cv_inputs,
            cv_outputs,
        }
    }

    fn autoconnect(&self, client: &jack::Client) {
        info!("Connecting audio outputs to playback devices.");
        let playback_ports = client.ports(
            None,
            Some(jack::jack_sys::FLOAT_MONO_AUDIO),
            jack::PortFlags::IS_PHYSICAL | jack::PortFlags::IS_INPUT,
        );
        for (output, input) in self.audio_outputs.iter().zip(playback_ports.iter()) {
            let output = output.name().unwrap_or_default();
            match client.connect_ports_by_name(&output, input) {
                Ok(()) => info!("Connected audio port {:?} to {:?}.", output, input),
                Err(err) => error!(
                    "Failed to connect audio port {:?} to {:?}. Error: {:?}.",
                    output, input, err
                ),
            };
        }

        info!("Connecting midi devices to midi inputs.");
        let midi_input_devices = client.ports(
            None,
            Some(jack::jack_sys::RAW_MIDI_TYPE),
            jack::PortFlags::IS_OUTPUT,
        );
        for (output, (input, _)) in midi_input_devices
            .iter()
            .zip(self.event_inputs.iter().cycle())
        {
            let input = input.name().unwrap_or_default();
            match client.connect_ports_by_name(output, &input) {
                Ok(()) => info!("Connected midi port {:?} to {:?}.", output, input),
                Err(err) => error!(
                    "Failed to connect midi port {:?} to {:?}. Error: {:?}.",
                    output, input, err
                ),
            };
        }
    }
}

impl jack::ProcessHandler for Processor {
    fn process(&mut self, _: &jack::Client, ps: &jack::ProcessScope) -> jack::Control {
        for (src, dst) in &mut self.event_inputs.iter_mut() {
            copy_midi_in_to_atom_sequence(src, dst, ps, self.midi_urid)
        }

        let ports = livi::PortConnections {
            audio_inputs: self.audio_inputs.iter().map(|p| p.as_slice(ps)),
            audio_outputs: self.audio_outputs.iter_mut().map(|p| p.as_mut_slice(ps)),
            atom_sequence_inputs: self.event_inputs.iter().map(|(_, e)| e),
            atom_sequence_outputs: self.event_outputs.iter_mut().map(|(_, e)| e),
            cv_inputs: self.cv_inputs.iter().map(|p| p.as_slice(ps)),
            cv_outputs: self.cv_outputs.iter_mut().map(|p| p.as_mut_slice(ps)),
        };
        match unsafe { self.plugin.run(ps.n_frames() as usize, ports) } {
            Ok(()) => (),
            Err(e) => {
                error!("Error: {:?}", e);
                return jack::Control::Quit;
            }
        }
        for (dst, src) in &mut self.event_outputs.iter_mut() {
            copy_atom_sequence_to_midi_out(src, dst, ps, self.midi_urid)
        }
        jack::Control::Continue
    }
}

fn copy_midi_in_to_atom_sequence(
    src: &jack::Port<jack::MidiIn>,
    dst: &mut LV2AtomSequence,
    ps: &jack::ProcessScope,
    midi_urid: lv2_raw::LV2Urid,
) {
    dst.clear();
    for midi in src.iter(ps) {
        const MAX_SUPPORTED_MIDI_SIZE: usize = 32;
        match dst.push_midi_event::<MAX_SUPPORTED_MIDI_SIZE>(
            i64::from(midi.time),
            midi_urid,
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

fn copy_atom_sequence_to_midi_out(
    src: &LV2AtomSequence,
    dst: &mut jack::Port<jack::MidiOut>,
    ps: &jack::ProcessScope,
    midi_urid: lv2_raw::LV2Urid,
) {
    let mut writer = dst.writer(ps);
    for event in src.iter() {
        if event.event.body.mytype != midi_urid {
            warn!(
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
