extern crate rfnet_core;
extern crate futures;
extern crate hyper;
extern crate serde;
extern crate serde_json;
#[macro_use]
extern crate serde_derive;
extern crate base64;
extern crate rust_sodium;

use futures::Future;
use hyper::server::{Request, Response, Http, Service};
use futures::future;

#[derive(Serialize, Deserialize)]
struct SignRequest {
    sequence_id: u16,
    addr: String,
    url: String,
    headers: Vec<(String,String)>,
    method: String,
    body: String,
    signature: String,
    public_keys: Vec<String>
}

struct Sign {
}

fn response_error<T>(code: hyper::StatusCode, msg: T) -> Response where T: Into<String> {
    Response::new().with_status(code)
        .with_body(format!("{{ \"error\": \"{}\" }}", msg.into()))
}

fn response_success(verified: bool) -> Response {
    Response::new().with_status(hyper::StatusCode::Ok)
        .with_body(format!("{{ \"verified\": {} }}", verified))
}

fn verify_sign(request: &SignRequest) -> Response {
    use rfnet_core::message;
    use rust_sodium::crypto::sign;

    let method = match request.method.as_str() {
        "GET" => message::RESTMethod::GET,
        "PUT" => message::RESTMethod::PUT,
        "POST" => message::RESTMethod::POST,
        "PATCH" => message::RESTMethod::PATCH,
        "DELETE" => message::RESTMethod::DELETE,
        o => return response_error(hyper::StatusCode::BadRequest, format!("Invalid method: {}", o))
    };

    //note that we need to be really careful in the construction of this so it gets parsed *exactly* into the same headers
    let headers = request.headers.iter().fold("".to_string(), |mut h, &(ref k, ref v)| {
        if h.len() > 0 {
            h += "\r\n";
        }

        h += k.as_str();
        h += ": ";
        h += v.as_str();

        h
    });

    let body = match base64::decode(&request.body) {
        Ok(b) => b,
        Err(e) => return response_error(hyper::StatusCode::BadRequest, format!("Invalid Base64 body {}", e))
    };

    let message = message::RequestMessage {
        sequence_id: request.sequence_id,
        addr: request.addr.as_str(),
        req_type: message::RequestType::REST {
            method,
            url: request.url.as_str(),
            headers: headers.as_str(),
            body: &body[..]
        }
    };

    let mut encoded = vec!();
    if let Err(e) = message::encode_request_payload(&message, &mut encoded) {
        return response_error(hyper::StatusCode::InternalServerError, format!("Unable to encode message {}", e))
    }

    let signature = match base64::decode(&request.signature) {
        Ok(s) => match sign::Signature::from_slice(&s[..]) {
            Some(s) => s,
            None => return response_error(hyper::StatusCode::BadRequest, format!("Unable to decode signature key {}, too short", &request.signature))
        },
        Err(e) => return response_error(hyper::StatusCode::BadRequest, format!("Unable to decode signature key {} {}", &request.signature, e))
    };

    //Check against each key
    for key in &request.public_keys {
        let pub_key = match base64::decode(key) {
            Ok(k) => match sign::PublicKey::from_slice(&k[..]) {
                Some(k) => k,
                None => return response_error(hyper::StatusCode::BadRequest, format!("Unable to decode public key {} too short", key))
            }
            Err(e) => return response_error(hyper::StatusCode::BadRequest, format!("Unable to decode public key {} {}", key, e))
        };

        if sign::verify_detached(&signature, &encoded[..], &pub_key) {
            return response_success(true)
        }
    }

    response_success(false)
}

impl Service for Sign {
    type Request = Request;
    type Response = Response;
    type Error = hyper::Error;
    type Future = Box<Future<Item=Self::Response, Error=Self::Error>>;

    fn call(&self, req: Request) -> Self::Future {
        use futures::Stream;

        println!("Request: {:?}", &req);

        let result = req.body().concat2().then(|v| {
            let response = match v {
                Ok(body) => {
                    if let Ok(req_body) = ::std::str::from_utf8(&body) {
                        match serde_json::from_str::<SignRequest>(&req_body) {
                            Ok(req_json) => verify_sign(&req_json),
                            Err(e) => Response::new().with_status(hyper::StatusCode::BadRequest)
                                .with_body(format!("Unable to parse request: {:?}", e))
                        }
                    } else {
                        Response::new().with_status(hyper::StatusCode::BadRequest)
                            .with_body("Unable to translate body into UTF8")
                    }
                },
                Err(e) => Response::new().with_status(hyper::StatusCode::InternalServerError)
                    .with_body(format!("Unable to read request {:?}", e))
            };

            future::ok(response)
        });

        Box::new(result)
    }
}

fn main() {
    let addr = "0.0.0.0:80".parse().unwrap();
    let server = Http::new().bind(&addr, || Ok(Sign {})).unwrap();

    println!("Listening on port 80");

    server.run().unwrap();
}

#[test]
fn test_verify() {
    use rfnet_core::message;
    use rust_sodium::crypto::sign;

    let msg = message::RequestMessage {
        sequence_id: 1024,
        addr: "KI7EST@rfnet.net",
        req_type: message::RequestType::REST {
            method: message::RESTMethod::GET,
            url: "http://localhost/verify",
            headers: "Content-Type: application/octet-stream\r\nX-custom: foo",
            body: b"BODY"
        }
    };

    let (public_key, private_key) = sign::gen_keypair();

    let mut encoded = vec!();
    let mut scratch = vec!();

    message::encode_request_message(&msg, &private_key[..], &mut scratch, &mut encoded).unwrap();

    let decoded = message::decode_request_message(&encoded[..]).unwrap();

    let mut json_msg = SignRequest {
        sequence_id: msg.sequence_id,
        addr: msg.addr.to_string(),
        headers: vec!(
            ("Content-Type".to_string(), "application/octet-stream".to_string()),
            ("X-custom".to_string(), "foo".to_string())
            ),
        url: "http://localhost/verify".to_string(),
        method: "GET".to_string(),
        body: base64::encode(b"BODY"),
        signature: base64::encode(decoded.signature),
        public_keys: vec!(base64::encode(&public_key[..]))
    };

    test_signature(&json_msg, true);

    let valid_sig = json_msg.signature.clone();
    let invalid_sig = (0..64).collect::<Vec<u8>>();
    json_msg.signature = base64::encode(&sign::Signature::from_slice(&invalid_sig[..]).unwrap()[..]);

    test_signature(&json_msg, false);

    json_msg.signature = valid_sig;
    test_signature(&json_msg, true);

    json_msg.body = base64::encode(b"NOTBODY");
    test_signature(&json_msg, false);
}

#[cfg(test)]
fn test_signature(json_msg: &SignRequest, success: bool) {
    let json = serde_json::to_string(&json_msg).unwrap();
    let response = verify_sign(&serde_json::from_str(json.as_str()).unwrap());

    assert_eq!(response.status(), hyper::StatusCode::Ok);

    use futures::stream::Stream;
    let body = response.body().concat2().wait().unwrap();
    let body_str = ::std::str::from_utf8(&*body).unwrap();
    assert_eq!(body_str, format!("{{ \"verified\": {} }}", success));
}