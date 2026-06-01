//! JSON schema helpers for tool parameter definitions.
//!
//! Refs: I-Shell-ToolResult-PassThrough

/// Build a minimal JSON Schema `object` with string properties.
///
/// # Example
/// ```
/// use brioche_shell_runtime::schema::object_schema;
///
/// let schema = object_schema(
///     &["path"],
///     &[("path", "Path to the file")],
/// );
/// ```
pub fn object_schema(required: &[&str], properties: &[(&str, &str)]) -> serde_json::Value {
    let mut props = serde_json::Map::new();
    for (name, description) in properties {
        let mut p = serde_json::Map::new();
        p.insert("type".into(), serde_json::Value::String("string".into()));
        p.insert(
            "description".into(),
            serde_json::Value::String((*description).into()),
        );
        props.insert((*name).into(), serde_json::Value::Object(p));
    }
    let mut schema = serde_json::Map::new();
    schema.insert("type".into(), serde_json::Value::String("object".into()));
    schema.insert("properties".into(), serde_json::Value::Object(props));
    schema.insert(
        "required".into(),
        serde_json::Value::Array(
            required
                .iter()
                .map(|s| serde_json::Value::String((*s).into()))
                .collect(),
        ),
    );
    serde_json::Value::Object(schema)
}
