//! 1つのmcp-rsで、複数のWordPressのサイトを使う（#640）
//!
//! ツールは1つのサイトのときと同じ名前・同じ数のまま、引数`site`で
//! サイトを選ぶ（省けば最初のサイト）。サイトごとに`WordPressHandler`を
//! 作るので、資格情報・レート制限・監査ログ（サイトのURLを記録する）は
//! サイトごとに分かれる。リソース（`wordpress://posts`等）は、最初のサイト
//! だけ

use async_trait::async_trait;
use std::sync::Arc;

use crate::mcp::{
    InitializeParams, McpError, McpHandler, Resource, ResourceReadParams, Tool, ToolCallParams,
};

/// ツールの引数で、サイトを選ぶ名前
pub const SITE_ARGUMENT: &str = "site";

/// 複数のWordPressのサイト。最初のサイトが既定
pub struct WordPressSites {
    sites: Vec<(String, Arc<dyn McpHandler>)>,
}

impl WordPressSites {
    /// `sites`は（名前、ハンドラー）。最初が既定のサイト。空にはできない
    pub fn new(sites: Vec<(String, Arc<dyn McpHandler>)>) -> Self {
        assert!(!sites.is_empty(), "WordPressSites needs at least one site");
        Self { sites }
    }

    fn default_site(&self) -> &Arc<dyn McpHandler> {
        &self.sites[0].1
    }

    fn names(&self) -> Vec<&str> {
        self.sites.iter().map(|(name, _)| name.as_str()).collect()
    }

    /// 引数`site`のサイトを選び、引数から`site`を外す
    fn select(&self, params: &mut ToolCallParams) -> Result<&Arc<dyn McpHandler>, McpError> {
        let Some(value) = params
            .arguments
            .as_mut()
            .and_then(|args| args.remove(SITE_ARGUMENT))
        else {
            return Ok(self.default_site());
        };
        if value.is_null() {
            return Ok(self.default_site());
        }
        let name = value.as_str().unwrap_or_default();
        self.sites
            .iter()
            .find(|(site, _)| site == name)
            .map(|(_, handler)| handler)
            .ok_or_else(|| {
                McpError::InvalidParams(format!("site must be one of: {}", self.names().join(", ")))
            })
    }

    /// ツールに、引数`site`を足す
    fn with_site_argument(&self, mut tool: Tool) -> Tool {
        let names = self.names();
        let description = format!(
            "WordPress site to use: {}. Defaults to '{}'",
            names.join(", "),
            names[0]
        );
        if let Some(schema) = tool.input_schema.as_object_mut() {
            let properties = schema
                .entry("properties")
                .or_insert_with(|| serde_json::json!({}));
            if let Some(properties) = properties.as_object_mut() {
                properties.insert(
                    SITE_ARGUMENT.to_string(),
                    serde_json::json!({
                        "type": "string",
                        "enum": names,
                        "description": description,
                    }),
                );
            }
        }
        tool
    }
}

#[async_trait]
impl McpHandler for WordPressSites {
    async fn initialize(&self, params: InitializeParams) -> Result<serde_json::Value, McpError> {
        self.default_site().initialize(params).await
    }

    async fn list_tools(&self) -> Result<Vec<Tool>, McpError> {
        Ok(self
            .default_site()
            .list_tools()
            .await?
            .into_iter()
            .map(|tool| self.with_site_argument(tool))
            .collect())
    }

    async fn call_tool(&self, mut params: ToolCallParams) -> Result<serde_json::Value, McpError> {
        let handler = self.select(&mut params)?.clone();
        handler.call_tool(params).await
    }

    async fn list_resources(&self) -> Result<Vec<Resource>, McpError> {
        self.default_site().list_resources().await
    }

    async fn read_resource(
        &self,
        params: ResourceReadParams,
    ) -> Result<serde_json::Value, McpError> {
        self.default_site().read_resource(params).await
    }

    async fn status(&self) -> Option<String> {
        let mut lines = Vec::new();
        for (name, handler) in &self.sites {
            let status = handler
                .status()
                .await
                .unwrap_or_else(|| "WordPress site".to_string());
            lines.push(format!("{name}: {status}"));
        }
        Some(format!("WordPress sites ({})", lines.join("; ")))
    }

    /// プロンプトに、サイトを選ぶ引数`site`を足す（#643）
    fn prompts(&self) -> Vec<crate::mcp::PromptSpec> {
        let names = self.names();
        self.default_site()
            .prompts()
            .into_iter()
            .map(|mut spec| {
                spec.prompt.arguments.push(crate::mcp::PromptArgument {
                    name: SITE_ARGUMENT.to_string(),
                    description: Some(format!(
                        "WordPress site: {}. Defaults to '{}'",
                        names.join(", "),
                        names[0]
                    )),
                    required: false,
                });
                spec
            })
            .collect()
    }

    async fn get_prompt(
        &self,
        name: &str,
        arguments: &std::collections::HashMap<String, String>,
    ) -> Result<serde_json::Value, McpError> {
        let site = arguments
            .get(SITE_ARGUMENT)
            .map(|s| s.trim())
            .filter(|s| !s.is_empty());
        if let Some(site) = site {
            if !self.names().contains(&site) {
                return Err(McpError::InvalidParams(format!(
                    "site must be one of: {}",
                    self.names().join(", ")
                )));
            }
        }
        let mut result = self.default_site().get_prompt(name, arguments).await?;
        if let Some(site) = site {
            if let Some(text) = result["messages"][0]["content"]["text"].as_str() {
                let text = format!(
                    "Use the WordPress site \"{site}\": pass site \"{site}\" to every tool call.

{text}"
                );
                result["messages"][0]["content"]["text"] = serde_json::json!(text);
            }
        }
        Ok(result)
    }

    async fn list_resource_templates(&self) -> Result<Vec<crate::mcp::ResourceTemplate>, McpError> {
        self.default_site().list_resource_templates().await
    }

    fn resource_tool(&self, uri: &str) -> Option<String> {
        self.default_site().resource_tool(uri)
    }

    fn trusts_read_only_hints(&self) -> bool {
        self.sites
            .iter()
            .all(|(_, handler)| handler.trusts_read_only_hints())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    /// 呼ばれたツールと引数を、サイトの名前と一緒に返すハンドラー
    struct Echo(&'static str);

    #[async_trait]
    impl McpHandler for Echo {
        async fn initialize(&self, _: InitializeParams) -> Result<serde_json::Value, McpError> {
            Ok(serde_json::json!({}))
        }
        async fn list_tools(&self) -> Result<Vec<Tool>, McpError> {
            Ok(vec![
                Tool {
                    name: "get_posts".to_string(),
                    input_schema: serde_json::json!({
                        "type": "object",
                        "properties": {"search": {"type": "string"}}
                    }),
                    ..Default::default()
                },
                Tool {
                    name: "wordpress_health_check".to_string(),
                    input_schema: serde_json::json!({"type": "object"}),
                    ..Default::default()
                },
            ])
        }
        async fn call_tool(&self, params: ToolCallParams) -> Result<serde_json::Value, McpError> {
            Ok(serde_json::json!({
                "site": self.0,
                "tool": params.name,
                "arguments": params.arguments,
            }))
        }
        async fn list_resources(&self) -> Result<Vec<Resource>, McpError> {
            Ok(Vec::new())
        }
        async fn read_resource(
            &self,
            _: ResourceReadParams,
        ) -> Result<serde_json::Value, McpError> {
            Ok(serde_json::json!({ "site": self.0 }))
        }
        async fn status(&self) -> Option<String> {
            Some(format!("WordPress site https://{}.example.com", self.0))
        }
    }

    fn sites() -> WordPressSites {
        WordPressSites::new(vec![
            ("blog".to_string(), Arc::new(Echo("blog"))),
            ("shop".to_string(), Arc::new(Echo("shop"))),
        ])
    }

    fn call(args: serde_json::Value) -> ToolCallParams {
        ToolCallParams {
            name: "get_posts".to_string(),
            arguments: Some(serde_json::from_value::<HashMap<_, _>>(args).unwrap()),
        }
    }

    #[tokio::test]
    async fn test_site_selects_the_handler_and_is_removed() {
        let sites = sites();
        let result = sites
            .call_tool(call(serde_json::json!({"site": "shop", "search": "sale"})))
            .await
            .unwrap();
        assert_eq!(result["site"], "shop");
        assert_eq!(result["arguments"], serde_json::json!({"search": "sale"}));
    }

    #[tokio::test]
    async fn test_without_site_uses_the_first_site() {
        let sites = sites();
        for args in [
            serde_json::json!({"search": "rust"}),
            serde_json::json!({"site": null}),
        ] {
            let result = sites.call_tool(call(args)).await.unwrap();
            assert_eq!(result["site"], "blog");
        }
        let result = sites
            .call_tool(ToolCallParams {
                name: "wordpress_health_check".to_string(),
                arguments: None,
            })
            .await
            .unwrap();
        assert_eq!(result["site"], "blog");
    }

    #[tokio::test]
    async fn test_unknown_site_is_refused() {
        let sites = sites();
        for site in [serde_json::json!("other"), serde_json::json!(1)] {
            let result = sites
                .call_tool(call(serde_json::json!({"site": site})))
                .await;
            assert!(
                matches!(result, Err(McpError::InvalidParams(ref msg)) if msg == "site must be one of: blog, shop"),
                "{:?}",
                result
            );
        }
    }

    #[tokio::test]
    async fn test_every_tool_gets_the_site_argument() {
        let tools = sites().list_tools().await.unwrap();
        assert_eq!(tools.len(), 2);
        for tool in tools {
            let site = &tool.input_schema["properties"]["site"];
            assert_eq!(
                site["enum"],
                serde_json::json!(["blog", "shop"]),
                "{}",
                tool.name
            );
            assert!(site["description"]
                .as_str()
                .unwrap()
                .contains("Defaults to 'blog'"));
        }
    }

    #[tokio::test]
    async fn test_status_lists_every_site_and_resources_use_the_first() {
        let sites = sites();
        assert_eq!(
            sites.status().await.unwrap(),
            "WordPress sites (blog: WordPress site https://blog.example.com; shop: WordPress site https://shop.example.com)"
        );
        let resource = sites
            .read_resource(ResourceReadParams {
                uri: "wordpress://posts".to_string(),
            })
            .await
            .unwrap();
        assert_eq!(resource["site"], "blog");
    }
}
