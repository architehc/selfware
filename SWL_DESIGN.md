# Selfware Workflow Language (SWL) Design
## Declarative-Imperative Hybrid with Real-Time Observability

### Core Philosophy

SWL treats agent orchestration as **infrastructure-as-code** with **first-class observability**. Every construct emits telemetry automatically.

```yaml
# example.swl - Full spectrum example
version: "2.0"
name: code_review_pipeline

# ═══════════════════════════════════════════════════════════
# DECLARATIVE LAYER: What you want to achieve
# ═══════════════════════════════════════════════════════════

agents:
  architect:
    model: txn545/Qwen3.5-122B-A10B-NVFP4
    role: Analyze architecture and identify coupling
    instruction: |
      Review the codebase structure. Identify:
      1. Module boundaries
      2. Circular dependencies  
      3. Violations of SOLID principles
    
  security_auditor:
    model: claude-sonnet-4-6
    role: Security-focused code review
    instruction: |
      Scan for security issues:
      - Path traversal vulnerabilities
      - Unsafe command execution
      - Secret exposure

  performance_analyst:
    model: gemini-2.5-flash
    role: Performance optimization
    instruction: |
      Identify bottlenecks:
      - Unnecessary allocations
      - Blocking operations in async contexts
      - Algorithmic inefficiencies

# ═══════════════════════════════════════════════════════════
# IMPERATIVE LAYER: How to achieve it (embedded Rust/Python)
# ═══════════════════════════════════════════════════════════

workflows:
  parallel_review:
    type: map-reduce
    
    # Declarative: Distribute to all agents
    map:
      targets: [architect, security_auditor, performance_analyst]
      input: ${file://src/*.rs}
      
    # Imperative: Custom merge logic
    reduce:
      language: rust
      code: |
        // Full Rust code with access to all agent outputs
        fn merge(results: Vec<AgentOutput>) -> Report {
            let mut report = Report::new();
            
            // Deduplicate findings across agents
            let mut seen = HashSet::new();
            for output in results {
                for finding in output.findings {
                    let key = hash!(&finding.location, &finding.issue);
                    if seen.insert(key) {
                        report.add(finding);
                    }
                }
            }
            
            // Severity scoring algorithm
            report.score = calculate_severity(&report.findings);
            report
        }

# ═══════════════════════════════════════════════════════════
# OBSERVABILITY LAYER: Always-on telemetry
# ═══════════════════════════════════════════════════════════

telemetry:
  metrics:
    - name: inference_latency
      type: histogram
      labels: [agent, model, tokens_in, tokens_out]
      
    - name: context_window_usage
      type: gauge
      labels: [agent, percentage]
      
    - name: tool_execution_time
      type: histogram
      labels: [tool_name, status]
      
    - name: cost_per_request
      type: counter
      labels: [agent, model]
      
  traces:
    - workflow_execution
    - agent_delegation
    - tool_invocation
    
  logs:
    level: debug
    structured: true
    output: [stdout, file:///tmp/swl.log]

# ═══════════════════════════════════════════════════════════
# COMMAND CENTER: Real-time visualization config
# ═══════════════════════════════════════════════════════════

dashboard:
  layout: grid
  refresh: 100ms  # Real-time updates
  
  panels:
    - title: "Inference Speed"
      type: timeseries
      metric: inference_latency
      aggregation: p99
      color: heatmap
      
    - title: "Context Window"
      type: gauge
      metric: context_window_usage
      warning: 80%
      critical: 95%
      
    - title: "Agent Activity"
      type: topology_graph
      nodes: agents
      edges: delegations
      animate: true
      
    - title: "Live Logs"
      type: log_stream
      filter: level >= info
      tail: 100
      
    - title: "Cost Tracker"
      type: counter
      metric: cost_per_request
      currency: USD
      budget_alert: $10.00

# ═══════════════════════════════════════════════════════════
# SAFETY LAYER: Guardrails as code
# ═══════════════════════════════════════════════════════════

guardrails:
  - name: rate_limiter
    type: before_model
    condition: |
      rate(requests_per_minute) < 60
    on_violation: queue_and_retry
    
  - name: cost_cap
    type: global
    condition: |
      total_cost < budget.limit
    on_violation: pause_and_notify
    
  - name: safety_checker
    type: before_tool
    condition: |
      tool.name == "shell_exec" && 
      !args.command.contains("rm -rf")
    on_violation: block_and_log

# ═══════════════════════════════════════════════════════════
# STATE MANAGEMENT: Session persistence
# ═══════════════════════════════════════════════════════════

state:
  backend: sqlite  # or redis, postgresql
  ttl: 24h
  
  schemas:
    conversation:
      messages: Vec<Message>
      context_tokens: usize
      
    user_preferences:
      temp_unit: enum[Celsius, Fahrenheit]
      response_style: enum[concise, detailed]

# ═══════════════════════════════════════════════════════════
# EXTENSIBILITY: Plugin system
# ═══════════════════════════════════════════════════════════

plugins:
  - name: git_integration
    source: https://github.com/selfware/git-plugin
    config:
      auto_commit: true
      branch_prefix: "swl/"
      
  - name: slack_notifier
    webhook: ${env.SLACK_WEBHOOK}
    events: [workflow_complete, guardrail_violation]
