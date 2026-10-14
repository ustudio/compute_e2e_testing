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

        ["status", code] => Ok(Response::from_status(code.parse::<u16>()?)),

        ["echo"] => {
            let method = req.get_method_str().to_string();
            let header = req.get_header_str("X-Echo").unwrap_or_default().to_string();
            let body = req.into_body_str();
            Ok(Response::from_status(StatusCode::OK)
                .with_header("X-Echo", header)
                .with_body_text_plain(&format!("{method} {body}")))
        }

        ["backend", name, rest @ ..] => {
            let mut bereq = req;
            bereq.set_path(&format!("/{}", rest.join("/")));
            bereq.set_header("X-Forwarded-By", "fixture");
            match bereq.send(*name) {
                Ok(beresp) => Ok(beresp),
                Err(e) => Ok(Response::from_status(StatusCode::BAD_GATEWAY)
                    .with_body_text_plain(&e.to_string())),
            }
        }

        ["dictionary", name, key] => match ConfigStore::try_open(name) {
            Ok(store) => match store.get(key) {
                Some(value) => {
                    Ok(Response::from_status(StatusCode::OK).with_body_text_plain(&value))
                }
                None => Ok(Response::from_status(StatusCode::NOT_FOUND)),
            },
            Err(_) => Ok(Response::from_status(StatusCode::INTERNAL_SERVER_ERROR)),
        },

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
