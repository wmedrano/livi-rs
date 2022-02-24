//! livi is a library for hosting LV2 plugins in Rust.
//! ```
//! let world = livi::World::new();
//! const SAMPLE_RATE: f64 = 44100.0;
//! let worker_manager = std::sync::Arc::new(livi::WorkerManager::default());
//! let features = world.build_features(livi::FeaturesBuilder {
//! min_block_length: 1,
//! max_block_length: 4096,
//! worker_manager: worker_manager.clone(),
//! });
//! let plugin = world
//! // This is the URI for mda EPiano. You can use the `lv2ls` command line
//! // utility to see all available LV2 plugins.
//! .plugin_by_uri("http://drobilla.net/plugins/mda/EPiano")
//! .expect("Plugin not found.");
//! let mut instance = unsafe {
//! plugin
//! .instantiate(features.clone(), SAMPLE_RATE)
//! .expect("Could not instantiate plugin.")
//! };
//!
//! // Where midi events will be read from.
//! let input = {
//! let mut s = livi::event::LV2AtomSequence::new(&features, 1024);
//! let play_note_data = [0x90, 0x40, 0x7f];
//! s.push_midi_event::<3>(1, features.midi_urid(), &play_note_data)
//! .unwrap();
//! s
//! };
//!
//! // Where parameters can be set. We initialize to the plugin's default values.
//! let params: Vec<f32> = plugin
//! .ports_with_type(livi::PortType::ControlInput)
//! .map(|p| p.default_value)
//! .collect();
//! // This is where the audio data will be stored.
//! let mut outputs = [
//! vec![0.0; features.max_block_length()], // For mda EPiano, this is the left channel.
//! vec![0.0; features.max_block_length()], // For mda EPiano, this is the right channel.
//! ];
//!
//! // Set up the port configuration and run the plugin!
//! // The results will be stored in `outputs`.
//! let ports = livi::EmptyPortConnections::new()
//! .with_atom_sequence_inputs(std::iter::once(&input))
//! .with_audio_outputs(outputs.iter_mut().map(|output| output.as_mut_slice()));
//! unsafe { instance.run(features.max_block_length(), ports).unwrap() };
//!
//! // Plugins may push asynchronous works to the worker. When operating in
//! // Realtime, `run_workers` should be run in a separate thread.
//! worker_manager.run_workers();
//! ```
use log::{debug, error, info, warn};
use std::sync::Arc;

pub use features::worker::{Worker, WorkerManager};
pub use features::{Features, FeaturesBuilder};
pub use plugin::{Instance, Plugin};
pub use port::{EmptyPortConnections, Port, PortConnections, PortCounts, PortIndex, PortType};

/// Contains all the error types for the `livi` crate.
pub mod error;
/// Contains utility for dealing with `LV2` events.
pub mod event;
mod features;
mod plugin;
mod port;

/// Contains all plugins.
pub struct World {
    livi_plugins: Vec<Plugin>,
}

impl World {
    /// Create a new world that includes all plugins that are found and are
    /// supported.  Plugins that are not supported will be listed with a `warn!`
    /// message.
    #[must_use]
    pub fn new() -> World {
        World::with_plugin_predicate(|_| true)
    }

    /// Creates a new world that includes only a single
    /// plugin specified by bundle_uri.
    /// bundle_uri must be a fully qualified URI to the bundle directory,
    /// with the trailing slash, eg file:///usr/lib/lv2/foo.lv2/.
    pub fn with_load_bundle(bundle_uri: &str) -> World {
        let world = lilv::World::new();
        let uri = world.new_uri(bundle_uri);
        world.load_bundle(&uri);
        let common_uris = Arc::new(CommonUris::new(&world));
        let plugins: Vec<Plugin> = world
            .plugins()
            .into_iter()
            .map(|p| Plugin::from_raw(p, common_uris.clone()))
            .collect();

        World {
            livi_plugins: plugins,
        }
    }

    /// Creates a new world that includes all plugins that are found and return
    /// `true` for `predicate.
    #[must_use]
    pub fn with_plugin_predicate<P>(predicate: P) -> World
    where
        P: Fn(&Plugin) -> bool,
    {
        let world = lilv::World::with_load_all();
        let common_uris = Arc::new(CommonUris::new(&world));
        let supported_features = crate::Features::supported_features();
        info!(
            "Creating World with supported features {:?}",
            supported_features
        );
        let plugins: Vec<Plugin> = world
            .plugins()
            .into_iter()
            .filter(|p| {
                let is_supported = p
                    .required_features()
                    .into_iter()
                    .all(|f| supported_features.contains(f.as_uri().unwrap_or("")));
                if !is_supported {
                    warn!(
                        "Plugin {} requires unsupported features: {:?}",
                        p.uri().as_uri().unwrap_or("BAD_URI"),
                        p.required_features()
                    );
                }
                is_supported
            })
            .filter(|p| {
                if p.name().as_str().is_none() {
                    error!("Plugin {:?} did not return a string name.", p);
                    return false;
                }
                if p.uri().as_str().is_none() {
                    error!("Plugin {:?} did not return a valid uri.", p);
                    return false;
                }
                true
            })
            .filter(|p| {
                for port in p.iter_ports() {
                    for class in port.classes() {
                        if class != common_uris.input_port_uri
                            && class != common_uris.output_port_uri
                            && class != common_uris.audio_port_uri
                            && class != common_uris.control_port_uri
                            && class != common_uris.atom_port_uri
                            && class != common_uris.cv_port_uri
                        {
                            error!("Port class {:?} is not supported.", class);
                            return false;
                        }
                    }
                    if !port.is_a(&common_uris.input_port_uri)
                        && !port.is_a(&common_uris.output_port_uri)
                    {
                        error!(
                            "Port {:?} for plugin {} is neither an input or output.",
                            port,
                            p.uri().as_str().unwrap_or("BAD_URI")
                        );
                        return false;
                    }
                    if !port.is_a(&common_uris.audio_port_uri) && !port.is_a(&common_uris.control_port_uri) && !port.is_a(&common_uris.atom_port_uri) && !port.is_a(&common_uris.cv_port_uri) {
                        error!(
                            "Port {:?}for plugin {} not a recognized data type. Supported types are Audio and Control", port, p.uri().as_str().unwrap_or("BAD_URI")
                        );
                        return false;
                    }
                }
                true
            })
            .map(|p| Plugin::from_raw(p, common_uris.clone()))
            .filter(|p| {
                let keep = predicate(p);
                if !keep {
                    debug!("Ignoring plugin {} due to predicate.", p.uri());
                }
                keep
            })
            .inspect(|p| info!("Found plugin {}: {}", p.name(), p.uri()))
            .collect();
        World {
            livi_plugins: plugins,
        }
    }

    /// Iterate through all plugins.
    pub fn iter_plugins(&self) -> impl '_ + ExactSizeIterator + Iterator<Item = Plugin> {
        self.livi_plugins.iter().cloned()
    }

    /// Return the plugin given a URI or `None` if it does not exist.
    #[must_use]
    pub fn plugin_by_uri(&self, uri: &str) -> Option<Plugin> {
        self.iter_plugins().find(|p| p.uri() == uri)
    }

    /// Initialize the block length. This is the minimum and maximum number of
    /// samples that are processed per `run` method. This must be called before
    /// any plugins are instantiated and may only be called once.
    ///
    /// # Errors
    /// Returns an error if the block lengths are invalid.
    pub fn build_features(&self, builder: crate::features::FeaturesBuilder) -> Arc<Features> {
        builder.build(self)
    }
}

impl Default for World {
    fn default() -> Self {
        Self::new()
    }
}

struct CommonUris {
    input_port_uri: lilv::node::Node,
    output_port_uri: lilv::node::Node,
    control_port_uri: lilv::node::Node,
    audio_port_uri: lilv::node::Node,
    atom_port_uri: lilv::node::Node,
    cv_port_uri: lilv::node::Node,
    worker_schedule_feature_uri: lilv::node::Node,
}

impl CommonUris {
    fn new(world: &lilv::World) -> CommonUris {
        CommonUris {
            input_port_uri: world.new_uri("http://lv2plug.in/ns/lv2core#InputPort"),
            output_port_uri: world.new_uri("http://lv2plug.in/ns/lv2core#OutputPort"),
            control_port_uri: world.new_uri("http://lv2plug.in/ns/lv2core#ControlPort"),
            audio_port_uri: world.new_uri("http://lv2plug.in/ns/lv2core#AudioPort"),
            atom_port_uri: world.new_uri("http://lv2plug.in/ns/ext/atom#AtomPort"),
            cv_port_uri: world.new_uri("http://lv2plug.in/ns/lv2core#CVPort"),
            worker_schedule_feature_uri: world.new_uri("http://lv2plug.in/ns/ext/worker#schedule"),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use crate::event::LV2AtomSequence;

    use super::*;

    const MIN_BLOCK_SIZE: usize = 1;
    const MAX_BLOCK_SIZE: usize = 256;
    const SAMPLE_RATE: f64 = 44100.0;

    #[test]
    fn test_midi_urid_ok() {
        let world = World::new();
        let features = world.build_features(crate::features::FeaturesBuilder {
            min_block_length: MIN_BLOCK_SIZE,
            max_block_length: MAX_BLOCK_SIZE,
            worker_manager: Default::default(),
        });
        assert!(features.midi_urid() > 0, "midi urid is not valid");
    }

    #[test]
    fn test_all() {
        let world = World::new();
        let block_size = 64;
        let features = world.build_features(crate::features::FeaturesBuilder {
            min_block_length: block_size,
            max_block_length: block_size,
            worker_manager: Default::default(),
        });
        for plugin in world.iter_plugins() {
            println!("Running plugin: {}", plugin.uri());
            let port_counts = *plugin.port_counts();
            let audio_in = vec![0.0; port_counts.audio_inputs * block_size];
            let mut audio_out = vec![0.0; port_counts.audio_outputs * block_size];
            let cv_in = vec![0.0; port_counts.cv_inputs * block_size];
            let mut cv_out = vec![0.0; port_counts.cv_outputs * block_size];
            let play_note_data = [0x90, 0x40, 0x7f];
            let release_note_data = [0x80, 0x40, 0x00];
            let input_events_features = features.clone();
            let input_events = (0..port_counts.atom_sequence_inputs)
                .map(|_| {
                    let mut seq = LV2AtomSequence::new(&input_events_features, 1024);
                    seq.push_midi_event::<3>(4, input_events_features.midi_urid(), &play_note_data)
                        .unwrap();
                    seq.push_midi_event::<3>(
                        60,
                        input_events_features.midi_urid(),
                        &release_note_data,
                    )
                    .unwrap();
                    seq
                })
                .collect::<Vec<_>>();
            let output_events_features = features.clone();
            let mut output_events = (0..port_counts.atom_sequence_outputs)
                .map(|_| LV2AtomSequence::new(&output_events_features, 1024))
                .collect::<Vec<_>>();
            let mut instance = unsafe {
                plugin
                    .instantiate(features.clone(), SAMPLE_RATE)
                    .expect("Could not instantiate plugin.")
            };
            let ports = PortConnections {
                audio_inputs: audio_in
                    .chunks_exact(block_size)
                    .take(port_counts.audio_inputs),
                audio_outputs: audio_out
                    .chunks_exact_mut(block_size)
                    .take(port_counts.audio_outputs),
                atom_sequence_inputs: input_events.iter(),
                atom_sequence_outputs: output_events.iter_mut(),
                cv_inputs: cv_in.chunks_exact(block_size).take(port_counts.cv_inputs),
                cv_outputs: cv_out
                    .chunks_exact_mut(block_size)
                    .take(port_counts.cv_outputs),
            };
            unsafe {
                assert_eq!(
                    instance.run(block_size, ports),
                    Ok(()),
                    "Failed on run {} with plugin: {}",
                    block_size,
                    plugin.uri(),
                )
            };
        }
    }

    #[test]
    fn test_mda_epiano() {
        let world = World::new();
        let plugin = world
            // Electric Piano instrument.
            .plugin_by_uri("http://drobilla.net/plugins/mda/EPiano")
            .expect("Plugin not found.");
        assert_eq!(
            *plugin.port_counts(),
            PortCounts {
                control_inputs: 12,
                control_outputs: 0,
                audio_inputs: 0,
                audio_outputs: 2,
                atom_sequence_inputs: 1,
                atom_sequence_outputs: 0,
                cv_inputs: 0,
                cv_outputs: 0,
            }
        );
        let features = world.build_features(FeaturesBuilder {
            min_block_length: MIN_BLOCK_SIZE,
            max_block_length: MAX_BLOCK_SIZE,
            worker_manager: Default::default(),
        });
        let mut instance = unsafe {
            plugin
                .instantiate(features.clone(), SAMPLE_RATE)
                .expect("Could not instantiate plugin.")
        };
        assert_eq!(
            instance.port_counts(),
            PortCounts {
                control_inputs: 12,
                control_outputs: 0,
                audio_inputs: 0,
                audio_outputs: 2,
                atom_sequence_inputs: 1,
                atom_sequence_outputs: 0,
                cv_inputs: 0,
                cv_outputs: 0,
            }
        );
        let input = {
            let mut s = LV2AtomSequence::new(&features, 1024);
            let play_note_data = [0x90, 0x40, 0x7f];
            s.push_midi_event::<3>(1, features.midi_urid(), &play_note_data)
                .unwrap();
            s
        };
        let mut outputs = [vec![0.0; MAX_BLOCK_SIZE], vec![0.0; MAX_BLOCK_SIZE]];
        for block_size in MIN_BLOCK_SIZE..MAX_BLOCK_SIZE {
            let ports = EmptyPortConnections::new()
                .with_atom_sequence_inputs(std::iter::once(&input))
                .with_audio_outputs(outputs.iter_mut().map(|output| output.as_mut_slice()));
            unsafe { instance.run(block_size, ports).unwrap() };
        }
        for output in outputs.iter_mut() {
            assert!(
                output.iter().map(|x| x.abs()).sum::<f32>() > 0.0,
                "No signal was output."
            );
        }
    }

    #[test]
    fn test_fifths() {
        let world = World::new();
        let block_size = 128;
        let plugin = world
            // Takes a midi and adds the fifth of every note.
            .plugin_by_uri("http://lv2plug.in/plugins/eg-fifths")
            .expect("Plugin not found.");
        assert_eq!(
            *plugin.port_counts(),
            PortCounts {
                control_inputs: 0,
                control_outputs: 0,
                audio_inputs: 0,
                audio_outputs: 0,
                atom_sequence_inputs: 1,
                atom_sequence_outputs: 1,
                cv_inputs: 0,
                cv_outputs: 0,
            }
        );
        let features = world.build_features(FeaturesBuilder {
            min_block_length: block_size,
            max_block_length: block_size,
            worker_manager: Default::default(),
        });
        let mut instance = unsafe {
            plugin
                .instantiate(features.clone(), SAMPLE_RATE)
                .expect("Could not instantiate plugin.")
        };

        let play_c3 = [0x90, 0x30, 0x7f];
        let play_c4 = [0x90, 0x3C, 0x7f];
        let play_g4 = [0x90, 0x43, 0x7f];
        let release_c4 = [0x80, 0x3C, 0x00];
        let release_g4 = [0x80, 0x43, 0x00];

        let mut input = LV2AtomSequence::new(&features, 1024);
        input
            .push_midi_event::<3>(1, features.midi_urid(), &play_c4)
            .unwrap();
        input
            .push_midi_event::<3>(10, features.midi_urid(), &release_c4)
            .unwrap();

        let mut output = LV2AtomSequence::new(&features, 1024);
        // This note should be cleared from the output by the LV2 plugin.
        output
            .push_midi_event::<3>(1, features.midi_urid(), &play_c3)
            .unwrap();

        for _ in 0..10 {
            let ports = EmptyPortConnections::new()
                .with_atom_sequence_inputs(std::iter::once(&input))
                .with_atom_sequence_outputs(std::iter::once(&mut output));
            unsafe { instance.run(block_size, ports).unwrap() };
        }

        let got = output
            .iter()
            .map(|e| (e.event.time_in_frames, e.data))
            .collect::<Vec<_>>();
        let expected: Vec<(i64, &[u8])> = vec![
            (1, &play_c4),     // Original input.
            (1, &play_g4),     // Fifth added.
            (10, &release_c4), // Original input.
            (10, &release_g4), // Fifth added.
        ];
        assert_eq!(got, expected);
    }

    #[test]
    fn test_with_filter() {
        let uri = "http://drobilla.net/plugins/mda/EPiano";

        // EPiano only.
        let world = World::with_plugin_predicate(|p| p.uri() == uri);
        assert!(world.plugin_by_uri(uri).is_some());
        assert_eq!(world.iter_plugins().count(), 1);

        // No EPiano.
        assert!(World::with_plugin_predicate(|p| p.uri() != uri)
            .plugin_by_uri(uri)
            .is_none());

        // Empty.
        assert_eq!(
            World::with_plugin_predicate(|_| false)
                .iter_plugins()
                .count(),
            0
        );
    }

    #[test]
    fn test_supported_features() {
        let supported_features = Features::supported_features();

        assert!(supported_features.contains("http://lv2plug.in/ns/ext/urid#map"));

        let want = HashSet::from([
            "http://lv2plug.in/ns/ext/urid#map",
            "http://lv2plug.in/ns/ext/urid#unmap",
            "http://lv2plug.in/ns/ext/options#options",
            "http://lv2plug.in/ns/ext/buf-size#boundedBlockLength",
            "http://lv2plug.in/ns/ext/worker#schedule",
        ]);
        assert_eq!(want, supported_features);
    }
}
