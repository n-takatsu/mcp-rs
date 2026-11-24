---
layout: page
title: WordPress Integration
permalink: /docs/wordpress/
nav_order: 4
---

# WordPress Integration

{: .no_toc }

**[Home]({{ site.baseurl }}/) > [Documentation]({{ site.baseurl }}/docs/) > WordPress Integration**

Complete WordPress REST API integration with advanced media management capabilities.
{: .fs-6 .fw-300 }

## Table of contents

{: .no_toc .text-delta }

1. TOC
{:toc}

---

## Overview

MCP-RS provides comprehensive WordPress integration through the WordPress REST API, enabling AI agents to perform content management tasks with full featured image support.

## Features

## 📝 Content Management

- **Advanced Post Creation**: Create posts and pages with comprehensive options
- **Post Type Support**: Both posts (blog articles) and pages (static content)
- **Status Control**: Draft, publish, private, and scheduled posts
- **SEO Integration**: Meta fields for Yoast SEO and other plugins
- **Post Retrieval**: Get all published posts with metadata
- **Complete CRUD**: Create, read, update, and delete operations
- **Comment Management**: List comments by status, approve, mark as spam, reply, and delete (v0.30.0 and later)

## 🖼️ Media Management

- **File Upload**: Upload images, documents, and media files
- **Featured Images**: Set and manage post featured images
- **Multiple Formats**: Support for JPEG, PNG, GIF, PDF, and more
- **Base64 or Local Files**: Upload base64-encoded data, or a local file from an allowed folder (`upload_allowed_dirs`)

## 🔐 Security

- **Application Passwords**: Secure authentication with WordPress
- **Timeout Handling**: Configurable request timeouts
- **Retry Logic**: Automatic retry for transient failures (up to 3 attempts). Create and delete requests are not resent after a timeout or server error, since WordPress may already have processed them
- **Error Handling**: Comprehensive error reporting

## 🏷️ Category & Tag Management

- **Category Operations**: Create, read, update, and delete categories
- **Tag Operations**: Create, read, update, and delete tags
- **Hierarchical Categories**: Support for parent-child category relationships
- **Bulk Operations**: Efficient management of multiple categories and tags

## ⚙️ Advanced Features

- **Structured API**: Clean parameter structures for maintainable code
- **Flexible Updates**: Partial updates with optional parameters
- **Meta Data Support**: Custom fields and SEO metadata
- **Post Scheduling**: Future publication with ISO8601 timestamps

## Configuration

Add WordPress configuration to your `mcp-config.toml`:

```toml
[handlers.wordpress]
url = "https://your-wordpress-site.com"
username = "your_username"
password = "your_application_password"
enabled = true
timeout_seconds = 30

# Optional: tool calls are rate limited per WordPress site (defaults shown)
[handlers.wordpress.rate_limit]
requests_per_second = 10
burst_size = 20
enabled = true
```

`enabled = true` is required: without it, the WordPress tools are not registered (and `${...}` is not expanded), with no error.

To keep the Application Password out of the config file in plaintext:

- `keyring = "<name>"` instead of `username`/`password`: read them from the OS credential store (Windows Credential Manager, macOS Keychain, Secret Service). This works the same whether Claude Desktop, Claude Code, or a terminal starts mcp-rs. See [OS Credential Store](../security/#os-credential-store)
- `${WORDPRESS_PASSWORD}`: read an environment variable. Claude Desktop passes only the variables in `env` of its JSON config. See [Which environment variables reach mcp-rs](../security/#which-environment-variables-reach-mcp-rs)
- `encrypted_credentials`: see [Encrypted WordPress Credentials](../security/#encrypted-wordpress-credentials)

When `enabled = true`, one of `username`/`password`, `keyring`, or `encrypted_credentials` must be set, otherwise startup fails.

To let `upload_media` read local files with `file_path`, list the folders it may read from. It is disabled when the list is empty (the default). Startup fails if a listed folder does not exist.

```toml
[handlers.wordpress]
upload_allowed_dirs = ["C:/Users/me/Pictures/blog"]
```

### Custom post types {#custom-post-types}

Plugins add post types such as products (WooCommerce) and events, and taxonomies such as product categories. mcp-rs uses a custom post type only when it is listed in `post_types` (v0.30.0 and later), so the AI does not change products or other content by accident:

```toml
[handlers.wordpress]
post_types = ["product"]
```

- `post` and `page` are always allowed. With `sites`, a site without its own `post_types` uses the one in `[handlers.wordpress]`, and a site with `post_types = []` uses only `post` and `page`. The `post_type` argument of the tools lists the types allowed on any of the sites; each site accepts only its own
- `get_post_types` lists the site's post types and taxonomies, and shows which ones mcp-rs may use
- A listed type can be passed as `post_type` to `get_posts`, `get_post`, `create_advanced_post`, `update_post`, `delete_post` and the revision tools. The tools show only the allowed types in `post_type`
- The post type must be in the REST API under `/wp/v2` (the type's `show_in_rest`). Types with their own API, such as WooCommerce products in `wc/v3` on some sites, are refused with the reason
- `get_taxonomies` lists the taxonomies with their `rest_base`, and `get_terms` lists the terms of one. `create_advanced_post` and `update_post` take `terms`, such as `{"product_cat": [12, 15]}`, to set the terms of the listed taxonomies. A taxonomy that is not registered for the post type is refused with the valid ones (WordPress would ignore it). `get_post` shows them for a custom post type. Creating and changing terms of custom taxonomies is not supported yet
- A post type without the editor (no `content`) can be used too. `get_post` then says that the type has no content field instead of showing empty content
- The WordPress user's capabilities still apply: the Application Password user must be allowed to edit the post type

### Using more than one site {#sites}

One mcp-rs can use several WordPress sites (v0.30.0 and later). Write every site in its own `[handlers.wordpress.sites.<site>]` table, and keep only the settings for all sites in `[handlers.wordpress]`:

```toml
[handlers.wordpress]
enabled = true
default_site = "blog"               # the site used when a tool call names none
timeout_seconds = 30                # optional: for every site, unless a site sets its own

[handlers.wordpress.sites.blog]
url = "https://blog.example.com"
keyring = "blog"

[handlers.wordpress.sites.shop]
url = "https://shop.example.com"
keyring = "shop"
```

- **Tools**: the tools stay the same (the number of tools does not grow). Each tool gets a `site` argument: `get_posts` with `site: "shop"` reads the shop's posts. Without `site`, `default_site` is used, so ask the AI to name the site when it is not the default
- **`[handlers.wordpress]`**: `enabled = true` turns on all the sites. `default_site` is required when two or more sites are used. `timeout_seconds`, `rate_limit`, `upload_allowed_dirs` and `post_types` apply to every site that does not set its own. A site with `upload_allowed_dirs = []` or `post_types = []` does not use the ones in `[handlers.wordpress]`: `upload_allowed_dirs = []` turns off `file_path` for that site. Write `url` and the credentials (`keyring`, `username`/`password`, `encrypted_credentials`) only in the sites' tables
- **Each site**: `url` and the credentials, and optionally `timeout_seconds`, `rate_limit`, `upload_allowed_dirs` and `post_types`. Write `enabled = false` to stop using one site; its credentials are then not read. Site names use lowercase letters, digits, `_` and `-`, up to 32 characters
- **Credentials**: each site has its own, preferably a dedicated user with an Application Password on that site. Store them with `mcp-rs wordpress-keyring set <name>` for each site
- **Rate limit and audit log**: each site has its own rate limit. The audit log records the site's URL with each tool call
- `mcp-rs check` checks each site, and `mcp_rs_status` lists them. `mcp-rs uninstall` offers to delete each site's entry in the OS credential store
- Resources (`wordpress://posts` and others) and resource templates (`wordpress://post/{id}` and others) read the default site only
- In HTTP mode, the tool limits of an API key or access token apply to every site: a key allowed `get_posts` can read the posts of all the sites
- `WORDPRESS_URL`, `WORDPRESS_USERNAME` and `WORDPRESS_PASSWORD` do not change a config file with `sites`
- [`mcp-rs init`](../guides/#mcp-rs-init) sets up the first site in the one-site form (`url` and `keyring` directly in `[handlers.wordpress]`), and then asks whether to add another site. From v0.30.0, adding one asks for a name for the site you have (it moves to `[handlers.wordpress.sites.<name>]` and becomes `default_site`, so tool calls without `site` still go to it), and for the new site's URL, credentials (kept in the OS credential store) and name. The rest of the config file and its comments are kept, with a backup copy. When the config file has a password in plain text, `init` asks whether to delete the backup copy, and deletes it unless you answer n: answer n to keep it for going back to the previous file. Run `mcp-rs init` again and keep the config file to add more sites later. To change the default site, edit `default_site`

With one site, `url` and the credentials can still be written directly in `[handlers.wordpress]`, as in the examples above. That site is named `default`, and the tools get no `site` argument.

## Prompts {#prompts}

From v0.30.0, mcp-rs offers prompts: ready-made requests that you pick in the client (for example from the "+" menu in Claude Desktop) instead of typing them. They ask the AI to confirm with you before changing anything.

- **Draft a blog post** (`draft_post`): give a topic and optional notes. The AI picks categories and tags from the site and creates a draft, without publishing it
- **Review a post's SEO** (`review_post_seo`): give a post ID. The AI checks the title, slug, excerpt, headings and links and suggests changes, and changes the post only after you approve
- **Moderate comments** (`moderate_comments`): optionally a post ID. The AI goes through the comments awaiting moderation and suggests what to do with each

With more than one site, each prompt also takes `site`. The prompts are in English; the AI answers in the language you use.

## Setting up Application Passwords

Create the Application Password for a WordPress user with only the role the AI client needs, preferably a dedicated user:

| Role | What the AI client can do | Use when |
|------|---------------------------|----------|
| Contributor | Write and edit its own drafts. Cannot publish, and cannot upload media | You review and publish every post yourself |
| Author | Also publish its own posts and upload media | You want the AI client to publish |
| Editor, Administrator | Also edit and publish other users' posts and pages. They have `unfiltered_html`, so posts may contain `<script>` | Avoid |

The content the AI client writes comes partly from what it reads: a web page, a comment, or a post can contain instructions that make it write something else (prompt injection). With Editor or Administrator, such content can publish a post with `<script>` that runs in your visitors' browsers at once. With Contributor, nothing is published until you do it, and WordPress removes `<script>` from what a Contributor or an Author saves.

1. Go to **WordPress Admin** → **Users** → **Profile**
2. Scroll to **Application Passwords** section
3. Enter application name (e.g., "MCP-RS Integration")
4. Click **Add New Application Password**
5. Copy the generated password to your config

## Available Tools

## Core Content Management

### `create_post`

Create a new WordPress post (basic version).

**Parameters:**
- `title` (string): Post title
- `content` (string): Post content (HTML supported)
- `slug` (string, optional): URL slug, for example an English slug for a post with a Japanese title. If omitted, WordPress generates one from the title. See [Slugs](#slugs)

**Example Usage:**

```json
{
  "tool": "create_post",
  "arguments": {
    "title": "My New Blog Post",
    "content": "<p>This is the content of my post with <strong>HTML formatting</strong>.</p>"
  }
}

```

### `create_advanced_post`

Create a new WordPress post or page with advanced options.

**Parameters:**
- `title` (string): Post/page title
- `content` (string): Post/page content
- `post_type` (string): "post" (投稿) or "page" (固定ページ) [default: "post"]
- `status` (string, optional): "publish" (公開), "draft" (下書き), "pending" (レビュー待ち), "private" (非公開), "future" (予約投稿、`date`が必須), or a custom status registered on the site (for example by a plugin) [default: "draft"]
- `date` (string, optional): Publication date (ISO8601 format, required for "future" status)
- `categories` (array, optional): Category IDs (posts only)
- `tags` (array, optional): Tag IDs (posts only)
- `featured_media_id` (number, optional): Featured image media ID
- `meta` (object, optional): Meta fields for SEO (e.g., Yoast SEO fields)
- `slug` (string, optional): URL slug, for example an English slug for a post with a Japanese title. If omitted, WordPress generates one from the title. See [Slugs](#slugs)

**Example Usage:**

```json
{
  "tool": "create_advanced_post",
  "arguments": {
    "title": "SEO-Optimized Post",
    "content": "<p>Content with SEO optimization</p>",
    "post_type": "post",
    "status": "draft",
    "categories": [1, 5],
    "tags": [10, 15, 20],
    "featured_media_id": 123,
    "meta": {
      "_yoast_wpseo_metadesc": "Custom meta description",
      "_yoast_wpseo_meta-robots-noindex": "1"
    }
  }
}

```

### `create_post_with_embeds`

Create a post or page with embedded YouTube videos and social media content appended to the base content.

**Parameters:**
- `title` (string): Post title
- `content` (string): Base post content (embeds are added after it)
- `youtube_urls` (array of strings, optional): YouTube video URLs to embed as video players: `youtube.com/watch?v=…`, `youtu.be/…`, `youtube.com/embed/…`, `youtube.com/shorts/…`, `youtube.com/live/…` or `youtube-nocookie.com/embed/…` (Shorts, live and nocookie URLs from v0.30.0). URLs in other forms are not added; the result lists them after `Not embedded (unsupported URL):` (v0.30.0 and later)
- `social_urls` (array of strings, optional): Social media URLs to embed: X/Twitter, Instagram posts (`instagram.com/p/…`), Facebook or TikTok. Other URLs are not added and are listed in the result in the same way. Both lists accept only URLs on the services' own hosts (for example `www.youtube.com` or `x.com`, not a URL on another site that contains these names)
- `post_type` (string, optional): `post` or `page`
- `status` (string, optional): `publish`, `draft`, `pending`, `private`, or a custom status registered on the site (for example by a plugin). Defaults to `draft`. `future` (scheduled) is not available because this tool has no date parameter
- `categories` (array of numbers, optional): Category IDs (posts only)
- `tags` (array of numbers, optional): Tag IDs (posts only)
- `slug` (string, optional): URL slug, for example an English slug for a post with a Japanese title. If omitted, WordPress generates one from the title. See [Slugs](#slugs)

mcp-rs adds each social media URL to the post on its own line, and WordPress decides how to show it. WordPress itself has not embedded Instagram or Facebook since 5.5.2, because Meta requires a developer app and an access token: those URLs stay plain links unless the site has a plugin that embeds them (for example one that takes the app ID and token). The result `Created … with embedded content` means that the URLs were added to the post, not that WordPress shows them as embeds. Check the post's preview to see how they look.

**Example Usage:**

```json
{
  "tool": "create_post_with_embeds",
  "arguments": {
    "title": "Rust Conference Highlights",
    "content": "<p>Here are the talks we enjoyed most.</p>",
    "youtube_urls": ["https://www.youtube.com/watch?v=dQw4w9WgXcQ"],
    "status": "draft"
  }
}
```

### `update_post`

Update an existing WordPress post.

**Parameters:**
- `post_id` (number): ID of the post to update
- `post_type` (string, optional): `post` (default) or `page`, or a type listed in [`post_types`](#custom-post-types). A page needs `"post_type": "page"`
- `title` (string, optional): New post title
- `content` (string, optional): New post content
- `status` (string, optional): New post status: `publish`, `draft`, `pending`, `private`, or `future` (scheduled). `future` needs `date`, unless the post is already scheduled
- `date` (string, optional): Publication date, ISO8601 in the site's time zone (for example `2030-01-01T10:00:00`). See [Scheduling an existing post](#scheduling-an-existing-post) (v0.29.0 and later)
- `categories` (array, optional): New category IDs
- `tags` (array, optional): New tag IDs
- `featured_media_id` (number, optional): New featured image ID
- `meta` (object, optional): New meta fields
- `slug` (string, optional): New URL slug. WordPress does not change the slug when the title changes. See [Slugs](#slugs)

**Example Usage:**

```json
{
  "tool": "update_post",
  "arguments": {
    "post_id": 123,
    "title": "Updated Title",
    "status": "publish",
    "categories": [1, 2, 3]
  }
}

```

### Scheduling an existing post

To check a draft first and schedule it afterwards, update it with `status: "future"` and a future `date` (v0.29.0 and later):

```json
{
  "tool": "update_post",
  "arguments": {
    "post_id": 123,
    "status": "future",
    "date": "2030-01-01T10:00:00"
  }
}
```

- Replace `date` with the time you want, in the future. With a date in the past, WordPress publishes the post immediately instead of scheduling it
- `date` is in the site's time zone (Settings → General). `get_post` shows the date with a `Date:` line, marked `scheduled` for a scheduled post
- `status: "future"` without `date` is refused unless the post is already scheduled: WordPress would publish it immediately. Up to v0.28.0, `update_post` had no `date` and sent `future` as it was
- To change the time of a scheduled post, send only `date`
- `get_posts` with `status: "future"` lists the scheduled posts

### Revisions {#revisions}

WordPress keeps earlier versions (revisions) of posts and pages when they are saved. If the AI changed a post in a way you do not want, go back to an earlier version (v0.30.0 and later). These tools need credentials with permission to edit the post.

- `get_revisions`: the revisions of a post or page, newest first, with the revision ID, date, author, title, length and the start of the content. Parameters: `post_id` (required), `post_type` (`post`, the default, or `page`)
- `get_revision`: one revision's title, excerpt and content. Parameters: `post_id`, `revision_id` (required), `post_type`, `format` (`raw`, the default, `rendered` or `text`, as in `get_post`)
- `restore_revision`: replace the title, content and excerpt of the post with those of the revision. The status, publish date, slug, categories and tags are not changed, so a published post stays published with the restored content. Parameters: `post_id`, `revision_id` (required), `post_type`

When revisions are enabled on the site, WordPress saves the content before the restore as a revision, so a restore can usually be undone the same way. The site may turn revisions off or limit how many it keeps (`WP_POST_REVISIONS`); then the content before the restore may not be kept, so read it with `get_post` first if you must keep it. A revision of another post is refused.

### `delete_post`

Delete a WordPress post.

**Parameters:**
- `post_id` (number): ID of the post to delete
- `force` (boolean, optional): Permanently delete (true) or move to trash (false) [default: false]

### Slugs

The post creation tools and `update_post` accept an optional `slug`. Without it, WordPress generates the slug from the title, so a Japanese title gives a Japanese slug (percent-encoded in the URL). To use an English slug, pass it when creating the post:

```json
{
  "tool": "create_post",
  "arguments": {
    "title": "はじめての投稿",
    "content": "<p>本文</p>",
    "slug": "my-first-post"
  }
}
```

Or change the slug of an existing post or page. Only the slug is changed:

```json
{
  "tool": "update_post",
  "arguments": {
    "post_id": 123,
    "slug": "my-first-post"
  }
}
```

Notes:

- WordPress may change the slug you pass. It removes characters that cannot be used in a URL, and appends `-2` and so on if another post already uses the slug. The tool result shows the actual slug, for example `Slug: my-first-post-2`
- For a draft, WordPress keeps the slug as given and makes it unique when the post is published
- Changing the slug of a published post changes its URL. Whether the old URL redirects depends on WordPress and your plugins; mcp-rs does not set up redirects
- An empty `slug`, or one with no letters or digits once HTML tags and entities are removed (such as `!!!` or `&amp;`, which WordPress would make empty), is rejected. Omit `slug` to leave it unchanged

## Content Retrieval

### `get_posts`

Retrieve WordPress posts. By default, only published posts.

**Parameters:**
- `status` (string, optional): Which posts to list: `publish` (default), `future` (scheduled), `draft`, `pending`, `private`, or `any`. Several can be given with commas, for example `future,draft` (v0.29.0 and later). Other than `publish`, the credentials must be allowed to edit posts
- `search` (string, optional): Keyword to search for in the title and content
- `categories`, `tags` (arrays of IDs, optional): Only posts in any of these categories, or with any of these tags (`get_categories` and `get_tags` list the IDs)
- `author` (number, optional): Author user ID
- `after`, `before` (string, optional): Only posts published after or before this date or date-time, in the site's time zone (`2026-11-01` or `2026-11-01T09:00:00`)
- `orderby` (optional): `date` (default), `modified`, `title`, `id`, `slug`, `author`, or `relevance` (with `search`)
- `order` (optional): `desc` (default) or `asc`
- `per_page` (1-100), `page` (from 1) (optional)

Without any of `search`, `categories`, `tags`, `author`, `after`, `before`, `orderby`, `order`, `per_page`, and `page`, `get_posts` returns all posts, up to 100 pages of 100 (10,000 posts). With more, it returns an error instead of a list that is missing posts: read them one page at a time with `per_page` and `page`, or narrow them down (v0.30.0 and later; up to v0.29.0, it returned the first 10,000 as if they were all). With any of them (v0.29.0 and later), it reads only one page (20 posts unless `per_page` is given), and the first line shows the page and the total, for example `Found 3 posts (page 1 of 3, 3 per page; 8 in total)`. Use `page` for the next ones:

```json
{
  "tool": "get_posts",
  "arguments": { "search": "mcp-rs", "after": "2026-11-01", "per_page": 10 }
}
```

### `get_pages`

Retrieve WordPress pages. It takes the same `status`, `search`, `author`, `after`, `before`, `orderby` (also `menu_order`), `order`, `per_page`, and `page` as `get_posts` (v0.29.0 and later), but not `categories` and `tags`, which pages do not have.

### `get_all_content`

Retrieve both WordPress posts and pages.

### `get_post`

Retrieve a single WordPress post or page by ID, with its full content (not truncated).

**Parameters:**
- `post_id` (number): ID of the post or page to retrieve
- `post_type` (string, optional): `post` (default) or `page`
- `format` (string, optional): How to return the title and content
  - `raw` (default): The editable source, including Gutenberg block markup. It can be passed back to `update_post` unchanged. Requires credentials with edit permission. Without it, the rendered HTML is returned instead, and the `Format:` line says so
  - `rendered`: The HTML as displayed on the site
  - `text`: Plain text with tags removed

For a password-protected post read without edit permission, WordPress hides the content, and the output includes a note not to treat the content as empty.

The output has a `Date:` line with the publication date in the site's time zone, marked `scheduled` for a scheduled post, for example `Date: 2026-11-19T10:00:00 (site time, scheduled)` (v0.29.0 and later).

The output has a `Slug:` line with the post's URL slug (a Japanese slug is shown decoded). A draft created without a slug shows `none`, because WordPress generates the slug when the post is published.

The output also has a `Featured Media:` line with the featured image's media ID and URL, for example `Featured Media: 123 (https://example.com/wp-content/uploads/photo.png)`. It shows `none` when no featured image is set. If the media item cannot be fetched, only the ID is shown.

## Media Management

### `upload_media`

Upload a media file to WordPress media library.

**Parameters:** Pass either `file_data` or `file_path`, not both.
- `file_data` (string): Base64-encoded file data
- `file_path` (string): Absolute path of a file on the machine running mcp-rs. Disabled unless [`upload_allowed_dirs`](#configuration) is set
- `filename` (string): Original filename. Its extension must match `mime_type` (e.g., `photo.jpg` for `image/jpeg`). Required with `file_data`. With `file_path`, defaults to the file's name
- `mime_type` (string): MIME type. Must be one of the [supported file types](#supported-file-types). Required with `file_data`. With `file_path`, defaults to the type for the file's extension

The upload is rejected before anything is sent to WordPress if the MIME type is not supported, the extension does not match the MIME type, the file content does not match the MIME type, the filename contains path separators or control characters, or the file is too large. These checks apply to `file_path` too, so files other than the supported types (such as `.env`) cannot be sent even from an allowed folder.

**`file_path` restrictions:**
- The path must be absolute
- The file must be inside one of the `upload_allowed_dirs` folders. The check uses the resolved path, so `..` and symbolic links (or junctions on Windows) cannot point outside them. After opening the file, mcp-rs checks that it opened the file it checked, so replacing an item in the folder with a link at that moment does not let it read a file outside. On Windows, the check uses the opened file's actual path. On Linux and macOS, it re-resolves the path and compares the files; a folder inside an allowed folder that is replaced twice at exactly the right moments can still get past it, so do not let others write to the allowed folders
- On Windows, the path must be on a drive (such as `C:\...`). Network paths (`\\server\share\...`) and device paths (`\\.\...`) are rejected before mcp-rs touches them, so it does not connect to the server (and send your Windows credentials) for a path it would not allow
- A path outside the allowed folders and a file that does not exist give the same error, so the error does not reveal which files exist elsewhere
- With STDIO (Claude Code, Claude Desktop), mcp-rs runs on the same machine as the client, so it reads the client's local files. Over HTTP/WebSocket, the client may be on another machine, and the files are read from the server. mcp-rs logs a warning at startup in that case
- The uploaded file is public at once: WordPress gives every media file a URL that anyone can open (the `source_url` in the result), even before it is used in a post. Put only files you are willing to publish in `upload_allowed_dirs`
- The AI client decides which file to upload. A request in a web page, a document or a plugin's answer can lead it to upload a file you did not mean to publish. Do not allow whole folders such as Pictures or Documents: make a folder only for files to publish (for example `C:/Users/me/Pictures/mcp-upload`), and move files into it when you want the AI to use them

**Example Usage:**

```json
{
  "tool": "upload_media",
  "arguments": {
    "file_data": "iVBORw0KGgoAAAANSUhEUgAA...",
    "filename": "hero-image.jpg",
    "mime_type": "image/jpeg"
  }
}

```

With `file_path` (`filename` and `mime_type` are taken from the file):

```json
{
  "tool": "upload_media",
  "arguments": {
    "file_path": "C:/Users/me/Pictures/blog/hero-image.png"
  }
}
```

### `get_media`

Retrieve all WordPress media files.

**Parameters:** None

### `get_media_item`

Retrieve a single WordPress media item by ID.

**Parameters:**
- `media_id` (number): Media ID to retrieve

### `update_media`

Update WordPress media metadata (title, alt text, caption, description).

**Parameters:**
- `media_id` (number): Media ID to update
- `title` (string, optional): Media title
- `alt_text` (string, optional): Alternative text for accessibility
- `caption` (string, optional): Media caption
- `description` (string, optional): Media description
- `post` (number, optional): Post ID to attach media to

**Example Usage:**

```json
{
  "tool": "update_media",
  "arguments": {
    "media_id": 123,
    "alt_text": "Product image showing red handbag",
    "caption": "Spring 2024 collection",
    "description": "Detailed view of our latest handbag design"
  }
}

```

### `delete_media`

Delete WordPress media file.

**Parameters:**
- `media_id` (number): Media ID to delete
- `force` (boolean, optional): Permanently delete (bypass trash) [default: false]

WordPress does not support moving media to the trash unless the site enables it (`MEDIA_TRASH`), so `force: true` is usually required. Without it, WordPress returns `rest_trash_not_supported`.

### `create_post_with_featured_image`

Create a post with a featured image in one operation.

**Parameters:**
- `title` (string): Post title
- `content` (string): Post content
- `featured_media_id` (number): Media ID from uploaded image. If WordPress does not set it (for example, the ID does not exist), the post is still created: the result says so and gives the post ID. Do not create the post again; check the media ID and use `set_featured_image`
- `status` (string, optional): `publish`, `draft`, `pending`, `private`, or a custom status registered on the site (for example by a plugin). Defaults to `draft`. `future` (scheduled) is not available because this tool has no date parameter
- `slug` (string, optional): URL slug, for example an English slug for a post with a Japanese title. If omitted, WordPress generates one from the title. See [Slugs](#slugs)

### `set_featured_image`

Set featured image for an existing post.

**Parameters:**
- `post_id` (number): ID of the post to update
- `media_id` (number): ID of the media to set as featured image

## Category & Tag Management

**Returns:** Array of post objects with metadata

## `get_post`

Retrieve a single WordPress post or page by ID, with its full content. See [`get_post`](#get_post) above for the `post_type` and `format` parameters.

**Parameters:**
- `post_id` (number): ID of the post or page to retrieve
- `post_type` (string, optional): `post` (default) or `page`
- `format` (string, optional): `raw` (default), `rendered`, or `text`

**Example Usage:**

```json
{
  "tool": "get_post",
  "arguments": {
    "post_id": 123,
    "format": "raw"
  }
}

```

## `update_post`

Update an existing WordPress post with comprehensive options.

**Parameters:**
- `post_id` (number): ID of post to update
- `post_type` (string, optional): `post` (default) or `page`, or a type listed in [`post_types`](#custom-post-types). A page needs `"post_type": "page"`
- `title` (string, optional): New post title
- `content` (string, optional): New post content
- `status` (string, optional): `publish`, `draft`, `pending`, `private`, or `future` (scheduled, with `date`). Unchanged when omitted
- `date` (string, optional): Publication date, ISO8601 in the site's time zone. See [Scheduling an existing post](#scheduling-an-existing-post)
- `categories` (array, optional): Array of category IDs
- `tags` (array, optional): Array of tag IDs
- `featured_media_id` (number, optional): Featured image media ID
- `slug` (string, optional): New URL slug. WordPress does not change the slug when the title changes. See [Slugs](#slugs)

**Example Usage:**

```json
{
  "tool": "update_post",
  "arguments": {
    "post_id": 123,
    "title": "Updated Post Title",
    "content": "<p>Updated content here...</p>",
    "status": "publish",
    "categories": [5, 12],
    "tags": [23, 45]
  }
}

```

## `delete_post`

Delete a WordPress post.

**Parameters:**
- `post_id` (number): ID of post to delete
- `force` (boolean, optional): Force delete (permanently delete, bypass trash)

**Example Usage:**

```json
{
  "tool": "delete_post",
  "arguments": {
    "post_id": 123,
    "force": false
  }
}

```

**Note:** When `force` is `false` (default), the post is moved to trash. When `true`, it's permanently deleted.

## Comments {#comments}

Comments are read and changed as the configured WordPress user, who needs to be able to moderate comments (an Editor or Administrator) to list comments awaiting moderation, spam or trash, and to change them. The commenter's email and IP addresses are not shown (v0.30.0 and later; up to v0.29.0, `get_comments` showed the email address).

## `get_comments`

Retrieve WordPress comments: ID, post, parent (the comment it replies to, `0` if none), status, author name, date, link, and content.

**Parameters:**
- `post_id` (number, optional): Filter comments by specific post
- `status` (string, optional): `approve` (approved, the default), `hold` (awaiting moderation), `spam`, `trash`, or `all` (approved and awaiting moderation). v0.30.0 and later

**Example Usage:**

```json
{
  "tool": "get_comments",
  "arguments": {
    "post_id": 123
  }
}

```

## `create_comment`

Create a comment as the configured WordPress user, usually a reply to another comment (v0.30.0 and later).

WordPress comments have no drafts. So that you can check a reply the AI wrote before it appears on the site, the comment is held for moderation by default. Approve it with `update_comment` (`status: "approve"`) or in the WordPress admin (**Comments**). Pass `status: "approve"` to publish it at once.

**Parameters:**
- `content` (string, required): Comment text
- `parent` (number, optional): ID of the comment to reply to
- `post_id` (number, optional): Post or page to comment on. Required without `parent`; with `parent`, the post of that comment is used
- `status` (string, optional): `hold` (held for moderation, the default) or `approve` (published at once)

**Example Usage:**

```json
{
  "tool": "create_comment",
  "arguments": {
    "parent": 45,
    "content": "Thank you for your question. The setting is under Settings > Reading."
  }
}
```

## `update_comment`

Change a comment's status or text (v0.30.0 and later). Approving a comment publishes it on the site.

**Parameters:**
- `comment_id` (number, required): Comment ID
- `status` (string, optional): `approve` (published), `hold` (back to awaiting moderation), `spam`, or `trash`
- `content` (string, optional): New comment text

Pass `status`, `content`, or both.

## `delete_comment`

Delete a comment (v0.30.0 and later).

**Parameters:**
- `comment_id` (number, required): Comment ID
- `force` (boolean, optional): Delete permanently instead of moving to the trash (default: `false`)

## `get_categories`

Retrieve all WordPress categories.

**Parameters:** None

**Returns:** Array of category objects with metadata

## `create_category`

Create a new WordPress category.

**Parameters:**
- `name` (string): Category name
- `description` (string, optional): Category description
- `parent` (number, optional): Parent category ID for hierarchical structure

**Example Usage:**

```json
{
  "tool": "create_category",
  "arguments": {
    "name": "Technology",
    "description": "Posts about technology and software",
    "parent": 5
  }
}

```

## `update_category`

Update an existing WordPress category.

**Parameters:**
- `category_id` (number): ID of category to update
- `name` (string, optional): New category name
- `description` (string, optional): New category description

**Example Usage:**

```json
{
  "tool": "update_category",
  "arguments": {
    "category_id": 10,
    "name": "Web Development",
    "description": "Updated description"
  }
}

```

## `delete_category`

Delete a WordPress category permanently. WordPress has no trash for categories, so `force` must be `true`; without it, WordPress refuses the request. Posts that were only in this category are moved to the default category, and the default category itself cannot be deleted.

**Parameters:**
- `category_id` (number): ID of category to delete
- `force` (boolean): Must be `true`

**Example Usage:**

```json
{
  "tool": "delete_category",
  "arguments": {
    "category_id": 10,
    "force": true
  }
}

```

## `get_tags`

Retrieve all WordPress tags.

**Parameters:** None

**Returns:** Array of tag objects with metadata

## `create_tag`

Create a new WordPress tag.

**Parameters:**
- `name` (string): Tag name
- `description` (string, optional): Tag description

**Example Usage:**

```json
{
  "tool": "create_tag",
  "arguments": {
    "name": "rust",
    "description": "Posts about Rust programming language"
  }
}

```

## `update_tag`

Update an existing WordPress tag.

**Parameters:**
- `tag_id` (number): ID of tag to update
- `name` (string, optional): New tag name
- `description` (string, optional): New tag description

**Example Usage:**

```json
{
  "tool": "update_tag",
  "arguments": {
    "tag_id": 15,
    "name": "programming",
    "description": "Updated tag description"
  }
}

```

## `delete_tag`

Delete a WordPress tag permanently. WordPress has no trash for tags, so `force` must be `true`; without it, WordPress refuses the request. The tag is removed from every post that has it.

**Parameters:**
- `tag_id` (number): ID of tag to delete
- `force` (boolean): Must be `true`

**Example Usage:**

```json
{
  "tool": "delete_tag",
  "arguments": {
    "tag_id": 15,
    "force": true
  }
}

```

## `create_post_with_categories_tags`

Create a new WordPress post with categories and tags.

**Parameters:**
- `title` (string): Post title
- `content` (string): Post content
- `categories` (array, optional): Array of category IDs
- `tags` (array, optional): Array of tag IDs
- `featured_media_id` (number, optional): Featured image media ID
- `slug` (string, optional): URL slug, for example an English slug for a post with a Japanese title. If omitted, WordPress generates one from the title. See [Slugs](#slugs)

**Example Usage:**

```json
{
  "tool": "create_post_with_categories_tags",
  "arguments": {
    "title": "Complete Guide to Rust",
    "content": "<p>This comprehensive guide covers Rust programming...</p>",
    "categories": [5, 12],
    "tags": [23, 45, 67],
    "featured_media_id": 89
  }
}

```

## `update_post_categories_tags`

Update categories and tags for an existing WordPress post.

**Parameters:**
- `post_id` (number): ID of post to update
- `categories` (array, optional): Array of category IDs to set
- `tags` (array, optional): Array of tag IDs to set

**Example Usage:**

```json
{
  "tool": "update_post_categories_tags",
  "arguments": {
    "post_id": 123,
    "categories": [5, 8],
    "tags": [15, 20, 25]
  }
}

```

## Site Settings

## `get_settings`

Retrieve the WordPress site settings (title, tagline, timezone, front page configuration, posts per page, default category, language, and so on).

**Parameters:** None

## `update_settings`

Update WordPress site settings. Only the specified fields are changed.

**Parameters (all optional):**
- `title` (string): Site title
- `description` (string): Site tagline
- `timezone` (string): Site timezone (e.g., `Asia/Tokyo`)
- `show_on_front` (string): `posts` or `page`
- `page_on_front` (number): Static front page ID (when `show_on_front` is `page`)
- `page_for_posts` (number): Posts page ID (when `show_on_front` is `page`)
- `posts_per_page` (number): Number of posts per page
- `default_category` (number): Default category for new posts
- `language` (string): Site language code (e.g., `ja`, `en_US`)

**Example Usage:**

```json
{
  "tool": "update_settings",
  "arguments": {
    "description": "Notes on Rust and AI agents",
    "timezone": "Asia/Tokyo"
  }
}
```

## `set_front_page`

Set a static page as the front page.

**Parameters:**
- `page_id` (number): ID of the page to show on the front page

## `set_front_page_to_posts`

Set the front page to show the latest posts.

**Parameters:**
- `posts_page_id` (number, optional): ID of a page to use for the blog posts

## Workflow Examples

## Basic Blog Post

```

AI automatically:
1. Uses create_post tool
2. Generates title and content
3. Returns post URL and ID

```

## Featured Image Workflow

```

AI automatically:
1. Uses upload_media with base64 image data
2. Uses create_post_with_featured_image with returned media ID
3. Creates complete post with featured image

```

## Update Existing Post

```

AI automatically:
1. Uploads image using upload_media
2. Uses set_featured_image to update post
3. Confirms successful update

```

## Category Management Workflow

```

AI automatically:
1. Uses create_category with name and description
2. Returns category ID and details
3. Can be used for organizing posts

User: "Create a subcategory under Technology"
AI automatically:
1. Uses get_categories to find Technology category ID
2. Uses create_category with parent parameter
3. Creates hierarchical category structure

```

## Tag Management Workflow

```

AI automatically:
1. Uses create_tag for each relevant tag (rust, programming, tutorial)
2. Returns tag IDs for future reference
3. Tags can be applied to posts during creation

User: "Update the description of the 'rust' tag"
AI automatically:
1. Uses get_tags to find tag ID
2. Uses update_tag with new description
3. Confirms update success

```

## Smart Content Creation Workflow

```

AI intelligently:
1. Uses get_categories to find existing categories
2. Finds "Technology" category (avoids creating duplicates)
3. Suggests relevant tags from get_tags
4. Uses create_post_with_categories_tags with proper taxonomy
5. Creates well-organized content

User: "Add the 'tutorial' tag to post #456"
AI automatically:
1. Gets current post categories/tags using get_posts
2. Adds new tag to existing taxonomy
3. Uses update_post_categories_tags to preserve existing data
4. Confirms successful update

```

## Complete Content Management Workflow

```

AI automatically:
1. Uses get_post to retrieve current post details
2. Shows current title, content, and taxonomy
3. Uses update_post with new title and categories
4. Preserves existing content and other metadata
5. Confirms successful update

User: "Delete the draft post about outdated technology"
AI automatically:
1. Uses get_posts to find draft posts
2. Identifies the specific post by content analysis
3. Uses delete_post with force=false (moves to trash)
4. Confirms post moved to trash for recovery if needed

```

## Content Lifecycle Management

```

AI automatically:
1. Uses get_post to retrieve draft details
2. Uses upload_media for featured image
3. Uses update_post with status="publish" and featured_media_id
4. Complete publication workflow in one operation

User: "Archive old posts from 2022"
AI automatically:
1. Uses get_posts to find posts from 2022
2. Shows list for user confirmation
3. Uses update_post to change status to "private"
4. Bulk content management operation

```

## AI-Assisted Taxonomy Management

```

AI intelligently:
1. Uses get_categories to scan existing categories
2. Finds similar: "Web Development", "ウェブ技術", "webdev"
3. Suggests: "Should I use existing 'Web Development' or create new 'ウェブ開発'?"
4. User confirms choice
5. Creates post with appropriate categorization

This workflow prevents duplicate/similar categories and maintains clean taxonomy structure.

```

## Error Handling

The WordPress integration includes comprehensive error handling:

- **Authentication Errors**: Invalid credentials or permissions
- **API Errors**: WordPress REST API specific errors
- **Network Errors**: Timeout, connection failures
- **Validation Errors**: Invalid parameters or data format
- **File Upload Errors**: Unsupported file types or size limits

All errors include detailed messages for debugging and user feedback.

## Supported File Types

`upload_media` accepts only the following types. Each file's content is checked against its type's magic number.

| Type | MIME type | Extensions |
|------|-----------|------------|
| JPEG | `image/jpeg` | .jpg, .jpeg, .jpe |
| PNG | `image/png` | .png |
| GIF | `image/gif` | .gif |
| WebP | `image/webp` | .webp |
| PDF | `application/pdf` | .pdf |
| MP4 video | `video/mp4` | .mp4, .m4v |
| WebM video | `video/webm` | .webm |
| Ogg video | `video/ogg` | .ogv |
| MP3 audio | `audio/mpeg` | .mp3 |
| MP4 audio | `audio/mp4` | .m4a |
| WAV audio | `audio/wav` | .wav |
| Ogg audio | `audio/ogg` | .ogg, .oga |

MIME types are case-insensitive, parameters such as `; charset=...` are ignored, and common aliases (`image/jpg`, `image/pjpeg`, `image/x-png`, `audio/mp3`, `audio/x-m4a`, `audio/x-wav`) are accepted.

The following are intentionally not supported:

- **SVG**: can embed scripts. WordPress also disables it by default
- **Plain text**: has no magic number, so its content cannot be checked
- **Office documents (.doc, .docx, etc.)**: the content can only be identified as a ZIP archive
- **QuickTime (.mov)**: the order of its leading data is not fixed, so it cannot be identified reliably

**Size limits**: files larger than 10 MiB are rejected. With `file_data`, the transport's message size limit (1 MiB by default for STDIO and HTTP, including the roughly 33% base64 overhead) usually applies first. `file_path` is not affected by that limit. WordPress's own limits (`upload_max_filesize` and `post_max_size`) also apply.

## Performance Optimization

- **Connection Pooling**: Reuses HTTP connections for better performance
- **Timeout Configuration**: Configurable request timeouts
- **Retry Logic**: Automatic retry for transient failures (up to 3 attempts). Create and delete requests are not resent after a timeout or server error, since WordPress may already have processed them
- **Efficient Uploads**: Streaming multipart uploads for large files

## Troubleshooting

## Common Issues

**Authentication Failed**
- Verify application password is correct
- Check username matches WordPress user
- Ensure user has appropriate permissions

**Upload Failed**
- Check file size against WordPress limits
- Verify MIME type is supported
- Ensure proper base64 encoding

**Connection Timeout**
- Increase `timeout_seconds` in configuration
- Check network connectivity
- Verify WordPress URL is accessible

## Debug Mode

Enable detailed logging by setting environment variable:

```bash

```

This will show detailed HTTP requests and responses for troubleshooting.

---

## Health Check System

The WordPress MCP integration includes a comprehensive health check system to validate your environment before performing operations. This ensures reliable operation and helps identify configuration issues early.

## `wordpress_health_check`

Perform comprehensive environment validation.

**Parameters:** None

**Usage Example:**

```json
{
  "tool": "wordpress_health_check",
  "arguments": {}
}

```

## Check Process

The health check performs five critical validation stages:

### 1. 🌐 Site Accessibility

- **Purpose**: Verify WordPress site is reachable
- **Test**: HTTP GET request to site URL
- **Success**: Site responds with 200 OK status
- **Failure**: Network timeout, DNS resolution, or server errors

### 2. 🔌 REST API Availability

- **Purpose**: Confirm WordPress REST API is enabled and accessible
- **Test**: GET request to `/wp-json/wp/v2/` endpoint
- **Success**: API responds with namespace information
- **Failure**: REST API disabled, blocked, or misconfigured

### 3. 🔐 Authentication Validation

- **Purpose**: Verify credentials are correct and user exists
- **Test**: Authenticated request to `/wp-json/wp/v2/users/me`
- **Success**: Returns user profile information
- **Failure**: Invalid username, incorrect password, or user not found

### 4. ✅ Permission Verification

- **Purpose**: Check user has required capabilities
- **Test**: Validate publish_posts, upload_files, and edit_posts permissions
- **Success**: User has all required capabilities
- **Failure**: Insufficient permissions for content operations

### 5. 📁 Media Upload Capability

- **Purpose**: Test file upload functionality end-to-end
- **Test**: Upload small test image to media library
- **Success**: File uploaded successfully and media ID returned
- **Failure**: Upload restrictions, storage issues, or API limitations

## Health Report

The health check returns a comprehensive report with the following information:

```json
{
  "status": "healthy",
  "site_info": {
    "url": "https://your-site.com",
    "name": "Your Site Name",
    "version": "6.3.2",
    "api_version": "2.0"
  },
  "user_info": {
    "id": 1,
    "username": "admin",
    "display_name": "Site Administrator",
    "email": "admin@yoursite.com",
    "roles": ["administrator"]
  },
  "capabilities": {
    "publish_posts": true,
    "upload_files": true,
    "edit_posts": true,
    "manage_options": true
  },
  "media_upload": {
    "supported": true,
    "test_upload_id": 789,
    "max_upload_size": "64M"
  },
  "performance": {
    "response_time_ms": 245,
    "api_response_time_ms": 89
  }
}

```

## Status Levels

**🟢 Healthy**
- All checks passed
- System ready for full operation
- No configuration issues detected

**🟡 Warning**
- Minor issues detected
- Basic functionality available
- Some features may be limited

**🔴 Critical**
- Major issues prevent operation
- Configuration required
- System not usable

## Troubleshooting Guide

### Site Accessibility Issues

```

🔧 Solutions:
   • Verify URL is correct and includes https://
   • Check site is online and responding
   • Ensure firewall/security plugins allow access
   • Test URL in browser manually

```

### REST API Problems

```

🔧 Solutions:
   • Enable REST API in WordPress settings
   • Check security plugins aren't blocking API
   • Verify .htaccess isn't blocking /wp-json/
   • Test API endpoint in browser: /wp-json/wp/v2/

```

### Authentication Failures

```

🔧 Solutions:
   • Regenerate application password
   • Verify username is exact match
   • Check user account is active
   • Ensure application passwords are enabled

```

### Permission Issues

```

🔧 Solutions:
   • Use a role that allows what you need: Contributor for drafts,
     Author to publish and upload media (avoid Editor and Administrator,
     see "Setting up Application Passwords")
   • Check user has publish_posts capability
   • Verify upload_files permission enabled
   • Review role-based access restrictions

```

### Upload Problems

```

🔧 Solutions:
   • Check upload_max_filesize in php.ini
   • Verify post_max_size setting
   • Ensure uploads directory is writable
   • Check disk space availability

```

## Best Practices

### Before Production Use

1. **Run Health Check**: Always validate environment first
2. **Monitor Performance**: Check response times regularly
3. **Test Permissions**: Verify all required capabilities
4. **Validate Uploads**: Confirm media functionality works

### Production Operations

#### Application Password Management

WordPress application passwords may be invalidated by:
- **Hosting Provider Policies**: Automatic expiration by hosting services
- **Security Plugin Policies**: Plugins like SiteGuard enforcing rotation
- **WordPress Updates**: Major core updates affecting authentication
- **Server Changes**: PHP or environment configuration modifications

**Monitoring Strategy:**

```bash

## Daily health monitoring (recommended)

cargo run --example comprehensive_test

## Deep diagnostic for authentication issues

cargo run --example settings_api_deep_diagnosis

## Authentication verification

cargo run --example auth_diagnosis

```

#### Maintenance Mode Operations

**LightStart Plugin Configuration:**
When using WordPress maintenance mode plugins, configure exclusions for REST API access:

```

## Add to LightStart exclusions (slug format, no leading slash):

wp-json/*

```

This ensures MCP-RS can continue content operations during maintenance windows.

**Benefits:**
- ✅ Content management continuity during maintenance
- ✅ Zero-downtime WordPress updates
- ✅ Emergency content access during site maintenance

#### Production Monitoring

**Alert Criteria:**
- **HTTP 401 Errors**: Application password expiration → Regenerate password
- **Connection Timeouts**: Network/hosting issues → Check infrastructure
- **API Changes**: Plugin/core updates → Verify endpoint compatibility
- **SSL Issues**: Certificate problems → Update certificates

**Incident Response:**
1. **Detection**: Automated monitoring or error reports
2. **Diagnosis**: Run `settings_api_deep_diagnosis` for detailed analysis
3. **Classification**:
   - Password issue → WordPress Admin → Generate new application password
   - Plugin interference → Configure maintenance mode exclusions
   - Network problems → Infrastructure team investigation
4. **Resolution**: Apply fix and verify with comprehensive test
5. **Documentation**: Update operational logs and procedures

### Automated Monitoring

```rust

use mcp_rs::handlers::wordpress::WordPressHandler;

async fn monitor_wordpress_health() {
    let handler = WordPressHandler::new(config).await?;

    match handler.health_check().await {
        Ok(report) if report.status == "healthy" => {
            println!("✅ WordPress system healthy");
        },
        Ok(report) => {
            println!("⚠️ Issues detected: {}", report.status);
            // Handle warnings or critical issues
        },
        Err(e) => {
            println!("❌ Health check failed: {}", e);
            // Alert administrators
        }
    }
}

```

### Integration Workflow

```

   ├── Configure mcp-config.toml
   ├── Run wordpress_health_check
   └── Address any issues found

2. Validation Success
   ├── Proceed with content operations
   ├── Monitor performance metrics
   └── Schedule regular health checks

3. Issue Detection
   ├── Review detailed error messages
   ├── Apply recommended solutions
   └── Re-run health check to verify fixes

4. Production Monitoring
   ├── Weekly comprehensive tests
   ├── Application password rotation (as needed)
   ├── Maintenance mode coordination
   └── Incident response procedures

```

This health check system ensures your WordPress integration is production-ready and helps prevent common configuration issues that could interrupt your workflow.
