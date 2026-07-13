//! Configuration Profiles
//!
//! Pre-configured settings for different use cases

use crate::config::Config;
use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Profile definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Profile {
    pub name: String,
    pub description: String,
    pub config_overrides: ConfigOverrides,
}

/// Configuration overrides for a profile
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ConfigOverrides {
    pub max_tokens: Option<usize>,
    pub temperature: Option<f32>,
    pub max_iterations: Option<usize>,
    pub step_timeout_secs: Option<u64>,
    pub concurrency: Option<ConcurrencyOverrides>,
    pub agent: Option<AgentOverrides>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConcurrencyOverrides {
    pub max_parallel_requests: Option<usize>,
    pub timeout_secs: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentOverrides {
    pub streaming: Option<bool>,
    pub native_function_calling: Option<bool>,
    pub enable_thinking: Option<bool>,
}

/// Profile manager
pub struct ProfileManager {
    profiles: HashMap<String, Profile>,
}

impl ProfileManager {
    /// Create with built-in profiles
    pub fn new() -> Self {
        let mut profiles = HashMap::new();

        // Single powerful agent (architect mode)
        profiles.insert(
            "architect".to_string(),
            Profile {
                name: "Architect".to_string(),
                description: "Deep thinking, single agent for complex design tasks".to_string(),
                config_overrides: ConfigOverrides {
                    max_tokens: Some(8192),
                    temperature: Some(0.7),
                    max_iterations: Some(100),
                    step_timeout_secs: Some(900),
                    concurrency: Some(ConcurrencyOverrides {
                        max_parallel_requests: Some(4),
                        timeout_secs: Some(120),
                    }),
                    agent: Some(AgentOverrides {
                        streaming: Some(false),
                        native_function_calling: Some(true),
                        enable_thinking: Some(true),
                    }),
                },
            },
        );

        // 8-agent swarm
        profiles.insert(
            "swarm-8".to_string(),
            Profile {
                name: "Swarm-8".to_string(),
                description: "8 concurrent agents for collaborative tasks".to_string(),
                config_overrides: ConfigOverrides {
                    max_tokens: Some(4096),
                    temperature: Some(0.6),
                    max_iterations: Some(50),
                    step_timeout_secs: Some(600),
                    concurrency: Some(ConcurrencyOverrides {
                        max_parallel_requests: Some(16),
                        timeout_secs: Some(300),
                    }),
                    agent: Some(AgentOverrides {
                        streaming: Some(false),
                        native_function_calling: Some(true),
                        enable_thinking: Some(false),
                    }),
                },
            },
        );

        // 16-agent batch (sweet spot for 2x4090)
        profiles.insert(
            "batch-16".to_string(),
            Profile {
                name: "Batch-16".to_string(),
                description: "16 concurrent agents - optimal for 2x RTX 4090".to_string(),
                config_overrides: ConfigOverrides {
                    max_tokens: Some(4096),
                    temperature: Some(0.6),
                    max_iterations: Some(30),
                    step_timeout_secs: Some(900),
                    concurrency: Some(ConcurrencyOverrides {
                        max_parallel_requests: Some(24),
                        timeout_secs: Some(600),
                    }),
                    agent: Some(AgentOverrides {
                        streaming: Some(false),
                        native_function_calling: Some(true),
                        enable_thinking: Some(false),
                    }),
                },
            },
        );

        // 32-way batch (direct API only)
        profiles.insert(
            "batch-32".to_string(),
            Profile {
                name: "Batch-32".to_string(),
                description: "Maximum throughput for simple tasks".to_string(),
                config_overrides: ConfigOverrides {
                    max_tokens: Some(2048),
                    temperature: Some(0.5),
                    max_iterations: Some(10),
                    step_timeout_secs: Some(300),
                    concurrency: Some(ConcurrencyOverrides {
                        max_parallel_requests: Some(32),
                        timeout_secs: Some(120),
                    }),
                    agent: Some(AgentOverrides {
                        streaming: Some(false),
                        native_function_calling: Some(false),
                        enable_thinking: Some(false),
                    }),
                },
            },
        );

        // Visual validation mode
        profiles.insert(
            "visual".to_string(),
            Profile {
                name: "Visual".to_string(),
                description: "Website building with visual validation".to_string(),
                config_overrides: ConfigOverrides {
                    max_tokens: Some(4096),
                    temperature: Some(0.6),
                    max_iterations: Some(50),
                    step_timeout_secs: Some(600),
                    concurrency: Some(ConcurrencyOverrides {
                        max_parallel_requests: Some(8),
                        timeout_secs: Some(300),
                    }),
                    agent: Some(AgentOverrides {
                        streaming: Some(false),
                        native_function_calling: Some(true),
                        enable_thinking: Some(false),
                    }),
                },
            },
        );

        // Quick mode (fastest)
        profiles.insert(
            "quick".to_string(),
            Profile {
                name: "Quick".to_string(),
                description: "Fast responses, minimal verification".to_string(),
                config_overrides: ConfigOverrides {
                    max_tokens: Some(2048),
                    temperature: Some(0.5),
                    max_iterations: Some(10),
                    step_timeout_secs: Some(120),
                    concurrency: Some(ConcurrencyOverrides {
                        max_parallel_requests: Some(32),
                        timeout_secs: Some(60),
                    }),
                    agent: Some(AgentOverrides {
                        streaming: Some(false),
                        native_function_calling: Some(false),
                        enable_thinking: Some(false),
                    }),
                },
            },
        );

        Self { profiles }
    }

    /// Get a profile by name
    pub fn get(&self, name: &str) -> Option<&Profile> {
        self.profiles.get(name)
    }

    /// List all available profiles
    pub fn list(&self) -> Vec<&Profile> {
        self.profiles.values().collect()
    }

    /// Apply profile to config
    pub fn apply_profile(&self, config: &mut Config, profile_name: &str) -> Result<()> {
        let profile = self
            .get(profile_name)
            .ok_or_else(|| anyhow::anyhow!("Profile '{}' not found", profile_name))?;

        // Apply every override that has a concrete Config target. (Previously
        // only max_tokens/temperature were applied, so --profile silently
        // dropped max_iterations, step_timeout, streaming, etc.)
        let o = &profile.config_overrides;

        if let Some(max_tokens) = o.max_tokens {
            config.max_tokens = max_tokens;
        }
        if let Some(temperature) = o.temperature {
            config.temperature = temperature;
        }
        if let Some(max_iterations) = o.max_iterations {
            config.agent.max_iterations = max_iterations;
        }
        if let Some(step_timeout_secs) = o.step_timeout_secs {
            config.agent.step_timeout_secs = step_timeout_secs;
        }
        if let Some(agent) = &o.agent {
            if let Some(streaming) = agent.streaming {
                config.agent.streaming = streaming;
            }
            if let Some(native_fc) = agent.native_function_calling {
                config.agent.native_function_calling = native_fc;
            }
            // `enable_thinking` has no direct Config field — it's expressed as
            // chat_template_kwargs in extra_body — so it's intentionally not
            // mapped here rather than silently faked. (Follow-up: wire it into
            // extra_body if profile-level thinking control is wanted.)
        }
        if let Some(concurrency) = &o.concurrency {
            if let Some(max_parallel) = concurrency.max_parallel_requests {
                config.concurrency.max_streams = max_parallel;
            }
            // `timeout_secs` has no ConcurrencyConfig field; not mapped (see above).
        }

        Ok(())
    }

    /// Get profile description
    pub fn describe(&self, name: &str) -> Option<String> {
        self.get(name)
            .map(|p| format!("{}: {}", p.name, p.description))
    }
}

impl Default for ProfileManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_profile_manager() {
        let manager = ProfileManager::new();
        assert!(manager.get("architect").is_some());
        assert!(manager.get("batch-16").is_some());
    }

    #[test]
    fn apply_profile_applies_all_mappable_overrides() {
        let manager = ProfileManager::new();
        let mut config = crate::config::Config::default();
        manager.apply_profile(&mut config, "architect").unwrap();

        // Previously only these two were applied.
        assert_eq!(config.max_tokens, 8192);
        assert!((config.temperature - 0.7).abs() < f32::EPSILON);
        // These were silently dropped before the fix.
        assert_eq!(config.agent.max_iterations, 100);
        assert_eq!(config.agent.step_timeout_secs, 900);
        assert!(!config.agent.streaming);
        assert!(config.agent.native_function_calling);
        assert_eq!(config.concurrency.max_streams, 4);
    }

    #[test]
    fn apply_profile_unknown_name_errors() {
        let manager = ProfileManager::new();
        let mut config = crate::config::Config::default();
        assert!(manager
            .apply_profile(&mut config, "no-such-profile")
            .is_err());
    }

    #[test]
    fn test_list_profiles() {
        let manager = ProfileManager::new();
        let profiles = manager.list();
        assert!(!profiles.is_empty());
    }
}
