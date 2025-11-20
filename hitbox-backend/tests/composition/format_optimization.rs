use hitbox_backend::serializer::{FormatExt, JsonFormat, BincodeFormat};
use hitbox_backend::composition::CompositionFormat;
use serde::{Serialize, Deserialize};
use std::sync::Arc;

#[derive(Debug, Serialize, Deserialize, PartialEq)]
struct TestData {
    id: u32,
    name: String,
    values: Vec<i32>,
}

impl TestData {
    fn large() -> Self {
        TestData {
            id: 42,
            name: "test".repeat(100),
            values: (0..1000).collect(),
        }
    }
}

#[test]
fn test_same_format_optimization() {
    // Both L1 and L2 use JSON
    let composition = CompositionFormat::new(
        Arc::new(JsonFormat),
        Arc::new(JsonFormat),
    );

    let data = TestData::large();
    let serialized = composition.serialize(&data).unwrap();

    // Deserialize to verify it works
    let deserialized: TestData = composition.deserialize(&serialized).unwrap();
    assert_eq!(data, deserialized);

    println!("Same format test passed");
}

#[test]
fn test_different_formats() {
    // L1 uses JSON, L2 uses Bincode
    let composition = CompositionFormat::new(
        Arc::new(JsonFormat),
        Arc::new(BincodeFormat),
    );

    let data = TestData::large();
    let serialized = composition.serialize(&data).unwrap();

    // Deserialize to verify it works
    let deserialized: TestData = composition.deserialize(&serialized).unwrap();
    assert_eq!(data, deserialized);

    println!("Different formats test passed");
}

#[test]
fn test_serialization_size_comparison() {
    let data = TestData::large();

    // Single JSON serialization
    let json_format = JsonFormat;
    let json_size = json_format.serialize(&data).unwrap().len();

    // CompositionFormat with same formats (should be ~2x JSON + small overhead)
    let composition_same = CompositionFormat::new(
        Arc::new(JsonFormat),
        Arc::new(JsonFormat),
    );
    let composition_same_size = composition_same.serialize(&data).unwrap().len();

    // CompositionFormat with different formats
    let composition_diff = CompositionFormat::new(
        Arc::new(JsonFormat),
        Arc::new(BincodeFormat),
    );
    let composition_diff_size = composition_diff.serialize(&data).unwrap().len();

    println!("JSON size: {} bytes", json_size);
    println!("Composition (same format) size: {} bytes", composition_same_size);
    println!("Composition (different formats) size: {} bytes", composition_diff_size);

    // Composition should be roughly 2x the single format size (plus small bincode overhead)
    // Allow some margin for bincode envelope
    assert!(composition_same_size < json_size * 2 + 100,
        "Same format composition size should be ~2x single format");

    assert!(composition_diff_size > json_size,
        "Different formats should be larger than single JSON");
}
