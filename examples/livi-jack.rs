/// livi-jack hosts an LV2 plugin on JACK!
///
/// Run with: `cargo run --release -- --plugin-uri=${PLUGIN_URI}`
use livi::event::LV2AtomSequence;
use log::{debug, error, info, warn};
use std::{convert::TryFrom, str::FromStr, sync::Arc};
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

    /// The ports to automatically connect. The values are playback [default]
    /// and none.
    #[structopt(long = "auto-connect", default_value = "playback")]
    auto_connect: AutoConnect,
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

    let (process_handler, workers) = Processor::new(&livi, plugin, &client);
    let plugin_audio_output_ports = process_handler.output_audio_ports();

    // Keep reference to client to prevent it from dropping.
    let active_client = client.activate_async((), process_handler).unwrap();
    let audio_playback_ports = active_client.as_client().ports(
        None,
        None,
        jack::PortFlags::IS_INPUT | jack::PortFlags::IS_PHYSICAL,
    );
    if config.auto_connect == AutoConnect::Playback {
        for (plugin_port, playback_port) in plugin_audio_output_ports
            .iter()
            .zip(audio_playback_ports.iter())
        {
            match active_client
                .as_client()
                .connect_ports_by_name(plugin_port.as_str(), playback_port.as_str())
            {
                Ok(()) => info!(
                    "Automatically connected port {} to {}.",
                    plugin_port, playback_port
                ),
                Err(err) => error!(
                    "Failed to connect port {} to {}: {:?}.",
                    plugin_port, playback_port, err
                ),
            };
        }
    }

    std::thread::spawn(move || loop {
        workers.run_workers();
        // Add some sleep to avoid busy looping.
        // Busy looping may lead to increased CPU usage.
        std::thread::sleep(std::time::Duration::from_millis(100));
    });
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
    fn new(
        world: &livi::World,
        plugin: livi::Plugin,
        client: &jack::Client,
    ) -> (Processor, Arc<livi::WorkerManager>) {
        let buffer_size = client.buffer_size() as usize;
        let worker_manager = Arc::new(livi::WorkerManager::default());
        let features = world.build_features(livi::FeaturesBuilder {
            min_block_length: buffer_size,
            max_block_length: buffer_size,
            worker_manager: worker_manager.clone(),
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
        (
            Processor {
                plugin: plugin_instance,
                midi_urid: features.midi_urid(),
                audio_inputs,
                audio_outputs,
                event_inputs,
                event_outputs,
                cv_inputs,
                cv_outputs,
            },
            worker_manager,
        )
    }

    fn output_audio_ports(&self) -> Vec<String> {
        self.audio_outputs
            .iter()
            .map(|p| p.name().unwrap())
            .collect()
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

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
enum AutoConnect {
    None,
    Playback,
}

impl FromStr for AutoConnect {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if s.eq_ignore_ascii_case("none") {
            return Ok(AutoConnect::None);
        }
        if s.eq_ignore_ascii_case("playback") {
            return Ok(AutoConnect::Playback);
        }
        Err(format!("{} not recognized", s))
    }
}
