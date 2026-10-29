//! 手写 HTTP 解析与请求来源校验的纯函数，可单独测试。
//!
//! Linux 本地 API 只供同源桌面 UI 使用；不要为跨域网页授予 CORS 读取权限。

/// 请求头最大允许长度，超过直接拒绝，避免恶意超长头把内存吃满。
pub const MAX_HEADER_BYTES: usize = 64 * 1024;

pub fn find_subslice(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() || haystack.len() < needle.len() {
        return None;
    }
    haystack.windows(needle.len()).position(|w| w == needle)
}

/// 在已读缓冲区里找请求头结束位置，返回「请求头 + 空行」的总字节数。
pub fn header_end_offset(buf: &[u8]) -> Option<usize> {
    find_subslice(buf, b"\r\n\r\n").map(|pos| pos + 4)
}

/// 解析 `Content-Length`（大小写不敏感）。
pub fn parse_content_length(header_text: &str) -> usize {
    for line in header_text.lines() {
        let lower = line.to_ascii_lowercase();
        if let Some(rest) = lower.strip_prefix("content-length:") {
            return rest.trim().parse().unwrap_or(0);
        }
    }
    0
}

/// 按 `Content-Length` 裁剪请求体，丢弃多余的字节。
pub fn trim_body(mut body: Vec<u8>, content_length: usize) -> Vec<u8> {
    if content_length > 0 {
        body.truncate(content_length);
    }
    body
}

/// 拒绝任何跨源浏览器请求。无 Origin 的本地客户端仍可访问 loopback API；
/// 浏览器同源请求必须与本次启动的随机/固定本地 UI origin 完全匹配。
pub fn origin_is_allowed(headers: &str, allowed_origin: &str) -> bool {
    let mut origins = headers.lines().filter_map(|line| {
        let (name, value) = line.split_once(':')?;
        name.trim()
            .eq_ignore_ascii_case("origin")
            .then(|| value.trim())
    });
    match (origins.next(), origins.next()) {
        (None, None) => true,
        (Some(origin), None) => origin == allowed_origin,
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_header_end_across_split_points() {
        let raw = b"POST /__noesis_ipc__ HTTP/1.1\r\nContent-Length: 5\r\n\r\nhello";
        let full_end = header_end_offset(raw).expect("header end in full request");
        assert_eq!(full_end, raw.len() - 5);
        for cut in full_end..raw.len() {
            assert_eq!(header_end_offset(&raw[..cut]), Some(full_end));
        }
        assert_eq!(header_end_offset(&raw[..full_end - 1]), None);
    }

    #[test]
    fn parses_content_length_case_insensitively() {
        assert_eq!(
            parse_content_length("POST / HTTP/1.1\r\nContent-Length: 1048576\r\n"),
            1_048_576
        );
        assert_eq!(
            parse_content_length("POST / HTTP/1.1\r\ncontent-length:42\r\n"),
            42
        );
        assert_eq!(parse_content_length("POST / HTTP/1.1\r\nHost: x\r\n"), 0);
    }

    #[test]
    fn accepts_1mb_body() {
        let payload = "A".repeat(1024 * 1024);
        let raw = format!(
            "POST /__noesis_ipc__ HTTP/1.1\r\nHost: 127.0.0.1\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
            payload.len(), payload
        );
        let bytes = raw.as_bytes();
        let end = header_end_offset(bytes).expect("header end");
        let header_text = String::from_utf8_lossy(&bytes[..end]).to_string();
        let content_length = parse_content_length(&header_text);
        assert_eq!(content_length, payload.len());
        let body = trim_body(bytes[end..].to_vec(), content_length);
        assert_eq!(body.len(), 1024 * 1024);
        assert!(String::from_utf8_lossy(&body).chars().all(|c| c == 'A'));
    }

    #[test]
    fn rejects_oversized_header() {
        let big = vec![b'x'; MAX_HEADER_BYTES + 8];
        assert_eq!(header_end_offset(&big), None);
    }

    #[test]
    fn only_accepts_the_active_local_origin() {
        let allowed = "http://127.0.0.1:54321";
        assert!(origin_is_allowed(
            "GET / HTTP/1.1\r\nOrigin: http://127.0.0.1:54321\r\n",
            allowed
        ));
        assert!(origin_is_allowed("GET / HTTP/1.1\r\n", allowed));
        assert!(!origin_is_allowed(
            "GET / HTTP/1.1\r\nOrigin: https://attacker.example\r\n",
            allowed
        ));
        assert!(!origin_is_allowed(
            "GET / HTTP/1.1\r\nOrigin: null\r\n",
            allowed
        ));
        assert!(!origin_is_allowed(
            "GET / HTTP/1.1\r\nOrigin: http://127.0.0.1:54321\r\nOrigin: https://attacker.example\r\n",
            allowed
        ));
    }
}
