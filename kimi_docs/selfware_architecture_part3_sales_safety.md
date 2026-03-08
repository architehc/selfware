## 6. Sales Outbound Automation Module

### 6.1 Sales Automation Architecture

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                     SALES OUTBOUND AUTOMATION MODULE                        │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                              │
│  ┌─────────────────────────────────────────────────────────────────────┐   │
│  │                    Campaign Orchestrator                             │   │
│  │  - Sequence management           - A/B testing                       │   │
│  │  - Scheduling                    - Performance tracking              │   │
│  └───────────────────────────────┬─────────────────────────────────────┘   │
│                                  │                                          │
│  ┌───────────────────────────────┴─────────────────────────────────────┐   │
│  │  ┌─────────────┐  ┌─────────────┐  ┌─────────────┐  ┌─────────────┐ │   │
│  │  │   Email     │  │  LinkedIn   │  │    CRM      │  │   Social    │ │   │
│  │  │ Automation  │  │ Automation  │  │ Integration │  │  Outreach   │ │   │
│  │  └─────────────┘  └─────────────┘  └─────────────┘  └─────────────┘ │   │
│  └─────────────────────────────────────────────────────────────────────┘   │
│                                  │                                          │
│  ┌───────────────────────────────┴─────────────────────────────────────┐   │
│  │                    Follow-Up Sequences                               │   │
│  │  ┌──────────┐ ┌──────────┐ ┌──────────┐ ┌──────────┐ ┌──────────┐  │   │
│  │  │  Step 1  │ │  Step 2  │ │  Step 3  │ │  Step 4  │ │  Step N  │  │   │
│  │  │ (Day 0)  │ │ (Day 3)  │ │ (Day 7)  │ │ (Day 14) │ │ (Day 30) │  │   │
│  │  └──────────┘ └──────────┘ └──────────┘ └──────────┘ └──────────┘  │   │
│  └─────────────────────────────────────────────────────────────────────┘   │
│                                  │                                          │
│  ┌───────────────────────────────┴─────────────────────────────────────┐   │
│  │                    Compliance & Safety                               │   │
│  │  ┌──────────┐ ┌──────────┐ ┌──────────┐ ┌──────────┐ ┌──────────┐  │   │
│  │  │  Rate    │ │  Spam    │ │  Unsub-  │ │  Content │ │  Audit   │  │   │
│  │  │  Limiter │ │  Filter  │ │  scribe  │ │  Scanner │ │  Logger  │  │   │
│  │  └──────────┘ └──────────┘ └──────────┘ └──────────┘ └──────────┘  │   │
│  └─────────────────────────────────────────────────────────────────────┘   │
│                                                                              │
└─────────────────────────────────────────────────────────────────────────────┘
```

### 6.2 Email Automation Workflows

```rust
// src/sales/email_automation.rs
pub struct EmailAutomation {
    smtp_pool: SmtpConnectionPool,
    template_engine: TemplateEngine,
    personalization: PersonalizationEngine,
    compliance: EmailCompliance,
}

#[derive(Debug, Clone)]
pub struct EmailCampaign {
    pub id: String,
    pub name: String,
    pub template: EmailTemplate,
    pub recipients: Vec<Contact>,
    pub schedule: CampaignSchedule,
    pub tracking: TrackingConfig,
}

#[derive(Debug, Clone)]
pub struct EmailTemplate {
    pub subject: String,
    pub body_html: String,
    pub body_text: String,
    pub variables: Vec<TemplateVariable>,
}

impl EmailAutomation {
    pub async fn send_personalized_email(
        &self,
        contact: &Contact,
        template: &EmailTemplate,
        context: &HashMap<String, String>,
    ) -> Result<EmailResult, SalesError> {
        // 1. Check compliance
        self.compliance.check_can_email(contact).await?;
        
        // 2. Personalize content
        let personalized = self.personalization.personalize(template, contact, context)?;
        
        // 3. Spam score check
        let spam_score = self.compliance.check_spam_score(&personalized).await?;
        if spam_score > 5.0 {
            return Err(SalesError::HighSpamScore(spam_score));
        }
        
        // 4. Rate limit check
        self.compliance.check_rate_limit(contact).await?;
        
        // 5. Send
        let message = self.build_message(contact, &personalized)?;
        let result = self.smtp_pool.send(message).await?;
        
        // 6. Track
        self.track_send(contact, &result).await?;
        
        Ok(EmailResult {
            message_id: result.message_id,
            sent_at: Instant::now(),
        })
    }
    
    pub async fn run_sequence(
        &self,
        sequence: &FollowUpSequence,
        contact: &Contact,
    ) -> Result<SequenceResult, SalesError> {
        let mut results = Vec::new();
        
        for step in &sequence.steps {
            if !self.should_proceed(contact, &results).await? {
                break;
            }
            
            if step.delay_days > 0 {
                tokio::time::sleep(Duration::from_secs(step.delay_days as u64 * 86400)).await;
            }
            
            if self.has_replied(contact, &sequence.id).await? {
                break;
            }
            
            let result = self.send_personalized_email(
                contact, &step.template, &step.context
            ).await?;
            
            results.push(StepResult {
                step_id: step.id.clone(),
                email_result: result,
            });
        }
        
        Ok(SequenceResult {
            sequence_id: sequence.id.clone(),
            step_results: results,
        })
    }
    
    async fn should_proceed(
        &self,
        contact: &Contact,
        previous_results: &[StepResult],
    ) -> Result<bool, SalesError> {
        let bounce_count = previous_results.iter()
            .filter(|r| r.email_result.is_bounce()).count();
        if bounce_count > 0 || contact.unsubscribed {
            return Ok(false);
        }
        Ok(true)
    }
}
```

### 6.3 LinkedIn/Social Media Automation

```rust
// src/sales/linkedin_automation.rs
pub struct LinkedInAutomation {
    browser: Arc<BrowserController>,
    session_store: SessionStore,
    rate_limiter: RateLimiter,
}

#[derive(Debug, Clone)]
pub struct LinkedInMessage {
    pub recipient_profile_url: String,
    pub message: String,
    pub connection_request: bool,
    pub note: Option<String>,
}

impl LinkedInAutomation {
    pub async fn send_connection_request(
        &self,
        profile_url: &str,
        personalized_note: Option<&str>,
    ) -> Result<ConnectionResult, SalesError> {
        self.rate_limiter.check("linkedin_connection").await?;
        
        let page = self.browser.new_page().await?;
        page.navigate(profile_url).await?;
        
        if self.is_already_connected(&page).await? {
            return Ok(ConnectionResult::AlreadyConnected);
        }
        
        let connect_btn = page.find("button[aria-label='Connect']").await?;
        connect_btn.click().await?;
        
        if let Some(note) = personalized_note {
            let add_note_btn = page.find("button[aria-label='Add a note']").await?;
            add_note_btn.click().await?;
            let textarea = page.find("textarea[name='message']").await?;
            textarea.type_text(note).await?;
        }
        
        let send_btn = page.find("button[aria-label='Send now']").await?;
        send_btn.click().await?;
        
        self.track_connection(profile_url).await?;
        Ok(ConnectionResult::Sent)
    }
    
    pub async fn send_message(
        &self,
        profile_url: &str,
        message: &str,
    ) -> Result<MessageResult, SalesError> {
        self.rate_limiter.check("linkedin_message").await?;
        
        let page = self.browser.new_page().await?;
        page.navigate(&format!("{}/overlay/contact-info/", profile_url)).await?;
        
        let msg_btn = page.find("button[aria-label='Message']").await?;
        msg_btn.click().await?;
        
        let msg_input = page.find("div[role='textbox']").await?;
        msg_input.type_text(message).await?;
        
        let send_btn = page.find("button[type='submit']").await?;
        send_btn.click().await?;
        
        self.track_message(profile_url, message).await?;
        Ok(MessageResult::Sent)
    }
    
    pub async fn extract_profile_info(&self, profile_url: &str) 
        -> Result<ProfileInfo, SalesError> {
        let page = self.browser.new_page().await?;
        page.navigate(profile_url).await?;
        
        let name = page.inner_text("h1").await?;
        let headline = page.inner_text(".text-body-medium").await.ok();
        let company = page.inner_text(".experience-item__company-name").await.ok();
        
        Ok(ProfileInfo { name, headline, company, url: profile_url.to_string() })
    }
}
```

### 6.4 CRM Integration Patterns

```rust
// src/sales/crm_integration.rs
pub struct CrmIntegration {
    connectors: HashMap<String, Box<dyn CrmConnector>>,
    sync_engine: SyncEngine,
}

#[async_trait]
pub trait CrmConnector: Send + Sync {
    fn name(&self) -> &str;
    async fn test_connection(&self) -> Result<(), CrmError>;
    async fn get_contact(&self, id: &str) -> Result<Contact, CrmError>;
    async fn create_contact(&self, contact: &Contact) -> Result<String, CrmError>;
    async fn update_contact(&self, id: &str, contact: &Contact) -> Result<(), CrmError>;
    async fn get_deals(&self, contact_id: Option<&str>) -> Result<Vec<Deal>, CrmError>;
    async fn create_deal(&self, deal: &Deal) -> Result<String, CrmError>;
    async fn log_activity(&self, activity: &Activity) -> Result<(), CrmError>;
}

pub struct HubSpotConnector {
    api_key: String,
    client: reqwest::Client,
}

#[async_trait]
impl CrmConnector for HubSpotConnector {
    fn name(&self) -> &str { "HubSpot" }
    
    async fn create_contact(&self, contact: &Contact) -> Result<String, CrmError> {
        let response = self.client
            .post("https://api.hubapi.com/crm/v3/objects/contacts")
            .header("Authorization", format!("Bearer {}", self.api_key))
            .json(&json!({
                "properties": {
                    "email": contact.email,
                    "firstname": contact.first_name,
                    "lastname": contact.last_name,
                    "company": contact.company,
                    "phone": contact.phone,
                    "linkedin_url": contact.linkedin_url,
                }
            }))
            .send().await?;
        
        let result: Value = response.json().await?;
        Ok(result["id"].as_str().unwrap().to_string())
    }
    
    async fn log_activity(&self, activity: &Activity) -> Result<(), CrmError> {
        let activity_type = match activity.activity_type {
            ActivityType::Email => "EMAIL",
            ActivityType::Call => "CALL",
            ActivityType::Meeting => "MEETING",
            ActivityType::Note => "NOTE",
        };
        
        self.client
            .post("https://api.hubapi.com/engagements/v1/engagements")
            .header("Authorization", format!("Bearer {}", self.api_key))
            .json(&json!({
                "engagement": { "type": activity_type, "timestamp": activity.timestamp.timestamp_millis() },
                "associations": { "contactIds": [activity.contact_id] },
                "metadata": { "body": activity.notes }
            }))
            .send().await?;
        Ok(())
    }
}

impl CrmIntegration {
    pub async fn sync_contact(&self, crm_name: &str, contact: &Contact) 
        -> Result<SyncResult, SalesError> {
        let connector = self.connectors.get(crm_name)
            .ok_or_else(|| SalesError::CrmNotFound(crm_name.to_string()))?;
        
        if let Some(crm_id) = &contact.crm_ids.get(crm_name) {
            connector.update_contact(crm_id, contact).await?;
            Ok(SyncResult::Updated(crm_id.clone()))
        } else {
            let new_id = connector.create_contact(contact).await?;
            Ok(SyncResult::Created(new_id))
        }
    }
}
```

### 6.5 Follow-Up Sequences

```rust
// src/sales/sequences.rs
pub struct SequenceEngine {
    db: Arc<dyn SequenceStore>,
    scheduler: Scheduler,
    executors: HashMap<ChannelType, Box<dyn ChannelExecutor>>,
}

#[derive(Debug, Clone)]
pub struct FollowUpSequence {
    pub id: String,
    pub name: String,
    pub steps: Vec<SequenceStep>,
    pub exit_conditions: Vec<ExitCondition>,
    pub metadata: SequenceMetadata,
}

#[derive(Debug, Clone)]
pub struct SequenceStep {
    pub id: String,
    pub channel: ChannelType,
    pub template_id: String,
    pub delay_days: u32,
    pub condition: Option<StepCondition>,
}

#[derive(Debug, Clone)]
pub enum ChannelType { Email, LinkedIn, Phone, SMS }

#[derive(Debug, Clone)]
pub enum ExitCondition {
    ReplyReceived, MeetingBooked, Unsubscribed, Bounced, MaxStepsReached(usize),
}

impl SequenceEngine {
    pub async fn enroll_contact(
        &self,
        sequence_id: &str,
        contact: &Contact,
    ) -> Result<Enrollment, SalesError> {
        let sequence = self.db.get_sequence(sequence_id).await?;
        
        if self.db.is_enrolled(sequence_id, &contact.id).await? {
            return Err(SalesError::AlreadyEnrolled);
        }
        
        let enrollment = Enrollment {
            id: Uuid::new_v4().to_string(),
            sequence_id: sequence_id.to_string(),
            contact_id: contact.id.clone(),
            current_step: 0,
            started_at: Utc::now(),
            status: EnrollmentStatus::Active,
        };
        
        self.db.create_enrollment(&enrollment).await?;
        
        if let Some(first_step) = sequence.steps.first() {
            self.schedule_step(&enrollment, first_step, Utc::now()).await?;
        }
        
        Ok(enrollment)
    }
    
    pub async fn process_step(&self, enrollment_id: &str) -> Result<StepResult, SalesError> {
        let enrollment = self.db.get_enrollment(enrollment_id).await?;
        let sequence = self.db.get_sequence(&enrollment.sequence_id).await?;
        let step = sequence.steps.get(enrollment.current_step)
            .ok_or(SalesError::InvalidStep)?;
        let contact = self.db.get_contact(&enrollment.contact_id).await?;
        
        for condition in &sequence.exit_conditions {
            if self.check_exit_condition(&contact, condition).await? {
                self.db.update_enrollment_status(
                    enrollment_id, EnrollmentStatus::Exited(condition.clone())
                ).await?;
                return Ok(StepResult::Exited(condition.clone()));
            }
        }
        
        let executor = self.executors.get(&step.channel)
            .ok_or(SalesError::ChannelNotSupported)?;
        let result = executor.execute(&contact, step).await?;
        
        self.log_step_execution(&enrollment, step, &result).await?;
        
        let next_step_index = enrollment.current_step + 1;
        if next_step_index < sequence.steps.len() {
            self.db.advance_enrollment(enrollment_id).await?;
            let next_step = &sequence.steps[next_step_index];
            let scheduled_at = Utc::now() + Duration::days(next_step.delay_days as i64);
            self.schedule_step(&enrollment, next_step, scheduled_at).await?;
        } else {
            self.db.update_enrollment_status(enrollment_id, EnrollmentStatus::Completed).await?;
        }
        
        Ok(StepResult::Executed(result))
    }
}
```

### 6.6 Compliance and Anti-Spam

```rust
// src/sales/compliance.rs
pub struct ComplianceEngine {
    rate_limiter: RateLimiter,
    spam_checker: SpamChecker,
    unsubscribe_handler: UnsubscribeHandler,
    audit_logger: AuditLogger,
}

#[derive(Debug, Clone)]
pub struct ComplianceRules {
    pub max_emails_per_day: u32,
    pub max_emails_per_week: u32,
    pub min_hours_between_emails: u32,
    pub require_unsubscribe: bool,
    pub max_spam_score: f64,
}

impl ComplianceEngine {
    pub async fn check_email_compliance(
        &self,
        contact: &Contact,
        email: &EmailMessage,
    ) -> Result<ComplianceResult, ComplianceError> {
        let mut violations = Vec::new();
        
        if contact.unsubscribed {
            violations.push(ComplianceViolation::ContactUnsubscribed);
        }
        
        let recent_emails = self.audit_logger
            .count_emails_to_contact(&contact.id, Duration::days(1)).await?;
        if recent_emails >= self.rules.max_emails_per_day {
            violations.push(ComplianceViolation::RateLimitExceeded {
                limit: self.rules.max_emails_per_day,
                actual: recent_emails,
                period: "day".to_string(),
            });
        }
        
        if let Some(last_email) = contact.last_email_sent {
            let hours_since = (Utc::now() - last_email).num_hours();
            if hours_since < self.rules.min_hours_between_emails as i64 {
                violations.push(ComplianceViolation::TooSoon {
                    min_hours: self.rules.min_hours_between_emails,
                    actual_hours: hours_since as u32,
                });
            }
        }
        
        let spam_score = self.spam_checker.check(email).await?;
        if spam_score > self.rules.max_spam_score {
            violations.push(ComplianceViolation::HighSpamScore(spam_score));
        }
        
        if self.rules.require_unsubscribe && !email.body_html.contains("unsubscribe") {
            violations.push(ComplianceViolation::MissingUnsubscribe);
        }
        
        if violations.is_empty() {
            Ok(ComplianceResult::Compliant)
        } else {
            Ok(ComplianceResult::NonCompliant(violations))
        }
    }
    
    pub async fn handle_unsubscribe(&self, email: &str) -> Result<(), ComplianceError> {
        self.db.unsubscribe_contact(email).await?;
        self.sequence_engine.stop_sequences_for_contact(email).await?;
        self.audit_logger.log_unsubscribe(email).await?;
        Ok(())
    }
}
```

---

## 7. Safety and Security Framework

### 7.1 Permission and Sandboxing Models

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                     SAFETY & SECURITY FRAMEWORK                             │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                              │
│  ┌─────────────────────────────────────────────────────────────────────┐   │
│  │                    Permission System                                 │   │
│  │  ┌──────────┐ ┌──────────┐ ┌──────────┐ ┌──────────┐ ┌──────────┐  │   │
│  │  │  File    │ │  Network │ │  System  │ │  Browser │ │  Shell   │  │   │
│  │  │  Access  │ │  Access  │ │  Access  │ │  Access  │ │  Access  │  │   │
│  │  └──────────┘ └──────────┘ └──────────┘ └──────────┘ └──────────┘  │   │
│  │                                                                     │   │
│  │  Permission Levels: None | Read | Write | Execute | Admin          │   │
│  └─────────────────────────────────────────────────────────────────────┘   │
│                                  │                                          │
│  ┌───────────────────────────────┴─────────────────────────────────────┐   │
│  │                    Sandbox Manager                                   │   │
│  │  ┌─────────────┐ ┌─────────────┐ ┌─────────────┐ ┌─────────────┐   │   │
│  │  │   Docker    │ │   chroot    │ │   Firejail  │ │   WASM      │   │   │
│  │  │  Sandbox    │ │   Jail      │ │  Sandbox    │ │  Sandbox    │   │   │
│  │  └─────────────┘ └─────────────┘ └─────────────┘ └─────────────┘   │   │
│  └─────────────────────────────────────────────────────────────────────┘   │
│                                  │                                          │
│  ┌───────────────────────────────┴─────────────────────────────────────┐   │
│  │                    Threat Detection                                  │   │
│  │  ┌──────────┐ ┌──────────┐ ┌──────────┐ ┌──────────┐ ┌──────────┐  │   │
│  │  │  Secret  │ │  Malware │ │  Prompt  │ │  Command │ │  Pattern │  │   │
│  │  │  Scan    │ │  Scan    │ │  Inject  │ │  Inject  │ │  Match   │  │   │
│  │  └──────────┘ └──────────┘ └──────────┘ └──────────┘ └──────────┘  │   │
│  └─────────────────────────────────────────────────────────────────────┘   │
│                                                                              │
└─────────────────────────────────────────────────────────────────────────────┘
```

### 7.2 Permission System Implementation

```rust
// src/safety/permissions.rs
pub struct PermissionSystem {
    grants: DashMap<String, PermissionGrant>,
    default_policy: PermissionPolicy,
}

#[derive(Debug, Clone)]
pub struct PermissionGrant {
    pub resource: ResourcePattern,
    pub action: ActionType,
    pub level: PermissionLevel,
    pub granted_by: String,
    pub granted_at: DateTime<Utc>,
    pub expires_at: Option<DateTime<Utc>>,
    pub conditions: Vec<PermissionCondition>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PermissionLevel { None, Read, Write, Execute, Admin }

impl PermissionSystem {
    pub async fn check_permission(
        &self,
        resource: &str,
        action: ActionType,
    ) -> Result<PermissionCheck, PermissionError> {
        for grant in self.grants.iter() {
            if grant.matches(resource, action) {
                if let Some(expires) = grant.expires_at {
                    if Utc::now() > expires { continue; }
                }
                let conditions_met = self.evaluate_conditions(&grant.conditions).await?;
                if conditions_met {
                    return Ok(PermissionCheck::Granted(grant.level));
                }
            }
        }
        
        match self.default_policy {
            PermissionPolicy::DenyAll => Ok(PermissionCheck::Denied("No permission".to_string())),
            PermissionPolicy::AllowRead => {
                if action == ActionType::Read {
                    Ok(PermissionCheck::Granted(PermissionLevel::Read))
                } else {
                    Ok(PermissionCheck::Denied("Write/Execute not allowed".to_string()))
                }
            }
            PermissionPolicy::AllowAll => Ok(PermissionCheck::Granted(PermissionLevel::Admin)),
        }
    }
    
    pub async fn request_permission(
        &self,
        resource: &str,
        action: ActionType,
        level: PermissionLevel,
    ) -> Result<PermissionGrant, PermissionError> {
        let request = PermissionRequest {
            resource: resource.to_string(), action, level, requested_at: Utc::now(),
        };
        let response = self.prompt_user_for_permission(request).await?;
        
        if response.approved {
            let grant = PermissionGrant {
                resource: ResourcePattern::from_string(resource)?,
                action, level,
                granted_by: response.user_id,
                granted_at: Utc::now(),
                expires_at: response.duration.map(|d| Utc::now() + d),
                conditions: response.conditions,
            };
            self.grants.insert(grant.resource.to_string(), grant.clone());
            Ok(grant)
        } else {
            Err(PermissionError::DeniedByUser)
        }
    }
}
```

### 7.3 Audit Logging

```rust
// src/safety/audit_logger.rs
pub struct AuditLogger {
    storage: Arc<dyn AuditStorage>,
    buffer: Arc<RwLock<Vec<AuditEvent>>>,
    flush_interval: Duration,
}

#[derive(Debug, Clone, Serialize)]
pub struct AuditEvent {
    pub id: Uuid,
    pub timestamp: DateTime<Utc>,
    pub event_type: AuditEventType,
    pub actor: Actor,
    pub resource: Resource,
    pub action: Action,
    pub result: ActionResult,
    pub metadata: Value,
}

#[derive(Debug, Clone, Serialize)]
pub enum AuditEventType {
    ToolExecution, FileAccess, NetworkRequest, CommandExecution,
    PermissionChange, UserApproval, SafetyViolation, SystemChange,
}

impl AuditLogger {
    pub async fn log(&self, event: AuditEvent) -> Result<(), AuditError> {
        self.buffer.write().await.push(event);
        if self.buffer.read().await.len() >= 100 {
            self.flush().await?;
        }
        Ok(())
    }
    
    pub async fn flush(&self) -> Result<(), AuditError> {
        let mut buffer = self.buffer.write().await;
        if buffer.is_empty() { return Ok(()); }
        self.storage.store_batch(&buffer).await?;
        buffer.clear();
        Ok(())
    }
    
    pub async fn query(&self, query: AuditQuery) -> Result<Vec<AuditEvent>, AuditError> {
        self.storage.query(query).await
    }
}
```

### 7.4 Human Approval Workflows

```rust
// src/safety/approval_workflow.rs
pub struct ApprovalWorkflow {
    pending: DashMap<Uuid, PendingApproval>,
    notification_service: Arc<dyn NotificationService>,
    timeout: Duration,
}

#[derive(Debug, Clone)]
pub struct PendingApproval {
    pub request: ApprovalRequest,
    pub requested_at: Instant,
    pub responder: oneshot::Sender<ApprovalResponse>,
}

#[derive(Debug, Clone)]
pub struct ApprovalRequest {
    pub id: Uuid,
    pub request_type: RequestType,
    pub description: String,
    pub risk_level: RiskLevel,
    pub details: Value,
    pub requested_by: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RiskLevel { Low, Medium, High, Critical }

impl ApprovalWorkflow {
    pub async fn request_approval(&self, request: ApprovalRequest) 
        -> Result<ApprovalResponse, ApprovalError> {
        let (tx, rx) = oneshot::channel();
        let pending = PendingApproval {
            request: request.clone(),
            requested_at: Instant::now(),
            responder: tx,
        };
        self.pending.insert(request.id, pending);
        
        self.notification_service.send_approval_request(&request).await?;
        
        match tokio::time::timeout(self.timeout, rx).await {
            Ok(Ok(response)) => { self.pending.remove(&request.id); Ok(response) }
            Ok(Err(_)) => { self.pending.remove(&request.id); Err(ApprovalError::ChannelClosed) }
            Err(_) => {
                self.pending.remove(&request.id);
                Ok(ApprovalResponse {
                    request_id: request.id,
                    approved: false,
                    reason: Some("Approval timeout".to_string()),
                    responded_by: None,
                })
            }
        }
    }
    
    pub async fn respond_to_approval(
        &self,
        request_id: Uuid,
        approved: bool,
        reason: Option<String>,
        responder: String,
    ) -> Result<(), ApprovalError> {
        let (_, pending) = self.pending.remove(&request_id)
            .ok_or(ApprovalError::RequestNotFound)?;
        
        let response = ApprovalResponse {
            request_id, approved, reason, responded_by: Some(responder),
        };
        pending.responder.send(response).map_err(|_| ApprovalError::ChannelClosed)?;
        Ok(())
    }
    
    pub fn get_pending(&self) -> Vec<ApprovalRequest> {
        self.pending.iter().map(|p| p.request.clone()).collect()
    }
}
```

### 7.5 Rate Limiting and Abuse Prevention

```rust
// src/safety/rate_limiter.rs
pub struct RateLimiter {
    store: Arc<dyn RateLimitStore>,
    rules: HashMap<String, RateLimitRule>,
}

#[derive(Debug, Clone)]
pub struct RateLimitRule {
    pub resource: String,
    pub max_requests: u32,
    pub window: Duration,
    pub burst_size: Option<u32>,
}

#[derive(Debug, Clone)]
pub struct RateLimitCheck {
    pub allowed: bool,
    pub remaining: u32,
    pub reset_at: DateTime<Utc>,
    pub retry_after: Option<Duration>,
}

impl RateLimiter {
    pub async fn check(&self, resource: &str, key: &str) 
        -> Result<RateLimitCheck, RateLimitError> {
        let rule = self.rules.get(resource)
            .ok_or_else(|| RateLimitError::RuleNotFound(resource.to_string()))?;
        
        let now = Utc::now();
        let window_start = now - rule.window;
        let count = self.store.count_requests(resource, key, window_start, now).await?;
        
        if count >= rule.max_requests {
            let oldest_request = self.store.get_oldest_request(resource, key).await?;
            let reset_at = oldest_request + rule.window;
            let retry_after = reset_at - now;
            
            return Ok(RateLimitCheck {
                allowed: false, remaining: 0, reset_at,
                retry_after: Some(retry_after.to_std().unwrap_or(Duration::from_secs(0))),
            });
        }
        
        self.store.record_request(resource, key, now).await?;
        
        Ok(RateLimitCheck {
            allowed: true,
            remaining: rule.max_requests - count - 1,
            reset_at: now + rule.window,
            retry_after: None,
        })
    }
    
    pub async fn check_with_cost(&self, resource: &str, key: &str, cost: u32) 
        -> Result<RateLimitCheck, RateLimitError> {
        let rule = self.rules.get(resource)
            .ok_or_else(|| RateLimitError::RuleNotFound(resource.to_string()))?;
        
        let now = Utc::now();
        let window_start = now - rule.window;
        let usage = self.store.get_usage(resource, key, window_start, now).await?;
        
        if usage + cost > rule.max_requests {
            let reset_at = self.store.get_reset_time(resource, key, rule.window).await?;
            return Ok(RateLimitCheck {
                allowed: false,
                remaining: rule.max_requests.saturating_sub(usage), reset_at,
                retry_after: Some((reset_at - now).to_std().unwrap_or(Duration::from_secs(0))),
            });
        }
        
        self.store.record_usage(resource, key, cost, now).await?;
        
        Ok(RateLimitCheck {
            allowed: true,
            remaining: rule.max_requests - usage - cost,
            reset_at: now + rule.window,
            retry_after: None,
        })
    }
}
```
