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

    unreachable_operation!(add_pet, petstore::AddPetRequest, petstore::AddPetResponse);
    unreachable_operation!(
        update_pet,
        petstore::UpdatePetRequest,
        petstore::UpdatePetResponse
    );
    unreachable_operation!(
        find_pets_by_status,
        petstore::FindPetsByStatusRequest,
        petstore::FindPetsByStatusResponse
    );
    unreachable_operation!(
        find_pets_by_tags,
        petstore::FindPetsByTagsRequest,
        petstore::FindPetsByTagsResponse
    );
    unreachable_operation!(
        update_pet_with_form,
        petstore::UpdatePetWithFormRequest,
        petstore::UpdatePetWithFormResponse
    );
    unreachable_operation!(
        delete_pet,
        petstore::DeletePetRequest,
        petstore::DeletePetResponse
    );
    unreachable_operation!(
        upload_file,
        petstore::UploadFileRequest,
        petstore::UploadFileResponse
    );
    unreachable_operation!(
        get_inventory,
        petstore::GetInventoryRequest,
        petstore::GetInventoryResponse
    );
    unreachable_operation!(
        place_order,
        petstore::PlaceOrderRequest,
        petstore::PlaceOrderResponse
    );
    unreachable_operation!(
        get_order_by_id,
        petstore::GetOrderByIdRequest,
        petstore::GetOrderByIdResponse
    );
    unreachable_operation!(
        delete_order,
        petstore::DeleteOrderRequest,
        petstore::DeleteOrderResponse
    );
    unreachable_operation!(
        create_user,
        petstore::CreateUserRequest,
        petstore::CreateUserResponse
    );
    unreachable_operation!(
        create_users_with_list_input,
        petstore::CreateUsersWithListInputRequest,
        petstore::CreateUsersWithListInputResponse
    );
    unreachable_operation!(
        login_user,
        petstore::LoginUserRequest,
        petstore::LoginUserResponse
    );
    unreachable_operation!(
        logout_user,
        petstore::LogoutUserRequest,
        petstore::LogoutUserResponse
    );
    unreachable_operation!(
        get_user_by_name,
        petstore::GetUserByNameRequest,
        petstore::GetUserByNameResponse
    );
    unreachable_operation!(
        update_user,
        petstore::UpdateUserRequest,
        petstore::UpdateUserResponse
    );
    unreachable_operation!(
        delete_user,
        petstore::DeleteUserRequest,
        petstore::DeleteUserResponse
    );

    async fn get_pet_by_id(
        &self,
        req: petstore::GetPetByIdRequest,
        _options: Option<openapi_trait::RequestOptions>,
    ) -> Result<petstore::GetPetByIdResponse, Self::Error> {
        if req.pet_id == 42 {
            Ok(petstore::GetPetByIdResponse::Status200(petstore::Pet {
                id: Some(42),
                name: "doggie".into(),
                photo_urls: vec!["https://example.com/photo.jpg".into()],
                category: None,
                tags: None,
                status: None,
            }))
        } else {
            Ok(petstore::GetPetByIdResponse::Status404)
        }
    }
}

#[test]
fn get_pet_by_id_request_has_pet_id_field() {
    let req = petstore::GetPetByIdRequest { pet_id: 42 };
    assert_eq!(req.pet_id, 42);
}

#[test]
fn find_pets_by_status_uses_enum_query() {
    let req = petstore::FindPetsByStatusRequest {
        status: petstore::FindPetsByStatusStatusQuery::Available,
    };
    let _ = req;
}

#[tokio::test]
async fn generated_client_trait_uses_generated_types() {
    let client = MockPetstoreClient;

    let response = client
        .get_pet_by_id(petstore::GetPetByIdRequest { pet_id: 42 }, None)
        .await
        .unwrap();

    match response {
        petstore::GetPetByIdResponse::Status200(pet) => {
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
        .get_pet_by_id(petstore::GetPetByIdRequest { pet_id: 99 }, None)
        .await
        .unwrap();

    assert!(matches!(response, petstore::GetPetByIdResponse::Status404));
}
