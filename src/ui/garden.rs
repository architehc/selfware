//! Digital Garden Visualization
//!
//! View your codebase as a living garden - files as plants,
//! modules as garden beds, tests as pollinators.

use anyhow::Result;
use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::time::SystemTime;
use tracing::{debug, warn};
use walkdir::WalkDir;

use super::style::{Glyphs, SelfwareStyle};

/// A file in the garden, viewed as a plant
#[derive(Debug, Clone)]
pub struct GardenPlant {
    pub path: String,
    pub name: String,
    pub extension: String,
    pub lines: usize,
    pub age_days: u64,
    pub last_tended_days: u64,
    pub growth_stage: GrowthStage,
    pub plant_type: PlantType,
}

/// Growth stages based on file maturity
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum GrowthStage {
    Seedling,    // < 50 lines, new
    Sprout,      // 50-200 lines
    Established, // 200-500 lines
    Mature,      // 500+ lines
    Ancient,     // Very old, large files
    Wilting,     // Not touched in 90+ days
}

impl GrowthStage {
    pub fn from_metrics(lines: usize, _age_days: u64, last_tended_days: u64) -> Self {
        if last_tended_days > 90 {
            return GrowthStage::Wilting;
        }

        match lines {
            0..=50 => GrowthStage::Seedling,
            51..=200 => GrowthStage::Sprout,
            201..=500 => GrowthStage::Established,
            501..=1000 => GrowthStage::Mature,
            _ => GrowthStage::Ancient,
        }
    }

    pub fn glyph(&self) -> &'static str {
        match self {
            GrowthStage::Seedling => Glyphs::SEEDLING,
            GrowthStage::Sprout => Glyphs::SPROUT,
            GrowthStage::Established => Glyphs::LEAF,
            GrowthStage::Mature => Glyphs::TREE,
            GrowthStage::Ancient => Glyphs::TREE,
            GrowthStage::Wilting => Glyphs::FALLEN_LEAF,
        }
    }

    pub fn description(&self) -> &'static str {
        match self {
            GrowthStage::Seedling => "seedling",
            GrowthStage::Sprout => "sprouting",
            GrowthStage::Established => "established",
            GrowthStage::Mature => "mature",
            GrowthStage::Ancient => "ancient",
            GrowthStage::Wilting => "needs attention",
        }
    }
}

/// Types of plants based on file purpose
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PlantType {
    Flower,     // Main code (lib.rs, main.rs)
    Herb,       // Utilities, helpers
    Vegetable,  // Core business logic
    Fruit,      // Output/build artifacts
    Pollinator, // Tests
    Roots,      // Configuration
    Trellis,    // Infrastructure (CI, build scripts)
}

impl PlantType {
    pub fn from_path(path: &str) -> Self {
        let path_lower = path.to_lowercase();

        if path_lower.contains("test") {
            return PlantType::Pollinator;
        }
        if path_lower.ends_with("main.rs") || path_lower.ends_with("lib.rs") {
            return PlantType::Flower;
        }
        if path_lower.contains("config")
            || path_lower.ends_with(".toml")
            || path_lower.ends_with(".json")
        {
            return PlantType::Roots;
        }
        if path_lower.contains("util") || path_lower.contains("helper") {
            return PlantType::Herb;
        }
        if path_lower.contains(".github")
            || path_lower.contains("ci")
            || path_lower.ends_with(".sh")
        {
            return PlantType::Trellis;
        }
        if path_lower.contains("target")
            || path_lower.contains("build")
            || path_lower.contains("dist")
        {
            return PlantType::Fruit;
        }

        PlantType::Vegetable
    }

    pub fn description(&self) -> &'static str {
        match self {
            PlantType::Flower => "flowering (entry points)",
            PlantType::Herb => "herbs (utilities)",
            PlantType::Vegetable => "vegetables (core logic)",
            PlantType::Fruit => "fruits (outputs)",
            PlantType::Pollinator => "pollinators (tests)",
            PlantType::Roots => "roots (config)",
            PlantType::Trellis => "trellis (infrastructure)",
        }
    }
}

/// A garden bed (directory/module)
#[derive(Debug, Clone)]
pub struct GardenBed {
    pub name: String,
    pub path: String,
    pub plants: Vec<GardenPlant>,
    pub total_lines: usize,
    pub health_score: f32,
}

impl GardenBed {
    pub fn new(path: &str) -> Self {
        Self {
            name: Path::new(path)
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| path.to_string()),
            path: path.to_string(),
            plants: Vec::new(),
            total_lines: 0,
            health_score: 1.0,
        }
    }

    pub fn add_plant(&mut self, plant: GardenPlant) {
        self.total_lines += plant.lines;
        self.plants.push(plant);
        self.recalculate_health();
    }

    fn recalculate_health(&mut self) {
        if self.plants.is_empty() {
            self.health_score = 1.0;
            return;
        }

        let wilting_count = self
            .plants
            .iter()
            .filter(|p| p.growth_stage == GrowthStage::Wilting)
            .count();

        let health = 1.0 - (wilting_count as f32 / self.plants.len() as f32);
        self.health_score = health.max(0.0);
    }

    pub fn health_indicator(&self) -> &'static str {
        if self.health_score > 0.8 {
            Glyphs::BLOOM
        } else if self.health_score > 0.5 {
            Glyphs::WILT
        } else {
            Glyphs::FROST
        }
    }
}

/// The complete digital garden
#[derive(Debug, Clone)]
pub struct DigitalGarden {
    pub project_name: String,
    pub beds: HashMap<String, GardenBed>,
    pub total_plants: usize,
    pub total_lines: usize,
    pub season: Season,
}

/// Current "season" based on recent activity
#[derive(Debug, Clone, Copy)]
pub enum Season {
    Spring, // Lots of new files
    Summer, // Active development
    Autumn, // Maintenance mode
    Winter, // Dormant
}

impl Season {
    pub fn glyph(&self) -> &'static str {
        match self {
            Season::Spring => "🌸",
            Season::Summer => "☀️",
            Season::Autumn => "🍂",
            Season::Winter => "❄️",
        }
    }

    pub fn description(&self) -> &'static str {
        match self {
            Season::Spring => "spring (rapid growth)",
            Season::Summer => "summer (active tending)",
            Season::Autumn => "autumn (harvesting)",
            Season::Winter => "winter (resting)",
        }
    }
}

impl DigitalGarden {
    pub fn new(project_name: &str) -> Self {
        Self {
            project_name: project_name.to_string(),
            beds: HashMap::new(),
            total_plants: 0,
            total_lines: 0,
            season: Season::Summer,
        }
    }

    pub fn add_plant(&mut self, plant: GardenPlant) {
        let bed_path = Path::new(&plant.path)
            .parent()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_else(|| ".".to_string());

        let bed = self
            .beds
            .entry(bed_path.clone())
            .or_insert_with(|| GardenBed::new(&bed_path));

        self.total_lines += plant.lines;
        self.total_plants += 1;
        bed.add_plant(plant);
    }

    /// Render the garden overview
    pub fn render(&self) -> String {
        let mut output = String::new();

        // Header
        output.push_str(&format!(
            "\n{} Your Digital Garden: {}\n",
            Glyphs::TREE,
            self.project_name.as_str().emphasis()
        ));
        output.push_str(&format!(
            "{} Season: {}\n\n",
            self.season.glyph(),
            self.season.description().craftsman_voice()
        ));

        // Summary stats
        output.push_str(&self.render_summary());
        output.push('\n');

        // Growth stage breakdown
        output.push_str(&self.render_growth_stages());
        output.push('\n');

        // Garden beds
        output.push_str(&self.render_beds());

        output
    }

    fn render_summary(&self) -> String {
        let _seedlings = self.count_by_stage(GrowthStage::Seedling);
        let established = self.count_by_stage(GrowthStage::Established)
            + self.count_by_stage(GrowthStage::Mature);
        let wilting = self.count_by_stage(GrowthStage::Wilting);

        format!(
            r#"Garden Summary:
    {} {} plants across {} beds
    {} {} lines of carefully tended code
    {} {} healthy, {} need attention
"#,
            Glyphs::SPROUT,
            self.total_plants.to_string().emphasis(),
            self.beds.len().to_string().muted(),
            Glyphs::HARVEST,
            self.total_lines.to_string().garden_healthy(),
            Glyphs::BLOOM,
            established.to_string().garden_healthy(),
            if wilting > 0 {
                wilting.to_string().garden_wilting()
            } else {
                "0".to_string().muted()
            }
        )
    }

    fn render_growth_stages(&self) -> String {
        let stages = [
            (GrowthStage::Seedling, "Seedlings (new code)"),
            (GrowthStage::Sprout, "Sprouts (growing)"),
            (GrowthStage::Established, "Established"),
            (GrowthStage::Mature, "Mature"),
            (GrowthStage::Wilting, "Need attention"),
        ];

        let mut output = String::from("Growth Stages:\n");

        for (stage, desc) in stages {
            let count = self.count_by_stage(stage);
            if count > 0 {
                let bar = self.render_bar(count, self.total_plants.max(1), 20);
                output.push_str(&format!(
                    "    {} {:.<20} {} {}\n",
                    stage.glyph(),
                    desc,
                    bar,
                    count.to_string().muted()
                ));
            }
        }

        output
    }

    fn render_beds(&self) -> String {
        let mut output = String::from("Garden Beds:\n");

        let mut beds: Vec<_> = self.beds.values().collect();
        beds.sort_by(|a, b| b.total_lines.cmp(&a.total_lines));

        for bed in beds.iter().take(10) {
            output.push_str(&format!(
                "    {} {} {} — {} plants, {} lines\n",
                bed.health_indicator(),
                Glyphs::BRANCH.muted(),
                bed.name.as_str().path_local(),
                bed.plants.len().to_string().muted(),
                bed.total_lines.to_string().muted()
            ));
        }

        if beds.len() > 10 {
            output.push_str(&format!(
                "    {} ... and {} more beds\n",
                Glyphs::LEAF_BRANCH.muted(),
                (beds.len() - 10).to_string().muted()
            ));
        }

        output
    }

    fn render_bar(&self, value: usize, max: usize, width: usize) -> String {
        let filled = (value as f32 / max as f32 * width as f32) as usize;
        let empty = width.saturating_sub(filled);
        format!(
            "{}{}",
            "█".repeat(filled).garden_healthy(),
            "░".repeat(empty).muted()
        )
    }

    fn count_by_stage(&self, stage: GrowthStage) -> usize {
        self.beds
            .values()
            .flat_map(|b| &b.plants)
            .filter(|p| p.growth_stage == stage)
            .count()
    }
}

/// Build a digital garden visualization from a path.
pub fn build_garden_from_path(path: &str) -> Result<DigitalGarden> {
    let project_name = Path::new(path)
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| {
            warn!(
                "Could not derive project name from path '{}'; using fallback name",
                path
            );
            "your garden".to_string()
        });

    let mut garden = DigitalGarden::new(&project_name);

    let sep = std::path::MAIN_SEPARATOR_STR;

    for entry in WalkDir::new(path)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file())
    {
        let path_str = entry.path().display().to_string();

        if path_str.contains(&format!("{sep}."))
            || path_str.contains(&format!("{sep}target{sep}"))
            || path_str.contains(&format!("{sep}node_modules{sep}"))
            || path_str.contains(&format!("{sep}__pycache__{sep}"))
        {
            continue;
        }

        let ext = entry
            .path()
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or_else(|| {
                debug!(
                    "Skipping file with non-UTF8 extension: {}",
                    entry.path().display()
                );
                ""
            });

        if !matches!(
            ext,
            "rs" | "py"
                | "js"
                | "ts"
                | "tsx"
                | "jsx"
                | "go"
                | "rb"
                | "java"
                | "c"
                | "cpp"
                | "h"
                | "hpp"
                | "md"
                | "toml"
                | "yaml"
                | "yml"
                | "json"
        ) {
            continue;
        }

        let metadata = fs::metadata(entry.path()).ok();
        let lines = fs::read_to_string(entry.path())
            .map(|c| c.lines().count())
            .unwrap_or_else(|err| {
                debug!(
                    "Failed to read '{}' when computing garden metrics: {}",
                    entry.path().display(),
                    err
                );
                0
            });

        let modified = metadata
            .as_ref()
            .and_then(|m| m.modified().ok())
            .and_then(|t| t.duration_since(SystemTime::UNIX_EPOCH).ok())
            .map(|d| d.as_secs())
            .unwrap_or_else(|| {
                debug!(
                    "Could not read modified time for '{}'; using epoch fallback",
                    entry.path().display()
                );
                0
            });

        let now = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or_else(|err| {
                warn!(
                    "System clock appears invalid when building garden view: {}",
                    err
                );
                0
            });

        let age_days = now.saturating_sub(modified) / 86400;

        let plant = GardenPlant {
            path: path_str.clone(),
            name: entry.file_name().to_string_lossy().to_string(),
            extension: ext.to_string(),
            lines,
            age_days,
            last_tended_days: age_days,
            growth_stage: GrowthStage::from_metrics(lines, age_days, age_days),
            plant_type: PlantType::from_path(&path_str),
        };

        garden.add_plant(plant);
    }

    Ok(garden)
}

/// Render a single file in garden view
pub fn render_plant(plant: &GardenPlant) -> String {
    format!(
        "{} {} {} — {} lines, {} days old",
        plant.growth_stage.glyph(),
        plant.name.as_str().emphasis(),
        format!("({})", plant.growth_stage.description()).muted(),
        plant.lines.to_string().muted(),
        plant.age_days.to_string().muted()
    )
}

/// Quick garden status for the status bar
pub fn garden_status_short(garden: &DigitalGarden) -> String {
    let health =
        garden.beds.values().map(|b| b.health_score).sum::<f32>() / garden.beds.len().max(1) as f32;

    let health_glyph = if health > 0.8 {
        Glyphs::BLOOM
    } else if health > 0.5 {
        Glyphs::SPROUT
    } else {
        Glyphs::WILT
    };

    format!("{} {} plants", health_glyph, garden.total_plants)
}

/// Scan a directory and create a DigitalGarden from its contents
pub fn scan_directory(dir: &Path) -> DigitalGarden {
    use walkdir::WalkDir;

    let project_name = dir
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "project".to_string());

    let mut garden = DigitalGarden::new(&project_name);

    // Code file extensions to include
    let code_extensions = [
        "rs", "toml", "md", "ts", "tsx", "js", "jsx", "py", "go", "java", "c", "cpp", "h", "hpp",
        "cs", "rb", "php", "swift", "kt", "scala", "sh", "bash", "zsh", "yaml", "yml", "json",
    ];

    let sep = std::path::MAIN_SEPARATOR_STR;

    for entry in WalkDir::new(dir)
        .max_depth(8) // Limit depth to avoid scanning too deep
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file())
    {
        let path = entry.path();
        let path_str = path.strip_prefix(dir).unwrap_or(path).display().to_string();

        // Skip common non-code directories (use platform path separator)
        if path_str.contains(&format!("{sep}target{sep}"))
            || path_str.contains(&format!("{sep}node_modules{sep}"))
            || path_str.contains(&format!("{sep}.git{sep}"))
            || path_str.contains(&format!("{sep}__pycache__{sep}"))
            || path_str.contains(&format!("{sep}vendor{sep}"))
            || path_str.contains(&format!("{sep}dist{sep}"))
            || path_str.contains(&format!("{sep}build{sep}"))
        {
            continue;
        }

        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_lowercase();

        if !code_extensions.contains(&ext.as_str()) {
            continue;
        }

        // Read file metadata
        let lines = std::fs::read_to_string(path)
            .map(|s| s.lines().count())
            .unwrap_or(0);

        let metadata = std::fs::metadata(path).ok();
        let age_days = metadata
            .as_ref()
            .and_then(|m| m.created().ok())
            .and_then(|t| t.elapsed().ok())
            .map(|d| d.as_secs() / 86400)
            .unwrap_or(0);

        let last_modified_days = metadata
            .and_then(|m| m.modified().ok())
            .and_then(|t| t.elapsed().ok())
            .map(|d| d.as_secs() / 86400)
            .unwrap_or(0);

        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();

        let plant = GardenPlant {
            path: path_str,
            name,
            extension: ext,
            lines,
            age_days,
            last_tended_days: last_modified_days,
            growth_stage: GrowthStage::from_metrics(lines, age_days, last_modified_days),
            plant_type: PlantType::from_path(path.to_string_lossy().as_ref()),
        };

        garden.add_plant(plant);
    }

    garden
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_growth_stage_from_metrics() {
        assert_eq!(GrowthStage::from_metrics(10, 1, 1), GrowthStage::Seedling);
        assert_eq!(GrowthStage::from_metrics(100, 30, 5), GrowthStage::Sprout);
        assert_eq!(
            GrowthStage::from_metrics(300, 60, 10),
            GrowthStage::Established
        );
        assert_eq!(
            GrowthStage::from_metrics(100, 30, 100),
            GrowthStage::Wilting
        );
    }

    #[test]
    fn test_growth_stage_all_stages() {
        // Seedling: 0-50 lines
        assert_eq!(GrowthStage::from_metrics(0, 1, 1), GrowthStage::Seedling);
        assert_eq!(GrowthStage::from_metrics(50, 1, 1), GrowthStage::Seedling);

        // Sprout: 51-200 lines
        assert_eq!(GrowthStage::from_metrics(51, 1, 1), GrowthStage::Sprout);
        assert_eq!(GrowthStage::from_metrics(200, 1, 1), GrowthStage::Sprout);

        // Established: 201-500 lines
        assert_eq!(
            GrowthStage::from_metrics(201, 1, 1),
            GrowthStage::Established
        );
        assert_eq!(
            GrowthStage::from_metrics(500, 1, 1),
            GrowthStage::Established
        );

        // Mature: 501-1000 lines
        assert_eq!(GrowthStage::from_metrics(501, 1, 1), GrowthStage::Mature);
        assert_eq!(GrowthStage::from_metrics(1000, 1, 1), GrowthStage::Mature);

        // Ancient: >1000 lines
        assert_eq!(GrowthStage::from_metrics(1001, 1, 1), GrowthStage::Ancient);
        assert_eq!(GrowthStage::from_metrics(5000, 1, 1), GrowthStage::Ancient);

        // Wilting overrides all (>90 days)
        assert_eq!(GrowthStage::from_metrics(5000, 1, 91), GrowthStage::Wilting);
    }

    #[test]
    fn test_growth_stage_glyph() {
        assert_eq!(GrowthStage::Seedling.glyph(), Glyphs::SEEDLING);
        assert_eq!(GrowthStage::Sprout.glyph(), Glyphs::SPROUT);
        assert_eq!(GrowthStage::Established.glyph(), Glyphs::LEAF);
        assert_eq!(GrowthStage::Mature.glyph(), Glyphs::TREE);
        assert_eq!(GrowthStage::Ancient.glyph(), Glyphs::TREE);
        assert_eq!(GrowthStage::Wilting.glyph(), Glyphs::FALLEN_LEAF);
    }

    #[test]
    fn test_growth_stage_description() {
        assert_eq!(GrowthStage::Seedling.description(), "seedling");
        assert_eq!(GrowthStage::Sprout.description(), "sprouting");
        assert_eq!(GrowthStage::Established.description(), "established");
        assert_eq!(GrowthStage::Mature.description(), "mature");
        assert_eq!(GrowthStage::Ancient.description(), "ancient");
        assert_eq!(GrowthStage::Wilting.description(), "needs attention");
    }

    #[test]
    fn test_plant_type_from_path() {
        assert_eq!(PlantType::from_path("src/main.rs"), PlantType::Flower);
        assert_eq!(PlantType::from_path("tests/unit.rs"), PlantType::Pollinator);
        assert_eq!(PlantType::from_path("config.toml"), PlantType::Roots);
    }

    #[test]
    fn test_plant_type_from_path_comprehensive() {
        // Flower - entry points
        assert_eq!(PlantType::from_path("src/main.rs"), PlantType::Flower);
        assert_eq!(PlantType::from_path("src/lib.rs"), PlantType::Flower);

        // Pollinator - tests
        assert_eq!(PlantType::from_path("tests/unit.rs"), PlantType::Pollinator);
        assert_eq!(
            PlantType::from_path("src/test_utils.rs"),
            PlantType::Pollinator
        );

        // Roots - config
        assert_eq!(PlantType::from_path("config.toml"), PlantType::Roots);
        assert_eq!(PlantType::from_path("settings.json"), PlantType::Roots);
        assert_eq!(PlantType::from_path("src/config/mod.rs"), PlantType::Roots);

        // Herb - utilities
        assert_eq!(PlantType::from_path("src/utils.rs"), PlantType::Herb);
        assert_eq!(PlantType::from_path("src/helpers/mod.rs"), PlantType::Herb);

        // Trellis - infrastructure
        assert_eq!(
            PlantType::from_path(".github/workflows/ci.yml"),
            PlantType::Trellis
        );
        assert_eq!(PlantType::from_path("scripts/build.sh"), PlantType::Trellis);

        // Fruit - build outputs
        assert_eq!(PlantType::from_path("target/debug/main"), PlantType::Fruit);
        assert_eq!(PlantType::from_path("build/output.js"), PlantType::Fruit);
        assert_eq!(PlantType::from_path("dist/bundle.js"), PlantType::Fruit);

        // Vegetable - default
        assert_eq!(PlantType::from_path("src/api.rs"), PlantType::Vegetable);
        assert_eq!(
            PlantType::from_path("src/models/user.rs"),
            PlantType::Vegetable
        );
    }

    #[test]
    fn test_plant_type_description() {
        assert_eq!(PlantType::Flower.description(), "flowering (entry points)");
        assert_eq!(PlantType::Herb.description(), "herbs (utilities)");
        assert_eq!(
            PlantType::Vegetable.description(),
            "vegetables (core logic)"
        );
        assert_eq!(PlantType::Fruit.description(), "fruits (outputs)");
        assert_eq!(PlantType::Pollinator.description(), "pollinators (tests)");
        assert_eq!(PlantType::Roots.description(), "roots (config)");
        assert_eq!(PlantType::Trellis.description(), "trellis (infrastructure)");
    }

    #[test]
    fn test_garden_bed_health() {
        let mut bed = GardenBed::new("src");
        assert_eq!(bed.health_score, 1.0);

        bed.add_plant(GardenPlant {
            path: "src/lib.rs".to_string(),
            name: "lib.rs".to_string(),
            extension: "rs".to_string(),
            lines: 100,
            age_days: 10,
            last_tended_days: 5,
            growth_stage: GrowthStage::Sprout,
            plant_type: PlantType::Flower,
        });

        assert_eq!(bed.health_score, 1.0);
    }

    #[test]
    fn test_garden_bed_with_wilting_plants() {
        let mut bed = GardenBed::new("src");

        // Add healthy plant
        bed.add_plant(GardenPlant {
            path: "src/healthy.rs".to_string(),
            name: "healthy.rs".to_string(),
            extension: "rs".to_string(),
            lines: 100,
            age_days: 10,
            last_tended_days: 5,
            growth_stage: GrowthStage::Sprout,
            plant_type: PlantType::Vegetable,
        });

        // Add wilting plant
        bed.add_plant(GardenPlant {
            path: "src/wilting.rs".to_string(),
            name: "wilting.rs".to_string(),
            extension: "rs".to_string(),
            lines: 100,
            age_days: 200,
            last_tended_days: 150,
            growth_stage: GrowthStage::Wilting,
            plant_type: PlantType::Vegetable,
        });

        // 1 of 2 wilting = 0.5 health (not > 0.5, so FROST)
        assert_eq!(bed.health_score, 0.5);
        assert_eq!(bed.health_indicator(), Glyphs::FROST);
    }

    #[test]
    fn test_garden_bed_all_wilting() {
        let mut bed = GardenBed::new("src");

        bed.add_plant(GardenPlant {
            path: "src/old1.rs".to_string(),
            name: "old1.rs".to_string(),
            extension: "rs".to_string(),
            lines: 100,
            age_days: 200,
            last_tended_days: 150,
            growth_stage: GrowthStage::Wilting,
            plant_type: PlantType::Vegetable,
        });

        bed.add_plant(GardenPlant {
            path: "src/old2.rs".to_string(),
            name: "old2.rs".to_string(),
            extension: "rs".to_string(),
            lines: 100,
            age_days: 200,
            last_tended_days: 150,
            growth_stage: GrowthStage::Wilting,
            plant_type: PlantType::Vegetable,
        });

        assert_eq!(bed.health_score, 0.0);
        assert_eq!(bed.health_indicator(), Glyphs::FROST);
    }

    #[test]
    fn test_garden_bed_health_indicator() {
        let mut bed = GardenBed::new("test");

        // Empty bed
        assert_eq!(bed.health_indicator(), Glyphs::BLOOM);

        // Add healthy plant - should still be healthy
        bed.add_plant(GardenPlant {
            path: "test/file.rs".to_string(),
            name: "file.rs".to_string(),
            extension: "rs".to_string(),
            lines: 50,
            age_days: 5,
            last_tended_days: 1,
            growth_stage: GrowthStage::Seedling,
            plant_type: PlantType::Vegetable,
        });

        assert_eq!(bed.health_indicator(), Glyphs::BLOOM);
    }

    #[test]
    fn test_digital_garden() {
        let mut garden = DigitalGarden::new("test-project");

        garden.add_plant(GardenPlant {
            path: "src/main.rs".to_string(),
            name: "main.rs".to_string(),
            extension: "rs".to_string(),
            lines: 50,
            age_days: 10,
            last_tended_days: 1,
            growth_stage: GrowthStage::Seedling,
            plant_type: PlantType::Flower,
        });

        assert_eq!(garden.total_plants, 1);
        assert_eq!(garden.total_lines, 50);
    }

    #[test]
    fn test_digital_garden_multiple_beds() {
        let mut garden = DigitalGarden::new("multi-bed");

        // Add to src/
        garden.add_plant(GardenPlant {
            path: "src/main.rs".to_string(),
            name: "main.rs".to_string(),
            extension: "rs".to_string(),
            lines: 100,
            age_days: 10,
            last_tended_days: 1,
            growth_stage: GrowthStage::Sprout,
            plant_type: PlantType::Flower,
        });

        // Add to tests/
        garden.add_plant(GardenPlant {
            path: "tests/test.rs".to_string(),
            name: "test.rs".to_string(),
            extension: "rs".to_string(),
            lines: 50,
            age_days: 10,
            last_tended_days: 1,
            growth_stage: GrowthStage::Seedling,
            plant_type: PlantType::Pollinator,
        });

        assert_eq!(garden.total_plants, 2);
        assert_eq!(garden.total_lines, 150);
        assert_eq!(garden.beds.len(), 2);
    }

    #[test]
    fn test_digital_garden_render() {
        let mut garden = DigitalGarden::new("render-test");

        garden.add_plant(GardenPlant {
            path: "src/main.rs".to_string(),
            name: "main.rs".to_string(),
            extension: "rs".to_string(),
            lines: 100,
            age_days: 10,
            last_tended_days: 1,
            growth_stage: GrowthStage::Sprout,
            plant_type: PlantType::Flower,
        });

        let output = garden.render();
        assert!(output.contains("render-test"));
        assert!(output.contains("Digital Garden"));
        assert!(output.contains("Season"));
        assert!(output.contains("Garden Summary"));
        assert!(output.contains("Growth Stages"));
        assert!(output.contains("Garden Beds"));
    }

    #[test]
    fn test_digital_garden_render_empty() {
        let garden = DigitalGarden::new("empty-garden");
        let output = garden.render();
        assert!(output.contains("empty-garden"));
        assert!(output.contains("plants across"));
        assert!(output.contains("0"));
    }

    #[test]
    fn test_digital_garden_render_many_beds() {
        let mut garden = DigitalGarden::new("large-project");

        // Add plants to 15 different directories
        for i in 0..15 {
            garden.add_plant(GardenPlant {
                path: format!("src/mod{}/file.rs", i),
                name: "file.rs".to_string(),
                extension: "rs".to_string(),
                lines: 100 * (i + 1),
                age_days: 10,
                last_tended_days: 1,
                growth_stage: GrowthStage::Established,
                plant_type: PlantType::Vegetable,
            });
        }

        let output = garden.render();
        // Should show "and X more beds" message
        assert!(output.contains("more beds"));
    }

    #[test]
    fn test_season_glyph() {
        assert!(!Season::Spring.glyph().is_empty());
        assert!(!Season::Summer.glyph().is_empty());
        assert!(!Season::Autumn.glyph().is_empty());
        assert!(!Season::Winter.glyph().is_empty());
    }

    #[test]
    fn test_season_description() {
        assert!(Season::Spring.description().contains("spring"));
        assert!(Season::Summer.description().contains("summer"));
        assert!(Season::Autumn.description().contains("autumn"));
        assert!(Season::Winter.description().contains("winter"));
    }

    #[test]
    fn test_render_plant() {
        let plant = GardenPlant {
            path: "src/lib.rs".to_string(),
            name: "lib.rs".to_string(),
            extension: "rs".to_string(),
            lines: 250,
            age_days: 30,
            last_tended_days: 5,
            growth_stage: GrowthStage::Established,
            plant_type: PlantType::Flower,
        };

        let rendered = render_plant(&plant);
        assert!(rendered.contains("lib.rs"));
        assert!(rendered.contains("250"));
        assert!(rendered.contains("30"));
        assert!(rendered.contains("established"));
    }

    #[test]
    fn test_garden_status_short() {
        let mut garden = DigitalGarden::new("status-test");

        garden.add_plant(GardenPlant {
            path: "src/main.rs".to_string(),
            name: "main.rs".to_string(),
            extension: "rs".to_string(),
            lines: 100,
            age_days: 10,
            last_tended_days: 1,
            growth_stage: GrowthStage::Sprout,
            plant_type: PlantType::Flower,
        });

        let status = garden_status_short(&garden);
        assert!(status.contains("1 plants"));
    }

    #[test]
    fn test_garden_status_short_healthy() {
        let mut garden = DigitalGarden::new("healthy");

        // Add healthy plants
        for i in 0..5 {
            garden.add_plant(GardenPlant {
                path: format!("src/file{}.rs", i),
                name: format!("file{}.rs", i),
                extension: "rs".to_string(),
                lines: 100,
                age_days: 10,
                last_tended_days: 1,
                growth_stage: GrowthStage::Sprout,
                plant_type: PlantType::Vegetable,
            });
        }

        let status = garden_status_short(&garden);
        assert!(status.contains("5 plants"));
        assert!(status.contains(Glyphs::BLOOM)); // Healthy indicator
    }

    #[test]
    fn test_garden_status_short_struggling() {
        let mut garden = DigitalGarden::new("struggling");

        // Add some wilting plants
        for i in 0..3 {
            garden.add_plant(GardenPlant {
                path: format!("src/old{}.rs", i),
                name: format!("old{}.rs", i),
                extension: "rs".to_string(),
                lines: 100,
                age_days: 200,
                last_tended_days: 150,
                growth_stage: GrowthStage::Wilting,
                plant_type: PlantType::Vegetable,
            });
        }

        // Add one healthy plant
        garden.add_plant(GardenPlant {
            path: "src/new.rs".to_string(),
            name: "new.rs".to_string(),
            extension: "rs".to_string(),
            lines: 50,
            age_days: 5,
            last_tended_days: 1,
            growth_stage: GrowthStage::Seedling,
            plant_type: PlantType::Vegetable,
        });

        let status = garden_status_short(&garden);
        assert!(status.contains("4 plants"));
    }

    #[test]
    fn test_garden_bed_new() {
        let bed = GardenBed::new("/home/user/project/src");
        assert_eq!(bed.name, "src");
        assert_eq!(bed.path, "/home/user/project/src");
        assert!(bed.plants.is_empty());
        assert_eq!(bed.total_lines, 0);
        assert_eq!(bed.health_score, 1.0);
    }

    #[test]
    fn test_garden_bed_new_simple_path() {
        let bed = GardenBed::new("src");
        assert_eq!(bed.name, "src");
        assert_eq!(bed.path, "src");
    }

    #[test]
    fn test_count_by_stage() {
        let mut garden = DigitalGarden::new("count-test");

        // Add plants with different stages
        garden.add_plant(GardenPlant {
            path: "src/seedling.rs".to_string(),
            name: "seedling.rs".to_string(),
            extension: "rs".to_string(),
            lines: 20,
            age_days: 1,
            last_tended_days: 1,
            growth_stage: GrowthStage::Seedling,
            plant_type: PlantType::Vegetable,
        });

        garden.add_plant(GardenPlant {
            path: "src/mature.rs".to_string(),
            name: "mature.rs".to_string(),
            extension: "rs".to_string(),
            lines: 800,
            age_days: 100,
            last_tended_days: 5,
            growth_stage: GrowthStage::Mature,
            plant_type: PlantType::Vegetable,
        });

        garden.add_plant(GardenPlant {
            path: "src/wilting.rs".to_string(),
            name: "wilting.rs".to_string(),
            extension: "rs".to_string(),
            lines: 100,
            age_days: 200,
            last_tended_days: 150,
            growth_stage: GrowthStage::Wilting,
            plant_type: PlantType::Vegetable,
        });

        // Render should include correct counts
        let output = garden.render();
        assert!(output.contains("need attention"));
    }

    #[test]
    fn test_garden_plant_clone() {
        let plant = GardenPlant {
            path: "src/test.rs".to_string(),
            name: "test.rs".to_string(),
            extension: "rs".to_string(),
            lines: 100,
            age_days: 10,
            last_tended_days: 5,
            growth_stage: GrowthStage::Sprout,
            plant_type: PlantType::Vegetable,
        };

        let cloned = plant.clone();
        assert_eq!(cloned.path, plant.path);
        assert_eq!(cloned.lines, plant.lines);
    }

    #[test]
    fn test_garden_bed_clone() {
        let mut bed = GardenBed::new("src");
        bed.add_plant(GardenPlant {
            path: "src/file.rs".to_string(),
            name: "file.rs".to_string(),
            extension: "rs".to_string(),
            lines: 100,
            age_days: 10,
            last_tended_days: 5,
            growth_stage: GrowthStage::Sprout,
            plant_type: PlantType::Vegetable,
        });

        let cloned = bed.clone();
        assert_eq!(cloned.name, bed.name);
        assert_eq!(cloned.plants.len(), bed.plants.len());
    }

    #[test]
    fn test_digital_garden_clone() {
        let mut garden = DigitalGarden::new("test");
        garden.add_plant(GardenPlant {
            path: "src/main.rs".to_string(),
            name: "main.rs".to_string(),
            extension: "rs".to_string(),
            lines: 50,
            age_days: 5,
            last_tended_days: 1,
            growth_stage: GrowthStage::Seedling,
            plant_type: PlantType::Flower,
        });

        let cloned = garden.clone();
        assert_eq!(cloned.project_name, garden.project_name);
        assert_eq!(cloned.total_plants, garden.total_plants);
    }
}
