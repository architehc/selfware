# MCP Integration Guide

Selfware supports the [Model Context Protocol (MCP)](https://modelcontextprotocol.io/) to connect to external tool servers. MCP servers expose tools that selfware can use alongside its built-in tools.

## How It Works

MCP uses JSON-RPC 2.0 over stdio transport. Each configured MCP server runs as a child process that communicates via stdin/stdout. On startup, selfware:

1. Spawns each configured MCP server process
2. Sends an `initialize` request
3. Discovers available tools via `tools/list`
4. Registers discovered tools in the agent's tool registry

MCP tools then appear alongside built-in tools and can be called by the LLM during task execution.

## Configuration

Add MCP servers in `selfware.toml`:

```toml
[[mcp.servers]]
name = "github"                                      # Human-readable name
command = "npx"                                       # Command to spawn
args = ["-y", "@modelcontextprotocol/server-github"]  # Arguments
env = { GITHUB_TOKEN = "ghp_..." }                   # Environment variables
init_timeout_secs = 30                                # Startup timeout (default: 30)
```

### Fields

| Field | Type | Required | Default | Description |
|-------|------|----------|---------|-------------|
| `name` | string | yes | -- | Human-readable server name |
| `command` | string | yes | -- | Command to spawn the server process |
| `args` | array | no | `[]` | Command arguments |
| `env` | object | no | `{}` | Environment variables for the process |
| `init_timeout_secs` | number | no | `30` | Seconds to wait for initialization |

## Popular MCP Servers

### GitHub

Create issues, PRs, search repos, manage branches:

```toml
[[mcp.servers]]
name = "github"
command = "npx"
args = ["-y", "@modelcontextprotocol/server-github"]
env = { GITHUB_TOKEN = "ghp_your_token_here" }
```

### Filesystem

Extended filesystem access with search capabilities:

```toml
[[mcp.servers]]
name = "filesystem"
command = "npx"
args = ["-y", "@modelcontextprotocol/server-filesystem", "/path/to/allowed/dir"]
```

### PostgreSQL

Query and manage PostgreSQL databases:

```toml
[[mcp.servers]]
name = "postgres"
command = "npx"
args = ["-y", "@modelcontextprotocol/server-postgres"]
env = { DATABASE_URL = "postgresql://user:pass@localhost/dbname" }
```

### SQLite

Read and query SQLite databases:

```toml
[[mcp.servers]]
name = "sqlite"
command = "npx"
args = ["-y", "@modelcontextprotocol/server-sqlite", "--db-path", "./data.db"]
```

### Puppeteer (Browser Automation)

Full browser automation with navigation, screenshots, and interaction:

```toml
[[mcp.servers]]
name = "puppeteer"
command = "npx"
args = ["-y", "@modelcontextprotocol/server-puppeteer"]
```

### Brave Search

Web search via Brave Search API:

```toml
[[mcp.servers]]
name = "brave-search"
command = "npx"
args = ["-y", "@modelcontextprotocol/server-brave-search"]
env = { BRAVE_API_KEY = "BSA_your_key_here" }
```

### Slack

Send and read Slack messages:

```toml
[[mcp.servers]]
name = "slack"
command = "npx"
args = ["-y", "@modelcontextprotocol/server-slack"]
env = { SLACK_BOT_TOKEN = "xoxb-your-token" }
```

### Memory (Persistent Knowledge Graph)

Persistent entity-relationship memory across sessions:

```toml
[[mcp.servers]]
name = "memory"
command = "npx"
args = ["-y", "@modelcontextprotocol/server-memory"]
```

## Multiple Servers

You can configure multiple MCP servers. Each gets its own `[[mcp.servers]]` entry:

```toml
[[mcp.servers]]
name = "github"
command = "npx"
args = ["-y", "@modelcontextprotocol/server-github"]
env = { GITHUB_TOKEN = "ghp_..." }

[[mcp.servers]]
name = "postgres"
command = "npx"
args = ["-y", "@modelcontextprotocol/server-postgres"]
env = { DATABASE_URL = "postgresql://..." }

[[mcp.servers]]
name = "slack"
command = "npx"
args = ["-y", "@modelcontextprotocol/server-slack"]
env = { SLACK_BOT_TOKEN = "xoxb-..." }
```

## Writing Custom MCP Servers

Any program that speaks MCP over stdio can be used. The server must:

1. Read JSON-RPC 2.0 messages from stdin
2. Write JSON-RPC 2.0 responses to stdout
3. Respond to `initialize`, `tools/list`, and `tools/call` methods

Popular SDKs for building MCP servers:

- **TypeScript**: `@modelcontextprotocol/sdk` (npm)
- **Python**: `mcp` (pip)
- **Rust**: implement the JSON-RPC protocol directly

Example custom server:

```toml
[[mcp.servers]]
name = "my-tools"
command = "python"
args = ["-m", "my_mcp_server"]
env = { MY_API_KEY = "..." }
init_timeout_secs = 15
```

## Troubleshooting

**Server fails to start:**
- Check that the command exists (`which npx`, `which python`)
- Run `selfware doctor` to verify Node.js is installed
- Try running the command manually to see error output

**Tools not discovered:**
- Increase `init_timeout_secs` for slow-starting servers
- Check server logs (stderr is forwarded to selfware's log output)
- Enable debug logging: `SELFWARE_LOG_LEVEL=debug selfware chat`

**Environment variables not working:**
- Verify the `env` table syntax in TOML (use `{ KEY = "value" }` format)
- Check that tokens/keys are valid
