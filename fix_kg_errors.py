import re

with open('src/cognitive/knowledge_graph.rs', 'r') as f:
    content = f.read()

# Fix RustEntityExtractor
content = content.replace('#[derive(Debug, Clone, Serialize, Deserialize)]\npub struct RustEntityExtractor', 'pub struct RustEntityExtractor')
content = content.replace('#[derive(Debug, Clone, Serialize, Deserialize)]\npub struct SmellDetector', 'pub struct SmellDetector')
content = content.replace('#[derive(Debug, Clone, Serialize, Deserialize)]\npub struct SemanticLinker', 'pub struct SemanticLinker')
content = content.replace('#[derive(Debug, Clone, Serialize, Deserialize)]\npub struct PatternRecognizer', 'pub struct PatternRecognizer')

# Fix CodeSmell borrow
content = content.replace('description: format!("{} detected", smell),', 'description: format!("{} detected", smell.clone()),')

# Fix EntityType partial move
content = content.replace('.entry(entity.entity_type)', '.entry(entity.entity_type.clone())')

# Fix RelationType moved
content = content.replace('relation_type.is_none_or', 'relation_type.clone().is_none_or')

# Fix *t move in EntityType mapping
content = content.replace('.map(|(t, ids)| (*t, ids.len()))', '.map(|(t, ids)| (t.clone(), ids.len()))')

# Fix visibility moves
content = content.replace('self.extract_function(trimmed, file_path, line_num + 1, visibility)', 'self.extract_function(trimmed, file_path, line_num + 1, visibility.clone())')
content = content.replace('self.extract_struct(trimmed, file_path, line_num + 1, visibility)', 'self.extract_struct(trimmed, file_path, line_num + 1, visibility.clone())')
content = content.replace('self.extract_enum(trimmed, file_path, line_num + 1, visibility)', 'self.extract_enum(trimmed, file_path, line_num + 1, visibility.clone())')
content = content.replace('self.extract_trait(trimmed, file_path, line_num + 1, visibility)', 'self.extract_trait(trimmed, file_path, line_num + 1, visibility.clone())')

# Fix pattern_type move
content = content.replace('results.push((name.clone(), *pattern_type, confidence));', 'results.push((name.clone(), pattern_type.clone(), confidence));')


with open('src/cognitive/knowledge_graph.rs', 'w') as f:
    f.write(content)

