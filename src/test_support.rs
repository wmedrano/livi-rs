// Bundled test plugin: keeps its bundle alive for as long as the test needs it.
pub(crate) struct Fixture {
    _directory: tempfile::TempDir,
    pub(crate) bundle_uri: String,
}

impl Fixture {
    pub(crate) fn new(metadata: &str) -> Self {
        let directory = tempfile::tempdir().unwrap();
        let source = directory.path().join("probe.c");
        std::fs::write(&source, include_str!("../tests/fixtures/probe.c")).unwrap();
        let status = std::process::Command::new("cc")
            .args(["-shared", "-fPIC", "-o"])
            .arg(directory.path().join("probe.so"))
            .arg(source)
            .status()
            .expect("test fixtures require a C compiler and LV2 development headers");
        assert!(status.success());
        std::fs::write(
            directory.path().join("manifest.ttl"),
            r#"
@prefix lv2: <http://lv2plug.in/ns/lv2core#> .
@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .
<urn:livi:test> a lv2:Plugin; lv2:binary <probe.so>; rdfs:seeAlso <plugin.ttl> .
"#,
        )
        .unwrap();
        std::fs::write(
            directory.path().join("plugin.ttl"),
            format!(
                r#"
@prefix lv2: <http://lv2plug.in/ns/lv2core#> .
@prefix doap: <http://usefulinc.com/ns/doap#> .
@prefix atom: <http://lv2plug.in/ns/ext/atom#> .
<urn:livi:test> a lv2:Plugin; doap:name "Livi regression probe"; {metadata} .
"#
            ),
        )
        .unwrap();
        Self {
            bundle_uri: format!("file://{}/", directory.path().display()),
            _directory: directory,
        }
    }
}
