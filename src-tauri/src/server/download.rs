use axum::body::{Body, Bytes};
use axum::http::{header, StatusCode};
use axum::response::Response;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncSeekExt};
use tokio_util::io::ReaderStream;

/// Stream buffer size: 64KB
const STREAM_CHUNK_SIZE: usize = 64 * 1024;

/// Parsed HTTP Range specification (inclusive byte range).
#[derive(Debug, Clone, Copy)]
pub struct RangeSpec {
    pub start: u64,
    /// Inclusive end offset.
    pub end: u64,
}

/// Parse an HTTP `Range` header value such as `bytes=0-1023`, `bytes=512-`,
/// or `bytes=-256` (suffix range). Returns `None` when the header is missing,
/// malformed, or unsatisfiable for the given `file_size`.
pub fn parse_range_header(range_header: &str, file_size: u64) -> Option<RangeSpec> {
    let range = range_header.strip_prefix("bytes=")?;

    // Only support a single range (no multipart ranges).
    if range.contains(',') {
        return None;
    }

    let (start_str, end_str) = range.split_once('-')?;

    if start_str.is_empty() {
        // Suffix range: bytes=-N  →  last N bytes
        let suffix: u64 = end_str.parse().ok()?;
        if suffix == 0 || suffix > file_size {
            return None;
        }
        return Some(RangeSpec {
            start: file_size - suffix,
            end: file_size - 1,
        });
    }

    let start: u64 = start_str.parse().ok()?;
    if start >= file_size {
        return None;
    }

    let end: u64 = if end_str.is_empty() {
        file_size - 1
    } else {
        let e: u64 = end_str.parse().ok()?;
        e.min(file_size - 1)
    };

    if start > end {
        return None;
    }

    Some(RangeSpec { start, end })
}

/// Build a streaming HTTP response for a file, honouring an optional byte
/// range. The file is streamed in 64KB chunks so it is never loaded fully
/// into memory.
///
/// * `Ok(response)` – a `200 OK` (full file) or `206 Partial Content` response.
/// * `Err(io_error)` – the file could not be opened or seeked.
pub async fn stream_file_response(
    path: &Path,
    range: Option<RangeSpec>,
    file_size: u64,
    mime_type: &str,
) -> Result<Response, std::io::Error> {
    match range {
        Some(spec) => {
            let content_length = spec.end - spec.start + 1;

            let mut file = tokio::fs::File::open(path).await?;
            file.seek(std::io::SeekFrom::Start(spec.start)).await?;

            // Limit the reader to exactly the requested byte count so the
            // stream terminates at the right boundary.
            let limited = file.take(content_length);
            let stream = ReaderStream::with_capacity(limited, STREAM_CHUNK_SIZE);
            let body = Body::from_stream(stream);

            let content_range = format!("bytes {}-{}/{}", spec.start, spec.end, file_size);

            let response = Response::builder()
                .status(StatusCode::PARTIAL_CONTENT)
                .header(header::CONTENT_TYPE, mime_type)
                .header(header::ACCEPT_RANGES, "bytes")
                .header(header::CONTENT_LENGTH, content_length)
                .header(header::CONTENT_RANGE, content_range)
                .body(body)
                .map_err(std::io::Error::other)?;

            Ok(response)
        }
        None => {
            let file = tokio::fs::File::open(path).await?;
            let stream = ReaderStream::with_capacity(file, STREAM_CHUNK_SIZE);
            let body = Body::from_stream(stream);

            let response = Response::builder()
                .status(StatusCode::OK)
                .header(header::CONTENT_TYPE, mime_type)
                .header(header::ACCEPT_RANGES, "bytes")
                .header(header::CONTENT_LENGTH, file_size)
                .body(body)
                .map_err(std::io::Error::other)?;

            Ok(response)
        }
    }
}

/// Sleep long enough after each chunk to keep the stream near the configured
/// bandwidth cap. `0` disables throttling.
async fn pace_result(
    result: Result<Bytes, std::io::Error>,
    speed_limit_bytes_per_sec: u64,
) -> Result<Bytes, std::io::Error> {
    if speed_limit_bytes_per_sec > 0 {
        if let Ok(ref bytes) = result {
            let chunk_len = bytes.len() as u64;
            let target = std::time::Duration::from_secs_f64(
                chunk_len as f64 / speed_limit_bytes_per_sec as f64,
            );
            tokio::time::sleep(target).await;
        }
    }
    result
}

/// Build a streaming response that also tracks how many bytes have been sent
/// so far via a shared atomic counter. The caller can use the counter to
/// broadcast progress events while the stream is being consumed.
pub async fn stream_file_with_progress(
    path: &Path,
    range: Option<RangeSpec>,
    file_size: u64,
    mime_type: &str,
    bytes_counter: Arc<AtomicU64>,
    // Optional bandwidth cap in bytes per second; 0 disables throttling.
    speed_limit_bytes_per_sec: u64,
) -> Result<Response, std::io::Error> {
    use futures_util::StreamExt;

    let initial_offset = range.map(|r| r.start).unwrap_or(0);
    bytes_counter.store(initial_offset, Ordering::Relaxed);

    match range {
        Some(spec) => {
            let content_length = spec.end - spec.start + 1;

            let mut file = tokio::fs::File::open(path).await?;
            file.seek(std::io::SeekFrom::Start(spec.start)).await?;

            let limited = file.take(content_length);
            let stream = ReaderStream::with_capacity(limited, STREAM_CHUNK_SIZE);

            let counter = bytes_counter.clone();
            let counting_stream = stream.then(move |result| {
                if let Ok(ref bytes) = result {
                    counter.fetch_add(bytes.len() as u64, Ordering::Relaxed);
                }
                pace_result(result, speed_limit_bytes_per_sec)
            });

            let body = Body::from_stream(counting_stream);
            let content_range = format!("bytes {}-{}/{}", spec.start, spec.end, file_size);

            let response = Response::builder()
                .status(StatusCode::PARTIAL_CONTENT)
                .header(header::CONTENT_TYPE, mime_type)
                .header(header::ACCEPT_RANGES, "bytes")
                .header(header::CONTENT_LENGTH, content_length)
                .header(header::CONTENT_RANGE, content_range)
                .body(body)
                .map_err(std::io::Error::other)?;

            Ok(response)
        }
        None => {
            let file = tokio::fs::File::open(path).await?;
            let stream = ReaderStream::with_capacity(file, STREAM_CHUNK_SIZE);

            let counter = bytes_counter.clone();
            let counting_stream = stream.then(move |result| {
                if let Ok(ref bytes) = result {
                    counter.fetch_add(bytes.len() as u64, Ordering::Relaxed);
                }
                pace_result(result, speed_limit_bytes_per_sec)
            });

            let body = Body::from_stream(counting_stream);

            let response = Response::builder()
                .status(StatusCode::OK)
                .header(header::CONTENT_TYPE, mime_type)
                .header(header::ACCEPT_RANGES, "bytes")
                .header(header::CONTENT_LENGTH, file_size)
                .body(body)
                .map_err(std::io::Error::other)?;

            Ok(response)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_range_full() {
        let r = parse_range_header("bytes=0-1023", 2048);
        assert!(r.is_some());
        let r = r.unwrap();
        assert_eq!(r.start, 0);
        assert_eq!(r.end, 1023);
    }

    #[test]
    fn test_parse_range_open_end() {
        let r = parse_range_header("bytes=512-", 2048);
        assert!(r.is_some());
        let r = r.unwrap();
        assert_eq!(r.start, 512);
        assert_eq!(r.end, 2047);
    }

    #[test]
    fn test_parse_range_suffix() {
        let r = parse_range_header("bytes=-256", 2048);
        assert!(r.is_some());
        let r = r.unwrap();
        assert_eq!(r.start, 1792);
        assert_eq!(r.end, 2047);
    }

    #[test]
    fn test_parse_range_clamped() {
        let r = parse_range_header("bytes=0-9999", 2048);
        assert!(r.is_some());
        let r = r.unwrap();
        assert_eq!(r.end, 2047);
    }

    #[test]
    fn test_parse_range_unsatisfiable() {
        assert!(parse_range_header("bytes=3000-4000", 2048).is_none());
    }

    #[test]
    fn test_parse_range_invalid() {
        assert!(parse_range_header("items=0-100", 2048).is_none());
        assert!(parse_range_header("bytes=abc", 2048).is_none());
    }
}
