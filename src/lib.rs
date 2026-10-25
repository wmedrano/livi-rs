//! livi is a library for hosting LV2 plugins in Rust.
//! ```
//! let world = livi::World::new();
//! const SAMPLE_RATE: f64 = 44100.0;
//! let features = world.try_build_features(livi::FeaturesBuilder::default()).expect("invalid feature block bounds");
//! let plugin = world
//!     // This is the URI for mda EPiano. You can use the `lv2ls` command line
//!     // utility to see all available LV2 plugins.
//!     .plugin_by_uri("http://drobilla.net/plugins/mda/EPiano")
//!     .expect("Plugin not found.");
//! let mut instance = unsafe {
//!     plugin
//!         .instantiate(features.clone(), SAMPLE_RATE)
//!         .expect("Could not instantiate plugin.")
//! };
//!
//! // Where midi events will be read from.
//! let input = {
//!     let mut s = livi::event::LV2AtomSequence::new(&features, 1024);
//!     let play_note_data = [0x90, 0x40, 0x7f];
//!     s.push_midi_event::<3>(1, features.midi_urid(), &play_note_data)
//!         .unwrap();
//!     s
//! };
//!
//! // This is where the audio data will be stored.
//! let mut outputs = [
//!     vec![0.0; features.max_block_length()], // For mda EPiano, this is the left channel.
//!     vec![0.0; features.max_block_length()], // For mda EPiano, this is the right channel.
//! ];
//!
//! // Set up the port configuration and run the plugin!
//! // The results will be stored in `outputs`.
//! let ports = livi::EmptyPortConnections::new()
//!     .with_atom_sequence_inputs(std::iter::once(&input))
//!     .with_audio_outputs(outputs.iter_mut().map(|output| output.as_mut_slice()));
//! unsafe { instance.run(features.max_block_length(), ports).unwrap() };
//! ```
use error::StateError;
use log::{debug, error, info, warn};
use std::path::Path;
use std::sync::Arc;

pub use features::worker::{Worker, WorkerManager};
pub use features::{Features, FeaturesBuilder};
pub use plugin::{Instance, Plugin};
pub use port::{EmptyPortConnections, Port, PortConnections, PortCounts, PortIndex, PortType};
pub use state::State;

/// The underlying `lilv` library.
pub use lilv;

mod class_utils;
/// Contains all the error types for the `livi` crate.
pub mod error;
/// Contains utility for dealing with `LV2` events.
pub mod event;
mod features;
mod plugin;
mod port;
/// Contains support for saving and restoring plugin state.
pub mod state;

/// Contains all plugins.
pub struct World {
    // TODO: Remove the Arc once lilv::World implements Clone. lilv::World is
    // already reference counted internally.
    world: Arc<lilv::World>,
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
        let world = Arc::new(lilv::World::new());
        let uri = world.new_uri(bundle_uri);
        world.load_bundle(&uri);
        fn any_plugin_pred(_: &Plugin) -> bool {
            true
        }
        Self::from_loaded_world(world, any_plugin_pred)
    }

    /// Get the underlying lilv world.
    pub fn raw(&self) -> &lilv::World {
        &self.world
    }

    /// Creates a new world that includes all plugins that are found and return
    /// `true` for `predicate.
    #[must_use]
    pub fn with_plugin_predicate<P>(predicate: P) -> World
    where
        P: Fn(&Plugin) -> bool,
    {
        let world = Arc::new(lilv::World::with_load_all());
        Self::from_loaded_world(world, predicate)
    }

    // Apply the same validation before constructing a Plugin, regardless of
    // how the underlying world was populated.
    fn from_loaded_world<P>(world: Arc<lilv::World>, plugin_predicate: P) -> World
    where
        P: Fn(&Plugin) -> bool,
    {
        let common_uris = Arc::new(CommonUris::new(&world));
        let supported_features = crate::Features::supported_features();
        info!(
            "Creating World with supported features {:?}",
            supported_features
        );
        let class_to_parent = class_utils::make_class_to_parent_map(&world);
        // TODO: Collect list of warnings per plugin instead of just displaying one.
        let plugins: Vec<Plugin> = world
            .plugins()
            .into_iter()
            .filter(|p| {
                let unsupported_features: Vec<_> = p
                    .required_features()
                    .into_iter()
                    .filter(|f| !supported_features.contains(f.as_uri().unwrap_or("")))
                    .collect();
                let is_supported = unsupported_features.is_empty();
                if !is_supported {
                    warn!(
                        "Plugin {} requires unsupported features: {:?}",
                        p.uri().as_uri().unwrap_or("BAD_URI"),
                        unsupported_features
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
            .filter(|p| ports_supported(p, &common_uris))
            .map(|p| {
                let classes = class_utils::class_with_parents(&p.class(), &class_to_parent);
                Plugin::from_raw(p, world.clone(), common_uris.clone(), classes)
            })
            .filter(|p| {
                let keep = plugin_predicate(p);
                if !keep {
                    debug!("Ignoring plugin {} due to predicate.", p.uri());
                }
                keep
            })
            .inspect(|p| info!("Found plugin {}: {}", p.name(), p.uri()))
            .collect();
        World {
            world,
            livi_plugins: plugins,
        }
    }

    /// Iterate through all plugins.
    pub fn iter_plugins(&self) -> impl '_ + ExactSizeIterator<Item = Plugin> {
        self.livi_plugins.iter().cloned()
    }

    /// Return the plugin given a URI or `None` if it does not exist.
    #[must_use]
    pub fn plugin_by_uri(&self, uri: &str) -> Option<Plugin> {
        self.iter_plugins().find(|p| p.uri() == uri)
    }

    /// Build plugin features.
    ///
    /// # Panics
    /// Panics if the block bounds are invalid. Use `try_build_features` to
    /// receive a configuration error instead.
    #[deprecated(
        since = "0.9.0",
        note = "use try_build_features to handle invalid block bounds"
    )]
    pub fn build_features(&self, builder: crate::features::FeaturesBuilder) -> Arc<Features> {
        self.try_build_features(builder)
            .expect("invalid feature block bounds")
    }

    /// Build plugin features, returning an error for invalid block bounds.
    pub fn try_build_features(
        &self,
        builder: FeaturesBuilder,
    ) -> Result<Arc<Features>, error::FeatureError> {
        builder.try_build(self)
    }

    /// Load a state from a string, such as one produced by `State::to_string`.
    ///
    /// # Errors
    /// Returns an error if the state could not be loaded.
    pub fn state_from_string(
        &self,
        features: &Arc<Features>,
        state: &str,
    ) -> Result<State, StateError> {
        let mut map = features.urid_map.lv2_urid_map();
        let inner = self
            .world
            .new_state_from_string(&mut map, state)
            .ok_or(StateError::LoadFailed)?;
        Ok(State::new(inner, features.clone()))
    }

    /// Load a state from a Turtle file, such as one produced by `State::save`.
    /// If `uri` is `None`, then the subject of the state is taken to be the
    /// file itself.
    ///
    /// # Errors
    /// Returns an error if `path` is not valid UTF-8 or the state could not be
    /// loaded.
    pub fn state_from_file(
        &self,
        features: &Arc<Features>,
        uri: Option<&str>,
        path: &Path,
    ) -> Result<State, StateError> {
        let path = path.to_str().ok_or(StateError::InvalidPath)?;
        let mut map = features.urid_map.lv2_urid_map();
        let subject = uri.map(|uri| self.world.new_uri(uri));
        let inner = self
            .world
            .new_state_from_file(&mut map, subject.as_ref(), path)
            .ok_or(StateError::LoadFailed)?;
        Ok(State::new(inner, features.clone()))
    }

    /// Load a state, such as a preset, that has been loaded into the world. If
    /// `uri` is a plugin URI, then the default state of the plugin is loaded.
    ///
    /// # Errors
    /// Returns an error if the state could not be loaded.
    pub fn state_from_uri(&self, features: &Arc<Features>, uri: &str) -> Result<State, StateError> {
        let mut map = features.urid_map.lv2_urid_map();
        let inner = self
            .world
            .new_state(&mut map, &self.world.new_uri(uri))
            .ok_or(StateError::LoadFailed)?;
        Ok(State::new(inner, features.clone()))
    }
}

fn ports_supported(plugin: &lilv::plugin::Plugin, uris: &CommonUris) -> bool {
    let direction_classes = [&uris.input_port_uri, &uris.output_port_uri];
    let data_classes = [
        &uris.audio_port_uri,
        &uris.control_port_uri,
        &uris.atom_port_uri,
        &uris.cv_port_uri,
    ];

    plugin.iter_ports().all(|port| {
        for class in port.classes() {
            if !direction_classes.contains(&&class)
                && !data_classes.contains(&&class)
                && class != uris.mod_cv_port_uri
            {
                error!("Port class {:?} is not supported.", class);
                return false;
            }
        }
        if !direction_classes.iter().any(|class| port.is_a(class)) {
            error!(
                "Port {:?} for plugin {} is neither an input nor output.",
                port,
                plugin.uri().as_str().unwrap_or("BAD_URI")
            );
            return false;
        }
        if !data_classes.iter().any(|class| port.is_a(class)) {
            error!(
                "Port {:?} for plugin {} is not a recognized data type. \
                 Supported types are Audio, Control, Atom and CV",
                port,
                plugin.uri().as_str().unwrap_or("BAD_URI")
            );
            return false;
        }
        if port.is_a(&uris.atom_port_uri)
            && !port
                .value(&uris.atom_buffer_type_uri)
                .into_iter()
                .any(|value| value == uris.atom_sequence_uri)
        {
            warn!(
                "Plugin {:?} has an atom port without Sequence buffer support",
                plugin.uri()
            );
            return false;
        }
        true
    })
}

impl Default for World {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Debug for World {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("World")
            .field("livi_plugins", &self.livi_plugins)
            .finish()
    }
}

#[derive(Debug)]
struct CommonUris {
    input_port_uri: lilv::node::Node,
    output_port_uri: lilv::node::Node,
    control_port_uri: lilv::node::Node,
    audio_port_uri: lilv::node::Node,
    atom_port_uri: lilv::node::Node,
    atom_buffer_type_uri: lilv::node::Node,
    atom_sequence_uri: lilv::node::Node,
    cv_port_uri: lilv::node::Node,
    mod_cv_port_uri: lilv::node::Node,
    worker_schedule_feature_uri: lilv::node::Node,
    load_default_state_feature_uri: lilv::node::Node,
}

impl CommonUris {
    fn new(world: &lilv::World) -> CommonUris {
        CommonUris {
            input_port_uri: world.new_uri("http://lv2plug.in/ns/lv2core#InputPort"),
            output_port_uri: world.new_uri("http://lv2plug.in/ns/lv2core#OutputPort"),
            control_port_uri: world.new_uri("http://lv2plug.in/ns/lv2core#ControlPort"),
            audio_port_uri: world.new_uri("http://lv2plug.in/ns/lv2core#AudioPort"),
            atom_port_uri: world.new_uri("http://lv2plug.in/ns/ext/atom#AtomPort"),
            atom_buffer_type_uri: world.new_uri("http://lv2plug.in/ns/ext/atom#bufferType"),
            atom_sequence_uri: world.new_uri("http://lv2plug.in/ns/ext/atom#Sequence"),
            cv_port_uri: world.new_uri("http://lv2plug.in/ns/lv2core#CVPort"),
            mod_cv_port_uri: world.new_uri("http://moddevices.com/ns/mod#CVPort"),
            worker_schedule_feature_uri: world.new_uri("http://lv2plug.in/ns/ext/worker#schedule"),
            load_default_state_feature_uri: world
                .new_uri("http://lv2plug.in/ns/ext/state#loadDefaultState"),
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
    fn atom_ports_require_sequence_buffer_type() {
        for (declaration, supported) in [
            ("atom:bufferType atom:Float;", false),
            ("", false),
            ("atom:bufferType atom:Sequence;", true),
            ("atom:bufferType atom:Float, atom:Sequence;", true),
        ] {
            let fixture = crate::test_support::Fixture::new(&format!(
                r#"
                lv2:port [ a lv2:InputPort, atom:AtomPort; lv2:index 0;
                    lv2:symbol "atom"; lv2:name "Atom"; {declaration} ]
            "#
            ));
            let world = World::with_load_bundle(&fixture.bundle_uri);
            assert_eq!(
                world.plugin_by_uri("urn:livi:test").is_some(),
                supported,
                "{declaration}"
            );
        }
    }

    #[test]
    fn bundle_loading_rejects_unsupported_features() {
        let fixture =
            crate::test_support::Fixture::new("lv2:requiredFeature <urn:livi:unsupported>");
        assert_eq!(
            World::with_load_bundle(&fixture.bundle_uri)
                .iter_plugins()
                .count(),
            0
        );
    }

    #[test]
    fn bundle_loading_rejects_unsupported_ports() {
        let fixture = crate::test_support::Fixture::new(
            r#"
            lv2:port [ a lv2:InputPort, <http://lv2plug.in/ns/ext/event#EventPort>;
                lv2:index 0; lv2:symbol "event"; lv2:name "Event" ]
        "#,
        );
        assert_eq!(
            World::with_load_bundle(&fixture.bundle_uri)
                .iter_plugins()
                .count(),
            0
        );
    }

    #[test]
    fn bundle_loading_accepts_supported_plugins() {
        let fixture =
            crate::test_support::Fixture::new("lv2:optionalFeature <urn:livi:unsupported>");
        assert!(
            World::with_load_bundle(&fixture.bundle_uri)
                .plugin_by_uri("urn:livi:test")
                .is_some()
        );
    }

    #[test]
    fn test_midi_urid_ok() {
        let world = World::new();
        let features = world
            .try_build_features(crate::features::FeaturesBuilder {
                min_block_length: MIN_BLOCK_SIZE,
                max_block_length: MAX_BLOCK_SIZE,
            })
            .expect("invalid feature block bounds");
        assert!(features.midi_urid() > 0, "midi urid is not valid");
    }

    #[test]
    fn test_all() {
        let world = World::new();
        let block_size = 64;
        let features = world
            .try_build_features(crate::features::FeaturesBuilder {
                min_block_length: block_size,
                max_block_length: block_size,
            })
            .expect("invalid feature block bounds");
        for plugin in world.iter_plugins() {
            if plugin
                .uri()
                .starts_with("http://breakfastquay.com/rdf/lv2-rubberband")
            {
                continue;
            }
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
        let features = world
            .try_build_features(FeaturesBuilder {
                min_block_length: MIN_BLOCK_SIZE,
                max_block_length: MAX_BLOCK_SIZE,
            })
            .expect("invalid feature block bounds");
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
            .expect("Plugin http://lv2plug.in/plugins/eg-fifths not found.");
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
        let features = world
            .try_build_features(FeaturesBuilder {
                min_block_length: block_size,
                max_block_length: block_size,
            })
            .expect("invalid feature block bounds");
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
        assert!(
            World::with_plugin_predicate(|p| p.uri() != uri)
                .plugin_by_uri(uri)
                .is_none()
        );

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
            "http://lv2plug.in/ns/ext/state#loadDefaultState",
        ]);
        assert_eq!(want, supported_features);
    }
}

#[cfg(test)]
mod test_support;
