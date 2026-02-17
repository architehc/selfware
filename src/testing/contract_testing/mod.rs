//! Contract & Integration Testing Tools
//!
//! Provides consumer-driven contracts, service virtualization,
//! test container orchestration, and API compatibility checking.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

static CONTRACT_COUNTER: AtomicU64 = AtomicU64::new(1);
static STUB_COUNTER: AtomicU64 = AtomicU64::new(1);
static CONTAINER_COUNTER: AtomicU64 = AtomicU64::new(1);

fn current_timestamp() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

pub mod api_compat;
pub mod containers;
pub mod contracts;
pub mod stubs;

// Re-export everything from contracts
pub use contracts::{
    Contract, ContractRequest, ContractResponse, ContractVerifier, HttpMethod, Interaction,
    InteractionResult, Matcher, VerificationResult,
};

// Re-export everything from stubs
pub use stubs::{FaultType, MockServer, RequestLogEntry, StubMapping, StubRequest, StubResponse};

// Re-export everything from containers
pub use containers::{ContainerOrchestrator, ContainerState, ContainerType, TestContainer};

// Re-export everything from api_compat
pub use api_compat::{
    ApiEndpoint, ApiParameter, ApiSchema, ApiSchemaProperty, ApiVersion, CompatibilityChange,
    CompatibilityChangeType, CompatibilityChecker,
};
