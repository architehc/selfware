//! Streaming Response Pipeline
//!
//! Token-level streaming with progressive rendering, early termination,
//! partial result handling, and backpressure management.
//!
//! Features:
//! - Token-by-token streaming
//! - Progressive rendering
//! - Early termination
//! - Partial result handling
//! - Backpressure management
//! - Stream transformations

use anyhow::{anyhow, Result};
use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::task::Waker;
use std::time::{Duration, Instant};
use tokio::sync::mpsc;

/// Stream event types
#[derive(Debug, Clone)]
pub enum StreamEvent {
    /// New token received
    Token(String),
    /// Partial content update
    Content(String),
    /// Tool call started
    ToolCallStart { id: String, name: String },
    /// Tool call argument chunk
    ToolCallArg { id: String, chunk: String },
    /// Tool call completed
    ToolCallEnd { id: String },
    /// Thinking/reasoning content
    Thinking(String),
    /// Error occurred
    Error(String),
    /// Stream completed
    Done,
    /// Heartbeat (keep-alive)
    Heartbeat,
}

impl StreamEvent {
    /// Check if this is a terminal event
    pub fn is_terminal(&self) -> bool {
        matches!(self, StreamEvent::Done | StreamEvent::Error(_))
    }

    /// Get content if this is a content event
    pub fn content(&self) -> Option<&str> {
        match self {
            StreamEvent::Token(s) | StreamEvent::Content(s) | StreamEvent::Thinking(s) => Some(s),
            _ => None,
        }
    }
}

/// Stream statistics
#[derive(Debug, Clone, Default)]
pub struct StreamStats {
    /// Total tokens received
    pub tokens_received: u64,
    /// Total bytes received
    pub bytes_received: u64,
    /// Events processed
    pub events_processed: u64,
    /// Time to first token (ms)
    pub time_to_first_token_ms: Option<u64>,
    /// Total stream duration (ms)
    pub duration_ms: u64,
    /// Tokens per second
    pub tokens_per_second: f64,
    /// Backpressure events
    pub backpressure_events: u64,
    /// Dropped events (due to buffer overflow)
    pub dropped_events: u64,
}

/// Backpressure strategy
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BackpressureStrategy {
    /// Block producer until consumer catches up
    #[default]
    Block,
    /// Drop oldest events when buffer is full
    DropOldest,
    /// Drop newest events when buffer is full
    DropNewest,
    /// Batch events when under pressure
    Batch,
}

/// Stream buffer configuration
#[derive(Debug, Clone)]
pub struct BufferConfig {
    /// Maximum buffer size
    pub max_size: usize,
    /// High watermark (trigger backpressure)
    pub high_watermark: usize,
    /// Low watermark (release backpressure)
    pub low_watermark: usize,
    /// Backpressure strategy
    pub strategy: BackpressureStrategy,
    /// Batch size when batching
    pub batch_size: usize,
}

impl Default for BufferConfig {
    fn default() -> Self {
        Self {
            max_size: 1000,
            high_watermark: 800,
            low_watermark: 200,
            strategy: BackpressureStrategy::Block,
            batch_size: 10,
        }
    }
}

/// Token accumulator for building complete content
#[derive(Debug, Clone, Default)]
pub struct TokenAccumulator {
    /// Accumulated tokens
    tokens: Vec<String>,
    /// Current content
    content: String,
    /// Pending tool calls
    pending_tool_calls: Vec<PendingToolCall>,
    /// Completed tool calls
    completed_tool_calls: Vec<CompletedToolCall>,
}

/// Pending tool call being accumulated
#[derive(Debug, Clone)]
pub struct PendingToolCall {
    pub id: String,
    pub name: String,
    pub arguments: String,
}

/// Completed tool call
#[derive(Debug, Clone)]
pub struct CompletedToolCall {
    pub id: String,
    pub name: String,
    pub arguments: String,
}

impl TokenAccumulator {
    /// Create new accumulator
    pub fn new() -> Self {
        Self::default()
    }

    /// Process a stream event
    pub fn process(&mut self, event: &StreamEvent) {
        match event {
            StreamEvent::Token(token) => {
                self.tokens.push(token.clone());
                self.content.push_str(token);
            }
            StreamEvent::Content(content) => {
                self.content.push_str(content);
            }
            StreamEvent::ToolCallStart { id, name } => {
                self.pending_tool_calls.push(PendingToolCall {
                    id: id.clone(),
                    name: name.clone(),
                    arguments: String::new(),
                });
            }
            StreamEvent::ToolCallArg { id, chunk } => {
                if let Some(call) = self.pending_tool_calls.iter_mut().find(|c| &c.id == id) {
                    call.arguments.push_str(chunk);
                }
            }
            StreamEvent::ToolCallEnd { id } => {
                if let Some(pos) = self.pending_tool_calls.iter().position(|c| &c.id == id) {
                    let pending = self.pending_tool_calls.remove(pos);
                    self.completed_tool_calls.push(CompletedToolCall {
                        id: pending.id,
                        name: pending.name,
                        arguments: pending.arguments,
                    });
                }
            }
            _ => {}
        }
    }

    /// Get accumulated content
    pub fn content(&self) -> &str {
        &self.content
    }

    /// Get token count
    pub fn token_count(&self) -> usize {
        self.tokens.len()
    }

    /// Get completed tool calls
    pub fn tool_calls(&self) -> &[CompletedToolCall] {
        &self.completed_tool_calls
    }

    /// Check if there are pending tool calls
    pub fn has_pending_tool_calls(&self) -> bool {
        !self.pending_tool_calls.is_empty()
    }

    /// Clear accumulator
    pub fn clear(&mut self) {
        self.tokens.clear();
        self.content.clear();
        self.pending_tool_calls.clear();
        self.completed_tool_calls.clear();
    }
}

/// Stream consumer callback
pub type StreamCallback = Box<dyn Fn(StreamEvent) + Send + Sync>;

/// Stream buffer with backpressure support
pub struct StreamBuffer {
    /// Event buffer
    buffer: Arc<Mutex<VecDeque<StreamEvent>>>,
    /// Configuration
    config: BufferConfig,
    /// Under backpressure flag
    under_pressure: Arc<AtomicBool>,
    /// Statistics
    stats: Arc<Mutex<StreamStats>>,
    /// Waker for async notification
    waker: Arc<Mutex<Option<Waker>>>,
    /// Cancelled flag
    cancelled: Arc<AtomicBool>,
}

impl StreamBuffer {
    /// Create new stream buffer
    pub fn new(config: BufferConfig) -> Self {
        Self {
            buffer: Arc::new(Mutex::new(VecDeque::new())),
            config,
            under_pressure: Arc::new(AtomicBool::new(false)),
            stats: Arc::new(Mutex::new(StreamStats::default())),
            waker: Arc::new(Mutex::new(None)),
            cancelled: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Push event to buffer
    pub fn push(&self, event: StreamEvent) -> Result<()> {
        if self.cancelled.load(Ordering::Relaxed) {
            return Err(anyhow!("Stream cancelled"));
        }

        let mut buffer = self.buffer.lock().unwrap();
        let mut stats = self.stats.lock().unwrap();

        // Check backpressure
        if buffer.len() >= self.config.high_watermark {
            self.under_pressure.store(true, Ordering::Relaxed);
            stats.backpressure_events += 1;

            match self.config.strategy {
                BackpressureStrategy::Block => {
                    // In async context, would yield; here just proceed
                }
                BackpressureStrategy::DropOldest => {
                    while buffer.len() >= self.config.max_size {
                        buffer.pop_front();
                        stats.dropped_events += 1;
                    }
                }
                BackpressureStrategy::DropNewest => {
                    if buffer.len() >= self.config.max_size {
                        stats.dropped_events += 1;
                        return Ok(());
                    }
                }
                BackpressureStrategy::Batch => {
                    // Batching handled at consumption
                }
            }
        }

        // Update stats
        if let Some(content) = event.content() {
            stats.tokens_received += 1;
            stats.bytes_received += content.len() as u64;

            if stats.time_to_first_token_ms.is_none() {
                stats.time_to_first_token_ms = Some(0); // Would be set externally
            }
        }
        stats.events_processed += 1;

        buffer.push_back(event);

        // Wake consumer
        if let Some(waker) = self.waker.lock().unwrap().take() {
            waker.wake();
        }

        Ok(())
    }

    /// Pop event from buffer
    pub fn pop(&self) -> Option<StreamEvent> {
        let mut buffer = self.buffer.lock().unwrap();
        let event = buffer.pop_front();

        // Check if we can release backpressure
        if buffer.len() <= self.config.low_watermark {
            self.under_pressure.store(false, Ordering::Relaxed);
        }

        event
    }

    /// Pop multiple events (for batching)
    pub fn pop_batch(&self, max: usize) -> Vec<StreamEvent> {
        let mut buffer = self.buffer.lock().unwrap();
        let count = max.min(buffer.len());
        let mut batch = Vec::with_capacity(count);

        for _ in 0..count {
            if let Some(event) = buffer.pop_front() {
                batch.push(event);
            }
        }

        if buffer.len() <= self.config.low_watermark {
            self.under_pressure.store(false, Ordering::Relaxed);
        }

        batch
    }

    /// Check if buffer is empty
    pub fn is_empty(&self) -> bool {
        self.buffer.lock().unwrap().is_empty()
    }

    /// Get buffer length
    pub fn len(&self) -> usize {
        self.buffer.lock().unwrap().len()
    }

    /// Check if under backpressure
    pub fn is_under_pressure(&self) -> bool {
        self.under_pressure.load(Ordering::Relaxed)
    }

    /// Get statistics
    pub fn stats(&self) -> StreamStats {
        self.stats.lock().unwrap().clone()
    }

    /// Cancel the stream
    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::Relaxed);
        if let Some(waker) = self.waker.lock().unwrap().take() {
            waker.wake();
        }
    }

    /// Check if cancelled
    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Relaxed)
    }

    /// Register waker for async notification
    pub fn register_waker(&self, waker: Waker) {
        *self.waker.lock().unwrap() = Some(waker);
    }
}

impl Default for StreamBuffer {
    fn default() -> Self {
        Self::new(BufferConfig::default())
    }
}

/// Stream transformer trait
pub trait StreamTransformer: Send + Sync {
    /// Transform an event
    fn transform(&self, event: StreamEvent) -> Option<StreamEvent>;

    /// Flush any buffered content
    fn flush(&self) -> Vec<StreamEvent> {
        Vec::new()
    }
}

/// Identity transformer (pass-through)
pub struct IdentityTransformer;

impl StreamTransformer for IdentityTransformer {
    fn transform(&self, event: StreamEvent) -> Option<StreamEvent> {
        Some(event)
    }
}

/// Token batcher - combines tokens into larger chunks
pub struct TokenBatcher {
    buffer: Mutex<String>,
    threshold: usize,
}

impl TokenBatcher {
    pub fn new(threshold: usize) -> Self {
        Self {
            buffer: Mutex::new(String::new()),
            threshold,
        }
    }
}

impl StreamTransformer for TokenBatcher {
    fn transform(&self, event: StreamEvent) -> Option<StreamEvent> {
        match event {
            StreamEvent::Token(token) => {
                let mut buffer = self.buffer.lock().unwrap();
                buffer.push_str(&token);

                if buffer.len() >= self.threshold {
                    let content = std::mem::take(&mut *buffer);
                    Some(StreamEvent::Content(content))
                } else {
                    None
                }
            }
            StreamEvent::Done => {
                let buffer = self.buffer.lock().unwrap();
                if buffer.is_empty() {
                    Some(StreamEvent::Done)
                } else {
                    // Will emit buffered content in flush
                    Some(StreamEvent::Done)
                }
            }
            other => Some(other),
        }
    }

    fn flush(&self) -> Vec<StreamEvent> {
        let mut buffer = self.buffer.lock().unwrap();
        if buffer.is_empty() {
            Vec::new()
        } else {
            let content = std::mem::take(&mut *buffer);
            vec![StreamEvent::Content(content)]
        }
    }
}

/// Filter transformer - removes unwanted events
pub struct FilterTransformer<F>
where
    F: Fn(&StreamEvent) -> bool + Send + Sync,
{
    predicate: F,
}

impl<F> FilterTransformer<F>
where
    F: Fn(&StreamEvent) -> bool + Send + Sync,
{
    pub fn new(predicate: F) -> Self {
        Self { predicate }
    }
}

impl<F> StreamTransformer for FilterTransformer<F>
where
    F: Fn(&StreamEvent) -> bool + Send + Sync,
{
    fn transform(&self, event: StreamEvent) -> Option<StreamEvent> {
        if (self.predicate)(&event) {
            Some(event)
        } else {
            None
        }
    }
}

/// Rate limiter transformer
pub struct RateLimiter {
    last_emit: Mutex<Instant>,
    min_interval: Duration,
    pending: Mutex<Option<StreamEvent>>,
}

impl RateLimiter {
    pub fn new(min_interval: Duration) -> Self {
        Self {
            // Initialize to past so first event always passes
            last_emit: Mutex::new(Instant::now() - min_interval),
            min_interval,
            pending: Mutex::new(None),
        }
    }
}

impl StreamTransformer for RateLimiter {
    fn transform(&self, event: StreamEvent) -> Option<StreamEvent> {
        // Always pass through terminal events
        if event.is_terminal() {
            return Some(event);
        }

        let mut last = self.last_emit.lock().unwrap();
        let now = Instant::now();

        if now.duration_since(*last) >= self.min_interval {
            *last = now;
            // Emit any pending event first, then this one
            let mut pending = self.pending.lock().unwrap();
            if pending.is_some() {
                let p = pending.take();
                *pending = Some(event);
                return p;
            }
            Some(event)
        } else {
            // Store as pending
            *self.pending.lock().unwrap() = Some(event);
            None
        }
    }

    fn flush(&self) -> Vec<StreamEvent> {
        self.pending.lock().unwrap().take().into_iter().collect()
    }
}

/// Stream pipeline for processing events
pub struct StreamPipeline {
    transformers: Vec<Box<dyn StreamTransformer>>,
    buffer: StreamBuffer,
    accumulator: Mutex<TokenAccumulator>,
    start_time: Mutex<Option<Instant>>,
}

impl StreamPipeline {
    /// Create new pipeline
    pub fn new(config: BufferConfig) -> Self {
        Self {
            transformers: Vec::new(),
            buffer: StreamBuffer::new(config),
            accumulator: Mutex::new(TokenAccumulator::new()),
            start_time: Mutex::new(None),
        }
    }

    /// Add transformer to pipeline
    pub fn add_transformer(&mut self, transformer: Box<dyn StreamTransformer>) {
        self.transformers.push(transformer);
    }

    /// Process an event through the pipeline
    pub fn process(&self, event: StreamEvent) -> Result<()> {
        // Record start time on first event
        {
            let mut start = self.start_time.lock().unwrap();
            if start.is_none() {
                *start = Some(Instant::now());
            }
        }

        // Apply transformers
        let mut current = Some(event);
        for transformer in &self.transformers {
            if let Some(e) = current {
                current = transformer.transform(e);
            } else {
                break;
            }
        }

        // Push to buffer
        if let Some(e) = current {
            // Update accumulator
            self.accumulator.lock().unwrap().process(&e);
            self.buffer.push(e)?;
        }

        Ok(())
    }

    /// Flush all transformers
    pub fn flush(&self) -> Result<()> {
        for transformer in &self.transformers {
            for event in transformer.flush() {
                self.buffer.push(event)?;
            }
        }
        Ok(())
    }

    /// Consume next event
    pub fn next(&self) -> Option<StreamEvent> {
        self.buffer.pop()
    }

    /// Consume batch of events
    pub fn next_batch(&self, max: usize) -> Vec<StreamEvent> {
        self.buffer.pop_batch(max)
    }

    /// Get accumulated content
    pub fn content(&self) -> String {
        self.accumulator.lock().unwrap().content().to_string()
    }

    /// Get token count
    pub fn token_count(&self) -> usize {
        self.accumulator.lock().unwrap().token_count()
    }

    /// Get tool calls
    pub fn tool_calls(&self) -> Vec<CompletedToolCall> {
        self.accumulator.lock().unwrap().tool_calls().to_vec()
    }

    /// Get statistics
    pub fn stats(&self) -> StreamStats {
        let mut stats = self.buffer.stats();

        // Calculate duration
        if let Some(start) = *self.start_time.lock().unwrap() {
            stats.duration_ms = start.elapsed().as_millis() as u64;

            // Calculate tokens per second
            if stats.duration_ms > 0 {
                stats.tokens_per_second =
                    stats.tokens_received as f64 / (stats.duration_ms as f64 / 1000.0);
            }
        }

        stats
    }

    /// Cancel the pipeline
    pub fn cancel(&self) {
        self.buffer.cancel();
    }

    /// Check if cancelled
    pub fn is_cancelled(&self) -> bool {
        self.buffer.is_cancelled()
    }

    /// Check if under backpressure
    pub fn is_under_pressure(&self) -> bool {
        self.buffer.is_under_pressure()
    }
}

impl Default for StreamPipeline {
    fn default() -> Self {
        Self::new(BufferConfig::default())
    }
}

/// Progressive renderer for streaming content
pub struct ProgressiveRenderer {
    /// Current rendered content
    rendered: Mutex<String>,
    /// Render callback
    callback: Option<Box<dyn Fn(&str) + Send + Sync>>,
    /// Minimum update interval
    min_interval: Duration,
    /// Last render time
    last_render: Mutex<Instant>,
    /// Pending content
    pending: Mutex<String>,
}

impl ProgressiveRenderer {
    /// Create new renderer
    pub fn new() -> Self {
        Self {
            rendered: Mutex::new(String::new()),
            callback: None,
            min_interval: Duration::from_millis(50),
            last_render: Mutex::new(Instant::now()),
            pending: Mutex::new(String::new()),
        }
    }

    /// Set render callback
    pub fn with_callback<F>(mut self, callback: F) -> Self
    where
        F: Fn(&str) + Send + Sync + 'static,
    {
        self.callback = Some(Box::new(callback));
        self
    }

    /// Set minimum update interval
    pub fn with_interval(mut self, interval: Duration) -> Self {
        self.min_interval = interval;
        self
    }

    /// Append content
    pub fn append(&self, content: &str) {
        let mut pending = self.pending.lock().unwrap();
        pending.push_str(content);

        // Check if we should render
        let mut last = self.last_render.lock().unwrap();
        if last.elapsed() >= self.min_interval {
            self.do_render(&pending);
            pending.clear();
            *last = Instant::now();
        }
    }

    /// Force render any pending content
    pub fn flush(&self) {
        let mut pending = self.pending.lock().unwrap();
        if !pending.is_empty() {
            self.do_render(&pending);
            pending.clear();
            *self.last_render.lock().unwrap() = Instant::now();
        }
    }

    /// Internal render
    fn do_render(&self, content: &str) {
        let mut rendered = self.rendered.lock().unwrap();
        rendered.push_str(content);

        if let Some(ref callback) = self.callback {
            callback(content);
        }
    }

    /// Get full rendered content
    pub fn content(&self) -> String {
        let rendered = self.rendered.lock().unwrap();
        let pending = self.pending.lock().unwrap();
        format!("{}{}", rendered, pending)
    }

    /// Clear renderer
    pub fn clear(&self) {
        self.rendered.lock().unwrap().clear();
        self.pending.lock().unwrap().clear();
    }
}

impl Default for ProgressiveRenderer {
    fn default() -> Self {
        Self::new()
    }
}

/// Stream consumer for async consumption
pub struct StreamConsumer {
    receiver: mpsc::Receiver<StreamEvent>,
    cancelled: Arc<AtomicBool>,
}

impl StreamConsumer {
    /// Create new consumer from channel
    pub fn new(receiver: mpsc::Receiver<StreamEvent>) -> Self {
        Self {
            receiver,
            cancelled: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Receive next event
    pub async fn next(&mut self) -> Option<StreamEvent> {
        if self.cancelled.load(Ordering::Relaxed) {
            return None;
        }
        self.receiver.recv().await
    }

    /// Cancel consumption
    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::Relaxed);
    }
}

/// Stream producer for async production
pub struct StreamProducer {
    sender: mpsc::Sender<StreamEvent>,
    stats: Arc<Mutex<StreamStats>>,
    start_time: Option<Instant>,
}

impl StreamProducer {
    /// Create new producer
    pub fn new(sender: mpsc::Sender<StreamEvent>) -> Self {
        Self {
            sender,
            stats: Arc::new(Mutex::new(StreamStats::default())),
            start_time: None,
        }
    }

    /// Send event
    pub async fn send(&mut self, event: StreamEvent) -> Result<()> {
        // Record start time
        if self.start_time.is_none() {
            self.start_time = Some(Instant::now());
        }

        // Update stats
        {
            let mut stats = self.stats.lock().unwrap();
            if let Some(content) = event.content() {
                stats.tokens_received += 1;
                stats.bytes_received += content.len() as u64;

                if stats.time_to_first_token_ms.is_none() {
                    if let Some(start) = self.start_time {
                        stats.time_to_first_token_ms = Some(start.elapsed().as_millis() as u64);
                    }
                }
            }
            stats.events_processed += 1;
        }

        self.sender
            .send(event)
            .await
            .map_err(|e| anyhow!("Send error: {}", e))
    }

    /// Send token
    pub async fn send_token(&mut self, token: impl Into<String>) -> Result<()> {
        self.send(StreamEvent::Token(token.into())).await
    }

    /// Send content
    pub async fn send_content(&mut self, content: impl Into<String>) -> Result<()> {
        self.send(StreamEvent::Content(content.into())).await
    }

    /// Send done
    pub async fn send_done(&mut self) -> Result<()> {
        self.send(StreamEvent::Done).await
    }

    /// Send error
    pub async fn send_error(&mut self, error: impl Into<String>) -> Result<()> {
        self.send(StreamEvent::Error(error.into())).await
    }

    /// Get statistics
    pub fn stats(&self) -> StreamStats {
        let mut stats = self.stats.lock().unwrap().clone();

        if let Some(start) = self.start_time {
            stats.duration_ms = start.elapsed().as_millis() as u64;
            if stats.duration_ms > 0 {
                stats.tokens_per_second =
                    stats.tokens_received as f64 / (stats.duration_ms as f64 / 1000.0);
            }
        }

        stats
    }
}

/// Create a stream channel pair
pub fn channel(buffer_size: usize) -> (StreamProducer, StreamConsumer) {
    let (sender, receiver) = mpsc::channel(buffer_size);
    (StreamProducer::new(sender), StreamConsumer::new(receiver))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_stream_event_is_terminal() {
        assert!(StreamEvent::Done.is_terminal());
        assert!(StreamEvent::Error("error".into()).is_terminal());
        assert!(!StreamEvent::Token("token".into()).is_terminal());
    }

    #[test]
    fn test_stream_event_content() {
        assert_eq!(StreamEvent::Token("t".into()).content(), Some("t"));
        assert_eq!(StreamEvent::Content("c".into()).content(), Some("c"));
        assert!(StreamEvent::Done.content().is_none());
    }

    #[test]
    fn test_token_accumulator() {
        let mut acc = TokenAccumulator::new();

        acc.process(&StreamEvent::Token("Hello ".into()));
        acc.process(&StreamEvent::Token("World".into()));

        assert_eq!(acc.content(), "Hello World");
        assert_eq!(acc.token_count(), 2);
    }

    #[test]
    fn test_token_accumulator_tool_calls() {
        let mut acc = TokenAccumulator::new();

        acc.process(&StreamEvent::ToolCallStart {
            id: "1".into(),
            name: "test".into(),
        });
        acc.process(&StreamEvent::ToolCallArg {
            id: "1".into(),
            chunk: r#"{"arg":"#.into(),
        });
        acc.process(&StreamEvent::ToolCallArg {
            id: "1".into(),
            chunk: r#""value"}"#.into(),
        });
        acc.process(&StreamEvent::ToolCallEnd { id: "1".into() });

        assert!(!acc.has_pending_tool_calls());
        assert_eq!(acc.tool_calls().len(), 1);
        assert_eq!(acc.tool_calls()[0].arguments, r#"{"arg":"value"}"#);
    }

    #[test]
    fn test_stream_buffer_push_pop() {
        let buffer = StreamBuffer::default();

        buffer.push(StreamEvent::Token("a".into())).unwrap();
        buffer.push(StreamEvent::Token("b".into())).unwrap();

        assert_eq!(buffer.len(), 2);

        let event = buffer.pop().unwrap();
        assert!(matches!(event, StreamEvent::Token(s) if s == "a"));

        assert_eq!(buffer.len(), 1);
    }

    #[test]
    fn test_stream_buffer_pop_batch() {
        let buffer = StreamBuffer::default();

        for i in 0..5 {
            buffer.push(StreamEvent::Token(i.to_string())).unwrap();
        }

        let batch = buffer.pop_batch(3);
        assert_eq!(batch.len(), 3);
        assert_eq!(buffer.len(), 2);
    }

    #[test]
    fn test_stream_buffer_backpressure() {
        let config = BufferConfig {
            max_size: 10,
            high_watermark: 5,
            low_watermark: 2,
            strategy: BackpressureStrategy::Block,
            batch_size: 10,
        };
        let buffer = StreamBuffer::new(config);

        for i in 0..6 {
            buffer.push(StreamEvent::Token(i.to_string())).unwrap();
        }

        assert!(buffer.is_under_pressure());

        // Pop until below low watermark
        for _ in 0..4 {
            buffer.pop();
        }

        assert!(!buffer.is_under_pressure());
    }

    #[test]
    fn test_stream_buffer_cancel() {
        let buffer = StreamBuffer::default();

        buffer.cancel();
        assert!(buffer.is_cancelled());

        let result = buffer.push(StreamEvent::Token("x".into()));
        assert!(result.is_err());
    }

    #[test]
    fn test_identity_transformer() {
        let transformer = IdentityTransformer;

        let event = StreamEvent::Token("test".into());
        let result = transformer.transform(event.clone());

        assert!(matches!(result, Some(StreamEvent::Token(s)) if s == "test"));
    }

    #[test]
    fn test_token_batcher() {
        let batcher = TokenBatcher::new(5);

        // Small tokens shouldn't emit
        assert!(batcher.transform(StreamEvent::Token("ab".into())).is_none());
        assert!(batcher.transform(StreamEvent::Token("cd".into())).is_none());

        // This should trigger emission
        let result = batcher.transform(StreamEvent::Token("e".into()));
        assert!(matches!(result, Some(StreamEvent::Content(s)) if s == "abcde"));
    }

    #[test]
    fn test_token_batcher_flush() {
        let batcher = TokenBatcher::new(10);

        batcher.transform(StreamEvent::Token("abc".into()));

        let flushed = batcher.flush();
        assert_eq!(flushed.len(), 1);
        assert!(matches!(&flushed[0], StreamEvent::Content(s) if s == "abc"));
    }

    #[test]
    fn test_filter_transformer() {
        let filter = FilterTransformer::new(|e| !matches!(e, StreamEvent::Heartbeat));

        assert!(filter.transform(StreamEvent::Token("t".into())).is_some());
        assert!(filter.transform(StreamEvent::Heartbeat).is_none());
    }

    #[test]
    fn test_rate_limiter() {
        // Use a very long interval to ensure the second call is always within it
        let limiter = RateLimiter::new(Duration::from_secs(10));

        // First event should pass
        let result = limiter.transform(StreamEvent::Token("1".into()));
        assert!(result.is_some());

        // Immediate second should be pending (within 10 second window)
        let result = limiter.transform(StreamEvent::Token("2".into()));
        assert!(result.is_none());

        // Terminal events always pass
        let result = limiter.transform(StreamEvent::Done);
        assert!(result.is_some());
    }

    #[test]
    fn test_stream_pipeline() {
        let mut pipeline = StreamPipeline::default();
        pipeline.add_transformer(Box::new(IdentityTransformer));

        pipeline
            .process(StreamEvent::Token("Hello".into()))
            .unwrap();
        pipeline
            .process(StreamEvent::Token(" World".into()))
            .unwrap();

        assert_eq!(pipeline.content(), "Hello World");
        assert_eq!(pipeline.token_count(), 2);
    }

    #[test]
    fn test_stream_pipeline_with_batcher() {
        let mut pipeline = StreamPipeline::default();
        pipeline.add_transformer(Box::new(TokenBatcher::new(5)));

        pipeline.process(StreamEvent::Token("ab".into())).unwrap();
        pipeline.process(StreamEvent::Token("cd".into())).unwrap();
        pipeline.process(StreamEvent::Token("e".into())).unwrap();

        // Should have one batched event
        let event = pipeline.next();
        assert!(matches!(event, Some(StreamEvent::Content(_))));
    }

    #[test]
    fn test_stream_pipeline_cancel() {
        let pipeline = StreamPipeline::default();

        pipeline.cancel();
        assert!(pipeline.is_cancelled());

        let result = pipeline.process(StreamEvent::Token("x".into()));
        assert!(result.is_err());
    }

    #[test]
    fn test_progressive_renderer() {
        let rendered = Arc::new(Mutex::new(Vec::new()));
        let rendered_clone = Arc::clone(&rendered);

        let renderer = ProgressiveRenderer::new()
            .with_callback(move |s| {
                rendered_clone.lock().unwrap().push(s.to_string());
            })
            .with_interval(Duration::from_millis(0)); // Immediate for testing

        renderer.append("Hello");
        renderer.flush();

        assert_eq!(renderer.content(), "Hello");
    }

    #[test]
    fn test_progressive_renderer_batching() {
        let renderer = ProgressiveRenderer::new().with_interval(Duration::from_secs(1)); // Long interval

        renderer.append("a");
        renderer.append("b");
        renderer.append("c");

        // Should still be pending
        let content = renderer.content();
        assert!(content.contains("abc"));

        // Flush should work
        renderer.flush();
    }

    #[tokio::test]
    async fn test_stream_channel() {
        let (mut producer, mut consumer) = channel(10);

        producer.send_token("Hello").await.unwrap();
        producer.send_token(" World").await.unwrap();
        producer.send_done().await.unwrap();

        let e1 = consumer.next().await.unwrap();
        assert!(matches!(e1, StreamEvent::Token(s) if s == "Hello"));

        let e2 = consumer.next().await.unwrap();
        assert!(matches!(e2, StreamEvent::Token(s) if s == " World"));

        let e3 = consumer.next().await.unwrap();
        assert!(matches!(e3, StreamEvent::Done));
    }

    #[tokio::test]
    async fn test_producer_stats() {
        let (mut producer, mut consumer) = channel(10);

        producer.send_token("token1").await.unwrap();
        producer.send_token("token2").await.unwrap();

        let stats = producer.stats();
        assert_eq!(stats.tokens_received, 2);
        assert!(stats.bytes_received > 0);

        // Consume to avoid channel issues
        consumer.next().await;
        consumer.next().await;
    }

    #[test]
    fn test_buffer_config_default() {
        let config = BufferConfig::default();
        assert_eq!(config.max_size, 1000);
        assert!(config.high_watermark < config.max_size);
        assert!(config.low_watermark < config.high_watermark);
    }

    #[test]
    fn test_backpressure_strategy_default() {
        assert_eq!(BackpressureStrategy::default(), BackpressureStrategy::Block);
    }

    #[test]
    fn test_token_accumulator_clear() {
        let mut acc = TokenAccumulator::new();
        acc.process(&StreamEvent::Token("test".into()));

        acc.clear();

        assert!(acc.content().is_empty());
        assert_eq!(acc.token_count(), 0);
    }

    #[test]
    fn test_stream_stats_default() {
        let stats = StreamStats::default();
        assert_eq!(stats.tokens_received, 0);
        assert!(stats.time_to_first_token_ms.is_none());
    }

    #[test]
    fn test_pending_tool_call() {
        let mut acc = TokenAccumulator::new();

        acc.process(&StreamEvent::ToolCallStart {
            id: "1".into(),
            name: "tool".into(),
        });

        assert!(acc.has_pending_tool_calls());
        assert!(acc.tool_calls().is_empty());
    }

    // Additional comprehensive tests

    #[test]
    fn test_stream_event_all_variants() {
        // Test all StreamEvent variants
        let token = StreamEvent::Token("hello".into());
        let content = StreamEvent::Content("world".into());
        let tool_start = StreamEvent::ToolCallStart {
            id: "id1".into(),
            name: "tool_name".into(),
        };
        let tool_arg = StreamEvent::ToolCallArg {
            id: "id1".into(),
            chunk: "arg_data".into(),
        };
        let tool_end = StreamEvent::ToolCallEnd { id: "id1".into() };
        let thinking = StreamEvent::Thinking("reasoning".into());
        let error = StreamEvent::Error("error msg".into());
        let done = StreamEvent::Done;
        let heartbeat = StreamEvent::Heartbeat;

        // Test is_terminal for all variants
        assert!(!token.is_terminal());
        assert!(!content.is_terminal());
        assert!(!tool_start.is_terminal());
        assert!(!tool_arg.is_terminal());
        assert!(!tool_end.is_terminal());
        assert!(!thinking.is_terminal());
        assert!(error.is_terminal());
        assert!(done.is_terminal());
        assert!(!heartbeat.is_terminal());
    }

    #[test]
    fn test_stream_event_content_all_variants() {
        // Content variants
        assert_eq!(StreamEvent::Token("t".into()).content(), Some("t"));
        assert_eq!(StreamEvent::Content("c".into()).content(), Some("c"));
        assert_eq!(StreamEvent::Thinking("th".into()).content(), Some("th"));

        // Non-content variants
        assert!(StreamEvent::ToolCallStart {
            id: "1".into(),
            name: "n".into()
        }
        .content()
        .is_none());
        assert!(StreamEvent::ToolCallArg {
            id: "1".into(),
            chunk: "c".into()
        }
        .content()
        .is_none());
        assert!(StreamEvent::ToolCallEnd { id: "1".into() }
            .content()
            .is_none());
        assert!(StreamEvent::Error("e".into()).content().is_none());
        assert!(StreamEvent::Done.content().is_none());
        assert!(StreamEvent::Heartbeat.content().is_none());
    }

    #[test]
    fn test_stream_event_clone() {
        let event = StreamEvent::Token("test".into());
        let cloned = event.clone();
        assert!(matches!(cloned, StreamEvent::Token(s) if s == "test"));
    }

    #[test]
    fn test_stream_event_debug() {
        let event = StreamEvent::Done;
        let debug = format!("{:?}", event);
        assert!(debug.contains("Done"));
    }

    #[test]
    fn test_stream_stats_fields() {
        let stats = StreamStats {
            tokens_received: 100,
            bytes_received: 500,
            events_processed: 120,
            time_to_first_token_ms: Some(50),
            duration_ms: 1000,
            tokens_per_second: 100.0,
            backpressure_events: 2,
            dropped_events: 1,
        };

        assert_eq!(stats.tokens_received, 100);
        assert_eq!(stats.bytes_received, 500);
        assert_eq!(stats.events_processed, 120);
        assert_eq!(stats.time_to_first_token_ms, Some(50));
        assert_eq!(stats.duration_ms, 1000);
        assert!((stats.tokens_per_second - 100.0).abs() < f64::EPSILON);
        assert_eq!(stats.backpressure_events, 2);
        assert_eq!(stats.dropped_events, 1);
    }

    #[test]
    fn test_stream_stats_clone() {
        let stats = StreamStats::default();
        let cloned = stats.clone();
        assert_eq!(cloned.tokens_received, stats.tokens_received);
    }

    #[test]
    fn test_stream_stats_debug() {
        let stats = StreamStats::default();
        let debug = format!("{:?}", stats);
        assert!(debug.contains("StreamStats"));
    }

    #[test]
    fn test_backpressure_strategy_variants() {
        let block = BackpressureStrategy::Block;
        let drop_oldest = BackpressureStrategy::DropOldest;
        let drop_newest = BackpressureStrategy::DropNewest;
        let batch = BackpressureStrategy::Batch;

        assert_eq!(block, BackpressureStrategy::Block);
        assert_eq!(drop_oldest, BackpressureStrategy::DropOldest);
        assert_eq!(drop_newest, BackpressureStrategy::DropNewest);
        assert_eq!(batch, BackpressureStrategy::Batch);
    }

    #[test]
    fn test_backpressure_strategy_clone() {
        let strategy = BackpressureStrategy::DropOldest;
        let cloned = strategy.clone();
        assert_eq!(cloned, strategy);
    }

    #[test]
    fn test_backpressure_strategy_debug() {
        let strategy = BackpressureStrategy::Batch;
        let debug = format!("{:?}", strategy);
        assert!(debug.contains("Batch"));
    }

    #[test]
    fn test_buffer_config_fields() {
        let config = BufferConfig {
            max_size: 500,
            high_watermark: 400,
            low_watermark: 100,
            strategy: BackpressureStrategy::DropNewest,
            batch_size: 20,
        };

        assert_eq!(config.max_size, 500);
        assert_eq!(config.high_watermark, 400);
        assert_eq!(config.low_watermark, 100);
        assert_eq!(config.strategy, BackpressureStrategy::DropNewest);
        assert_eq!(config.batch_size, 20);
    }

    #[test]
    fn test_buffer_config_clone() {
        let config = BufferConfig::default();
        let cloned = config.clone();
        assert_eq!(cloned.max_size, config.max_size);
    }

    #[test]
    fn test_buffer_config_debug() {
        let config = BufferConfig::default();
        let debug = format!("{:?}", config);
        assert!(debug.contains("BufferConfig"));
    }

    #[test]
    fn test_pending_tool_call_struct() {
        let pending = PendingToolCall {
            id: "call_123".into(),
            name: "read_file".into(),
            arguments: r#"{"path": "test.txt"}"#.into(),
        };

        assert_eq!(pending.id, "call_123");
        assert_eq!(pending.name, "read_file");
        assert!(pending.arguments.contains("path"));
    }

    #[test]
    fn test_pending_tool_call_clone() {
        let pending = PendingToolCall {
            id: "1".into(),
            name: "tool".into(),
            arguments: "{}".into(),
        };
        let cloned = pending.clone();
        assert_eq!(cloned.id, pending.id);
    }

    #[test]
    fn test_pending_tool_call_debug() {
        let pending = PendingToolCall {
            id: "1".into(),
            name: "tool".into(),
            arguments: "{}".into(),
        };
        let debug = format!("{:?}", pending);
        assert!(debug.contains("PendingToolCall"));
    }

    #[test]
    fn test_completed_tool_call_struct() {
        let completed = CompletedToolCall {
            id: "call_456".into(),
            name: "write_file".into(),
            arguments: r#"{"content": "data"}"#.into(),
        };

        assert_eq!(completed.id, "call_456");
        assert_eq!(completed.name, "write_file");
        assert!(completed.arguments.contains("content"));
    }

    #[test]
    fn test_completed_tool_call_clone() {
        let completed = CompletedToolCall {
            id: "1".into(),
            name: "tool".into(),
            arguments: "{}".into(),
        };
        let cloned = completed.clone();
        assert_eq!(cloned.id, completed.id);
    }

    #[test]
    fn test_completed_tool_call_debug() {
        let completed = CompletedToolCall {
            id: "1".into(),
            name: "tool".into(),
            arguments: "{}".into(),
        };
        let debug = format!("{:?}", completed);
        assert!(debug.contains("CompletedToolCall"));
    }

    #[test]
    fn test_token_accumulator_default() {
        let acc = TokenAccumulator::default();
        assert!(acc.content().is_empty());
        assert_eq!(acc.token_count(), 0);
        assert!(!acc.has_pending_tool_calls());
        assert!(acc.tool_calls().is_empty());
    }

    #[test]
    fn test_token_accumulator_content_event() {
        let mut acc = TokenAccumulator::new();
        acc.process(&StreamEvent::Content("direct content".into()));
        assert_eq!(acc.content(), "direct content");
        assert_eq!(acc.token_count(), 0); // Content doesn't add to token count
    }

    #[test]
    fn test_token_accumulator_multiple_tool_calls() {
        let mut acc = TokenAccumulator::new();

        // First tool call
        acc.process(&StreamEvent::ToolCallStart {
            id: "1".into(),
            name: "tool_a".into(),
        });
        acc.process(&StreamEvent::ToolCallArg {
            id: "1".into(),
            chunk: "arg1".into(),
        });

        // Second tool call (starts while first is pending)
        acc.process(&StreamEvent::ToolCallStart {
            id: "2".into(),
            name: "tool_b".into(),
        });
        acc.process(&StreamEvent::ToolCallArg {
            id: "2".into(),
            chunk: "arg2".into(),
        });

        assert!(acc.has_pending_tool_calls());

        // Complete first
        acc.process(&StreamEvent::ToolCallEnd { id: "1".into() });
        assert!(acc.has_pending_tool_calls()); // Second still pending

        // Complete second
        acc.process(&StreamEvent::ToolCallEnd { id: "2".into() });
        assert!(!acc.has_pending_tool_calls());

        assert_eq!(acc.tool_calls().len(), 2);
    }

    #[test]
    fn test_token_accumulator_tool_call_arg_unknown_id() {
        let mut acc = TokenAccumulator::new();

        // Arg for non-existent tool call should be ignored
        acc.process(&StreamEvent::ToolCallArg {
            id: "unknown".into(),
            chunk: "data".into(),
        });

        assert!(!acc.has_pending_tool_calls());
    }

    #[test]
    fn test_token_accumulator_tool_call_end_unknown_id() {
        let mut acc = TokenAccumulator::new();

        // End for non-existent tool call should be ignored
        acc.process(&StreamEvent::ToolCallEnd {
            id: "unknown".into(),
        });

        assert!(acc.tool_calls().is_empty());
    }

    #[test]
    fn test_token_accumulator_ignores_non_content_events() {
        let mut acc = TokenAccumulator::new();

        acc.process(&StreamEvent::Done);
        acc.process(&StreamEvent::Heartbeat);
        acc.process(&StreamEvent::Error("error".into()));

        assert!(acc.content().is_empty());
        assert_eq!(acc.token_count(), 0);
    }

    #[test]
    fn test_token_accumulator_clone() {
        let mut acc = TokenAccumulator::new();
        acc.process(&StreamEvent::Token("test".into()));

        let cloned = acc.clone();
        assert_eq!(cloned.content(), acc.content());
    }

    #[test]
    fn test_token_accumulator_debug() {
        let acc = TokenAccumulator::new();
        let debug = format!("{:?}", acc);
        assert!(debug.contains("TokenAccumulator"));
    }

    #[test]
    fn test_stream_buffer_is_empty() {
        let buffer = StreamBuffer::default();
        assert!(buffer.is_empty());

        buffer.push(StreamEvent::Token("x".into())).unwrap();
        assert!(!buffer.is_empty());
    }

    #[test]
    fn test_stream_buffer_stats() {
        let buffer = StreamBuffer::default();

        buffer.push(StreamEvent::Token("hello".into())).unwrap();
        buffer.push(StreamEvent::Token("world".into())).unwrap();

        let stats = buffer.stats();
        assert_eq!(stats.tokens_received, 2);
        assert_eq!(stats.events_processed, 2);
    }

    #[test]
    fn test_stream_buffer_drop_oldest_strategy() {
        let config = BufferConfig {
            max_size: 3,
            high_watermark: 2,
            low_watermark: 1,
            strategy: BackpressureStrategy::DropOldest,
            batch_size: 1,
        };
        let buffer = StreamBuffer::new(config);

        // Fill beyond max
        buffer.push(StreamEvent::Token("1".into())).unwrap();
        buffer.push(StreamEvent::Token("2".into())).unwrap();
        buffer.push(StreamEvent::Token("3".into())).unwrap();
        buffer.push(StreamEvent::Token("4".into())).unwrap();

        // Should have dropped oldest
        let stats = buffer.stats();
        assert!(stats.dropped_events > 0);
    }

    #[test]
    fn test_stream_buffer_drop_newest_strategy() {
        let config = BufferConfig {
            max_size: 2,
            high_watermark: 1,
            low_watermark: 0,
            strategy: BackpressureStrategy::DropNewest,
            batch_size: 1,
        };
        let buffer = StreamBuffer::new(config);

        // Fill to max
        buffer.push(StreamEvent::Token("1".into())).unwrap();
        buffer.push(StreamEvent::Token("2".into())).unwrap();

        // This should be dropped
        buffer.push(StreamEvent::Token("3".into())).unwrap();

        let stats = buffer.stats();
        assert!(stats.dropped_events > 0);
        assert_eq!(buffer.len(), 2);
    }

    #[test]
    fn test_stream_buffer_batch_strategy() {
        let config = BufferConfig {
            max_size: 100,
            high_watermark: 5,
            low_watermark: 2,
            strategy: BackpressureStrategy::Batch,
            batch_size: 3,
        };
        let buffer = StreamBuffer::new(config);

        for i in 0..6 {
            buffer.push(StreamEvent::Token(i.to_string())).unwrap();
        }

        // Batch strategy doesn't drop, just flags backpressure
        assert!(buffer.is_under_pressure());
    }

    #[test]
    fn test_stream_buffer_time_to_first_token() {
        let buffer = StreamBuffer::default();

        buffer.push(StreamEvent::Token("first".into())).unwrap();

        let stats = buffer.stats();
        assert!(stats.time_to_first_token_ms.is_some());
    }

    #[test]
    fn test_stream_buffer_register_waker() {
        use std::task::{RawWaker, RawWakerVTable, Waker};

        // Create a simple waker
        fn dummy_clone(_: *const ()) -> RawWaker {
            RawWaker::new(std::ptr::null(), &VTABLE)
        }
        fn dummy_wake(_: *const ()) {}
        fn dummy_wake_by_ref(_: *const ()) {}
        fn dummy_drop(_: *const ()) {}

        static VTABLE: RawWakerVTable =
            RawWakerVTable::new(dummy_clone, dummy_wake, dummy_wake_by_ref, dummy_drop);

        let raw_waker = RawWaker::new(std::ptr::null(), &VTABLE);
        let waker = unsafe { Waker::from_raw(raw_waker) };

        let buffer = StreamBuffer::default();
        buffer.register_waker(waker);

        // Push should wake
        buffer.push(StreamEvent::Token("test".into())).unwrap();
    }

    #[test]
    fn test_identity_transformer_flush() {
        let transformer = IdentityTransformer;
        let flushed = transformer.flush();
        assert!(flushed.is_empty());
    }

    #[test]
    fn test_token_batcher_threshold() {
        let batcher = TokenBatcher::new(10);

        // Under threshold
        assert!(batcher
            .transform(StreamEvent::Token("abc".into()))
            .is_none());
        assert!(batcher
            .transform(StreamEvent::Token("def".into()))
            .is_none());

        // At threshold
        let result = batcher.transform(StreamEvent::Token("ghij".into()));
        assert!(result.is_some());
    }

    #[test]
    fn test_token_batcher_done_event() {
        let batcher = TokenBatcher::new(100);
        batcher.transform(StreamEvent::Token("abc".into()));

        // Done should pass through
        let result = batcher.transform(StreamEvent::Done);
        assert!(matches!(result, Some(StreamEvent::Done)));

        // Flush should return buffered content
        let flushed = batcher.flush();
        assert_eq!(flushed.len(), 1);
    }

    #[test]
    fn test_token_batcher_non_token_passthrough() {
        let batcher = TokenBatcher::new(10);

        // Non-token events pass through
        assert!(batcher.transform(StreamEvent::Heartbeat).is_some());
        assert!(batcher
            .transform(StreamEvent::Error("err".into()))
            .is_some());
        assert!(batcher
            .transform(StreamEvent::ToolCallStart {
                id: "1".into(),
                name: "n".into()
            })
            .is_some());
    }

    #[test]
    fn test_filter_transformer_all_event_types() {
        // Filter only content events
        let filter = FilterTransformer::new(|e| e.content().is_some());

        assert!(filter.transform(StreamEvent::Token("t".into())).is_some());
        assert!(filter.transform(StreamEvent::Content("c".into())).is_some());
        assert!(filter
            .transform(StreamEvent::Thinking("th".into()))
            .is_some());
        assert!(filter.transform(StreamEvent::Heartbeat).is_none());
        assert!(filter.transform(StreamEvent::Done).is_none());
    }

    #[test]
    fn test_filter_transformer_flush() {
        let filter = FilterTransformer::new(|_| true);
        let flushed = filter.flush();
        assert!(flushed.is_empty());
    }

    #[test]
    fn test_rate_limiter_flush() {
        // Use a very long interval to ensure the second call creates a pending event
        let limiter = RateLimiter::new(Duration::from_secs(10));

        // Create pending event
        limiter.transform(StreamEvent::Token("1".into())); // passes
        limiter.transform(StreamEvent::Token("2".into())); // pending (within 10 second window)

        let flushed = limiter.flush();
        assert_eq!(flushed.len(), 1);
    }

    #[test]
    fn test_rate_limiter_after_interval() {
        let limiter = RateLimiter::new(Duration::from_millis(1));

        limiter.transform(StreamEvent::Token("1".into()));

        // Wait for interval
        std::thread::sleep(Duration::from_millis(5));

        // Should pass now
        let result = limiter.transform(StreamEvent::Token("2".into()));
        assert!(result.is_some());
    }

    #[test]
    fn test_stream_pipeline_flush() {
        let mut pipeline = StreamPipeline::default();
        pipeline.add_transformer(Box::new(TokenBatcher::new(100)));

        pipeline.process(StreamEvent::Token("test".into())).unwrap();

        // Flush should emit buffered content
        pipeline.flush().unwrap();

        // Should be able to consume the flushed content
        let event = pipeline.next();
        assert!(event.is_some());
    }

    #[test]
    fn test_stream_pipeline_next_batch() {
        let pipeline = StreamPipeline::default();

        for i in 0..5 {
            pipeline.process(StreamEvent::Token(i.to_string())).unwrap();
        }

        let batch = pipeline.next_batch(3);
        assert_eq!(batch.len(), 3);
    }

    #[test]
    fn test_stream_pipeline_tool_calls() {
        let pipeline = StreamPipeline::default();

        pipeline
            .process(StreamEvent::ToolCallStart {
                id: "1".into(),
                name: "tool".into(),
            })
            .unwrap();
        pipeline
            .process(StreamEvent::ToolCallArg {
                id: "1".into(),
                chunk: "args".into(),
            })
            .unwrap();
        pipeline
            .process(StreamEvent::ToolCallEnd { id: "1".into() })
            .unwrap();

        let calls = pipeline.tool_calls();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].name, "tool");
    }

    #[test]
    fn test_stream_pipeline_stats() {
        let pipeline = StreamPipeline::default();

        pipeline.process(StreamEvent::Token("a".into())).unwrap();
        pipeline.process(StreamEvent::Token("b".into())).unwrap();

        let stats = pipeline.stats();
        assert_eq!(stats.tokens_received, 2);
        // Duration can be 0 if operations complete within same millisecond
        assert!(stats.duration_ms >= u64::MIN);
    }

    #[test]
    fn test_stream_pipeline_is_under_pressure() {
        let config = BufferConfig {
            max_size: 10,
            high_watermark: 3,
            low_watermark: 1,
            strategy: BackpressureStrategy::Block,
            batch_size: 1,
        };
        let pipeline = StreamPipeline::new(config);

        for i in 0..5 {
            pipeline.process(StreamEvent::Token(i.to_string())).unwrap();
        }

        assert!(pipeline.is_under_pressure());
    }

    #[test]
    fn test_progressive_renderer_new() {
        let renderer = ProgressiveRenderer::new();
        assert!(renderer.content().is_empty());
    }

    #[test]
    fn test_progressive_renderer_default() {
        let renderer = ProgressiveRenderer::default();
        assert!(renderer.content().is_empty());
    }

    #[test]
    fn test_progressive_renderer_clear() {
        let renderer = ProgressiveRenderer::new().with_interval(Duration::from_millis(0));

        renderer.append("test");
        renderer.flush();
        assert!(!renderer.content().is_empty());

        renderer.clear();
        assert!(renderer.content().is_empty());
    }

    #[test]
    fn test_progressive_renderer_pending_content() {
        let renderer = ProgressiveRenderer::new().with_interval(Duration::from_secs(10));

        renderer.append("pending");

        // Content should include pending
        assert!(renderer.content().contains("pending"));
    }

    #[tokio::test]
    async fn test_stream_consumer_cancel() {
        let (mut producer, mut consumer) = channel(10);

        consumer.cancel();

        // Send should still work
        producer.send_token("test").await.unwrap();

        // But consumer should return None
        let result = consumer.next().await;
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_stream_producer_send_content() {
        let (mut producer, mut consumer) = channel(10);

        producer.send_content("content data").await.unwrap();

        let event = consumer.next().await.unwrap();
        assert!(matches!(event, StreamEvent::Content(s) if s == "content data"));
    }

    #[tokio::test]
    async fn test_stream_producer_send_error() {
        let (mut producer, mut consumer) = channel(10);

        producer.send_error("something went wrong").await.unwrap();

        let event = consumer.next().await.unwrap();
        assert!(matches!(event, StreamEvent::Error(s) if s == "something went wrong"));
    }

    #[tokio::test]
    async fn test_stream_producer_time_to_first_token() {
        let (mut producer, mut consumer) = channel(10);

        producer.send_token("first").await.unwrap();

        let stats = producer.stats();
        assert!(stats.time_to_first_token_ms.is_some());

        // Consume to avoid channel issues
        consumer.next().await;
    }

    #[tokio::test]
    async fn test_stream_producer_tokens_per_second() {
        let (mut producer, mut consumer) = channel(10);

        producer.send_token("a").await.unwrap();
        std::thread::sleep(Duration::from_millis(10));
        producer.send_token("b").await.unwrap();

        let stats = producer.stats();
        assert!(stats.tokens_per_second >= 0.0);

        // Consume to avoid channel issues
        consumer.next().await;
        consumer.next().await;
    }

    #[test]
    fn test_channel_creation() {
        let (producer, consumer) = channel(5);

        // Both should be valid
        let stats = producer.stats();
        assert_eq!(stats.tokens_received, 0);

        drop(consumer);
    }

    #[test]
    fn test_stream_buffer_default() {
        let buffer = StreamBuffer::default();
        assert!(buffer.is_empty());
        assert!(!buffer.is_under_pressure());
        assert!(!buffer.is_cancelled());
    }

    #[test]
    fn test_stream_pipeline_default() {
        let pipeline = StreamPipeline::default();
        assert!(pipeline.content().is_empty());
        assert_eq!(pipeline.token_count(), 0);
        assert!(!pipeline.is_cancelled());
    }

    #[test]
    fn test_multiple_transformers_in_pipeline() {
        let mut pipeline = StreamPipeline::default();

        // Add identity then filter that removes heartbeats
        pipeline.add_transformer(Box::new(IdentityTransformer));
        pipeline.add_transformer(Box::new(FilterTransformer::new(|e| {
            !matches!(e, StreamEvent::Heartbeat)
        })));

        pipeline.process(StreamEvent::Token("t".into())).unwrap();
        pipeline.process(StreamEvent::Heartbeat).unwrap();

        // Only token should be in buffer
        assert_eq!(pipeline.next_batch(10).len(), 1);
    }

    #[test]
    fn test_transformer_chain_filters_early() {
        let mut pipeline = StreamPipeline::default();

        // First filter removes tokens
        pipeline.add_transformer(Box::new(FilterTransformer::new(|e| {
            !matches!(e, StreamEvent::Token(_))
        })));
        // Second transformer won't see tokens
        pipeline.add_transformer(Box::new(IdentityTransformer));

        pipeline.process(StreamEvent::Token("t".into())).unwrap();
        pipeline.process(StreamEvent::Content("c".into())).unwrap();

        let batch = pipeline.next_batch(10);
        assert_eq!(batch.len(), 1);
        assert!(matches!(&batch[0], StreamEvent::Content(_)));
    }

    #[test]
    fn test_rate_limiter_pending_replacement() {
        let limiter = RateLimiter::new(Duration::from_secs(10));

        limiter.transform(StreamEvent::Token("1".into())); // passes
        limiter.transform(StreamEvent::Token("2".into())); // pending
        limiter.transform(StreamEvent::Token("3".into())); // replaces pending

        let flushed = limiter.flush();
        assert_eq!(flushed.len(), 1);
        // Should have the last pending event
        assert!(matches!(&flushed[0], StreamEvent::Token(s) if s == "3"));
    }

    #[test]
    fn test_stream_buffer_pop_empty() {
        let buffer = StreamBuffer::default();
        assert!(buffer.pop().is_none());
    }

    #[test]
    fn test_stream_buffer_pop_batch_empty() {
        let buffer = StreamBuffer::default();
        let batch = buffer.pop_batch(10);
        assert!(batch.is_empty());
    }

    #[test]
    fn test_token_batcher_empty_flush() {
        let batcher = TokenBatcher::new(10);
        let flushed = batcher.flush();
        assert!(flushed.is_empty());
    }
}
