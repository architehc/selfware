//! Container Input Validation
//!
//! Validation functions for container tool inputs.

pub const SHELL_METACHARACTERS: &[char] = &[
    '`', '$', '(', ')', '|', ';', '&', '!', '<', '>', '\n', '\r', '\0',
];

pub fn validate_port_mapping(mapping: &str) -> bool {
    let (port_part, proto) = if let Some(idx) = mapping.rfind('/') {
        let (p, pr) = mapping.split_at(idx);
        // pr starts with '/'; trailing '/' with no protocol is invalid
        if pr.len() <= 1 {
            return false;
        }
        (p, Some(&pr[1..]))
    } else {
        (mapping, None)
    };
    if let Some(proto) = proto {
        if proto != "tcp" && proto != "udp" {
            return false;
        }
    }
    if mapping.contains(SHELL_METACHARACTERS) {
        return false;
    }
    let parts: Vec<&str> = port_part.split(':').collect();
    match parts.len() {
        2 => is_valid_port(parts[0]) && is_valid_port(parts[1]),
        3 => {
            let ip = parts[0];
            !ip.is_empty()
                && ip.chars().all(|c| {
                    c.is_ascii_alphanumeric() || c == '.' || c == ':' || c == '[' || c == ']'
                })
                && is_valid_port(parts[1])
                && is_valid_port(parts[2])
        }
        _ => false,
    }
}

pub fn is_valid_port(s: &str) -> bool {
    if s.is_empty() {
        return false;
    }
    matches!(s.parse::<u16>(), Ok(p) if p >= 1)
}

pub fn validate_volume_spec(spec: &str) -> bool {
    if spec.contains(SHELL_METACHARACTERS) {
        return false;
    }
    let parts: Vec<&str> = spec.splitn(3, ':').collect();
    match parts.len() {
        2 => {
            let host = parts[0];
            let container = parts[1];
            !host.is_empty() && !container.is_empty() && container.starts_with('/')
        }
        3 => {
            let host = parts[0];
            let container = parts[1];
            let opts = parts[2];
            !host.is_empty()
                && !container.is_empty()
                && container.starts_with('/')
                && matches!(
                    opts,
                    "ro" | "rw" | "z" | "Z" | "ro,z" | "rw,z" | "ro,Z" | "rw,Z"
                )
        }
        _ => false,
    }
}
