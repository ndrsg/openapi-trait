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
        _req: pets::list_pets::Request,
        _options: Option<openapi_trait::RequestOptions>,
    ) -> Result<pets::list_pets::Response, Self::Error> {
        Ok(pets::list_pets::Response::Status200(vec![]))
    }

    async fn create_pet(
        &self,
        req: pets::create_pet::Request,
        _options: Option<openapi_trait::RequestOptions>,
    ) -> Result<pets::create_pet::Response, Self::Error> {
        Ok(pets::create_pet::Response::Status201(pets::Pet {
            id: 1,
            name: req.body.name,
            status: req.body.status,
        }))
    }

    async fn get_pet(
        &self,
        _req: pets::get_pet::Request,
        _options: Option<openapi_trait::RequestOptions>,
    ) -> Result<pets::get_pet::Response, Self::Error> {
        Ok(pets::get_pet::Response::Status404)
    }
}
// ANCHOR_END: impl

// ANCHOR: generic
/// Code written against the trait works with any implementation.
async fn pet_name<C: pets::PetsClient>(client: &C, pet_id: i64) -> Option<String> {
    match client
        .get_pet(pets::get_pet::Request { pet_id }, None)
        .await
    {
        Ok(pets::get_pet::Response::Status200(pet)) => Some(pet.name),
        _ => None,
    }
}
// ANCHOR_END: generic

#[tokio::main]
async fn main() {
    println!("{:?}", pet_name(&FakePets, 1).await);
}
