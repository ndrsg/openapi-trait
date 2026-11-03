//! Integration tests for the `openapi_trait_client` macro using the Petstore spec.

#[openapi_trait::client("assets/testdata/petstore.openapi.yaml")]
pub mod petstore {}

use petstore::PetstoreClient as _;

macro_rules! unreachable_operation {
    ($name:ident, $req:ty, $resp:ty) => {
        async fn $name(
            &self,
            _req: $req,
            _options: Option<openapi_trait::RequestOptions>,
        ) -> Result<$resp, Self::Error> {
            unreachable!()
        }
    };
}

struct MockPetstoreClient;

impl petstore::PetstoreClient for MockPetstoreClient {
    type Error = ::std::convert::Infallible;

    unreachable_operation!(
        add_pet,
        petstore::add_pet::Request,
        petstore::add_pet::Response
    );
    unreachable_operation!(
        update_pet,
        petstore::update_pet::Request,
        petstore::update_pet::Response
    );
    unreachable_operation!(
        find_pets_by_status,
        petstore::find_pets_by_status::Request,
        petstore::find_pets_by_status::Response
    );
    unreachable_operation!(
        find_pets_by_tags,
        petstore::find_pets_by_tags::Request,
        petstore::find_pets_by_tags::Response
    );
    unreachable_operation!(
        update_pet_with_form,
        petstore::update_pet_with_form::Request,
        petstore::update_pet_with_form::Response
    );
    unreachable_operation!(
        delete_pet,
        petstore::delete_pet::Request,
        petstore::delete_pet::Response
    );
    unreachable_operation!(
        upload_file,
        petstore::upload_file::Request,
        petstore::upload_file::Response
    );
    unreachable_operation!(
        get_inventory,
        petstore::get_inventory::Request,
        petstore::get_inventory::Response
    );
    unreachable_operation!(
        place_order,
        petstore::place_order::Request,
        petstore::place_order::Response
    );
    unreachable_operation!(
        get_order_by_id,
        petstore::get_order_by_id::Request,
        petstore::get_order_by_id::Response
    );
    unreachable_operation!(
        delete_order,
        petstore::delete_order::Request,
        petstore::delete_order::Response
    );
    unreachable_operation!(
        create_user,
        petstore::create_user::Request,
        petstore::create_user::Response
    );
    unreachable_operation!(
        create_users_with_list_input,
        petstore::create_users_with_list_input::Request,
        petstore::create_users_with_list_input::Response
    );
    unreachable_operation!(
        login_user,
        petstore::login_user::Request,
        petstore::login_user::Response
    );
    unreachable_operation!(
        logout_user,
        petstore::logout_user::Request,
        petstore::logout_user::Response
    );
    unreachable_operation!(
        get_user_by_name,
        petstore::get_user_by_name::Request,
        petstore::get_user_by_name::Response
    );
    unreachable_operation!(
        update_user,
        petstore::update_user::Request,
        petstore::update_user::Response
    );
    unreachable_operation!(
        delete_user,
        petstore::delete_user::Request,
        petstore::delete_user::Response
    );

    async fn get_pet_by_id(
        &self,
        req: petstore::get_pet_by_id::Request,
        _options: Option<openapi_trait::RequestOptions>,
    ) -> Result<petstore::get_pet_by_id::Response, Self::Error> {
        if req.pet_id == 42 {
            Ok(petstore::get_pet_by_id::Response::Status200(
                petstore::Pet {
                    id: Some(42),
                    name: "doggie".into(),
                    photo_urls: vec!["https://example.com/photo.jpg".into()],
                    category: None,
                    tags: None,
                    status: None,
                },
            ))
        } else {
            Ok(petstore::get_pet_by_id::Response::Status404)
        }
    }
}

#[test]
fn get_pet_by_id_request_has_pet_id_field() {
    let req = petstore::get_pet_by_id::Request { pet_id: 42 };
    assert_eq!(req.pet_id, 42);
}

#[test]
fn find_pets_by_status_uses_enum_query() {
    let req = petstore::find_pets_by_status::Request {
        status: petstore::find_pets_by_status::StatusQuery::Available,
    };
    let _ = req;
}

#[tokio::test]
async fn generated_client_trait_uses_generated_types() {
    let client = MockPetstoreClient;

    let response = client
        .get_pet_by_id(petstore::get_pet_by_id::Request { pet_id: 42 }, None)
        .await
        .unwrap();

    match response {
        petstore::get_pet_by_id::Response::Status200(pet) => {
            assert_eq!(pet.id, Some(42));
            assert_eq!(pet.name, "doggie");
        }
        _ => panic!("expected 200 response"),
    }
}

#[tokio::test]
async fn generated_client_trait_can_return_non_success_status() {
    let client = MockPetstoreClient;

    let response = client
        .get_pet_by_id(petstore::get_pet_by_id::Request { pet_id: 99 }, None)
        .await
        .unwrap();

    assert!(matches!(
        response,
        petstore::get_pet_by_id::Response::Status404
    ));
}
