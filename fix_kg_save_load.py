import re

with open('src/cognitive/knowledge_graph.rs', 'r') as f:
    content = f.read()

# The incorrect insertion:
bad_block = """    /// Save the knowledge graph to a JSON file
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

# Remove ALL occurrences
content = content.replace(bad_block, '')

# Add it ONLY to KnowledgeGraph
kg_impl = "impl KnowledgeGraph {"
content = content.replace(kg_impl, kg_impl + '\n' + bad_block)

# Fix borrow of smell CodeSmell
content = content.replace('smell,\n            description: format!("{} detected", smell.clone()),', 'smell: smell.clone(),\n            description: format!("{} detected", smell),')

with open('src/cognitive/knowledge_graph.rs', 'w') as f:
    f.write(content)
