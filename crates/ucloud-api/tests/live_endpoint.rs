//! A live client must use the configured endpoint and credentials without
//! touching the real UCloud account. The test server only answers GetBalance.
use std::io::{Read, Write};
use std::net::TcpListener;
use ucloud_api::{Credentials, UcloudClient};

#[test]
fn custom_endpoint_and_credentials_are_used_for_live_reads() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let server = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut buffer = [0u8; 8192];
        let count = stream.read(&mut buffer).unwrap();
        let request = String::from_utf8_lossy(&buffer[..count]);
        assert!(request.starts_with("POST / "));
        assert!(request.contains("Action=GetBalance"));
        assert!(request.contains("PublicKey=test-public"));
        assert!(request.contains("Signature="));
        let body =
            r#"{"Action":"GetBalance","RetCode":0,"Message":"","AccountInfo":{"Amount":12.5}}"#;
        write!(
            stream,
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            body.len(),
            body
        )
        .unwrap();
    });
    let client = UcloudClient::live_with(
        Credentials::new("test-public", "test-private", None, None).unwrap(),
        Some(format!("http://127.0.0.1:{port}")),
        Some("cn-bj2".into()),
    )
    .unwrap();
    let balance = client.balance().unwrap();
    assert!((balance.account.amount - 12.5).abs() < f64::EPSILON);
    server.join().unwrap();
}
