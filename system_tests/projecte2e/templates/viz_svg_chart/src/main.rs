use viz_svg_chart::{BarChart, DataPoint};

fn main() {
    let data = vec![
        DataPoint { label: "Rust".into(), value: 95.0 },
        DataPoint { label: "Python".into(), value: 88.0 },
        DataPoint { label: "Go".into(), value: 72.0 },
        DataPoint { label: "Java".into(), value: 65.0 },
        DataPoint { label: "C++".into(), value: 78.0 },
    ];

    let chart = BarChart::new("Language Popularity", data);
    let svg = chart.render();

    std::fs::write("output.svg", &svg).expect("Failed to write output.svg");
    println!("SVG written to output.svg ({} bytes)", svg.len());
}
