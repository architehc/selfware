//! Test Container Orchestration

use super::*;

/// Container type
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ContainerType {
    Database,
    MessageQueue,
    Cache,
    MockService,
    Custom,
}

/// Container state
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ContainerState {
    Created,
    Starting,
    Running,
    Stopping,
    Stopped,
    Failed,
}

/// Test container configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestContainer {
    /// Container ID
    pub container_id: String,
    /// Name
    pub name: String,
    /// Image
    pub image: String,
    /// Container type
    pub container_type: ContainerType,
    /// Environment variables
    pub env: HashMap<String, String>,
    /// Port mappings (host:container)
    pub ports: HashMap<u16, u16>,
    /// Volume mounts
    pub volumes: Vec<(String, String)>,
    /// State
    pub state: ContainerState,
    /// Health check command
    pub health_check: Option<String>,
    /// Startup timeout
    pub startup_timeout: Duration,
}

impl TestContainer {
    pub fn new(name: impl Into<String>, image: impl Into<String>) -> Self {
        let container_id = format!("tc_{}", CONTAINER_COUNTER.fetch_add(1, Ordering::SeqCst));
        Self {
            container_id,
            name: name.into(),
            image: image.into(),
            container_type: ContainerType::Custom,
            env: HashMap::new(),
            ports: HashMap::new(),
            volumes: Vec::new(),
            state: ContainerState::Created,
            health_check: None,
            startup_timeout: Duration::from_secs(60),
        }
    }

    pub fn postgres() -> Self {
        Self::new("postgres", "postgres:15")
            .with_type(ContainerType::Database)
            .with_env("POSTGRES_PASSWORD", "test")
            .with_port(5432, 5432)
            .with_health_check("pg_isready -U postgres")
    }

    pub fn redis() -> Self {
        Self::new("redis", "redis:7")
            .with_type(ContainerType::Cache)
            .with_port(6379, 6379)
            .with_health_check("redis-cli ping")
    }

    pub fn rabbitmq() -> Self {
        Self::new("rabbitmq", "rabbitmq:3-management")
            .with_type(ContainerType::MessageQueue)
            .with_port(5672, 5672)
            .with_port(15672, 15672)
    }

    pub fn with_type(mut self, container_type: ContainerType) -> Self {
        self.container_type = container_type;
        self
    }

    pub fn with_env(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.env.insert(key.into(), value.into());
        self
    }

    pub fn with_port(mut self, host_port: u16, container_port: u16) -> Self {
        self.ports.insert(host_port, container_port);
        self
    }

    pub fn with_volume(
        mut self,
        host_path: impl Into<String>,
        container_path: impl Into<String>,
    ) -> Self {
        self.volumes.push((host_path.into(), container_path.into()));
        self
    }

    pub fn with_health_check(mut self, command: impl Into<String>) -> Self {
        self.health_check = Some(command.into());
        self
    }

    pub fn with_startup_timeout(mut self, timeout: Duration) -> Self {
        self.startup_timeout = timeout;
        self
    }

    pub fn to_docker_run(&self) -> String {
        let mut cmd = format!("docker run -d --name {}", self.name);

        for (key, value) in &self.env {
            cmd.push_str(&format!(" -e {}={}", key, value));
        }

        for (host, container) in &self.ports {
            cmd.push_str(&format!(" -p {}:{}", host, container));
        }

        for (host, container) in &self.volumes {
            cmd.push_str(&format!(" -v {}:{}", host, container));
        }

        cmd.push_str(&format!(" {}", self.image));
        cmd
    }
}

/// Container orchestrator
#[derive(Debug, Clone)]
pub struct ContainerOrchestrator {
    /// Project name (prefix for container names)
    pub project: String,
    /// Containers
    pub containers: HashMap<String, TestContainer>,
    /// Dependency order
    dependencies: HashMap<String, Vec<String>>,
}

impl ContainerOrchestrator {
    pub fn new(project: impl Into<String>) -> Self {
        Self {
            project: project.into(),
            containers: HashMap::new(),
            dependencies: HashMap::new(),
        }
    }

    pub fn add_container(&mut self, container: TestContainer) {
        self.containers.insert(container.name.clone(), container);
    }

    pub fn set_dependency(&mut self, container: impl Into<String>, depends_on: impl Into<String>) {
        self.dependencies
            .entry(container.into())
            .or_default()
            .push(depends_on.into());
    }

    pub fn start_order(&self) -> Vec<&str> {
        // Simple topological sort for startup order
        let mut order = Vec::new();
        let mut started: std::collections::HashSet<&str> = std::collections::HashSet::new();

        while started.len() < self.containers.len() {
            for name in self.containers.keys() {
                if started.contains(name.as_str()) {
                    continue;
                }

                let deps = self.dependencies.get(name).cloned().unwrap_or_default();
                if deps.iter().all(|d| started.contains(d.as_str())) {
                    order.push(name.as_str());
                    started.insert(name);
                }
            }

            // Break if no progress (cycle detected)
            if order.len() == started.len() && started.len() < self.containers.len() {
                break;
            }
        }

        order
    }

    pub fn generate_compose(&self) -> String {
        let mut yaml = String::new();
        yaml.push_str("version: '3.8'\n\n");
        yaml.push_str("services:\n");

        for (name, container) in &self.containers {
            yaml.push_str(&format!("  {}:\n", name));
            yaml.push_str(&format!("    image: {}\n", container.image));

            if !container.env.is_empty() {
                yaml.push_str("    environment:\n");
                for (key, value) in &container.env {
                    yaml.push_str(&format!("      - {}={}\n", key, value));
                }
            }

            if !container.ports.is_empty() {
                yaml.push_str("    ports:\n");
                for (host, container_port) in &container.ports {
                    yaml.push_str(&format!("      - \"{}:{}\"\n", host, container_port));
                }
            }

            if let Some(deps) = self.dependencies.get(name) {
                if !deps.is_empty() {
                    yaml.push_str("    depends_on:\n");
                    for dep in deps {
                        yaml.push_str(&format!("      - {}\n", dep));
                    }
                }
            }

            yaml.push('\n');
        }

        yaml
    }

    pub fn start_all(&mut self) {
        let order: Vec<String> = self
            .start_order()
            .into_iter()
            .map(|s| s.to_string())
            .collect();
        for name in order {
            if let Some(container) = self.containers.get_mut(&name) {
                container.state = ContainerState::Running;
            }
        }
    }

    pub fn stop_all(&mut self) {
        for container in self.containers.values_mut() {
            container.state = ContainerState::Stopped;
        }
    }

    pub fn is_healthy(&self) -> bool {
        self.containers
            .values()
            .all(|c| c.state == ContainerState::Running)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_container_postgres() {
        let container = TestContainer::postgres();

        assert_eq!(container.name, "postgres");
        assert!(container.image.contains("postgres"));
        assert!(container.ports.contains_key(&5432));
    }

    #[test]
    fn test_container_redis() {
        let container = TestContainer::redis();

        assert_eq!(container.name, "redis");
        assert!(container.ports.contains_key(&6379));
    }

    #[test]
    fn test_container_docker_run() {
        let container = TestContainer::new("myapp", "myapp:latest")
            .with_env("DEBUG", "true")
            .with_port(8080, 80);

        let cmd = container.to_docker_run();
        assert!(cmd.contains("docker run"));
        assert!(cmd.contains("-e DEBUG=true"));
        assert!(cmd.contains("-p 8080:80"));
    }

    #[test]
    fn test_container_orchestrator() {
        let mut orchestrator = ContainerOrchestrator::new("test-project");

        orchestrator.add_container(TestContainer::postgres());
        orchestrator.add_container(TestContainer::redis());

        orchestrator.set_dependency("redis", "postgres");

        let order = orchestrator.start_order();
        assert!(!order.is_empty());
    }

    #[test]
    fn test_container_compose() {
        let mut orchestrator = ContainerOrchestrator::new("test");

        orchestrator.add_container(TestContainer::postgres());
        orchestrator.add_container(TestContainer::new("app", "app:latest").with_port(8080, 8080));
        orchestrator.set_dependency("app", "postgres");

        let yaml = orchestrator.generate_compose();
        assert!(yaml.contains("version:"));
        assert!(yaml.contains("services:"));
        assert!(yaml.contains("postgres:"));
    }

    #[test]
    fn test_container_type_all_variants() {
        let types = [
            ContainerType::Database,
            ContainerType::MessageQueue,
            ContainerType::Cache,
            ContainerType::MockService,
            ContainerType::Custom,
        ];

        for t in types {
            let _ = serde_json::to_string(&t).unwrap();
        }
    }

    #[test]
    fn test_container_state_all_variants() {
        let states = [
            ContainerState::Created,
            ContainerState::Starting,
            ContainerState::Running,
            ContainerState::Stopping,
            ContainerState::Stopped,
            ContainerState::Failed,
        ];

        for state in states {
            let _ = serde_json::to_string(&state).unwrap();
        }
    }

    #[test]
    fn test_test_container_rabbitmq() {
        let container = TestContainer::rabbitmq();
        assert_eq!(container.name, "rabbitmq");
        assert!(container.ports.contains_key(&5672));
        assert!(container.ports.contains_key(&15672));
    }

    #[test]
    fn test_test_container_with_volume() {
        let container =
            TestContainer::new("app", "app:latest").with_volume("/data", "/container/data");

        assert_eq!(container.volumes.len(), 1);
        assert_eq!(container.volumes[0].0, "/data");
    }

    #[test]
    fn test_test_container_with_startup_timeout() {
        let container = TestContainer::new("slow", "slow:latest")
            .with_startup_timeout(Duration::from_secs(120));

        assert_eq!(container.startup_timeout, Duration::from_secs(120));
    }

    #[test]
    fn test_container_orchestrator_start_stop_all() {
        let mut orchestrator = ContainerOrchestrator::new("test");
        orchestrator.add_container(TestContainer::postgres());
        orchestrator.add_container(TestContainer::redis());

        orchestrator.start_all();
        assert!(orchestrator.is_healthy());

        orchestrator.stop_all();
        assert!(!orchestrator.is_healthy());
    }

    #[test]
    fn test_container_orchestrator_empty() {
        let orchestrator = ContainerOrchestrator::new("empty");
        let order = orchestrator.start_order();
        assert!(order.is_empty());
        assert!(orchestrator.is_healthy()); // Empty is considered healthy
    }

    #[test]
    fn test_test_container_clone() {
        let container = TestContainer::postgres();
        let cloned = container.clone();

        assert_eq!(container.name, cloned.name);
        assert_eq!(container.image, cloned.image);
    }

    #[test]
    fn test_container_orchestrator_clone() {
        let mut orchestrator = ContainerOrchestrator::new("test");
        orchestrator.add_container(TestContainer::postgres());

        let cloned = orchestrator.clone();
        assert_eq!(orchestrator.project, cloned.project);
        assert_eq!(orchestrator.containers.len(), cloned.containers.len());
    }
}
