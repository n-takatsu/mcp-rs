use async_trait::async_trait;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use tracing::{info, warn};

use crate::core::{Tool, Resource, McpError};
use crate::config::PluginConfig;
use crate::plugins::{Plugin, PluginMetadata, PluginResult, ToolProvider, ResourceProvider, PluginFactory, UnifiedPlugin, PluginCapability};
use crate::sdk::prelude::*;

/// GitHub plugin configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitHubConfig {
    /// GitHub token for authentication
    pub token: String,
    
    /// Default owner/organization
    pub owner: String,
    
    /// Default repositories to work with
    pub repos: Vec<String>,
    
    /// Request timeout in seconds
    pub timeout: Option<u64>,
    
    /// GitHub API base URL (for enterprise)
    pub base_url: Option<String>,
}

/// GitHub API models
#[derive(Debug, Serialize, Deserialize)]
pub struct GitHubRepo {
    pub id: u64,
    pub name: String,
    pub full_name: String,
    pub description: Option<String>,
    pub private: bool,
    pub html_url: String,
    pub clone_url: String,
    pub ssh_url: String,
    pub default_branch: String,
    pub language: Option<String>,
    pub stargazers_count: u32,
    pub forks_count: u32,
    pub open_issues_count: u32,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct GitHubIssue {
    pub id: u64,
    pub number: u32,
    pub title: String,
    pub body: Option<String>,
    pub state: String,
    pub user: GitHubUser,
    pub assignee: Option<GitHubUser>,
    pub labels: Vec<GitHubLabel>,
    pub html_url: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct GitHubUser {
    pub id: u64,
    pub login: String,
    pub avatar_url: String,
    pub html_url: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct GitHubLabel {
    pub id: u64,
    pub name: String,
    pub color: String,
    pub description: Option<String>,
}

/// GitHub plugin
pub struct GitHubPlugin {
    config: Option<GitHubConfig>,
    client: Client,
}

impl GitHubPlugin {
    pub fn new() -> Self {
        Self {
            config: None,
            client: Client::new(),
        }
    }
    
    fn get_config(&self) -> Result<&GitHubConfig, McpError> {
        self.config.as_ref()
            .ok_or_else(|| McpError::Other("GitHub plugin not initialized".to_string()))
    }
    
    fn get_base_url(&self) -> String {
        self.get_config()
            .ok()
            .and_then(|c| c.base_url.clone())
            .unwrap_or_else(|| "https://api.github.com".to_string())
    }
    
    async fn make_request(&self, endpoint: &str) -> Result<reqwest::Response, McpError> {
        let config = self.get_config()?;
        let url = format!("{}/{}", self.get_base_url().trim_end_matches('/'), endpoint);
        
        let mut request = self.client
            .get(&url)
            .header("Authorization", format!("token {}", config.token))
            .header("User-Agent", format!("mcp-rs/{}", env!("CARGO_PKG_VERSION")))
            .header("Accept", "application/vnd.github.v3+json");
        
        if let Some(timeout) = config.timeout {
            request = request.timeout(std::time::Duration::from_secs(timeout));
        }
        
        let response = request.send().await?;
        
        if !response.status().is_success() {
            return Err(McpError::ExternalApi(format!(
                "GitHub API error: {} - {}",
                response.status(),
                response.text().await.unwrap_or_default()
            )));
        }
        
        Ok(response)
    }
    
    async fn get_repositories(&self) -> Result<Vec<GitHubRepo>, McpError> {
        let config = self.get_config()?;
        let endpoint = format!("users/{}/repos", config.owner);
        
        let response = self.make_request(&endpoint).await?;
        let repos: Vec<GitHubRepo> = response.json().await?;
        
        Ok(repos)
    }
    
    async fn get_issues(&self, repo: &str, state: Option<&str>) -> Result<Vec<GitHubIssue>, McpError> {
        let config = self.get_config()?;
        let mut endpoint = format!("repos/{}/{}/issues", config.owner, repo);
        
        if let Some(state) = state {
            endpoint.push_str(&format!("?state={}", state));
        }
        
        let response = self.make_request(&endpoint).await?;
        let issues: Vec<GitHubIssue> = response.json().await?;
        
        Ok(issues)
    }
}

#[async_trait]
impl Plugin for GitHubPlugin {
    fn metadata(&self) -> PluginMetadata {
        PluginMetadata {
            name: "github".to_string(),
            version: "0.1.0".to_string(),
            description: "GitHub API integration for MCP".to_string(),
            author: "n-takatsu".to_string(),
            homepage: Some("https://github.com/n-takatsu/mcp-rs".to_string()),
            dependencies: vec!["http".to_string()],
        }
    }
    
    async fn initialize(&mut self, config: &PluginConfig) -> PluginResult {
        let github_config: GitHubConfig = ConfigUtils::load_plugin_config(config)?;
        
        info!("Initializing GitHub plugin for owner: {}", github_config.owner);
        self.config = Some(github_config);
        
        Ok(())
    }
    
    async fn shutdown(&mut self) -> PluginResult {
        info!("Shutting down GitHub plugin");
        self.config = None;
        Ok(())
    }
    
    async fn health_check(&self) -> PluginResult<bool> {
        if let Ok(_) = self.get_config() {
            match self.make_request("user").await {
                Ok(_) => Ok(true),
                Err(e) => {
                    warn!("GitHub health check failed: {}", e);
                    Ok(false)
                }
            }
        } else {
            Ok(false)
        }
    }
}

#[async_trait]
impl ToolProvider for GitHubPlugin {
    async fn list_tools(&self) -> PluginResult<Vec<Tool>> {
        Ok(vec![
            tool!("github_list_repos", "List GitHub repositories", {
                "owner": {
                    "type": "string",
                    "description": "Repository owner (optional, uses default if not specified)"
                }
            }),
            tool!("github_get_issues", "Get issues from a repository", {
                "repo": {
                    "type": "string", 
                    "description": "Repository name"
                },
                "state": {
                    "type": "string",
                    "enum": ["open", "closed", "all"],
                    "default": "open",
                    "description": "Issue state filter"
                }
            }),
            tool!("github_create_issue", "Create a new issue", {
                "repo": {
                    "type": "string",
                    "description": "Repository name"
                },
                "title": {
                    "type": "string",
                    "description": "Issue title"
                },
                "body": {
                    "type": "string",
                    "description": "Issue body"
                },
                "labels": {
                    "type": "array",
                    "items": {"type": "string"},
                    "description": "Issue labels"
                }
            }),
            tool!("github_search_repos", "Search repositories", {
                "query": {
                    "type": "string",
                    "description": "Search query"
                },
                "language": {
                    "type": "string",
                    "description": "Filter by programming language"
                }
            }),
        ])
    }
    
    async fn call_tool(&self, name: &str, arguments: Option<HashMap<String, Value>>) -> PluginResult<Value> {
        let args = arguments.unwrap_or_default();
        
        match name {
            "github_list_repos" => {
                let repos = self.get_repositories().await?;
                Ok(tool_result!(json {
                    "repositories": repos,
                    "count": repos.len()
                }))
            },
            
            "github_get_issues" => {
                let repo = extract_param!(args, "repo", as_str, "Missing repo parameter")?;
                let state = extract_param!(args, "state", as_str);
                
                let issues = self.get_issues(repo, state).await?;
                Ok(tool_result!(json {
                    "issues": issues,
                    "count": issues.len()
                }))
            },
            
            "github_create_issue" => {
                // Implementation would create an issue via POST
                Ok(tool_result!("Issue creation functionality would be implemented here"))
            },
            
            "github_search_repos" => {
                let query = extract_param!(args, "query", as_str, "Missing query parameter")?;
                let language = extract_param!(args, "language", as_str);
                
                let mut search_query = query.to_string();
                if let Some(lang) = language {
                    search_query.push_str(&format!(" language:{}", lang));
                }
                
                let endpoint = format!("search/repositories?q={}", urlencoding::encode(&search_query));
                let response = self.make_request(&endpoint).await?;
                let search_result: Value = response.json().await?;
                
                Ok(tool_result!(json search_result))
            },
            
            _ => Err(McpError::ToolNotFound(name.to_string())),
        }
    }
}

#[async_trait]
impl ResourceProvider for GitHubPlugin {
    async fn list_resources(&self) -> PluginResult<Vec<Resource>> {
        let config = self.get_config()?;
        let mut resources = vec![
            resource!(
                "github://repositories",
                "GitHub Repositories",
                "List of all repositories",
                "application/json"
            ),
        ];
        
        // Add repository-specific resources
        for repo in &config.repos {
            resources.push(resource!(
                &format!("github://repos/{}/issues", repo),
                &format!("{} Issues", repo),
                &format!("Issues from {} repository", repo),
                "application/json"
            ));
        }
        
        Ok(resources)
    }
    
    async fn read_resource(&self, uri: &str) -> PluginResult<Value> {
        match uri {
            "github://repositories" => {
                let repos = self.get_repositories().await?;
                Ok(resource_result!(uri, "application/json", serde_json::to_string_pretty(&repos)?))
            },
            
            uri if uri.starts_with("github://repos/") && uri.ends_with("/issues") => {
                let repo = uri.strip_prefix("github://repos/")
                    .and_then(|s| s.strip_suffix("/issues"))
                    .ok_or_else(|| McpError::ResourceNotFound(uri.to_string()))?;
                
                let issues = self.get_issues(repo, None).await?;
                Ok(resource_result!(uri, "application/json", serde_json::to_string_pretty(&issues)?))
            },
            
            _ => Err(McpError::ResourceNotFound(uri.to_string())),
        }
    }
}

#[async_trait]
impl UnifiedPlugin for GitHubPlugin {
    fn capabilities(&self) -> Vec<PluginCapability> {
        vec![PluginCapability::Tools, PluginCapability::Resources]
    }
    
    async fn list_tools(&self) -> PluginResult<Vec<Tool>> {
        ToolProvider::list_tools(self).await
    }
    
    async fn call_tool(&self, name: &str, arguments: Option<HashMap<String, Value>>) -> PluginResult<Value> {
        ToolProvider::call_tool(self, name, arguments).await
    }
    
    async fn list_resources(&self) -> PluginResult<Vec<Resource>> {
        ResourceProvider::list_resources(self).await
    }
    
    async fn read_resource(&self, uri: &str) -> PluginResult<Value> {
        ResourceProvider::read_resource(self, uri).await
    }
}

/// GitHub plugin factory
pub struct GitHubPluginFactory;

impl PluginFactory for GitHubPluginFactory {
    fn create(&self) -> Box<dyn UnifiedPlugin> {
        Box::new(GitHubPlugin::new())
    }
    
    fn name(&self) -> &str {
        "github"
    }
    
    fn capabilities(&self) -> Vec<PluginCapability> {
        vec![PluginCapability::Tools, PluginCapability::Resources]
    }
}