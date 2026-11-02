//! Native HTTPS downloads. Cancelling drops the active request, without a helper process.
use anyhow::{ensure, Context, Result};
use std::{
    fs::OpenOptions,
    io::{Seek, SeekFrom, Write},
    path::Path,
    sync::atomic::{AtomicBool, Ordering},
    time::Duration,
};

const RANGE_THRESHOLD: u64 = 4 * 1024 * 1024;
const RANGE_PARTS: u64 = 4;

pub fn download(url: &str, destination: &Path, limit: u64, cancel: &AtomicBool) -> Result<()> {
    run(url, destination, limit, None, cancel)
}

pub fn download_update(
    url: &str,
    destination: &Path,
    size: u64,
    limit: u64,
    cancel: &AtomicBool,
) -> Result<()> {
    ensure!(size > 0 && size <= limit, "更新文件大小超出限制");
    run(url, destination, size, Some(size), cancel)
}

async fn write_body(
    mut response: reqwest::Response,
    mut file: std::fs::File,
    limit: u64,
) -> Result<u64> {
    let mut total = 0_u64;
    while let Some(bytes) = response.chunk().await? {
        total += bytes.len() as u64;
        ensure!(total <= limit, "下载文件超过大小限制");
        file.write_all(&bytes)?;
    }
    Ok(total)
}

fn check_range(response: &reqwest::Response, start: u64, end: u64, size: u64) -> Result<()> {
    ensure!(
        response.status() == reqwest::StatusCode::PARTIAL_CONTENT,
        "服务器未返回请求的下载分片"
    );
    let range = response
        .headers()
        .get(reqwest::header::CONTENT_RANGE)
        .context("分片响应缺少 Content-Range")?
        .to_str()?;
    ensure!(
        range == format!("bytes {start}-{end}/{size}"),
        "分片响应范围与请求不一致"
    );
    ensure!(
        response
            .content_length()
            .is_none_or(|length| length == end - start + 1),
        "分片响应长度不正确"
    );
    Ok(())
}

async fn receive(
    client: reqwest::Client,
    url: &str,
    destination: &Path,
    limit: u64,
    size: Option<u64>,
) -> Result<()> {
    let segmented = size.filter(|size| *size >= RANGE_THRESHOLD);
    let mut request = client.get(url);
    let part_size = segmented.map(|size| size.div_ceil(RANGE_PARTS));
    if let Some(part_size) = part_size {
        request = request.header(reqwest::header::RANGE, format!("bytes=0-{}", part_size - 1));
    }
    let response = request
        .send()
        .await
        .context("无法连接下载地址")?
        .error_for_status()?;
    if let (Some(size), Some(part_size)) = (segmented, part_size) {
        if response.status() == reqwest::StatusCode::PARTIAL_CONTENT {
            check_range(&response, 0, part_size - 1, size)?;
            let file = OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(destination)?;
            file.set_len(size)?;
            let mut parts = tokio::task::JoinSet::new();
            parts.spawn(async move {
                ensure!(
                    write_body(response, file, part_size).await? == part_size,
                    "下载分片不完整"
                );
                Ok::<(), anyhow::Error>(())
            });
            for index in 1..RANGE_PARTS {
                let client = client.clone();
                let url = url.to_string();
                let destination = destination.to_path_buf();
                let start = index * part_size;
                let end = ((index + 1) * part_size).min(size) - 1;
                parts.spawn(async move {
                    let response = client
                        .get(url)
                        .header(reqwest::header::RANGE, format!("bytes={start}-{end}"))
                        .send()
                        .await?
                        .error_for_status()?;
                    check_range(&response, start, end, size)?;
                    let mut file = OpenOptions::new().write(true).open(destination)?;
                    file.seek(SeekFrom::Start(start))?;
                    ensure!(
                        write_body(response, file, end - start + 1).await? == end - start + 1,
                        "下载分片不完整"
                    );
                    Ok::<(), anyhow::Error>(())
                });
            }
            while let Some(result) = parts.join_next().await {
                result??;
            }
            return Ok(());
        }
    }
    // A 200 response to Range already contains the full file; consume it once.
    ensure!(
        response.status() != reqwest::StatusCode::PARTIAL_CONTENT,
        "服务器返回了未请求的分片"
    );
    ensure!(
        response.content_length().is_none_or(|size| size <= limit),
        "下载文件超过大小限制"
    );
    let file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(destination)?;
    ensure!(
        write_body(response, file, limit).await? > 0,
        "下载地址返回了空文件"
    );
    Ok(())
}

fn run(
    url: &str,
    destination: &Path,
    limit: u64,
    size: Option<u64>,
    cancel: &AtomicBool,
) -> Result<()> {
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
                    .connect_timeout(Duration::from_secs(180))
                    .read_timeout(Duration::from_secs(180))
                    .build()?;
                receive(client, url, destination, limit, size).await
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
        net::{TcpListener, TcpStream},
        thread,
        time::Instant,
    };

    fn request(listener: &TcpListener) -> (TcpStream, Option<String>) {
        let deadline = Instant::now() + Duration::from_secs(10);
        let mut connection = loop {
            match listener.accept() {
                Ok((connection, _)) => break connection,
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    assert!(
                        Instant::now() < deadline,
                        "parallel download did not issue every request"
                    );
                    thread::sleep(Duration::from_millis(10));
                }
                Err(error) => panic!("{error}"),
            }
        };
        connection.set_nonblocking(false).unwrap();
        connection
            .set_read_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        let mut range = None;
        {
            let mut reader = BufReader::new(&mut connection);
            let mut line = String::new();
            loop {
                assert!(reader.read_line(&mut line).unwrap() > 0);
                if line == "\r\n" {
                    break;
                }
                if let Some((name, value)) = line.split_once(':') {
                    if name.eq_ignore_ascii_case("range") {
                        range = Some(value.trim().to_string());
                    }
                }
                line.clear();
            }
        }
        (connection, range)
    }

    #[test]
    fn large_updates_download_four_ranges_concurrently_and_reject_wrong_ranges() {
        let size = RANGE_THRESHOLD + 13;
        let payload: Vec<u8> = (0..size).map(|index| (index % 251) as u8).collect();
        for mode in ["ranges", "ignore-range", "wrong-range"] {
            let listener = TcpListener::bind("127.0.0.1:0").unwrap();
            listener.set_nonblocking(true).unwrap();
            let url = format!("http://{}/update.exe", listener.local_addr().unwrap());
            let data = payload.clone();
            let server = thread::spawn(move || {
                let (mut first, range) = request(&listener);
                let part_size = size.div_ceil(RANGE_PARTS);
                assert_eq!(
                    range.as_deref(),
                    Some(format!("bytes=0-{}", part_size - 1).as_str())
                );
                if mode == "ignore-range" {
                    write!(
                        first,
                        "HTTP/1.1 200 OK\r\nContent-Length: {size}\r\nConnection: close\r\n\r\n"
                    )
                    .unwrap();
                    first.write_all(&data).unwrap();
                    return;
                }
                let total = if mode == "wrong-range" {
                    size + 1
                } else {
                    size
                };
                write!(first, "HTTP/1.1 206 Partial Content\r\nContent-Range: bytes 0-{}/{total}\r\nContent-Length: {part_size}\r\nConnection: close\r\n\r\n", part_size - 1).unwrap();
                if mode == "wrong-range" {
                    return;
                }
                // Hold the first body until all other ranges have been requested.
                let mut connections = vec![(first, 0, part_size - 1)];
                for _ in 1..RANGE_PARTS {
                    let (connection, range) = request(&listener);
                    let range = range.unwrap();
                    let (start, end) = range
                        .strip_prefix("bytes=")
                        .unwrap()
                        .split_once('-')
                        .unwrap();
                    connections.push((
                        connection,
                        start.parse::<u64>().unwrap(),
                        end.parse::<u64>().unwrap(),
                    ));
                }
                let mut requested = Vec::new();
                for (mut connection, start, end) in connections {
                    if start != 0 {
                        write!(connection, "HTTP/1.1 206 Partial Content\r\nContent-Range: bytes {start}-{end}/{size}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", end - start + 1).unwrap();
                    }
                    requested.push((start, end));
                    connection
                        .write_all(&data[start as usize..=end as usize])
                        .unwrap();
                }
                requested.sort();
                assert_eq!(requested.len(), RANGE_PARTS as usize);
                assert_eq!(requested.first().unwrap().0, 0);
                assert_eq!(requested.last().unwrap().1, size - 1);
                for pair in requested.windows(2) {
                    assert_eq!(pair[0].1 + 1, pair[1].0);
                }
            });
            let temp = tempfile::tempdir().unwrap();
            let destination = temp.path().join("update.exe");
            let result = download_update(&url, &destination, size, size, &AtomicBool::new(false));
            server.join().unwrap();
            if mode == "wrong-range" {
                assert!(format!("{:#}", result.unwrap_err()).contains("范围"));
                assert!(!destination.exists());
            } else {
                result.unwrap();
                assert_eq!(std::fs::read(destination).unwrap(), payload);
            }
        }
    }

    #[test]
    fn cancellation_closes_every_range_and_releases_the_output_file() {
        use std::{io::Read, sync::Arc};
        let size = RANGE_THRESHOLD + 13;
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let url = format!("http://{}/update.exe", listener.local_addr().unwrap());
        let temp = tempfile::tempdir().unwrap();
        let destination = temp.path().join("update.exe");
        let output = destination.clone();
        let cancel = Arc::new(AtomicBool::new(false));
        let stop = cancel.clone();
        let worker = thread::spawn(move || download_update(&url, &output, size, size, &stop));
        let mut connections = Vec::new();
        for _ in 0..RANGE_PARTS {
            let (mut connection, range) = request(&listener);
            let range = range.unwrap();
            let bounds = range.strip_prefix("bytes=").unwrap();
            let (start, end) = bounds.split_once('-').unwrap();
            let length = end.parse::<u64>().unwrap() - start.parse::<u64>().unwrap() + 1;
            write!(connection, "HTTP/1.1 206 Partial Content\r\nContent-Range: bytes {bounds}/{size}\r\nContent-Length: {length}\r\nConnection: close\r\n\r\n").unwrap();
            connections.push(connection);
        }
        let started = Instant::now();
        cancel.store(true, Ordering::Relaxed);
        let error = worker.join().unwrap().unwrap_err();
        assert!(format!("{error:#}").contains("取消"));
        assert!(started.elapsed() < Duration::from_secs(3));
        std::fs::remove_file(destination).expect("cancelled ranges must release all file handles");
        for mut connection in connections {
            let mut bytes = Vec::new();
            if let Err(error) = connection.read_to_end(&mut bytes) {
                assert!(
                    matches!(
                        error.kind(),
                        std::io::ErrorKind::ConnectionReset | std::io::ErrorKind::ConnectionAborted
                    ),
                    "range connection remains open: {error}"
                );
            }
        }
    }

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
                connection.set_nonblocking(false).unwrap();
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
