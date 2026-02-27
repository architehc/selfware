import re

with open('src/cognitive/knowledge_graph.rs', 'r') as f:
    content = f.read()

# Add serde imports if missing
if 'use serde::{Deserialize, Serialize};' not in content:
    content = content.replace('use std::collections::', 'use serde::{Deserialize, Serialize};\nuse std::collections::')

# Add derivations to data structures
for struct_name in ['Entity', 'Relation', 'Pattern', 'PatternExample', 'CodeSmellInstance', 'KnowledgeGraph']:
    # Replace #[derive(Debug, Clone)] with #[derive(Debug, Clone, Serialize, Deserialize)]
    # Replace #[derive(Debug)] with #[derive(Debug, Clone, Serialize, Deserialize)]
    pattern1 = r'#\[derive\([^\]]*\)\]\s*pub struct ' + struct_name
    def repl1(m):
        return '#[derive(Debug, Clone, Serialize, Deserialize)]\npub struct ' + struct_name
    content = re.sub(pattern1, repl1, content)

for enum_name in ['EntityType', 'Visibility', 'RelationType', 'PatternType', 'CodeSmell']:
    pattern2 = r'#\[derive\([^\]]*\)\]\s*pub enum ' + enum_name
    def repl2(m):
        s = m.group(0)
        if 'Serialize' not in s:
            return '#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]\npub enum ' + enum_name
        return s
    content = re.sub(pattern2, repl2, content)

# Also fix KnowledgeGraph if it had just #[derive(Debug)]
content = content.replace('#[derive(Debug)]\npub struct KnowledgeGraph', '#[derive(Debug, Clone, Serialize, Deserialize)]\npub struct KnowledgeGraph')

# Add save/load methods to KnowledgeGraph
save_load_methods = """
    /// Save the knowledge graph to a JSON file
    pub fn save_to_file(&self, path: &std::path::Path) -> anyhow::Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let json = serde_json::to_string_pretty(self)?;
        std::fs::write(path, json)?;
        Ok(())
    }

    /// Load the knowledge graph from a JSON file
    pub fn load_from_file(path: &std::path::Path) -> anyhow::Result<Self> {
        if path.exists() {
            let json = std::fs::read_to_string(path)?;
            let graph = serde_json::from_str(&json)?;
            Ok(graph)
        } else {
            Ok(Self::new())
        }
    }
"""

content = content.replace('pub fn new() -> Self {', save_load_methods + '\n    pub fn new() -> Self {')

with open('src/cognitive/knowledge_graph.rs', 'w') as f:
    f.write(content)
