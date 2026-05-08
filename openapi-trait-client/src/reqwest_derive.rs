use proc_macro2::TokenStream;
use quote::quote;
use syn::{Data, DeriveInput, Error, Fields};

/// Expand `#[derive(ReqwestClient)]` for a user-owned reqwest client carrier type.
pub fn expand_reqwest_client(input: DeriveInput) -> syn::Result<TokenStream> {
    let ident = input.ident;
    let (impl_generics, ty_generics, where_clause) = input.generics.split_for_impl();

    let fields = match input.data {
        Data::Struct(data) => match data.fields {
            Fields::Named(fields) => fields.named,
            Fields::Unnamed(_) | Fields::Unit => {
                return Err(Error::new_spanned(
                    &ident,
                    "ReqwestClient can only be derived for structs with named fields",
                ));
            }
        },
        Data::Enum(_) | Data::Union(_) => {
            return Err(Error::new_spanned(
                &ident,
                "ReqwestClient can only be derived for structs",
            ));
        }
    };

    let mut explicit_client = None;
    let mut explicit_base_url = None;
    let mut default_client = None;
    let mut default_base_url = None;

    for field in fields {
        let field_ident = field.ident.expect("named fields always have identifiers");
        let markers = parse_markers(&field.attrs)?;

        if field_ident == "client" {
            default_client = Some(field_ident.clone());
        }
        if field_ident == "base_url" {
            default_base_url = Some(field_ident.clone());
        }

        if markers.client {
            if explicit_client.replace(field_ident.clone()).is_some() {
                return Err(Error::new_spanned(
                    &field_ident,
                    "duplicate #[openapi_trait(client)] field",
                ));
            }
        }

        if markers.base_url {
            if explicit_base_url.replace(field_ident.clone()).is_some() {
                return Err(Error::new_spanned(
                    &field_ident,
                    "duplicate #[openapi_trait(base_url)] field",
                ));
            }
        }
    }

    let client_field = explicit_client.or(default_client).ok_or_else(|| {
        Error::new_spanned(
            &ident,
            "ReqwestClient derive requires a `client` field or #[openapi_trait(client)]",
        )
    })?;
    let base_url_field = explicit_base_url.or(default_base_url).ok_or_else(|| {
        Error::new_spanned(
            &ident,
            "ReqwestClient derive requires a `base_url` field or #[openapi_trait(base_url)]",
        )
    })?;

    Ok(quote! {
        #[automatically_derived]
        impl #impl_generics ::openapi_trait::ReqwestClientCore for #ident #ty_generics #where_clause {
            fn reqwest_client(&self) -> &::openapi_trait::reqwest::Client {
                &self.#client_field
            }

            fn base_url(&self) -> &str {
                self.#base_url_field.as_ref()
            }
        }
    })
}

#[derive(Default)]
struct FieldMarkers {
    client: bool,
    base_url: bool,
}

fn parse_markers(attrs: &[syn::Attribute]) -> syn::Result<FieldMarkers> {
    let mut markers = FieldMarkers::default();

    for attr in attrs {
        if !attr.path().is_ident("openapi_trait") {
            continue;
        }

        attr.parse_nested_meta(|meta| {
            if meta.path.is_ident("client") {
                markers.client = true;
                return Ok(());
            }

            if meta.path.is_ident("base_url") {
                markers.base_url = true;
                return Ok(());
            }

            Err(meta.error("unsupported openapi_trait attribute; expected `client` or `base_url`"))
        })?;
    }

    Ok(markers)
}