import re

with open('src/cognitive/knowledge_graph.rs', 'r') as f:
    content = f.read()

content = content.replace('smell,\n            entity_id: entity_id.into(),\n            file,\n            line,\n            description: format!("{} detected", smell.clone()),', 'smell: smell.clone(),\n            entity_id: entity_id.into(),\n            file,\n            line,\n            description: format!("{} detected", smell),')

with open('src/cognitive/knowledge_graph.rs', 'w') as f:
    f.write(content)
