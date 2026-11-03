//! A reqwest-backed client generated from `examples/pets.openapi.yaml`.
//!
//! Start the server first with `cargo run --example server`, then run
//! `cargo run --example reqwest_client`.

// ANCHOR: macro
#[openapi_trait::client("examples/pets.openapi.yaml")]
pub mod pets {}
// ANCHOR_END: macro

// ANCHOR: carrier
/// Holds everything the generated client implementation needs.
#[derive(Clone, openapi_trait::ReqwestClient)]
struct PetsHttpClient {
    /// The HTTP client used for every request.
    #[openapi_trait(client)]
    http: openapi_trait::reqwest::Client,
    /// Base URL that operation paths are appended to.
    #[openapi_trait(base_url)]
    base_url: String,
    /// Credentials for the spec's security schemes.
    #[openapi_trait(auth)]
    auth: pets::PetsAuthState,
}
// ANCHOR_END: carrier

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // ANCHOR: usage
    use pets::PetsClient as _;
    use pets::PetsClientAuth as _;

    let client = PetsHttpClient {
        http: openapi_trait::reqwest::Client::new(),
        base_url: "http://127.0.0.1:3000".into(),
        auth: pets::PetsAuthState::default(),
    }
    .with_bearer_auth("secret");

    let created = client
        .create_pet(
            pets::CreatePetRequest {
                body: pets::NewPet {
                    name: "doggie".into(),
                    status: Some(pets::PetStatus::Available),
                },
            },
            None,
        )
        .await?;
    println!("created: {created:?}");

    let all = client
        .list_pets(pets::ListPetsRequest { limit: Some(10) }, None)
        .await?;
    println!("listed: {all:?}");
    // ANCHOR_END: usage

    // ANCHOR: options
    let pet = client
        .get_pet(
            pets::GetPetRequest { pet_id: 1 },
            Some(
                openapi_trait::RequestOptions::new()
                    .bearer_auth("another-token")
                    .header("X-Request-Id", "abc-789"),
            ),
        )
        .await?;
    // ANCHOR_END: options
    println!("fetched: {pet:?}");

    Ok(())
}
