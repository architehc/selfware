import os
import re

with open('src/agent/mod.rs', 'r') as f:
    content = f.read()

content = content.replace('fn reflect_on_step(&mut self, step: usize) {', 'async fn reflect_on_step(&mut self, step: usize) {')

content = content.replace('self.reflect_on_step(1);', 'self.reflect_on_step(1).await;')
content = content.replace('self.reflect_on_step(step + 1);', 'self.reflect_on_step(step + 1).await;')

with open('src/agent/mod.rs', 'w') as f:
    f.write(content)
