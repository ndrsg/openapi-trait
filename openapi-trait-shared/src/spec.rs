//! Loading an `OpenAPI` document into the [`oas3`] model.
//!
//! [`oas3`] models OAS 3.1 only. [`load_spec`] therefore reads the document as
//! plain JSON first, converts a 3.0 document to its 3.1 equivalent (see
//! `upconvert_30`), and works around the few places where the model is
//! stricter or less precise than the codegen needs (see `normalize`). Only
//! then is the result deserialized into [`oas3::Spec`].

use serde_json::{Map, Value};

/// Extension that `normalize` sets on every operation with its own
/// `security` field (stored by [`oas3`] without its `x-` prefix).
///
/// [`oas3::spec::Operation::security`] is a plain `Vec`, so an operation
/// without `security`, which inherits the document's requirements, can't be
/// told apart from `security: []`, which turns them off.
pub const OPERATION_HAS_SECURITY: &str = "openapi-trait-has-security";

/// Parse an `OpenAPI` 3.0 or 3.1 document, in YAML or JSON.
///
/// # Errors
///
/// Returns a message when the document isn't valid YAML, isn't an `OpenAPI`
/// 3.0 or 3.1 document, or doesn't match the `OpenAPI` model.
pub fn load_spec(content: &str) -> Result<oas3::Spec, String> {
    let yaml: yaml_serde::Value = yaml_serde::from_str(content).map_err(|e| e.to_string())?;
    let mut doc = yaml_to_json(yaml)?;
    let root = doc
        .as_object_mut()
        .ok_or("the document is not a YAML or JSON object")?;

    match document_version(root)?.as_str() {
        v if v.starts_with("3.0") => upconvert_30(root),
        v if v.starts_with("3.1") => {
            // Write the version back as a string, in case it was a number.
            root.insert("openapi".to_owned(), Value::String(v.to_owned()));
        }
        v => {
            return Err(format!(
                "only OpenAPI 3.0 and 3.1 documents are supported, but this one declares `openapi: {v}`"
            ))
        }
    }
    normalize(root);

    serde_path_to_error::deserialize(doc).map_err(|e| {
        let path = e.path().to_string();
        if path == "." {
            e.into_inner().to_string()
        } else {
            format!("at `{path}`: {}", e.into_inner())
        }
    })
}

/// The document's `openapi` version, as a string even when YAML read it as a
/// number (`openapi: 3.1`).
fn document_version(root: &Map<String, Value>) -> Result<String, String> {
    match root.get("openapi") {
        Some(Value::String(v)) => Ok(v.clone()),
        Some(Value::Number(v)) => Ok(v.to_string()),
        Some(_) => Err("the `openapi` field must be a version string such as `3.1.0`".to_owned()),
        None if root.contains_key("swagger") => Err(
            "this is a Swagger (OpenAPI 2.0) document; only OpenAPI 3.0 and 3.1 are supported, so convert it to OpenAPI 3 first"
                .to_owned(),
        ),
        None => Err("the document has no `openapi` field".to_owned()),
    }
}

/// Convert a YAML value to JSON. Non-string mapping keys, such as unquoted
/// response codes (`200:`), become strings.
fn yaml_to_json(value: yaml_serde::Value) -> Result<Value, String> {
    Ok(match value {
        yaml_serde::Value::Null => Value::Null,
        yaml_serde::Value::Bool(b) => Value::Bool(b),
        yaml_serde::Value::Number(n) => yaml_number_to_json(&n),
        yaml_serde::Value::String(s) => Value::String(s),
        yaml_serde::Value::Sequence(items) => Value::Array(
            items
                .into_iter()
                .map(yaml_to_json)
                .collect::<Result<_, _>>()?,
        ),
        yaml_serde::Value::Mapping(mapping) => {
            let mut out = Map::new();
            for (key, value) in mapping {
                let key = match key {
                    yaml_serde::Value::String(s) => s,
                    yaml_serde::Value::Number(n) => n.to_string(),
                    yaml_serde::Value::Bool(b) => b.to_string(),
                    yaml_serde::Value::Null => "null".to_owned(),
                    other => return Err(format!("unsupported YAML mapping key: {other:?}")),
                };
                out.insert(key, yaml_to_json(value)?);
            }
            Value::Object(out)
        }
        yaml_serde::Value::Tagged(tagged) => yaml_to_json(tagged.value)?,
    })
}

/// Convert a YAML number, keeping integers as integers. A non-finite float,
/// which JSON can't hold, becomes `null`.
fn yaml_number_to_json(n: &yaml_serde::Number) -> Value {
    n.as_i64().map_or_else(
        || {
            n.as_u64().map_or_else(
                || {
                    n.as_f64()
                        .and_then(serde_json::Number::from_f64)
                        .map_or(Value::Null, Value::Number)
                },
                Value::from,
            )
        },
        Value::from,
    )
}

/// Rewrite a 3.0 document in place into its 3.1 equivalent:
///
/// - `nullable: true` becomes `"null"` in the `type` list, or, on a schema
///   without `type` (such as `allOf: [$ref]`), an `anyOf` with a
///   `{type: "null"}` branch.
/// - The boolean `exclusiveMinimum` / `exclusiveMaximum` flags become the
///   numeric bounds of 3.1.
fn upconvert_30(root: &mut Map<String, Value>) {
    root.insert("openapi".to_owned(), Value::String("3.1.0".to_owned()));
    for_each_schema(root, &mut upconvert_schema_30);
}

/// `upconvert_30` for one schema object; `for_each_schema` handles nesting.
fn upconvert_schema_30(schema: &mut Map<String, Value>) {
    for (flag, bound) in [
        ("exclusiveMinimum", "minimum"),
        ("exclusiveMaximum", "maximum"),
    ] {
        match schema.get(flag) {
            Some(Value::Bool(true)) => {
                if let Some(value) = schema.remove(bound) {
                    schema.insert(flag.to_owned(), value);
                } else {
                    schema.remove(flag);
                }
            }
            Some(Value::Bool(false)) => {
                schema.remove(flag);
            }
            _ => {}
        }
    }

    if schema.remove("nullable") != Some(Value::Bool(true)) {
        return;
    }
    match schema.get_mut("type") {
        Some(Value::String(ty)) => {
            let ty = std::mem::take(ty);
            schema.insert("type".to_owned(), serde_json::json!([ty, "null"]));
        }
        Some(_) => {}
        None => {
            let rest = std::mem::take(schema);
            schema.insert(
                "anyOf".to_owned(),
                serde_json::json!([Value::Object(rest), { "type": "null" }]),
            );
        }
    }
}

/// Work around the places where [`oas3`] can't represent what the codegen
/// needs:
///
/// - `info` is reduced to `title` and `version` (as strings), and the
///   top-level and per-operation `externalDocs` are removed: [`oas3`] parses
///   their URLs as absolute `url::Url`s, so a relative one fails, and the
///   codegen doesn't read them.
/// - `OAuth2` `flows` are emptied for the same reason: their URLs must be
///   absolute, and scopes aren't checked.
/// - Every operation that declares `security` gets
///   [`OPERATION_HAS_SECURITY`].
fn normalize(root: &mut Map<String, Value>) {
    root.remove("externalDocs");
    if let Some(Value::Object(info)) = root.get_mut("info") {
        let mut kept = Map::new();
        for key in ["title", "version"] {
            let value = match info.remove(key) {
                Some(Value::String(s)) => s,
                Some(Value::Null) | None => String::new(),
                Some(other) => other.to_string(),
            };
            kept.insert(key.to_owned(), Value::String(value));
        }
        *info = kept;
    }

    if let Some(Value::Object(schemes)) = root
        .get_mut("components")
        .and_then(|c| c.get_mut("securitySchemes"))
    {
        for scheme in schemes.values_mut().filter_map(Value::as_object_mut) {
            if scheme.contains_key("flows") {
                scheme.insert("flows".to_owned(), Value::Object(Map::new()));
            }
        }
    }

    for item in path_items(root) {
        for (method, operation) in item.iter_mut() {
            if !HTTP_METHODS.contains(&method.as_str()) {
                continue;
            }
            let Some(operation) = operation.as_object_mut() else {
                continue;
            };
            operation.remove("externalDocs");
            if operation.contains_key("security") {
                operation.insert(format!("x-{OPERATION_HAS_SECURITY}"), Value::Bool(true));
            }
        }
    }
}

/// The keys of a path item that hold operations.
const HTTP_METHODS: [&str; 8] = [
    "get", "put", "post", "delete", "options", "head", "patch", "trace",
];

/// The path item objects under `paths`.
fn path_items(root: &mut Map<String, Value>) -> impl Iterator<Item = &mut Map<String, Value>> {
    root.get_mut("paths")
        .and_then(Value::as_object_mut)
        .into_iter()
        .flat_map(|paths| paths.values_mut().filter_map(Value::as_object_mut))
}

/// Call `f` on every schema object in the document, innermost first: the
/// component schemas, and the schemas of parameters, headers, request bodies
/// and responses, both in `components` and under `paths`, including the
/// schemas nested in them. `$ref` objects are skipped.
fn for_each_schema(root: &mut Map<String, Value>, f: &mut impl FnMut(&mut Map<String, Value>)) {
    if let Some(Value::Object(components)) = root.get_mut("components") {
        for (section, value) in components.iter_mut() {
            let Some(entries) = value.as_object_mut() else {
                continue;
            };
            for entry in entries.values_mut() {
                match section.as_str() {
                    "schemas" => visit_schema(entry, f),
                    "parameters" | "headers" => visit_parameter(entry, f),
                    "requestBodies" | "responses" => visit_body_or_response(entry, f),
                    _ => {}
                }
            }
        }
    }

    for item in path_items(root) {
        for (key, value) in item.iter_mut() {
            if key == "parameters" {
                visit_parameters(value, f);
            } else if HTTP_METHODS.contains(&key.as_str()) {
                let Some(operation) = value.as_object_mut() else {
                    continue;
                };
                if let Some(parameters) = operation.get_mut("parameters") {
                    visit_parameters(parameters, f);
                }
                if let Some(body) = operation.get_mut("requestBody") {
                    visit_body_or_response(body, f);
                }
                if let Some(Value::Object(responses)) = operation.get_mut("responses") {
                    for response in responses.values_mut() {
                        visit_body_or_response(response, f);
                    }
                }
            }
        }
    }
}

/// [`for_each_schema`] for a list of parameters.
fn visit_parameters(parameters: &mut Value, f: &mut impl FnMut(&mut Map<String, Value>)) {
    if let Value::Array(parameters) = parameters {
        for parameter in parameters {
            visit_parameter(parameter, f);
        }
    }
}

/// [`for_each_schema`] for a parameter or header: its `schema` or `content`.
fn visit_parameter(parameter: &mut Value, f: &mut impl FnMut(&mut Map<String, Value>)) {
    let Some(parameter) = parameter.as_object_mut() else {
        return;
    };
    if let Some(schema) = parameter.get_mut("schema") {
        visit_schema(schema, f);
    }
    if let Some(content) = parameter.get_mut("content") {
        visit_content(content, f);
    }
}

/// [`for_each_schema`] for a request body or response: its `content`, and a
/// response's `headers`.
fn visit_body_or_response(value: &mut Value, f: &mut impl FnMut(&mut Map<String, Value>)) {
    let Some(value) = value.as_object_mut() else {
        return;
    };
    if let Some(content) = value.get_mut("content") {
        visit_content(content, f);
    }
    if let Some(Value::Object(headers)) = value.get_mut("headers") {
        for header in headers.values_mut() {
            visit_parameter(header, f);
        }
    }
}

/// [`for_each_schema`] for a `content` map: each media type's `schema`.
fn visit_content(content: &mut Value, f: &mut impl FnMut(&mut Map<String, Value>)) {
    let Some(content) = content.as_object_mut() else {
        return;
    };
    for media in content.values_mut() {
        if let Some(schema) = media.get_mut("schema") {
            visit_schema(schema, f);
        }
    }
}

/// [`for_each_schema`] for a schema: its subschemas first, then the schema
/// itself.
fn visit_schema(schema: &mut Value, f: &mut impl FnMut(&mut Map<String, Value>)) {
    let Some(obj) = schema.as_object_mut() else {
        return;
    };
    if obj.contains_key("$ref") {
        return;
    }
    for (key, value) in obj.iter_mut() {
        match key.as_str() {
            "properties" => {
                if let Some(properties) = value.as_object_mut() {
                    for property in properties.values_mut() {
                        visit_schema(property, f);
                    }
                }
            }
            "items" | "additionalProperties" | "not" => visit_schema(value, f),
            "allOf" | "oneOf" | "anyOf" => {
                if let Some(branches) = value.as_array_mut() {
                    for branch in branches {
                        visit_schema(branch, f);
                    }
                }
            }
            _ => {}
        }
    }
    f(obj);
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A 3.0 document whose only component schema is `schema` (YAML).
    fn with_schema(version: &str, schema: &str) -> String {
        let schema = schema.replace('\n', "\n        ");
        format!(
            "openapi: {version}\ninfo: {{ title: t, version: '1' }}\npaths: {{}}\ncomponents:\n  schemas:\n    S:\n        {schema}\n"
        )
    }

    /// The component schema `S` of `doc` after loading, as JSON.
    fn loaded_schema(doc: &str) -> Value {
        let spec = load_spec(doc).expect("document loads");
        serde_json::to_value(&spec.components.unwrap().schemas["S"]).unwrap()
    }

    #[test]
    fn nullable_with_type_becomes_a_type_list() {
        let doc = with_schema("3.0.3", "type: string\nnullable: true");
        assert_eq!(
            loaded_schema(&doc),
            serde_json::json!({ "type": ["string", "null"] })
        );
    }

    #[test]
    fn nullable_without_type_becomes_an_any_of_with_a_null_branch() {
        let doc = with_schema(
            "3.0.3",
            "allOf: [{ $ref: '#/components/schemas/T' }]\nnullable: true\ndescription: d",
        );
        assert_eq!(
            loaded_schema(&doc),
            serde_json::json!({ "anyOf": [
                { "allOf": [{ "$ref": "#/components/schemas/T" }], "description": "d" },
                { "type": "null" },
            ] })
        );
    }

    #[test]
    fn nested_nullable_properties_and_items_are_converted() {
        let doc = with_schema(
            "3.0.3",
            "type: object\nproperties:\n  a:\n    type: array\n    items: { type: integer, nullable: true }",
        );
        assert_eq!(
            loaded_schema(&doc)["properties"]["a"]["items"],
            serde_json::json!({ "type": ["integer", "null"] })
        );
    }

    #[test]
    fn boolean_exclusive_bounds_become_numeric() {
        let doc = with_schema(
            "3.0.3",
            "type: integer\nminimum: 1\nexclusiveMinimum: true\nmaximum: 9\nexclusiveMaximum: false",
        );
        assert_eq!(
            loaded_schema(&doc),
            serde_json::json!({ "type": "integer", "exclusiveMinimum": 1, "maximum": 9 })
        );
    }

    #[test]
    fn a_31_document_is_left_as_is() {
        let doc = with_schema("3.1.0", "type: [string, 'null']\nexclusiveMinimum: 3");
        assert_eq!(
            loaded_schema(&doc),
            serde_json::json!({ "type": ["string", "null"], "exclusiveMinimum": 3 })
        );
    }

    #[test]
    fn parameter_and_response_schemas_are_converted() {
        let doc = r"
openapi: 3.0.0
info: { title: t, version: '1' }
paths:
  /a:
    get:
      parameters:
        - { name: q, in: query, schema: { type: string, nullable: true } }
      responses:
        200:
          description: ok
          content:
            application/json:
              schema: { type: integer, nullable: true }
";
        let spec = serde_json::to_value(load_spec(doc).unwrap()).unwrap();
        let op = &spec["paths"]["/a"]["get"];
        assert_eq!(
            op["parameters"][0]["schema"],
            serde_json::json!({ "type": ["string", "null"] })
        );
        assert_eq!(
            op["responses"]["200"]["content"]["application/json"]["schema"],
            serde_json::json!({ "type": ["integer", "null"] })
        );
    }

    #[test]
    fn swagger_2_is_rejected_with_a_clear_message() {
        let err =
            load_spec("swagger: '2.0'\ninfo: { title: t, version: '1' }\npaths: {}").unwrap_err();
        assert!(err.contains("Swagger (OpenAPI 2.0)"), "{err}");
    }

    #[test]
    fn other_versions_are_rejected() {
        let err = load_spec("openapi: 4.0.0\ninfo: { title: t, version: '1' }").unwrap_err();
        assert!(err.contains("`openapi: 4.0.0`"), "{err}");
    }

    #[test]
    fn a_numeric_version_is_accepted() {
        load_spec("openapi: 3.1\ninfo: { title: t, version: 1.0 }\npaths: {}").unwrap();
    }

    #[test]
    fn relative_urls_in_unused_fields_do_not_fail() {
        let doc = r"
openapi: 3.0.0
info: { title: t, version: '1', license: { name: MIT, url: /license } }
externalDocs: { url: /docs }
paths: {}
components:
  securitySchemes:
    oauth:
      type: oauth2
      flows:
        password: { tokenUrl: /token, scopes: {} }
";
        load_spec(doc).unwrap();
    }

    #[test]
    fn operations_declaring_security_are_marked() {
        let doc = r"
openapi: 3.1.0
info: { title: t, version: '1' }
paths:
  /a:
    get: { security: [], responses: {} }
    put: { responses: {} }
";
        let spec = load_spec(doc).unwrap();
        let item = &spec.paths.unwrap()["/a"];
        assert!(item
            .get
            .as_ref()
            .unwrap()
            .extensions
            .contains_key(OPERATION_HAS_SECURITY));
        assert!(!item
            .put
            .as_ref()
            .unwrap()
            .extensions
            .contains_key(OPERATION_HAS_SECURITY));
    }

    #[test]
    fn model_errors_name_the_location() {
        let err = load_spec("openapi: 3.1.0\ninfo: { title: t, version: '1' }\npaths: { /a: { get: { parameters: [{ name: x }] } } }")
            .unwrap_err();
        assert!(err.contains("paths"), "{err}");
    }
}
