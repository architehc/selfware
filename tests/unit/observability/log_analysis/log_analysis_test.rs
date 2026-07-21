use super::*;

#[test]
fn test_log_level_from_str() {
    assert_eq!(LogLevel::from_str("ERROR"), LogLevel::Error);
    assert_eq!(LogLevel::from_str("warn"), LogLevel::Warn);
    assert_eq!(LogLevel::from_str("INFO"), LogLevel::Info);
}

#[test]
fn test_log_level_is_error() {
    assert!(LogLevel::Error.is_error());
    assert!(LogLevel::Fatal.is_error());
    assert!(!LogLevel::Warn.is_error());
}

#[test]
fn test_log_entry_new() {
    let entry = LogEntry::new(LogLevel::Info, "app", "Test message");
    assert_eq!(entry.level, LogLevel::Info);
    assert_eq!(entry.source, "app");
    assert_eq!(entry.message, "Test message");
}

#[test]
fn test_log_entry_with_field() {
    let entry = LogEntry::new(LogLevel::Info, "app", "msg").with_field("key", "value");
    assert_eq!(entry.fields.get("key"), Some(&"value".to_string()));
}

#[test]
fn test_log_parser_json() {
    let parser = LogParser::new(LogFormat::Json);
    let line = r#"{"level":"error","message":"Test error","source":"app"}"#;

    let entry = parser.parse(line).unwrap();
    assert_eq!(entry.level, LogLevel::Error);
    assert_eq!(entry.message, "Test error");
}

#[test]
fn test_log_parser_plain() {
    let parser = LogParser::new(LogFormat::Plain);
    let line = "[ERROR] Something went wrong";

    let entry = parser.parse(line).unwrap();
    assert_eq!(entry.level, LogLevel::Error);
}

#[test]
fn test_pattern_detector_process() {
    let detector = PatternDetector::default();

    for _ in 0..5 {
        let entry = LogEntry::new(LogLevel::Error, "app", "Connection failed to 192.168.1.1");
        detector.process(&entry);
    }

    let summary = detector.summary();
    assert!(summary.logs_processed >= 5);
}

#[test]
fn test_pattern_detector_top_patterns() {
    let detector = PatternDetector::default();

    for i in 0..10 {
        let entry = LogEntry::new(LogLevel::Info, "app", &format!("Request {} processed", i));
        detector.process(&entry);
    }

    let top = detector.top_patterns(5);
    assert!(!top.is_empty());
}

#[test]
fn test_anomaly_new() {
    let anomaly = Anomaly::new(AnomalyType::ErrorSpike, 0.8, "Test anomaly");
    assert_eq!(anomaly.anomaly_type, AnomalyType::ErrorSpike);
    assert_eq!(anomaly.severity, 0.8);
}

#[test]
fn test_anomaly_detector_analyze() {
    let detector = AnomalyDetector::default();
    let entry = LogEntry::new(LogLevel::Error, "new-source", "Error message");

    // First entry from new source after baseline shouldn't trigger
    let _result = detector.analyze(&entry);

    let summary = detector.summary();
    assert_eq!(summary.logs_analyzed, 1);
}

#[test]
fn test_root_cause_analyzer() {
    let analyzer = RootCauseAnalyzer::default();

    let entries = vec![LogEntry::new(
        LogLevel::Error,
        "app",
        "Connection refused to localhost:5432",
    )];

    let causes = analyzer.analyze(&entries);
    assert!(!causes.is_empty());
    assert_eq!(causes[0].category, RootCauseCategory::Network);
}

#[test]
fn test_alert_new() {
    let alert = Alert::new("Test alert", AlertSeverity::High, "app");
    assert_eq!(alert.title, "Test alert");
    assert_eq!(alert.severity, AlertSeverity::High);
    assert_eq!(alert.status, AlertStatus::Open);
}

#[test]
fn test_alert_correlator_process() {
    let correlator = AlertCorrelator::default();

    let alert = correlator.process("Error 1", AlertSeverity::High, "app", vec![1, 2]);
    assert_eq!(alert.title, "Error 1");

    let summary = correlator.summary();
    assert_eq!(summary.alerts_created, 1);
}

#[test]
fn test_alert_correlator_resolve() {
    let correlator = AlertCorrelator::default();
    let alert = correlator.process("Error", AlertSeverity::High, "app", vec![]);

    correlator.resolve(&alert.id);

    let open = correlator.open_alerts();
    assert!(open.is_empty());
}

#[test]
fn test_log_analyzer_process_line() {
    let analyzer = LogAnalyzer::new(LogFormat::Plain);
    let entry = analyzer.process_line("[ERROR] Test error message");

    assert!(entry.is_some());
    assert_eq!(entry.unwrap().level, LogLevel::Error);
}

#[test]
fn test_log_analyzer_summary() {
    let analyzer = LogAnalyzer::default();
    analyzer.process_line("[INFO] Test message");

    let summary = analyzer.summary();
    assert_eq!(summary.logs_stored, 1);
}

#[test]
fn test_anomaly_type_enum() {
    assert_eq!(AnomalyType::ErrorSpike, AnomalyType::ErrorSpike);
    assert_ne!(AnomalyType::ErrorSpike, AnomalyType::NewError);
}

#[test]
fn test_alert_severity_ordering() {
    assert!(AlertSeverity::Critical > AlertSeverity::High);
    assert!(AlertSeverity::High > AlertSeverity::Medium);
    assert!(AlertSeverity::Medium > AlertSeverity::Low);
}

// Additional tests for comprehensive coverage

#[test]
fn test_log_level_all_variants() {
    assert_eq!(LogLevel::from_str("TRACE"), LogLevel::Trace);
    assert_eq!(LogLevel::from_str("DEBUG"), LogLevel::Debug);
    assert_eq!(LogLevel::from_str("WARNING"), LogLevel::Warn);
    assert_eq!(LogLevel::from_str("ERR"), LogLevel::Error);
    assert_eq!(LogLevel::from_str("CRITICAL"), LogLevel::Fatal);
    assert_eq!(LogLevel::from_str("CRIT"), LogLevel::Fatal);
    assert_eq!(LogLevel::from_str("unknown"), LogLevel::Info);
}

#[test]
fn test_log_level_clone_debug() {
    let level = LogLevel::Error;
    let cloned = level;
    assert_eq!(level, cloned);
    let debug_str = format!("{:?}", level);
    assert!(debug_str.contains("Error"));
}

#[test]
fn test_log_level_ordering() {
    assert!(LogLevel::Fatal > LogLevel::Error);
    assert!(LogLevel::Error > LogLevel::Warn);
    assert!(LogLevel::Warn > LogLevel::Info);
    assert!(LogLevel::Info > LogLevel::Debug);
    assert!(LogLevel::Debug > LogLevel::Trace);
}

#[test]
fn test_log_entry_with_raw() {
    let entry = LogEntry::new(LogLevel::Info, "app", "msg").with_raw("raw log line");
    assert_eq!(entry.raw, "raw log line");
}

#[test]
fn test_log_entry_serialization() {
    let entry = LogEntry::new(LogLevel::Error, "app", "test message").with_field("key", "value");

    let json = serde_json::to_string(&entry).unwrap();
    let deserialized: LogEntry = serde_json::from_str(&json).unwrap();

    assert_eq!(deserialized.level, entry.level);
    assert_eq!(deserialized.source, entry.source);
    assert_eq!(deserialized.message, entry.message);
}

#[test]
fn test_log_format_variants() {
    let formats = [
        LogFormat::Plain,
        LogFormat::Json,
        LogFormat::CommonLog,
        LogFormat::Syslog,
        LogFormat::Custom,
    ];
    for fmt in formats {
        let _ = format!("{:?}", fmt);
        let cloned = fmt;
        assert_eq!(fmt, cloned);
    }
}

#[test]
fn test_log_parser_default() {
    let parser = LogParser::default();
    assert_eq!(parser.format, LogFormat::Plain);
}

#[test]
fn test_log_parser_add_pattern() {
    let mut parser = LogParser::new(LogFormat::Custom);
    parser.add_pattern("level", r"\[(\w+)\]");
    parser.add_pattern("message", r": (.+)$");

    let line = "[ERROR]: Something went wrong";
    let entry = parser.parse(line);
    assert!(entry.is_some());
}

#[test]
fn test_log_parser_common_format() {
    let parser = LogParser::new(LogFormat::CommonLog);
    let line =
        "127.0.0.1 - frank [10/Oct/2000:13:55:36 -0700] \"GET /apache_pb.gif HTTP/1.0\" 200 2326";

    let entry = parser.parse(line);
    assert!(entry.is_some());
    let e = entry.unwrap();
    assert_eq!(e.source, "httpd");
}

#[test]
fn test_log_parser_syslog_format() {
    let parser = LogParser::new(LogFormat::Syslog);
    let line = "Oct 11 22:14:15 mymachine app[1234]: Test message";

    let entry = parser.parse(line);
    assert!(entry.is_some());
}

#[test]
fn test_log_parser_json_with_alternatives() {
    let parser = LogParser::new(LogFormat::Json);

    // Test with 'msg' instead of 'message'
    let line1 = r#"{"severity":"warn","msg":"Warning message"}"#;
    let entry1 = parser.parse(line1);
    assert!(entry1.is_some());
    assert_eq!(entry1.unwrap().level, LogLevel::Warn);

    // Test with 'ts' timestamp
    let line2 = r#"{"level":"info","message":"Test","ts":1234567890}"#;
    let entry2 = parser.parse(line2);
    assert!(entry2.is_some());
}

#[test]
fn test_pattern_detector_error_patterns() {
    let detector = PatternDetector::default();

    let error_entry = LogEntry::new(LogLevel::Error, "app", "Database connection failed");
    let info_entry = LogEntry::new(LogLevel::Info, "app", "Request processed");

    detector.process(&error_entry);
    detector.process(&info_entry);

    let error_patterns = detector.error_patterns();
    assert_eq!(error_patterns.len(), 1);
}

#[test]
fn test_pattern_detector_clear() {
    let detector = PatternDetector::default();

    let entry = LogEntry::new(LogLevel::Info, "app", "Test message");
    detector.process(&entry);
    assert!(detector.summary().patterns_detected >= 1);

    detector.clear();
    assert_eq!(detector.top_patterns(10).len(), 0);
}

#[test]
fn test_pattern_summary_clone() {
    let summary = PatternSummary {
        logs_processed: 100,
        patterns_detected: 10,
        pattern_matches: 50,
        unique_patterns: 10,
    };

    let cloned = summary.clone();
    assert_eq!(summary.logs_processed, cloned.logs_processed);
}

#[test]
fn test_anomaly_with_related_logs() {
    let anomaly =
        Anomaly::new(AnomalyType::ErrorSpike, 0.9, "Test").with_related_logs(vec![1, 2, 3]);

    assert_eq!(anomaly.related_logs, vec![1, 2, 3]);
}

#[test]
fn test_anomaly_with_action() {
    let anomaly = Anomaly::new(AnomalyType::FrequencyAnomaly, 0.7, "Test")
        .with_action("Investigate immediately");

    assert_eq!(
        anomaly.suggested_action,
        Some("Investigate immediately".to_string())
    );
}

#[test]
fn test_anomaly_type_all_variants() {
    let types = [
        AnomalyType::ErrorSpike,
        AnomalyType::FrequencyAnomaly,
        AnomalyType::NewError,
        AnomalyType::MissingLogs,
        AnomalyType::UnusualSource,
        AnomalyType::TimingAnomaly,
    ];

    for t in types {
        let _ = format!("{:?}", t);
        let cloned = t;
        assert_eq!(t, cloned);
    }
}

#[test]
fn test_anomaly_detector_recent_anomalies() {
    let detector = AnomalyDetector::default();
    let anomalies = detector.recent_anomalies(10);
    assert!(anomalies.is_empty());
}

#[test]
fn test_anomaly_detector_clear() {
    let detector = AnomalyDetector::default();
    detector.clear();
    assert!(detector.recent_anomalies(10).is_empty());
}

#[test]
fn test_anomaly_summary_clone() {
    let summary = AnomalySummary {
        logs_analyzed: 100,
        anomalies_detected: 5,
        known_sources: 3,
    };

    let cloned = summary.clone();
    assert_eq!(summary.anomalies_detected, cloned.anomalies_detected);
}

#[test]
fn test_root_cause_category_all_variants() {
    let categories = [
        RootCauseCategory::Configuration,
        RootCauseCategory::Resource,
        RootCauseCategory::Network,
        RootCauseCategory::Dependency,
        RootCauseCategory::Code,
        RootCauseCategory::Data,
        RootCauseCategory::Unknown,
    ];

    for cat in categories {
        let _ = format!("{:?}", cat);
        let cloned = cat;
        assert_eq!(cat, cloned);
    }
}

#[test]
fn test_root_cause_analyzer_patterns() {
    let analyzer = RootCauseAnalyzer::default();

    // Out of memory
    let entries1 = vec![LogEntry::new(LogLevel::Error, "app", "OOM killed process")];
    let causes1 = analyzer.analyze(&entries1);
    assert!(!causes1.is_empty());
    assert_eq!(causes1[0].category, RootCauseCategory::Resource);

    // Timeout
    let entries2 = vec![LogEntry::new(LogLevel::Error, "app", "Request timed out")];
    let causes2 = analyzer.analyze(&entries2);
    assert!(!causes2.is_empty());

    // Permission denied
    let entries3 = vec![LogEntry::new(
        LogLevel::Error,
        "app",
        "Permission denied accessing file",
    )];
    let causes3 = analyzer.analyze(&entries3);
    assert!(!causes3.is_empty());
    assert_eq!(causes3[0].category, RootCauseCategory::Configuration);

    // Disk full
    let entries4 = vec![LogEntry::new(
        LogLevel::Error,
        "app",
        "No space left on device",
    )];
    let causes4 = analyzer.analyze(&entries4);
    assert!(!causes4.is_empty());
    assert_eq!(causes4[0].category, RootCauseCategory::Resource);
}

#[test]
fn test_root_cause_analyzer_recent() {
    let analyzer = RootCauseAnalyzer::default();
    let entries = vec![LogEntry::new(LogLevel::Error, "app", "Connection refused")];
    analyzer.analyze(&entries);

    let recent = analyzer.recent_analyses(10);
    assert!(!recent.is_empty());
}

#[test]
fn test_alert_status_variants() {
    let statuses = [
        AlertStatus::Open,
        AlertStatus::Acknowledged,
        AlertStatus::Resolved,
        AlertStatus::Suppressed,
    ];

    for status in statuses {
        let _ = format!("{:?}", status);
        let cloned = status;
        assert_eq!(status, cloned);
    }
}

#[test]
fn test_alert_correlator_correlation_window() {
    let correlator = AlertCorrelator::new(60); // 1 minute window

    let alert1 = correlator.process("Error 1", AlertSeverity::High, "app", vec![1]);
    let alert2 = correlator.process("Error 2", AlertSeverity::High, "app", vec![2]);

    // Second alert should be correlated with first
    assert_eq!(alert2.id, alert1.id);
}

#[test]
fn test_alert_correlator_default() {
    let correlator = AlertCorrelator::default();
    let summary = correlator.summary();
    assert_eq!(summary.alerts_created, 0);
}

#[test]
fn test_correlator_summary_clone() {
    let summary = CorrelatorSummary {
        alerts_created: 10,
        alerts_correlated: 3,
        alerts_resolved: 2,
        open_alerts: 5,
    };

    let cloned = summary.clone();
    assert_eq!(summary.alerts_created, cloned.alerts_created);
}

#[test]
fn test_log_analyzer_components() {
    let analyzer = LogAnalyzer::default();

    let _patterns = analyzer.patterns();
    let _anomalies = analyzer.anomalies();
    let _alerts = analyzer.alerts();
}

#[test]
fn test_log_analyzer_analyze_root_cause() {
    let analyzer = LogAnalyzer::new(LogFormat::Plain);
    analyzer.process_line("[ERROR] Connection refused to database");

    let causes = analyzer.analyze_root_cause();
    assert!(!causes.is_empty());
}

#[test]
fn test_log_analyzer_summary_clone() {
    let analyzer = LogAnalyzer::default();
    let summary = analyzer.summary();
    let cloned = summary.clone();
    assert_eq!(summary.logs_stored, cloned.logs_stored);
}

#[test]
fn test_log_pattern_clone() {
    let pattern = LogPattern {
        id: "pat_1".to_string(),
        template: "Error <NUM>".to_string(),
        count: 5,
        first_seen: 1000,
        last_seen: 2000,
        examples: vec!["Error 1".to_string()],
        level: LogLevel::Error,
    };

    let cloned = pattern.clone();
    assert_eq!(pattern.id, cloned.id);
    assert_eq!(pattern.count, cloned.count);
}

#[test]
fn test_anomaly_severity_clamping() {
    let anomaly1 = Anomaly::new(AnomalyType::ErrorSpike, 1.5, "Test");
    assert_eq!(anomaly1.severity, 1.0);

    let anomaly2 = Anomaly::new(AnomalyType::ErrorSpike, -0.5, "Test");
    assert_eq!(anomaly2.severity, 0.0);
}

#[test]
fn test_pattern_detector_threshold() {
    let detector1 = PatternDetector::new(0.5);
    let detector2 = PatternDetector::new(1.5); // Should clamp to 1.0
    let detector3 = PatternDetector::new(-0.5); // Should clamp to 0.0

    assert_eq!(detector1.threshold, 0.5);
    assert_eq!(detector2.threshold, 1.0);
    assert_eq!(detector3.threshold, 0.0);
}
