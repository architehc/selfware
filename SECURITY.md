# Security Policy

## Supported Versions

| Version | Supported          |
|---------|--------------------|
| 0.1.x   | Yes                |
| < 0.1   | No                 |

## Security Features

Selfware includes several built-in security mechanisms:

- **Safety Layer:** All tool invocations pass through a safety layer that evaluates operations before execution, blocking potentially destructive actions.
- **Path Validation:** File system operations are validated to prevent path traversal attacks and unauthorized access outside the designated workspace.
- **Command Filtering:** Shell commands are inspected and filtered to block known-dangerous patterns (e.g., recursive deletion of system directories, privilege escalation attempts).

These features are enabled by default and are integral to the agent runtime. They should not be disabled in production use.

## Reporting a Vulnerability

If you discover a security vulnerability in Selfware, please report it responsibly.

**Do not open a public GitHub issue for security vulnerabilities.**

### How to Report

1. Send an email to **security@architehc.com** with the following information:
   - Description of the vulnerability.
   - Steps to reproduce.
   - Potential impact assessment.
   - Any suggested mitigations, if applicable.

2. You will receive an acknowledgment within **48 hours**.

3. We aim to provide an initial assessment within **5 business days** and will work with you on a coordinated disclosure timeline.

4. Once a fix is available, we will publish a security advisory and credit you (unless you prefer to remain anonymous).

## Security Best Practices for Users

- **Keep Selfware updated.** Always run the latest patch release within your supported version line.
- **Review tool permissions.** Understand which tools your agent configuration enables and restrict them to the minimum required set.
- **Use workspace isolation.** Run Selfware within a dedicated workspace directory to limit the scope of file system access.
- **Audit agent journals.** Regularly review checkpoint and journal logs for unexpected tool invocations or anomalous behavior.
- **Do not disable the safety layer.** The safety layer exists to prevent harmful operations. Disabling it removes a critical safeguard.
- **Limit network access.** When running local-first, ensure the host machine does not expose unnecessary network services to the agent runtime.
