use super::*;

#[test]
fn endpoint_is_literal_loopback_without_implicit_destinations() {
    for endpoint in ["http://127.0.0.1:8080/v1", "http://[::1]:8080/v1/"] {
        assert!(validate_endpoint(endpoint).is_ok(), "{endpoint}");
    }
    for endpoint in [
        "http://localhost:8080/v1",
        "https://127.0.0.1/v1",
        "http://192.168.1.1/v1",
        "http://127.0.0.1.evil.test/v1",
        "http://user:pass@127.0.0.1/v1",
        "http://127.0.0.1/v1?redirect=bad",
        "http://127.0.0.1/admin",
        "http://127.0.0.1/v1#fragment",
    ] {
        assert!(validate_endpoint(endpoint).is_err(), "{endpoint}");
    }
}

#[tokio::test]
async fn compatible_http_model_round_trip_and_redirect_refusal() {
    use std::io::{Read, Write};
    for redirect in [false, true] {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let server = std::thread::spawn(move || {
            let (mut socket, _) = listener.accept().unwrap();
            socket
                .set_read_timeout(Some(std::time::Duration::from_secs(5)))
                .unwrap();
            let mut bytes = vec![];
            loop {
                let mut buffer = [0; 4096];
                let n = socket.read(&mut buffer).unwrap();
                if n == 0 {
                    break;
                }
                bytes.extend_from_slice(&buffer[..n]);
                if let Some(end) = bytes.windows(4).position(|b| b == b"\r\n\r\n") {
                    let headers = String::from_utf8_lossy(&bytes[..end]);
                    let size = headers
                        .lines()
                        .find_map(|line| {
                            line.to_lowercase()
                                .strip_prefix("content-length:")
                                .and_then(|n| n.trim().parse::<usize>().ok())
                        })
                        .unwrap();
                    if bytes.len() >= end + 4 + size {
                        break;
                    }
                }
            }
            let request = String::from_utf8(bytes).unwrap();
            assert!(request.starts_with("POST /v1/chat/completions HTTP/1.1"));
            assert!(request.contains("fixture-model"));
            if redirect {
                socket.write_all(b"HTTP/1.1 302 Found\r\nLocation: http://192.0.2.1/v1/chat/completions\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").unwrap();
            } else {
                let body =
                    r#"{"choices":[{"message":{"role":"assistant","content":"local fixture"}}]}"#;
                write!(socket,"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",body.len(),body).unwrap();
            }
        });
        let model =
            LocalModel::new(&format!("http://{address}/v1"), "fixture-model".into()).unwrap();
        let answer = model
            .complete(&[json!({"role":"user","content":"test"})], &[])
            .await;
        if redirect {
            assert!(answer.is_err());
        } else {
            assert_eq!(answer.unwrap()["content"], "local fixture");
        }
        server.join().unwrap();
    }
}
