from openai import OpenAI

client = OpenAI(base_url='https://crazyshit.ngrok.io/v1', api_key='dummy')

prompt = """You are a security auditor reviewing Rust code. Here are excerpts from 3 critical files. Analyze each for security risks.

=== src/tools/shell.rs (lines 48-166) — Shell command execution ===
Key logic:
- Takes "command", "cwd", "timeout_secs", "env" from JSON args
- Validates command length <= 10,000 chars
- Calls find_dangerous_shell_pattern() to block dangerous patterns
- Validates cwd is absolute and has no ".." components
- Validates env var names/values have no null bytes
- Runs: sh -c "${args.command}" via tokio::process::Command
- Uses kill_on_drop(true) and timeout

=== src/tools/pty_shell.rs (lines 74-120) — Interactive shell sessions ===
Key logic:
- Spawns persistent shell sessions with piped stdin/stdout/stderr
- Injects __SELFWARE_CMD_DONE_ marker after commands
- Strips ANSI codes from output
- MAX_COMMAND_LENGTH = 10,000
- MAX_SESSIONS = 5
- IDLE_TIMEOUT = 30 minutes
- Rate limit: 1 second between commands
- No explicit dangerous pattern filtering (unlike shell.rs)

=== src/config/api_key.rs (full file, 83 lines) — API key handling ===
Key logic:
- load_api_key_from_keyring() uses OS keyring via keyring crate
- save_api_key_to_keyring() saves to OS keyring
- Falls back to ConfigFile (plaintext TOML) if keyring unavailable
- is_local_endpoint() checks for localhost/127.0.0.1

=== src/safety/path_validator.rs (lines 89-120) ===
Key logic:
- Rejects null bytes in paths (prevents C-string truncation)
- Rejects Unicode homoglyphs
- Uses O_NOFOLLOW on Unix to prevent symlink races
- Canonicalizes paths before validation
- Checks against allowed_paths/denied_paths glob patterns

=== src/safety/permissions.rs (full file, 232 lines) ===
Key logic:
- PermissionGrant with tool_pattern (glob), resource_pattern, expires_at
- pattern_matches uses simple glob with * wildcard
- No path traversal check in pattern matching

For each file, give: risk found, severity, line reference, fix recommendation.
Then an overall risk score (1-10) and top 3 priority fixes.
"""

resp = client.chat.completions.create(
    model="/media/thread/trebuchet6/qwen35/models/Qwen3.5-122B-A10B-NVFP4-yarn-1010k",
    messages=[{"role": "user", "content": prompt}],
    max_tokens=4096,
    temperature=0.2,
    extra_body={"chat_template_kwargs": {"enable_thinking": False}},
)
print(resp.choices[0].message.content)
print(f"\n--- Tokens: {resp.usage}")
