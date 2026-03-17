use colored::*;

use super::*;

/// Context compression strategy - controls how we reduce context size
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompressionStrategy {
    /// Aggressive: reduce to 60% of max context (more room for future messages)
    Aggressive,
    /// Moderate: reduce to 75% of max context (balanced)
    Moderate,
    /// Conservative: reduce to 85% of max context (minimal trimming)
    Conservative,
}

impl Default for CompressionStrategy {
    fn default() -> Self {
        Self::Conservative
    }
}

/// Warning thresholds for context usage
#[allow(dead_code)]
pub const CONTEXT_WARNING_THRESHOLDS: [(f64, &str, &str); 3] = [
    (50.0, "⚠️", "Context at 50% - consider being selective with file references"),
    (75.0, "🟡", "Context at 75% - approaching capacity, /compress may help"),
    (90.0, "🔴", "CRITICAL: Context at 90% - immediate action recommended"),
];

#[allow(dead_code)]
impl Agent {
    // =========================================================================
    // Context Compression & Analytics
    // =========================================================================

    /// Check context usage and warn if approaching limits
    /// Returns true if a warning was displayed
    pub(super) fn check_context_warnings(&self) -> bool {
        let pct = self.context_usage_pct();
        let mut warned = false;

        for &(threshold, icon, message) in &CONTEXT_WARNING_THRESHOLDS {
            if pct >= threshold {
                println!("{} {}", icon.bright_yellow(), message);
                warned = true;
            }
        }

        warned
    }

    /// Get detailed context composition analysis
    pub(super) fn analyze_context_composition(&self) -> ContextComposition {
        let mut system_tokens = 0;
        let mut user_tokens = 0;
        let mut assistant_tokens = 0;
        let mut tool_tokens = 0;
        let mut context_file_tokens = 0;
        let mut message_count = 0;

        for message in &self.messages {
            let tokens = crate::token_count::estimate_tokens_with_overhead(
                &message.content.text_all(),
                4,
            ) + message
                .content
                .image_count()
                * crate::tokens::DEFAULT_IMAGE_TOKEN_ESTIMATE;

            match message.role.as_str() {
                "system" => system_tokens += tokens,
                "user" => {
                    user_tokens += tokens;
                    // Check if this is a context file reference
                    if message.content.contains("// FILE: ") {
                        context_file_tokens += tokens;
                    }
                }
                "assistant" => assistant_tokens += tokens,
                "tool" => tool_tokens += tokens,
                _ => {}
            }
            message_count += 1;
        }

        let total = system_tokens + user_tokens + assistant_tokens + tool_tokens;
        let window = self.memory.context_window();

        ContextComposition {
            total_tokens: total,
            system_tokens,
            user_tokens,
            assistant_tokens,
            tool_tokens,
            context_file_tokens,
            message_count,
            window,
            usage_pct: if window > 0 {
                (total as f64 / window as f64 * 100.0).min(100.0)
            } else {
                0.0
            },
        }
    }

    /// Print detailed context analysis
    pub(super) fn print_context_analysis(&self) {
        let analysis = self.analyze_context_composition();
        
        println!();
        println!("  {}Context Composition Analysis{}", "📊".bright_cyan(), "".dimmed());
        println!("  {}{}", "─".repeat(60).dimmed(), "".dimmed());

        // Total usage
        println!(
            "  {} Total Tokens: {:>12} / {:.0}k ({:.1}% of {} window)",
            if analysis.usage_pct > 90.0 {
                "🔴".bright_red()
            } else if analysis.usage_pct > 75.0 {
                "🟡".bright_yellow()
            } else {
                "🟢".bright_green()
            },
            analysis.total_tokens,
            analysis.total_tokens as f64 / 1000.0,
            analysis.usage_pct,
            analysis.window,
        );

        println!("  {} {}{:>12} tokens{} ({:.1}%)", 
            "System:".dimmed(),
            "\x1b[38;5;245m", // gray
            analysis.system_tokens,
            "\x1b[0m",
            if analysis.total_tokens > 0 {
                analysis.system_tokens as f64 / analysis.total_tokens as f64 * 100.0
            } else {
                0.0
            }
        );
        println!("  {} {}{:>12} tokens{} ({:.1}%)", 
            "User:".dimmed(),
            "\x1b[38;5;111m", // blue
            analysis.user_tokens,
            "\x1b[0m",
            if analysis.total_tokens > 0 {
                analysis.user_tokens as f64 / analysis.total_tokens as f64 * 100.0
            } else {
                0.0
            }
        );
        println!("  {} {}{:>12} tokens{} ({:.1}%)", 
            "Assistant:".dimmed(),
            "\x1b[38;5;109m", // green
            analysis.assistant_tokens,
            "\x1b[0m",
            if analysis.total_tokens > 0 {
                analysis.assistant_tokens as f64 / analysis.total_tokens as f64 * 100.0
            } else {
                0.0
            }
        );
        println!("  {} {}{:>12} tokens{} ({:.1}%)", 
            "Tool Results:".dimmed(),
            "\x1b[38;5;208m", // orange
            analysis.tool_tokens,
            "\x1b[0m",
            if analysis.total_tokens > 0 {
                analysis.tool_tokens as f64 / analysis.total_tokens as f64 * 100.0
            } else {
                0.0
            }
        );

        // Context files breakdown
        println!("  {}  └─ Context Files: {:>10} tokens ({:.1}%)", 
            "User:".dimmed(),
            analysis.context_file_tokens,
            if analysis.total_tokens > 0 {
                analysis.context_file_tokens as f64 / analysis.total_tokens as f64 * 100.0
            } else {
                0.0
            }
        );

        println!("  {} {}{:>12} messages{}", 
            "Messages:".dimmed(),
            "\x1b[38;5;245m",
            analysis.message_count,
            "\x1b[0m"
        );

        // Recommendations
        if analysis.usage_pct > 70.0 {
            println!();
            println!("  {} Recommendations: {}", "💡".bright_yellow(), "".dimmed());
            
            if analysis.user_tokens > 0 && analysis.context_file_tokens > analysis.tool_tokens * 2 {
                println!(
                    "    {} Context files are {}% of user tokens - consider /ctx clear and reload only needed files",
                    "→".dimmed(),
                    (analysis.context_file_tokens as f64 / analysis.user_tokens as f64 * 100.0).max(0.0).min(100.0) as u32
                );
            }

            if analysis.message_count > 100 {
                println!(
                    "    {} High message count ({} messages) - /compress can help reduce history",
                    "→".dimmed(),
                    analysis.message_count
                );
            }

            if analysis.total_tokens > 0 && analysis.tool_tokens as f64 > analysis.total_tokens as f64 * 0.3 {
                println!(
                    "    {} Tool results are {}% of context - consider reviewing tool output verbosity",
                    "→".dimmed(),
                    (analysis.tool_tokens as f64 / analysis.total_tokens as f64 * 100.0) as u32
                );
            }
        }

        println!();
    }

    /// Calculate optimal context window size based on message composition
    pub(super) fn calculate_optimal_window(&self) -> usize {
        let composition = self.analyze_context_composition();
        
        // Ideal: keep 20% buffer for future messages
        let ideal_usage_pct = 0.80;
        let optimal = (composition.total_tokens as f64 / ideal_usage_pct) as usize;
        
        // Round up to nice numbers
        let rounded = match optimal {
            x if x < 8_000 => 8_000,
            x if x < 16_000 => 16_000,
            x if x < 32_000 => 32_000,
            x if x < 64_000 => 64_000,
            x if x < 128_000 => 128_000,
            _ => optimal,
        };

        // Ensure it's at least the current window
        rounded.max(self.memory.context_window())
    }

    /// Compress context using specified strategy
    pub(super) fn compress_context(&mut self, strategy: CompressionStrategy) -> CompressionResult {
        let before_tokens = self.estimate_messages_tokens();
        let before_messages = self.messages.len();
        let target_pct = match strategy {
            CompressionStrategy::Aggressive => 0.60,
            CompressionStrategy::Moderate => 0.75,
            CompressionStrategy::Conservative => 0.85,
        };

        let target_tokens = (self.max_context_tokens as f64 * target_pct) as usize;
        
        println!(
            "\n{} Compressing context to {:.0}% of budget ({:.0}k → {} tokens)...",
            "🗜️".bright_cyan(),
            target_pct * 100.0,
            before_tokens as f64 / 1000.0,
            target_tokens
        );

        // Perform the actual trimming
        self.trim_message_history();

        let after_tokens = self.estimate_messages_tokens();
        let after_messages = self.messages.len();
        let removed_messages = before_messages.saturating_sub(after_messages);
        let saved_tokens = before_tokens.saturating_sub(after_tokens);

        println!(
            "{} Removed {} messages, saved {:.0}k tokens (now at {:.1}% of budget)",
            if removed_messages > 0 { "✓".bright_green() } else { "⚠️".bright_yellow() },
            removed_messages,
            saved_tokens as f64 / 1000.0,
            self.context_usage_pct()
        );

        // Log the compression event
        self.log_context_compression_event(super::session_log::ContextCompressionLogDetails {
            strategy: format!("{:?}", strategy).as_str(),
            before_tokens,
            after_tokens,
            threshold: 0,
            before_messages,
            after_messages,
            success: removed_messages > 0,
            error: None,
        });

        CompressionResult {
            success: removed_messages > 0,
            before_tokens,
            after_tokens,
            saved_tokens,
            removed_messages,
        }
    }

    /// Get a recommendation for context management based on current state
    pub(super) fn get_context_recommendation(&self) -> Option<ContextRecommendation> {
        let composition = self.analyze_context_composition();
        
        if composition.usage_pct < 50.0 {
            return None; // No action needed
        }

        if composition.usage_pct > 90.0 {
            Some(ContextRecommendation {
                priority: Priority::Critical,
                action: "compress",
                message: "Context is critically full - immediate compression recommended".to_string(),
                command: "/compress moderate",
            })
        } else if composition.context_file_tokens > composition.tool_tokens * 3 && composition.user_tokens > 0 {
            Some(ContextRecommendation {
                priority: Priority::Medium,
                action: "refresh",
                message: "Context files dominate your context - consider reloading only needed files".to_string(),
                command: "/ctx clear && /ctx load <specific-files>",
            })
        } else if composition.message_count > 150 {
            Some(ContextRecommendation {
                priority: Priority::Medium,
                action: "compress",
                message: "High message count detected - compression can reduce overhead".to_string(),
                command: "/compress conservative",
            })
        } else if composition.usage_pct > 75.0 {
            Some(ContextRecommendation {
                priority: Priority::Low,
                action: "monitor",
                message: "Context usage elevated but manageable".to_string(),
                command: "/ctx stats",
            })
        } else {
            None
        }
    }
}

/// Result of a compression operation
#[derive(Debug, Clone)]
pub struct CompressionResult {
    #[allow(dead_code)]
    pub success: bool,
    #[allow(dead_code)]
    pub before_tokens: usize,
    #[allow(dead_code)]
    pub after_tokens: usize,
    #[allow(dead_code)]
    pub saved_tokens: usize,
    #[allow(dead_code)]
    pub removed_messages: usize,
}

/// Analysis of context token distribution
#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct ContextComposition {
    pub total_tokens: usize,
    pub system_tokens: usize,
    pub user_tokens: usize,
    pub assistant_tokens: usize,
    pub tool_tokens: usize,
    pub context_file_tokens: usize,
    pub message_count: usize,
    pub window: usize,
    pub usage_pct: f64,
}

/// Context management recommendation
#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct ContextRecommendation {
    pub priority: Priority,
    pub action: &'static str,
    pub message: String,
    pub command: &'static str,
}

#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Priority {
    Low,
    Medium,
    High,
    Critical,
}
