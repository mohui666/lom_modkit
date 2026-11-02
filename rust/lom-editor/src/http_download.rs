//! Native HTTPS downloads. Cancelling drops the active request, without a helper process.
use anyhow::{ensure, Context, Result};
use std::{
    fs::OpenOptions,
    io::Write,
    path::Path,
    sync::atomic::{AtomicBool, Ordering},
    time::Duration,
};

pub fn download(url: &str, destination: &Path, limit: u64, cancel: &AtomicBool) -> Result<()> {
    ensure!(!cancel.load(Ordering::Relaxed), "下载已取消");
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    let result = runtime.block_on(async {
        let cancelled = async {
            while !cancel.load(Ordering::Relaxed) {
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
        };
        tokio::select! {
            biased;
            _ = cancelled => anyhow::bail!("下载已取消"),
            result = async {
                let client = reqwest::Client::builder()
                    .user_agent("lom-modkit-native-updater")
                    .timeout(Duration::from_secs(180))
                    .build()?;
                let mut response = client.get(url).send().await.context("无法连接下载地址")?.error_for_status()?;
                ensure!(response.content_length().is_none_or(|size| size <= limit), "下载文件超过大小限制");
                let mut file = OpenOptions::new().write(true).create_new(true).open(destination)?;
                let mut total = 0_u64;
                while let Some(bytes) = response.chunk().await? {
                    total += bytes.len() as u64;
                    ensure!(total <= limit, "下载文件超过大小限制");
                    file.write_all(&bytes)?;
                }
                ensure!(total > 0, "下载地址返回了空文件");
                Ok(())
            } => result,
        }
    });
    // A cancelled system DNS lookup must not hold up the GUI's exit callback.
    runtime.shutdown_timeout(Duration::from_millis(100));
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        io::{BufRead, BufReader},
        net::TcpListener,
        thread,
        time::Instant,
    };

    #[test]
    fn native_http_keeps_status_size_and_empty_body_checks() {
        for (response, limit, succeeds) in [
            ("HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\n{}", 8, true),
            ("HTTP/1.1 503 Unavailable\r\nContent-Length: 2\r\nConnection: close\r\n\r\n{}", 8, false),
            ("HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\n{}", 1, false),
            ("HTTP/1.1 200 OK\r\nContent-Length: 0\r\nConnection: close\r\n\r\n", 8, false),
            ("HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\nConnection: close\r\n\r\n4\r\n1234\r\n0\r\n\r\n", 3, false),
        ] {
            let listener = TcpListener::bind("127.0.0.1:0").unwrap();
            listener.set_nonblocking(true).unwrap();
            let url = format!("http://{}/download", listener.local_addr().unwrap());
            let server = thread::spawn(move || {
                let deadline = Instant::now() + Duration::from_secs(10);
                let mut connection = loop {
                    match listener.accept() {
                        Ok((connection, _)) => break connection,
                        Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                            assert!(Instant::now() < deadline, "native HTTP client did not connect");
                            thread::sleep(Duration::from_millis(10));
                        }
                        Err(error) => panic!("{error}"),
                    }
                };
                connection.set_read_timeout(Some(Duration::from_secs(3))).unwrap();
                {
                    let mut request = BufReader::new(&mut connection);
                    let mut line = String::new();
                    loop {
                        assert!(request.read_line(&mut line).unwrap() > 0);
                        if line == "\r\n" { break; }
                        line.clear();
                    }
                }
                connection.write_all(response.as_bytes()).unwrap();
            });
            let temp = tempfile::tempdir().unwrap();
            let file = temp.path().join("download.bin");
            let result = download(&url, &file, limit, &AtomicBool::new(false));
            server.join().unwrap();
            assert_eq!(result.is_ok(), succeeds, "{result:?}");
            if succeeds { assert_eq!(std::fs::read(&file).unwrap(), b"{}"); }
        }
    }
}
