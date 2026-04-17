# Task: Deep security audit of selfware

Read these specific files and write a security report to /home/ivo/selfware/swarm_outputs/SECURITY_AUDIT.md:

1. src/tools/shell.rs — check for shell injection risks
2. src/tools/pty_shell.rs — check for shell injection risks  
3. src/config/api_key.rs — check how API keys are stored/handled
4. src/safety/confirm.rs — check what safety confirmations exist
5. src/safety/path_validator.rs — check path traversal protections
6. src/safety/permissions.rs — check permission model

For each file, write:
- What it does
- Security risks found (with line numbers)
- Severity (Critical/High/Medium/Low)
- Recommended fix

Be thorough. Write the full report to the file using file_write.
