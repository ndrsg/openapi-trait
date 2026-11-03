//! Validating generated models against the spec's constraints.
//!
//! Run with `cargo run --example validation --features validation`.

#[openapi_trait::client("examples/pets.openapi.yaml")]
pub mod pets {}

// ANCHOR: validate
use openapi_trait::serde_valid::Validate as _;

fn main() {
    let ok = pets::NewPet {
        name: "doggie".into(),
        status: None,
    };
    assert!(ok.validate().is_ok());

    // `name` has `minLength: 1` in the spec.
    let empty = pets::NewPet {
        name: String::new(),
        status: None,
    };
    let err = empty.validate().unwrap_err();
    println!("{err}");
}
// ANCHOR_END: validate
