use crate::protocol::{
    AgentCapabilities, CancelRequest, CancelResult, CompressRequest, Envelope, JobStatus,
    PlanRequest, PlanResult, StatusRequest,
};
use crate::service::AgentService;
use rmcp::handler::server::tool::IntoCallToolResult;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::{CallToolResult, ContentBlock};
use rmcp::{ServiceExt, tool, tool_router, transport::stdio};
use schemars::JsonSchema;
use serde::Serialize;
use serde_json::{Map, json};
use std::sync::Arc;

struct AgentToolResult<T>(Envelope<T>);

fn envelope_output_schema<T: JsonSchema + 'static>() -> Arc<Map<String, serde_json::Value>> {
    let generated = rmcp::handler::server::tool::schema_for_output::<T>()
        .expect("image-slim result schema must be an object");
    let mut result_schema = (*generated).clone();
    strip_schema_metadata(&mut result_schema);
    let mut definitions = result_schema.remove("$defs");
    if let Some(definitions) = definitions
        .as_mut()
        .and_then(serde_json::Value::as_object_mut)
    {
        if definitions.contains_key("ErrorCode") {
            definitions.insert("ErrorCode".into(), json!({"type": "string"}));
        }
        if definitions.contains_key("AppError") {
            definitions.insert(
                "AppError".into(),
                json!({
                    "type": "object",
                    "properties": {
                        "code": {"type": "string"},
                        "params": {"type": "object"},
                        "path": {"type": ["string", "null"]},
                        "detail": {"type": ["string", "null"]},
                        "retryable": {"type": "boolean"}
                    },
                    "required": ["code", "params", "path", "detail", "retryable"]
                }),
            );
        }
    }
    let mut root = Map::from_iter([
        ("type".into(), json!("object")),
        (
            "properties".into(),
            json!({
                "ok": {"type": "boolean"},
                "result": result_schema,
                "error": {
                    "type": "object",
                    "properties": {
                        "code": {"type": "string"},
                        "params": {"type": "object"},
                        "path": {"type": ["string", "null"]},
                        "detail": {"type": ["string", "null"]},
                        "retryable": {"type": "boolean"}
                    },
                    "required": ["code", "params", "path", "detail", "retryable"]
                }
            }),
        ),
        ("required".into(), json!(["ok"])),
    ]);
    if let Some(definitions) = definitions {
        root.insert("$defs".into(), definitions);
    }
    Arc::new(root)
}

fn strip_schema_metadata(schema: &mut Map<String, serde_json::Value>) {
    schema.remove("title");
    schema.remove("description");
    schema.remove("default");
    for value in schema.values_mut() {
        strip_value_metadata(value);
    }
}

fn strip_value_metadata(value: &mut serde_json::Value) {
    match value {
        serde_json::Value::Object(object) => strip_schema_metadata(object),
        serde_json::Value::Array(items) => {
            for item in items {
                strip_value_metadata(item);
            }
        }
        _ => {}
    }
}

impl<T: JsonSchema + Serialize + 'static> IntoCallToolResult for AgentToolResult<T> {
    fn into_call_tool_result(self) -> Result<CallToolResult, rmcp::ErrorData> {
        let is_error = !self.0.ok;
        let summary = self
            .0
            .error
            .as_ref()
            .map(|error| format!("image-slim error: {:?}", error.code))
            .unwrap_or_else(|| "image-slim request completed".to_string());
        let structured_content = serde_json::to_value(self.0).map_err(|error| {
            rmcp::ErrorData::internal_error(
                format!("failed to serialize image-slim result: {error}"),
                None,
            )
        })?;
        let mut result = CallToolResult::success(vec![ContentBlock::text(summary)]);
        result.structured_content = Some(structured_content);
        result.is_error = is_error.then_some(true);
        Ok(result)
    }
}

#[derive(Clone)]
pub struct AgentMcp {
    service: AgentService,
}

impl AgentMcp {
    pub fn new(service: AgentService) -> Self {
        Self { service }
    }
}

#[tool_router(server_handler)]
impl AgentMcp {
    #[tool(
        name = "image_slim_capabilities",
        description = "Return local image-slim formats, limits, allowed roots, and overwrite policy.",
        output_schema = envelope_output_schema::<AgentCapabilities>(),
        annotations(read_only_hint = true, idempotent_hint = true, open_world_hint = false)
    )]
    fn capabilities(&self) -> AgentToolResult<AgentCapabilities> {
        AgentToolResult(self.service.capabilities())
    }

    #[tool(
        name = "image_slim_plan",
        description = "Scan allowed local paths and create an expiring compression plan without returning the file list.",
        output_schema = envelope_output_schema::<PlanResult>(),
        annotations(read_only_hint = true, idempotent_hint = true, open_world_hint = false)
    )]
    fn plan(&self, Parameters(request): Parameters<PlanRequest>) -> AgentToolResult<PlanResult> {
        AgentToolResult(self.service.plan_mcp(request))
    }

    #[tool(
        name = "image_slim_compress",
        description = "Compress an existing plan or allowed local paths and return bounded aggregate progress.",
        output_schema = envelope_output_schema::<JobStatus>(),
        annotations(read_only_hint = false, destructive_hint = true, idempotent_hint = true, open_world_hint = false)
    )]
    fn compress(
        &self,
        Parameters(request): Parameters<CompressRequest>,
    ) -> AgentToolResult<JobStatus> {
        AgentToolResult(self.service.compress(request))
    }

    #[tool(
        name = "image_slim_status",
        description = "Return bounded aggregate progress and a page of issues for an image-slim job.",
        output_schema = envelope_output_schema::<JobStatus>(),
        annotations(read_only_hint = true, idempotent_hint = true, open_world_hint = false)
    )]
    fn status(&self, Parameters(request): Parameters<StatusRequest>) -> AgentToolResult<JobStatus> {
        AgentToolResult(self.service.status(request))
    }

    #[tool(
        name = "image_slim_cancel",
        description = "Cooperatively cancel an image-slim job; repeated calls are safe.",
        output_schema = envelope_output_schema::<CancelResult>(),
        annotations(read_only_hint = false, destructive_hint = true, idempotent_hint = true, open_world_hint = false)
    )]
    fn cancel(
        &self,
        Parameters(request): Parameters<CancelRequest>,
    ) -> AgentToolResult<CancelResult> {
        AgentToolResult(self.service.cancel(request))
    }
}

pub async fn serve(service: AgentService) -> anyhow::Result<()> {
    let server = AgentMcp::new(service).serve(stdio()).await?;
    server.waiting().await?;
    Ok(())
}
