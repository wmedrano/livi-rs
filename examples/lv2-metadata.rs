use log::error;
use structopt::StructOpt;

/// The configuration for the backend.
#[derive(StructOpt, Debug)]
struct Configuration {
    /// The uri of the plugin to view metadata for.
    #[structopt(long = "plugin-uri", default_value = "")]
    plugin_uri: String,
}

fn main() {
    let config = Configuration::from_args();
    env_logger::builder().init();

    let world = livi::World::new();
    let plugin = world.plugin_by_uri(&config.plugin_uri);
    match plugin {
        Some(plugin) => println!("{plugin:#?}"),
        None => {
            error!("Could not find --plugin-uri {:?}", &config.plugin_uri);
            let plugin_uris = world.iter_plugins().map(|p| p.uri()).collect::<Vec<_>>();
            println!("Plugins: {plugin_uris:?}");
        }
    }
}
