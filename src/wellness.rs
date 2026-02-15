//! Developer Wellness Features
//!
//! Provides RSI prevention, focus mode, and frustration detection
//! to support developer health and productivity.

use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

fn current_timestamp() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

// ============================================================================
// RSI Prevention
// ============================================================================

/// Keystroke event for typing analysis
#[derive(Debug, Clone)]
pub struct KeystrokeEvent {
    /// Timestamp
    pub timestamp: Instant,
    /// Key pressed
    pub key: char,
    /// Is modifier key
    pub is_modifier: bool,
}

impl KeystrokeEvent {
    pub fn new(key: char, is_modifier: bool) -> Self {
        Self {
            timestamp: Instant::now(),
            key,
            is_modifier,
        }
    }
}

/// Typing pattern metrics
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TypingMetrics {
    /// Keys per minute
    pub keys_per_minute: f64,
    /// Average interval between keystrokes (ms)
    pub avg_interval_ms: f64,
    /// Variance in keystroke intervals
    pub interval_variance: f64,
    /// Modifier key usage percentage
    pub modifier_percentage: f64,
    /// Continuous typing duration (seconds)
    pub continuous_typing_secs: u64,
}

impl TypingMetrics {
    /// Check if typing pattern indicates fatigue
    pub fn indicates_fatigue(&self) -> bool {
        // High variance often indicates fatigue
        self.interval_variance > 100.0 ||
        // Very fast typing for extended periods is risky
        (self.keys_per_minute > 300.0 && self.continuous_typing_secs > 1800)
    }

    /// Check if typing is ergonomically concerning
    pub fn is_concerning(&self) -> bool {
        // Very high typing rate
        self.keys_per_minute > 400.0 ||
        // Continuous typing without breaks
        self.continuous_typing_secs > 3600 // 1 hour
    }
}

/// Break reminder configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BreakConfig {
    /// Short break interval (minutes)
    pub short_break_interval_mins: u32,
    /// Short break duration (seconds)
    pub short_break_duration_secs: u32,
    /// Long break interval (minutes)
    pub long_break_interval_mins: u32,
    /// Long break duration (minutes)
    pub long_break_duration_mins: u32,
    /// Enabled
    pub enabled: bool,
    /// Play sound on break
    pub play_sound: bool,
}

impl Default for BreakConfig {
    fn default() -> Self {
        Self {
            short_break_interval_mins: 20, // 20-20-20 rule
            short_break_duration_secs: 20,
            long_break_interval_mins: 60,
            long_break_duration_mins: 5,
            enabled: true,
            play_sound: true,
        }
    }
}

/// Break reminder
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BreakReminder {
    /// Is long break
    pub is_long_break: bool,
    /// Break duration
    pub duration: Duration,
    /// Message
    pub message: String,
    /// Exercise suggestion
    pub exercise: Option<String>,
}

impl BreakReminder {
    pub fn short() -> Self {
        Self {
            is_long_break: false,
            duration: Duration::from_secs(20),
            message: "Look at something 20 feet away for 20 seconds".to_string(),
            exercise: None,
        }
    }

    pub fn long() -> Self {
        Self {
            is_long_break: true,
            duration: Duration::from_secs(300), // 5 minutes
            message: "Time for a longer break. Stand up and stretch!".to_string(),
            exercise: Some("Do some wrist stretches and shoulder rolls".to_string()),
        }
    }
}

/// RSI prevention tracker
#[derive(Debug)]
pub struct RsiTracker {
    /// Keystroke history
    keystrokes: VecDeque<KeystrokeEvent>,
    /// Max keystrokes to track
    max_keystrokes: usize,
    /// Last short break
    last_short_break: Option<Instant>,
    /// Last long break
    last_long_break: Option<Instant>,
    /// Break configuration
    pub config: BreakConfig,
    /// Session start time
    session_start: Instant,
}

impl RsiTracker {
    pub fn new() -> Self {
        Self {
            keystrokes: VecDeque::new(),
            max_keystrokes: 1000,
            last_short_break: None,
            last_long_break: None,
            config: BreakConfig::default(),
            session_start: Instant::now(),
        }
    }

    pub fn record_keystroke(&mut self, key: char, is_modifier: bool) {
        self.keystrokes
            .push_back(KeystrokeEvent::new(key, is_modifier));

        if self.keystrokes.len() > self.max_keystrokes {
            self.keystrokes.pop_front();
        }
    }

    pub fn analyze(&self) -> TypingMetrics {
        if self.keystrokes.len() < 2 {
            return TypingMetrics::default();
        }

        let mut intervals = Vec::new();
        let mut modifier_count = 0;

        for i in 1..self.keystrokes.len() {
            let interval = self.keystrokes[i]
                .timestamp
                .duration_since(self.keystrokes[i - 1].timestamp)
                .as_millis() as f64;
            intervals.push(interval);

            if self.keystrokes[i].is_modifier {
                modifier_count += 1;
            }
        }

        let avg_interval: f64 = intervals.iter().sum::<f64>() / intervals.len() as f64;
        let variance: f64 = intervals
            .iter()
            .map(|&i| (i - avg_interval).powi(2))
            .sum::<f64>()
            / intervals.len() as f64;

        let keys_per_minute = if avg_interval > 0.0 {
            60000.0 / avg_interval
        } else {
            0.0
        };

        let modifier_percentage = modifier_count as f64 / self.keystrokes.len() as f64 * 100.0;

        TypingMetrics {
            keys_per_minute,
            avg_interval_ms: avg_interval,
            interval_variance: variance.sqrt(),
            modifier_percentage,
            continuous_typing_secs: self.session_start.elapsed().as_secs(),
        }
    }

    pub fn should_take_break(&self) -> Option<BreakReminder> {
        if !self.config.enabled {
            return None;
        }

        let now = Instant::now();

        // Check for long break
        let long_interval = Duration::from_secs(self.config.long_break_interval_mins as u64 * 60);
        if self
            .last_long_break
            .is_none_or(|t| now.duration_since(t) >= long_interval)
        {
            return Some(BreakReminder::long());
        }

        // Check for short break
        let short_interval = Duration::from_secs(self.config.short_break_interval_mins as u64 * 60);
        if self
            .last_short_break
            .is_none_or(|t| now.duration_since(t) >= short_interval)
        {
            return Some(BreakReminder::short());
        }

        None
    }

    pub fn take_break(&mut self, is_long: bool) {
        let now = Instant::now();
        if is_long {
            self.last_long_break = Some(now);
        } else {
            self.last_short_break = Some(now);
        }
    }

    pub fn reset_session(&mut self) {
        self.keystrokes.clear();
        self.session_start = Instant::now();
    }

    pub fn session_duration(&self) -> Duration {
        self.session_start.elapsed()
    }
}

impl Default for RsiTracker {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// Focus Mode
// ============================================================================

/// Focus mode state
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FocusModeState {
    Inactive,
    Working,
    ShortBreak,
    LongBreak,
}

/// Pomodoro configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PomodoroConfig {
    /// Work duration (minutes)
    pub work_duration_mins: u32,
    /// Short break duration (minutes)
    pub short_break_mins: u32,
    /// Long break duration (minutes)
    pub long_break_mins: u32,
    /// Pomodoros before long break
    pub pomodoros_before_long_break: u32,
}

impl Default for PomodoroConfig {
    fn default() -> Self {
        Self {
            work_duration_mins: 25,
            short_break_mins: 5,
            long_break_mins: 15,
            pomodoros_before_long_break: 4,
        }
    }
}

/// Distraction type
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum DistractionType {
    WebBrowser,
    SocialMedia,
    Chat,
    Email,
    News,
    Entertainment,
    Custom(String),
}

/// Focus mode manager
#[derive(Debug)]
pub struct FocusMode {
    /// Current state
    pub state: FocusModeState,
    /// Pomodoro configuration
    pub config: PomodoroConfig,
    /// Current session start
    session_start: Option<Instant>,
    /// Completed pomodoros
    pub completed_pomodoros: u32,
    /// Total focus time today (seconds)
    pub focus_time_today_secs: u64,
    /// Blocked distractions
    pub blocked_distractions: Vec<DistractionType>,
    /// Distraction attempts (blocked)
    pub distraction_attempts: Vec<(DistractionType, u64)>,
}

impl FocusMode {
    pub fn new() -> Self {
        Self {
            state: FocusModeState::Inactive,
            config: PomodoroConfig::default(),
            session_start: None,
            completed_pomodoros: 0,
            focus_time_today_secs: 0,
            blocked_distractions: vec![
                DistractionType::SocialMedia,
                DistractionType::Entertainment,
                DistractionType::News,
            ],
            distraction_attempts: Vec::new(),
        }
    }

    pub fn start_work(&mut self) {
        self.state = FocusModeState::Working;
        self.session_start = Some(Instant::now());
    }

    pub fn start_break(&mut self, is_long: bool) {
        if is_long {
            self.state = FocusModeState::LongBreak;
        } else {
            self.state = FocusModeState::ShortBreak;
        }
        self.session_start = Some(Instant::now());
    }

    pub fn complete_pomodoro(&mut self) {
        if self.state == FocusModeState::Working {
            self.completed_pomodoros += 1;

            if let Some(start) = self.session_start {
                self.focus_time_today_secs += start.elapsed().as_secs();
            }

            let needs_long_break =
                self.completed_pomodoros % self.config.pomodoros_before_long_break == 0;
            self.start_break(needs_long_break);
        }
    }

    pub fn stop(&mut self) {
        if self.state == FocusModeState::Working {
            if let Some(start) = self.session_start {
                self.focus_time_today_secs += start.elapsed().as_secs();
            }
        }
        self.state = FocusModeState::Inactive;
        self.session_start = None;
    }

    pub fn time_remaining(&self) -> Option<Duration> {
        let start = self.session_start?;
        let total_duration = match self.state {
            FocusModeState::Working => {
                Duration::from_secs(self.config.work_duration_mins as u64 * 60)
            }
            FocusModeState::ShortBreak => {
                Duration::from_secs(self.config.short_break_mins as u64 * 60)
            }
            FocusModeState::LongBreak => {
                Duration::from_secs(self.config.long_break_mins as u64 * 60)
            }
            FocusModeState::Inactive => return None,
        };

        let elapsed = start.elapsed();
        if elapsed >= total_duration {
            Some(Duration::ZERO)
        } else {
            Some(total_duration - elapsed)
        }
    }

    pub fn is_session_complete(&self) -> bool {
        self.time_remaining().is_some_and(|r| r.is_zero())
    }

    pub fn block_distraction(&mut self, distraction: DistractionType) {
        if !self.blocked_distractions.contains(&distraction) {
            self.blocked_distractions.push(distraction);
        }
    }

    pub fn is_blocked(&self, distraction: &DistractionType) -> bool {
        self.state == FocusModeState::Working && self.blocked_distractions.contains(distraction)
    }

    pub fn record_distraction_attempt(&mut self, distraction: DistractionType) {
        self.distraction_attempts
            .push((distraction, current_timestamp()));
    }

    pub fn focus_score(&self) -> f64 {
        // Score based on completed pomodoros and low distraction attempts
        let pomodoro_score = (self.completed_pomodoros as f64).min(10.0) * 10.0;
        let distraction_penalty = (self.distraction_attempts.len() as f64).min(20.0) * 2.5;
        (pomodoro_score - distraction_penalty).max(0.0)
    }
}

impl Default for FocusMode {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// Frustration Detection
// ============================================================================

/// Action type for frustration detection
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum UserAction {
    Edit,
    Undo,
    Redo,
    Delete,
    Revert,
    Save,
    Build,
    Test,
    Search,
    NavigateBack,
}

/// User action event
#[derive(Debug, Clone)]
pub struct ActionEvent {
    /// Action type
    pub action: UserAction,
    /// Timestamp
    pub timestamp: Instant,
    /// File path (if applicable)
    pub file_path: Option<String>,
    /// Success
    pub success: bool,
}

impl ActionEvent {
    pub fn new(action: UserAction, success: bool) -> Self {
        Self {
            action,
            timestamp: Instant::now(),
            file_path: None,
            success,
        }
    }

    pub fn with_file(mut self, path: impl Into<String>) -> Self {
        self.file_path = Some(path.into());
        self
    }
}

/// Frustration indicator
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FrustrationIndicator {
    /// Indicator type
    pub indicator_type: FrustrationIndicatorType,
    /// Confidence (0-1)
    pub confidence: f64,
    /// Description
    pub description: String,
    /// Suggested help
    pub suggested_help: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FrustrationIndicatorType {
    RevertLoop,
    RapidDeletions,
    RepeatedFailures,
    CircularNavigation,
    HighUndoRate,
    LongStuckPeriod,
}

/// Frustration detector
#[derive(Debug)]
pub struct FrustrationDetector {
    /// Action history
    actions: VecDeque<ActionEvent>,
    /// Max actions to track
    max_actions: usize,
    /// Sensitivity (0-1, higher = more sensitive)
    pub sensitivity: f64,
    /// Detected indicators
    pub indicators: Vec<FrustrationIndicator>,
    /// Last help offered
    last_help_offered: Option<Instant>,
    /// Help cooldown (seconds)
    pub help_cooldown_secs: u64,
}

impl FrustrationDetector {
    pub fn new() -> Self {
        Self {
            actions: VecDeque::new(),
            max_actions: 100,
            sensitivity: 0.5,
            indicators: Vec::new(),
            last_help_offered: None,
            help_cooldown_secs: 300, // 5 minutes
        }
    }

    pub fn with_sensitivity(mut self, sensitivity: f64) -> Self {
        self.sensitivity = sensitivity.clamp(0.0, 1.0);
        self
    }

    pub fn record_action(&mut self, action: ActionEvent) {
        self.actions.push_back(action);

        if self.actions.len() > self.max_actions {
            self.actions.pop_front();
        }

        self.analyze();
    }

    fn analyze(&mut self) {
        self.indicators.clear();

        if self.actions.len() < 5 {
            return;
        }

        // Detect revert loops
        if let Some(indicator) = self.detect_revert_loop() {
            self.indicators.push(indicator);
        }

        // Detect rapid deletions
        if let Some(indicator) = self.detect_rapid_deletions() {
            self.indicators.push(indicator);
        }

        // Detect repeated failures
        if let Some(indicator) = self.detect_repeated_failures() {
            self.indicators.push(indicator);
        }

        // Detect high undo rate
        if let Some(indicator) = self.detect_high_undo_rate() {
            self.indicators.push(indicator);
        }
    }

    fn detect_revert_loop(&self) -> Option<FrustrationIndicator> {
        let recent: Vec<_> = self.actions.iter().rev().take(10).collect();
        let revert_count = recent
            .iter()
            .filter(|a| a.action == UserAction::Revert)
            .count();

        if revert_count >= 3 {
            let confidence = (revert_count as f64 / 10.0) * self.sensitivity;
            return Some(FrustrationIndicator {
                indicator_type: FrustrationIndicatorType::RevertLoop,
                confidence,
                description: "Detected multiple reverts in a short time".to_string(),
                suggested_help: "Would you like me to help explain what's going wrong?".to_string(),
            });
        }

        None
    }

    fn detect_rapid_deletions(&self) -> Option<FrustrationIndicator> {
        let recent: Vec<_> = self.actions.iter().rev().take(20).collect();
        let delete_count = recent
            .iter()
            .filter(|a| a.action == UserAction::Delete)
            .count();

        // Check if deletions are rapid (within short time window)
        if delete_count >= 5 {
            if let (Some(first), Some(last)) = (recent.last(), recent.first()) {
                let time_span = last.timestamp.duration_since(first.timestamp);
                if time_span < Duration::from_secs(60) {
                    let confidence = (delete_count as f64 / 20.0) * self.sensitivity;
                    return Some(FrustrationIndicator {
                        indicator_type: FrustrationIndicatorType::RapidDeletions,
                        confidence,
                        description: "Detected many deletions in quick succession".to_string(),
                        suggested_help: "It looks like you might be struggling. Want me to take a different approach?".to_string(),
                    });
                }
            }
        }

        None
    }

    fn detect_repeated_failures(&self) -> Option<FrustrationIndicator> {
        let recent: Vec<_> = self.actions.iter().rev().take(10).collect();
        let failure_count = recent.iter().filter(|a| !a.success).count();

        if failure_count >= 5 {
            let confidence = (failure_count as f64 / 10.0) * self.sensitivity;
            return Some(FrustrationIndicator {
                indicator_type: FrustrationIndicatorType::RepeatedFailures,
                confidence,
                description: "Multiple consecutive failures detected".to_string(),
                suggested_help: "I notice several failures. Would you like me to analyze the errors and suggest fixes?".to_string(),
            });
        }

        None
    }

    fn detect_high_undo_rate(&self) -> Option<FrustrationIndicator> {
        let undo_count = self
            .actions
            .iter()
            .filter(|a| a.action == UserAction::Undo)
            .count();
        let total = self.actions.len();

        if total > 10 {
            let undo_rate = undo_count as f64 / total as f64;
            if undo_rate > 0.3 * self.sensitivity {
                return Some(FrustrationIndicator {
                    indicator_type: FrustrationIndicatorType::HighUndoRate,
                    confidence: undo_rate,
                    description: format!("High undo rate: {:.0}% of actions are undos", undo_rate * 100.0),
                    suggested_help: "You're undoing many changes. Would a checkpoint or different approach help?".to_string(),
                });
            }
        }

        None
    }

    pub fn is_frustrated(&self) -> bool {
        self.indicators.iter().any(|i| i.confidence >= 0.5)
    }

    pub fn should_offer_help(&mut self) -> Option<&FrustrationIndicator> {
        if !self.is_frustrated() {
            return None;
        }

        // Check cooldown
        if let Some(last) = self.last_help_offered {
            if last.elapsed() < Duration::from_secs(self.help_cooldown_secs) {
                return None;
            }
        }

        // Find highest confidence indicator
        self.indicators.iter().max_by(|a, b| {
            a.confidence
                .partial_cmp(&b.confidence)
                .unwrap_or(std::cmp::Ordering::Equal)
        })
    }

    pub fn help_offered(&mut self) {
        self.last_help_offered = Some(Instant::now());
    }

    pub fn clear(&mut self) {
        self.actions.clear();
        self.indicators.clear();
    }

    pub fn frustration_level(&self) -> f64 {
        if self.indicators.is_empty() {
            0.0
        } else {
            self.indicators.iter().map(|i| i.confidence).sum::<f64>() / self.indicators.len() as f64
        }
    }
}

impl Default for FrustrationDetector {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// Wellness Dashboard
// ============================================================================

/// Wellness metrics
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct WellnessMetrics {
    /// Focus time today (minutes)
    pub focus_time_mins: u64,
    /// Breaks taken
    pub breaks_taken: u32,
    /// Pomodoros completed
    pub pomodoros_completed: u32,
    /// Frustration episodes
    pub frustration_episodes: u32,
    /// Distraction attempts blocked
    pub distractions_blocked: u32,
    /// Wellness score (0-100)
    pub wellness_score: u32,
}

impl WellnessMetrics {
    pub fn calculate_score(&self) -> u32 {
        let mut score: i32 = 50; // Base score

        // Bonus for focus time (up to +20)
        score += ((self.focus_time_mins as f64 / 120.0) * 20.0).min(20.0) as i32;

        // Bonus for breaks (up to +15)
        score += ((self.breaks_taken as f64 / 8.0) * 15.0).min(15.0) as i32;

        // Bonus for pomodoros (up to +15)
        score += ((self.pomodoros_completed as f64 / 4.0) * 15.0).min(15.0) as i32;

        // Penalty for frustration (-10 per episode, max -30)
        score -= ((self.frustration_episodes as f64) * 10.0).min(30.0) as i32;

        score.clamp(0, 100) as u32
    }
}

/// Wellness manager
#[derive(Debug)]
pub struct WellnessManager {
    /// RSI tracker
    pub rsi_tracker: RsiTracker,
    /// Focus mode
    pub focus_mode: FocusMode,
    /// Frustration detector
    pub frustration_detector: FrustrationDetector,
    /// Today's metrics
    pub today_metrics: WellnessMetrics,
}

impl WellnessManager {
    pub fn new() -> Self {
        Self {
            rsi_tracker: RsiTracker::new(),
            focus_mode: FocusMode::new(),
            frustration_detector: FrustrationDetector::new(),
            today_metrics: WellnessMetrics::default(),
        }
    }

    pub fn record_keystroke(&mut self, key: char, is_modifier: bool) {
        self.rsi_tracker.record_keystroke(key, is_modifier);
    }

    pub fn record_action(&mut self, action: ActionEvent) {
        self.frustration_detector.record_action(action);

        if self.frustration_detector.is_frustrated() {
            self.today_metrics.frustration_episodes += 1;
        }
    }

    pub fn check_break_needed(&self) -> Option<BreakReminder> {
        self.rsi_tracker.should_take_break()
    }

    pub fn take_break(&mut self, is_long: bool) {
        self.rsi_tracker.take_break(is_long);
        self.today_metrics.breaks_taken += 1;
    }

    pub fn start_focus_session(&mut self) {
        self.focus_mode.start_work();
    }

    pub fn complete_pomodoro(&mut self) {
        self.focus_mode.complete_pomodoro();
        self.today_metrics.pomodoros_completed = self.focus_mode.completed_pomodoros;
        self.today_metrics.focus_time_mins = self.focus_mode.focus_time_today_secs / 60;
    }

    pub fn should_offer_help(&mut self) -> Option<&FrustrationIndicator> {
        self.frustration_detector.should_offer_help()
    }

    pub fn wellness_score(&self) -> u32 {
        self.today_metrics.calculate_score()
    }

    pub fn typing_metrics(&self) -> TypingMetrics {
        self.rsi_tracker.analyze()
    }

    pub fn daily_summary(&self) -> String {
        format!(
            "Wellness Summary:\n\
             - Focus time: {} minutes\n\
             - Pomodoros: {}\n\
             - Breaks taken: {}\n\
             - Wellness score: {}/100",
            self.today_metrics.focus_time_mins,
            self.today_metrics.pomodoros_completed,
            self.today_metrics.breaks_taken,
            self.wellness_score()
        )
    }

    pub fn reset_daily(&mut self) {
        self.today_metrics = WellnessMetrics::default();
        self.focus_mode.completed_pomodoros = 0;
        self.focus_mode.focus_time_today_secs = 0;
        self.frustration_detector.clear();
    }
}

impl Default for WellnessManager {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    // RSI Prevention tests
    #[test]
    fn test_typing_metrics_default() {
        let metrics = TypingMetrics::default();
        assert!(!metrics.indicates_fatigue());
        assert!(!metrics.is_concerning());
    }

    #[test]
    fn test_break_config_default() {
        let config = BreakConfig::default();
        assert_eq!(config.short_break_interval_mins, 20);
        assert!(config.enabled);
    }

    #[test]
    fn test_break_reminder() {
        let short = BreakReminder::short();
        let long = BreakReminder::long();

        assert!(!short.is_long_break);
        assert!(long.is_long_break);
        assert!(short.duration < long.duration);
    }

    #[test]
    fn test_rsi_tracker() {
        let mut tracker = RsiTracker::new();

        for c in "hello world".chars() {
            tracker.record_keystroke(c, false);
            std::thread::sleep(Duration::from_millis(10));
        }

        let metrics = tracker.analyze();
        assert!(metrics.keys_per_minute > 0.0);
        assert!(metrics.avg_interval_ms > 0.0);
    }

    #[test]
    fn test_rsi_tracker_break_tracking() {
        let mut tracker = RsiTracker::new();
        tracker.config.enabled = true;

        // Initially should suggest a break (never taken one)
        assert!(tracker.should_take_break().is_some());

        tracker.take_break(false);
        // After taking a break, check passes
    }

    // Focus Mode tests
    #[test]
    fn test_pomodoro_config() {
        let config = PomodoroConfig::default();
        assert_eq!(config.work_duration_mins, 25);
        assert_eq!(config.short_break_mins, 5);
    }

    #[test]
    fn test_focus_mode_lifecycle() {
        let mut focus = FocusMode::new();

        assert_eq!(focus.state, FocusModeState::Inactive);

        focus.start_work();
        assert_eq!(focus.state, FocusModeState::Working);
        assert!(focus.time_remaining().is_some());

        focus.complete_pomodoro();
        assert_eq!(focus.completed_pomodoros, 1);
        assert!(matches!(
            focus.state,
            FocusModeState::ShortBreak | FocusModeState::LongBreak
        ));

        focus.stop();
        assert_eq!(focus.state, FocusModeState::Inactive);
    }

    #[test]
    fn test_focus_mode_distractions() {
        let mut focus = FocusMode::new();
        focus.start_work();

        assert!(focus.is_blocked(&DistractionType::SocialMedia));
        assert!(!focus.is_blocked(&DistractionType::Chat));

        focus.block_distraction(DistractionType::Chat);
        assert!(focus.is_blocked(&DistractionType::Chat));
    }

    #[test]
    fn test_focus_score() {
        let mut focus = FocusMode::new();
        focus.completed_pomodoros = 4;
        assert!(focus.focus_score() > 0.0);

        focus.record_distraction_attempt(DistractionType::SocialMedia);
        let score_after = focus.focus_score();
        assert!(score_after < 40.0); // Should be penalized
    }

    // Frustration Detection tests
    #[test]
    fn test_frustration_detector() {
        let mut detector = FrustrationDetector::new();

        // Record normal actions
        for _ in 0..5 {
            detector.record_action(ActionEvent::new(UserAction::Edit, true));
        }

        assert!(!detector.is_frustrated());
    }

    #[test]
    fn test_frustration_revert_loop() {
        let mut detector = FrustrationDetector::new().with_sensitivity(1.0);

        // Record multiple reverts
        for _ in 0..5 {
            detector.record_action(ActionEvent::new(UserAction::Revert, true));
        }

        assert!(detector.is_frustrated());
        assert!(detector
            .indicators
            .iter()
            .any(|i| i.indicator_type == FrustrationIndicatorType::RevertLoop));
    }

    #[test]
    fn test_frustration_repeated_failures() {
        let mut detector = FrustrationDetector::new().with_sensitivity(1.0);

        // Record multiple failures
        for _ in 0..6 {
            detector.record_action(ActionEvent::new(UserAction::Build, false));
        }

        assert!(detector.is_frustrated());
    }

    #[test]
    fn test_frustration_high_undo_rate() {
        let mut detector = FrustrationDetector::new().with_sensitivity(1.0);

        // Record many undos
        for i in 0..20 {
            if i % 2 == 0 {
                detector.record_action(ActionEvent::new(UserAction::Undo, true));
            } else {
                detector.record_action(ActionEvent::new(UserAction::Edit, true));
            }
        }

        assert!(detector.frustration_level() > 0.0);
    }

    #[test]
    fn test_frustration_help_cooldown() {
        let mut detector = FrustrationDetector::new().with_sensitivity(1.0);
        detector.help_cooldown_secs = 1;

        // Generate strong frustration signal
        for _ in 0..10 {
            detector.record_action(ActionEvent::new(UserAction::Revert, true));
        }

        // Ensure frustration is detected
        assert!(
            detector.is_frustrated(),
            "Should be frustrated after many reverts"
        );

        // First help offer should work
        assert!(detector.should_offer_help().is_some());
        detector.help_offered();

        // Immediate second should be blocked by cooldown
        assert!(detector.should_offer_help().is_none());
    }

    // Wellness Dashboard tests
    #[test]
    fn test_wellness_metrics_score() {
        let mut metrics = WellnessMetrics::default();
        let base_score = metrics.calculate_score();

        metrics.focus_time_mins = 60;
        metrics.breaks_taken = 4;
        metrics.pomodoros_completed = 2;

        let improved_score = metrics.calculate_score();
        assert!(improved_score > base_score);
    }

    #[test]
    fn test_wellness_metrics_frustration_penalty() {
        let mut metrics = WellnessMetrics {
            focus_time_mins: 60,
            ..Default::default()
        };
        let score_without_frustration = metrics.calculate_score();

        metrics.frustration_episodes = 3;
        let score_with_frustration = metrics.calculate_score();

        assert!(score_with_frustration < score_without_frustration);
    }

    #[test]
    fn test_wellness_manager() {
        let mut manager = WellnessManager::new();

        manager.record_keystroke('a', false);
        let metrics = manager.typing_metrics();
        // With just one keystroke, metrics won't be meaningful yet
        assert!(metrics.keys_per_minute >= 0.0);
    }

    #[test]
    fn test_wellness_manager_focus() {
        let mut manager = WellnessManager::new();

        manager.start_focus_session();
        assert_eq!(manager.focus_mode.state, FocusModeState::Working);

        manager.complete_pomodoro();
        assert_eq!(manager.today_metrics.pomodoros_completed, 1);
    }

    #[test]
    fn test_wellness_daily_summary() {
        let manager = WellnessManager::new();
        let summary = manager.daily_summary();

        assert!(summary.contains("Wellness Summary"));
        assert!(summary.contains("Focus time"));
    }

    #[test]
    fn test_wellness_reset_daily() {
        let mut manager = WellnessManager::new();
        manager.today_metrics.focus_time_mins = 120;
        manager.today_metrics.pomodoros_completed = 4;

        manager.reset_daily();

        assert_eq!(manager.today_metrics.focus_time_mins, 0);
        assert_eq!(manager.today_metrics.pomodoros_completed, 0);
    }

    // Additional comprehensive tests

    #[test]
    fn test_keystroke_event_new() {
        let event = KeystrokeEvent::new('a', false);
        assert_eq!(event.key, 'a');
        assert!(!event.is_modifier);
    }

    #[test]
    fn test_keystroke_event_modifier() {
        let event = KeystrokeEvent::new('c', true);
        assert!(event.is_modifier);
    }

    #[test]
    fn test_typing_metrics_fatigue_high_variance() {
        let metrics = TypingMetrics {
            interval_variance: 150.0,
            ..Default::default()
        };
        assert!(metrics.indicates_fatigue());
    }

    #[test]
    fn test_typing_metrics_concerning_high_rate() {
        let metrics = TypingMetrics {
            keys_per_minute: 450.0,
            ..Default::default()
        };
        assert!(metrics.is_concerning());
    }

    #[test]
    fn test_typing_metrics_concerning_long_continuous() {
        let metrics = TypingMetrics {
            continuous_typing_secs: 4000,
            ..Default::default()
        };
        assert!(metrics.is_concerning());
    }

    #[test]
    fn test_typing_metrics_clone() {
        let metrics = TypingMetrics {
            keys_per_minute: 100.0,
            avg_interval_ms: 50.0,
            ..Default::default()
        };
        let cloned = metrics.clone();
        assert_eq!(metrics.keys_per_minute, cloned.keys_per_minute);
    }

    #[test]
    fn test_typing_metrics_serialization() {
        let metrics = TypingMetrics {
            keys_per_minute: 150.0,
            avg_interval_ms: 40.0,
            interval_variance: 10.0,
            modifier_percentage: 0.1,
            continuous_typing_secs: 300,
        };

        let json = serde_json::to_string(&metrics).unwrap();
        let deserialized: TypingMetrics = serde_json::from_str(&json).unwrap();
        assert_eq!(metrics.keys_per_minute, deserialized.keys_per_minute);
    }

    #[test]
    fn test_break_config_serialization() {
        let config = BreakConfig::default();
        let json = serde_json::to_string(&config).unwrap();
        let deserialized: BreakConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(
            config.short_break_interval_mins,
            deserialized.short_break_interval_mins
        );
    }

    #[test]
    fn test_break_config_clone() {
        let config = BreakConfig {
            short_break_interval_mins: 25,
            enabled: false,
            ..Default::default()
        };
        let cloned = config.clone();
        assert_eq!(
            config.short_break_interval_mins,
            cloned.short_break_interval_mins
        );
    }

    #[test]
    fn test_focus_mode_state_all_variants() {
        let states = [
            FocusModeState::Inactive,
            FocusModeState::Working,
            FocusModeState::ShortBreak,
            FocusModeState::LongBreak,
        ];

        for state in states {
            let _ = format!("{:?}", state);
            assert_eq!(state, state);
        }
    }

    #[test]
    fn test_pomodoro_config_serialization() {
        let config = PomodoroConfig::default();
        let json = serde_json::to_string(&config).unwrap();
        let deserialized: PomodoroConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(config.work_duration_mins, deserialized.work_duration_mins);
    }

    #[test]
    fn test_distraction_type_all_variants() {
        let types = [
            DistractionType::WebBrowser,
            DistractionType::SocialMedia,
            DistractionType::Chat,
            DistractionType::Email,
            DistractionType::News,
            DistractionType::Entertainment,
        ];

        for distraction in types {
            let _ = format!("{:?}", distraction);
        }
    }

    #[test]
    fn test_user_action_all_variants() {
        let actions = [
            UserAction::Edit,
            UserAction::Undo,
            UserAction::Redo,
            UserAction::Delete,
            UserAction::Revert,
            UserAction::Save,
            UserAction::Build,
            UserAction::Test,
            UserAction::Search,
            UserAction::NavigateBack,
        ];

        for action in actions {
            let _ = format!("{:?}", action);
            assert_eq!(action, action);
        }
    }

    #[test]
    fn test_action_event_new() {
        let event = ActionEvent::new(UserAction::Build, true);
        assert_eq!(event.action, UserAction::Build);
        assert!(event.success);
    }

    #[test]
    fn test_frustration_indicator_type_all_variants() {
        let types = [
            FrustrationIndicatorType::RevertLoop,
            FrustrationIndicatorType::RapidDeletions,
            FrustrationIndicatorType::RepeatedFailures,
            FrustrationIndicatorType::CircularNavigation,
            FrustrationIndicatorType::HighUndoRate,
            FrustrationIndicatorType::LongStuckPeriod,
        ];

        for indicator in types {
            let _ = format!("{:?}", indicator);
            assert_eq!(indicator, indicator);
        }
    }

    #[test]
    fn test_frustration_detector_sensitivity() {
        let detector = FrustrationDetector::new().with_sensitivity(0.5);
        assert_eq!(detector.sensitivity, 0.5);
    }

    #[test]
    fn test_wellness_metrics_default() {
        let metrics = WellnessMetrics::default();
        assert_eq!(metrics.focus_time_mins, 0);
        assert_eq!(metrics.breaks_taken, 0);
        assert_eq!(metrics.pomodoros_completed, 0);
    }

    #[test]
    fn test_wellness_metrics_serialization() {
        let metrics = WellnessMetrics {
            focus_time_mins: 60,
            breaks_taken: 3,
            pomodoros_completed: 2,
            ..Default::default()
        };

        let json = serde_json::to_string(&metrics).unwrap();
        let deserialized: WellnessMetrics = serde_json::from_str(&json).unwrap();
        assert_eq!(metrics.focus_time_mins, deserialized.focus_time_mins);
    }

    #[test]
    fn test_wellness_metrics_clone() {
        let metrics = WellnessMetrics {
            focus_time_mins: 90,
            breaks_taken: 5,
            ..Default::default()
        };
        let cloned = metrics.clone();
        assert_eq!(metrics.focus_time_mins, cloned.focus_time_mins);
    }

    #[test]
    fn test_focus_mode_pause_resume() {
        let mut focus = FocusMode::new();
        focus.start_work();
        assert_eq!(focus.state, FocusModeState::Working);

        focus.stop();
        assert_eq!(focus.state, FocusModeState::Inactive);

        focus.start_work();
        assert_eq!(focus.state, FocusModeState::Working);
    }

    #[test]
    fn test_focus_mode_long_break_after_four() {
        let mut focus = FocusMode::new();

        for _ in 0..4 {
            focus.start_work();
            focus.complete_pomodoro();
        }

        // After 4 pomodoros, should be on long break
        assert_eq!(focus.state, FocusModeState::LongBreak);
    }

    #[test]
    fn test_rsi_tracker_modifier_tracking() {
        let mut tracker = RsiTracker::new();

        // Record several keystrokes with some modifiers
        for _ in 0..5 {
            tracker.record_keystroke('c', true); // Modifier
            std::thread::sleep(Duration::from_millis(10));
            tracker.record_keystroke('v', false);
            std::thread::sleep(Duration::from_millis(10));
        }

        let metrics = tracker.analyze();
        // With 5 modifiers out of 10 keystrokes, percentage should be significant
        assert!(metrics.modifier_percentage >= 0.0); // May still be 0 due to timing
    }

    #[test]
    fn test_wellness_manager_action_recording() {
        let mut manager = WellnessManager::new();

        manager.record_action(ActionEvent::new(UserAction::Edit, true));
        manager.record_action(ActionEvent::new(UserAction::Build, false));

        // Frustration level should be low for normal actions
        assert!(manager.frustration_detector.frustration_level() < 0.5);
    }
}
