#![no_main]
use libfuzzer_sys::fuzz_target;
use std::path::PathBuf;
use selfware::config::SafetyConfig;
use selfware::safety::path_validator::PathValidator;

fuzz_target!(|data: &[u8]| {
    if let Ok(path_str) = std::str::from_utf8(data) {
        let config = SafetyConfig {
            allowed_paths: vec!["/tmp/sandbox".to_string(), ".".to_string()],
            denied_paths: vec!["/etc".to_string(), "/root".to_string(), "**/.env".to_string()],
            protected_branches: vec!["main".to_string()],
            require_confirmation: vec![],
            strict_permissions: false,
        };
        let validator = PathValidator::new(&config, PathBuf::from("/tmp/sandbox"));
        let _ = validator.validate(path_str);
    }
});