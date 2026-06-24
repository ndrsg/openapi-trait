use proc_macro2::TokenStream;
use quote::quote;
use syn::{Data, DeriveInput, Error, Fields};

/// Resolved field bindings for the `ReqwestClient` derive.
struct ResolvedFields {
    /// Field holding the `reqwest::Client`.
    client: syn::Ident,
    /// Field holding the base URL.
    base_url: syn::Ident,
    /// Field holding the generated `{Mod}AuthState`, if any.
    auth: Option<(syn::Ident, syn::Type)>,
}

/// Walk the struct's named fields and pick out the client/base-url/auth bindings.
fn resolve_fields(
    container: &syn::Ident,
    fields: syn::punctuated::Punctuated<syn::Field, syn::Token![,]>,
) -> syn::Result<ResolvedFields> {
    let mut explicit_client = None;
    let mut explicit_base_url = None;
    let mut explicit_auth: Option<(syn::Ident, syn::Type)> = None;
    let mut default_client = None;
    let mut default_base_url = None;
    let mut default_auth: Option<(syn::Ident, syn::Type)> = None;

    for field in fields {
        let field_ident = field.ident.expect("named fields always have identifiers");
        let markers = parse_markers(&field.attrs)?;

        if field_ident == "client" {
            default_client = Some(field_ident.clone());
        }
        if field_ident == "base_url" {
            default_base_url = Some(field_ident.clone());
        }
        if field_ident == "auth" {
            default_auth = Some((field_ident.clone(), field.ty.clone()));
        }

        if markers.client && explicit_client.replace(field_ident.clone()).is_some() {
            return Err(Error::new_spanned(
                &field_ident,
                "duplicate #[openapi_trait(client)] field",
            ));
        }
        if markers.base_url && explicit_base_url.replace(field_ident.clone()).is_some() {
            return Err(Error::new_spanned(
                &field_ident,
                "duplicate #[openapi_trait(base_url)] field",
            ));
        }
        if markers.auth
            && explicit_auth
                .replace((field_ident.clone(), field.ty.clone()))
                .is_some()
        {
            return Err(Error::new_spanned(
                &field_ident,
                "duplicate #[openapi_trait(auth)] field",
            ));
        }
    }

    let client = explicit_client.or(default_client).ok_or_else(|| {
        Error::new_spanned(
            container,
            "ReqwestClient derive requires a `client` field or #[openapi_trait(client)]",
        )
    })?;
    let base_url = explicit_base_url.or(default_base_url).ok_or_else(|| {
        Error::new_spanned(
            container,
            "ReqwestClient derive requires a `base_url` field or #[openapi_trait(base_url)]",
        )
    })?;
    Ok(ResolvedFields {
        client,
        base_url,
        auth: explicit_auth.or(default_auth),
    })
}

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

    let resolved = resolve_fields(&ident, fields)?;
    let client_field = resolved.client;
    let base_url_field = resolved.base_url;

    let auth_impls = if let Some((field, ty)) = resolved.auth {
        quote! {
            #[automatically_derived]
            impl #impl_generics ::openapi_trait::ReqwestClientAuth<#ty>
                for #ident #ty_generics #where_clause
            {
                fn auth_state(&self) -> &#ty {
                    &self.#field
                }
            }

            #[automatically_derived]
            impl #impl_generics ::core::convert::AsMut<#ty>
                for #ident #ty_generics #where_clause
            {
                fn as_mut(&mut self) -> &mut #ty {
                    &mut self.#field
                }
            }
        }
    } else {
        quote! {}
    };

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

        #auth_impls
    })
}

#[derive(Default)]
/// Track whether a field is explicitly marked for reqwest client extraction.
struct FieldMarkers {
    /// Whether the field stores the `reqwest::Client`.
    client: bool,
    /// Whether the field stores the service base URL.
    base_url: bool,
    /// Whether the field stores the generated `{Mod}AuthState`.
    auth: bool,
}

/// Parse `#[openapi_trait(...)]` markers from one struct field.
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

            if meta.path.is_ident("auth") {
                markers.auth = true;
                return Ok(());
            }

            Err(meta.error(
                "unsupported openapi_trait attribute; expected `client`, `base_url`, or `auth`",
            ))
        })?;
    }

    Ok(markers)
}
