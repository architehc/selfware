use super::*;

#[test]
fn test_theme_switching() {
    set_theme(ThemeId::Amber);
    assert_eq!(current_theme_id(), ThemeId::Amber);

    set_theme(ThemeId::Ocean);
    assert_eq!(current_theme_id(), ThemeId::Ocean);

    set_theme(ThemeId::Minimal);
    assert_eq!(current_theme_id(), ThemeId::Minimal);

    set_theme(ThemeId::HighContrast);
    assert_eq!(current_theme_id(), ThemeId::HighContrast);
}

#[test]
fn test_theme_colors() {
    let amber = theme_colors(ThemeId::Amber);
    assert_eq!(amber.primary.r, 212);
    assert_eq!(amber.primary.g, 163);
    assert_eq!(amber.primary.b, 115);

    let ocean = theme_colors(ThemeId::Ocean);
    assert_eq!(ocean.primary.r, 100);
    assert_eq!(ocean.primary.g, 149);
    assert_eq!(ocean.primary.b, 237);
}

#[test]
fn test_theme_from_name_all_variants() {
    assert_eq!(theme_from_name("amber"), Some(ThemeId::Amber));
    assert_eq!(theme_from_name("ocean"), Some(ThemeId::Ocean));
    assert_eq!(theme_from_name("minimal"), Some(ThemeId::Minimal));
    assert_eq!(theme_from_name("highcontrast"), Some(ThemeId::HighContrast));
    assert_eq!(
        theme_from_name("high-contrast"),
        Some(ThemeId::HighContrast)
    );
    assert_eq!(
        theme_from_name("high_contrast"),
        Some(ThemeId::HighContrast)
    );
    assert_eq!(theme_from_name("dracula"), Some(ThemeId::Dracula));
    assert_eq!(theme_from_name("monokai"), Some(ThemeId::Monokai));
    assert_eq!(
        theme_from_name("solarized-dark"),
        Some(ThemeId::SolarizedDark)
    );
    assert_eq!(
        theme_from_name("solarizeddark"),
        Some(ThemeId::SolarizedDark)
    );
    assert_eq!(
        theme_from_name("solarized_dark"),
        Some(ThemeId::SolarizedDark)
    );
    assert_eq!(
        theme_from_name("solarized-light"),
        Some(ThemeId::SolarizedLight)
    );
    assert_eq!(
        theme_from_name("solarizedlight"),
        Some(ThemeId::SolarizedLight)
    );
    assert_eq!(
        theme_from_name("solarized_light"),
        Some(ThemeId::SolarizedLight)
    );
    assert_eq!(theme_from_name("nord"), Some(ThemeId::Nord));
    assert_eq!(theme_from_name("gruvbox"), Some(ThemeId::Gruvbox));
    assert_eq!(theme_from_name("nonexistent"), None);
    assert_eq!(theme_from_name(""), None);
}

#[test]
fn test_theme_from_name_case_insensitive() {
    assert_eq!(theme_from_name("AMBER"), Some(ThemeId::Amber));
    assert_eq!(theme_from_name("Dracula"), Some(ThemeId::Dracula));
    assert_eq!(theme_from_name("NORD"), Some(ThemeId::Nord));
}

#[test]
fn test_theme_id_from_u8_roundtrip() {
    for i in 0u8..10 {
        let id = ThemeId::from_u8(i);
        assert_eq!(id.to_u8(), i);
    }
}

#[test]
fn test_theme_id_from_u8_overflow() {
    assert_eq!(ThemeId::from_u8(10), ThemeId::Amber);
    assert_eq!(ThemeId::from_u8(255), ThemeId::Amber);
}

#[test]
fn test_available_themes_count() {
    let themes = available_themes();
    assert_eq!(themes.len(), 10);
}

#[test]
fn test_current_theme_after_set() {
    set_theme(ThemeId::Dracula);
    let colors = current_theme();
    let dracula = theme_colors(ThemeId::Dracula);
    assert_eq!(colors.primary.r, dracula.primary.r);
    assert_eq!(colors.primary.g, dracula.primary.g);
    assert_eq!(colors.primary.b, dracula.primary.b);
}

#[test]
fn test_theme_colors_all_variants() {
    // Just ensure each variant returns valid colors without panicking
    let _ = theme_colors(ThemeId::Amber);
    let _ = theme_colors(ThemeId::Ocean);
    let _ = theme_colors(ThemeId::Minimal);
    let _ = theme_colors(ThemeId::HighContrast);
    let _ = theme_colors(ThemeId::Dracula);
    let _ = theme_colors(ThemeId::Monokai);
    let _ = theme_colors(ThemeId::SolarizedDark);
    let _ = theme_colors(ThemeId::SolarizedLight);
    let _ = theme_colors(ThemeId::Nord);
    let _ = theme_colors(ThemeId::Gruvbox);
}

#[test]
fn test_theme_colors_monokai_values() {
    let monokai = theme_colors(ThemeId::Monokai);
    assert_eq!(monokai.primary.r, 102);
    assert_eq!(monokai.primary.g, 217);
    assert_eq!(monokai.primary.b, 239);
}

#[test]
fn test_theme_colors_nord_values() {
    let nord = theme_colors(ThemeId::Nord);
    assert_eq!(nord.primary.r, 136);
    assert_eq!(nord.primary.g, 192);
    assert_eq!(nord.primary.b, 208);
}

#[test]
fn test_theme_colors_gruvbox_values() {
    let gruvbox = theme_colors(ThemeId::Gruvbox);
    assert_eq!(gruvbox.primary.r, 131);
    assert_eq!(gruvbox.primary.g, 165);
    assert_eq!(gruvbox.primary.b, 152);
}

#[test]
fn test_theme_colors_solarized_dark_values() {
    let sol = theme_colors(ThemeId::SolarizedDark);
    assert_eq!(sol.primary.r, 38);
    assert_eq!(sol.primary.g, 139);
    assert_eq!(sol.primary.b, 210);
}

#[test]
fn test_theme_colors_solarized_light_values() {
    let sol = theme_colors(ThemeId::SolarizedLight);
    assert_eq!(sol.primary.r, 38);
}

#[test]
fn test_theme_colors_high_contrast_values() {
    let hc = theme_colors(ThemeId::HighContrast);
    assert_eq!(hc.primary.r, 255);
    assert_eq!(hc.primary.g, 255);
    assert_eq!(hc.primary.b, 255);
    assert_eq!(hc.error.r, 255);
    assert_eq!(hc.error.g, 0);
    assert_eq!(hc.error.b, 0);
}

#[test]
fn test_theme_id_default() {
    let default_id = ThemeId::default();
    assert_eq!(default_id, ThemeId::Amber);
}
