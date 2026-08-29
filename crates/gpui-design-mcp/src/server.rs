use std::io::{self, BufRead as _, BufReader, BufWriter, Write as _};

use gpui_design_core::{
    AUTHORING_SCHEMA_VERSION, AuthoringDocument, DESIGN_COMMAND_VERSION, DesignTransaction,
    DesignWorkspace, TransactionError, WorkspaceError, document_fingerprint, lowering_manifest,
    validate_document,
};
use serde::{Serialize, de::DeserializeOwned};
use serde_json::{Map, Value, json};

const MAX_MESSAGE_BYTES: usize = 8 * 1024 * 1024;
const MODERN_PROTOCOL_VERSION: &str = "2026-07-28";
const LEGACY_PROTOCOL_VERSION: &str = "2025-11-25";
const SUPPORTED_VERSIONS: [&str; 5] = [
    MODERN_PROTOCOL_VERSION,
    LEGACY_PROTOCOL_VERSION,
    "2025-06-18",
    "2025-03-26",
    "2024-11-05",
];
const SERVER_NAME: &str = "gpui-design-mcp";
const RESOURCE_MANIFEST_URI: &str = "gpui-design://workspace/manifest";

#[derive(Debug, Default)]
pub struct Server {
    workspace: DesignWorkspace,
}

impl Server {
    pub fn run(&mut self) -> io::Result<()> {
        let stdin = io::stdin();
        let stdout = io::stdout();
        let mut reader = BufReader::new(stdin.lock());
        let mut writer = BufWriter::new(stdout.lock());
        let mut message = Vec::new();

        loop {
            message.clear();
            let read = reader.read_until(b'\n', &mut message)?;
            if read == 0 {
                writer.flush()?;
                return Ok(());
            }
            if message.len() > MAX_MESSAGE_BYTES {
                write_message(
                    &mut writer,
                    &error_response(
                        Value::Null,
                        RpcError::invalid_request("MCP message exceeds 8 MiB"),
                    ),
                )?;
                continue;
            }
            while matches!(message.last(), Some(b'\n' | b'\r')) {
                message.pop();
            }
            if message.is_empty() {
                continue;
            }

            let response = match serde_json::from_slice::<Value>(&message) {
                Ok(value) => self.handle_message(value),
                Err(error) => Some(error_response(
                    Value::Null,
                    RpcError::parse_error(error.to_string()),
                )),
            };
            if let Some(response) = response {
                write_message(&mut writer, &response)?;
            }
        }
    }

    fn handle_message(&mut self, message: Value) -> Option<Value> {
        let Some(object) = message.as_object() else {
            return Some(error_response(
                Value::Null,
                RpcError::invalid_request("JSON-RPC message must be an object"),
            ));
        };
        let has_id = object.contains_key("id");
        let id = object.get("id").cloned().unwrap_or(Value::Null);
        if object.get("jsonrpc").and_then(Value::as_str) != Some("2.0") {
            return has_id.then(|| {
                error_response(
                    id,
                    RpcError::invalid_request("jsonrpc must be exactly \"2.0\""),
                )
            });
        }
        let Some(method) = object.get("method").and_then(Value::as_str) else {
            return has_id
                .then(|| error_response(id, RpcError::invalid_request("method must be a string")));
        };
        let params = object.get("params").cloned().unwrap_or_else(|| json!({}));

        if !has_id {
            self.handle_notification(method, &params);
            return None;
        }
        if !matches!(method, "initialize" | "server/discover") {
            if let Some(protocol) = requested_protocol(&params)
                && !SUPPORTED_VERSIONS.contains(&protocol)
            {
                return Some(error_response(id, RpcError::unsupported_protocol(protocol)));
            }
        }

        let result = self.dispatch(method, &params);
        Some(match result {
            Ok(result) => success_response(id, result),
            Err(error) => error_response(id, error),
        })
    }

    fn handle_notification(&mut self, method: &str, _params: &Value) {
        match method {
            "notifications/initialized" | "notifications/cancelled" => {}
            _ => {}
        }
    }

    fn dispatch(&mut self, method: &str, params: &Value) -> Result<Value, RpcError> {
        match method {
            "initialize" => Ok(initialize_result(params)),
            "server/discover" => Ok(discover_result()),
            "ping" => Ok(json!({ "resultType": "complete" })),
            "tools/list" => Ok(list_tools_result()),
            "tools/call" => self.call_tool(params),
            "resources/list" => self.list_resources(),
            "resources/templates/list" => Ok(list_resource_templates_result()),
            "resources/read" => self.read_resource(params),
            "prompts/list" => Ok(list_prompts_result()),
            "prompts/get" => self.get_prompt(params),
            _ => Err(RpcError::method_not_found(method)),
        }
    }

    fn call_tool(&mut self, params: &Value) -> Result<Value, RpcError> {
        let name = required_string(params, "name")?;
        let arguments = params
            .get("arguments")
            .cloned()
            .unwrap_or_else(|| json!({}));
        match name {
            "gpui_design_capabilities" => Ok(tool_success(capability_document())),
            "gpui_design_create_document" => {
                let args: CreateDocumentArgs = parse_arguments(arguments)?;
                let document = AuthoringDocument::new(args.document_id, args.name);
                Ok(match self.workspace.open_document(document, false) {
                    Ok(receipt) => tool_success(to_value(receipt)?),
                    Err(error) => workspace_error_result(error),
                })
            }
            "gpui_design_validate_document" => {
                let args: DocumentArgs = parse_arguments(arguments)?;
                let report = validate_document(&args.document);
                let fingerprint = document_fingerprint(&args.document)
                    .map_err(|error| RpcError::internal(error.to_string()))?;
                Ok(tool_success(json!({
                    "report": report,
                    "fingerprint": fingerprint
                })))
            }
            "gpui_design_open_document" => {
                let args: OpenDocumentArgs = parse_arguments(arguments)?;
                Ok(
                    match self.workspace.open_document(args.document, args.replace) {
                        Ok(receipt) => tool_success(to_value(receipt)?),
                        Err(error) => workspace_error_result(error),
                    },
                )
            }
            "gpui_design_list_documents" => Ok(tool_success(to_value(self.workspace.summaries())?)),
            "gpui_design_get_document" => {
                let args: DocumentIdArgs = parse_arguments(arguments)?;
                match self.workspace.document(&args.document_id) {
                    Some(document) => Ok(tool_success(to_value(document)?)),
                    None => Ok(tool_error(
                        "DOCUMENT_NOT_FOUND",
                        format!("document {:?} is not open", args.document_id),
                        None,
                    )),
                }
            }
            "gpui_design_close_document" => {
                let args: DocumentIdArgs = parse_arguments(arguments)?;
                Ok(match self.workspace.remove_document(&args.document_id) {
                    Ok(()) => tool_success(json!({ "document_id": args.document_id })),
                    Err(error) => workspace_error_result(error),
                })
            }
            "gpui_design_apply_transaction" => {
                let args: TransactionArgs = parse_arguments(arguments)?;
                Ok(match self.workspace.apply(&args.transaction) {
                    Ok(receipt) => tool_success(to_value(receipt)?),
                    Err(error) => workspace_error_result(error),
                })
            }
            "gpui_design_lowering_manifest" => {
                let args: DocumentIdArgs = parse_arguments(arguments)?;
                let Some(document) = self.workspace.document(&args.document_id) else {
                    return Ok(tool_error(
                        "DOCUMENT_NOT_FOUND",
                        format!("document {:?} is not open", args.document_id),
                        None,
                    ));
                };
                let manifest = lowering_manifest(document)
                    .map_err(|error| RpcError::internal(error.to_string()))?;
                Ok(tool_success(to_value(manifest)?))
            }
            _ => Err(RpcError::invalid_params(
                "name",
                format!("unknown GPUI design tool {name:?}"),
            )),
        }
    }

    fn list_resources(&self) -> Result<Value, RpcError> {
        let mut resources = vec![json!({
            "uri": RESOURCE_MANIFEST_URI,
            "name": "GPUI Design workspace manifest",
            "description": "Capabilities and currently open authoring documents.",
            "mimeType": "application/json"
        })];
        for summary in self.workspace.summaries() {
            let encoded = encode_uri_component(&summary.document_id);
            resources.push(json!({
                "uri": format!("gpui-design://document/{encoded}"),
                "name": format!("{} authoring document", summary.document_id),
                "description": "Canonical editable GPUI design document.",
                "mimeType": "application/json"
            }));
            resources.push(json!({
                "uri": format!("gpui-design://document/{encoded}/validation"),
                "name": format!("{} validation", summary.document_id),
                "description": "Deterministic graph, token, component, and code-ownership diagnostics.",
                "mimeType": "application/json"
            }));
            resources.push(json!({
                "uri": format!("gpui-design://document/{encoded}/lowering"),
                "name": format!("{} lowering manifest", summary.document_id),
                "description": "Readiness manifest for GPUI lowering and code binding.",
                "mimeType": "application/json"
            }));
        }
        Ok(json!({
            "resultType": "complete",
            "resources": resources,
            "nextCursor": null,
            "ttlMs": 0,
            "cacheScope": "private"
        }))
    }

    fn read_resource(&self, params: &Value) -> Result<Value, RpcError> {
        let uri = required_string(params, "uri")?;
        if uri == RESOURCE_MANIFEST_URI {
            return resource_result(
                uri,
                json!({
                    "server": server_info(),
                    "capabilities": capability_document(),
                    "documents": self.workspace.summaries()
                }),
            );
        }
        let Some(path) = uri.strip_prefix("gpui-design://document/") else {
            return Err(RpcError::resource_not_found(uri));
        };
        let (encoded_id, view) = if let Some(id) = path.strip_suffix("/validation") {
            (id, ResourceView::Validation)
        } else if let Some(id) = path.strip_suffix("/lowering") {
            (id, ResourceView::Lowering)
        } else {
            (path, ResourceView::Document)
        };
        let document_id = decode_uri_component(encoded_id)?;
        let Some(document) = self.workspace.document(&document_id) else {
            return Err(RpcError::resource_not_found(uri));
        };
        match view {
            ResourceView::Document => resource_result(uri, to_value(document)?),
            ResourceView::Validation => {
                resource_result(uri, to_value(validate_document(document))?)
            }
            ResourceView::Lowering => resource_result(
                uri,
                to_value(
                    lowering_manifest(document)
                        .map_err(|error| RpcError::internal(error.to_string()))?,
                )?,
            ),
        }
    }

    fn get_prompt(&self, params: &Value) -> Result<Value, RpcError> {
        let name = required_string(params, "name")?;
        let arguments = params.get("arguments").and_then(Value::as_object);
        let document_id = arguments
            .and_then(|items| items.get("document_id"))
            .and_then(Value::as_str)
            .unwrap_or("<document_id>");
        let subject = arguments
            .and_then(|items| items.get("subject"))
            .and_then(Value::as_str)
            .unwrap_or("requested slice");
        let text = match name {
            "gpui_design_component" => format!(
                "Design or revise {subject} in GPUI Design document {document_id}. Read the workspace manifest and document resources first. Reuse primitive and semantic tokens, define a typed component contract with variants, slots, states, and events, and bind only presentation symbols. Keep networking, validation, navigation, trading logic, and other domain behavior handwritten and reference-only. Validate before applying one revision-checked transaction, then read the lowering manifest."
            ),
            "gpui_design_shell" => format!(
                "Design or revise the {subject} shell in GPUI Design document {document_id}. Treat shell, module chrome, workspace composition, dialog, overlay, docking, density, and platform/window concerns as explicit contracts rather than one-off frames. Reuse existing tokens and components, preserve code ownership, apply changes transactionally, and use actual GPUI render evidence as the visual authority."
            ),
            "gpui_design_tokenize" => format!(
                "Tokenize {subject} in GPUI Design document {document_id}. Separate primitive, semantic, component, module, shell, and platform token scopes. Preserve literal fallbacks, prevent alias cycles, avoid duplicate one-off values, validate the document, and apply the smallest atomic transaction."
            ),
            _ => return Err(RpcError::invalid_params("name", "unknown prompt name")),
        };
        Ok(json!({
            "resultType": "complete",
            "description": "GPUI-native design workflow prompt",
            "messages": [{
                "role": "user",
                "content": { "type": "text", "text": text }
            }]
        }))
    }
}

#[derive(Debug, serde::Deserialize)]
struct CreateDocumentArgs {
    document_id: String,
    name: String,
}

#[derive(Debug, serde::Deserialize)]
struct DocumentArgs {
    document: AuthoringDocument,
}

#[derive(Debug, serde::Deserialize)]
struct OpenDocumentArgs {
    document: AuthoringDocument,
    #[serde(default)]
    replace: bool,
}

#[derive(Debug, serde::Deserialize)]
struct DocumentIdArgs {
    document_id: String,
}

#[derive(Debug, serde::Deserialize)]
struct TransactionArgs {
    transaction: DesignTransaction,
}

#[derive(Debug, Clone, Copy)]
enum ResourceView {
    Document,
    Validation,
    Lowering,
}

#[derive(Debug)]
struct RpcError {
    code: i64,
    message: String,
    data: Option<Value>,
}

impl RpcError {
    fn parse_error(message: impl Into<String>) -> Self {
        Self {
            code: -32_700,
            message: format!("parse error: {}", message.into()),
            data: None,
        }
    }

    fn invalid_request(message: impl Into<String>) -> Self {
        Self {
            code: -32_600,
            message: message.into(),
            data: None,
        }
    }

    fn method_not_found(method: &str) -> Self {
        Self {
            code: -32_601,
            message: format!("method {method:?} is not supported"),
            data: None,
        }
    }

    fn invalid_params(property: &str, message: impl Into<String>) -> Self {
        Self {
            code: -32_602,
            message: format!("invalid params: {}", message.into()),
            data: Some(json!({ "property": property })),
        }
    }

    fn internal(message: impl Into<String>) -> Self {
        Self {
            code: -32_603,
            message: format!("internal MCP error: {}", message.into()),
            data: None,
        }
    }

    fn resource_not_found(uri: &str) -> Self {
        Self {
            code: -32_002,
            message: format!("resource {uri:?} was not found"),
            data: Some(json!({ "uri": uri })),
        }
    }

    fn unsupported_protocol(protocol: &str) -> Self {
        Self {
            code: -32_001,
            message: format!("unsupported MCP protocol version {protocol:?}"),
            data: Some(json!({ "supported": SUPPORTED_VERSIONS })),
        }
    }
}

fn initialize_result(params: &Value) -> Value {
    let requested = params
        .get("protocolVersion")
        .and_then(Value::as_str)
        .unwrap_or(LEGACY_PROTOCOL_VERSION);
    let selected = SUPPORTED_VERSIONS
        .iter()
        .copied()
        .find(|version| *version == requested && *version != MODERN_PROTOCOL_VERSION)
        .unwrap_or(LEGACY_PROTOCOL_VERSION);
    json!({
        "protocolVersion": selected,
        "capabilities": server_capabilities(),
        "serverInfo": server_info(),
        "instructions": server_instructions()
    })
}

fn discover_result() -> Value {
    json!({
        "resultType": "complete",
        "supportedVersions": SUPPORTED_VERSIONS,
        "capabilities": server_capabilities(),
        "instructions": server_instructions(),
        "ttlMs": 3_600_000,
        "cacheScope": "public"
    })
}

fn server_capabilities() -> Value {
    json!({
        "tools": { "listChanged": false },
        "resources": { "subscribe": false, "listChanged": false },
        "prompts": { "listChanged": false }
    })
}

fn server_info() -> Value {
    json!({
        "name": SERVER_NAME,
        "version": env!("CARGO_PKG_VERSION")
    })
}

fn server_instructions() -> &'static str {
    "Use the authoring document as the editable source of truth. Read before writing, validate first, apply one expected-revision transaction, and keep handwritten application/domain behavior reference-only. Figma is an optional importer, not the design authority."
}

fn capability_document() -> Value {
    json!({
        "authoring_schema_version": AUTHORING_SCHEMA_VERSION,
        "command_protocol_version": DESIGN_COMMAND_VERSION,
        "mcp_protocol_versions": SUPPORTED_VERSIONS,
        "transport": "stdio",
        "workspace_persistence": "PROCESS_LOCAL",
        "figma_role": "OPTIONAL_IMPORT_ADAPTER",
        "visual_authority": "ACTUAL_GPUI_RENDER",
        "code_ownership": {
            "generated_presentation": "WRITABLE_BY_LOWERING",
            "handwritten_presentation": "PATCH_PLAN_REQUIRED",
            "handwritten_behavior": "REFERENCE_ONLY"
        },
        "implemented": [
            "AUTHORING_DOCUMENT",
            "DETERMINISTIC_VALIDATION",
            "CONTENT_FINGERPRINTS",
            "REVISIONED_TRANSACTIONS",
            "IDEMPOTENT_TRANSACTION_REPLAY",
            "MCP_TOOLS",
            "MCP_RESOURCES",
            "MCP_PROMPTS"
        ],
        "planned": [
            "AUTHORING_TO_COMPILER_IR_LOWERING",
            "GPUI_PREVIEW_PROCESS",
            "GEOMETRY_HIT_TEST",
            "AST_SAFE_CODE_PATCHING",
            "DURABLE_PROJECT_STORE",
            "WEB_STUDIO"
        ]
    })
}

fn list_tools_result() -> Value {
    json!({
        "resultType": "complete",
        "tools": tool_catalog(),
        "nextCursor": null,
        "ttlMs": 3_600_000,
        "cacheScope": "public"
    })
}

fn tool_catalog() -> Vec<Value> {
    vec![
        tool_definition(
            "gpui_design_capabilities",
            "Report the implemented GPUI-native authoring, transaction, MCP, code-ownership, and planned lowering capabilities.",
            empty_schema(),
            true,
            false,
            true,
        ),
        tool_definition(
            "gpui_design_create_document",
            "Create and open an empty Figma-independent GPUI authoring document at revision zero.",
            json!({
                "type": "object",
                "additionalProperties": false,
                "properties": {
                    "document_id": { "type": "string", "minLength": 1 },
                    "name": { "type": "string", "minLength": 1 }
                },
                "required": ["document_id", "name"]
            }),
            false,
            false,
            true,
        ),
        tool_definition(
            "gpui_design_validate_document",
            "Validate a complete authoring document without loading or mutating the MCP workspace.",
            document_schema(false),
            true,
            false,
            true,
        ),
        tool_definition(
            "gpui_design_open_document",
            "Validate and open a complete authoring document in the process-local workspace. Replacement is explicit.",
            document_schema(true),
            false,
            false,
            false,
        ),
        tool_definition(
            "gpui_design_list_documents",
            "List deterministic summaries of all authoring documents open in this MCP process.",
            empty_schema(),
            true,
            false,
            true,
        ),
        tool_definition(
            "gpui_design_get_document",
            "Read one complete canonical authoring document.",
            document_id_schema(),
            true,
            false,
            true,
        ),
        tool_definition(
            "gpui_design_close_document",
            "Remove one document and its transaction replay history from the process-local workspace.",
            document_id_schema(),
            false,
            true,
            true,
        ),
        tool_definition(
            "gpui_design_apply_transaction",
            "Atomically apply revision-checked design commands. Invalid graphs leave the loaded document unchanged; repeated identical transaction IDs are idempotent.",
            json!({
                "type": "object",
                "additionalProperties": false,
                "properties": {
                    "transaction": {
                        "type": "object",
                        "description": "DesignTransaction with protocol_version, transaction_id, document_id, expected_revision, and commands."
                    }
                },
                "required": ["transaction"]
            }),
            false,
            false,
            true,
        ),
        tool_definition(
            "gpui_design_lowering_manifest",
            "Report validation, token/component counts, content fingerprint, code bindings, and unresolved component bindings before GPUI lowering.",
            document_id_schema(),
            true,
            false,
            true,
        ),
    ]
}

fn tool_definition(
    name: &str,
    description: &str,
    input_schema: Value,
    read_only: bool,
    destructive: bool,
    idempotent: bool,
) -> Value {
    json!({
        "name": name,
        "description": description,
        "inputSchema": input_schema,
        "annotations": {
            "readOnlyHint": read_only,
            "destructiveHint": destructive,
            "idempotentHint": idempotent,
            "openWorldHint": false
        }
    })
}

fn empty_schema() -> Value {
    json!({
        "type": "object",
        "additionalProperties": false,
        "properties": {}
    })
}

fn document_schema(include_replace: bool) -> Value {
    let mut properties = Map::from_iter([(
        "document".to_owned(),
        json!({
            "type": "object",
            "description": "Complete gpui-design authoring document."
        }),
    )]);
    if include_replace {
        properties.insert(
            "replace".to_owned(),
            json!({ "type": "boolean", "default": false }),
        );
    }
    json!({
        "type": "object",
        "additionalProperties": false,
        "properties": properties,
        "required": ["document"]
    })
}

fn document_id_schema() -> Value {
    json!({
        "type": "object",
        "additionalProperties": false,
        "properties": {
            "document_id": { "type": "string", "minLength": 1 }
        },
        "required": ["document_id"]
    })
}

fn list_resource_templates_result() -> Value {
    json!({
        "resultType": "complete",
        "resourceTemplates": [
            {
                "uriTemplate": "gpui-design://document/{document_id}",
                "name": "GPUI Design authoring document",
                "description": "Canonical document; URI-encode document_id when constructing a concrete URI.",
                "mimeType": "application/json"
            },
            {
                "uriTemplate": "gpui-design://document/{document_id}/validation",
                "name": "GPUI Design validation report",
                "mimeType": "application/json"
            },
            {
                "uriTemplate": "gpui-design://document/{document_id}/lowering",
                "name": "GPUI Design lowering manifest",
                "mimeType": "application/json"
            }
        ],
        "nextCursor": null,
        "ttlMs": 3_600_000,
        "cacheScope": "public"
    })
}

fn list_prompts_result() -> Value {
    json!({
        "resultType": "complete",
        "prompts": [
            {
                "name": "gpui_design_component",
                "description": "Create or revise a tokenized semantic GPUI component contract.",
                "arguments": [
                    { "name": "document_id", "description": "Open authoring document ID", "required": true },
                    { "name": "subject", "description": "Component or control to design", "required": true }
                ]
            },
            {
                "name": "gpui_design_shell",
                "description": "Create or revise a shell/workspace/module composition while preserving GPUI ownership boundaries.",
                "arguments": [
                    { "name": "document_id", "required": true },
                    { "name": "subject", "required": true }
                ]
            },
            {
                "name": "gpui_design_tokenize",
                "description": "Normalize literals into primitive through platform token scopes.",
                "arguments": [
                    { "name": "document_id", "required": true },
                    { "name": "subject", "required": true }
                ]
            }
        ],
        "nextCursor": null,
        "ttlMs": 3_600_000,
        "cacheScope": "public"
    })
}

fn tool_success(value: Value) -> Value {
    let text = pretty_text(&value);
    json!({
        "resultType": "complete",
        "content": [{ "type": "text", "text": text }],
        "structuredContent": value,
        "isError": false
    })
}

fn tool_error(code: &str, message: String, details: Option<Value>) -> Value {
    let payload = json!({
        "ok": false,
        "error": {
            "code": code,
            "message": message,
            "details": details
        }
    });
    json!({
        "resultType": "complete",
        "content": [{ "type": "text", "text": pretty_text(&payload) }],
        "structuredContent": payload,
        "isError": true
    })
}

fn workspace_error_result(error: WorkspaceError) -> Value {
    match error {
        WorkspaceError::ValidationFailed { report } => tool_error(
            "VALIDATION_FAILED",
            "document failed validation".to_owned(),
            serde_json::to_value(*report).ok(),
        ),
        WorkspaceError::Transaction(TransactionError::ValidationFailed { report }) => tool_error(
            "VALIDATION_FAILED",
            "transaction would leave the document invalid".to_owned(),
            serde_json::to_value(*report).ok(),
        ),
        WorkspaceError::Transaction(TransactionError::RevisionConflict { expected, actual }) => {
            tool_error(
                "REVISION_CONFLICT",
                format!("expected revision {expected}, current revision is {actual}"),
                Some(json!({ "expected": expected, "actual": actual })),
            )
        }
        WorkspaceError::TransactionIdConflict { transaction_id } => tool_error(
            "TRANSACTION_ID_CONFLICT",
            format!("transaction id {transaction_id:?} was reused with different content"),
            None,
        ),
        WorkspaceError::DocumentAlreadyExists(document_id) => tool_error(
            "DOCUMENT_ALREADY_EXISTS",
            format!("document {document_id:?} is already open"),
            None,
        ),
        WorkspaceError::DocumentNotFound(document_id) => tool_error(
            "DOCUMENT_NOT_FOUND",
            format!("document {document_id:?} is not open"),
            None,
        ),
        other => tool_error("DESIGN_OPERATION_FAILED", other.to_string(), None),
    }
}

fn resource_result(uri: &str, value: Value) -> Result<Value, RpcError> {
    let text = serde_json::to_string_pretty(&value)
        .map_err(|error| RpcError::internal(error.to_string()))?;
    Ok(json!({
        "resultType": "complete",
        "contents": [{
            "uri": uri,
            "mimeType": "application/json",
            "text": text
        }],
        "ttlMs": 0,
        "cacheScope": "private"
    }))
}

fn success_response(id: Value, mut result: Value) -> Value {
    attach_server_metadata(&mut result);
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "result": result
    })
}

fn error_response(id: Value, error: RpcError) -> Value {
    let mut body = json!({
        "code": error.code,
        "message": error.message
    });
    if let Some(data) = error.data
        && let Some(object) = body.as_object_mut()
    {
        object.insert("data".to_owned(), data);
    }
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "error": body
    })
}

fn attach_server_metadata(result: &mut Value) {
    let Some(object) = result.as_object_mut() else {
        return;
    };
    let metadata = object
        .entry("_meta".to_owned())
        .or_insert_with(|| Value::Object(Map::new()));
    if !metadata.is_object() {
        *metadata = Value::Object(Map::new());
    }
    if let Some(metadata) = metadata.as_object_mut() {
        metadata
            .entry("io.modelcontextprotocol/serverInfo".to_owned())
            .or_insert_with(server_info);
    }
}

fn write_message(writer: &mut BufWriter<io::StdoutLock<'_>>, message: &Value) -> io::Result<()> {
    serde_json::to_writer(&mut *writer, message).map_err(io::Error::other)?;
    writer.write_all(b"\n")?;
    writer.flush()
}

fn requested_protocol(params: &Value) -> Option<&str> {
    params
        .get("_meta")
        .and_then(|metadata| metadata.get("io.modelcontextprotocol/protocolVersion"))
        .and_then(Value::as_str)
}

fn required_string<'a>(params: &'a Value, property: &str) -> Result<&'a str, RpcError> {
    params
        .get(property)
        .and_then(Value::as_str)
        .ok_or_else(|| RpcError::invalid_params(property, "required string is missing"))
}

fn parse_arguments<T: DeserializeOwned>(arguments: Value) -> Result<T, RpcError> {
    serde_json::from_value(arguments)
        .map_err(|error| RpcError::invalid_params("arguments", error.to_string()))
}

fn to_value<T: Serialize>(value: T) -> Result<Value, RpcError> {
    serde_json::to_value(value).map_err(|error| RpcError::internal(error.to_string()))
}

fn pretty_text(value: &Value) -> String {
    serde_json::to_string_pretty(value).unwrap_or_else(|_| value.to_string())
}

fn encode_uri_component(value: &str) -> String {
    let mut output = String::with_capacity(value.len());
    for byte in value.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'~') {
            output.push(char::from(byte));
        } else {
            use std::fmt::Write as _;
            let _ = write!(output, "%{byte:02X}");
        }
    }
    output
}

fn decode_uri_component(value: &str) -> Result<String, RpcError> {
    let bytes = value.as_bytes();
    let mut output = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' {
            if index + 2 >= bytes.len() {
                return Err(RpcError::invalid_params("uri", "truncated percent escape"));
            }
            let high = decode_hex(bytes[index + 1])?;
            let low = decode_hex(bytes[index + 2])?;
            output.push((high << 4) | low);
            index += 3;
        } else {
            output.push(bytes[index]);
            index += 1;
        }
    }
    String::from_utf8(output).map_err(|error| RpcError::invalid_params("uri", error.to_string()))
}

fn decode_hex(byte: u8) -> Result<u8, RpcError> {
    match byte {
        b'0'..=b'9' => Ok(byte - b'0'),
        b'a'..=b'f' => Ok(byte - b'a' + 10),
        b'A'..=b'F' => Ok(byte - b'A' + 10),
        _ => Err(RpcError::invalid_params("uri", "invalid percent escape")),
    }
}
