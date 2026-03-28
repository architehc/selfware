# Command Center Architecture
## Real-Time Agent Orchestration Dashboard

### Overview

A TUI/GUI hybrid that provides "Mission Control" visibility into running agent workflows.

```
┌─────────────────────────────────────────────────────────────────────────────┐
│ SELFWARE COMMAND CENTER                           [Live] [▶ Recording] [?]  │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                             │
│  ┌──────────────────────┐  ┌──────────────────────┐  ┌──────────────────┐  │
│  │ INFERENCE LATENCY    │  │ CONTEXT WINDOW       │  │ AGENT TOPOLOGY   │  │
│  │                      │  │                      │  │                  │  │
│  │  ████░░░░░░░░░░░░░   │  │    ╭─────────╮       │  │     ┌───┐        │  │
│  │  ████████░░░░░░░░░   │  │   ╱    45%   ╲      │  │  ┌──┤ A ├─┐      │  │
│  │  ██████████░░░░░░░   │  │  │   ▓▓▓▓░   │      │  │  │  └───┘ │      │  │
│  │  ████████████░░░░░   │  │  │  12K/32K  │      │  │  ▼        ▼      │  │
│  │  ██████████████░░░   │  │   ╲         ╱       │  │ ┌─┐    ┌───┐     │  │
│  │                      │  │    ╰─────────╯       │  │ │B│    │ C │     │  │
│  │  p50: 245ms          │  │                      │  │ └─┘    └───┘     │  │
│  │  p99: 1.2s ⚠️        │  │  architect @ 45%    │  │                  │  │
│  │                      │  │  security @ 23%     │  │  A=architect     │  │
│  └──────────────────────┘  └──────────────────────┘  │  B=security      │  │
│                                                      │  C=performance   │  │
│  ┌──────────────────────┐  ┌──────────────────────┐  └──────────────────┘  │
│  │ TOKEN THROUGHPUT     │  │ LIVE LOG STREAM      │  │ COST TRACKER     │  │
│  │                      │  │                      │  │                  │  │
│  │  ██████████████████  │  │ 14:32:01 [architect] │  │  Session: $4.23  │  │
│  │  ██████████████████  │  │ > Analyzing mod.rs   │  │  Budget: $10.00  │  │
│  │  ████████████████░░  │  │ 14:32:02 [security]  │  │  ████████░░░░░░  │  │
│  │  ██████████████░░░░  │  │ > Found 2 issues     │  │                  │  │
│  │  ████████████░░░░░░  │  │ 14:32:03 [perf]      │  │  42% used        │  │
│  │                      │  │ > Check complete     │  │                  │  │
│  │  2,450 tok/s 🔥      │  │                      │  │  Rate: $0.42/min │  │
│  │                      │  │ 3 agents active      │  │                  │  │
│  └──────────────────────┘  └──────────────────────┘  └──────────────────┘  │
│                                                                             │
├─────────────────────────────────────────────────────────────────────────────┤
│ PROMPT: [Type command or agent query]                    [Send] [Stop All]  │
└─────────────────────────────────────────────────────────────────────────────┘
```

### Component Architecture

```rust
// telemetry_collector.rs
pub struct TelemetryCollector {
    /// Ring buffer for metrics (keeps last N data points)
    metrics: Arc<RwLock<MetricsRingBuffer>>,
    
    /// WebSocket connections to dashboard clients
    subscribers: Arc<RwLock<Vec<WebSocketSender>>>,
    
    /// Aggregation windows
    windows: HashMap<MetricName, AggregationWindow>,
}

impl TelemetryCollector {
    pub async fn record(&self, event: TelemetryEvent) {
        // 1. Store in ring buffer
        self.metrics.write().push(event.clone());
        
        // 2. Update aggregation windows
        self.aggregate(&event);
        
        // 3. Broadcast to dashboard clients
        self.broadcast(event).await;
    }
}

// Metric types
#[derive(Clone, Debug)]
pub enum TelemetryEvent {
    InferenceStart {
        agent_id: String,
        model: String,
        input_tokens: usize,
        timestamp: Instant,
    },
    InferenceComplete {
        agent_id: String,
        output_tokens: usize,
        latency: Duration,
        cost: f64,
    },
    ContextWindowUpdate {
        agent_id: String,
        used_tokens: usize,
        max_tokens: usize,
        percentage: f32,
    },
    ToolInvocation {
        agent_id: String,
        tool_name: String,
        args: Value,
        start_time: Instant,
    },
    ToolComplete {
        agent_id: String,
        tool_name: String,
        result: Value,
        duration: Duration,
    },
    Delegation {
        from_agent: String,
        to_agent: String,
        reason: String,
    },
    GuardrailViolation {
        guardrail_name: String,
        violation_type: String,
        context: Value,
    },
}
```

### Dashboard Server

```rust
// dashboard_server.rs
pub struct DashboardServer {
    collector: Arc<TelemetryCollector>,
    port: u16,
}

impl DashboardServer {
    pub async fn run(&self) -> Result<()> {
        let app = Router::new()
            // WebSocket for real-time updates
            .route("/ws", get(ws_handler))
            // REST API for historical data
            .route("/api/metrics", get(metrics_handler))
            .route("/api/traces/:trace_id", get(trace_handler))
            // Static files for web UI
            .route("/", get(index_handler))
            .layer(Extension(self.collector.clone()));
            
        axum::Server::bind(&([0,0,0,0], self.port).into())
            .serve(app.into_make_service())
            .await?;
            
        Ok(())
    }
}

// WebSocket handler pushes updates
async fn ws_handler(
    ws: WebSocketUpgrade,
    Extension(collector): Extension<Arc<TelemetryCollector>>,
) -> impl IntoResponse {
    ws.on_upgrade(|socket| {
        collector.add_subscriber(socket).await
    })
}
```

### TUI Dashboard (ratatui)

```rust
// tui_dashboard.rs
pub struct DashboardApp {
    // Data models
    latency_data: Vec<(Instant, f64)>,  // Time series
    context_usage: HashMap<String, f32>, // Agent -> percentage
    topology: TopologyGraph,
    logs: Vec<LogEntry>,
    
    // UI state
    selected_panel: Panel,
    paused: bool,
    show_help: bool,
    
    // Event source
    event_receiver: mpsc::Receiver<TelemetryEvent>,
}

impl DashboardApp {
    pub fn draw(&mut self, frame: &mut Frame) {
        let layout = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3),  // Header
                Constraint::Min(10),    // Main content
                Constraint::Length(3),  // Footer
            ])
            .split(frame.size());
            
        self.draw_header(frame, layout[0]);
        self.draw_main(frame, layout[1]);
        self.draw_footer(frame, layout[2]);
    }
    
    fn draw_latency_panel(&self, frame: &mut Frame, area: Rect) {
        // Sparkline for latency over time
        let sparkline = Sparkline::default()
            .block(Block::default().title("Latency (ms)").borders(Borders::ALL))
            .data(&self.latency_data.iter().map(|(_, v)| *v as u64).collect::<Vec<_>>())
            .style(Style::default().fg(Color::Yellow))
            .max(1000);
            
        frame.render_widget(sparkline, area);
    }
    
    fn draw_topology_panel(&self, frame: &mut Frame, area: Rect) {
        // Custom canvas for agent graph
        let canvas = Canvas::default()
            .block(Block::default().title("Agent Topology").borders(Borders::ALL))
            .paint(|ctx| {
                // Draw nodes (agents)
                for (id, pos) in &self.topology.nodes {
                    ctx.draw(&Circle {
                        x: pos.x,
                        y: pos.y,
                        radius: 2.0,
                        color: self.agent_color(id),
                    });
                }
                
                // Draw edges (delegations)
                for edge in &self.topology.edges {
                    ctx.draw(&Line {
                        x1: edge.from.x,
                        y1: edge.from.y,
                        x2: edge.to.x,
                        y2: edge.to.y,
                        color: Color::Gray,
                    });
                }
            })
            .x_bounds([0.0, 100.0])
            .y_bounds([0.0, 100.0]);
            
        frame.render_widget(canvas, area);
    }
}
```

### Integration with SWL

```yaml
# workflow.swl with telemetry hooks
workflow:
  name: security_audit
  
  agents:
    - name: scanner
      model: gemini-2.5-flash
      
      # Inline telemetry configuration
      telemetry:
        export:
          - metric: inference_latency
            aggregation: histogram
            
          - metric: context_window
            export_every: 5s
            
          - trace: full_request
            sample_rate: 1.0
            
        alerts:
          - condition: latency > 2s
            action: notify
            
          - condition: context_window > 80%
            action: escalate

  # Dashboard auto-generates from this spec
  dashboard:
    auto_generate: true
    panels:
      - metric: inference_latency
        type: heatmap
        
      - metric: context_window
        type: gauge
        agent_selector: all
```

### Streaming Protocol

```typescript
// WebSocket message protocol
interface TelemetryMessage {
  type: 'metric' | 'trace' | 'log' | 'topology';
  timestamp: number;
  data: MetricData | TraceData | LogData | TopologyData;
}

interface MetricData {
  name: string;
  value: number;
  labels: Record<string, string>;
  aggregation?: 'instant' | 'window';
}

interface TraceData {
  trace_id: string;
  span_id: string;
  parent_span_id?: string;
  operation: string;
  start_time: number;
  duration_ms: number;
  status: 'ok' | 'error';
  attributes: Record<string, any>;
}

// Client subscription
interface SubscribeRequest {
  filters: {
    agents?: string[];
    metrics?: string[];
    min_severity?: 'debug' | 'info' | 'warn' | 'error';
  };
  aggregation: {
    window_ms: number;
    functions: ('avg' | 'p50' | 'p99' | 'max')[];
  };
}
```

### Deployment Modes

```rust
// Standalone mode (local development)
$ swl run workflow.swl --dashboard
→ Starts dashboard on http://localhost:8080

// Headless mode (CI/CD)
$ swl run workflow.swl --output json
→ Exports metrics to stdout/file

// Remote dashboard (production)
$ swl agent --connect ws://dashboard.corp.internal:8080/ws
→ Streams telemetry to central command center
```
