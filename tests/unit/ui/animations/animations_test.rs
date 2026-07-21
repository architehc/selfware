use super::*;

#[test]
fn test_animator_creation() {
    let animator = Animator::new(10);
    assert_eq!(animator.frame_rate(), 10);
    assert_eq!(animator.tick(), 0);
}

#[test]
fn test_animator_default() {
    let animator = Animator::default();
    assert_eq!(animator.frame_rate(), 10);
}

#[test]
fn test_animator_reset() {
    let mut animator = Animator::new(10);
    animator.tick = 100;
    animator.reset();
    assert_eq!(animator.tick(), 0);
}

#[test]
fn test_animator_set_frame_rate() {
    let mut animator = Animator::new(10);
    animator.set_frame_rate(30);
    assert_eq!(animator.frame_rate(), 30);

    // Should not allow 0
    animator.set_frame_rate(0);
    assert_eq!(animator.frame_rate(), 1);
}

#[test]
fn test_spinner_animation_dots() {
    let spinner = SpinnerAnimation::dots();
    assert_eq!(spinner.frame(0), "⠋");
    assert_eq!(spinner.frame(1), "⠙");
    assert_eq!(spinner.frame(10), "⠋"); // Wraps
}

#[test]
fn test_spinner_animation_with_message() {
    let spinner = SpinnerAnimation::dots().with_message("Loading");
    let frame = spinner.frame(0);
    assert!(frame.contains("Loading"));
    assert!(frame.contains("⠋"));
}

#[test]
fn test_spinner_variants() {
    let _braille = SpinnerAnimation::braille();
    let _arrows = SpinnerAnimation::arrows();
    let _bounce = SpinnerAnimation::bounce();
    let _garden = SpinnerAnimation::garden();
    let _line = SpinnerAnimation::line();
    let _arc = SpinnerAnimation::arc();
}

#[test]
fn test_wave_animation() {
    let wave = WaveAnimation::new(10);
    let frame = wave.frame(0);
    assert_eq!(frame.chars().count(), 10);
}

#[test]
fn test_wave_animation_changes() {
    let wave = WaveAnimation::new(5);
    let frame1 = wave.frame(0);
    let frame2 = wave.frame(1);
    assert_ne!(frame1, frame2);
}

#[test]
fn test_progress_animation() {
    let mut progress = ProgressAnimation::new(20);
    progress.set_progress(0.5);
    assert_eq!(progress.progress(), 0.5);

    let frame = progress.frame(0);
    assert!(frame.contains("50%"));
}

#[test]
fn test_progress_animation_clamping() {
    let mut progress = ProgressAnimation::new(20);
    progress.set_progress(1.5);
    assert_eq!(progress.progress(), 1.0);

    progress.set_progress(-0.5);
    assert_eq!(progress.progress(), 0.0);
}

#[test]
fn test_progress_animation_complete() {
    let mut progress = ProgressAnimation::new(10);
    progress.set_progress(0.5);
    assert!(!progress.is_complete(0));

    progress.set_progress(1.0);
    assert!(progress.is_complete(0));
}

#[test]
fn test_progress_animation_variants() {
    let blocks = ProgressAnimation::new(10).with_blocks();
    let _ = blocks.frame(0);

    let ascii = ProgressAnimation::new(10).with_ascii();
    let _ = ascii.frame(0);

    let no_percent = ProgressAnimation::new(10).hide_percentage();
    let frame = no_percent.frame(0);
    assert!(!frame.contains('%'));
}

#[test]
fn test_progress_worm_animation() {
    let worm = ProgressWormAnimation::new(20);
    let frame1 = worm.frame(0);
    let frame2 = worm.frame(1);
    assert_ne!(frame1, frame2);
}

#[test]
fn test_progress_worm_with_length() {
    let worm = ProgressWormAnimation::new(20).with_length(5);
    assert_eq!(worm.worm_length, 5);
}

#[test]
fn test_pulse_animation() {
    let pulse = PulseAnimation::new();
    let frame = pulse.frame(0);
    assert!(!frame.is_empty());
}

#[test]
fn test_pulse_default() {
    let pulse = PulseAnimation::default();
    assert!(!pulse.chars.is_empty());
}

#[test]
fn test_sparkle_animation() {
    let sparkle = SparkleAnimation::new(20);
    let frame = sparkle.frame(0);
    assert_eq!(frame.chars().count(), 20);
}

#[test]
fn test_sparkle_density() {
    let sparkle = SparkleAnimation::new(100).with_density(0.5);
    assert_eq!(sparkle.density, 0.5);
}

#[test]
fn test_fire_animation() {
    let fire = FireAnimation::new(10, 5);
    let frame = fire.frame(0);
    let lines: Vec<&str> = frame.lines().collect();
    assert_eq!(lines.len(), 5);
}

#[test]
fn test_matrix_rain_animation() {
    let matrix = MatrixRainAnimation::new(10, 5);
    let frame = matrix.frame(0);
    let lines: Vec<&str> = frame.lines().collect();
    assert_eq!(lines.len(), 5);
}

#[test]
fn test_color_rgb() {
    let color = Color::rgb(255, 128, 64);
    assert_eq!(color.r, 255);
    assert_eq!(color.g, 128);
    assert_eq!(color.b, 64);
}

#[test]
fn test_color_codes() {
    let color = Color::rgb(255, 0, 0);
    let fg = color.fg_code();
    assert!(fg.contains("255"));
    assert!(fg.contains("38;2"));

    let bg = color.bg_code();
    assert!(bg.contains("255"));
    assert!(bg.contains("48;2"));
}

#[test]
fn test_color_blend() {
    let c1 = Color::rgb(0, 0, 0);
    let c2 = Color::rgb(255, 255, 255);

    let mid = Color::blend(c1, c2, 0.5);
    assert!(mid.r > 100 && mid.r < 150);
    assert!(mid.g > 100 && mid.g < 150);
    assert!(mid.b > 100 && mid.b < 150);
}

#[test]
fn test_color_blend_edges() {
    let c1 = Color::rgb(100, 100, 100);
    let c2 = Color::rgb(200, 200, 200);

    let start = Color::blend(c1, c2, 0.0);
    assert_eq!(start.r, c1.r);

    let end = Color::blend(c1, c2, 1.0);
    assert_eq!(end.r, c2.r);
}

#[test]
fn test_cycle_mode_default() {
    assert_eq!(CycleMode::default(), CycleMode::Loop);
}

#[test]
fn test_color_cycler() {
    let cycler = ColorCycler::from_palette(palettes::SUNSET);
    let color = cycler.color_at(0);
    assert_eq!(color.r, 212); // AMBER
}

#[test]
fn test_color_cycler_loop() {
    let cycler = ColorCycler::new(vec![Color::rgb(255, 0, 0), Color::rgb(0, 255, 0)]);
    assert_eq!(cycler.color_at(0).r, 255);
    assert_eq!(cycler.color_at(1).g, 255);
    assert_eq!(cycler.color_at(2).r, 255); // Loops
}

#[test]
fn test_color_cycler_bounce() {
    let cycler = ColorCycler::new(vec![
        Color::rgb(255, 0, 0),
        Color::rgb(0, 255, 0),
        Color::rgb(0, 0, 255),
    ])
    .with_mode(CycleMode::Bounce);

    // Forward
    assert_eq!(cycler.color_at(0).r, 255);
    assert_eq!(cycler.color_at(1).g, 255);
    assert_eq!(cycler.color_at(2).b, 255);
    // Backward
    assert_eq!(cycler.color_at(3).g, 255);
    assert_eq!(cycler.color_at(4).r, 255);
}

#[test]
fn test_color_cycler_speed() {
    let cycler = ColorCycler::new(vec![Color::rgb(255, 0, 0), Color::rgb(0, 255, 0)]).with_speed(2);

    assert_eq!(cycler.color_at(0).r, 255);
    assert_eq!(cycler.color_at(1).r, 255); // Still first color
    assert_eq!(cycler.color_at(2).g, 255); // Now second
}

#[test]
fn test_color_cycler_smooth() {
    let cycler = ColorCycler::new(vec![Color::rgb(0, 0, 0), Color::rgb(255, 255, 255)]);

    let mid = cycler.smooth_color_at(5, 10);
    assert!(mid.r > 100 && mid.r < 150);
}

#[test]
fn test_color_cycler_empty() {
    let cycler = ColorCycler::new(vec![]);
    let color = cycler.color_at(0);
    assert_eq!(color.r, 255); // Default white
}

#[test]
fn test_animated_status() {
    let status = AnimatedStatus::new("Processing");
    let frame = status.frame(0);
    assert!(frame.contains("Processing"));
    assert!(frame.contains("s]")); // Elapsed time
}

#[test]
fn test_animated_status_hide_elapsed() {
    let status = AnimatedStatus::new("Test").hide_elapsed();
    let frame = status.frame(0);
    assert!(!frame.contains('['));
}

#[test]
fn test_animated_status_with_spinner() {
    let status = AnimatedStatus::new("Test").with_spinner(SpinnerAnimation::garden());
    let frame = status.frame(0);
    assert!(frame.contains("🌱"));
}

#[test]
fn test_animated_status_set_message() {
    let mut status = AnimatedStatus::new("Old");
    status.set_message("New");
    let frame = status.frame(0);
    assert!(frame.contains("New"));
}

#[test]
fn test_format_duration() {
    assert_eq!(format_duration(Duration::from_millis(500)), "500ms");
    assert_eq!(format_duration(Duration::from_secs(5)), "5.000s");
    assert_eq!(format_duration(Duration::from_secs(65)), "1m 05s");
    assert_eq!(format_duration(Duration::from_secs(3665)), "1h 01m 05s");
}

#[test]
fn test_simple_progress_bar() {
    let bar = simple_progress_bar(0.5, 10);
    assert!(bar.contains("50%"));
    assert!(bar.contains('█'));
    assert!(bar.contains('░'));
}

#[test]
fn test_gradient_progress_bar() {
    let start = Color::rgb(255, 0, 0);
    let end = Color::rgb(0, 255, 0);
    let bar = gradient_progress_bar(0.5, 10, start, end);
    assert!(bar.contains("50%"));
    assert!(bar.contains("\x1b[")); // ANSI codes
}

#[test]
fn test_animation_is_complete_default() {
    let spinner = SpinnerAnimation::dots();
    assert!(!spinner.is_complete(100));
}

#[test]
fn test_animation_frame_rate_default() {
    let spinner = SpinnerAnimation::dots();
    assert_eq!(spinner.frame_rate(), 10);
}

#[test]
fn test_spinner_presets_exist() {
    assert!(!SPINNER_DOTS.is_empty());
    assert!(!SPINNER_BRAILLE.is_empty());
    assert!(!SPINNER_ARROWS.is_empty());
    assert!(!SPINNER_BOUNCE.is_empty());
    assert!(!SPINNER_CLOCK.is_empty());
    assert!(!SPINNER_GARDEN.is_empty());
    assert!(!SPINNER_MOON.is_empty());
    assert!(!SPINNER_BOX.is_empty());
    assert!(!SPINNER_LINE.is_empty());
    assert!(!SPINNER_ARC.is_empty());
}

#[test]
fn test_progress_presets_exist() {
    assert!(!PROGRESS_BLOCKS.is_empty());
    assert!(!PROGRESS_SHADES.is_empty());
    assert!(!PROGRESS_SIMPLE.is_empty());
    assert!(!PROGRESS_ASCII.is_empty());
    assert!(!PROGRESS_DOTS.is_empty());
}

#[test]
fn test_wave_presets_exist() {
    assert!(!WAVE_BARS.is_empty());
    assert!(!WAVE_SINE.is_empty());
    assert!(!WAVE_WATER.is_empty());
}

#[test]
fn test_palettes_exist() {
    assert!(!palettes::SUNSET.is_empty());
    assert!(!palettes::OCEAN.is_empty());
    assert!(!palettes::FIRE.is_empty());
    assert!(!palettes::ICE.is_empty());
    assert!(!palettes::RAINBOW.is_empty());
}

#[test]
fn test_wave_with_custom_chars() {
    let wave = WaveAnimation::new(5).with_chars(WAVE_SINE);
    let frame = wave.frame(0);
    assert_eq!(frame.chars().count(), 5);
}

#[test]
fn test_pulse_with_custom_chars() {
    let pulse = PulseAnimation::new().with_chars(vec!["A", "B", "C"]);
    assert_eq!(pulse.frame(0), "A");
    assert_eq!(pulse.frame(1), "B");
    assert_eq!(pulse.frame(2), "C");
}
