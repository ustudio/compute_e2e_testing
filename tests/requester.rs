mod common;

use common::{get, requester};
use hyper::{Body, Response};
use std::collections::HashMap;
use std::convert::Infallible;
use std::sync::{Arc, Mutex};

#[test]
fn returns_status_and_headers_and_body() {
    let result = requester().fetch(get("/hello"));

    assert_eq!(200, result.get_status());
    assert_eq!("hello", result.expect_header("X-Fixture"));
    assert_eq!(
        "hello",
        result
            .get_headers()
            .get("x-fixture")
            .unwrap()
            .to_str()
            .unwrap()
    );
    assert_eq!("Hello, world!", result.body_string());
}

#[test]
fn returns_non_success_status() {
    let result = requester().fetch(get("/status/418"));

    assert_eq!(418, result.get_status());
    assert_eq!("", result.body_string());
}

#[test]
fn passes_method_and_headers_and_body_to_module() {
    let req = http::Request::post("http://example.com/echo")
        .header("X-Echo", "echoed")
        .body(Body::from("request body"))
        .unwrap();

    let result = requester().fetch(req);

    assert_eq!(200, result.get_status());
    assert_eq!("echoed", result.expect_header("X-Echo"));
    assert_eq!("POST request body", result.body_string());
}

#[test]
#[should_panic(expected = "X-Missing header should be a string")]
fn expect_header_panics_when_header_is_missing() {
    requester().fetch(get("/hello")).expect_header("X-Missing");
}

#[test]
fn default_is_equivalent_to_new() {
    common::requester();

    let result = compute_e2e_testing::Requester::default().fetch(get("/hello"));

    assert_eq!(200, result.get_status());
}

fn origin(_req: http::Request<Body>) -> Result<Response<Body>, Infallible> {
    Ok(Response::builder()
        .status(201)
        .header("X-Origin", "plain-fn")
        .body(Body::from("from origin"))
        .unwrap())
}

#[test]
fn proxies_to_fn_backend() {
    let result = requester()
        .with_backend("origin", origin)
        .fetch(get("/backend/origin/some/path"));

    assert_eq!(201, result.get_status());
    assert_eq!("plain-fn", result.expect_header("X-Origin"));
    assert_eq!("from origin", result.body_string());
}

#[test]
fn closure_backend_observes_requests() {
    let seen = Arc::new(Mutex::new(Vec::new()));
    let recorder = seen.clone();

    let result = requester()
        .with_backend("origin", move |req: http::Request<Body>| {
            recorder.lock().unwrap().push((
                req.method().to_string(),
                req.uri().path().to_string(),
                req.headers()
                    .get("X-Forwarded-By")
                    .map(|v| v.to_str().unwrap().to_string()),
            ));

            Ok(Response::new(Body::from("recorded")))
        })
        .fetch(get("/backend/origin/a/b"));

    assert_eq!(200, result.get_status());
    assert_eq!("recorded", result.body_string());
    assert_eq!(
        vec![(
            "GET".to_string(),
            "/a/b".to_string(),
            Some("fixture".to_string())
        )],
        *seen.lock().unwrap()
    );
}

#[test]
fn closure_backend_can_mutate_captured_state() {
    let mut calls = 0;

    let result = requester()
        .with_backend("counter", move |_req| {
            calls += 1;
            Ok(Response::new(Body::from(calls.to_string())))
        })
        .fetch(get("/backend/counter"));

    assert_eq!("1", result.body_string());
}

#[test]
fn routes_to_multiple_backends_by_name() {
    let backend =
        |name: &'static str| move |_req| Ok::<_, Infallible>(Response::new(Body::from(name)));

    let first = requester()
        .with_backend("first", backend("first"))
        .with_backend("second", backend("second"))
        .fetch(get("/backend/first"));

    let second = requester()
        .with_backend("first", backend("first"))
        .with_backend("second", backend("second"))
        .fetch(get("/backend/second"));

    assert_eq!("first", first.body_string());
    assert_eq!("second", second.body_string());
}

#[test]
fn unregistered_backend_fails_to_send() {
    let result = requester().fetch(get("/backend/nowhere"));

    assert_eq!(502, result.get_status());
}

#[test]
fn reads_dictionary_values() {
    let contents = HashMap::from([("greeting", "hi there"), ("other", "value")]);

    let result = requester()
        .with_dictionary("settings", &contents)
        .fetch(get("/dictionary/settings/greeting"));

    assert_eq!(200, result.get_status());
    assert_eq!("hi there", result.body_string());
}

#[test]
fn missing_dictionary_key_is_not_found() {
    let result = requester()
        .with_dictionary("settings", &HashMap::from([("greeting", "hi")]))
        .fetch(get("/dictionary/settings/absent"));

    assert_eq!(404, result.get_status());
}

#[test]
fn supports_multiple_dictionaries() {
    let result = requester()
        .with_dictionary("one", &HashMap::from([("key", "from one")]))
        .with_dictionary("two", &HashMap::from([("key", "from two")]))
        .fetch(get("/dictionary/two/key"));

    assert_eq!("from two", result.body_string());
}

#[test]
fn unregistered_dictionary_cannot_be_opened() {
    let result = requester().fetch(get("/dictionary/missing/key"));

    assert_eq!(500, result.get_status());
}

#[test]
fn captured_logs_returns_logs_captured_during_request() {
    let result = requester().fetch(get("/log"));

    assert_eq!(204, result.get_status());

    let logs = result.captured_logs();
    let pair = |e: &str, m: &str| (e.to_string(), m.to_string());

    assert!(
        logs.contains(&pair("my_endpoint", "first message")),
        "{logs:?}"
    );
    assert!(
        logs.contains(&pair("my_endpoint", "second message")),
        "{logs:?}"
    );
    assert!(logs.contains(&pair("stdout", "to stdout")), "{logs:?}");
    assert!(logs.contains(&pair("stderr", "to stderr")), "{logs:?}");

    let endpoint_logs: Vec<_> = logs.iter().filter(|(e, _)| e == "my_endpoint").collect();
    assert_eq!(
        vec![
            &pair("my_endpoint", "first message"),
            &pair("my_endpoint", "second message")
        ],
        endpoint_logs
    );
}

#[test]
fn captured_logs_returns_empty_vector_when_nothing_is_logged() {
    let result = requester().fetch(get("/hello"));

    assert_eq!(Vec::<(String, String)>::new(), result.captured_logs());
}
