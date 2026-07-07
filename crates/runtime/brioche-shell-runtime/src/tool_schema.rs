//! Tool and provider schema construction for shell-runtime integrations.
//!
//! ## Public interface
//! - [`ToolSchemaProperty`]: typed parameter metadata for object schemas.
//! - [`tool_parameters_schema`]: builds JSON Schema parameter objects.
//! - [`openai_function_tool_schema`]: wraps parameter objects for OpenAI tools.
//!
//! ## Invariants upheld
//! - I-Shell-Runtime-OnlyIO: schema construction performs no I/O.
//! - I-Shell-ToolResult-PassThrough: tool parameter contracts remain structural metadata.
//!
//! Refs: docs/SPECS.md §Book III-A

/// JSON Schema primitive types supported by Brioche tool parameters.
///
/// Refs: I-Shell-Runtime-OnlyIO
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ToolSchemaPropertyType {
    /// UTF-8 string parameter.
    String,
    /// Boolean parameter.
    Boolean,
    /// Integer parameter.
    Integer,
}

impl ToolSchemaPropertyType {
    fn as_json_type(self) -> &'static str {
        match self {
            Self::String => "string",
            Self::Boolean => "boolean",
            Self::Integer => "integer",
        }
    }
}

/// Typed metadata for one tool parameter object property.
///
/// Refs: I-Shell-Runtime-OnlyIO
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ToolSchemaProperty {
    name: &'static str,
    property_type: ToolSchemaPropertyType,
    description: &'static str,
    required: bool,
}

impl ToolSchemaProperty {
    /// Creates a parameter property contract used by Brioche tool schemas.
    ///
    /// Refs: I-Shell-Runtime-OnlyIO
    ///
    /// # Complexity
    /// O(1). Stores borrowed static metadata; no heap allocation.
    ///
    /// # Panic / Safety
    /// Never panics.
    pub const fn new(
        name: &'static str,
        property_type: ToolSchemaPropertyType,
        description: &'static str,
        required: bool,
    ) -> Self {
        Self {
            name,
            property_type,
            description,
            required,
        }
    }
}

/// Builds the JSON Schema object for a tool parameter list.
///
/// Refs: I-Shell-Runtime-OnlyIO
///
/// # Complexity
/// O(P) where P is the number of properties. Allocates one JSON object per property
/// plus the top-level schema object and the required-field array when non-empty.
///
/// # Panic / Safety
/// Never panics.
pub fn tool_parameters_schema(properties: &[ToolSchemaProperty]) -> serde_json::Value {
    let mut props = serde_json::Map::new();
    let mut required = Vec::new();

    for property in properties {
        let mut prop = serde_json::Map::new();
        prop.insert(
            "type".into(),
            serde_json::Value::String(property.property_type.as_json_type().into()),
        );
        prop.insert(
            "description".into(),
            serde_json::Value::String(property.description.into()),
        );
        props.insert(property.name.into(), serde_json::Value::Object(prop));
        if property.required {
            required.push(serde_json::Value::String(property.name.into()));
        }
    }

    let mut schema = serde_json::Map::new();
    schema.insert("type".into(), serde_json::Value::String("object".into()));
    schema.insert("properties".into(), serde_json::Value::Object(props));
    if !required.is_empty() {
        schema.insert("required".into(), serde_json::Value::Array(required));
    }
    serde_json::Value::Object(schema)
}

/// Wraps a parameter schema in the OpenAI function-tool object shape.
///
/// Refs: I-Shell-Runtime-OnlyIO
///
/// # Complexity
/// O(1) plus ownership of the provided parameter schema. Allocates two JSON objects
/// and does not clone `parameters`.
///
/// # Panic / Safety
/// Never panics.
pub fn openai_function_tool_schema(
    name: impl Into<String>,
    description: impl Into<String>,
    parameters: serde_json::Value,
) -> serde_json::Value {
    let mut function = serde_json::Map::new();
    function.insert("name".into(), serde_json::Value::String(name.into()));
    function.insert(
        "description".into(),
        serde_json::Value::String(description.into()),
    );
    function.insert("parameters".into(), parameters);

    let mut obj = serde_json::Map::new();
    obj.insert("type".into(), serde_json::Value::String("function".into()));
    obj.insert("function".into(), serde_json::Value::Object(function));
    serde_json::Value::Object(obj)
}

/// Builds an OpenAI function-tool object directly from typed parameter metadata.
///
/// Refs: I-Shell-Runtime-OnlyIO
///
/// # Complexity
/// O(P) where P is the number of properties. Allocates the parameter schema and
/// wrapper objects once.
///
/// # Panic / Safety
/// Never panics.
pub fn openai_function_tool(
    name: impl Into<String>,
    description: impl Into<String>,
    properties: &[ToolSchemaProperty],
) -> serde_json::Value {
    openai_function_tool_schema(name, description, tool_parameters_schema(properties))
}

#[cfg(test)]
mod tests {
    use super::{
        ToolSchemaProperty, ToolSchemaPropertyType, openai_function_tool, tool_parameters_schema,
    };

    #[test]
    fn tool_parameters_schema_preserves_string_boolean_integer_shape() {
        let schema = tool_parameters_schema(&[
            ToolSchemaProperty::new("path", ToolSchemaPropertyType::String, "File path", true),
            ToolSchemaProperty::new(
                "append",
                ToolSchemaPropertyType::Boolean,
                "Append flag",
                false,
            ),
            ToolSchemaProperty::new(
                "timeout_ms",
                ToolSchemaPropertyType::Integer,
                "Timeout",
                false,
            ),
        ]);

        assert_eq!(schema["type"], "object");
        assert_eq!(schema["properties"]["path"]["type"], "string");
        assert_eq!(schema["properties"]["append"]["type"], "boolean");
        assert_eq!(schema["properties"]["timeout_ms"]["type"], "integer");
        assert_eq!(
            schema["required"],
            serde_json::Value::Array(vec![serde_json::Value::String("path".into())])
        );
    }

    #[test]
    fn openai_function_tool_wraps_parameters_without_extra_shape() {
        let tool = openai_function_tool(
            "fetch_url",
            "Fetch URL",
            &[ToolSchemaProperty::new(
                "url",
                ToolSchemaPropertyType::String,
                "URL to fetch",
                true,
            )],
        );

        assert_eq!(tool["type"], "function");
        assert_eq!(tool["function"]["name"], "fetch_url");
        assert_eq!(
            tool["function"]["parameters"]["required"],
            serde_json::Value::Array(vec![serde_json::Value::String("url".into())])
        );
    }
}
