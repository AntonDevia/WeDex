use super::*;
use reqwest::{
    blocking::Response,
    header::{CONTENT_RANGE, ETAG, IF_RANGE, RANGE},
};

/// Resume an interrupted response without replaying bytes already buffered.
/// Servers ignoring Range are handled by discarding the repeated prefix.
pub(super) fn stream(
    http: &reqwest::blocking::Client,
    url: &str,
    token: &str,
    initial: Option<Response>,
    target: &SharedBuffer,
    generation: &AtomicU64,
    ticket: u64,
) -> io::Result<()> {
    let mut initial = initial;
    let mut received = 0_u64;
    let mut expected = None;
    let mut etag = None;
    let mut last = io::Error::other("Аудиопоток не загрузился");
    for attempt in 0..4 {
        let check = || {
            if generation.load(Ordering::Relaxed) != ticket {
                Err(io::Error::new(
                    io::ErrorKind::Interrupted,
                    "Загрузка отменена",
                ))
            } else {
                Ok(())
            }
        };
        check()?;
        if attempt > 0 {
            let _ = crate::diagnostics::append(
                &crate::diagnostics::log_path(),
                "audio-retry",
                &format!(
                    "attempt={attempt} buffered={received} error_kind={:?}",
                    last.kind()
                ),
                &[],
            );
            std::thread::sleep(Duration::from_millis(150 * attempt));
        }
        let response = if let Some(response) = initial.take() {
            Ok(response)
        } else {
            let mut request = audio_request(http, url, token);
            if received > 0 {
                request = request.header(RANGE, format!("bytes={received}-"));
                if let Some(tag) = &etag {
                    request = request.header(IF_RANGE, tag);
                }
            }
            response(request.send()).map_err(|e| io::Error::other(format!("{e:#}")))
        };
        let mut response = match response {
            Ok(r) => r,
            Err(e) => {
                last = e;
                continue;
            }
        };
        if attempt == 0 {
            expected = response.content_length();
            etag = response
                .headers()
                .get(ETAG)
                .and_then(|h| h.to_str().ok())
                .map(str::to_owned);
        }
        if let (Some(old), Some(new)) = (
            &etag,
            response.headers().get(ETAG).and_then(|h| h.to_str().ok()),
        ) {
            if old != new {
                return Err(io::Error::other("Аудиозапись изменилась во время загрузки"));
            }
        }
        if received > 0 && response.status() == reqwest::StatusCode::PARTIAL_CONTENT {
            let start = response
                .headers()
                .get(CONTENT_RANGE)
                .and_then(|h| h.to_str().ok())
                .and_then(|h| h.strip_prefix("bytes "))
                .and_then(|h| h.split_once('-'))
                .and_then(|h| h.0.parse::<u64>().ok());
            if start != Some(received) {
                return Err(io::Error::other("Сервер вернул неверную позицию аудио"));
            }
        } else if received > 0 {
            let skipped = io::copy(&mut response.by_ref().take(received), &mut io::sink());
            match skipped {
                Ok(n) if n == received => {}
                Ok(_) => {
                    last = io::Error::new(io::ErrorKind::UnexpectedEof, "Неполный повторный ответ");
                    continue;
                }
                Err(e) => {
                    last = e;
                    continue;
                }
            }
        }
        let mut chunk = [0_u8; 32 * 1024];
        loop {
            check()?;
            match response.read(&mut chunk) {
                Ok(0) if expected.is_none_or(|n| received == n) => return Ok(()),
                Ok(0) => {
                    last = io::Error::new(
                        io::ErrorKind::UnexpectedEof,
                        "Аудиопоток оборвался до конца записи",
                    );
                    break;
                }
                Ok(n) => {
                    let mut buffer = target.0.lock().unwrap();
                    if buffer.bytes.len() + n > MAX_BUFFER {
                        return Err(io::Error::other("Запись превышает лимит буфера"));
                    }
                    buffer.bytes.extend_from_slice(&chunk[..n]);
                    received += n as u64;
                    target.1.notify_all();
                }
                Err(e) => {
                    last = e;
                    break;
                }
            }
        }
    }
    Err(last)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{io::Write, net::TcpListener};
    #[test]
    fn interrupted_http_audio_resumes_without_duplicate_bytes() {
        let expected = (0..64000).map(|i| (i % 251) as u8).collect::<Vec<_>>();
        let bytes = expected.clone();
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let server = std::thread::spawn(move || {
            for index in 0..2 {
                let (mut socket, _) = listener.accept().unwrap();
                socket
                    .set_read_timeout(Some(Duration::from_secs(3)))
                    .unwrap();
                let mut request = vec![];
                let mut byte = [0_u8; 1];
                while socket.read_exact(&mut byte).is_ok() {
                    request.push(byte[0]);
                    if request.ends_with(b"\r\n\r\n") {
                        break;
                    }
                }
                if index == 0 {
                    write!(socket,"HTTP/1.1 200 OK\r\nContent-Length: {}\r\nETag: fixture\r\nConnection: close\r\n\r\n",bytes.len()).unwrap();
                    socket.write_all(&bytes[..16000]).unwrap();
                } else {
                    let request = String::from_utf8(request).unwrap().to_lowercase();
                    assert!(request.contains("range: bytes=16000-"));
                    write!(socket,"HTTP/1.1 206 Partial Content\r\nContent-Length: {}\r\nContent-Range: bytes 16000-63999/64000\r\nETag: fixture\r\nConnection: close\r\n\r\n",bytes.len()-16000).unwrap();
                    socket.write_all(&bytes[16000..]).unwrap();
                }
            }
        });
        let target = Arc::new((
            Mutex::new(Buffer {
                bytes: vec![],
                done: false,
                error: None,
            }),
            Condvar::new(),
        ));
        let http = reqwest::blocking::Client::builder()
            .timeout(Duration::from_secs(3))
            .build()
            .unwrap();
        stream(
            &http,
            &format!("http://{addr}/audio"),
            "",
            None,
            &target,
            &AtomicU64::new(1),
            1,
        )
        .unwrap();
        assert_eq!(target.0.lock().unwrap().bytes, expected);
        server.join().unwrap();
    }
}
