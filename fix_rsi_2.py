import re

with open('src/cognitive/rsi_orchestrator.rs', 'r') as f:
    content = f.read()

# Fix the analyze_self unwrap
content = content.replace(
    'let targets = self.edit_orchestrator.analyze_self().unwrap_or_default();',
    'let targets = self.edit_orchestrator.analyze_self().unwrap_or_else(|_| vec![]);'
)

# Fix the PerformanceSnapshot struct fields
content = content.replace(
    'token_efficiency: 0.0',
    'first_try_verification_rate: 0.0, avg_tokens: 0.0, test_pass_rate: 0.0, compilation_errors_per_task: 0.0, label: None'
)

with open('src/cognitive/rsi_orchestrator.rs', 'w') as f:
    f.write(content)
