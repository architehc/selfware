// Illustrative files for offline reading practice; not workspace snapshots.
export const SAMPLE_FILES = {
  'container_tools.rs': {
    title: 'tools.rs · Container Hypervisor Sandbox',
    lang: 'rust',
    code: `// Selfware Container Hypervisor Execution Engine
// Hardened zero-trust sandbox profile injection

pub fn security_flags(args: &ContainerRunArgs) -> Result<Vec<String>, SandboxError> {
    let mut flags = Vec::new();
    let profile = args.profile.as_deref().unwrap_or("hardened");

    match profile {
        "hardened" => {
            flags.push("--security-opt".into());
            flags.push("no-new-privileges".into());
            flags.push("--pids-limit".into());
            flags.push("1024".into());
            flags.push("--cap-drop".into());
            flags.push("NET_RAW".into());
            flags.push("--cap-drop".into());
            flags.push("MKNOD".into());
        }
        "sealed" => {
            flags.push("--cap-drop".into());
            flags.push("ALL".into());
            flags.push("--read-only".into());
            flags.push("--tmpfs".into());
            flags.push("/tmp:rw,nosuid,nodev,size=256m".into());
            flags.push("--user".into());
            flags.push("65534:65534".into());
        }
        _ => return Err(SandboxError::InvalidProfile(profile.into())),
    }

    // Bind memory-swap strictly to match memory
    flags.push(format!("--memory={}", args.memory));
    flags.push(format!("--memory-swap={}", args.memory));

    Ok(flags)
}`
  },

  'validation.rs': {
    title: 'validation.rs · Safe Host Mount Verifier',
    lang: 'rust',
    code: `// Selfware Volume Mount Sanitization Engine
// Example validator: not a proof of containment

pub fn is_safe_host_mount(host_path: &Path) -> bool {
    let forbidden_prefixes = [
        "/var/run/docker.sock",
        "/proc",
        "/sys",
        "/dev",
        "/etc",
        "/root",
    ];

    for prefix in forbidden_prefixes {
        if host_path.starts_with(prefix) {
            return false;
        }
    }

    // Disallow path traversal attacks
    if host_path.components().any(|c| c == Component::ParentDir) {
        return false;
    }

    // Disallow credential directories
    let path_str = host_path.to_string_lossy();
    if path_str.contains(".ssh") || path_str.contains(".aws") || path_str.contains(".git") {
        return false;
    }

    true
}`
  },

  'radix_cache.py': {
    title: 'radix_cache_stress.py · Prefix Caching Stress Test',
    lang: 'python',
    code: `# SGLang RadixAttention KV Cache Multi-Stream Stress Test
# Example nonce check; not a proof of cache isolation

import asyncio
import hashlib

async def probe_prefix_caching(client, shared_prefix, branch_nonces):
    tasks = []
    for nonce in branch_nonces:
        prompt = f"{shared_prefix}\\nBranch Nonce: {nonce}"
        tasks.append(client.generate(prompt=prompt, max_tokens=64))

    responses = await asyncio.gather(*tasks)
    verified = all(nonce in r.text for nonce, r in zip(branch_nonces, responses))
    assert verified, "Fatal: Cross-stream cache contamination detected!"
    return {"streams": len(branch_nonces), "isolation": "verified"}`
  }
};

