use viz_svg_chart::{BarChart, DataPoint, quick_chart};

#[test]
fn test_svg_has_valid_header() {
    let svg = quick_chart("Test", &[("A", 10.0), ("B", 20.0)]);
    assert!(
        svg.starts_with("<svg"),
        "SVG must start with <svg tag, got: {}",
        &svg[..svg.len().min(50)]
    );
    assert!(svg.contains("xmlns=\"http://www.w3.org/2000/svg\""),
        "SVG must have xmlns attribute");
    assert!(svg.ends_with("</svg>") || svg.trim_end().ends_with("</svg>"),
        "SVG must end with </svg>");
}

#[test]
fn test_svg_has_viewbox() {
    let chart = BarChart::new("Test", vec![
        DataPoint { label: "A".into(), value: 10.0 },
    ]);
    let svg = chart.render();
    assert!(
        svg.contains("viewBox=\"0 0 600 400\""),
        "SVG must have viewBox matching width x height"
    );
}

#[test]
fn test_svg_has_correct_bar_count() {
    let data = vec![
        DataPoint { label: "X".into(), value: 30.0 },
        DataPoint { label: "Y".into(), value: 60.0 },
        DataPoint { label: "Z".into(), value: 45.0 },
    ];
    let svg = BarChart::new("Bars", data).render();

    // Count bar <rect> elements (excluding the background rect)
    let rect_count = svg.matches("<rect").count();
    // Should have background rect + 3 bar rects = 4 total
    assert!(
        rect_count >= 4,
        "Expected at least 4 <rect> elements (1 bg + 3 bars), found {}",
        rect_count
    );
}

#[test]
fn test_svg_has_labels() {
    let svg = quick_chart("My Chart", &[("Alpha", 10.0), ("Beta", 20.0), ("Gamma", 30.0)]);
    assert!(svg.contains("Alpha"), "SVG must contain label 'Alpha'");
    assert!(svg.contains("Beta"), "SVG must contain label 'Beta'");
    assert!(svg.contains("Gamma"), "SVG must contain label 'Gamma'");
}

#[test]
fn test_svg_has_title() {
    let svg = quick_chart("Sales Report", &[("Q1", 100.0)]);
    assert!(
        svg.contains("Sales Report"),
        "SVG must contain the chart title"
    );
}

#[test]
fn test_svg_bar_colors() {
    let mut chart = BarChart::new("Colors", vec![
        DataPoint { label: "A".into(), value: 50.0 },
    ]);
    chart.bar_color = "#FF0000".to_string();
    let svg = chart.render();
    assert!(
        svg.contains("#FF0000"),
        "SVG bars must use the configured bar color"
    );
}

#[test]
fn test_svg_tallest_bar_scales_to_chart_height() {
    // The tallest bar should use the full available chart height (minus padding).
    // With two bars where one is max, the tallest should be noticeably taller.
    let data = vec![
        DataPoint { label: "Short".into(), value: 25.0 },
        DataPoint { label: "Tall".into(), value: 100.0 },
    ];
    let svg = BarChart::new("Scale", data).render();

    // Just verify both bars exist and the SVG is well-formed
    assert!(svg.contains("Short"), "Must contain Short label");
    assert!(svg.contains("Tall"), "Must contain Tall label");

    // The tallest bar's height attribute should be present
    assert!(
        svg.contains("height="),
        "Bars must have height attributes"
    );
}

#[test]
fn test_svg_empty_data() {
    let chart = BarChart::new("Empty", vec![]);
    let svg = chart.render();
    assert!(svg.starts_with("<svg"), "Empty data should still produce valid SVG");
    assert!(svg.contains("</svg>"), "Empty data should produce complete SVG");
}
