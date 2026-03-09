# Privacy Policy

**Selfware does not collect, transmit, or store any of your data outside your machine.**

That's the entire policy. Here are the details:

## What Selfware Does NOT Do

- Does not send telemetry to any server
- Does not phone home for updates
- Does not collect usage statistics
- Does not track which files you edit
- Does not log your prompts to any cloud service
- Does not require an account or registration
- Does not include any third-party analytics or tracking

## What Stays on Your Machine

- All code analysis and generation
- All conversation history (`~/.selfware/sessions/`)
- All audit logs (`~/.selfware/audit/`)
- All memory and learned patterns (`~/.selfware/memory/`)
- All configuration (`~/.config/selfware/`)

## Network Requests

Selfware makes network requests ONLY to:

1. **Your configured LLM endpoint** (e.g., `http://localhost:8000/v1`) — to send prompts and receive completions
2. **MCP servers you explicitly configure** — to connect to tool servers you choose to run

No other network requests are made. Ever.

In air-gapped mode, ALL network requests are disabled.

## Encryption

Session data can be encrypted at rest using AES-256-GCM with a key derived from your OS keychain (PBKDF2-HMAC-SHA256, 100,000 iterations).

## Your Rights

- You can delete all Selfware data: `rm -rf ~/.selfware ~/.config/selfware`
- You can audit all network traffic: Selfware only connects to endpoints you configure
- You can read every line of code: MIT licensed, no proprietary blobs
