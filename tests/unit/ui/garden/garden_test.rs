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
    assert_eq!(GrowthStage::Seedling.glyph(), Glyphs::seedling());
    assert_eq!(GrowthStage::Sprout.glyph(), Glyphs::sprout());
    assert_eq!(GrowthStage::Established.glyph(), Glyphs::leaf());
    assert_eq!(GrowthStage::Mature.glyph(), Glyphs::tree());
    assert_eq!(GrowthStage::Ancient.glyph(), Glyphs::tree());
    assert_eq!(GrowthStage::Wilting.glyph(), Glyphs::fallen_leaf());
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
    assert_eq!(bed.health_indicator(), Glyphs::frost());
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
    assert_eq!(bed.health_indicator(), Glyphs::frost());
}

#[test]
fn test_garden_bed_health_indicator() {
    let mut bed = GardenBed::new("test");

    // Empty bed
    assert_eq!(bed.health_indicator(), Glyphs::bloom());

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

    assert_eq!(bed.health_indicator(), Glyphs::bloom());
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
    assert!(status.contains(Glyphs::bloom())); // Healthy indicator
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
