# Production Action Plan

**Priority-ordered task list for making Selfware production-ready**

---

## 🔴 P0 - Critical (Fix Immediately)

### Security
- [ ] **SSRF-001**: Fix DNS rebinding TOCTOU in `src/safety/checker.rs:375-434`
  - Implement DNS pinning resolver
  - Estimated: 1 day
  
- [ ] **PATH-001**: Fix path validation race condition in `src/safety/path_validator.rs:119-146`
  - Use O_NOFOLLOW directly without exists() check
  - Estimated: 1 day
  
- [ ] **INJ-001**: Fix shell command injection in `src/tools/shell.rs:46-161`
  - Add comprehensive shell metacharacter validation
  - Estimated: 2 days
  
- [ ] **INJ-002**: Fix container command injection in `src/tools/container.rs:671-734`
  - Validate all command arguments
  - Estimated: 1 day
  
- [ ] **PATH-002**: Fix Git commit path traversal in `src/tools/git.rs:303-314`
  - Validate file paths before git add
  - Estimated: 0.5 day
  
- [ ] **PATH-003**: Fix KnowledgeExport path traversal in `src/tools/knowledge.rs:768`
  - Add path validation
  - Estimated: 0.5 day
  
- [ ] **INJ-003**: Add ProcessStart command validation in `src/tools/process.rs:94-170`
  - Implement command allowlist
  - Estimated: 1 day

### Async/Blocking
- [ ] **ASYNC-001**: Replace blocking file I/O in `src/agent/mod.rs:1235,1380,1418`
  - Use `tokio::fs` instead of `std::fs`
  - Estimated: 1 day
  
- [ ] **ASYNC-002**: Fix blocking clipboard in `src/agent/interactive.rs:1194-1238`
  - Use `tokio::process::Command`
  - Estimated: 0.5 day
  
- [ ] **ASYNC-003**: Fix blocking shell execution in `src/agent/interactive.rs:166-171`
  - Use async process spawn
  - Estimated: 0.5 day
  
- [ ] **ASYNC-004**: Fix blocking retry sleep in `src/self_healing/executor.rs:324-424`
  - Use `tokio::time::sleep().await`
  - Estimated: 0.5 day

### Process Management
- [ ] **PROC-001**: Fix orphaned processes on timeout in `src/tools/shell.rs:138-150`
  - Kill child process on timeout
  - Estimated: 0.5 day
  
- [ ] **PROC-002**: Fix PID reuse vulnerability in `src/devops/process_manager.rs:400-410`
  - Verify PID before sending signal
  - Estimated: 1 day

### Data Integrity
- [ ] **DATA-001**: Fix checkpoint non-atomic save in `src/agent/checkpointing.rs:106-130`
  - Use temp file + atomic rename
  - Estimated: 1 day
  
- [ ] **DATA-002**: Fix episodic memory non-atomic save in `src/cognitive/episodic.rs:856-881`
  - Write to temp directory then rename
  - Estimated: 1 day

---

## 🟠 P1 - High Priority (Fix Before Beta)

### Security
- [ ] **SEC-001**: Add PII redaction to all learning/memory persistence
  - Apply redaction before storage in cognitive modules
  - Estimated: 3 days
  
- [ ] **SEC-002**: Add secret zeroization for RedactedString
  - Use `zeroize` crate
  - Estimated: 1 day
  
- [ ] **SEC-003**: Add HTTP redirect validation for SSRF protection
  - Validate each redirect target
  - Estimated: 1 day
  
- [ ] **SEC-004**: Fix YOLO mode bypass vulnerability
  - Use normalized command checking
  - Estimated: 0.5 day
  
- [ ] **SEC-005**: Add container security defaults
  - read-only rootfs, no-new-privileges, drop-all-caps
  - Estimated: 1 day

### Testing Infrastructure
- [ ] **TEST-001**: Build mock LLM server
  - Implement MockLlmServer for testing
  - Estimated: 3 days
  
- [ ] **TEST-002**: Add API client unit tests
  - Use mock server for testing
  - Estimated: 2 days
  
- [ ] **TEST-003**: Add SSRF/security test suite
  - Test all bypass vectors
  - Estimated: 2 days
  
- [ ] **TEST-004**: Add memory/RAG unit tests
  - Test vector operations and search
  - Estimated: 2 days
  
- [ ] **TEST-005**: Convert simulation tests to real tests
  - Fix extended_e2e.rs
  - Estimated: 2 days

### Performance
- [ ] **PERF-001**: Fix O(n²) message trimming in `src/agent/mod.rs:817-835`
  - Use two-pointer approach
  - Estimated: 1 day
  
- [ ] **PERF-002**: Implement HNSW indexing for vector search
  - Replace brute-force search
  - Estimated: 5 days
  
- [ ] **PERF-003**: Add connection pooling to API client
  - Configure reqwest pool settings
  - Estimated: 0.5 day

### Reliability
- [ ] **REL-001**: Add integrity verification to all JSON persistence
  - Add SHA-256 checksums
  - Estimated: 2 days
  
- [ ] **REL-002**: Add corruption recovery to cognitive modules
  - Implement backup/fallback pattern
  - Estimated: 2 days
  
- [ ] **REL-003**: Add CargoTest timeout
  - Prevent indefinite test runs
  - Estimated: 0.5 day

---

## 🟡 P2 - Medium Priority (Fix Before GA)

### Code Quality
- [ ] **QUAL-001**: Make Config fields private with validated setters
  - Prevent validation bypass
  - Estimated: 2 days
  
- [ ] **QUAL-002**: Complete feature flag implementation
  - Add cfg gates for all features
  - Estimated: 2 days
  
- [ ] **QUAL-003**: Add comprehensive error context
  - Use `with_context()` throughout
  - Estimated: 2 days
  
- [ ] **QUAL-004**: Standardize on structured logging
  - Use tracing fields consistently
  - Estimated: 2 days

### Testing
- [ ] **TEST-006**: Add property-based tests for critical paths
  - Token estimation, path validation, encryption
  - Estimated: 3 days
  
- [ ] **TEST-007**: Add fuzz testing targets
  - Tool parser, input validation
  - Estimated: 2 days
  
- [ ] **TEST-008**: Achieve >80% code coverage
  - Fill gaps in testing
  - Estimated: 5 days

### Observability
- [ ] **OBS-001**: Add structured logging throughout
  - Consistent event logging
  - Estimated: 3 days
  
- [ ] **OBS-002**: Implement metrics collection
  - Performance counters
  - Estimated: 2 days
  
- [ ] **OBS-003**: Add access control to dashboards
  - Authentication layer
  - Estimated: 2 days

---

## 🟢 P3 - Low Priority (Nice to Have)

- [ ] Add Windows ACL checks for config permissions
- [ ] Implement log rotation for telemetry
- [ ] Add metrics cardinality limits
- [ ] Improve token counting heuristics
- [ ] Add comprehensive documentation
- [ ] Create architecture diagrams
- [ ] Add chaos testing framework
- [ ] Implement load testing suite

---

## 📅 Suggested Timeline

### Week 1: Critical Security Fixes
- SSRF, path validation, command injection fixes
- Async blocking fixes

### Week 2: Critical Reliability
- Process management fixes
- Atomic file operations
- Data integrity improvements

### Week 3: Testing Infrastructure
- Mock LLM server
- Security test suite
- API client tests

### Week 4: Performance & Polish
- O(n²) fixes
- Connection pooling
- Code quality improvements

### Week 5: Observability & Documentation
- Structured logging
- Metrics collection
- Documentation

### Week 6: Final Testing & Hardening
- Coverage improvements
- Load testing
- Security audit

---

## ✅ Definition of Done

A task is complete when:
1. Code is implemented and reviewed
2. Tests are added/updated
3. Documentation is updated
4. No new clippy warnings
5. CI passes

---

**Total Estimated Effort: 6-8 weeks for 2-3 developers**
