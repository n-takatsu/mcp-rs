//! Helper macros for plugin development

/// Create a Tool with schema validation
///
/// # Examples
///
/// ```rust
/// use mcp_rs_sdk::prelude::*;
///
/// let tool = tool!("get_weather", "Get current weather", {
///     "location": {
///         "type": "string",
///         "description": "City name"
///     },
///     "units": {
///         "type": "string",
///         "enum": ["celsius", "fahrenheit"],
///         "default": "celsius"
///     }
/// });
/// ```
#[macro_export]
macro_rules! tool {
    ($name:expr, $description:expr, $schema:tt) => {
        $crate::core::Tool {
            name: $name.to_string(),
            description: $description.to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": $schema,
                "additionalProperties": false
            }),
        }
    };
}

/// Create a Resource definition
///
/// # Examples
///
/// ```rust
/// use mcp_rs_sdk::prelude::*;
///
/// let resource = resource!(
///     "file://config.json",
///     "Configuration File",
///     "Application configuration in JSON format",
///     "application/json"
/// );
/// ```
#[macro_export]
macro_rules! resource {
    ($uri:expr, $name:expr, $description:expr, $mime_type:expr) => {
        $crate::core::Resource {
            uri: $uri.to_string(),
            name: $name.to_string(),
            description: Some($description.to_string()),
            mime_type: Some($mime_type.to_string()),
        }
    };

    ($uri:expr, $name:expr) => {
        $crate::core::Resource {
            uri: $uri.to_string(),
            name: $name.to_string(),
            description: None,
            mime_type: None,
        }
    };
}

/// Create a Prompt definition
///
/// # Examples
///
/// ```rust
/// use mcp_rs_sdk::prelude::*;
///
/// let prompt = prompt!("summarize", "Summarize the given text", [
///     ("text", "Text to summarize", true),
///     ("max_length", "Maximum summary length", false)
/// ]);
/// ```
#[macro_export]
macro_rules! prompt {
    ($name:expr, $description:expr, [$( ($arg_name:expr, $arg_desc:expr, $required:expr) ),*]) => {
        $crate::core::Prompt {
            name: $name.to_string(),
            description: $description.to_string(),
            arguments: Some(vec![
                $(
                    $crate::core::PromptArgument {
                        name: $arg_name.to_string(),
                        description: $arg_desc.to_string(),
                        required: Some($required),
                    }
                ),*
            ]),
        }
    };

    ($name:expr, $description:expr) => {
        $crate::core::Prompt {
            name: $name.to_string(),
            description: $description.to_string(),
            arguments: None,
        }
    };
}

/// Create a successful ToolCallResult
///
/// # Examples
///
/// ```rust
/// use mcp_rs_sdk::prelude::*;
///
/// let result = tool_result!("Operation completed successfully");
/// let result = tool_result!(json!({"status": "ok", "count": 42}));
/// ```
#[macro_export]
macro_rules! tool_result {
    ($content:expr) => {
        serde_json::to_value($crate::core::ToolCallResult {
            content: vec![$crate::core::Content::Text {
                text: $content.to_string(),
            }],
            is_error: Some(false),
        })
        .unwrap_or_else(|_| serde_json::Value::String(format!("Serialization error: {}", $content)))
    };

    (json $content:expr) => {
        serde_json::to_value($crate::core::ToolCallResult {
            content: vec![$crate::core::Content::Text {
                text: serde_json::to_string_pretty(&$content)
                    .unwrap_or_else(|_| "JSON serialization failed".to_string()),
            }],
            is_error: Some(false),
        })
        .unwrap_or_else(|_| {
            serde_json::Value::String("Tool result serialization failed".to_string())
        })
    };
}

/// Create an error ToolCallResult
///
/// # Examples
///
/// ```rust
/// use mcp_rs_sdk::prelude::*;
///
/// let result = tool_error!("Operation failed: invalid input");
/// ```
#[macro_export]
macro_rules! tool_error {
    ($message:expr) => {
        serde_json::to_value($crate::core::ToolCallResult {
            content: vec![$crate::core::Content::Text {
                text: format!("Error: {}", $message),
            }],
            is_error: Some(true),
        })
        .unwrap_or_else(|_| {
            serde_json::Value::String(format!("Error: {} (plus serialization failed)", $message))
        })
    };
}

/// Create a ResourceReadResult
///
/// # Examples
///
/// ```rust
/// use mcp_rs_sdk::prelude::*;
///
/// let result = resource_result!("file://data.json", "application/json", r#"{"key": "value"}"#);
/// ```
#[macro_export]
macro_rules! resource_result {
    ($uri:expr, $mime_type:expr, $content:expr) => {
        serde_json::to_value($crate::core::ResourceReadResult {
            contents: vec![$crate::core::ResourceContent {
                uri: $uri.to_string(),
                mime_type: Some($mime_type.to_string()),
                text: Some($content.to_string()),
                blob: None,
            }],
        })
        .unwrap_or_else(|_| {
            serde_json::Value::String(format!("Resource result serialization failed for {}", $uri))
        })
    };

    (blob $uri:expr, $mime_type:expr, $blob:expr) => {
        serde_json::to_value($crate::core::ResourceReadResult {
            contents: vec![$crate::core::ResourceContent {
                uri: $uri.to_string(),
                mime_type: Some($mime_type.to_string()),
                text: None,
                blob: Some($blob.to_string()),
            }],
        })
        .unwrap_or_else(|_| {
            serde_json::Value::String(format!(
                "Resource blob result serialization failed for {}",
                $uri
            ))
        })
    };
}

/// Extract and validate parameter from tool arguments
///
/// # Examples
///
/// ```rust
/// use mcp_rs_sdk::prelude::*;
///
/// async fn call_tool(&self, name: &str, args: Option<HashMap<String, Value>>) -> PluginResult<Value> {
///     let args = args.unwrap_or_default();
///     let location = extract_param!(args, "location", as_str, "Missing location parameter")?;
///     let units = extract_param!(args, "units", as_str).unwrap_or("celsius");
///
///     // Use location and units...
///     Ok(tool_result!("Weather data"))
/// }
/// ```
#[macro_export]
macro_rules! extract_param {
    ($args:expr, $key:expr, $method:ident, $error:expr) => {
        $args.get($key).and_then(|v| v.$method()).ok_or_else(|| {
            $crate::core::McpError::InvalidParams {
                message: $error.to_string(),
            }
        })
    };

    ($args:expr, $key:expr, $method:ident) => {
        $args.get($key).and_then(|v| v.$method())
    };
}

/// Plugin factory creation helper
///
/// # Examples
///
/// ```rust
/// use mcp_rs_sdk::prelude::*;
///
/// plugin_factory!(MyPlugin, "my_plugin", || {
///     Box::new(MyPlugin::new())
/// });
/// ```
#[macro_export]
macro_rules! plugin_factory {
    ($plugin_type:ty, $name:expr, $constructor:expr) => {
        pub struct Factory;

        impl $crate::plugins::PluginFactory for Factory {
            fn create(&self) -> Box<dyn $crate::plugins::Plugin> {
                $constructor()
            }

            fn name(&self) -> &str {
                $name
            }
        }

        pub fn factory() -> Factory {
            Factory
        }
    };
}
