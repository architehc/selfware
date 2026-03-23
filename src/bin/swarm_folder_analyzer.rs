//! Swarm Folder Analyzer
//!
//! Spawns a swarm of specialized agents to analyze subfolders and create
//! a recommended list of interesting/documented/important folders.

use anyhow::{Context, Result};
use std::path::{Path, PathBuf};
use std::time::Instant;
use tracing::{info, warn};

use selfware::swarm::{
    create_dev_swarm, Agent, AgentRole, SharedMemory, Swarm, SwarmTask,
};

/// Analyze a folder and return metadata
struct FolderAnalysis {
    path: PathBuf,
    name: String,
    file_count: usize,
    total_size: u64,
    has_readme: bool,
    has_code: bool,
    has_tests: bool,
    has_docs: bool,
    depth: usize,
    description: String,
    tags: Vec<String>,
    priority_score: f32,
}

impl FolderAnalysis {
    fn new(path: &Path, depth: usize) -> Self {
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| path.display().to_string());

        Self {
            path: path.to_path_buf(),
            name,
            file_count: 0,
            total_size: 0,
            has_readme: false,
            has_code: false,
            has_tests: false,
            has_docs: false,
            depth,
            description: String::new(),
            tags: Vec::new(),
            priority_score: 0.0,
        }
    }

    fn calculate_score(&mut self) {
        // Scoring algorithm based on folder characteristics
        let mut score = 0.0;

        // Base score from file count (logarithmic)
        if self.file_count > 0 {
            score += (self.file_count as f32).ln() * 5.0;
        }

        // Has README is important
        if self.has_readme {
            score += 15.0;
        }

        // Has code
        if self.has_code {
            score += 10.0;
        }

        // Has tests is good
        if self.has_tests {
            score += 8.0;
        }

        // Has documentation
        if self.has_docs {
            score += 12.0;
        }

        // Depth penalty (prefer top-level folders)
        score -= self.depth as f32 * 2.0;

        // Size bonus (but not too much)
        if self.total_size > 0 {
            score += (self.total_size as f32).ln() * 0.5;
        }

        self.priority_score = score.max(0.0);
    }
}

/// Folder scanner that collects metadata
struct FolderScanner {
    root: PathBuf,
    max_depth: usize,
}

impl FolderScanner {
    fn new(root: impl Into<PathBuf>, max_depth: usize) -> Self {
        Self {
            root: root.into(),
            max_depth,
        }
    }

    fn scan(&self) -> Result<Vec<FolderAnalysis>> {
        let mut folders = Vec::new();
        self.scan_directory(&self.root, 0, &mut folders)?;
        Ok(folders)
    }

    fn scan_directory(
        &self,
        dir: &Path,
        depth: usize,
        folders: &mut Vec<FolderAnalysis>,
    ) -> Result<()> {
        if depth > self.max_depth {
            return Ok(());
        }

        // Skip hidden directories and common non-interesting folders
        let dir_name = dir
            .file_name()
            .map(|n| n.to_string_lossy())
            .unwrap_or_default();

        if dir_name.starts_with('.')
            || dir_name == "target"
            || dir_name == "node_modules"
            || dir_name == ".git"
            || dir_name == ".next"
            || dir_name == "dist"
            || dir_name == "build"
        {
            return Ok(());
        }

        let mut analysis = FolderAnalysis::new(dir, depth);

        // Walk the directory and collect stats
        if let Ok(entries) = std::fs::read_dir(dir) {
            for entry in entries.flatten() {
                let entry_path = entry.path();
                if entry_path.is_file() {
                    analysis.file_count += 1;

                    // Check file type
                    let file_name = entry_path
                        .file_name()
                        .map(|n| n.to_string_lossy().to_string())
                        .unwrap_or_default();

                    if file_name.to_lowercase().contains("readme") {
                        analysis.has_readme = true;
                    }

                    if file_name.ends_with(".rs")
                        || file_name.ends_with(".py")
                        || file_name.ends_with(".js")
                        || file_name.ends_with(".ts")
                        || file_name.ends_with(".go")
                        || file_name.ends_with(".java")
                        || file_name.ends_with(".cpp")
                        || file_name.ends_with(".c")
                    {
                        analysis.has_code = true;
                    }

                    if file_name.ends_with("_test.rs")
                        || file_name.ends_with(".test.ts")
                        || file_name.ends_with(".spec.ts")
                        || file_name.contains("test")
                    {
                        analysis.has_tests = true;
                    }

                    if file_name.ends_with(".md")
                        || file_name.ends_with(".txt")
                        || file_name.ends_with(".rst")
                    {
                        analysis.has_docs = true;
                    }

                    // Add to total size
                    if let Ok(metadata) = entry_path.metadata() {
                        analysis.total_size += metadata.len();
                    }
                } else if entry_path.is_dir() {
                    // Recursively scan subdirectories
                    self.scan_directory(&entry_path, depth + 1, folders)?;
                }
            }
        }

        analysis.calculate_score();

        // Generate description
        analysis.description = self.generate_description(&analysis);

        // Add tags
        analysis.tags = self.generate_tags(&analysis);

        folders.push(analysis);

        // Recursively scan subdirectories
        if let Ok(entries) = std::fs::read_dir(dir) {
            for entry in entries.flatten() {
                if entry.path().is_dir() {
                    self.scan_directory(&entry.path(), depth + 1, folders)?;
                }
            }
        }

        Ok(())
    }

    fn generate_description(&self, analysis: &FolderAnalysis) -> String {
        let mut desc = Vec::new();

        if analysis.has_readme {
            desc.push("documented");
        }
        if analysis.has_code {
            desc.push("code");
        }
        if analysis.has_tests {
            desc.push("tested");
        }
        if analysis.has_docs {
            desc.push("documentation");
        }

        let file_desc = if analysis.file_count > 100 {
            "large"
        } else if analysis.file_count > 20 {
            "medium"
        } else {
            "small"
        };

        format!(
            "{} {} folder with {} files",
            desc.join(", "),
            file_desc,
            analysis.file_count
        )
    }

    fn generate_tags(&self, analysis: &FolderAnalysis) -> Vec<String> {
        let mut tags = Vec::new();

        if analysis.has_readme {
            tags.push("documented");
        }
        if analysis.has_code {
            tags.push("source");
        }
        if analysis.has_tests {
            tags.push("tests");
        }
        if analysis.has_docs {
            tags.push("docs");
        }

        if analysis.priority_score > 30.0 {
            tags.push("high-priority");
        } else if analysis.priority_score > 15.0 {
            tags.push("medium-priority");
        }

        tags
    }
}

/// Swarm-based folder analyzer
pub struct FolderAnalyzerSwarm {
    swarm: Swarm,
    scanner: FolderScanner,
}

impl FolderAnalyzerSwarm {
    pub fn new(root: impl Into<PathBuf>, max_depth: usize) -> Self {
        let scanner = FolderScanner::new(&root, max_depth);
        let mut swarm = create_dev_swarm();

        // Add specialized agents for folder analysis
        swarm.add_agent(
            Agent::new("Explorer", AgentRole::General)
                .with_prompt("You are a folder explorer. Analyze folder structures, \
                             identify patterns, and categorize content.")
                .with_expertise("File Systems")
                .with_expertise("Organization"),
        );

        swarm.add_agent(
            Agent::new("Categorizer", AgentRole::Architect)
                .with_prompt("You are a categorization expert. Identify the purpose and \
                             type of folders based on their contents and structure.")
                .with_expertise("Classification")
                .with_expertise("Patterns"),
        );

        swarm.add_agent(
            Agent::new("Prioritizer", AgentRole::Reviewer)
                .with_prompt("You are a prioritization specialist. Rank folders by \
                             importance, usefulness, and relevance.")
                .with_expertise("Prioritization")
                .with_expertise("Evaluation"),
        );

        Self { swarm, scanner }
    }

    pub async fn analyze(&mut self) -> Result<Vec<FolderAnalysis>> {
        info!("Starting folder analysis with swarm...");
        let start = Instant::now();

        // Phase 1: Scan folders
        info!("Scanning folders...");
        let mut folders = self.scanner.scan()?;
        info!("Found {} folders in {}", folders.len(), self.scanner.root.display());

        // Store folder data in shared memory
        {
            let memory = self.swarm.memory();
            let mut mem = memory.write().unwrap_or_else(|e| {
                warn!("Shared memory write lock poisoned, recovering");
                e.into_inner()
            });

            for (i, folder) in folders.iter().enumerate() {
                let key = format!("folder:{}", i);
                mem.write(
                    key,
                    serde_json::to_string(folder).unwrap_or_default(),
                    "scanner",
                );
            }

            mem.write("folder:count", folders.len().to_string(), "scanner");
        }

        // Phase 2: Create analysis tasks for top folders
        let top_folders: Vec<_> = folders
            .iter()
            .take(20) // Analyze top 20 folders in detail
            .enumerate()
            .collect();

        for (i, folder) in top_folders {
            let task = SwarmTask::new(format!(
                "Analyze folder '{}': {}. Description: {}. Files: {}, Size: {} bytes",
                folder.name, folder.path.display(), folder.description, folder.file_count,
                folder.total_size
            ))
            .with_role(AgentRole::Architect)
            .with_priority((30.0 - folder.priority_score.max(0.0).min(30.0)) as u8 + 5);

            self.swarm.queue_task(task)?;
        }

        // Phase 3: Create recommendation task
        let rec_task = SwarmTask::new("Create a recommended list of folders based on analysis")
            .with_role(AgentRole::Reviewer)
            .with_role(AgentRole::Architect)
            .with_priority(10);

        self.swarm.queue_task(rec_task)?;

        info!(
            "Analysis complete in {:.2?}. Found {} folders.",
            start.elapsed(),
            folders.len()
        );

        // Sort by priority score
        folders.sort_by(|a, b| b.priority_score.partial_cmp(&a.priority_score).unwrap());

        Ok(folders)
    }

    pub fn generate_recommendations(&self, folders: &[FolderAnalysis]) -> String {
        let mut output = String::from("# 📁 Recommended Folders\n\n");
        output.push_str("Generated by Selfware Swarm Folder Analyzer\n\n");
        output.push_str("## Top Recommendations\n\n");

        let top_folders: Vec<_> = folders
            .iter()
            .take(15)
            .enumerate()
            .collect();

        for (i, folder) in top_folders {
            output.push_str(&format!(
                "### {}. {} (Score: {:.1})\n\n",
                i + 1,
                folder.name,
                folder.priority_score
            ));

            output.push_str(&format!("**Path:** `{}`\n\n", folder.path.display()));
            output.push_str(&format!("**Description:** {}\n\n", folder.description));

            output.push_str("**Characteristics:**\n");
            if folder.has_readme {
                output.push_str("- ✅ Has README\n");
            }
            if folder.has_code {
                output.push_str("- 💻 Contains code\n");
            }
            if folder.has_tests {
                output.push_str("- 🧪 Has tests\n");
            }
            if folder.has_docs {
                output.push_str("- 📚 Has documentation\n");
            }

            output.push_str(&format!("\n**Stats:**\n"));
            output.push_str(&format!("- Files: {}\n", folder.file_count));
            output.push_str(&format!(
                "- Size: {:.2} MB\n",
                folder.total_size as f64 / 1_000_000.0
            ));
            output.push_str(&format!("- Depth: {}\n", folder.depth));

            if !folder.tags.is_empty() {
                output.push_str(&format!("\n**Tags:** {}\n", folder.tags.join(", ")));
            }

            output.push_str("\n---\n\n");
        }

        // Summary statistics
        output.push_str("## Summary Statistics\n\n");
        output.push_str(&format!("- **Total folders analyzed:** {}\n", folders.len()));
        output.push_str(&format!(
            "- **Folders with README:** {}\n",
            folders.iter().filter(|f| f.has_readme).count()
        ));
        output.push_str(&format!(
            "- **Folders with code:** {}\n",
            folders.iter().filter(|f| f.has_code).count()
        ));
        output.push_str(&format!(
            "- **Folders with tests:** {}\n",
            folders.iter().filter(|f| f.has_tests).count()
        ));
        output.push_str(&format!(
            "- **Average priority score:** {:.2}\n",
            folders
                .iter()
                .map(|f| f.priority_score)
                .sum::<f32>()
                / folders.len().max(1) as f32
        ));

        output
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize tracing
    tracing_subscriber::fmt::init();

    let root = std::env::args()
        .nth(1)
        .unwrap_or_else(|| ".".to_string());

    let max_depth = std::env::args()
        .nth(2)
        .map(|s| s.parse().unwrap_or(3))
        .unwrap_or(3);

    info!("Analyzing folders in: {}", root);
    info!("Max depth: {}", max_depth);

    let mut analyzer = FolderAnalyzerSwarm::new(&root, max_depth);
    let folders = analyzer.analyze().await?;

    // Generate recommendations
    let recommendations = analyzer.generate_recommendations(&folders);

    // Save to file
    let output_path = PathBuf::from(&root).join("RECOMMENDED_FOLDERS.md");
    std::fs::write(&output_path, &recommendments).with_context(|| {
        format!("Failed to write recommendations to {}", output_path.display())
    })?;

    info!(
        "Recommendations saved to {}",
        output_path.display()
    );

    // Print summary
    println!("\n{}", recommendations);

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_folder_analysis_scoring() {
        let mut analysis = FolderAnalysis::new(Path::new("test"), 0);
        analysis.file_count = 50;
        analysis.has_readme = true;
        analysis.has_code = true;
        analysis.has_tests = true;
        analysis.has_docs = true;
        analysis.total_size = 1_000_000;

        analysis.calculate_score();

        assert!(analysis.priority_score > 0.0);
        assert!(analysis.priority_score > 30.0); // Should be high priority
    }

    #[test]
    fn test_folder_analysis_depth_penalty() {
        let mut shallow = FolderAnalysis::new(Path::new("shallow"), 0);
        shallow.file_count = 10;
        shallow.has_readme = true;

        let mut deep = FolderAnalysis::new(Path::new("deep"), 5);
        deep.file_count = 10;
        deep.has_readme = true;

        shallow.calculate_score();
        deep.calculate_score();

        assert!(shallow.priority_score > deep.priority_score);
    }

    #[test]
    fn test_folder_scanner() {
        let temp_dir = tempfile::tempdir().unwrap();
        let scanner = FolderScanner::new(temp_dir.path(), 3);

        // Create some test structure
        std::fs::create_dir(temp_dir.path().join("src")).unwrap();
        std::fs::write(temp_dir.path().join("README.md")).unwrap();
        std::fs::write(temp_dir.path().join("src").join("main.rs")).unwrap();

        let folders = scanner.scan().unwrap();
        assert!(!folders.is_empty());
    }

    #[test]
    fn test_folder_analysis_description() {
        let analysis = FolderAnalysis::new(Path::new("test"), 0);
        let scanner = FolderScanner::new(".", 3);

        let desc = scanner.generate_description(&analysis);
        assert!(!desc.is_empty());
    }

    #[test]
    fn test_folder_analysis_tags() {
        let mut analysis = FolderAnalysis::new(Path::new("test"), 0);
        analysis.has_readme = true;
        analysis.has_code = true;
        analysis.priority_score = 35.0;

        let scanner = FolderScanner::new(".", 3);
        let tags = scanner.generate_tags(&analysis);

        assert!(tags.contains(&"documented".to_string()));
        assert!(tags.contains(&"source".to_string()));
        assert!(tags.contains(&"high-priority".to_string()));
    }
}
