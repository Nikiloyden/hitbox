use cucumber::writer::{Basic, JUnit};
use cucumber::writer::{Verbosity, basic::Coloring};
use cucumber::{World, WriterExt};
use hitbox_test::core::HitboxWorld;
use std::fs::{File, create_dir_all};
use std::io::stdout;
use std::path::PathBuf;

#[tokio::main]
pub async fn main() {
    // Use workspace root target directory
    // CARGO_MANIFEST_DIR points to hitbox-test, so we go up one level to workspace root
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let workspace_root = manifest_dir
        .parent()
        .expect("Failed to find workspace root");
    let target_dir = workspace_root.join("target");

    create_dir_all(&target_dir).expect("Failed to create target directory");

    let junit_path = target_dir.join("cucumber-junit.xml");
    let file = File::create(&junit_path).expect("Failed to create JUnit XML file");

    HitboxWorld::cucumber()
        .max_concurrent_scenarios(None) // Remove any concurrency limits
        .with_writer(
            Basic::new(stdout(), Coloring::Auto, Verbosity::Default)
                .summarized()
                .tee(JUnit::for_tee(file, 0))
                .normalized(),
        )
        .filter_run("tests/features", |_feature, _rule, scenario| {
            !scenario.tags.iter().any(|tag| tag == "allow.failed")
        })
        .await;
}
