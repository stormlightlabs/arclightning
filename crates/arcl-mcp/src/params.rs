//! Tool arguments that fail with the CLI's failure envelope.
//!
//! rmcp's own `Parameters` extractor turns an argument it cannot decode, such
//! as an unknown field or a string where a number belongs, into a protocol
//! error with no structured content. [`Parameters`] keeps the decode result
//! instead, so the tool returns it as an `invalid_request` [`ToolError`] like
//! any other failure.

use rmcp::{
    handler::server::{common::FromContextPart, tool::ToolCallContext},
    model::JsonObject,
};
use schemars::JsonSchema;
use serde::de::DeserializeOwned;

use crate::error::ToolError;

/// A tool's arguments, decoded into `P`, or the `invalid_request` failure the
/// tool returns when they do not decode.
///
/// The `#[tool]` macro reads the input schema from a parameter whose type is
/// named `Parameters`, so this type keeps that name.
#[derive(Debug)]
pub struct Parameters<P>(pub Result<P, ToolError>);

impl<P: DeserializeOwned> Parameters<P> {
    /// Decode a tool call's `arguments` object. Absent arguments decode as
    /// an empty object.
    pub fn from_arguments(arguments: Option<JsonObject>) -> Self {
        let arguments = serde_json::Value::Object(arguments.unwrap_or_default());
        Self(
            serde_json::from_value(arguments)
                .map_err(|error| ToolError::invalid_request(format!("invalid tool arguments: {error}"))),
        )
    }
}

impl<P> From<P> for Parameters<P> {
    fn from(input: P) -> Self {
        Self(Ok(input))
    }
}

impl<P: JsonSchema> JsonSchema for Parameters<P> {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        P::schema_name()
    }

    fn json_schema(generator: &mut schemars::SchemaGenerator) -> schemars::Schema {
        P::json_schema(generator)
    }
}

impl<S, P: DeserializeOwned> FromContextPart<ToolCallContext<'_, S>> for Parameters<P> {
    fn from_context_part(context: &mut ToolCallContext<S>) -> Result<Self, rmcp::ErrorData> {
        Ok(Self::from_arguments(context.arguments.take()))
    }
}
