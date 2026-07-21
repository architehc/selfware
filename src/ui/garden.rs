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
            GrowthStage::Seedling => Glyphs::seedling(),
            GrowthStage::Sprout => Glyphs::sprout(),
            GrowthStage::Established => Glyphs::leaf(),
            GrowthStage::Mature => Glyphs::tree(),
            GrowthStage::Ancient => Glyphs::tree(),
            GrowthStage::Wilting => Glyphs::fallen_leaf(),
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
            Glyphs::bloom()
        } else if self.health_score > 0.5 {
            Glyphs::wilt()
        } else {
            Glyphs::frost()
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
            Glyphs::tree(),
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
            Glyphs::sprout(),
            self.total_plants.to_string().emphasis(),
            self.beds.len().to_string().muted(),
            Glyphs::harvest(),
            self.total_lines.to_string().garden_healthy(),
            Glyphs::bloom(),
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
        beds.sort_by_key(|x| std::cmp::Reverse(x.total_lines));

        for bed in beds.iter().take(10) {
            output.push_str(&format!(
                "    {} {} {} — {} plants, {} lines\n",
                bed.health_indicator(),
                Glyphs::branch().muted(),
                bed.name.as_str().path_local(),
                bed.plants.len().to_string().muted(),
                bed.total_lines.to_string().muted()
            ));
        }

        if beds.len() > 10 {
            output.push_str(&format!(
                "    {} ... and {} more beds\n",
                Glyphs::leaf_branch().muted(),
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
        Glyphs::bloom()
    } else if health > 0.5 {
        Glyphs::sprout()
    } else {
        Glyphs::wilt()
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
#[path = "../../tests/unit/ui/garden/garden_test.rs"]
mod tests;
