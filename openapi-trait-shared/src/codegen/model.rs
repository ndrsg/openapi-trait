//! A thin view over the [`oas3`] schema model, shaped like what the codegen
//! matches on.
//!
//! [`oas3`] stores every schema as one flat [`ObjectSchema`] with a `type`
//! list and optional `allOf` / `oneOf` / `anyOf`. The codegen instead needs
//! to know what a schema *is*: a reference, an object, a string, a
//! composition, … [`view`] and [`kind`] answer that, and [`is_nullable`]
//! answers whether `null` is allowed, which 3.1 expresses either as `"null"`
//! in the `type` list or as a `{type: "null"}` branch.
//!
//! Keeping this in one place also keeps changes to the [`oas3`] API out of
//! the rest of the codegen.

use std::sync::LazyLock;

use oas3::spec::{ObjectOrReference, ObjectSchema, Schema, SchemaType, SchemaTypeSet, Spec};

/// A schema seen as either a `$ref` or an inline schema.
#[derive(Debug, Clone, Copy)]
pub enum SchemaRef<'a> {
    /// A `$ref`, holding the reference string (`#/components/schemas/Pet`).
    Ref(&'a str),
    /// An inline schema.
    Item(&'a ObjectSchema),
}

/// What an inline schema describes, which decides the Rust type it becomes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// `type: object`, or no `type` but `properties` / `additionalProperties`.
    Object,
    /// `type: array`, or no `type` but `items`.
    Array,
    /// `type: string`.
    String,
    /// `type: integer`.
    Integer,
    /// `type: number`.
    Number,
    /// `type: boolean`.
    Boolean,
    /// `oneOf`.
    OneOf,
    /// `allOf`.
    AllOf,
    /// `anyOf`.
    AnyOf,
    /// Anything else: no constraints, several types, a boolean schema, `not`,
    /// … These become `serde_json::Value`.
    Any,
}

/// The empty schema, which a boolean schema (`true` / `false`) is viewed as.
static EMPTY: LazyLock<ObjectSchema> = LazyLock::new(ObjectSchema::default);

/// View `schema` as a `$ref` or an inline schema.
///
/// A schema that only wraps another one to allow `null`
/// (`oneOf: [<schema>, {type: "null"}]`, or the same with `anyOf`) is viewed
/// as the wrapped schema; [`is_nullable`] still reports the `null`. A
/// boolean schema is viewed as the empty schema.
#[must_use]
pub fn view(schema: &Schema) -> SchemaRef<'_> {
    match schema {
        Schema::Boolean(_) => SchemaRef::Item(&EMPTY),
        Schema::Object(obj) => match obj.as_ref() {
            ObjectOrReference::Ref { ref_path, .. } => SchemaRef::Ref(ref_path),
            ObjectOrReference::Object(obj) => null_wrapped(obj).map_or(SchemaRef::Item(obj), view),
        },
    }
}

/// The inline schema behind `schema`, if it isn't a `$ref` (see [`view`]).
#[must_use]
pub fn item(schema: &Schema) -> Option<&ObjectSchema> {
    match view(schema) {
        SchemaRef::Item(obj) => Some(obj),
        SchemaRef::Ref(_) => None,
    }
}

/// The schema that `obj` wraps to allow `null`: the other branch of a
/// two-branch `oneOf` / `anyOf` whose second branch is `{type: "null"}`,
/// when `obj` has no `type` of its own.
fn null_wrapped(obj: &ObjectSchema) -> Option<&Schema> {
    if obj.schema_type.is_some() || !obj.all_of.is_empty() {
        return None;
    }
    let ((branches, []) | ([], branches)) = (obj.one_of.as_slice(), obj.any_of.as_slice()) else {
        return None;
    };
    match branches {
        [a, b] if is_null_schema(b) && !is_null_schema(a) => Some(a),
        [a, b] if is_null_schema(a) && !is_null_schema(b) => Some(b),
        _ => None,
    }
}

/// Whether `schema` only allows `null` (`type: "null"`).
#[must_use]
pub fn is_null_schema(schema: &Schema) -> bool {
    let Schema::Object(obj) = schema else {
        return false;
    };
    let ObjectOrReference::Object(obj) = obj.as_ref() else {
        return false;
    };
    match &obj.schema_type {
        Some(SchemaTypeSet::Single(SchemaType::Null)) => true,
        Some(SchemaTypeSet::Multiple(types)) => types == &[SchemaType::Null],
        _ => false,
    }
}

/// Whether `schema` allows `null`: `"null"` is in its `type` list, or one of
/// its `oneOf` / `anyOf` branches is `{type: "null"}`.
///
/// A `$ref` is never nullable; a nullable reference is written as
/// `oneOf: [$ref, {type: "null"}]`.
#[must_use]
pub fn is_nullable(schema: &Schema) -> bool {
    let Schema::Object(obj) = schema else {
        return false;
    };
    let ObjectOrReference::Object(obj) = obj.as_ref() else {
        return false;
    };
    obj.schema_type
        .as_ref()
        .is_some_and(|types| types.contains(SchemaType::Null))
        || obj.one_of.iter().chain(&obj.any_of).any(is_null_schema)
}

/// The branches of a composition that aren't `{type: "null"}`. A null
/// branch makes the composition nullable (see [`is_nullable`]) rather than
/// adding a variant.
pub fn non_null_branches(branches: &[Schema]) -> impl Iterator<Item = &Schema> {
    branches.iter().filter(|branch| !is_null_schema(branch))
}

/// The description of `schema`: its own, or, for a wrapper that only allows
/// `null` (see [`view`]), the wrapped schema's.
#[must_use]
pub fn description(schema: &Schema) -> Option<&String> {
    if let Schema::Object(obj) = schema {
        if let ObjectOrReference::Object(obj) = obj.as_ref() {
            if obj.description.is_some() {
                return obj.description.as_ref();
            }
        }
    }
    item(schema).and_then(|obj| obj.description.as_ref())
}

/// What `obj` describes (see [`Kind`]).
///
/// A composition keyword wins over `type`, and `"null"` in the `type` list
/// is ignored (see [`is_nullable`]).
#[must_use]
pub fn kind(obj: &ObjectSchema) -> Kind {
    if !obj.one_of.is_empty() {
        return Kind::OneOf;
    }
    if !obj.all_of.is_empty() {
        return Kind::AllOf;
    }
    if !obj.any_of.is_empty() {
        return Kind::AnyOf;
    }
    let types: Vec<SchemaType> = match &obj.schema_type {
        None => Vec::new(),
        Some(SchemaTypeSet::Single(ty)) => vec![*ty],
        Some(SchemaTypeSet::Multiple(types)) => types.clone(),
    };
    let non_null: Vec<SchemaType> = types
        .into_iter()
        .filter(|t| *t != SchemaType::Null)
        .collect();
    match non_null.as_slice() {
        [SchemaType::Object] => Kind::Object,
        [SchemaType::Array] => Kind::Array,
        [SchemaType::String] => Kind::String,
        [SchemaType::Integer] => Kind::Integer,
        [SchemaType::Number] => Kind::Number,
        [SchemaType::Boolean] => Kind::Boolean,
        [] if obj.schema_type.is_none()
            && (!obj.properties.is_empty() || obj.additional_properties.is_some()) =>
        {
            Kind::Object
        }
        [] if obj.schema_type.is_none() && obj.items.is_some() => Kind::Array,
        _ => Kind::Any,
    }
}

/// The values of a string enum: a string schema with an `enum` of strings
/// (`null` entries are skipped), or a `const` string. `None` for any other
/// schema.
#[must_use]
pub fn string_enum_values(obj: &ObjectSchema) -> Option<Vec<String>> {
    if kind(obj) != Kind::String {
        return None;
    }
    let values: Vec<&serde_json::Value> = if obj.enum_values.is_empty() {
        obj.const_value.iter().collect()
    } else {
        obj.enum_values.iter().filter(|v| !v.is_null()).collect()
    };
    let strings: Option<Vec<String>> = values
        .into_iter()
        .map(|v| v.as_str().map(str::to_owned))
        .collect();
    strings.filter(|s| !s.is_empty())
}

/// Whether `obj` is a string enum (see [`string_enum_values`]).
#[must_use]
pub fn is_string_enum(obj: &ObjectSchema) -> bool {
    string_enum_values(obj).is_some()
}

/// Whether `obj` is a string with `format: <format>`.
#[must_use]
pub fn is_string_format(obj: &ObjectSchema, format: &str) -> bool {
    kind(obj) == Kind::String && obj.format.as_deref() == Some(format)
}

/// The component name a `$ref` points at: `#/components/schemas/Foo` → `Foo`.
#[must_use]
pub fn ref_name(reference: &str) -> &str {
    reference.rsplit('/').next().unwrap_or(reference)
}

/// The component schemas of `spec`, in document order.
pub fn component_schemas(spec: &Spec) -> impl Iterator<Item = (&String, &Schema)> {
    spec.components
        .iter()
        .flat_map(|components| components.schemas.iter())
}

/// The component schema a `$ref` points at, if it is a local
/// `#/components/schemas/…` reference that exists.
#[must_use]
pub fn resolve_schema_ref<'a>(spec: &'a Spec, reference: &str) -> Option<&'a Schema> {
    let name = reference.strip_prefix("#/components/schemas/")?;
    spec.components.as_ref()?.schemas.get(name)
}

/// The key of an entry in an operation's `responses`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StatusKey {
    /// A status code such as `200`.
    Code(u16),
    /// A range such as `2XX`, holding its first digit.
    Range(u8),
    /// `default`.
    Default,
    /// Anything else.
    Invalid,
}

/// Parse the key of an entry in an operation's `responses`.
#[must_use]
pub fn parse_status_key(key: &str) -> StatusKey {
    if key == "default" {
        return StatusKey::Default;
    }
    if let Ok(code) = key.parse::<u16>() {
        return StatusKey::Code(code);
    }
    match key.as_bytes() {
        [digit @ b'1'..=b'5', b'X' | b'x', b'X' | b'x'] => StatusKey::Range(digit - b'0'),
        _ => StatusKey::Invalid,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Parse a schema from YAML.
    fn schema(yaml: &str) -> Schema {
        yaml_serde::from_str(yaml).expect("schema parses")
    }

    #[test]
    fn type_list_with_null_is_nullable_and_keeps_its_kind() {
        let s = schema("type: [string, 'null']");
        assert!(is_nullable(&s));
        assert_eq!(kind(item(&s).unwrap()), Kind::String);
    }

    #[test]
    fn several_non_null_types_are_any() {
        let s = schema("type: [string, integer]");
        assert!(!is_nullable(&s));
        assert_eq!(kind(item(&s).unwrap()), Kind::Any);
    }

    #[test]
    fn one_of_ref_and_null_is_viewed_as_the_ref() {
        for keyword in ["oneOf", "anyOf"] {
            let s = schema(&format!(
                "{keyword}: [{{ $ref: '#/components/schemas/Pet' }}, {{ type: 'null' }}]"
            ));
            assert!(is_nullable(&s));
            assert!(matches!(
                view(&s),
                SchemaRef::Ref("#/components/schemas/Pet")
            ));
        }
    }

    #[test]
    fn a_wrapper_keeps_its_own_description() {
        let s = schema("description: d\noneOf: [{ type: string }, { type: 'null' }]");
        assert_eq!(description(&s).map(String::as_str), Some("d"));
    }

    #[test]
    fn a_composition_with_several_branches_and_null_stays_a_composition() {
        let s = schema("oneOf: [{ type: string }, { type: integer }, { type: 'null' }]");
        assert!(is_nullable(&s));
        let obj = item(&s).unwrap();
        assert_eq!(kind(obj), Kind::OneOf);
        assert_eq!(non_null_branches(&obj.one_of).count(), 2);
    }

    #[test]
    fn boolean_schema_is_any() {
        let s = schema("true");
        assert_eq!(kind(item(&s).unwrap()), Kind::Any);
    }

    #[test]
    fn properties_without_type_are_an_object() {
        let s = schema("properties: { a: { type: string } }");
        assert_eq!(kind(item(&s).unwrap()), Kind::Object);
    }

    #[test]
    fn const_string_is_a_one_value_enum() {
        let s = schema("type: string\nconst: fixed");
        assert_eq!(
            string_enum_values(item(&s).unwrap()),
            Some(vec!["fixed".to_owned()])
        );
    }

    #[test]
    fn enum_null_entries_are_skipped() {
        let s = schema("type: [string, 'null']\nenum: [a, null, b]");
        assert_eq!(
            string_enum_values(item(&s).unwrap()),
            Some(vec!["a".to_owned(), "b".to_owned()])
        );
    }

    #[test]
    fn status_keys_parse() {
        assert_eq!(parse_status_key("200"), StatusKey::Code(200));
        assert_eq!(parse_status_key("4XX"), StatusKey::Range(4));
        assert_eq!(parse_status_key("default"), StatusKey::Default);
        assert_eq!(parse_status_key("teapot"), StatusKey::Invalid);
    }
}
