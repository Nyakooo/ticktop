use std::{
    io::{Read, Write},
    net::TcpListener,
    time::Duration,
};
use ticktop::{
    config::{Network, ProxyMode},
    market,
};

#[tokio::test]
async fn custom_http_proxy_receives_absolute_target_without_direct_dns() {
    let proxy = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = proxy.local_addr().unwrap();
    let task = std::thread::spawn(move || {
        let (mut stream, _) = proxy.accept().unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(3)))
            .unwrap();
        let mut buffer = [0; 4096];
        let n = stream.read(&mut buffer).unwrap();
        let request = String::from_utf8_lossy(&buffer[..n]).into_owned();
        stream
            .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\nok")
            .unwrap();
        request
    });
    let network = Network {
        proxy_mode: ProxyMode::Custom,
        proxy_url: format!("http://{addr}"),
        timeout_secs: 3,
    };
    let response = market::client(&network)
        .unwrap()
        .get("http://ticktop-does-not-exist.invalid/quotes")
        .send()
        .await
        .unwrap();
    assert_eq!(response.text().await.unwrap(), "ok");
    assert!(
        task.join()
            .unwrap()
            .starts_with("GET http://ticktop-does-not-exist.invalid/quotes HTTP/1.1")
    );
}
#[tokio::test]
async fn socks5h_sends_hostname_to_proxy_instead_of_resolving_locally() {
    let proxy = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = proxy.local_addr().unwrap();
    let task = std::thread::spawn(move || {
        let (mut stream, _) = proxy.accept().unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(3)))
            .unwrap();
        let mut header = [0; 2];
        stream.read_exact(&mut header).unwrap();
        assert_eq!(header[0], 5);
        let mut methods = vec![0; header[1] as usize];
        stream.read_exact(&mut methods).unwrap();
        stream.write_all(&[5, 0]).unwrap();
        let mut connect = [0; 5];
        stream.read_exact(&mut connect).unwrap();
        assert_eq!(&connect[..4], &[5, 1, 0, 3]);
        let mut hostname = vec![0; connect[4] as usize];
        stream.read_exact(&mut hostname).unwrap();
        let mut port = [0; 2];
        stream.read_exact(&mut port).unwrap();
        stream
            .write_all(&[5, 0, 0, 1, 127, 0, 0, 1, 0, 80])
            .unwrap();
        let mut request = [0; 2048];
        let received = stream.read(&mut request).unwrap();
        assert!(received > 0);
        stream
            .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\nok")
            .unwrap();
        String::from_utf8(hostname).unwrap()
    });
    let network = Network {
        proxy_mode: ProxyMode::Custom,
        proxy_url: format!("socks5h://{addr}"),
        timeout_secs: 3,
    };
    let response = market::client(&network)
        .unwrap()
        .get("http://ticktop-socks.invalid/data")
        .send()
        .await
        .unwrap();
    assert_eq!(response.text().await.unwrap(), "ok");
    assert_eq!(task.join().unwrap(), "ticktop-socks.invalid");
}
#[tokio::test]
async fn broken_custom_proxy_never_falls_back_to_direct() {
    let upstream = TcpListener::bind("127.0.0.1:0").unwrap();
    upstream.set_nonblocking(true).unwrap();
    let target = upstream.local_addr().unwrap();
    let proxy = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = proxy.local_addr().unwrap();
    drop(proxy);
    let network = Network {
        proxy_mode: ProxyMode::Custom,
        proxy_url: format!("http://user:private-password@{addr}"),
        timeout_secs: 1,
    };
    let error = market::client(&network)
        .unwrap()
        .get(format!("http://{target}/"))
        .send()
        .await
        .unwrap_err();
    assert!(upstream.accept().is_err());
    assert!(!market::safe_error(&error.into()).contains("private-password"));
}

#[test]
fn malformed_environment_proxies_are_rejected_without_exposing_credentials() {
    for value in [
        "http://alice:secret@",
        "ftp://alice:secret@proxy:21",
        "http://bad host:7890",
    ] {
        let result = market::system_proxies(|name| {
            Ok(if name == "HTTPS_PROXY" {
                Some(value.into())
            } else {
                None
            })
        });
        let message = format!("{:#}", result.unwrap_err());
        assert!(message.contains("HTTPS_PROXY"));
        assert!(!message.contains("secret"));
    }
    assert!(
        market::system_proxies(|name| Ok(if name == "https_proxy" {
            Some("socks5h://localhost:1080".into())
        } else {
            None
        }))
        .is_ok()
    );
}

#[tokio::test]
async fn environment_no_proxy_exclusion_is_respected() {
    let upstream = TcpListener::bind("127.0.0.1:0").unwrap();
    let target = upstream.local_addr().unwrap();
    let task = std::thread::spawn(move || {
        let (mut stream, _) = upstream.accept().unwrap();
        let mut request = [0; 2048];
        assert!(stream.read(&mut request).unwrap() > 0);
        stream
            .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\nok")
            .unwrap();
    });
    let proxies = market::system_proxies(|name| {
        Ok(match name {
            "ALL_PROXY" => Some("http://127.0.0.1:1".into()),
            "NO_PROXY" => Some("127.0.0.1".into()),
            _ => None,
        })
    })
    .unwrap();
    let mut builder = reqwest::Client::builder()
        .no_proxy()
        .timeout(Duration::from_secs(3));
    for proxy in proxies {
        builder = builder.proxy(proxy);
    }
    let body = builder
        .build()
        .unwrap()
        .get(format!("http://{target}/"))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert_eq!(body, "ok");
    task.join().unwrap();
}

#[test]
fn unused_all_proxy_does_not_block_explicit_http_and_https_proxies() {
    let result = market::system_proxies(|name| {
        Ok(match name {
            "HTTP_PROXY" | "HTTPS_PROXY" => Some("http://localhost:7890".into()),
            "ALL_PROXY" => Some("socks://localhost:1080".into()),
            _ => None,
        })
    });
    assert!(result.is_ok());
    let applicable = market::system_proxies(|name| {
        Ok(match name {
            "HTTPS_PROXY" => Some("http://localhost:7890".into()),
            "ALL_PROXY" => Some("socks://localhost:1080".into()),
            _ => None,
        })
    });
    assert!(
        applicable.is_err(),
        "an applicable malformed ALL_PROXY must still block requests"
    );
}
