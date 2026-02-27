import re
import os

# Fix memory_hierarchy.rs
with open('src/cognitive/memory_hierarchy.rs', 'r') as f:
    mh = f.read()

# Fix vector_index creation
mh = mh.replace('VectorIndex::new(embedding.clone())', 'VectorIndex::new(1536)')

# Fix vector_index.add
mh = re.sub(
    r'episode\.embedding_id = self\.vector_index\.add\(embedding_vec, episode\.id\.clone\(\)\)\.await\?;',
    r'self.vector_index.add(episode.id.clone(), embedding_vec)?;\n        episode.embedding_id = episode.id.clone();',
    mh
)

# Fix vector_index.remove awaits
mh = mh.replace('self.vector_index.remove(&episode.embedding_id).await?', 'self.vector_index.remove(&episode.embedding_id)?')

# Fix vector_index.search awaits
mh = mh.replace('self.vector_index.search(&query_embedding, limit * 2).await?', 'self.vector_index.search(&query_embedding, limit * 2)?')

# Fix CodeContext derive
mh = mh.replace('pub struct CodeContext', '#[derive(Debug, Clone)]\npub struct CodeContext')

# Fix join
mh = mh.replace('chunks.iter().map(|c| &c.content).collect::<Vec<_>>().join("\\n")', 'chunks.iter().map(|c| c.content.as_str()).collect::<Vec<_>>().join("\\n")')

# Add get_file to SemanticMemory
get_file_code = """
    pub fn get_file(&self, path: &str) -> Option<&IndexedFile> {
        self.files.get(path)
    }
"""
mh = mh.replace('pub fn get_recent_files(&self, limit: usize) -> Vec<FileContextEntry> {', get_file_code + '\n    pub fn get_recent_files(&self, limit: usize) -> Vec<FileContextEntry> {')

# Fix recursive async index_directory
mh = mh.replace('async fn index_directory(&mut self, dir: &std::path::Path) -> Result<()> {', 'fn index_directory(\'a, \'b, self_: &\'a mut Self, dir: &\'b std::path::Path) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<()>> + Send + \'a>> where \'b: \'a {\n        Box::pin(async move {')
mh = mh.replace('self.index_directory(&path).await?;', 'Self::index_directory(self_, &path).await?;')
# we need to rename self to self_ inside index_directory, which is tricky. Let's just use the async_recursion crate or box it cleanly.
# Alternatively, rewrite index_directory to be non-recursive using a queue/stack.
