use super::*;
use crate::tools::Tool;

// =========================================================================
// parse_build_output tests
// =========================================================================

#[test]
fn test_parse_build_output_successfully_built() {
    let stdout = "Step 3/3: COPY . /app\nSuccessfully built abc123def456";
    assert_eq!(
        parse_build_output(stdout, ""),
        Some("abc123def456".to_string())
    );
}

#[test]
fn test_parse_build_output_sha256() {
    let stderr = "writing image sha256:deadbeef01234567890";
    assert_eq!(
        parse_build_output("", stderr),
        Some("deadbeef01234567890".to_string())
    );
}

#[test]
fn test_parse_build_output_sha256_with_trailing_text() {
    let stderr = "writing image sha256:abc123 done";
    assert_eq!(parse_build_output("", stderr), Some("abc123".to_string()));
}

#[test]
fn test_parse_build_output_no_match() {
    assert_eq!(
        parse_build_output("just some log output", "another line"),
        None
    );
}

#[test]
fn test_parse_build_output_empty() {
    assert_eq!(parse_build_output("", ""), None);
}

#[test]
fn test_parse_build_output_in_stderr() {
    let stderr = "Step 1/3: FROM ubuntu\nStep 2/3: RUN apt-get update\nSuccessfully built xyz789";
    assert_eq!(parse_build_output("", stderr), Some("xyz789".to_string()));
}

#[test]
fn test_parse_build_output_prefers_first_match_in_stdout() {
    let stdout = "Successfully built first_id\nSuccessfully built second_id";
    assert_eq!(parse_build_output(stdout, ""), Some("first_id".to_string()));
}

// =========================================================================
// truncate_output tests
// =========================================================================

#[test]
fn test_truncate_output_short() {
    let short = "hello world";
    assert_eq!(truncate_output(short, 100), short);
}

#[test]
fn test_truncate_output_exact() {
    let s = "12345";
    assert_eq!(truncate_output(s, 5), "12345");
}

#[test]
fn test_truncate_output_long() {
    let long = "x".repeat(1000);
    let result = truncate_output(&long, 50);
    assert!(result.len() < 1000);
    assert!(result.contains("truncated"));
}

#[test]
fn test_truncate_output_empty() {
    assert_eq!(truncate_output("", 100), "");
}

// =========================================================================
// Tool name tests
// =========================================================================

#[test]
fn test_container_run_name() {
    assert_eq!(ContainerRun.name(), "container_run");
}

#[test]
fn test_container_stop_name() {
    assert_eq!(ContainerStop.name(), "container_stop");
}

#[test]
fn test_container_list_name() {
    assert_eq!(ContainerList.name(), "container_list");
}

#[test]
fn test_container_logs_name() {
    assert_eq!(ContainerLogs.name(), "container_logs");
}

#[test]
fn test_container_exec_name() {
    assert_eq!(ContainerExec.name(), "container_exec");
}

#[test]
fn test_container_build_name() {
    assert_eq!(ContainerBuild.name(), "container_build");
}

#[test]
fn test_container_images_name() {
    assert_eq!(ContainerImages.name(), "container_images");
}

#[test]
fn test_container_pull_name() {
    assert_eq!(ContainerPull.name(), "container_pull");
}

#[test]
fn test_container_remove_name() {
    assert_eq!(ContainerRemove.name(), "container_remove");
}

#[test]
fn test_compose_up_name() {
    assert_eq!(ComposeUp.name(), "compose_up");
}

// =========================================================================
// Tool description tests
// =========================================================================

#[test]
fn test_all_descriptions_non_empty() {
    assert!(!ContainerRun.description().is_empty());
    assert!(!ContainerStop.description().is_empty());
    assert!(!ContainerList.description().is_empty());
    assert!(!ContainerLogs.description().is_empty());
    assert!(!ContainerExec.description().is_empty());
    assert!(!ContainerBuild.description().is_empty());
    assert!(!ContainerImages.description().is_empty());
    assert!(!ContainerPull.description().is_empty());
    assert!(!ContainerRemove.description().is_empty());
    assert!(!ComposeUp.description().is_empty());
}

// =========================================================================
// Tool schema tests
// =========================================================================

#[test]
fn test_container_run_schema_has_image() {
    let schema = ContainerRun.schema();
    assert!(schema["properties"].get("image").is_some());
    let required = schema["required"].as_array().unwrap();
    assert!(required.contains(&json!("image")));
}

#[test]
fn test_container_run_schema_has_ports() {
    let schema = ContainerRun.schema();
    assert!(schema["properties"].get("ports").is_some());
}

#[test]
fn test_container_run_schema_has_volumes() {
    let schema = ContainerRun.schema();
    assert!(schema["properties"].get("volumes").is_some());
}

#[test]
fn test_container_run_schema_has_env() {
    let schema = ContainerRun.schema();
    assert!(schema["properties"].get("env").is_some());
}

#[test]
fn test_container_stop_schema_has_container() {
    let schema = ContainerStop.schema();
    assert!(schema["properties"].get("container").is_some());
    let required = schema["required"].as_array().unwrap();
    assert!(required.contains(&json!("container")));
}

#[test]
fn test_container_exec_schema_has_command() {
    let schema = ContainerExec.schema();
    assert!(schema["properties"].get("command").is_some());
    let required = schema["required"].as_array().unwrap();
    assert!(required.contains(&json!("container")));
    assert!(required.contains(&json!("command")));
}

#[test]
fn test_container_build_schema_has_tag() {
    let schema = ContainerBuild.schema();
    assert!(schema["properties"].get("tag").is_some());
    let required = schema["required"].as_array().unwrap();
    assert!(required.contains(&json!("tag")));
}

#[test]
fn test_container_pull_schema_has_image() {
    let schema = ContainerPull.schema();
    assert!(schema["properties"].get("image").is_some());
    let required = schema["required"].as_array().unwrap();
    assert!(required.contains(&json!("image")));
}

#[test]
fn test_container_remove_schema_has_force() {
    let schema = ContainerRemove.schema();
    assert!(schema["properties"].get("force").is_some());
}

#[test]
fn test_container_logs_schema_has_tail() {
    let schema = ContainerLogs.schema();
    assert!(schema["properties"].get("tail").is_some());
    assert!(schema["properties"].get("since").is_some());
}

#[test]
fn test_compose_up_schema_has_path() {
    let schema = ComposeUp.schema();
    assert!(schema["properties"].get("path").is_some());
    assert!(schema["properties"].get("services").is_some());
}

// =========================================================================
// ContainerInfo serialization tests
// =========================================================================

#[test]
fn test_container_info_serialization() {
    let info = ContainerInfo {
        id: "abc123".to_string(),
        image: "nginx:latest".to_string(),
        command: "/docker-entrypoint.sh".to_string(),
        created: "2024-01-01".to_string(),
        status: "Up 5 minutes".to_string(),
        ports: "0.0.0.0:80->80/tcp".to_string(),
        names: "my-nginx".to_string(),
    };
    let json = serde_json::to_string(&info).unwrap();
    assert!(json.contains("abc123"));
    assert!(json.contains("nginx:latest"));
    assert!(json.contains("my-nginx"));
}

#[test]
fn test_image_info_serialization() {
    let info = ImageInfo {
        id: "sha256:abc".to_string(),
        repository: "nginx".to_string(),
        tag: "latest".to_string(),
        created: "3 days ago".to_string(),
        size: "142MB".to_string(),
    };
    let json = serde_json::to_string(&info).unwrap();
    assert!(json.contains("nginx"));
    assert!(json.contains("latest"));
    assert!(json.contains("142MB"));
}

// =========================================================================
// Runtime-specific schema field tests
// =========================================================================

#[test]
fn test_all_schemas_have_runtime_field() {
    let tools: Vec<Box<dyn Tool>> = vec![
        Box::new(ContainerRun),
        Box::new(ContainerStop),
        Box::new(ContainerList),
        Box::new(ContainerLogs),
        Box::new(ContainerExec),
        Box::new(ContainerBuild),
        Box::new(ContainerImages),
        Box::new(ContainerPull),
        Box::new(ContainerRemove),
        Box::new(ComposeUp),
    ];
    for tool in &tools {
        let schema = tool.schema();
        assert!(
            schema["properties"].get("runtime").is_some(),
            "Tool {} is missing runtime field in schema",
            tool.name()
        );
    }
}
