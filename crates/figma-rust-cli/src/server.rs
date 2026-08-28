use std::{
    fs::{self, OpenOptions},
    io::{Cursor, Read, Write as _},
    path::Path,
    sync::atomic::{AtomicU64, Ordering},
};

use serde::Serialize;
use sha2::Digest as _;
use tiny_http::{Header, Method, Request, Response, Server, StatusCode};

use crate::compiler::{CliError, CompilerResponse, compile_source, lint_source};

const MAX_BODY_BYTES: usize = 8 * 1024 * 1024;
const MAX_RESPONSE_BYTES: usize = 16 * 1024 * 1024;
static EXPORT_TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

#[derive(Serialize)]
struct ErrorResponse<'a> {
    error: &'a str,
    diagnostics: [(); 0],
}

#[derive(Serialize)]
struct ExportResponse {
    path: String,
    byte_length: usize,
    sha256: String,
    complete: bool,
    traversal_complete: Option<bool>,
}

#[derive(Clone, Copy)]
struct ExportConfig<'a> {
    path: &'a Path,
    token: &'a str,
}

pub fn serve(
    port: u16,
    export_path: Option<&Path>,
    export_token_file: Option<&Path>,
) -> Result<(), CliError> {
    let export_token = match (export_path, export_token_file) {
        (Some(_), Some(path)) => {
            let token = fs::read_to_string(path).map_err(|error| {
                CliError::Server(format!(
                    "reading export token file {}: {error}",
                    path.display()
                ))
            })?;
            let token = token.trim().to_owned();
            if !valid_export_token(&token) {
                return Err(CliError::Server(
                    "export token must contain at least 32 ASCII letters, digits, hyphens, or underscores"
                        .to_owned(),
                ));
            }
            Some(token)
        }
        (None, None) => None,
        _ => {
            return Err(CliError::Server(
                "--export and --export-token-file must be provided together".to_owned(),
            ));
        }
    };
    let address = format!("127.0.0.1:{port}");
    let server = Server::http(&address).map_err(|error| CliError::Server(error.to_string()))?;
    eprintln!("figma-rust compiler listening on http://{address}");
    if let Some(path) = &export_path {
        eprintln!(
            "download-free extraction export enabled at {}",
            path.display()
        );
    }
    for mut request in server.incoming_requests() {
        let export = export_path
            .zip(export_token.as_deref())
            .map(|(path, token)| ExportConfig { path, token });
        let response = if export.is_some() {
            build_response_with_export(&mut request, export)
        } else {
            build_response(&mut request)
        };
        request
            .respond(response)
            .map_err(|error| CliError::Server(error.to_string()))?;
    }
    Ok(())
}

fn build_response(request: &mut Request) -> Response<Cursor<Vec<u8>>> {
    build_response_with_export(request, None)
}

fn build_response_with_export(
    request: &mut Request,
    export: Option<ExportConfig<'_>>,
) -> Response<Cursor<Vec<u8>>> {
    if request.method() == &Method::Options {
        return if matches!(request.url(), "/lint" | "/compile" | "/export") {
            json_response(StatusCode(204), Vec::new())
        } else {
            error_response(StatusCode(404), "unknown compiler endpoint")
        };
    }
    if request.method() != &Method::Post {
        return error_response(StatusCode(405), "only POST is supported");
    }
    if !matches!(request.url(), "/lint" | "/compile" | "/export") {
        return error_response(StatusCode(404), "unknown compiler endpoint");
    }
    if request.url() == "/export" && export.is_none() {
        return error_response(
            StatusCode(409),
            "export endpoint is disabled; restart serve with --export <path>",
        );
    }
    if request.url() == "/export"
        && !export.is_some_and(|config| export_token_matches(request, config.token))
    {
        return error_response(StatusCode(403), "invalid export token");
    }
    if request
        .body_length()
        .is_some_and(|length| length > MAX_BODY_BYTES)
    {
        return error_response(StatusCode(413), "request body exceeds 8 MiB");
    }

    let body = match read_bounded_body(request) {
        Ok(body) => body,
        Err(BodyError::Oversize) => {
            return error_response(StatusCode(413), "request body exceeds 8 MiB");
        }
        Err(BodyError::Read) => {
            return error_response(StatusCode(400), "failed to read request body");
        }
        Err(BodyError::Utf8) => {
            return error_response(StatusCode(400), "request body is not UTF-8 JSON");
        }
    };

    if request.url() == "/export" {
        if let Some(config) = export {
            return export_response(&body, config.path);
        }
        return error_response(StatusCode(409), "export endpoint is disabled");
    }

    let result = if request.url() == "/lint" {
        lint_source(&body)
    } else {
        compile_source(&body)
    };
    match result {
        Ok(response) => bounded_json_response(StatusCode(200), &response),
        Err(error) => {
            let response = CompilerResponse {
                code: None,
                diagnostic_groups: Vec::new(),
                diagnostics: Vec::new(),
                source_map: None,
                error: Some(error),
            };
            bounded_json_response(StatusCode(400), &response)
        }
    }
}

fn valid_export_token(token: &str) -> bool {
    token.len() >= 32
        && token
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
}

fn export_token_matches(request: &Request, expected: &str) -> bool {
    request
        .headers()
        .iter()
        .find(|header| header.field.equiv("X-Figma-Rust-Export-Token"))
        .is_some_and(|header| {
            constant_time_eq(header.value.as_str().as_bytes(), expected.as_bytes())
        })
}

fn constant_time_eq(actual: &[u8], expected: &[u8]) -> bool {
    if actual.len() != expected.len() {
        return false;
    }
    actual
        .iter()
        .zip(expected)
        .fold(0_u8, |difference, (left, right)| {
            difference | (left ^ right)
        })
        == 0
}

fn export_response(body: &str, path: &Path) -> Response<Cursor<Vec<u8>>> {
    let bundle = match figma_rust_core::parse_bundle(body) {
        Ok(bundle) => bundle,
        Err(error) => {
            return error_response(
                StatusCode(400),
                &format!("export body is not a valid extraction bundle: {error}"),
            );
        }
    };
    if bundle.schema_version != figma_rust_core::normalize::EXTRACTION_SCHEMA_VERSION {
        return error_response(
            StatusCode(400),
            "export body is not a schema-v2 extraction bundle",
        );
    }

    match persist_export(path, body.as_bytes()) {
        Ok(()) => bounded_json_response(
            StatusCode(200),
            &ExportResponse {
                path: path.display().to_string(),
                byte_length: body.len(),
                sha256: sha256_hex(body.as_bytes()),
                complete: true,
                traversal_complete: bundle
                    .extraction_manifest
                    .map(|manifest| manifest.traversal.complete),
            },
        ),
        Err(error) => error_response(StatusCode(500), &error),
    }
}

fn persist_export(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    if !parent.is_dir() {
        return Err(format!(
            "export directory does not exist: {}",
            parent.display()
        ));
    }
    if let Ok(metadata) = fs::symlink_metadata(path)
        && (!metadata.file_type().is_file() || metadata.file_type().is_symlink())
    {
        return Err(format!(
            "export target is not a regular file: {}",
            path.display()
        ));
    }

    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| format!("export target has no UTF-8 file name: {}", path.display()))?;
    let counter = EXPORT_TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
    let temporary = parent.join(format!(
        ".{file_name}.{}.{}.tmp",
        std::process::id(),
        counter
    ));
    let result = (|| {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)
            .map_err(|error| format!("creating {}: {error}", temporary.display()))?;
        file.write_all(bytes)
            .map_err(|error| format!("writing {}: {error}", temporary.display()))?;
        file.sync_all()
            .map_err(|error| format!("syncing {}: {error}", temporary.display()))?;
        drop(file);

        let staged = fs::read(&temporary)
            .map_err(|error| format!("reading back {}: {error}", temporary.display()))?;
        if staged != bytes {
            return Err("staged export did not match the received request bytes".to_owned());
        }
        fs::rename(&temporary, path).map_err(|error| {
            format!(
                "publishing {} to {}: {error}",
                temporary.display(),
                path.display()
            )
        })?;
        let published =
            fs::read(path).map_err(|error| format!("reading back {}: {error}", path.display()))?;
        if published != bytes {
            return Err("published export did not match the received request bytes".to_owned());
        }
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

fn sha256_hex(bytes: &[u8]) -> String {
    let digest = sha2::Sha256::digest(bytes);
    let mut output = String::with_capacity(digest.len() * 2);
    for byte in digest {
        use std::fmt::Write as _;
        let _ = write!(output, "{byte:02x}");
    }
    output
}

enum BodyError {
    Oversize,
    Read,
    Utf8,
}

fn read_bounded_body(request: &mut Request) -> Result<String, BodyError> {
    let capacity = request.body_length().unwrap_or(0).min(MAX_BODY_BYTES);
    let mut bytes = Vec::with_capacity(capacity);
    request
        .as_reader()
        .take((MAX_BODY_BYTES + 1) as u64)
        .read_to_end(&mut bytes)
        .map_err(|_| BodyError::Read)?;
    if bytes.len() > MAX_BODY_BYTES {
        return Err(BodyError::Oversize);
    }
    String::from_utf8(bytes).map_err(|_| BodyError::Utf8)
}

fn bounded_json_response<T: Serialize>(status: StatusCode, value: &T) -> Response<Cursor<Vec<u8>>> {
    match serde_json::to_vec_pretty(value) {
        Ok(mut body) if body.len() < MAX_RESPONSE_BYTES => {
            body.push(b'\n');
            json_response(status, body)
        }
        Ok(_) => error_response(StatusCode(500), "compiler response exceeds 16 MiB"),
        Err(_) => error_response(StatusCode(500), "failed to serialize compiler response"),
    }
}

fn error_response(status: StatusCode, message: &str) -> Response<Cursor<Vec<u8>>> {
    let value = ErrorResponse {
        error: message,
        diagnostics: [],
    };
    let body = match serde_json::to_vec_pretty(&value) {
        Ok(mut body) => {
            body.push(b'\n');
            body
        }
        Err(_) => b"{\"error\":\"response serialization failed\",\"diagnostics\":[]}\n".to_vec(),
    };
    json_response(status, body)
}

fn json_response(status: StatusCode, body: Vec<u8>) -> Response<Cursor<Vec<u8>>> {
    let mut response = Response::from_data(body).with_status_code(status);
    for (name, value) in [
        ("Content-Type", "application/json; charset=utf-8"),
        ("Access-Control-Allow-Origin", "*"),
        ("Access-Control-Allow-Methods", "POST, OPTIONS"),
        (
            "Access-Control-Allow-Headers",
            "Content-Type, X-Figma-Rust-Export-Token",
        ),
    ] {
        if let Ok(header) = Header::from_bytes(name, value) {
            response.add_header(header);
        }
    }
    response
}

#[cfg(test)]
mod tests {
    use std::{fs, io::Read as _, time::SystemTime};

    use tiny_http::{Header, Method, StatusCode, TestRequest};

    use super::{ExportConfig, MAX_BODY_BYTES, build_response, build_response_with_export};
    use crate::compiler::lint_file;

    const BASIC: &str = include_str!("../../figma-rust-core/tests/fixtures/basic.raw.json");
    const BASIC_V1: &str = include_str!("../../figma-rust-core/tests/fixtures/basic.v1.raw.json");
    const EXPORT_TOKEN: &str = "0123456789abcdef0123456789abcdef";

    #[test]
    fn export_endpoint_persists_exact_multimegabyte_bytes_and_reports_completion()
    -> Result<(), Box<dyn std::error::Error>> {
        let directory = test_directory("export-success")?;
        let target = directory.join("figma-rust-extraction.json");
        let mut value: serde_json::Value = serde_json::from_str(BASIC)?;
        value["synthetic_public_payload"] = serde_json::Value::String("x".repeat(3 * 1024 * 1024));
        let source = serde_json::to_string(&value)?;

        let first = export_request(&source, &target)?;
        let second = export_request(&source, &target)?;
        assert_eq!(first["sha256"], second["sha256"]);
        assert_eq!(first["byte_length"], source.len());
        assert_eq!(first["complete"], true);
        assert_eq!(first["traversal_complete"], serde_json::Value::Null);
        assert_eq!(fs::read(&target)?, source.as_bytes());
        assert_eq!(lint_file(&target)?.summary.errors, 0);
        fs::remove_dir_all(directory)?;
        Ok(())
    }

    #[test]
    fn interrupted_export_never_replaces_the_previous_valid_bundle()
    -> Result<(), Box<dyn std::error::Error>> {
        let directory = test_directory("export-interrupted")?;
        let target = directory.join("figma-rust-extraction.json");
        fs::write(&target, BASIC)?;
        let mut request = TestRequest::new()
            .with_method(Method::Post)
            .with_path("/export")
            .with_header(export_token_header()?)
            .with_body("{")
            .into();
        let response = build_response_with_export(
            &mut request,
            Some(ExportConfig {
                path: &target,
                token: EXPORT_TOKEN,
            }),
        );
        assert_eq!(response.status_code(), StatusCode(400));
        assert_eq!(fs::read_to_string(&target)?, BASIC);
        fs::remove_dir_all(directory)?;
        Ok(())
    }

    #[test]
    fn export_endpoint_requires_a_configured_fixed_path() {
        let mut request = TestRequest::new()
            .with_method(Method::Post)
            .with_path("/export")
            .with_body(BASIC)
            .into();
        assert_eq!(build_response(&mut request).status_code(), StatusCode(409));
    }

    #[test]
    fn export_endpoint_rejects_missing_token_without_touching_the_target()
    -> Result<(), Box<dyn std::error::Error>> {
        let directory = test_directory("export-token")?;
        let target = directory.join("figma-rust-extraction.json");
        fs::write(&target, BASIC)?;
        let mut request = TestRequest::new()
            .with_method(Method::Post)
            .with_path("/export")
            .with_body(BASIC)
            .into();
        let response = build_response_with_export(
            &mut request,
            Some(ExportConfig {
                path: &target,
                token: EXPORT_TOKEN,
            }),
        );
        assert_eq!(response.status_code(), StatusCode(403));
        assert_eq!(fs::read_to_string(&target)?, BASIC);
        fs::remove_dir_all(directory)?;
        Ok(())
    }

    fn export_request(
        source: &str,
        target: &std::path::Path,
    ) -> Result<serde_json::Value, Box<dyn std::error::Error>> {
        let body: &'static str = Box::leak(source.to_owned().into_boxed_str());
        let mut request = TestRequest::new()
            .with_method(Method::Post)
            .with_path("/export")
            .with_header(export_token_header()?)
            .with_body(body)
            .into();
        let response = build_response_with_export(
            &mut request,
            Some(ExportConfig {
                path: target,
                token: EXPORT_TOKEN,
            }),
        );
        assert_eq!(response.status_code(), StatusCode(200));
        let mut response_body = String::new();
        response.into_reader().read_to_string(&mut response_body)?;
        Ok(serde_json::from_str(&response_body)?)
    }

    fn export_token_header() -> Result<Header, Box<dyn std::error::Error>> {
        Header::from_bytes("X-Figma-Rust-Export-Token", EXPORT_TOKEN)
            .map_err(|()| "invalid export token test header".into())
    }

    fn test_directory(label: &str) -> Result<std::path::PathBuf, Box<dyn std::error::Error>> {
        let nonce = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)?
            .as_nanos();
        let directory = std::env::temp_dir().join(format!(
            "figma-rust-server-{label}-{}-{nonce}",
            std::process::id()
        ));
        fs::create_dir(&directory)?;
        Ok(directory)
    }

    #[test]
    fn compile_response_is_json_and_contains_code() -> Result<(), Box<dyn std::error::Error>> {
        let mut request = TestRequest::new()
            .with_method(Method::Post)
            .with_path("/compile")
            .with_body(BASIC)
            .into();
        let response = build_response(&mut request);
        assert_eq!(response.status_code(), StatusCode(200));
        assert!(response.headers().iter().any(|header| {
            header.field.equiv("Content-Type")
                && header.value.as_str().starts_with("application/json")
        }));
        assert!(response.headers().iter().any(|header| {
            header.field.equiv("Access-Control-Allow-Origin") && header.value.as_str() == "*"
        }));
        let mut body = String::new();
        response.into_reader().read_to_string(&mut body)?;
        let value: serde_json::Value = serde_json::from_str(&body)?;
        assert!(
            value["code"]
                .as_str()
                .is_some_and(|code| code.contains("generated_view"))
        );
        assert!(value["diagnostics"].is_array());
        Ok(())
    }

    #[test]
    fn malformed_and_oversize_requests_have_bounded_json_errors()
    -> Result<(), Box<dyn std::error::Error>> {
        let mut malformed = TestRequest::new()
            .with_method(Method::Post)
            .with_path("/lint")
            .with_body("{")
            .into();
        let malformed_response = build_response(&mut malformed);
        assert_eq!(malformed_response.status_code(), StatusCode(400));

        let content_length = Header::from_bytes(
            "Content-Length",
            (MAX_BODY_BYTES + 1).to_string().as_bytes(),
        )
        .map_err(|()| "invalid test header")?;
        let mut oversize = TestRequest::new()
            .with_method(Method::Post)
            .with_path("/compile")
            .with_header(content_length)
            .with_body("{}")
            .into();
        let response = build_response(&mut oversize);
        assert_eq!(response.status_code(), StatusCode(413));
        assert!(response.data_length().is_some_and(|length| length < 1024));
        Ok(())
    }

    #[test]
    fn endpoint_rejects_commands_and_query_paths() {
        for path in ["/compile?out=/tmp", "/run", "/compile/generated.rs"] {
            let mut request = TestRequest::new()
                .with_method(Method::Post)
                .with_path(path)
                .with_body(BASIC)
                .into();
            assert_eq!(build_response(&mut request).status_code(), StatusCode(404));
        }
    }

    #[test]
    fn cors_preflight_is_allowed_only_for_fixed_endpoints() {
        for path in ["/lint", "/compile", "/export"] {
            let mut request = TestRequest::new()
                .with_method(Method::Options)
                .with_path(path)
                .into();
            let response = build_response(&mut request);
            assert_eq!(response.status_code(), StatusCode(204));
            assert!(response.headers().iter().any(|header| {
                header.field.equiv("Access-Control-Allow-Origin") && header.value.as_str() == "*"
            }));
            assert!(response.headers().iter().any(|header| {
                header.field.equiv("Access-Control-Allow-Headers")
                    && header.value.as_str() == "Content-Type, X-Figma-Rust-Export-Token"
            }));
            assert!(response.headers().iter().any(|header| {
                header.field.equiv("Access-Control-Allow-Methods")
                    && header.value.as_str() == "POST, OPTIONS"
            }));
        }

        let mut unknown = TestRequest::new()
            .with_method(Method::Options)
            .with_path("/run")
            .into();
        assert_eq!(build_response(&mut unknown).status_code(), StatusCode(404));
    }

    #[test]
    fn normalization_domain_error_remains_a_parseable_plugin_response()
    -> Result<(), Box<dyn std::error::Error>> {
        let invalid_version = BASIC.replace("\"schema_version\": 2", "\"schema_version\": 999");
        let body: &'static str = Box::leak(invalid_version.into_boxed_str());
        let mut request = TestRequest::new()
            .with_method(Method::Post)
            .with_path("/compile")
            .with_body(body)
            .into();
        let response = build_response(&mut request);
        assert_eq!(response.status_code(), StatusCode(200));
        let mut body = String::new();
        response.into_reader().read_to_string(&mut body)?;
        let value: serde_json::Value = serde_json::from_str(&body)?;
        assert_eq!(value["error"], "normalization failed");
        assert!(
            value["diagnostics"]
                .as_array()
                .is_some_and(|items| !items.is_empty())
        );
        Ok(())
    }

    #[test]
    fn schema_one_source_returns_a_parseable_version_diagnostic()
    -> Result<(), Box<dyn std::error::Error>> {
        let mut request = TestRequest::new()
            .with_method(Method::Post)
            .with_path("/lint")
            .with_body(BASIC_V1)
            .into();
        let response = build_response(&mut request);
        assert_eq!(response.status_code(), StatusCode(200));
        let mut body = String::new();
        response.into_reader().read_to_string(&mut body)?;
        let value: serde_json::Value = serde_json::from_str(&body)?;
        assert_eq!(value["error"], "normalization failed");
        assert!(value["diagnostics"].as_array().is_some_and(|items| {
            items
                .iter()
                .any(|diagnostic| diagnostic["code"] == "FR-SCHEMA-001")
        }));
        Ok(())
    }
}
