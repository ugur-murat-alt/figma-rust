use std::io::{Cursor, Read};

use serde::Serialize;
use tiny_http::{Header, Method, Request, Response, Server, StatusCode};

use crate::compiler::{CliError, CompilerResponse, compile_source, lint_source};

const MAX_BODY_BYTES: usize = 8 * 1024 * 1024;
const MAX_RESPONSE_BYTES: usize = 16 * 1024 * 1024;

#[derive(Serialize)]
struct ErrorResponse<'a> {
    error: &'a str,
    diagnostics: [(); 0],
}

pub fn serve(port: u16) -> Result<(), CliError> {
    let address = format!("127.0.0.1:{port}");
    let server = Server::http(&address).map_err(|error| CliError::Server(error.to_string()))?;
    eprintln!("figma-rust compiler listening on http://{address}");
    for mut request in server.incoming_requests() {
        let response = build_response(&mut request);
        request
            .respond(response)
            .map_err(|error| CliError::Server(error.to_string()))?;
    }
    Ok(())
}

fn build_response(request: &mut Request) -> Response<Cursor<Vec<u8>>> {
    if request.method() == &Method::Options {
        return if request.url() == "/lint" || request.url() == "/compile" {
            json_response(StatusCode(204), Vec::new())
        } else {
            error_response(StatusCode(404), "unknown compiler endpoint")
        };
    }
    if request.method() != &Method::Post {
        return error_response(StatusCode(405), "only POST is supported");
    }
    if request.url() != "/lint" && request.url() != "/compile" {
        return error_response(StatusCode(404), "unknown compiler endpoint");
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
                diagnostics: Vec::new(),
                source_map: None,
                error: Some(error),
            };
            bounded_json_response(StatusCode(400), &response)
        }
    }
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
        ("Access-Control-Allow-Headers", "Content-Type"),
    ] {
        if let Ok(header) = Header::from_bytes(name, value) {
            response.add_header(header);
        }
    }
    response
}

#[cfg(test)]
mod tests {
    use std::io::Read as _;

    use tiny_http::{Header, Method, StatusCode, TestRequest};

    use super::{MAX_BODY_BYTES, build_response};

    const BASIC: &str = include_str!("../../figma-rust-core/tests/fixtures/basic.raw.json");
    const BASIC_V1: &str = include_str!("../../figma-rust-core/tests/fixtures/basic.v1.raw.json");

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
        for path in ["/lint", "/compile"] {
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
                    && header.value.as_str() == "Content-Type"
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
