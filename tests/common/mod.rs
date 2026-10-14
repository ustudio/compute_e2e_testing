use compute_e2e_testing::Requester;
use std::env;
use std::path::PathBuf;
use std::process::Command;
use std::sync::OnceLock;

const FIXTURE_TARGET: &str = "wasm32-wasip1";

static MODULE: OnceLock<PathBuf> = OnceLock::new();

// Builds the fixture Compute service in `tests/fixtures/service` (once per
// test binary) and points `CARGO_FASTLY_E2E_MODULE` at it, unless the
// variable is already set by the caller.
fn fixture_module() -> &'static PathBuf {
    MODULE.get_or_init(|| {
        if let Ok(module) = env::var("CARGO_FASTLY_E2E_MODULE") {
            return PathBuf::from(module);
        }

        let fixture_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/service");

        let status = Command::new(env!("CARGO"))
            .args(["build", "--quiet", "--target", FIXTURE_TARGET])
            .current_dir(&fixture_dir)
            .status()
            .expect("cargo should run to build the fixture service");

        assert!(status.success(), "fixture service should build");

        let module = fixture_dir
            .join("target")
            .join(FIXTURE_TARGET)
            .join("debug/e2e-fixture-service.wasm");

        env::set_var("CARGO_FASTLY_E2E_MODULE", &module);

        module
    })
}

pub fn requester() -> Requester {
    fixture_module();

    Requester::new()
}

pub fn get(path: &str) -> http::Request<hyper::Body> {
    http::Request::get(format!("http://example.com{path}"))
        .body(hyper::Body::empty())
        .expect("Request should build")
}
