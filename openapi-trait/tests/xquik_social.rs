//! Compile-time coverage for a real OpenAPI 3.1 social API shape with header
//! API key auth, query cursors, path params, request bodies, and response refs.
#![cfg(feature = "reqwest-client")]

#[openapi_trait::client("assets/testdata/xquik_social.openapi.yaml")]
pub mod xquik_client {}

#[test]
fn xquik_social_spec_generates_client_types() {
    let _ = std::any::type_name::<xquik_client::SearchTweetsRequest>();
    let _ = std::any::type_name::<xquik_client::ReplyToTweetRequest>();
    let _ = std::any::type_name::<xquik_client::SearchTweetsResponse>();
}
