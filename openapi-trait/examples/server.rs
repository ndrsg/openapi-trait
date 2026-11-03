//! An axum server generated from `examples/pets.openapi.yaml`.
//!
//! Run with `cargo run --example server`, then try
//! `curl localhost:3000/pets` or `cargo run --example reqwest_client`.

// ANCHOR: macro
#[openapi_trait::axum("examples/pets.openapi.yaml")]
pub mod pets {}
// ANCHOR_END: macro

use std::sync::{Arc, Mutex};

// ANCHOR: state
/// Shared application state, handed to every handler via `axum::extract::State`.
#[derive(Clone, Default)]
struct AppState {
    /// In-memory pet storage.
    pets: Arc<Mutex<Vec<pets::Pet>>>,
}
// ANCHOR_END: state

// ANCHOR: impl
/// The type that implements the generated API trait.
#[derive(Clone)]
struct PetServer;

impl pets::PetsApi<AppState> for PetServer {
    type Error = pets::NotImplemented;

    async fn list_pets(
        &self,
        req: pets::list_pets::Request,
        state: axum::extract::State<AppState>,
        _headers: axum::http::HeaderMap,
    ) -> Result<pets::list_pets::Response, Self::Error> {
        let store = state.pets.lock().unwrap();
        let limit = req
            .limit
            .map_or(usize::MAX, |n| usize::try_from(n).unwrap_or(0));
        Ok(pets::list_pets::Response::Status200(
            store.iter().take(limit).cloned().collect(),
        ))
    }

    async fn create_pet(
        &self,
        req: pets::create_pet::Request,
        auth: pets::BearerAuth,
        state: axum::extract::State<AppState>,
        _headers: axum::http::HeaderMap,
    ) -> Result<pets::create_pet::Response, Self::Error> {
        // The macro only extracts the token; checking it is up to you.
        if auth.0 != "secret" {
            return Ok(pets::create_pet::Response::Status401);
        }
        let mut store = state.pets.lock().unwrap();
        let pet = pets::Pet {
            id: i64::try_from(store.len()).unwrap_or(i64::MAX) + 1,
            name: req.body.name,
            status: req.body.status,
        };
        store.push(pet.clone());
        drop(store);
        Ok(pets::create_pet::Response::Status201(pet))
    }

    async fn get_pet(
        &self,
        req: pets::get_pet::Request,
        _auth: pets::BearerAuth,
        state: axum::extract::State<AppState>,
        _headers: axum::http::HeaderMap,
    ) -> Result<pets::get_pet::Response, Self::Error> {
        let store = state.pets.lock().unwrap();
        Ok(store
            .iter()
            .find(|pet| pet.id == req.pet_id)
            .cloned()
            .map_or(
                pets::get_pet::Response::Status404,
                pets::get_pet::Response::Status200,
            ))
    }
}
// ANCHOR_END: impl

// ANCHOR: main
#[tokio::main]
async fn main() {
    use pets::PetsApi as _;

    let app = PetServer.router().with_state(AppState::default());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:3000")
        .await
        .unwrap();
    println!("listening on http://{}", listener.local_addr().unwrap());
    axum::serve(listener, app).await.unwrap();
}
// ANCHOR_END: main
