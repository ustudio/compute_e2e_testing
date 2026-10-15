use fastly::http::StatusCode;
use fastly::{ConfigStore, Error, Request, Response};
use std::io::Write;

#[fastly::main]
fn main(req: Request) -> Result<Response, Error> {
    let path = req.get_path().to_string();
    let segments: Vec<&str> = path.trim_start_matches('/').split('/').collect();

    match segments.as_slice() {
        ["hello"] => Ok(Response::from_status(StatusCode::OK)
            .with_header("X-Fixture", "hello")
            .with_body_text_plain("Hello, world!")),

        ["backend", name, rest @ ..] => {
            let mut bereq = req;
            bereq.set_path(&format!("/{}", rest.join("/")));
            bereq.set_header("X-Forwarded-By", "fixture");
            Ok(bereq
                .send(*name)
                .expect("Failed to send request to backend"))
        }

        ["dictionary", name, key] => Ok(Response::from_status(StatusCode::OK)
            .with_body_text_plain(
                &ConfigStore::open(name)
                    .get(key)
                    .expect("Failed to get value from config store"),
            )),

        ["log"] => {
            let mut endpoint = fastly::log::Endpoint::from_name("my_endpoint");
            writeln!(endpoint, "first message")?;
            writeln!(endpoint, "second message")?;
            println!("to stdout");
            eprintln!("to stderr");
            Ok(Response::from_status(StatusCode::NO_CONTENT))
        }

        _ => Ok(Response::from_status(StatusCode::NOT_FOUND)),
    }
}
