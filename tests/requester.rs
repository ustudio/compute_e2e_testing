mod common;

use common::{get, requester};
use hyper::{Body, Response};
use std::collections::HashMap;
use std::convert::Infallible;
use std::sync::{Arc, Mutex};

#[test]
fn fetch_sends_request_and_returns_struct_with_getters_for_response_status_and_headers_and_body() {
    let result = requester().fetch(get("/hello"));

    assert_eq!(200, result.get_status());
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
fn expect_header_returns_header_value_when_header_is_present() {
    let result = requester().fetch(get("/hello"));

    assert_eq!("hello", result.expect_header("X-Fixture"));
}

#[test]
#[should_panic(expected = "X-Missing header should be a string")]
fn expect_header_panics_when_header_is_missing() {
    requester().fetch(get("/hello")).expect_header("X-Missing");
}

#[test]
fn default_is_equivalent_to_new() {
    common::requester(); // Ensures that the fixture module is available

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
fn with_backend_supports_a_fn_argument() {
    let result = requester()
        .with_backend("origin", origin)
        .fetch(get("/backend/origin/some/path"));

    assert_eq!(201, result.get_status());
    assert_eq!("plain-fn", result.expect_header("X-Origin"));
    assert_eq!("from origin", result.body_string());
}

#[test]
fn with_backend_supports_a_closure_argument() {
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
fn with_dictionary_sets_contents_of_named_config_store() {
    let contents = HashMap::from([
        ("other", "value"),
        ("greeting", "hi there"),
        ("another", "thing"),
    ]);

    let result = requester()
        .with_dictionary("settings", &contents)
        .with_dictionary(
            "other",
            &HashMap::from([("greeting", "wrong config store")]),
        )
        .fetch(get("/dictionary/settings/greeting"));

    assert_eq!(200, result.get_status());
    assert_eq!("hi there", result.body_string());
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
