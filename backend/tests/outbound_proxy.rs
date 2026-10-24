// Its own test binary: the outbound proxy is process-wide and set only once.

use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::mpsc;

use elysia::config::{http_client_builder, set_outbound_proxy};
use elysia::services::files;

/// Proxy on a loopback port answering every request with an empty 200 and
/// reporting each request line, which names the absolute target URL.
fn spawn_proxy() -> (String, mpsc::Receiver<String>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let proxy_url = format!("http://{}", listener.local_addr().unwrap());
    let (sender, receiver) = mpsc::channel();
    std::thread::spawn(move || {
        for mut stream in listener.incoming().flatten() {
            let mut request = [0u8; 1024];
            let length = stream.read(&mut request).unwrap_or(0);
            let request = String::from_utf8_lossy(&request[..length]);
            let request_line = request.lines().next().unwrap_or_default().to_string();
            let _ = sender.send(request_line);
            let _ = stream
                .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\nConnection: close\r\n\r\n");
        }
    });
    (proxy_url, receiver)
}

#[actix_web::test]
async fn outbound_requests_go_through_the_configured_proxy() {
    let (proxy_url, request_lines) = spawn_proxy();
    set_outbound_proxy(&proxy_url);
    // `.invalid` never resolves, so these only succeed through the proxy.
    let target = "http://elysia-proxy-test.invalid/image.png";

    let plain_client = http_client_builder().build().unwrap();
    assert!(
        plain_client
            .get(target)
            .send()
            .await
            .unwrap()
            .status()
            .is_success()
    );
    assert_eq!(
        request_lines.recv().unwrap(),
        format!("GET {target} HTTP/1.1")
    );

    // The URL upload client, which otherwise only connects to public addresses.
    assert!(
        files::http_client()
            .get(target)
            .send()
            .await
            .unwrap()
            .status()
            .is_success()
    );
    assert_eq!(
        request_lines.recv().unwrap(),
        format!("GET {target} HTTP/1.1")
    );
}
