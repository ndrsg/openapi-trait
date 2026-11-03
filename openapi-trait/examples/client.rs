//! A hand-written implementation of the transport-agnostic client trait.
//!
//! Useful as a test double, or to back the trait with a transport other than
//! HTTP. Run with `cargo run --example client`.

#[openapi_trait::client("examples/pets.openapi.yaml")]
pub mod pets {}

// ANCHOR: impl
/// An in-memory stand-in for the real API.
struct FakePets;

impl pets::PetsClient for FakePets {
    type Error = std::convert::Infallible;

    async fn list_pets(
        &self,
        _req: pets::ListPetsRequest,
        _options: Option<openapi_trait::RequestOptions>,
    ) -> Result<pets::ListPetsResponse, Self::Error> {
        Ok(pets::ListPetsResponse::Status200(vec![]))
    }

    async fn create_pet(
        &self,
        req: pets::CreatePetRequest,
        _options: Option<openapi_trait::RequestOptions>,
    ) -> Result<pets::CreatePetResponse, Self::Error> {
        Ok(pets::CreatePetResponse::Status201(pets::Pet {
            id: 1,
            name: req.body.name,
            status: req.body.status,
        }))
    }

    async fn get_pet(
        &self,
        _req: pets::GetPetRequest,
        _options: Option<openapi_trait::RequestOptions>,
    ) -> Result<pets::GetPetResponse, Self::Error> {
        Ok(pets::GetPetResponse::Status404)
    }
}
// ANCHOR_END: impl

// ANCHOR: generic
/// Code written against the trait works with any implementation.
async fn pet_name<C: pets::PetsClient>(client: &C, pet_id: i64) -> Option<String> {
    match client.get_pet(pets::GetPetRequest { pet_id }, None).await {
        Ok(pets::GetPetResponse::Status200(pet)) => Some(pet.name),
        _ => None,
    }
}
// ANCHOR_END: generic

#[tokio::main]
async fn main() {
    println!("{:?}", pet_name(&FakePets, 1).await);
}
