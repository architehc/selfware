# Quick Fixes - Selfware Project

**This document contains the most critical fixes needed immediately.**

---

## 🔴 CRITICAL: Fix These First

### 1. Replace Blocking I/O with Async Versions

**File:** `src/agent/execution.rs` ~line 476

**Current (BAD):**
```rust
io::stdout().flush().ok();
let mut response = String::new();
io::stdin().read_line(&mut response).is_ok()
```

**Fix:**
```rust
use tokio::io::{self, AsyncBufReadExt, AsyncWriteExt};

io::stdout().flush().await.ok();
let mut response = String::new();
io::stdin().read_line(&mut response).await.is_ok()
```

---

**File:** `src/agent/checkpointing.rs` ~line 306

**Current (BAD):**
```rust
std::fs::create_dir_all(parent)?;
let content = serde_json::to_string_pretty(&self.cognitive_state.episodic_memory)?;
std::fs::write(&global_memory_path, content)?;
```

**Fix:**
```rust
tokio::fs::create_dir_all(parent).await?;
let content = serde_json::to_string_pretty(&self.cognitive_state.episodic_memory)?;
tokio::fs::write(&global_memory_path, content).await?;
```

---

### 2. Fix Test Mode Security Bypass

**File:** `src/tools/file.rs` ~line 485

**Current (BAD):**
```rust
#[cfg(test)]
{
    if std::env::var("SELFWARE_TEST_MODE").is_ok() {
        return Ok(());  // Complete bypass!
    }
}
```

**Fix:**
```rust
#[cfg(test)]
{
    if std::env::var("SELFWARE_TEST_MODE").is_ok() {
        // Only allow test fixture paths
        if !path.starts_with("tests/e2e-projects/") && !path.starts_with("/tmp/selfware-test-") {
            anyhow::bail!("Test mode only valid for test fixtures, got: {}", path.display());
        }
        return Ok(());
    }
}
```

---

### 3. Fix FIM Instruction Injection

**File:** `src/tools/fim.rs` ~line 94

**Current (BAD):**
```rust
let prompt = format!(
    "<|fim_prefix|>{}
// Instruction: {}
<|fim_suffix|>{}
<|fim_middle|>",
    prefix, instruction, suffix
);
```

**Fix:**
```rust
// Sanitize instruction to prevent prompt injection
fn sanitize_fim_instruction(s: &str) -> String {
    s.replace("<|", "")
     .replace("|>", "")
     .replace("// Instruction:", "")
     .trim()
     .to_string()
}

let sanitized = sanitize_fim_instruction(&args.instruction);
let prompt = format!(
    "<|fim_prefix|>{}
<|fim_suffix|>{}
<|fim_middle|>",
    prefix, suffix
);
// Pass instruction via separate field if API supports it
```

---

## 🟡 HIGH: Fix These Next

### 4. Add Validation for Critical Config Fields

**File:** `src/config/mod.rs` in `validate()` method

**Add these validations:**
```rust
if self.agent.max_recovery_attempts > 10 {
    bail!("agent.max_recovery_attempts must be <= 10");
}

if self.continuous_work.checkpoint_interval_tools < 1 {
    bail!("checkpoint_interval_tools must be >= 1");
}

for pattern in &self.safety.allowed_paths {
    if glob::Pattern::new(pattern).is_err() {
        bail!("Invalid glob pattern in allowed_paths: {}", pattern);
    }
}
```

---

### 5. Fix Token Cache Contention

**File:** `src/token_count.rs` ~line 82

**Current:**
```rust
static TOKEN_CACHE: RwLock<LruCache<...>> = ...;
```

**Fix:**
```rust
// Use dashmap for lock-free concurrent access
use dashmap::DashMap;

static TOKEN_CACHE: DashMap<String, usize> = DashMap::new();
// Or add metrics to track contention
```

---

### 6. Add API Task Spawning Limits

**File:** `src/api/mod.rs` ~line 63

**Current:**
```rust
pub async fn into_channel(self) -> mpsc::Receiver<Result<StreamChunk>> {
    let (tx, rx) = mpsc::channel(32);
    tokio::spawn(async move { ... });  // Unbounded!
    rx
}
```

**Fix:**
```rust
use tokio::sync::Semaphore;

static STREAM_SEMAPHORE: Lazy<Semaphore> = Lazy::new(|| Semaphore::new(100));

pub async fn into_channel(self) -> Result<mpsc::Receiver<Result<StreamChunk>>> {
    let permit = STREAM_SEMAPHORE.acquire().await?;
    let (tx, rx) = mpsc::channel(32);
    tokio::spawn(async move {
        let _permit = permit; // Hold permit until task completes
        // ... rest of implementation
    });
    Ok(rx)
}
```

---

## 🟢 MEDIUM: Fix When Convenient

### 7. Remove Dead Code

**File:** `src/config/typed.rs`

This entire file (1,168 lines) is unused. Either:
- Option A: Remove it entirely
- Option B: Integrate it to replace `mod.rs`

If removing:
```bash
rm src/config/typed.rs
# Remove from src/config/mod.rs module declarations
```

---

### 8. Fix Error Detection

**File:** `src/errors.rs` ~line 208

**Current (BAD):**
```rust
let msg = e.to_string().to_lowercase();
if msg.contains("config") {
    return EXIT_CONFIG_ERROR;
}
```

**Fix:**
```rust
// Add specific error types
#[derive(Error, Debug)]
pub enum AppError {
    #[error("Configuration error: {0}")]
    Config(String),
    #[error("API error: {0}")]
    Api(String),
    // ...
}

// Then match on type
match e.downcast_ref::<AppError>() {
    Some(AppError::Config(_)) => EXIT_CONFIG_ERROR,
    Some(AppError::Api(_)) => EXIT_API_ERROR,
    // ...
}
```

---

### 9. Fix Hardcoded Fitness Values

**File:** `src/evolution/daemon.rs` ~line 863

**Current:**
```rust
token_budget: 500_000.0,  // Hardcoded
coverage_percent: 82.0,   // Hardcoded
binary_size_mb: 15.0,     // Hardcoded
```

**Fix:** Implement actual measurement:
```rust
// Add measurement functions
token_budget: measure_token_usage().await?,
coverage_percent: run_coverage_check().await?,
binary_size_mb: measure_binary_size().await?,
```

---

## 📋 Checklist

- [ ] Fix blocking I/O in `execution.rs`
- [ ] Fix blocking I/O in `checkpointing.rs`
- [ ] Fix test mode bypass in `file.rs`
- [ ] Fix FIM injection in `fim.rs`
- [ ] Add config validation
- [ ] Add API spawning limits
- [ ] Fix token cache contention
- [ ] Remove or integrate `typed.rs`
- [ ] Fix error type detection
- [ ] Implement actual fitness measurement

---

**Estimated Time:** 2-3 days for all critical and high priority fixes.
