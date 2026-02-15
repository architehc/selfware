//! Communication Bridges
//!
//! Integrations with various communication platforms for agent notifications,
//! interactions, and collaborative workflows. Supports Slack, Discord, Teams,
//! Email, and webhooks.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::SystemTime;

static MESSAGE_COUNTER: AtomicU64 = AtomicU64::new(1);
static CHANNEL_COUNTER: AtomicU64 = AtomicU64::new(1);
static THREAD_COUNTER: AtomicU64 = AtomicU64::new(1);

fn generate_message_id() -> String {
    format!("msg-{}", MESSAGE_COUNTER.fetch_add(1, Ordering::SeqCst))
}

fn generate_channel_id() -> String {
    format!("ch-{}", CHANNEL_COUNTER.fetch_add(1, Ordering::SeqCst))
}

fn generate_thread_id() -> String {
    format!("thr-{}", THREAD_COUNTER.fetch_add(1, Ordering::SeqCst))
}

// ============================================================================
// Platform Types
// ============================================================================

/// Supported communication platforms
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Platform {
    Slack,
    Discord,
    Teams,
    Email,
    Webhook,
    Telegram,
    Matrix,
    Custom,
}

impl Platform {
    /// Get platform display name
    pub fn name(&self) -> &'static str {
        match self {
            Self::Slack => "Slack",
            Self::Discord => "Discord",
            Self::Teams => "Microsoft Teams",
            Self::Email => "Email",
            Self::Webhook => "Webhook",
            Self::Telegram => "Telegram",
            Self::Matrix => "Matrix",
            Self::Custom => "Custom",
        }
    }

    /// Check if platform supports threading
    pub fn supports_threads(&self) -> bool {
        matches!(
            self,
            Self::Slack | Self::Discord | Self::Email | Self::Matrix
        )
    }

    /// Check if platform supports reactions
    pub fn supports_reactions(&self) -> bool {
        matches!(
            self,
            Self::Slack | Self::Discord | Self::Teams | Self::Matrix
        )
    }

    /// Check if platform supports rich formatting
    pub fn supports_rich_formatting(&self) -> bool {
        matches!(
            self,
            Self::Slack | Self::Discord | Self::Teams | Self::Email | Self::Matrix
        )
    }

    /// Check if platform supports file attachments
    pub fn supports_attachments(&self) -> bool {
        matches!(
            self,
            Self::Slack | Self::Discord | Self::Teams | Self::Email | Self::Telegram | Self::Matrix
        )
    }
}

// ============================================================================
// Message Types
// ============================================================================

/// Message priority levels
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default)]
pub enum MessagePriority {
    Low,
    #[default]
    Normal,
    High,
    Urgent,
}

/// Message format types
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum MessageFormat {
    PlainText,
    #[default]
    Markdown,
    Html,
    SlackBlocks,
    DiscordEmbed,
    TeamsAdaptiveCard,
}

/// Message content
#[derive(Debug, Clone)]
pub struct MessageContent {
    pub text: String,
    pub format: MessageFormat,
    pub attachments: Vec<Attachment>,
    pub mentions: Vec<Mention>,
    pub metadata: HashMap<String, String>,
}

impl MessageContent {
    /// Create plain text message
    pub fn text(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            format: MessageFormat::PlainText,
            attachments: Vec::new(),
            mentions: Vec::new(),
            metadata: HashMap::new(),
        }
    }

    /// Create markdown message
    pub fn markdown(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            format: MessageFormat::Markdown,
            attachments: Vec::new(),
            mentions: Vec::new(),
            metadata: HashMap::new(),
        }
    }

    /// Add attachment
    pub fn with_attachment(mut self, attachment: Attachment) -> Self {
        self.attachments.push(attachment);
        self
    }

    /// Add mention
    pub fn with_mention(mut self, mention: Mention) -> Self {
        self.mentions.push(mention);
        self
    }

    /// Add metadata
    pub fn with_metadata(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.metadata.insert(key.into(), value.into());
        self
    }

    /// Convert to platform-specific format
    pub fn format_for(&self, platform: Platform) -> String {
        match (platform, self.format) {
            (Platform::Slack, MessageFormat::Markdown) => self.to_slack_mrkdwn(),
            (Platform::Discord, MessageFormat::Markdown) => self.to_discord_markdown(),
            (Platform::Teams, MessageFormat::Markdown) => self.to_teams_markdown(),
            (Platform::Email, MessageFormat::Markdown) => self.to_html(),
            _ => self.text.clone(),
        }
    }

    /// Convert markdown to Slack mrkdwn
    fn to_slack_mrkdwn(&self) -> String {
        let mut text = self.text.clone();
        // Convert **bold** to *bold*
        text = text.replace("**", "*");
        // Convert `code` stays the same
        // Convert [link](url) stays the same for Slack
        text
    }

    /// Convert to Discord markdown
    fn to_discord_markdown(&self) -> String {
        // Discord uses standard markdown
        self.text.clone()
    }

    /// Convert to Teams markdown
    fn to_teams_markdown(&self) -> String {
        // Teams uses a subset of markdown
        self.text.clone()
    }

    /// Convert markdown to HTML
    fn to_html(&self) -> String {
        let mut html = self.text.clone();
        // Simple conversions
        html = html.replace("**", "<strong>");
        html = html.replace("*", "<em>");
        html = html.replace("`", "<code>");
        format!("<p>{}</p>", html)
    }
}

/// File attachment
#[derive(Debug, Clone)]
pub struct Attachment {
    pub name: String,
    pub mime_type: String,
    pub size: usize,
    pub url: Option<String>,
    pub data: Option<Vec<u8>>,
}

impl Attachment {
    /// Create attachment from URL
    pub fn from_url(
        name: impl Into<String>,
        url: impl Into<String>,
        mime_type: impl Into<String>,
    ) -> Self {
        Self {
            name: name.into(),
            mime_type: mime_type.into(),
            size: 0,
            url: Some(url.into()),
            data: None,
        }
    }

    /// Create attachment from data
    pub fn from_data(name: impl Into<String>, data: Vec<u8>, mime_type: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            mime_type: mime_type.into(),
            size: data.len(),
            url: None,
            data: Some(data),
        }
    }
}

/// User mention
#[derive(Debug, Clone)]
pub struct Mention {
    pub user_id: String,
    pub display_name: String,
    pub mention_type: MentionType,
}

/// Types of mentions
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MentionType {
    User,
    Channel,
    Group,
    Everyone,
    Here,
}

impl Mention {
    /// Create user mention
    pub fn user(user_id: impl Into<String>, display_name: impl Into<String>) -> Self {
        Self {
            user_id: user_id.into(),
            display_name: display_name.into(),
            mention_type: MentionType::User,
        }
    }

    /// Create channel mention
    pub fn channel(channel_id: impl Into<String>, channel_name: impl Into<String>) -> Self {
        Self {
            user_id: channel_id.into(),
            display_name: channel_name.into(),
            mention_type: MentionType::Channel,
        }
    }

    /// Format mention for platform
    pub fn format_for(&self, platform: Platform) -> String {
        match (platform, self.mention_type) {
            (Platform::Slack, MentionType::User) => format!("<@{}>", self.user_id),
            (Platform::Slack, MentionType::Channel) => format!("<#{}>", self.user_id),
            (Platform::Slack, MentionType::Everyone) => "<!everyone>".to_string(),
            (Platform::Slack, MentionType::Here) => "<!here>".to_string(),
            (Platform::Discord, MentionType::User) => format!("<@{}>", self.user_id),
            (Platform::Discord, MentionType::Channel) => format!("<#{}>", self.user_id),
            (Platform::Discord, MentionType::Everyone) => "@everyone".to_string(),
            (Platform::Discord, MentionType::Here) => "@here".to_string(),
            (Platform::Teams, MentionType::User) => format!("<at>{}</at>", self.display_name),
            (Platform::Email, _) => self.display_name.clone(),
            _ => self.display_name.clone(),
        }
    }
}

// ============================================================================
// Channel Types
// ============================================================================

/// Communication channel
#[derive(Debug, Clone)]
pub struct Channel {
    pub id: String,
    pub platform: Platform,
    pub name: String,
    pub channel_type: ChannelType,
    pub external_id: Option<String>,
    pub webhook_url: Option<String>,
    pub metadata: HashMap<String, String>,
}

/// Channel types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChannelType {
    Public,
    Private,
    DirectMessage,
    Group,
}

impl Channel {
    /// Create new channel
    pub fn new(platform: Platform, name: impl Into<String>, channel_type: ChannelType) -> Self {
        Self {
            id: generate_channel_id(),
            platform,
            name: name.into(),
            channel_type,
            external_id: None,
            webhook_url: None,
            metadata: HashMap::new(),
        }
    }

    /// Set external ID (platform-specific channel ID)
    pub fn with_external_id(mut self, id: impl Into<String>) -> Self {
        self.external_id = Some(id.into());
        self
    }

    /// Set webhook URL
    pub fn with_webhook(mut self, url: impl Into<String>) -> Self {
        self.webhook_url = Some(url.into());
        self
    }

    /// Add metadata
    pub fn with_metadata(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.metadata.insert(key.into(), value.into());
        self
    }
}

// ============================================================================
// Message Thread
// ============================================================================

/// Message thread for conversations
#[derive(Debug, Clone)]
pub struct MessageThread {
    pub id: String,
    pub channel_id: String,
    pub parent_message_id: Option<String>,
    pub subject: Option<String>,
    pub messages: Vec<Message>,
    pub created_at: SystemTime,
    pub updated_at: SystemTime,
}

impl MessageThread {
    /// Create new thread
    pub fn new(channel_id: impl Into<String>) -> Self {
        let now = SystemTime::now();
        Self {
            id: generate_thread_id(),
            channel_id: channel_id.into(),
            parent_message_id: None,
            subject: None,
            messages: Vec::new(),
            created_at: now,
            updated_at: now,
        }
    }

    /// Set parent message
    pub fn with_parent(mut self, message_id: impl Into<String>) -> Self {
        self.parent_message_id = Some(message_id.into());
        self
    }

    /// Set subject
    pub fn with_subject(mut self, subject: impl Into<String>) -> Self {
        self.subject = Some(subject.into());
        self
    }

    /// Add message to thread
    pub fn add_message(&mut self, message: Message) {
        self.messages.push(message);
        self.updated_at = SystemTime::now();
    }

    /// Get message count
    pub fn message_count(&self) -> usize {
        self.messages.len()
    }

    /// Get latest message
    pub fn latest_message(&self) -> Option<&Message> {
        self.messages.last()
    }
}

// ============================================================================
// Message
// ============================================================================

/// A message sent or received
#[derive(Debug, Clone)]
pub struct Message {
    pub id: String,
    pub channel_id: String,
    pub thread_id: Option<String>,
    pub sender: MessageSender,
    pub content: MessageContent,
    pub priority: MessagePriority,
    pub timestamp: SystemTime,
    pub status: MessageStatus,
    pub reactions: Vec<Reaction>,
    pub external_id: Option<String>,
}

/// Message sender
#[derive(Debug, Clone)]
pub struct MessageSender {
    pub id: String,
    pub name: String,
    pub sender_type: SenderType,
    pub avatar_url: Option<String>,
}

/// Sender types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SenderType {
    User,
    Bot,
    System,
    Agent,
}

/// Message delivery status
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MessageStatus {
    Pending,
    Sent,
    Delivered,
    Read,
    Failed,
}

/// Message reaction
#[derive(Debug, Clone)]
pub struct Reaction {
    pub emoji: String,
    pub user_id: String,
    pub timestamp: SystemTime,
}

impl Message {
    /// Create new message
    pub fn new(
        channel_id: impl Into<String>,
        sender: MessageSender,
        content: MessageContent,
    ) -> Self {
        Self {
            id: generate_message_id(),
            channel_id: channel_id.into(),
            thread_id: None,
            sender,
            content,
            priority: MessagePriority::Normal,
            timestamp: SystemTime::now(),
            status: MessageStatus::Pending,
            reactions: Vec::new(),
            external_id: None,
        }
    }

    /// Set thread ID
    pub fn in_thread(mut self, thread_id: impl Into<String>) -> Self {
        self.thread_id = Some(thread_id.into());
        self
    }

    /// Set priority
    pub fn with_priority(mut self, priority: MessagePriority) -> Self {
        self.priority = priority;
        self
    }

    /// Add reaction
    pub fn add_reaction(&mut self, emoji: impl Into<String>, user_id: impl Into<String>) {
        self.reactions.push(Reaction {
            emoji: emoji.into(),
            user_id: user_id.into(),
            timestamp: SystemTime::now(),
        });
    }

    /// Mark as sent
    pub fn mark_sent(&mut self, external_id: Option<String>) {
        self.status = MessageStatus::Sent;
        self.external_id = external_id;
    }

    /// Mark as delivered
    pub fn mark_delivered(&mut self) {
        self.status = MessageStatus::Delivered;
    }

    /// Mark as read
    pub fn mark_read(&mut self) {
        self.status = MessageStatus::Read;
    }

    /// Mark as failed
    pub fn mark_failed(&mut self) {
        self.status = MessageStatus::Failed;
    }
}

impl MessageSender {
    /// Create user sender
    pub fn user(id: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            sender_type: SenderType::User,
            avatar_url: None,
        }
    }

    /// Create bot sender
    pub fn bot(id: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            sender_type: SenderType::Bot,
            avatar_url: None,
        }
    }

    /// Create agent sender
    pub fn agent(id: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            sender_type: SenderType::Agent,
            avatar_url: None,
        }
    }

    /// Set avatar
    pub fn with_avatar(mut self, url: impl Into<String>) -> Self {
        self.avatar_url = Some(url.into());
        self
    }
}

// ============================================================================
// Notification Types
// ============================================================================

/// Notification type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NotificationType {
    Info,
    Success,
    Warning,
    Error,
    TaskComplete,
    TaskFailed,
    ReviewRequired,
    Mention,
    Custom,
}

impl NotificationType {
    /// Get default emoji for notification type
    pub fn emoji(&self) -> &'static str {
        match self {
            Self::Info => "info",
            Self::Success => "white_check_mark",
            Self::Warning => "warning",
            Self::Error => "x",
            Self::TaskComplete => "heavy_check_mark",
            Self::TaskFailed => "heavy_multiplication_x",
            Self::ReviewRequired => "eyes",
            Self::Mention => "speech_balloon",
            Self::Custom => "bell",
        }
    }

    /// Get default color for notification
    pub fn color(&self) -> &'static str {
        match self {
            Self::Info => "#2196F3",
            Self::Success => "#4CAF50",
            Self::Warning => "#FF9800",
            Self::Error => "#F44336",
            Self::TaskComplete => "#4CAF50",
            Self::TaskFailed => "#F44336",
            Self::ReviewRequired => "#9C27B0",
            Self::Mention => "#00BCD4",
            Self::Custom => "#607D8B",
        }
    }
}

/// Notification
#[derive(Debug, Clone)]
pub struct Notification {
    pub id: String,
    pub notification_type: NotificationType,
    pub title: String,
    pub message: String,
    pub channels: Vec<String>,
    pub priority: MessagePriority,
    pub metadata: HashMap<String, String>,
    pub created_at: SystemTime,
}

impl Notification {
    /// Create new notification
    pub fn new(
        notification_type: NotificationType,
        title: impl Into<String>,
        message: impl Into<String>,
    ) -> Self {
        Self {
            id: generate_message_id(),
            notification_type,
            title: title.into(),
            message: message.into(),
            channels: Vec::new(),
            priority: MessagePriority::Normal,
            metadata: HashMap::new(),
            created_at: SystemTime::now(),
        }
    }

    /// Add target channel
    pub fn to_channel(mut self, channel_id: impl Into<String>) -> Self {
        self.channels.push(channel_id.into());
        self
    }

    /// Set priority
    pub fn with_priority(mut self, priority: MessagePriority) -> Self {
        self.priority = priority;
        self
    }

    /// Add metadata
    pub fn with_metadata(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.metadata.insert(key.into(), value.into());
        self
    }

    /// Convert to message content
    pub fn to_content(&self) -> MessageContent {
        let text = format!("**{}**\n\n{}", self.title, self.message);
        MessageContent::markdown(text)
            .with_metadata("notification_type", format!("{:?}", self.notification_type))
    }
}

// ============================================================================
// Platform Credentials
// ============================================================================

/// Platform credentials
#[derive(Debug, Clone)]
pub struct PlatformCredentials {
    pub platform: Platform,
    pub token: Option<String>,
    pub api_key: Option<String>,
    pub webhook_url: Option<String>,
    pub client_id: Option<String>,
    pub client_secret: Option<String>,
    pub extra: HashMap<String, String>,
}

impl PlatformCredentials {
    /// Create Slack credentials
    pub fn slack(token: impl Into<String>) -> Self {
        Self {
            platform: Platform::Slack,
            token: Some(token.into()),
            api_key: None,
            webhook_url: None,
            client_id: None,
            client_secret: None,
            extra: HashMap::new(),
        }
    }

    /// Create Discord credentials
    pub fn discord(token: impl Into<String>) -> Self {
        Self {
            platform: Platform::Discord,
            token: Some(token.into()),
            api_key: None,
            webhook_url: None,
            client_id: None,
            client_secret: None,
            extra: HashMap::new(),
        }
    }

    /// Create Teams credentials
    pub fn teams(client_id: impl Into<String>, client_secret: impl Into<String>) -> Self {
        Self {
            platform: Platform::Teams,
            token: None,
            api_key: None,
            webhook_url: None,
            client_id: Some(client_id.into()),
            client_secret: Some(client_secret.into()),
            extra: HashMap::new(),
        }
    }

    /// Create webhook credentials
    pub fn webhook(url: impl Into<String>) -> Self {
        Self {
            platform: Platform::Webhook,
            token: None,
            api_key: None,
            webhook_url: Some(url.into()),
            client_id: None,
            client_secret: None,
            extra: HashMap::new(),
        }
    }

    /// Create email credentials (SMTP)
    pub fn email(
        host: impl Into<String>,
        username: impl Into<String>,
        password: impl Into<String>,
    ) -> Self {
        let mut extra = HashMap::new();
        extra.insert("host".to_string(), host.into());
        extra.insert("username".to_string(), username.into());
        extra.insert("password".to_string(), password.into());
        Self {
            platform: Platform::Email,
            token: None,
            api_key: None,
            webhook_url: None,
            client_id: None,
            client_secret: None,
            extra,
        }
    }

    /// Validate credentials
    pub fn validate(&self) -> Result<(), String> {
        match self.platform {
            Platform::Slack | Platform::Discord | Platform::Telegram => {
                if self.token.is_none() {
                    return Err("Token is required".to_string());
                }
            }
            Platform::Teams => {
                if self.client_id.is_none() || self.client_secret.is_none() {
                    return Err("Client ID and secret are required".to_string());
                }
            }
            Platform::Webhook => {
                if self.webhook_url.is_none() {
                    return Err("Webhook URL is required".to_string());
                }
            }
            Platform::Email => {
                if !self.extra.contains_key("host") {
                    return Err("SMTP host is required".to_string());
                }
            }
            _ => {}
        }
        Ok(())
    }
}

// ============================================================================
// Message Templates
// ============================================================================

/// Message template
#[derive(Debug, Clone)]
pub struct MessageTemplate {
    pub name: String,
    pub format: MessageFormat,
    pub template: String,
    pub variables: Vec<String>,
}

impl MessageTemplate {
    /// Create new template
    pub fn new(name: impl Into<String>, template: impl Into<String>) -> Self {
        let template_str = template.into();
        let variables = Self::extract_variables(&template_str);
        Self {
            name: name.into(),
            format: MessageFormat::Markdown,
            template: template_str,
            variables,
        }
    }

    /// Extract variables from template (format: {{variable}})
    fn extract_variables(template: &str) -> Vec<String> {
        let mut vars = Vec::new();
        let mut start = 0;
        while let Some(pos) = template[start..].find("{{") {
            let var_start = start + pos + 2;
            if let Some(end_pos) = template[var_start..].find("}}") {
                let var = template[var_start..var_start + end_pos].trim().to_string();
                if !vars.contains(&var) {
                    vars.push(var);
                }
                start = var_start + end_pos + 2;
            } else {
                break;
            }
        }
        vars
    }

    /// Render template with values
    pub fn render(&self, values: &HashMap<String, String>) -> MessageContent {
        let mut text = self.template.clone();
        for (key, value) in values {
            text = text.replace(&format!("{{{{{}}}}}", key), value);
        }
        MessageContent {
            text,
            format: self.format,
            attachments: Vec::new(),
            mentions: Vec::new(),
            metadata: HashMap::new(),
        }
    }

    /// Check if all variables are provided
    pub fn validate_values(&self, values: &HashMap<String, String>) -> Result<(), Vec<String>> {
        let missing: Vec<_> = self
            .variables
            .iter()
            .filter(|v| !values.contains_key(*v))
            .cloned()
            .collect();

        if missing.is_empty() {
            Ok(())
        } else {
            Err(missing)
        }
    }
}

// ============================================================================
// Communication Bridge
// ============================================================================

/// Communication bridge for multi-platform messaging
#[derive(Debug)]
pub struct CommunicationBridge {
    credentials: HashMap<Platform, PlatformCredentials>,
    channels: HashMap<String, Channel>,
    threads: HashMap<String, MessageThread>,
    messages: Vec<Message>,
    templates: HashMap<String, MessageTemplate>,
    default_sender: MessageSender,
}

impl CommunicationBridge {
    /// Create new bridge
    pub fn new(agent_name: impl Into<String>) -> Self {
        Self {
            credentials: HashMap::new(),
            channels: HashMap::new(),
            threads: HashMap::new(),
            messages: Vec::new(),
            templates: HashMap::new(),
            default_sender: MessageSender::agent("agent", agent_name),
        }
    }

    /// Add platform credentials
    pub fn add_credentials(&mut self, credentials: PlatformCredentials) -> Result<(), String> {
        credentials.validate()?;
        self.credentials.insert(credentials.platform, credentials);
        Ok(())
    }

    /// Check if platform is configured
    pub fn has_platform(&self, platform: Platform) -> bool {
        self.credentials.contains_key(&platform)
    }

    /// Add channel
    pub fn add_channel(&mut self, channel: Channel) {
        self.channels.insert(channel.id.clone(), channel);
    }

    /// Get channel
    pub fn get_channel(&self, id: &str) -> Option<&Channel> {
        self.channels.get(id)
    }

    /// List channels by platform
    pub fn channels_by_platform(&self, platform: Platform) -> Vec<&Channel> {
        self.channels
            .values()
            .filter(|c| c.platform == platform)
            .collect()
    }

    /// Create thread
    pub fn create_thread(&mut self, channel_id: &str) -> Result<&MessageThread, String> {
        if !self.channels.contains_key(channel_id) {
            return Err(format!("Channel not found: {}", channel_id));
        }

        let thread = MessageThread::new(channel_id);
        let thread_id = thread.id.clone();
        self.threads.insert(thread_id.clone(), thread);
        Ok(self.threads.get(&thread_id).unwrap())
    }

    /// Get thread
    pub fn get_thread(&self, id: &str) -> Option<&MessageThread> {
        self.threads.get(id)
    }

    /// Get mutable thread
    pub fn get_thread_mut(&mut self, id: &str) -> Option<&mut MessageThread> {
        self.threads.get_mut(id)
    }

    /// Add template
    pub fn add_template(&mut self, template: MessageTemplate) {
        self.templates.insert(template.name.clone(), template);
    }

    /// Get template
    pub fn get_template(&self, name: &str) -> Option<&MessageTemplate> {
        self.templates.get(name)
    }

    /// Send message to channel
    pub fn send_message(
        &mut self,
        channel_id: &str,
        content: MessageContent,
    ) -> Result<Message, String> {
        let channel = self
            .channels
            .get(channel_id)
            .ok_or_else(|| format!("Channel not found: {}", channel_id))?;

        if !self.credentials.contains_key(&channel.platform) {
            return Err(format!(
                "No credentials for platform: {:?}",
                channel.platform
            ));
        }

        let mut message = Message::new(channel_id, self.default_sender.clone(), content);

        // Simulate sending (in real implementation, would call platform API)
        message.mark_sent(Some(format!("ext-{}", message.id)));

        self.messages.push(message.clone());
        Ok(message)
    }

    /// Send message to thread
    pub fn send_to_thread(
        &mut self,
        thread_id: &str,
        content: MessageContent,
    ) -> Result<Message, String> {
        let thread = self
            .threads
            .get(thread_id)
            .ok_or_else(|| format!("Thread not found: {}", thread_id))?;

        let channel_id = thread.channel_id.clone();

        let channel = self
            .channels
            .get(&channel_id)
            .ok_or_else(|| format!("Channel not found: {}", channel_id))?;

        if !self.credentials.contains_key(&channel.platform) {
            return Err(format!(
                "No credentials for platform: {:?}",
                channel.platform
            ));
        }

        let mut message =
            Message::new(&channel_id, self.default_sender.clone(), content).in_thread(thread_id);

        message.mark_sent(Some(format!("ext-{}", message.id)));

        // Add to thread
        if let Some(thread) = self.threads.get_mut(thread_id) {
            thread.add_message(message.clone());
        }

        self.messages.push(message.clone());
        Ok(message)
    }

    /// Send notification
    pub fn send_notification(
        &mut self,
        notification: Notification,
    ) -> Result<Vec<Message>, String> {
        let mut messages = Vec::new();
        let content = notification.to_content();

        for channel_id in &notification.channels {
            match self.send_message(channel_id, content.clone()) {
                Ok(msg) => messages.push(msg),
                Err(e) => {
                    // Log error but continue with other channels
                    eprintln!("Failed to send to channel {}: {}", channel_id, e);
                }
            }
        }

        if messages.is_empty() && !notification.channels.is_empty() {
            return Err("Failed to send to any channel".to_string());
        }

        Ok(messages)
    }

    /// Send using template
    pub fn send_from_template(
        &mut self,
        template_name: &str,
        channel_id: &str,
        values: &HashMap<String, String>,
    ) -> Result<Message, String> {
        let template = self
            .templates
            .get(template_name)
            .ok_or_else(|| format!("Template not found: {}", template_name))?
            .clone();

        template
            .validate_values(values)
            .map_err(|missing| format!("Missing template variables: {:?}", missing))?;

        let content = template.render(values);
        self.send_message(channel_id, content)
    }

    /// Broadcast to all channels of a platform
    pub fn broadcast(
        &mut self,
        platform: Platform,
        content: MessageContent,
    ) -> Vec<Result<Message, String>> {
        let channel_ids: Vec<_> = self
            .channels
            .values()
            .filter(|c| c.platform == platform)
            .map(|c| c.id.clone())
            .collect();

        channel_ids
            .iter()
            .map(|id| self.send_message(id, content.clone()))
            .collect()
    }

    /// Get message history for channel
    pub fn message_history(&self, channel_id: &str, limit: usize) -> Vec<&Message> {
        self.messages
            .iter()
            .filter(|m| m.channel_id == channel_id)
            .rev()
            .take(limit)
            .collect()
    }

    /// Get all messages
    pub fn all_messages(&self) -> &[Message] {
        &self.messages
    }

    /// Get message by ID
    pub fn get_message(&self, id: &str) -> Option<&Message> {
        self.messages.iter().find(|m| m.id == id)
    }

    /// Get mutable message by ID
    pub fn get_message_mut(&mut self, id: &str) -> Option<&mut Message> {
        self.messages.iter_mut().find(|m| m.id == id)
    }
}

impl Default for CommunicationBridge {
    fn default() -> Self {
        Self::new("Agent")
    }
}

// ============================================================================
// Webhook Handler
// ============================================================================

/// Incoming webhook payload
#[derive(Debug, Clone)]
pub struct WebhookPayload {
    pub source: Platform,
    pub event_type: WebhookEventType,
    pub channel_id: Option<String>,
    pub user_id: Option<String>,
    pub message: Option<String>,
    pub timestamp: SystemTime,
    pub raw_data: HashMap<String, String>,
}

/// Webhook event types
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum WebhookEventType {
    MessageReceived,
    MentionReceived,
    ReactionAdded,
    ReactionRemoved,
    ChannelJoined,
    ChannelLeft,
    UserJoined,
    UserLeft,
    Unknown,
}

/// Webhook handler
#[derive(Debug)]
pub struct WebhookHandler {
    events: Vec<WebhookPayload>,
    handlers: HashMap<WebhookEventType, Vec<String>>,
}

impl WebhookHandler {
    /// Create new handler
    pub fn new() -> Self {
        Self {
            events: Vec::new(),
            handlers: HashMap::new(),
        }
    }

    /// Register handler for event type
    pub fn on(&mut self, event_type: WebhookEventType, handler_id: impl Into<String>) {
        self.handlers
            .entry(event_type)
            .or_default()
            .push(handler_id.into());
    }

    /// Parse incoming webhook
    pub fn parse_webhook(
        &self,
        platform: Platform,
        data: &HashMap<String, String>,
    ) -> WebhookPayload {
        let event_type = self.detect_event_type(platform, data);

        WebhookPayload {
            source: platform,
            event_type,
            channel_id: data.get("channel_id").cloned(),
            user_id: data.get("user_id").cloned(),
            message: data.get("text").or_else(|| data.get("message")).cloned(),
            timestamp: SystemTime::now(),
            raw_data: data.clone(),
        }
    }

    /// Detect event type from payload
    fn detect_event_type(
        &self,
        platform: Platform,
        data: &HashMap<String, String>,
    ) -> WebhookEventType {
        let event = data
            .get("type")
            .or_else(|| data.get("event"))
            .map(|s| s.as_str());

        match (platform, event) {
            (Platform::Slack, Some("message")) => WebhookEventType::MessageReceived,
            (Platform::Slack, Some("app_mention")) => WebhookEventType::MentionReceived,
            (Platform::Slack, Some("reaction_added")) => WebhookEventType::ReactionAdded,
            (Platform::Discord, Some("MESSAGE_CREATE")) => WebhookEventType::MessageReceived,
            (Platform::Discord, Some("MESSAGE_REACTION_ADD")) => WebhookEventType::ReactionAdded,
            _ => WebhookEventType::Unknown,
        }
    }

    /// Handle incoming webhook
    pub fn handle(&mut self, payload: WebhookPayload) -> Vec<&str> {
        let handler_ids: Vec<_> = self
            .handlers
            .get(&payload.event_type)
            .map(|h| h.iter().map(|s| s.as_str()).collect())
            .unwrap_or_default();

        self.events.push(payload);
        handler_ids
    }

    /// Get event history
    pub fn events(&self) -> &[WebhookPayload] {
        &self.events
    }

    /// Get events by type
    pub fn events_by_type(&self, event_type: WebhookEventType) -> Vec<&WebhookPayload> {
        self.events
            .iter()
            .filter(|e| e.event_type == event_type)
            .collect()
    }
}

impl Default for WebhookHandler {
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
    fn test_platform_name() {
        assert_eq!(Platform::Slack.name(), "Slack");
        assert_eq!(Platform::Discord.name(), "Discord");
        assert_eq!(Platform::Teams.name(), "Microsoft Teams");
    }

    #[test]
    fn test_platform_features() {
        assert!(Platform::Slack.supports_threads());
        assert!(Platform::Discord.supports_reactions());
        assert!(Platform::Email.supports_attachments());
        assert!(!Platform::Webhook.supports_threads());
    }

    #[test]
    fn test_message_priority_ordering() {
        assert!(MessagePriority::Urgent > MessagePriority::High);
        assert!(MessagePriority::High > MessagePriority::Normal);
        assert!(MessagePriority::Normal > MessagePriority::Low);
    }

    #[test]
    fn test_message_content_text() {
        let content = MessageContent::text("Hello, world!");
        assert_eq!(content.text, "Hello, world!");
        assert_eq!(content.format, MessageFormat::PlainText);
    }

    #[test]
    fn test_message_content_markdown() {
        let content = MessageContent::markdown("**Hello**, _world_!");
        assert_eq!(content.format, MessageFormat::Markdown);
    }

    #[test]
    fn test_message_content_with_attachment() {
        let content = MessageContent::text("Check this file").with_attachment(
            Attachment::from_url("doc.pdf", "https://example.com/doc.pdf", "application/pdf"),
        );

        assert_eq!(content.attachments.len(), 1);
        assert_eq!(content.attachments[0].name, "doc.pdf");
    }

    #[test]
    fn test_message_content_with_mention() {
        let content = MessageContent::text("Hey!").with_mention(Mention::user("U123", "Alice"));

        assert_eq!(content.mentions.len(), 1);
        assert_eq!(content.mentions[0].display_name, "Alice");
    }

    #[test]
    fn test_message_content_format_for_slack() {
        let content = MessageContent::markdown("**Bold** text");
        let formatted = content.format_for(Platform::Slack);
        assert!(formatted.contains("*Bold*"));
    }

    #[test]
    fn test_attachment_from_url() {
        let attachment = Attachment::from_url(
            "file.pdf",
            "https://example.com/file.pdf",
            "application/pdf",
        );
        assert_eq!(attachment.name, "file.pdf");
        assert!(attachment.url.is_some());
        assert!(attachment.data.is_none());
    }

    #[test]
    fn test_attachment_from_data() {
        let data = vec![1, 2, 3, 4];
        let attachment =
            Attachment::from_data("file.bin", data.clone(), "application/octet-stream");
        assert_eq!(attachment.size, 4);
        assert!(attachment.data.is_some());
    }

    #[test]
    fn test_mention_format_slack() {
        let mention = Mention::user("U123", "Alice");
        assert_eq!(mention.format_for(Platform::Slack), "<@U123>");

        let channel = Mention::channel("C456", "general");
        assert_eq!(channel.format_for(Platform::Slack), "<#C456>");
    }

    #[test]
    fn test_mention_format_discord() {
        let mention = Mention::user("123456", "Bob");
        assert_eq!(mention.format_for(Platform::Discord), "<@123456>");
    }

    #[test]
    fn test_channel_creation() {
        let channel = Channel::new(Platform::Slack, "general", ChannelType::Public)
            .with_external_id("C12345")
            .with_webhook("https://hooks.slack.com/...");

        assert_eq!(channel.name, "general");
        assert_eq!(channel.platform, Platform::Slack);
        assert!(channel.external_id.is_some());
        assert!(channel.webhook_url.is_some());
    }

    #[test]
    fn test_message_thread() {
        let mut thread = MessageThread::new("ch-1").with_subject("Discussion");

        assert_eq!(thread.channel_id, "ch-1");
        assert_eq!(thread.subject, Some("Discussion".to_string()));

        let message = Message::new(
            "ch-1",
            MessageSender::user("u1", "Alice"),
            MessageContent::text("Hello"),
        );
        thread.add_message(message);

        assert_eq!(thread.message_count(), 1);
        assert!(thread.latest_message().is_some());
    }

    #[test]
    fn test_message_creation() {
        let sender = MessageSender::user("u1", "Alice");
        let content = MessageContent::text("Hello!");
        let message = Message::new("ch-1", sender, content).with_priority(MessagePriority::High);

        assert_eq!(message.channel_id, "ch-1");
        assert_eq!(message.priority, MessagePriority::High);
        assert_eq!(message.status, MessageStatus::Pending);
    }

    #[test]
    fn test_message_status_transitions() {
        let mut message = Message::new(
            "ch-1",
            MessageSender::bot("b1", "Bot"),
            MessageContent::text("Test"),
        );

        assert_eq!(message.status, MessageStatus::Pending);

        message.mark_sent(Some("ext-123".to_string()));
        assert_eq!(message.status, MessageStatus::Sent);
        assert_eq!(message.external_id, Some("ext-123".to_string()));

        message.mark_delivered();
        assert_eq!(message.status, MessageStatus::Delivered);

        message.mark_read();
        assert_eq!(message.status, MessageStatus::Read);
    }

    #[test]
    fn test_message_reactions() {
        let mut message = Message::new(
            "ch-1",
            MessageSender::user("u1", "Alice"),
            MessageContent::text("Great!"),
        );

        message.add_reaction("thumbsup", "u2");
        message.add_reaction("heart", "u3");

        assert_eq!(message.reactions.len(), 2);
        assert_eq!(message.reactions[0].emoji, "thumbsup");
    }

    #[test]
    fn test_message_sender_types() {
        let user = MessageSender::user("u1", "Alice");
        assert_eq!(user.sender_type, SenderType::User);

        let bot = MessageSender::bot("b1", "Bot");
        assert_eq!(bot.sender_type, SenderType::Bot);

        let agent = MessageSender::agent("a1", "Agent");
        assert_eq!(agent.sender_type, SenderType::Agent);
    }

    #[test]
    fn test_notification_type_emoji() {
        assert_eq!(NotificationType::Success.emoji(), "white_check_mark");
        assert_eq!(NotificationType::Error.emoji(), "x");
        assert_eq!(NotificationType::Warning.emoji(), "warning");
    }

    #[test]
    fn test_notification_type_color() {
        assert_eq!(NotificationType::Success.color(), "#4CAF50");
        assert_eq!(NotificationType::Error.color(), "#F44336");
    }

    #[test]
    fn test_notification_creation() {
        let notification = Notification::new(
            NotificationType::TaskComplete,
            "Task Done",
            "The task has been completed successfully",
        )
        .to_channel("ch-1")
        .to_channel("ch-2")
        .with_priority(MessagePriority::High);

        assert_eq!(notification.channels.len(), 2);
        assert_eq!(notification.priority, MessagePriority::High);
    }

    #[test]
    fn test_notification_to_content() {
        let notification =
            Notification::new(NotificationType::Info, "Update", "System update available");

        let content = notification.to_content();
        assert!(content.text.contains("Update"));
        assert!(content.text.contains("System update available"));
    }

    #[test]
    fn test_slack_credentials() {
        let creds = PlatformCredentials::slack("xoxb-token");
        assert!(creds.validate().is_ok());
        assert_eq!(creds.platform, Platform::Slack);
    }

    #[test]
    fn test_discord_credentials() {
        let creds = PlatformCredentials::discord("bot-token");
        assert!(creds.validate().is_ok());
    }

    #[test]
    fn test_teams_credentials() {
        let creds = PlatformCredentials::teams("client-id", "client-secret");
        assert!(creds.validate().is_ok());
    }

    #[test]
    fn test_webhook_credentials() {
        let creds = PlatformCredentials::webhook("https://example.com/webhook");
        assert!(creds.validate().is_ok());
    }

    #[test]
    fn test_email_credentials() {
        let creds = PlatformCredentials::email("smtp.example.com", "user", "pass");
        assert!(creds.validate().is_ok());
    }

    #[test]
    fn test_credentials_validation_failure() {
        let creds = PlatformCredentials {
            platform: Platform::Slack,
            token: None,
            api_key: None,
            webhook_url: None,
            client_id: None,
            client_secret: None,
            extra: HashMap::new(),
        };
        assert!(creds.validate().is_err());
    }

    #[test]
    fn test_message_template() {
        let template = MessageTemplate::new("welcome", "Hello {{name}}! Welcome to {{team}}.");

        assert_eq!(template.variables.len(), 2);
        assert!(template.variables.contains(&"name".to_string()));
        assert!(template.variables.contains(&"team".to_string()));
    }

    #[test]
    fn test_message_template_render() {
        let template = MessageTemplate::new("greeting", "Hello {{name}}!");

        let mut values = HashMap::new();
        values.insert("name".to_string(), "Alice".to_string());

        let content = template.render(&values);
        assert_eq!(content.text, "Hello Alice!");
    }

    #[test]
    fn test_message_template_validate() {
        let template = MessageTemplate::new("test", "Hello {{name}} from {{location}}");

        let mut values = HashMap::new();
        values.insert("name".to_string(), "Bob".to_string());

        let result = template.validate_values(&values);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains(&"location".to_string()));
    }

    #[test]
    fn test_communication_bridge_creation() {
        let bridge = CommunicationBridge::new("TestAgent");
        assert!(bridge.channels.is_empty());
    }

    #[test]
    fn test_communication_bridge_add_credentials() {
        let mut bridge = CommunicationBridge::new("Agent");

        let result = bridge.add_credentials(PlatformCredentials::slack("token"));
        assert!(result.is_ok());
        assert!(bridge.has_platform(Platform::Slack));
    }

    #[test]
    fn test_communication_bridge_add_channel() {
        let mut bridge = CommunicationBridge::new("Agent");

        let channel = Channel::new(Platform::Slack, "general", ChannelType::Public);
        let channel_id = channel.id.clone();
        bridge.add_channel(channel);

        assert!(bridge.get_channel(&channel_id).is_some());
    }

    #[test]
    fn test_communication_bridge_channels_by_platform() {
        let mut bridge = CommunicationBridge::new("Agent");

        bridge.add_channel(Channel::new(Platform::Slack, "ch1", ChannelType::Public));
        bridge.add_channel(Channel::new(Platform::Slack, "ch2", ChannelType::Private));
        bridge.add_channel(Channel::new(Platform::Discord, "ch3", ChannelType::Public));

        let slack_channels = bridge.channels_by_platform(Platform::Slack);
        assert_eq!(slack_channels.len(), 2);
    }

    #[test]
    fn test_communication_bridge_create_thread() {
        let mut bridge = CommunicationBridge::new("Agent");

        let channel = Channel::new(Platform::Slack, "general", ChannelType::Public);
        let channel_id = channel.id.clone();
        bridge.add_channel(channel);

        let result = bridge.create_thread(&channel_id);
        assert!(result.is_ok());
    }

    #[test]
    fn test_communication_bridge_send_message() {
        let mut bridge = CommunicationBridge::new("Agent");
        bridge
            .add_credentials(PlatformCredentials::slack("token"))
            .unwrap();

        let channel = Channel::new(Platform::Slack, "general", ChannelType::Public);
        let channel_id = channel.id.clone();
        bridge.add_channel(channel);

        let result = bridge.send_message(&channel_id, MessageContent::text("Hello!"));
        assert!(result.is_ok());

        let message = result.unwrap();
        assert_eq!(message.status, MessageStatus::Sent);
    }

    #[test]
    fn test_communication_bridge_send_no_credentials() {
        let mut bridge = CommunicationBridge::new("Agent");

        let channel = Channel::new(Platform::Slack, "general", ChannelType::Public);
        let channel_id = channel.id.clone();
        bridge.add_channel(channel);

        let result = bridge.send_message(&channel_id, MessageContent::text("Hello!"));
        assert!(result.is_err());
    }

    #[test]
    fn test_communication_bridge_send_to_thread() {
        let mut bridge = CommunicationBridge::new("Agent");
        bridge
            .add_credentials(PlatformCredentials::slack("token"))
            .unwrap();

        let channel = Channel::new(Platform::Slack, "general", ChannelType::Public);
        let channel_id = channel.id.clone();
        bridge.add_channel(channel);

        let thread = bridge.create_thread(&channel_id).unwrap();
        let thread_id = thread.id.clone();

        let result = bridge.send_to_thread(&thread_id, MessageContent::text("Reply"));
        assert!(result.is_ok());

        let thread = bridge.get_thread(&thread_id).unwrap();
        assert_eq!(thread.message_count(), 1);
    }

    #[test]
    fn test_communication_bridge_add_template() {
        let mut bridge = CommunicationBridge::new("Agent");

        let template = MessageTemplate::new("welcome", "Hello {{name}}!");
        bridge.add_template(template);

        assert!(bridge.get_template("welcome").is_some());
    }

    #[test]
    fn test_communication_bridge_send_from_template() {
        let mut bridge = CommunicationBridge::new("Agent");
        bridge
            .add_credentials(PlatformCredentials::slack("token"))
            .unwrap();

        let channel = Channel::new(Platform::Slack, "general", ChannelType::Public);
        let channel_id = channel.id.clone();
        bridge.add_channel(channel);

        bridge.add_template(MessageTemplate::new("welcome", "Hello {{name}}!"));

        let mut values = HashMap::new();
        values.insert("name".to_string(), "World".to_string());

        let result = bridge.send_from_template("welcome", &channel_id, &values);
        assert!(result.is_ok());
    }

    #[test]
    fn test_communication_bridge_message_history() {
        let mut bridge = CommunicationBridge::new("Agent");
        bridge
            .add_credentials(PlatformCredentials::slack("token"))
            .unwrap();

        let channel = Channel::new(Platform::Slack, "general", ChannelType::Public);
        let channel_id = channel.id.clone();
        bridge.add_channel(channel);

        for i in 0..5 {
            bridge
                .send_message(&channel_id, MessageContent::text(format!("Msg {}", i)))
                .unwrap();
        }

        let history = bridge.message_history(&channel_id, 3);
        assert_eq!(history.len(), 3);
    }

    #[test]
    fn test_communication_bridge_broadcast() {
        let mut bridge = CommunicationBridge::new("Agent");
        bridge
            .add_credentials(PlatformCredentials::slack("token"))
            .unwrap();

        for i in 0..3 {
            bridge.add_channel(Channel::new(
                Platform::Slack,
                format!("ch{}", i),
                ChannelType::Public,
            ));
        }

        let results = bridge.broadcast(Platform::Slack, MessageContent::text("Announcement"));
        assert_eq!(results.len(), 3);
        assert!(results.iter().all(|r| r.is_ok()));
    }

    #[test]
    fn test_webhook_handler() {
        let mut handler = WebhookHandler::new();
        handler.on(WebhookEventType::MessageReceived, "msg-handler");

        let mut data = HashMap::new();
        data.insert("type".to_string(), "message".to_string());
        data.insert("text".to_string(), "Hello".to_string());

        let payload = handler.parse_webhook(Platform::Slack, &data);
        assert_eq!(payload.event_type, WebhookEventType::MessageReceived);
        assert_eq!(payload.message, Some("Hello".to_string()));
    }

    #[test]
    fn test_webhook_handler_handle() {
        let mut handler = WebhookHandler::new();
        handler.on(WebhookEventType::MessageReceived, "handler1");
        handler.on(WebhookEventType::MessageReceived, "handler2");

        let payload = WebhookPayload {
            source: Platform::Slack,
            event_type: WebhookEventType::MessageReceived,
            channel_id: Some("C123".to_string()),
            user_id: Some("U456".to_string()),
            message: Some("Test".to_string()),
            timestamp: SystemTime::now(),
            raw_data: HashMap::new(),
        };

        let handler_ids = handler.handle(payload);
        assert_eq!(handler_ids.len(), 2);
    }

    #[test]
    fn test_webhook_handler_events() {
        let mut handler = WebhookHandler::new();

        let payload1 = WebhookPayload {
            source: Platform::Slack,
            event_type: WebhookEventType::MessageReceived,
            channel_id: None,
            user_id: None,
            message: None,
            timestamp: SystemTime::now(),
            raw_data: HashMap::new(),
        };

        let payload2 = WebhookPayload {
            source: Platform::Discord,
            event_type: WebhookEventType::ReactionAdded,
            channel_id: None,
            user_id: None,
            message: None,
            timestamp: SystemTime::now(),
            raw_data: HashMap::new(),
        };

        handler.handle(payload1);
        handler.handle(payload2);

        assert_eq!(handler.events().len(), 2);
        assert_eq!(
            handler
                .events_by_type(WebhookEventType::MessageReceived)
                .len(),
            1
        );
    }

    #[test]
    fn test_webhook_event_type_detection() {
        let handler = WebhookHandler::new();

        let mut slack_msg = HashMap::new();
        slack_msg.insert("type".to_string(), "message".to_string());
        let payload = handler.parse_webhook(Platform::Slack, &slack_msg);
        assert_eq!(payload.event_type, WebhookEventType::MessageReceived);

        let mut slack_mention = HashMap::new();
        slack_mention.insert("type".to_string(), "app_mention".to_string());
        let payload = handler.parse_webhook(Platform::Slack, &slack_mention);
        assert_eq!(payload.event_type, WebhookEventType::MentionReceived);

        let mut discord_msg = HashMap::new();
        discord_msg.insert("type".to_string(), "MESSAGE_CREATE".to_string());
        let payload = handler.parse_webhook(Platform::Discord, &discord_msg);
        assert_eq!(payload.event_type, WebhookEventType::MessageReceived);
    }

    #[test]
    fn test_send_notification() {
        let mut bridge = CommunicationBridge::new("Agent");
        bridge
            .add_credentials(PlatformCredentials::slack("token"))
            .unwrap();

        let channel = Channel::new(Platform::Slack, "alerts", ChannelType::Public);
        let channel_id = channel.id.clone();
        bridge.add_channel(channel);

        let notification = Notification::new(
            NotificationType::Success,
            "Build Complete",
            "The build finished successfully",
        )
        .to_channel(&channel_id);

        let result = bridge.send_notification(notification);
        assert!(result.is_ok());
        assert_eq!(result.unwrap().len(), 1);
    }

    #[test]
    fn test_message_in_thread() {
        let sender = MessageSender::user("u1", "Alice");
        let message =
            Message::new("ch-1", sender, MessageContent::text("Reply")).in_thread("thr-1");

        assert_eq!(message.thread_id, Some("thr-1".to_string()));
    }

    #[test]
    fn test_channel_with_metadata() {
        let channel = Channel::new(Platform::Slack, "general", ChannelType::Public)
            .with_metadata("purpose", "General discussion")
            .with_metadata("topic", "Updates");

        assert_eq!(channel.metadata.len(), 2);
        assert_eq!(
            channel.metadata.get("purpose"),
            Some(&"General discussion".to_string())
        );
    }

    #[test]
    fn test_message_content_with_metadata() {
        let content = MessageContent::text("Hello")
            .with_metadata("source", "api")
            .with_metadata("version", "1.0");

        assert_eq!(content.metadata.len(), 2);
    }

    #[test]
    fn test_message_sender_with_avatar() {
        let sender =
            MessageSender::user("u1", "Alice").with_avatar("https://example.com/avatar.png");

        assert_eq!(
            sender.avatar_url,
            Some("https://example.com/avatar.png".to_string())
        );
    }

    #[test]
    fn test_thread_with_parent() {
        let thread = MessageThread::new("ch-1")
            .with_parent("msg-123")
            .with_subject("Reply thread");

        assert_eq!(thread.parent_message_id, Some("msg-123".to_string()));
        assert_eq!(thread.subject, Some("Reply thread".to_string()));
    }
}
