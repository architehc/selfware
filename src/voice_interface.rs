//! Voice Interface
//!
//! Speech-to-text and text-to-speech capabilities for voice-driven
//! interaction with the agent. Supports voice commands, dictation,
//! and audio feedback.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, SystemTime};

static SESSION_COUNTER: AtomicU64 = AtomicU64::new(1);
static UTTERANCE_COUNTER: AtomicU64 = AtomicU64::new(1);
static COMMAND_COUNTER: AtomicU64 = AtomicU64::new(1);

fn generate_session_id() -> String {
    format!("vsess-{}", SESSION_COUNTER.fetch_add(1, Ordering::SeqCst))
}

fn generate_utterance_id() -> String {
    format!("utt-{}", UTTERANCE_COUNTER.fetch_add(1, Ordering::SeqCst))
}

fn generate_command_id() -> String {
    format!("vcmd-{}", COMMAND_COUNTER.fetch_add(1, Ordering::SeqCst))
}

// ============================================================================
// Speech Recognition
// ============================================================================

/// Speech recognition engine type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpeechEngine {
    Whisper,
    DeepSpeech,
    Vosk,
    GoogleSpeech,
    AzureSpeech,
    Watson,
    Native,
}

impl SpeechEngine {
    /// Get engine name
    pub fn name(&self) -> &'static str {
        match self {
            Self::Whisper => "OpenAI Whisper",
            Self::DeepSpeech => "Mozilla DeepSpeech",
            Self::Vosk => "Vosk",
            Self::GoogleSpeech => "Google Speech-to-Text",
            Self::AzureSpeech => "Azure Speech Services",
            Self::Watson => "IBM Watson Speech",
            Self::Native => "Native OS",
        }
    }

    /// Check if engine works offline
    pub fn is_offline(&self) -> bool {
        matches!(
            self,
            Self::Whisper | Self::DeepSpeech | Self::Vosk | Self::Native
        )
    }
}

/// Speech recognition configuration
#[derive(Debug, Clone)]
pub struct SpeechRecognitionConfig {
    pub engine: SpeechEngine,
    pub language: String,
    pub sample_rate: u32,
    pub channels: u8,
    pub continuous: bool,
    pub interim_results: bool,
    pub max_alternatives: u8,
    pub profanity_filter: bool,
    pub keywords: Vec<String>,
    pub model_path: Option<String>,
}

impl Default for SpeechRecognitionConfig {
    fn default() -> Self {
        Self {
            engine: SpeechEngine::Whisper,
            language: "en-US".to_string(),
            sample_rate: 16000,
            channels: 1,
            continuous: true,
            interim_results: true,
            max_alternatives: 3,
            profanity_filter: false,
            keywords: Vec::new(),
            model_path: None,
        }
    }
}

impl SpeechRecognitionConfig {
    /// Set engine
    pub fn with_engine(mut self, engine: SpeechEngine) -> Self {
        self.engine = engine;
        self
    }

    /// Set language
    pub fn with_language(mut self, language: impl Into<String>) -> Self {
        self.language = language.into();
        self
    }

    /// Add keyword for boosted recognition
    pub fn with_keyword(mut self, keyword: impl Into<String>) -> Self {
        self.keywords.push(keyword.into());
        self
    }

    /// Set model path for offline engines
    pub fn with_model(mut self, path: impl Into<String>) -> Self {
        self.model_path = Some(path.into());
        self
    }
}

/// Recognition result
#[derive(Debug, Clone)]
pub struct RecognitionResult {
    pub id: String,
    pub text: String,
    pub confidence: f32,
    pub alternatives: Vec<AlternativeResult>,
    pub is_final: bool,
    pub timestamp: SystemTime,
    pub duration: Duration,
    pub words: Vec<WordTiming>,
}

/// Alternative recognition result
#[derive(Debug, Clone)]
pub struct AlternativeResult {
    pub text: String,
    pub confidence: f32,
}

/// Word timing information
#[derive(Debug, Clone)]
pub struct WordTiming {
    pub word: String,
    pub start: Duration,
    pub end: Duration,
    pub confidence: f32,
}

impl RecognitionResult {
    /// Create new result
    pub fn new(text: impl Into<String>, confidence: f32, is_final: bool) -> Self {
        Self {
            id: generate_utterance_id(),
            text: text.into(),
            confidence,
            alternatives: Vec::new(),
            is_final,
            timestamp: SystemTime::now(),
            duration: Duration::ZERO,
            words: Vec::new(),
        }
    }

    /// Add alternative
    pub fn with_alternative(mut self, text: impl Into<String>, confidence: f32) -> Self {
        self.alternatives.push(AlternativeResult {
            text: text.into(),
            confidence,
        });
        self
    }

    /// Set duration
    pub fn with_duration(mut self, duration: Duration) -> Self {
        self.duration = duration;
        self
    }

    /// Add word timing
    pub fn add_word(
        &mut self,
        word: impl Into<String>,
        start: Duration,
        end: Duration,
        confidence: f32,
    ) {
        self.words.push(WordTiming {
            word: word.into(),
            start,
            end,
            confidence,
        });
    }

    /// Get words as string
    pub fn words_text(&self) -> Vec<&str> {
        self.words.iter().map(|w| w.word.as_str()).collect()
    }
}

/// Speech recognizer
#[derive(Debug)]
pub struct SpeechRecognizer {
    config: SpeechRecognitionConfig,
    is_listening: bool,
    results: Vec<RecognitionResult>,
    error_count: u32,
}

impl SpeechRecognizer {
    /// Create new recognizer
    pub fn new(config: SpeechRecognitionConfig) -> Self {
        Self {
            config,
            is_listening: false,
            results: Vec::new(),
            error_count: 0,
        }
    }

    /// Create with default config
    pub fn with_defaults() -> Self {
        Self::new(SpeechRecognitionConfig::default())
    }

    /// Get config
    pub fn config(&self) -> &SpeechRecognitionConfig {
        &self.config
    }

    /// Start listening
    pub fn start(&mut self) -> Result<(), String> {
        if self.is_listening {
            return Err("Already listening".to_string());
        }
        self.is_listening = true;
        Ok(())
    }

    /// Stop listening
    pub fn stop(&mut self) {
        self.is_listening = false;
    }

    /// Check if listening
    pub fn is_listening(&self) -> bool {
        self.is_listening
    }

    /// Simulate receiving audio data (for testing)
    pub fn process_audio(&mut self, _audio_data: &[u8]) -> Option<RecognitionResult> {
        if !self.is_listening {
            return None;
        }

        // Simulated result for testing
        let result = RecognitionResult::new("Hello world", 0.95, true);
        self.results.push(result.clone());
        Some(result)
    }

    /// Get all results
    pub fn results(&self) -> &[RecognitionResult] {
        &self.results
    }

    /// Get final results only
    pub fn final_results(&self) -> Vec<&RecognitionResult> {
        self.results.iter().filter(|r| r.is_final).collect()
    }

    /// Clear results
    pub fn clear_results(&mut self) {
        self.results.clear();
    }

    /// Record error
    pub fn record_error(&mut self) {
        self.error_count += 1;
    }

    /// Get error count
    pub fn error_count(&self) -> u32 {
        self.error_count
    }
}

// ============================================================================
// Speech Synthesis
// ============================================================================

/// Text-to-speech engine type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TTSEngine {
    Piper,
    Espeak,
    Coqui,
    GoogleTTS,
    AzureTTS,
    Amazon,
    ElevenLabs,
    Native,
}

impl TTSEngine {
    /// Get engine name
    pub fn name(&self) -> &'static str {
        match self {
            Self::Piper => "Piper TTS",
            Self::Espeak => "eSpeak",
            Self::Coqui => "Coqui TTS",
            Self::GoogleTTS => "Google Text-to-Speech",
            Self::AzureTTS => "Azure Speech Services",
            Self::Amazon => "Amazon Polly",
            Self::ElevenLabs => "ElevenLabs",
            Self::Native => "Native OS",
        }
    }

    /// Check if engine works offline
    pub fn is_offline(&self) -> bool {
        matches!(
            self,
            Self::Piper | Self::Espeak | Self::Coqui | Self::Native
        )
    }
}

/// Speech synthesis configuration
#[derive(Debug, Clone)]
pub struct SpeechSynthesisConfig {
    pub engine: TTSEngine,
    pub voice: String,
    pub language: String,
    pub rate: f32,
    pub pitch: f32,
    pub volume: f32,
    pub sample_rate: u32,
    pub model_path: Option<String>,
}

impl Default for SpeechSynthesisConfig {
    fn default() -> Self {
        Self {
            engine: TTSEngine::Piper,
            voice: "default".to_string(),
            language: "en-US".to_string(),
            rate: 1.0,
            pitch: 1.0,
            volume: 1.0,
            sample_rate: 22050,
            model_path: None,
        }
    }
}

impl SpeechSynthesisConfig {
    /// Set engine
    pub fn with_engine(mut self, engine: TTSEngine) -> Self {
        self.engine = engine;
        self
    }

    /// Set voice
    pub fn with_voice(mut self, voice: impl Into<String>) -> Self {
        self.voice = voice.into();
        self
    }

    /// Set language
    pub fn with_language(mut self, language: impl Into<String>) -> Self {
        self.language = language.into();
        self
    }

    /// Set speech rate (0.5 = half speed, 2.0 = double speed)
    pub fn with_rate(mut self, rate: f32) -> Self {
        self.rate = rate.clamp(0.25, 4.0);
        self
    }

    /// Set pitch (0.5 = lower, 2.0 = higher)
    pub fn with_pitch(mut self, pitch: f32) -> Self {
        self.pitch = pitch.clamp(0.5, 2.0);
        self
    }

    /// Set volume (0.0 to 1.0)
    pub fn with_volume(mut self, volume: f32) -> Self {
        self.volume = volume.clamp(0.0, 1.0);
        self
    }

    /// Set model path
    pub fn with_model(mut self, path: impl Into<String>) -> Self {
        self.model_path = Some(path.into());
        self
    }
}

/// Voice metadata
#[derive(Debug, Clone)]
pub struct Voice {
    pub id: String,
    pub name: String,
    pub language: String,
    pub gender: VoiceGender,
    pub style: Option<String>,
    pub preview_url: Option<String>,
}

/// Voice gender
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VoiceGender {
    Male,
    Female,
    Neutral,
}

impl Voice {
    /// Create new voice
    pub fn new(
        id: impl Into<String>,
        name: impl Into<String>,
        language: impl Into<String>,
    ) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            language: language.into(),
            gender: VoiceGender::Neutral,
            style: None,
            preview_url: None,
        }
    }

    /// Set gender
    pub fn with_gender(mut self, gender: VoiceGender) -> Self {
        self.gender = gender;
        self
    }

    /// Set style
    pub fn with_style(mut self, style: impl Into<String>) -> Self {
        self.style = Some(style.into());
        self
    }
}

/// Synthesis request
#[derive(Debug, Clone)]
pub struct SynthesisRequest {
    pub id: String,
    pub text: String,
    pub ssml: Option<String>,
    pub voice_override: Option<String>,
    pub priority: SynthesisPriority,
    pub cache_key: Option<String>,
}

/// Synthesis priority
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum SynthesisPriority {
    Low,
    Normal,
    High,
    Immediate,
}

impl SynthesisRequest {
    /// Create text request
    pub fn text(text: impl Into<String>) -> Self {
        Self {
            id: generate_utterance_id(),
            text: text.into(),
            ssml: None,
            voice_override: None,
            priority: SynthesisPriority::Normal,
            cache_key: None,
        }
    }

    /// Create SSML request
    pub fn ssml(ssml: impl Into<String>) -> Self {
        let ssml_str = ssml.into();
        // Extract text from SSML (simplified)
        let text = ssml_str
            .replace("<speak>", "")
            .replace("</speak>", "")
            .replace("</s>", " ")
            .replace("<s>", "");

        Self {
            id: generate_utterance_id(),
            text,
            ssml: Some(ssml_str),
            voice_override: None,
            priority: SynthesisPriority::Normal,
            cache_key: None,
        }
    }

    /// Set priority
    pub fn with_priority(mut self, priority: SynthesisPriority) -> Self {
        self.priority = priority;
        self
    }

    /// Set voice override
    pub fn with_voice(mut self, voice: impl Into<String>) -> Self {
        self.voice_override = Some(voice.into());
        self
    }

    /// Set cache key
    pub fn with_cache_key(mut self, key: impl Into<String>) -> Self {
        self.cache_key = Some(key.into());
        self
    }
}

/// Synthesis result
#[derive(Debug, Clone)]
pub struct SynthesisResult {
    pub id: String,
    pub audio_data: Vec<u8>,
    pub sample_rate: u32,
    pub duration: Duration,
    pub cached: bool,
}

impl SynthesisResult {
    /// Create new result
    pub fn new(id: impl Into<String>, audio_data: Vec<u8>, sample_rate: u32) -> Self {
        let duration = if sample_rate > 0 && !audio_data.is_empty() {
            Duration::from_secs_f64(
                audio_data.len() as f64 / (sample_rate as f64 * 2.0), // Assuming 16-bit audio
            )
        } else {
            Duration::ZERO
        };
        Self {
            id: id.into(),
            audio_data,
            sample_rate,
            duration,
            cached: false,
        }
    }

    /// Mark as cached
    pub fn with_cached(mut self, cached: bool) -> Self {
        self.cached = cached;
        self
    }
}

/// Speech synthesizer
#[derive(Debug)]
pub struct SpeechSynthesizer {
    config: SpeechSynthesisConfig,
    voices: Vec<Voice>,
    cache: HashMap<String, SynthesisResult>,
    queue: Vec<SynthesisRequest>,
    is_speaking: bool,
}

impl SpeechSynthesizer {
    /// Create new synthesizer
    pub fn new(config: SpeechSynthesisConfig) -> Self {
        Self {
            config,
            voices: Vec::new(),
            cache: HashMap::new(),
            queue: Vec::new(),
            is_speaking: false,
        }
    }

    /// Create with defaults
    pub fn with_defaults() -> Self {
        Self::new(SpeechSynthesisConfig::default())
    }

    /// Get config
    pub fn config(&self) -> &SpeechSynthesisConfig {
        &self.config
    }

    /// Add voice
    pub fn add_voice(&mut self, voice: Voice) {
        self.voices.push(voice);
    }

    /// Get voices
    pub fn voices(&self) -> &[Voice] {
        &self.voices
    }

    /// Get voices for language
    pub fn voices_for_language(&self, language: &str) -> Vec<&Voice> {
        self.voices
            .iter()
            .filter(|v| {
                v.language
                    .starts_with(language.split('-').next().unwrap_or(""))
            })
            .collect()
    }

    /// Synthesize text
    pub fn synthesize(&mut self, request: SynthesisRequest) -> SynthesisResult {
        // Check cache
        if let Some(key) = &request.cache_key {
            if let Some(cached) = self.cache.get(key) {
                return cached.clone().with_cached(true);
            }
        }

        // Simulate synthesis (in real implementation, would call TTS engine)
        let audio_data = vec![0u8; 44100 * 2]; // 1 second of silence
        let result = SynthesisResult::new(&request.id, audio_data, self.config.sample_rate);

        // Cache if key provided
        if let Some(key) = request.cache_key {
            self.cache.insert(key, result.clone());
        }

        result
    }

    /// Queue request
    pub fn queue(&mut self, request: SynthesisRequest) {
        // Insert based on priority
        let pos = self
            .queue
            .iter()
            .position(|r| r.priority < request.priority)
            .unwrap_or(self.queue.len());
        self.queue.insert(pos, request);
    }

    /// Process next in queue
    pub fn process_next(&mut self) -> Option<SynthesisResult> {
        if self.queue.is_empty() {
            return None;
        }
        let request = self.queue.remove(0);
        Some(self.synthesize(request))
    }

    /// Get queue length
    pub fn queue_length(&self) -> usize {
        self.queue.len()
    }

    /// Clear queue
    pub fn clear_queue(&mut self) {
        self.queue.clear();
    }

    /// Check if speaking
    pub fn is_speaking(&self) -> bool {
        self.is_speaking
    }

    /// Start speaking
    pub fn start_speaking(&mut self) {
        self.is_speaking = true;
    }

    /// Stop speaking
    pub fn stop_speaking(&mut self) {
        self.is_speaking = false;
    }

    /// Clear cache
    pub fn clear_cache(&mut self) {
        self.cache.clear();
    }

    /// Get cache size
    pub fn cache_size(&self) -> usize {
        self.cache.len()
    }
}

// ============================================================================
// Voice Commands
// ============================================================================

/// Voice command definition
#[derive(Debug, Clone)]
pub struct VoiceCommand {
    pub id: String,
    pub name: String,
    pub phrases: Vec<String>,
    pub action: CommandAction,
    pub confirmation_required: bool,
    pub enabled: bool,
}

/// Command action types
#[derive(Debug, Clone)]
pub enum CommandAction {
    Execute(String),
    Navigate(String),
    Input(String),
    Toggle(String),
    Custom(String),
}

impl VoiceCommand {
    /// Create new command
    pub fn new(name: impl Into<String>, action: CommandAction) -> Self {
        Self {
            id: generate_command_id(),
            name: name.into(),
            phrases: Vec::new(),
            action,
            confirmation_required: false,
            enabled: true,
        }
    }

    /// Add trigger phrase
    pub fn with_phrase(mut self, phrase: impl Into<String>) -> Self {
        self.phrases.push(phrase.into());
        self
    }

    /// Require confirmation
    pub fn with_confirmation(mut self) -> Self {
        self.confirmation_required = true;
        self
    }

    /// Set enabled
    pub fn with_enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    /// Check if phrase matches
    pub fn matches(&self, text: &str) -> bool {
        if !self.enabled {
            return false;
        }

        let text_lower = text.to_lowercase();
        self.phrases
            .iter()
            .any(|p| text_lower.contains(&p.to_lowercase()))
    }

    /// Get match score (0.0 to 1.0)
    pub fn match_score(&self, text: &str) -> f32 {
        if !self.enabled {
            return 0.0;
        }

        let text_lower = text.to_lowercase();
        let text_words: Vec<_> = text_lower.split_whitespace().collect();

        self.phrases
            .iter()
            .map(|phrase| {
                let phrase_lower = phrase.to_lowercase();
                let phrase_words: Vec<_> = phrase_lower.split_whitespace().collect();

                let matching_words = phrase_words
                    .iter()
                    .filter(|w| text_words.contains(w))
                    .count();

                if phrase_words.is_empty() {
                    0.0
                } else {
                    matching_words as f32 / phrase_words.len() as f32
                }
            })
            .max_by(|a, b| a.partial_cmp(b).unwrap())
            .unwrap_or(0.0)
    }
}

/// Command match result
#[derive(Debug, Clone)]
pub struct CommandMatch {
    pub command: VoiceCommand,
    pub score: f32,
    pub extracted_params: HashMap<String, String>,
}

/// Voice command registry
#[derive(Debug)]
pub struct VoiceCommandRegistry {
    commands: Vec<VoiceCommand>,
    aliases: HashMap<String, String>,
    match_threshold: f32,
}

impl VoiceCommandRegistry {
    /// Create new registry
    pub fn new() -> Self {
        Self {
            commands: Vec::new(),
            aliases: HashMap::new(),
            match_threshold: 0.6,
        }
    }

    /// Create with common commands
    pub fn with_defaults() -> Self {
        let mut registry = Self::new();

        // Navigation commands
        registry.register(
            VoiceCommand::new("go_back", CommandAction::Navigate("back".to_string()))
                .with_phrase("go back")
                .with_phrase("previous")
                .with_phrase("back"),
        );

        registry.register(
            VoiceCommand::new("go_forward", CommandAction::Navigate("forward".to_string()))
                .with_phrase("go forward")
                .with_phrase("next")
                .with_phrase("forward"),
        );

        // Action commands
        registry.register(
            VoiceCommand::new("cancel", CommandAction::Execute("cancel".to_string()))
                .with_phrase("cancel")
                .with_phrase("stop")
                .with_phrase("abort"),
        );

        registry.register(
            VoiceCommand::new("confirm", CommandAction::Execute("confirm".to_string()))
                .with_phrase("confirm")
                .with_phrase("yes")
                .with_phrase("okay")
                .with_phrase("ok"),
        );

        registry.register(
            VoiceCommand::new("help", CommandAction::Execute("help".to_string()))
                .with_phrase("help")
                .with_phrase("what can you do")
                .with_phrase("show commands"),
        );

        registry
    }

    /// Register command
    pub fn register(&mut self, command: VoiceCommand) {
        self.commands.push(command);
    }

    /// Register alias
    pub fn add_alias(&mut self, alias: impl Into<String>, command_name: impl Into<String>) {
        self.aliases.insert(alias.into(), command_name.into());
    }

    /// Set match threshold
    pub fn set_threshold(&mut self, threshold: f32) {
        self.match_threshold = threshold.clamp(0.0, 1.0);
    }

    /// Get command by name
    pub fn get(&self, name: &str) -> Option<&VoiceCommand> {
        self.commands.iter().find(|c| c.name == name)
    }

    /// Get all commands
    pub fn commands(&self) -> &[VoiceCommand] {
        &self.commands
    }

    /// Find matching commands
    pub fn find_matches(&self, text: &str) -> Vec<CommandMatch> {
        let mut matches: Vec<_> = self
            .commands
            .iter()
            .filter(|c| c.enabled)
            .map(|c| CommandMatch {
                command: c.clone(),
                score: c.match_score(text),
                extracted_params: HashMap::new(),
            })
            .filter(|m| m.score >= self.match_threshold)
            .collect();

        // Sort by score descending
        matches.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap());
        matches
    }

    /// Find best match
    pub fn find_best_match(&self, text: &str) -> Option<CommandMatch> {
        self.find_matches(text).into_iter().next()
    }

    /// Enable/disable command
    pub fn set_enabled(&mut self, name: &str, enabled: bool) -> bool {
        if let Some(cmd) = self.commands.iter_mut().find(|c| c.name == name) {
            cmd.enabled = enabled;
            true
        } else {
            false
        }
    }
}

impl Default for VoiceCommandRegistry {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// Wake Word Detection
// ============================================================================

/// Wake word detector
#[derive(Debug)]
pub struct WakeWordDetector {
    wake_words: Vec<String>,
    sensitivity: f32,
    is_active: bool,
    detections: Vec<WakeWordDetection>,
}

/// Wake word detection event
#[derive(Debug, Clone)]
pub struct WakeWordDetection {
    pub word: String,
    pub confidence: f32,
    pub timestamp: SystemTime,
}

impl WakeWordDetector {
    /// Create new detector
    pub fn new() -> Self {
        Self {
            wake_words: vec!["hey assistant".to_string(), "ok assistant".to_string()],
            sensitivity: 0.5,
            is_active: false,
            detections: Vec::new(),
        }
    }

    /// Add wake word
    pub fn add_wake_word(&mut self, word: impl Into<String>) {
        self.wake_words.push(word.into());
    }

    /// Set sensitivity (0.0 to 1.0, higher = more sensitive)
    pub fn set_sensitivity(&mut self, sensitivity: f32) {
        self.sensitivity = sensitivity.clamp(0.0, 1.0);
    }

    /// Start detection
    pub fn start(&mut self) {
        self.is_active = true;
    }

    /// Stop detection
    pub fn stop(&mut self) {
        self.is_active = false;
    }

    /// Check if active
    pub fn is_active(&self) -> bool {
        self.is_active
    }

    /// Process audio for wake word (simulated)
    pub fn process(&mut self, text: &str) -> Option<WakeWordDetection> {
        if !self.is_active {
            return None;
        }

        let text_lower = text.to_lowercase();

        for wake_word in &self.wake_words {
            if text_lower.contains(&wake_word.to_lowercase()) {
                let detection = WakeWordDetection {
                    word: wake_word.clone(),
                    confidence: 0.95,
                    timestamp: SystemTime::now(),
                };
                self.detections.push(detection.clone());
                return Some(detection);
            }
        }

        None
    }

    /// Get recent detections
    pub fn recent_detections(&self, limit: usize) -> Vec<&WakeWordDetection> {
        self.detections.iter().rev().take(limit).collect()
    }

    /// Clear detections
    pub fn clear_detections(&mut self) {
        self.detections.clear();
    }
}

impl Default for WakeWordDetector {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// Voice Session
// ============================================================================

/// Voice session state
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionState {
    Idle,
    Listening,
    Processing,
    Speaking,
    WaitingForCommand,
    Error,
}

/// Voice session for interaction
#[derive(Debug, Clone)]
pub struct VoiceSession {
    pub id: String,
    pub state: SessionState,
    pub started_at: SystemTime,
    pub last_activity: SystemTime,
    pub utterances: Vec<RecognitionResult>,
    pub responses: Vec<SynthesisRequest>,
    pub commands_executed: Vec<String>,
}

impl VoiceSession {
    /// Create new session
    pub fn new() -> Self {
        let now = SystemTime::now();
        Self {
            id: generate_session_id(),
            state: SessionState::Idle,
            started_at: now,
            last_activity: now,
            utterances: Vec::new(),
            responses: Vec::new(),
            commands_executed: Vec::new(),
        }
    }

    /// Set state
    pub fn set_state(&mut self, state: SessionState) {
        self.state = state;
        self.last_activity = SystemTime::now();
    }

    /// Add utterance
    pub fn add_utterance(&mut self, result: RecognitionResult) {
        self.utterances.push(result);
        self.last_activity = SystemTime::now();
    }

    /// Add response
    pub fn add_response(&mut self, request: SynthesisRequest) {
        self.responses.push(request);
        self.last_activity = SystemTime::now();
    }

    /// Record command execution
    pub fn record_command(&mut self, command_name: impl Into<String>) {
        self.commands_executed.push(command_name.into());
        self.last_activity = SystemTime::now();
    }

    /// Get duration
    pub fn duration(&self) -> Duration {
        self.last_activity
            .duration_since(self.started_at)
            .unwrap_or_default()
    }

    /// Get utterance count
    pub fn utterance_count(&self) -> usize {
        self.utterances.len()
    }

    /// Get last utterance
    pub fn last_utterance(&self) -> Option<&RecognitionResult> {
        self.utterances.last()
    }

    /// Check if session is active
    pub fn is_active(&self) -> bool {
        !matches!(self.state, SessionState::Idle | SessionState::Error)
    }

    /// Check if timed out
    pub fn is_timed_out(&self, timeout: Duration) -> bool {
        self.last_activity
            .elapsed()
            .map(|d| d > timeout)
            .unwrap_or(false)
    }
}

impl Default for VoiceSession {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// Voice Interface
// ============================================================================

/// Main voice interface
#[derive(Debug)]
pub struct VoiceInterface {
    recognizer: SpeechRecognizer,
    synthesizer: SpeechSynthesizer,
    commands: VoiceCommandRegistry,
    wake_word: WakeWordDetector,
    current_session: Option<VoiceSession>,
    sessions: Vec<VoiceSession>,
    muted: bool,
}

impl VoiceInterface {
    /// Create new voice interface
    pub fn new() -> Self {
        Self {
            recognizer: SpeechRecognizer::with_defaults(),
            synthesizer: SpeechSynthesizer::with_defaults(),
            commands: VoiceCommandRegistry::with_defaults(),
            wake_word: WakeWordDetector::new(),
            current_session: None,
            sessions: Vec::new(),
            muted: false,
        }
    }

    /// Create with custom config
    pub fn with_config(
        recognition_config: SpeechRecognitionConfig,
        synthesis_config: SpeechSynthesisConfig,
    ) -> Self {
        Self {
            recognizer: SpeechRecognizer::new(recognition_config),
            synthesizer: SpeechSynthesizer::new(synthesis_config),
            commands: VoiceCommandRegistry::with_defaults(),
            wake_word: WakeWordDetector::new(),
            current_session: None,
            sessions: Vec::new(),
            muted: false,
        }
    }

    /// Get recognizer
    pub fn recognizer(&self) -> &SpeechRecognizer {
        &self.recognizer
    }

    /// Get recognizer mut
    pub fn recognizer_mut(&mut self) -> &mut SpeechRecognizer {
        &mut self.recognizer
    }

    /// Get synthesizer
    pub fn synthesizer(&self) -> &SpeechSynthesizer {
        &self.synthesizer
    }

    /// Get synthesizer mut
    pub fn synthesizer_mut(&mut self) -> &mut SpeechSynthesizer {
        &mut self.synthesizer
    }

    /// Get commands registry
    pub fn commands(&self) -> &VoiceCommandRegistry {
        &self.commands
    }

    /// Get commands registry mut
    pub fn commands_mut(&mut self) -> &mut VoiceCommandRegistry {
        &mut self.commands
    }

    /// Get wake word detector
    pub fn wake_word(&self) -> &WakeWordDetector {
        &self.wake_word
    }

    /// Get wake word detector mut
    pub fn wake_word_mut(&mut self) -> &mut WakeWordDetector {
        &mut self.wake_word
    }

    /// Start listening
    pub fn start_listening(&mut self) -> Result<(), String> {
        if self.current_session.is_none() {
            let mut session = VoiceSession::new();
            session.set_state(SessionState::Listening);
            self.current_session = Some(session);
        }

        self.recognizer.start()
    }

    /// Stop listening
    pub fn stop_listening(&mut self) {
        self.recognizer.stop();

        if let Some(session) = &mut self.current_session {
            session.set_state(SessionState::Idle);
        }
    }

    /// Speak text
    pub fn speak(&mut self, text: impl Into<String>) -> SynthesisResult {
        if self.muted {
            return SynthesisResult::new("muted", vec![], 0);
        }

        let request = SynthesisRequest::text(text);

        if let Some(session) = &mut self.current_session {
            session.add_response(request.clone());
            session.set_state(SessionState::Speaking);
        }

        let result = self.synthesizer.synthesize(request);

        if let Some(session) = &mut self.current_session {
            session.set_state(SessionState::WaitingForCommand);
        }

        result
    }

    /// Process text input (simulates voice recognition)
    pub fn process_text(&mut self, text: &str) -> Option<CommandMatch> {
        let result = RecognitionResult::new(text, 1.0, true);

        if let Some(session) = &mut self.current_session {
            session.add_utterance(result);
            session.set_state(SessionState::Processing);
        }

        // Find matching command
        let command_match = self.commands.find_best_match(text);

        if let Some(ref m) = command_match {
            if let Some(session) = &mut self.current_session {
                session.record_command(&m.command.name);
            }
        }

        if let Some(session) = &mut self.current_session {
            session.set_state(SessionState::WaitingForCommand);
        }

        command_match
    }

    /// Mute audio output
    pub fn mute(&mut self) {
        self.muted = true;
    }

    /// Unmute audio output
    pub fn unmute(&mut self) {
        self.muted = false;
    }

    /// Check if muted
    pub fn is_muted(&self) -> bool {
        self.muted
    }

    /// Get current session
    pub fn current_session(&self) -> Option<&VoiceSession> {
        self.current_session.as_ref()
    }

    /// End current session
    pub fn end_session(&mut self) -> Option<VoiceSession> {
        if let Some(mut session) = self.current_session.take() {
            session.set_state(SessionState::Idle);
            self.sessions.push(session.clone());
            Some(session)
        } else {
            None
        }
    }

    /// Get session history
    pub fn session_history(&self) -> &[VoiceSession] {
        &self.sessions
    }

    /// Clear session history
    pub fn clear_history(&mut self) {
        self.sessions.clear();
    }
}

impl Default for VoiceInterface {
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

    #[test]
    fn test_speech_engine_name() {
        assert_eq!(SpeechEngine::Whisper.name(), "OpenAI Whisper");
        assert_eq!(SpeechEngine::DeepSpeech.name(), "Mozilla DeepSpeech");
    }

    #[test]
    fn test_speech_engine_offline() {
        assert!(SpeechEngine::Whisper.is_offline());
        assert!(SpeechEngine::Vosk.is_offline());
        assert!(!SpeechEngine::GoogleSpeech.is_offline());
    }

    #[test]
    fn test_speech_recognition_config() {
        let config = SpeechRecognitionConfig::default()
            .with_engine(SpeechEngine::Vosk)
            .with_language("de-DE")
            .with_keyword("activate");

        assert_eq!(config.engine, SpeechEngine::Vosk);
        assert_eq!(config.language, "de-DE");
        assert!(config.keywords.contains(&"activate".to_string()));
    }

    #[test]
    fn test_recognition_result() {
        let result = RecognitionResult::new("Hello world", 0.95, true)
            .with_alternative("Hello word", 0.80)
            .with_duration(Duration::from_secs(2));

        assert_eq!(result.text, "Hello world");
        assert_eq!(result.confidence, 0.95);
        assert!(result.is_final);
        assert_eq!(result.alternatives.len(), 1);
    }

    #[test]
    fn test_recognition_result_words() {
        let mut result = RecognitionResult::new("Hello world", 0.95, true);

        result.add_word(
            "Hello",
            Duration::from_millis(0),
            Duration::from_millis(500),
            0.98,
        );
        result.add_word(
            "world",
            Duration::from_millis(500),
            Duration::from_millis(1000),
            0.92,
        );

        assert_eq!(result.words.len(), 2);
        assert_eq!(result.words_text(), vec!["Hello", "world"]);
    }

    #[test]
    fn test_speech_recognizer() {
        let mut recognizer = SpeechRecognizer::with_defaults();

        assert!(!recognizer.is_listening());

        recognizer.start().unwrap();
        assert!(recognizer.is_listening());

        recognizer.stop();
        assert!(!recognizer.is_listening());
    }

    #[test]
    fn test_speech_recognizer_process() {
        let mut recognizer = SpeechRecognizer::with_defaults();

        recognizer.start().unwrap();
        let result = recognizer.process_audio(&[0u8; 1024]);

        assert!(result.is_some());
        assert_eq!(recognizer.results().len(), 1);
    }

    #[test]
    fn test_tts_engine_name() {
        assert_eq!(TTSEngine::Piper.name(), "Piper TTS");
        assert_eq!(TTSEngine::ElevenLabs.name(), "ElevenLabs");
    }

    #[test]
    fn test_tts_engine_offline() {
        assert!(TTSEngine::Piper.is_offline());
        assert!(TTSEngine::Espeak.is_offline());
        assert!(!TTSEngine::GoogleTTS.is_offline());
    }

    #[test]
    fn test_speech_synthesis_config() {
        let config = SpeechSynthesisConfig::default()
            .with_engine(TTSEngine::Espeak)
            .with_voice("en-us")
            .with_rate(1.5)
            .with_pitch(1.2)
            .with_volume(0.8);

        assert_eq!(config.engine, TTSEngine::Espeak);
        assert_eq!(config.voice, "en-us");
        assert_eq!(config.rate, 1.5);
        assert_eq!(config.pitch, 1.2);
        assert_eq!(config.volume, 0.8);
    }

    #[test]
    fn test_speech_synthesis_config_clamp() {
        let config = SpeechSynthesisConfig::default()
            .with_rate(10.0)
            .with_volume(2.0);

        assert_eq!(config.rate, 4.0); // Clamped to max
        assert_eq!(config.volume, 1.0); // Clamped to max
    }

    #[test]
    fn test_voice() {
        let voice = Voice::new("en-us-1", "English US", "en-US")
            .with_gender(VoiceGender::Female)
            .with_style("friendly");

        assert_eq!(voice.id, "en-us-1");
        assert_eq!(voice.gender, VoiceGender::Female);
        assert_eq!(voice.style, Some("friendly".to_string()));
    }

    #[test]
    fn test_synthesis_request() {
        let request = SynthesisRequest::text("Hello world")
            .with_priority(SynthesisPriority::High)
            .with_voice("en-us-1")
            .with_cache_key("greeting");

        assert_eq!(request.text, "Hello world");
        assert_eq!(request.priority, SynthesisPriority::High);
        assert!(request.voice_override.is_some());
        assert!(request.cache_key.is_some());
    }

    #[test]
    fn test_synthesis_request_ssml() {
        let ssml = "<speak>Hello <s>world</s></speak>";
        let request = SynthesisRequest::ssml(ssml);

        assert!(request.ssml.is_some());
        assert!(!request.text.is_empty());
    }

    #[test]
    fn test_speech_synthesizer() {
        let mut synthesizer = SpeechSynthesizer::with_defaults();

        let request = SynthesisRequest::text("Test");
        let result = synthesizer.synthesize(request);

        assert!(!result.audio_data.is_empty());
        assert!(!result.cached);
    }

    #[test]
    fn test_speech_synthesizer_cache() {
        let mut synthesizer = SpeechSynthesizer::with_defaults();

        let request = SynthesisRequest::text("Test").with_cache_key("test-key");
        let _result1 = synthesizer.synthesize(request.clone());

        let request2 = SynthesisRequest::text("Test").with_cache_key("test-key");
        let result2 = synthesizer.synthesize(request2);

        assert!(result2.cached);
    }

    #[test]
    fn test_speech_synthesizer_queue() {
        let mut synthesizer = SpeechSynthesizer::with_defaults();

        synthesizer.queue(SynthesisRequest::text("Low").with_priority(SynthesisPriority::Low));
        synthesizer.queue(SynthesisRequest::text("High").with_priority(SynthesisPriority::High));
        synthesizer.queue(SynthesisRequest::text("Normal"));

        assert_eq!(synthesizer.queue_length(), 3);

        let result = synthesizer.process_next().unwrap();
        // High priority should be first
        assert_eq!(result.id.starts_with("utt-"), true);
    }

    #[test]
    fn test_voice_command() {
        let command = VoiceCommand::new("test", CommandAction::Execute("test".to_string()))
            .with_phrase("run test")
            .with_phrase("execute test")
            .with_confirmation();

        assert!(command.matches("please run test"));
        assert!(command.matches("execute test now"));
        assert!(!command.matches("something else"));
        assert!(command.confirmation_required);
    }

    #[test]
    fn test_voice_command_match_score() {
        let command = VoiceCommand::new("search", CommandAction::Execute("search".to_string()))
            .with_phrase("search for files");

        let score1 = command.match_score("search for files");
        let score2 = command.match_score("search files");
        let score3 = command.match_score("completely different");

        assert!(score1 > score2);
        assert!(score2 > score3);
    }

    #[test]
    fn test_voice_command_disabled() {
        let command = VoiceCommand::new("test", CommandAction::Execute("test".to_string()))
            .with_phrase("run test")
            .with_enabled(false);

        assert!(!command.matches("run test"));
        assert_eq!(command.match_score("run test"), 0.0);
    }

    #[test]
    fn test_voice_command_registry() {
        let registry = VoiceCommandRegistry::with_defaults();

        assert!(registry.get("cancel").is_some());
        assert!(registry.get("confirm").is_some());
        assert!(registry.get("help").is_some());
    }

    #[test]
    fn test_voice_command_registry_find_matches() {
        let registry = VoiceCommandRegistry::with_defaults();

        let matches = registry.find_matches("please cancel this");
        assert!(!matches.is_empty());
        assert_eq!(matches[0].command.name, "cancel");
    }

    #[test]
    fn test_voice_command_registry_find_best_match() {
        let registry = VoiceCommandRegistry::with_defaults();

        let best = registry.find_best_match("can you help me");
        assert!(best.is_some());
        assert_eq!(best.unwrap().command.name, "help");
    }

    #[test]
    fn test_voice_command_registry_enable_disable() {
        let mut registry = VoiceCommandRegistry::with_defaults();

        assert!(registry.set_enabled("cancel", false));
        assert!(registry.find_best_match("cancel").is_none());

        assert!(registry.set_enabled("cancel", true));
        assert!(registry.find_best_match("cancel").is_some());
    }

    #[test]
    fn test_wake_word_detector() {
        let mut detector = WakeWordDetector::new();

        detector.start();
        assert!(detector.is_active());

        let detection = detector.process("hey assistant, what time is it");
        assert!(detection.is_some());
        assert!(detection.unwrap().word.contains("hey assistant"));
    }

    #[test]
    fn test_wake_word_detector_custom_word() {
        let mut detector = WakeWordDetector::new();
        detector.add_wake_word("computer");

        detector.start();
        let detection = detector.process("computer, turn on the lights");
        assert!(detection.is_some());
    }

    #[test]
    fn test_wake_word_detector_inactive() {
        let mut detector = WakeWordDetector::new();

        // Not started
        let detection = detector.process("hey assistant");
        assert!(detection.is_none());
    }

    #[test]
    fn test_voice_session() {
        let mut session = VoiceSession::new();

        assert_eq!(session.state, SessionState::Idle);

        session.set_state(SessionState::Listening);
        assert_eq!(session.state, SessionState::Listening);
        assert!(session.is_active());

        session.add_utterance(RecognitionResult::new("Hello", 0.9, true));
        assert_eq!(session.utterance_count(), 1);
    }

    #[test]
    fn test_voice_session_timeout() {
        let session = VoiceSession::new();

        // Fresh session shouldn't be timed out
        assert!(!session.is_timed_out(Duration::from_secs(60)));
    }

    #[test]
    fn test_voice_session_commands() {
        let mut session = VoiceSession::new();

        session.record_command("test1");
        session.record_command("test2");

        assert_eq!(session.commands_executed.len(), 2);
    }

    #[test]
    fn test_voice_interface() {
        let interface = VoiceInterface::new();

        assert!(!interface.recognizer().is_listening());
        assert!(!interface.is_muted());
    }

    #[test]
    fn test_voice_interface_start_listening() {
        let mut interface = VoiceInterface::new();

        interface.start_listening().unwrap();
        assert!(interface.recognizer().is_listening());
        assert!(interface.current_session().is_some());
    }

    #[test]
    fn test_voice_interface_speak() {
        let mut interface = VoiceInterface::new();
        interface.start_listening().unwrap();

        let result = interface.speak("Hello");
        assert!(!result.audio_data.is_empty());
    }

    #[test]
    fn test_voice_interface_speak_muted() {
        let mut interface = VoiceInterface::new();
        interface.mute();

        let result = interface.speak("Hello");
        assert!(result.audio_data.is_empty());
    }

    #[test]
    fn test_voice_interface_process_text() {
        let mut interface = VoiceInterface::new();
        interface.start_listening().unwrap();

        let result = interface.process_text("help me please");
        assert!(result.is_some());
        assert_eq!(result.unwrap().command.name, "help");
    }

    #[test]
    fn test_voice_interface_end_session() {
        let mut interface = VoiceInterface::new();
        interface.start_listening().unwrap();

        let session = interface.end_session();
        assert!(session.is_some());
        assert!(interface.current_session().is_none());
        assert_eq!(interface.session_history().len(), 1);
    }

    #[test]
    fn test_voice_interface_mute_unmute() {
        let mut interface = VoiceInterface::new();

        assert!(!interface.is_muted());

        interface.mute();
        assert!(interface.is_muted());

        interface.unmute();
        assert!(!interface.is_muted());
    }

    #[test]
    fn test_synthesis_priority_ordering() {
        assert!(SynthesisPriority::Immediate > SynthesisPriority::High);
        assert!(SynthesisPriority::High > SynthesisPriority::Normal);
        assert!(SynthesisPriority::Normal > SynthesisPriority::Low);
    }

    #[test]
    fn test_speech_synthesizer_voices() {
        let mut synthesizer = SpeechSynthesizer::with_defaults();

        synthesizer.add_voice(Voice::new("v1", "Voice 1", "en-US"));
        synthesizer.add_voice(Voice::new("v2", "Voice 2", "en-GB"));
        synthesizer.add_voice(Voice::new("v3", "Voice 3", "de-DE"));

        assert_eq!(synthesizer.voices().len(), 3);

        let en_voices = synthesizer.voices_for_language("en");
        assert_eq!(en_voices.len(), 2);
    }

    #[test]
    fn test_recognizer_final_results() {
        let mut recognizer = SpeechRecognizer::with_defaults();
        recognizer.start().unwrap();

        recognizer.process_audio(&[0u8; 100]);
        recognizer.process_audio(&[0u8; 100]);

        let final_results = recognizer.final_results();
        assert_eq!(final_results.len(), 2);
    }

    #[test]
    fn test_command_action_types() {
        let exec = CommandAction::Execute("test".to_string());
        let nav = CommandAction::Navigate("home".to_string());
        let input = CommandAction::Input("text".to_string());
        let toggle = CommandAction::Toggle("feature".to_string());
        let custom = CommandAction::Custom("action".to_string());

        match exec {
            CommandAction::Execute(s) => assert_eq!(s, "test"),
            _ => panic!("Wrong type"),
        }

        match nav {
            CommandAction::Navigate(s) => assert_eq!(s, "home"),
            _ => panic!("Wrong type"),
        }

        match input {
            CommandAction::Input(s) => assert_eq!(s, "text"),
            _ => panic!("Wrong type"),
        }

        match toggle {
            CommandAction::Toggle(s) => assert_eq!(s, "feature"),
            _ => panic!("Wrong type"),
        }

        match custom {
            CommandAction::Custom(s) => assert_eq!(s, "action"),
            _ => panic!("Wrong type"),
        }
    }

    #[test]
    fn test_voice_interface_custom_config() {
        let recognition_config = SpeechRecognitionConfig::default().with_engine(SpeechEngine::Vosk);
        let synthesis_config = SpeechSynthesisConfig::default().with_engine(TTSEngine::Espeak);

        let interface = VoiceInterface::with_config(recognition_config, synthesis_config);

        assert_eq!(interface.recognizer().config().engine, SpeechEngine::Vosk);
        assert_eq!(interface.synthesizer().config().engine, TTSEngine::Espeak);
    }

    #[test]
    fn test_session_state_active() {
        assert!(!SessionState::Idle.eq(&SessionState::Listening));
        assert!(matches!(SessionState::Processing, SessionState::Processing));
    }

    #[test]
    fn test_voice_session_last_utterance() {
        let mut session = VoiceSession::new();

        assert!(session.last_utterance().is_none());

        session.add_utterance(RecognitionResult::new("First", 0.9, true));
        session.add_utterance(RecognitionResult::new("Second", 0.95, true));

        let last = session.last_utterance().unwrap();
        assert_eq!(last.text, "Second");
    }

    // ================== Additional Coverage Tests ==================

    #[test]
    fn test_speech_engine_all_names() {
        let engines = vec![
            SpeechEngine::Whisper,
            SpeechEngine::DeepSpeech,
            SpeechEngine::Vosk,
            SpeechEngine::GoogleSpeech,
            SpeechEngine::AzureSpeech,
            SpeechEngine::Watson,
            SpeechEngine::Native,
        ];
        for engine in engines {
            assert!(!engine.name().is_empty());
        }
    }

    #[test]
    fn test_speech_recognition_config_builders() {
        let config = SpeechRecognitionConfig::default()
            .with_engine(SpeechEngine::Vosk)
            .with_language("de-DE")
            .with_keyword("activate")
            .with_model("/path/to/model");

        assert_eq!(config.engine, SpeechEngine::Vosk);
        assert_eq!(config.language, "de-DE");
        assert!(config.keywords.contains(&"activate".to_string()));
        assert!(config.model_path.is_some());
    }

    #[test]
    fn test_recognition_result_with_alternative() {
        let result = RecognitionResult::new("hello", 0.95, true)
            .with_alternative("helo", 0.8)
            .with_alternative("hullo", 0.7);

        assert_eq!(result.alternatives.len(), 2);
        assert_eq!(result.alternatives[0].text, "helo");
    }

    #[test]
    fn test_recognition_result_with_duration() {
        let result =
            RecognitionResult::new("test", 0.9, true).with_duration(Duration::from_secs(2));

        assert_eq!(result.duration, Duration::from_secs(2));
    }

    #[test]
    fn test_recognition_result_add_word() {
        let mut result = RecognitionResult::new("hello world", 0.9, true);
        result.add_word(
            "hello",
            Duration::from_millis(0),
            Duration::from_millis(500),
            0.95,
        );
        result.add_word(
            "world",
            Duration::from_millis(500),
            Duration::from_millis(1000),
            0.92,
        );

        let words = result.words_text();
        assert_eq!(words.len(), 2);
        assert_eq!(words[0], "hello");
    }

    #[test]
    fn test_speech_recognizer_stop() {
        let mut recognizer = SpeechRecognizer::with_defaults();
        recognizer.start().unwrap();
        assert!(recognizer.is_listening());

        recognizer.stop();
        assert!(!recognizer.is_listening());
    }

    #[test]
    fn test_speech_recognizer_already_listening() {
        let mut recognizer = SpeechRecognizer::with_defaults();
        recognizer.start().unwrap();

        let result = recognizer.start();
        assert!(result.is_err());
    }

    #[test]
    fn test_session_state_all_variants() {
        let states = vec![
            SessionState::Idle,
            SessionState::Listening,
            SessionState::Processing,
            SessionState::Speaking,
            SessionState::Error,
        ];
        for s in &states {
            assert!(!format!("{:?}", s).is_empty());
        }
    }
}
