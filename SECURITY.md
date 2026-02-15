# Security Policy

## Supported Versions

| Version | Supported          |
| ------- | ------------------ |
| 0.1.x   | :white_check_mark: |

## Security Features

Selfware includes several security mechanisms:

### Path Validation
- All file operations are validated against allowed paths
- Symlink traversal attacks are detected
- System paths (`/etc`, `/usr`, etc.) are protected by default

### Command Filtering
- Dangerous shell commands are blocked (e.g., `rm -rf /`, `mkfs`)
- Command injection patterns are detected
- Protected branch restrictions for git operations

### Safe Defaults
- YOLO mode is disabled by default
- Force push to main/master requires explicit configuration
- Sensitive paths (`.env`, `.ssh`, `.aws`) are denied by default

## Reporting a Vulnerability

If you discover a security vulnerability, please report it responsibly:

1. **DO NOT** open a public issue
2. Email security concerns to the maintainers (see repository contact)
3. Include:
   - Description of the vulnerability
   - Steps to reproduce
   - Potential impact
   - Suggested fix (if any)

### Response Timeline

- **Acknowledgment**: Within 48 hours
- **Initial Assessment**: Within 1 week
- **Fix Timeline**: Depends on severity
  - Critical: 24-72 hours
  - High: 1-2 weeks
  - Medium: 2-4 weeks
  - Low: Next release cycle

## Security Best Practices

When using Selfware:

1. **Run with least privilege** - Don't run as root
2. **Use specific allowed paths** - Avoid `./**` in production
3. **Review before enabling YOLO** - Understand what auto-approval means
4. **Keep updated** - Security fixes are released promptly
5. **Audit tool calls** - Review what the agent is doing

### Example Secure Configuration

```toml
[safety]
# Restrict to specific directories
allowed_paths = [
    "./src/**",
    "./tests/**",
    "./docs/**",
]

# Explicit denials
denied_paths = [
    "**/.env",
    "**/.env.*",
    "**/secrets/**",
    "**/.ssh/**",
    "**/.aws/**",
]

# Protect important branches
protected_branches = ["main", "master", "production"]

# Require confirmation for destructive operations
require_confirmation = ["git push", "rm"]
```

## Known Limitations

1. **Shell command validation** uses regex-based pattern matching with obfuscation detection - while significantly more robust than simple string matching, extremely sophisticated obfuscation may still bypass it
2. **TOCTOU** (Time-of-Check-Time-of-Use) race conditions are mitigated by symlink chain validation but cannot be completely eliminated
3. **LLM output sanitization** is best-effort - malicious prompts may produce unexpected tool calls

## Security Enhancements (v0.1.0)

### Shell Command Validation
- Regex-based pattern matching instead of simple string contains
- Command normalization to detect obfuscation (whitespace collapsing, slash normalization)
- Command chain detection (`;`, `&&`, `||` separated commands are individually validated)
- Base64-encoded command execution detection
- Netcat reverse shell detection
- Eval with command substitution detection

### Path Traversal Protection
- Symlink chain validation (detects loops and chains to protected paths)
- Canonical path validation only (original path not checked against allowed patterns)
- Protected system path detection via symlinks
- Maximum symlink depth enforcement (40 levels, matching Linux default)

## Security Updates

Security updates are announced via:
- GitHub Security Advisories
- Release notes
- Changelog

Enable Dependabot in your fork to receive automatic security updates.
