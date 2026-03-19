# Tool Reference

Selfware has 70+ built-in tools organized by category. The agent selects and invokes tools automatically based on the task. You can list all registered tools in interactive mode with `/tools`. Additional tools can be added at runtime via [MCP servers](mcp.md).

## File Operations

### `file_read`

Read file contents. Use for examining code, configs, or any text file.

**Parameters:**
- `path` (string, required) -- file path to read
- `offset` (number) -- line offset to start reading from
- `limit` (number) -- maximum number of lines to read

**Example:**
```json
{ "path": "src/main.rs", "offset": 0, "limit": 100 }
```

### `file_write`

Write or overwrite entire file. Creates parent directories if needed.

**Parameters:**
- `path` (string, required) -- file path to write
- `content` (string, required) -- full file content

**Example:**
```json
{ "path": "src/utils.rs", "content": "pub fn hello() {\n    println!(\"hello\");\n}\n" }
```

### `file_edit`

Apply surgical edit to file. The `old_str` must match EXACTLY once. Include enough context to ensure a unique match.

**Parameters:**
- `path` (string, required) -- file path to edit
- `old_str` (string, required) -- exact text to find (must be unique in file)
- `new_str` (string, required) -- replacement text

**Example:**
```json
{
  "path": "src/main.rs",
  "old_str": "fn main() {\n    println!(\"hello\");\n}",
  "new_str": "fn main() {\n    println!(\"goodbye\");\n}"
}
```

### `file_delete`

Delete a file. Irreversible without version control. Requires confirmation in normal mode.

**Parameters:**
- `path` (string, required) -- file path to delete

### `directory_tree`

List directory structure. Use to understand project layout.

**Parameters:**
- `path` (string, required) -- directory path
- `depth` (number) -- maximum depth to recurse
- `show_hidden` (boolean) -- include hidden files

---

## Search

### `grep_search`

Search for regex patterns in files. Returns matching lines with context.

**Parameters:**
- `pattern` (string, required) -- regex pattern to search for
- `path` (string) -- directory to search in (default: current directory)
- `include` (string) -- glob pattern to filter files (e.g., `"*.rs"`)
- `context_lines` (number) -- lines of context around matches

**Example:**
```json
{ "pattern": "fn\\s+main", "path": "src/", "include": "*.rs" }
```

### `glob_find`

Find files by glob pattern. Returns file paths with metadata.

**Parameters:**
- `pattern` (string, required) -- glob pattern (e.g., `"**/*.rs"`, `"src/**/*.ts"`)
- `path` (string) -- base directory

**Example:**
```json
{ "pattern": "**/*.toml", "path": "." }
```

### `symbol_search`

Find function, struct, enum, trait, or impl definitions in Rust code.

**Parameters:**
- `query` (string, required) -- symbol name or pattern to search for
- `path` (string) -- directory to search in

---

## Shell

### `shell_exec`

Execute a shell command. Runs with configurable timeout (max 1 hour). Requires confirmation in normal mode.

**Parameters:**
- `command` (string, required) -- shell command to execute
- `working_dir` (string) -- working directory for the command
- `timeout_secs` (number) -- timeout in seconds

**Example:**
```json
{ "command": "ls -la src/", "timeout_secs": 30 }
```

### `pty_shell`

Interactive shell sessions that persist across invocations. Use for commands that need a persistent environment (e.g., virtualenvs, nvm).

**Parameters:**
- `action` (string, required) -- `"create"`, `"send"`, `"read"`, `"close"`, or `"list"`
- `session_id` (string) -- session identifier
- `command` (string) -- command to send
- `timeout_secs` (number) -- timeout

---

## Git

### `git_status`

Get current git status including branch, staged/unstaged changes.

**Parameters:**
- `path` (string) -- repository path

### `git_diff`

Show diff of changes. Can diff working tree, staged, or between commits.

**Parameters:**
- `staged` (boolean) -- show staged changes only
- `commit` (string) -- compare against a specific commit
- `path` (string) -- limit diff to a specific path

### `git_commit`

Stage files and create a commit. Use conventional commit format.

**Parameters:**
- `message` (string, required) -- commit message
- `files` (array of strings) -- files to stage (empty = all changes)

**Example:**
```json
{ "message": "fix: handle edge case in parser", "files": ["src/parser.rs"] }
```

### `git_push`

Push commits to a remote repository. Force push is blocked by the safety checker. Requires confirmation.

**Parameters:**
- `remote` (string) -- remote name (default: `"origin"`)
- `branch` (string) -- branch to push

### `git_checkpoint`

Create a git checkpoint (commit) before dangerous operations. Returns commit hash for rollback.

**Parameters:**
- `message` (string) -- checkpoint description

---

## Cargo (Rust Build Tools)

### `cargo_test`

Run `cargo test` with structured output parsing. Returns detailed test results with pass/fail status, failure messages, and locations.

**Parameters:**
- `test_name` (string) -- specific test or pattern to run
- `package` (string) -- package to test
- `features` (string) -- feature flags

### `cargo_check`

Run `cargo check` with structured error parsing. Returns compiler errors with file locations, error codes, and suggestions.

**Parameters:**
- `package` (string) -- package to check
- `features` (string) -- feature flags

### `cargo_clippy`

Run `cargo clippy` with structured lint parsing. Returns categorized lints with severity levels and fix suggestions.

**Parameters:**
- `package` (string) -- package to lint
- `fix` (boolean) -- auto-apply fixes

### `cargo_fmt`

Run `cargo fmt` to format code. Use `--check` to verify formatting without changes.

**Parameters:**
- `check` (boolean) -- check only, don't modify files
- `path` (string) -- specific file to format

---

## Browser Automation

### `browser_fetch`

Fetch a web page and return its HTML content (uses headless browser or curl).

**Parameters:**
- `url` (string, required) -- URL to fetch
- `selector` (string) -- CSS selector to extract specific content
- `timeout_secs` (number) -- timeout

### `browser_screenshot`

Take a screenshot of a web page (requires Chrome/Chromium).

**Parameters:**
- `url` (string, required) -- URL to screenshot
- `output_path` (string) -- where to save the screenshot
- `width` (number) -- viewport width
- `height` (number) -- viewport height
- `full_page` (boolean) -- capture full page or viewport only

### `browser_pdf`

Save a web page as PDF (requires Chrome/Chromium).

**Parameters:**
- `url` (string, required) -- URL to render
- `output_path` (string, required) -- PDF output path

### `browser_eval`

Load a page and execute JavaScript, returning the result.

**Parameters:**
- `url` (string, required) -- URL to load
- `script` (string, required) -- JavaScript to execute

### `browser_links`

Extract all links from a web page.

**Parameters:**
- `url` (string, required) -- URL to extract links from

### `page_control`

Full Playwright-based browser automation with 28 actions. Spawns a companion Node.js process that communicates over NDJSON. Falls back to the existing `browser_*` fetch tools if Playwright is unavailable. Supports multi-tab browsing with up to 5 concurrent pages.

**Parameters:**
- `action` (string, required) -- one of the 28 supported actions (see below)
- `selector` (string) -- CSS selector for interaction actions
- `value` (string) -- value for type/fill/select actions
- `url` (string) -- URL for `goto` action
- `key` (string) -- key for `press` action
- `attribute` (string) -- attribute name for `attribute` action
- `script` (string) -- JavaScript for `evaluate` / `evaluate_handle` actions
- `tab_index` (number) -- tab index for `switch_tab` / `close_tab`
- `timeout_ms` (number) -- per-action timeout in milliseconds (default: 30000)

**Supported actions:**

| Category | Actions |
|----------|---------|
| Navigation | `goto`, `back`, `forward`, `reload`, `wait_for` |
| Interaction | `click`, `type`, `fill`, `select`, `check`, `uncheck`, `hover`, `press` |
| Content extraction | `text`, `html`, `attribute`, `value`, `count`, `visible` |
| Page info | `title`, `url`, `screenshot`, `pdf` |
| JavaScript | `evaluate`, `evaluate_handle` |
| Multi-tab | `new_tab`, `switch_tab`, `close_tab`, `list_tabs` |
| Lifecycle | `shutdown` |

**Example:**
```json
{ "action": "goto", "url": "https://example.com" }
```
```json
{ "action": "click", "selector": "#submit-button" }
```
```json
{ "action": "fill", "selector": "input[name=email]", "value": "user@example.com" }
```

---

## Computer Control

Desktop automation tools for GUI interaction. Requires accessibility permissions on macOS.

### `computer_mouse`

Control the mouse: move, click, scroll, drag.

**Parameters:**
- `action` (string, required) -- `"move_to"`, `"click"`, `"double_click"`, `"right_click"`, `"scroll"`, `"drag"`
- `x` (number) -- x coordinate
- `y` (number) -- y coordinate
- `scroll_x` (number) -- horizontal scroll amount
- `scroll_y` (number) -- vertical scroll amount

### `computer_keyboard`

Control the keyboard: type text, press keys, key combinations.

**Parameters:**
- `action` (string, required) -- `"type"`, `"press"`, `"combo"`
- `text` (string) -- text to type
- `key` (string) -- key to press
- `modifiers` (array of strings) -- modifier keys (e.g., `["ctrl", "shift"]`)

### `computer_screen`

Capture the screen: full screen or a specific region. Returns base64 PNG.

**Parameters:**
- `region` (object) -- optional region `{ x, y, width, height }`

### `computer_window`

Manage desktop windows: list visible windows, focus a window, get active window, launch applications.

**Parameters:**
- `action` (string, required) -- `"list"`, `"focus"`, `"active"`, `"launch"`
- `title` (string) -- window title to focus
- `app` (string) -- application to launch

---

## Package Managers

### `npm_install`

Install npm packages. Can install specific packages or all dependencies from `package.json`.

**Parameters:**
- `packages` (array of strings) -- packages to install (empty = install all)
- `dev` (boolean) -- install as dev dependency
- `path` (string) -- working directory

### `npm_run`

Run an npm script defined in `package.json`.

**Parameters:**
- `script` (string, required) -- script name (e.g., `"build"`, `"dev"`)
- `path` (string) -- working directory

### `npm_scripts`

List available npm scripts from `package.json`.

**Parameters:**
- `path` (string) -- working directory

### `pip_install`

Install Python packages using pip.

**Parameters:**
- `packages` (array of strings) -- packages to install
- `requirements` (string) -- path to requirements.txt
- `path` (string) -- working directory

### `pip_list`

List installed Python packages.

### `pip_freeze`

Output installed packages in `requirements.txt` format.

### `yarn_install`

Install packages using Yarn.

**Parameters:**
- `packages` (array of strings) -- packages to install
- `dev` (boolean) -- install as dev dependency
- `path` (string) -- working directory

---

## Container Tools (Docker/Podman)

### `container_run`

Run a container from an image.

**Parameters:**
- `image` (string, required) -- container image
- `name` (string) -- container name
- `ports` (array of strings) -- port mappings (e.g., `["8080:80"]`)
- `env` (object) -- environment variables
- `volumes` (array of strings) -- volume mounts
- `detach` (boolean) -- run in background
- `command` (string) -- override command

### `container_stop`

Stop a running container.

**Parameters:**
- `container` (string, required) -- container name or ID

### `container_list`

List containers (running by default, or all with `all: true`).

**Parameters:**
- `all` (boolean) -- include stopped containers

### `container_logs`

Get logs from a container.

**Parameters:**
- `container` (string, required) -- container name or ID
- `tail` (number) -- last N lines
- `follow` (boolean) -- stream logs

### `container_exec`

Execute a command inside a running container.

**Parameters:**
- `container` (string, required) -- container name or ID
- `command` (string, required) -- command to execute

### `container_build`

Build a container image from a Dockerfile.

**Parameters:**
- `path` (string, required) -- build context path
- `tag` (string) -- image tag
- `dockerfile` (string) -- Dockerfile path

### `container_images`

List container images.

### `container_pull`

Pull a container image from a registry.

**Parameters:**
- `image` (string, required) -- image to pull

### `container_remove`

Remove a stopped container (use `force` for running containers).

**Parameters:**
- `container` (string, required) -- container name or ID
- `force` (boolean) -- force remove running container

### `compose_up`

Start services defined in `docker-compose.yml`.

**Parameters:**
- `path` (string) -- directory containing compose file
- `detach` (boolean) -- run in background
- `services` (array of strings) -- specific services to start

### `compose_down`

Stop and remove containers defined in `docker-compose.yml`.

**Parameters:**
- `path` (string) -- directory containing compose file
- `volumes` (boolean) -- also remove volumes

---

## Process Management

### `process_start`

Start a background process (e.g., dev server, file watcher). Persists across agent steps.

**Parameters:**
- `command` (string, required) -- command to run
- `name` (string) -- human-readable process name
- `working_dir` (string) -- working directory

### `process_stop`

Stop a managed background process. Use `force=true` for immediate termination.

**Parameters:**
- `name` (string, required) -- process name
- `force` (boolean) -- SIGKILL instead of SIGTERM

### `process_list`

List all managed background processes with their status, uptime, and recent logs.

### `process_logs`

Get recent log output from a managed process.

**Parameters:**
- `name` (string, required) -- process name
- `lines` (number) -- number of log lines to return

### `process_restart`

Restart a managed process.

**Parameters:**
- `name` (string, required) -- process name

### `port_check`

Check port availability and find open ports.

**Parameters:**
- `port` (number, required) -- port to check
- `host` (string) -- host to check (default: `"localhost"`)

---

## LSP (Language Server Protocol)

Code intelligence tools powered by an LSP client. Requires a running language server for the target language.

### `lsp_goto_definition`

Go to the definition of a symbol.

**Parameters:**
- `file` (string, required) -- source file path
- `line` (number, required) -- cursor line (0-indexed)
- `column` (number, required) -- cursor column (0-indexed)

### `lsp_find_references`

Find all references to a symbol.

**Parameters:**
- `file` (string, required) -- source file path
- `line` (number, required) -- cursor line
- `column` (number, required) -- cursor column

### `lsp_document_symbols`

List all symbols in a source file -- functions, structs, classes, methods, constants, etc.

**Parameters:**
- `file` (string, required) -- source file path

### `lsp_hover`

Get hover information for a symbol -- type signatures, documentation, and other details.

**Parameters:**
- `file` (string, required) -- source file path
- `line` (number, required) -- cursor line
- `column` (number, required) -- cursor column

---

## Knowledge Graph

Build and query a session-local knowledge graph for reasoning about entities and relationships.

### `knowledge_add`

Add a node (entity, fact, concept) to the knowledge graph.

**Parameters:**
- `id` (string, required) -- unique node identifier
- `label` (string, required) -- node label/type
- `properties` (object) -- key-value properties

### `knowledge_relate`

Create a relationship between two nodes.

**Parameters:**
- `from` (string, required) -- source node ID
- `to` (string, required) -- target node ID
- `relation` (string, required) -- relationship type (e.g., `"depends_on"`, `"contains"`)

### `knowledge_query`

Query the knowledge graph for nodes and relationships.

**Parameters:**
- `query` (string, required) -- query string or node ID
- `depth` (number) -- traversal depth

### `knowledge_stats`

Get statistics about the knowledge graph.

### `knowledge_clear`

Clear all nodes and edges from the knowledge graph.

### `knowledge_remove`

Remove a node and its edges from the knowledge graph.

**Parameters:**
- `id` (string, required) -- node ID to remove

### `knowledge_export`

Export the knowledge graph to a JSON file.

**Parameters:**
- `path` (string, required) -- output file path

---

## Vision

### `vision_analyze`

Analyze an image using a vision-capable LLM. Send an image from file or URL with a question.

**Parameters:**
- `image` (string, required) -- image file path or URL
- `prompt` (string, required) -- question about the image

### `vision_compare`

Compare two images pixel-by-pixel and return a similarity score (0-100).

**Parameters:**
- `image1` (string, required) -- first image path
- `image2` (string, required) -- second image path

---

## HTTP

### `http_request`

Make HTTP requests to APIs or web endpoints. Supports GET, POST, PUT, DELETE methods.

**Parameters:**
- `url` (string, required) -- request URL
- `method` (string) -- HTTP method (default: `"GET"`)
- `headers` (object) -- request headers
- `body` (string) -- request body
- `timeout_secs` (number) -- timeout (max 5 minutes)

---

## Screen Capture

### `screen_capture`

Capture a screenshot of the screen, a specific window, or a region. Returns the image path.

**Parameters:**
- `target` (string) -- `"screen"`, `"window"`, or `"region"`
- `window_title` (string) -- window to capture
- `region` (object) -- region `{ x, y, width, height }`
- `output_path` (string) -- where to save the screenshot

---

## FIM (Fill-in-the-Middle)

### `fim_edit`

Use intelligent Fill-in-the-Middle to replace a block of code. Provide the path, start/end lines of the block to replace, and an instruction for what should go there.

**Parameters:**
- `path` (string, required) -- file path
- `start_line` (number, required) -- first line of the block
- `end_line` (number, required) -- last line of the block
- `instruction` (string, required) -- what the replacement code should do
