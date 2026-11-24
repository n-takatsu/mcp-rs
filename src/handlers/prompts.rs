//! mcp-rsのプロンプト（#643）
//!
//! プロンプトは、クライアントのメニュー（Claude Desktopの「+」等）から選ぶ、
//! 定型の頼み方。AIへの頼み（`user`の発言）の文を返すだけで、ツールは
//! 呼ばない。どのプロンプトも、サイト・データベースを変える前に、利用者に
//! 確かめるよう頼む。ただし、`draft_post`の下書きの作成は、利用者が選んだ
//! 頼みそのものなので確かめない（公開・予約はしない。分類を新しく作る前には
//! 確かめる。#774）。文は英語で、利用者の言葉で答えるよう頼む

use std::collections::HashMap;

use crate::mcp::{prompt_result, McpError, Prompt, PromptArgument, PromptSpec};

/// プロンプトの引数の長さの上限（文字）
const MAX_ARGUMENT_CHARS: usize = 2000;

/// どのプロンプトにも付ける、答えの言葉の頼み
const REPLY_IN_USER_LANGUAGE: &str = "Reply in the language I use.";

fn argument(name: &str, description: &str, required: bool) -> PromptArgument {
    PromptArgument {
        name: name.to_string(),
        description: Some(description.to_string()),
        required,
    }
}

fn spec(
    name: &str,
    title: &str,
    description: &str,
    arguments: Vec<PromptArgument>,
    tools: &[&str],
) -> PromptSpec {
    PromptSpec {
        prompt: Prompt {
            name: name.to_string(),
            title: Some(title.to_string()),
            description: Some(description.to_string()),
            arguments,
        },
        tools: tools.iter().map(|t| t.to_string()).collect(),
    }
}

/// 引数を読む。無いか空なら`None`。長すぎれば断る
fn text_arg<'a>(
    arguments: &'a HashMap<String, String>,
    name: &str,
) -> Result<Option<&'a str>, McpError> {
    let Some(value) = arguments
        .get(name)
        .map(|v| v.trim())
        .filter(|v| !v.is_empty())
    else {
        return Ok(None);
    };
    if value.chars().count() > MAX_ARGUMENT_CHARS {
        return Err(McpError::InvalidParams(format!(
            "The argument '{name}' is longer than {MAX_ARGUMENT_CHARS} characters"
        )));
    }
    Ok(Some(value))
}

/// 投稿のID等、正の整数の引数
fn id_arg(arguments: &HashMap<String, String>, name: &str) -> Result<Option<u64>, McpError> {
    match text_arg(arguments, name)? {
        None => Ok(None),
        Some(value) => value
            .parse::<u64>()
            .ok()
            .filter(|id| *id > 0)
            .map(Some)
            .ok_or_else(|| {
                McpError::InvalidParams(format!("The argument '{name}' must be a positive number"))
            }),
    }
}

/// WordPressのプロンプト
pub fn wordpress_prompts() -> Vec<PromptSpec> {
    vec![
        spec(
            "draft_post",
            "Draft a blog post",
            "Write a draft post on a topic, with categories and tags from the site. The post is created as a draft and not published",
            vec![
                argument("topic", "What the post is about", true),
                argument("notes", "Points to include, audience, tone or length", false),
            ],
            &["get_categories", "get_tags", "create_advanced_post"],
        ),
        spec(
            "review_post_seo",
            "Review a post's SEO",
            "Check a post's title, slug, excerpt, headings and links, and suggest improvements. The post is changed only after you approve",
            vec![argument("post_id", "ID of the post to review", true)],
            &["get_post"],
        ),
        spec(
            "moderate_comments",
            "Moderate comments",
            "Go through the comments awaiting moderation and suggest whether to approve, mark as spam, trash or reply to each. Nothing is changed until you decide",
            vec![argument(
                "post_id",
                "Only the comments on this post (optional)",
                false,
            )],
            &["get_comments"],
        ),
    ]
}

/// WordPressのプロンプトの文
pub fn wordpress_prompt(
    name: &str,
    arguments: &HashMap<String, String>,
) -> Result<serde_json::Value, McpError> {
    let description = wordpress_prompts()
        .into_iter()
        .find(|spec| spec.prompt.name == name)
        .and_then(|spec| spec.prompt.description)
        .ok_or_else(|| McpError::InvalidParams(format!("Unknown prompt: {name}")))?;
    let text = match name {
        "draft_post" => {
            let topic = text_arg(arguments, "topic")?
                .ok_or_else(|| McpError::InvalidParams("Missing topic".to_string()))?;
            let notes = text_arg(arguments, "notes")?
                .map(|notes| format!("\nNotes: {notes}\n"))
                .unwrap_or_default();
            format!(
                "Write a draft post for my WordPress site.\n\nTopic: {topic}\n{notes}\n\
                 1. Read the existing categories and tags with get_categories and get_tags, and choose from them. If none fits, suggest new ones and ask me before creating any.\n\
                 2. Write a title, the content and a short excerpt.\n\
                 3. Create the post with create_advanced_post and status \"draft\". Do not publish or schedule it.\n\
                 4. Tell me the draft's ID, title, slug, categories and tags, and suggest a featured image. Do not upload images unless I give them to you.\n\n\
                 {REPLY_IN_USER_LANGUAGE}"
            )
        }
        "review_post_seo" => {
            let post_id = id_arg(arguments, "post_id")?
                .ok_or_else(|| McpError::InvalidParams("Missing post_id".to_string()))?;
            format!(
                "Review the SEO of post {post_id} on my WordPress site.\n\n\
                 1. Read it with get_post (format \"raw\").\n\
                 2. Check the title (about 30 to 60 characters, the main keyword near the start), the slug (short and readable), the excerpt used as the meta description (about 120 characters), the heading structure, internal links, image alt text and readability.\n\
                 3. List concrete improvements, with a proposed title, slug and excerpt.\n\n\
                 Do not change the post. If I approve changes, make them with update_post and keep its status as it is.\n\n\
                 {REPLY_IN_USER_LANGUAGE}"
            )
        }
        "moderate_comments" => {
            let (on_post, filter) = match id_arg(arguments, "post_id")? {
                Some(id) => (format!(" on post {id}"), format!(" and post_id {id}")),
                None => (String::new(), String::new()),
            };
            format!(
                "Go through the comments awaiting moderation{on_post} on my WordPress site.\n\n\
                 1. Read them with get_comments (status \"hold\"{filter}).\n\
                 2. For each comment, suggest one of: approve, mark as spam, move to trash, or reply (with a draft reply), and say briefly why.\n\
                 3. Wait for my decision. Then change the comments with update_comment and write replies with create_comment (replies are held for moderation by default).\n\n\
                 Never approve, reply to or delete a comment without my confirmation.\n\n\
                 {REPLY_IN_USER_LANGUAGE}"
            )
        }
        _ => return Err(McpError::InvalidParams(format!("Unknown prompt: {name}"))),
    };
    Ok(prompt_result(&description, text))
}

/// データベースのプロンプト
pub fn database_prompts() -> Vec<PromptSpec> {
    vec![spec(
        "summarize_table",
        "Summarize a table",
        "Describe what a database table holds: its columns, number of rows and how values are distributed. Only reads, with small queries",
        vec![argument("table", "Name of the table", true)],
        &["get_schema", "execute_query"],
    )]
}

/// データベースのプロンプトの文
pub fn database_prompt(
    name: &str,
    arguments: &HashMap<String, String>,
) -> Result<serde_json::Value, McpError> {
    if name != "summarize_table" {
        return Err(McpError::InvalidParams(format!("Unknown prompt: {name}")));
    }
    let table = text_arg(arguments, "table")?
        .ok_or_else(|| McpError::InvalidParams("Missing table".to_string()))?;
    let description = database_prompts()[0]
        .prompt
        .description
        .clone()
        .unwrap_or_default();
    let text = format!(
        "Summarize the contents of the database table {table}.\n\n\
         1. Read its columns, keys and indexes with get_schema.\n\
         2. Use execute_query with small SELECT queries: counts, MIN and MAX, COUNT(DISTINCT ...), and GROUP BY on columns with few distinct values, each with a LIMIT. Do not read the whole table.\n\
         3. Report the number of rows, what each column holds, the ranges and distributions of values, empty values, and anything unusual.\n\n\
         Do not run execute_command. Do not quote personal data (such as names or email addresses) in the summary; describe it instead.\n\n\
         {REPLY_IN_USER_LANGUAGE}"
    );
    Ok(prompt_result(&description, text))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(pairs: &[(&str, &str)]) -> HashMap<String, String> {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    fn text(result: &serde_json::Value) -> &str {
        result["messages"][0]["content"]["text"].as_str().unwrap()
    }

    #[test]
    fn test_draft_post_keeps_the_post_a_draft() {
        let result = wordpress_prompt(
            "draft_post",
            &args(&[("topic", "Rustで作るMCPサーバー"), ("notes", "初心者向け")]),
        )
        .unwrap();
        assert_eq!(result["messages"][0]["role"], "user");
        let text = text(&result);
        assert!(text.contains("Topic: Rustで作るMCPサーバー"), "{text}");
        assert!(text.contains("Notes: 初心者向け"), "{text}");
        assert!(text.contains("status \"draft\". Do not publish"), "{text}");
        assert!(text.ends_with(REPLY_IN_USER_LANGUAGE), "{text}");
    }

    #[test]
    fn test_review_post_seo_needs_a_post_id() {
        let result = wordpress_prompt("review_post_seo", &args(&[("post_id", "483")])).unwrap();
        assert!(text(&result).starts_with("Review the SEO of post 483"));
        for value in ["abc", "0", "-1"] {
            let err =
                wordpress_prompt("review_post_seo", &args(&[("post_id", value)])).unwrap_err();
            assert!(
                err.to_string().contains("must be a positive number"),
                "{value}"
            );
        }
    }

    #[test]
    fn test_moderate_comments_with_and_without_a_post() {
        let all = wordpress_prompt("moderate_comments", &args(&[])).unwrap();
        assert!(text(&all).contains("(status \"hold\")"));
        let one = wordpress_prompt("moderate_comments", &args(&[("post_id", "7")])).unwrap();
        assert!(text(&one).contains("(status \"hold\" and post_id 7)"));
        assert!(text(&one).contains("Never approve"));
    }

    #[test]
    fn test_summarize_table_reads_only() {
        let result = database_prompt("summarize_table", &args(&[("table", "orders")])).unwrap();
        let text = text(&result);
        assert!(text.starts_with("Summarize the contents of the database table orders."));
        assert!(text.contains("Do not run execute_command"));
        assert!(database_prompt("other", &args(&[])).is_err());
    }

    #[test]
    fn test_long_arguments_are_refused() {
        let long = "a".repeat(MAX_ARGUMENT_CHARS + 1);
        let err = wordpress_prompt("draft_post", &args(&[("topic", &long)])).unwrap_err();
        assert!(err.to_string().contains("longer than"));
    }

    /// プロンプトの名前は重ならず、使うツールは実在する名前
    #[test]
    fn test_prompt_names_and_tools() {
        let mut names: Vec<String> = wordpress_prompts()
            .into_iter()
            .chain(database_prompts())
            .map(|spec| spec.prompt.name)
            .collect();
        let count = names.len();
        names.dedup();
        assert_eq!(names.len(), count);
        for spec in wordpress_prompts().into_iter().chain(database_prompts()) {
            assert!(!spec.tools.is_empty(), "{}", spec.prompt.name);
        }
    }
}
