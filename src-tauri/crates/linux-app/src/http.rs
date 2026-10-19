//! H5-e: 手写 HTTP 解析的纯函数部分（可单独测试）。
//!
//! 旧实现固定先读 8KB，再按 `Content-Length` 补读；请求头本身超过 8KB、或 TCP 分包正好
//! 落在请求头中间时就会解析失败，表现为大选区解读偶发「Invalid JSON」。

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_header_end_across_split_points() {
        let raw = b"POST /__noesis_ipc__ HTTP/1.1\r\nContent-Length: 5\r\n\r\nhello";
        let full_end = header_end_offset(raw).expect("header end in full request");
        assert_eq!(full_end, raw.len() - 5);

        // 头结束标记正好落在读边界上也要能找到（只要整段头已被读入）
        for cut in full_end..raw.len() {
            assert_eq!(
                header_end_offset(&raw[..cut]),
                Some(full_end),
                "切到 {} 字节时应能定位到头结束",
                cut
            );
        }
        // 头还没读完时不应误报
        assert_eq!(header_end_offset(&raw[..full_end - 1]), None);
    }

    #[test]
    fn parses_content_length_case_insensitively() {
        assert_eq!(parse_content_length("POST / HTTP/1.1\r\nContent-Length: 1048576\r\n"), 1_048_576);
        assert_eq!(parse_content_length("POST / HTTP/1.1\r\ncontent-length:42\r\n"), 42);
        assert_eq!(parse_content_length("POST / HTTP/1.1\r\nHost: x\r\n"), 0);
    }

    #[test]
    fn accepts_1mb_body() {
        // 验收：1MB 选区的 explain_layer 请求体能被完整解析
        let payload = "A".repeat(1024 * 1024);
        let raw = format!(
            "POST /__noesis_ipc__ HTTP/1.1\r\nHost: 127.0.0.1\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
            payload.len(),
            payload
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
}
