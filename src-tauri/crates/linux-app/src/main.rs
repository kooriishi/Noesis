use fs2::FileExt;
use include_dir::{include_dir, Dir};
use noesis_commands::{dispatch, AppState, EventSink, ACTIVE_TASKS, IS_INDEXING};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::{Arc, Mutex};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::broadcast;

static DIST_DIR: Dir = include_dir!("$CARGO_MANIFEST_DIR/../../../dist");

static LAST_HEARTBEAT: AtomicI64 = AtomicI64::new(0);

const IPC_SHIM_SCRIPT: &str = r#"<script>
(function() {
  const channels = new Map();

  function ensureChannel(channelId, onEventCallback) {
    if (!channelId || channels.has(channelId)) return;
    const es = new EventSource('/__noesis_events__?channel=' + encodeURIComponent(channelId));
    channels.set(channelId, es);
    es.onmessage = function(e) {
      try {
        const data = JSON.parse(e.data);
        if (typeof onEventCallback === 'function') {
          onEventCallback(data);
        }
      } catch (err) {
        console.error('SSE JSON parse error:', err);
      }
    };
    es.onerror = function() {
      es.close();
      channels.delete(channelId);
    };
  }

  // Intercept Channel constructors if instantiated
  if (typeof window !== 'undefined') {
    setInterval(() => {
      fetch('/__noesis_heartbeat__').catch(() => {});
    }, 10000);
  }

  window.__TAURI_INTERNALS__ = {
    invoke: async function(cmd, args) {
      let channelId = null;
      let onEventCb = null;
      if (args && args.onEvent) {
        if (typeof args.onEvent.onmessage === 'function') {
          onEventCb = (data) => args.onEvent.onmessage(data);
          channelId = args.onEvent.__tauri_channel_id__ || ('ch_' + Math.random().toString(36).substring(2, 9));
          args.onEvent.__tauri_channel_id__ = channelId;
        } else if (args.onEvent.__tauri_channel_id__) {
          channelId = String(args.onEvent.__tauri_channel_id__);
        }
      } else if (args && args.on_event) {
        if (typeof args.on_event.onmessage === 'function') {
          onEventCb = (data) => args.on_event.onmessage(data);
          channelId = args.on_event.__tauri_channel_id__ || ('ch_' + Math.random().toString(36).substring(2, 9));
          args.on_event.__tauri_channel_id__ = channelId;
        } else if (args.on_event.__tauri_channel_id__) {
          channelId = String(args.on_event.__tauri_channel_id__);
        }
      }

      if (channelId && onEventCb) {
        ensureChannel(channelId, onEventCb);
      }

      const q = channelId ? '?channel=' + encodeURIComponent(channelId) : '';
      const res = await fetch('/__noesis_ipc__' + q, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ cmd: cmd, args: args || {} })
      });
      const payload = await res.json();
      if (!res.ok || payload.error) {
        throw new Error(payload.error || 'Noesis IPC Error');
      }
      return payload.result;
    },
    transformCallback: function(callback, once) {
      const id = window._cb_id = (window._cb_id || 1) + 1;
      window['_' + id] = (res) => {
        callback(res);
        if (once) delete window['_' + id];
      };
      return id;
    },
    unregisterCallback: function(id) {
      delete window['_' + id];
    },
    metadata: {
      currentWindow: { label: 'main' },
      currentWebview: { label: 'main', windowLabel: 'main' }
    }
  };
})();
</script>"#;

struct SseEventSink {
    tx: broadcast::Sender<String>,
}

impl EventSink for SseEventSink {
    fn send_event(&self, event: Value) -> Result<(), String> {
        let msg = serde_json::to_string(&event).map_err(|e| e.to_string())?;
        let _ = self.tx.send(msg);
        Ok(())
    }
}

type ChannelMap = Arc<Mutex<HashMap<String, broadcast::Sender<String>>>>;

fn get_or_create_channel(channels: &ChannelMap, id: &str) -> broadcast::Sender<String> {
    let mut guard = channels.lock().unwrap();
    if let Some(tx) = guard.get(id) {
        tx.clone()
    } else {
        let (tx, _) = broadcast::channel(128);
        guard.insert(id.to_string(), tx.clone());
        tx
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 1. Single Instance File Lock (G1)
    let lock_dir = dirs::runtime_dir()
        .or_else(dirs::data_local_dir)
        .unwrap_or_else(|| PathBuf::from("/tmp"))
        .join("noesis");
    let _ = std::fs::create_dir_all(&lock_dir);
    let lock_path = lock_dir.join("noesis.lock");

    let mut lock_file = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(&lock_path)?;

    if lock_file.try_lock_exclusive().is_err() {
        let mut existing_url = String::new();
        let _ = lock_file.read_to_string(&mut existing_url);
        let trimmed = existing_url.trim();
        if trimmed.starts_with("http://") || trimmed.starts_with("https://") {
            println!("Noesis 实例已在运行：{}", trimmed);
            let _ = Command::new("xdg-open").arg(trimmed).spawn();
        } else {
            eprintln!("Noesis 另一个实例正在运行中。");
        }
        return Ok(());
    }

    // 2. Initialize AppState
    let app_data = dirs::data_local_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("noesis");
    let _ = std::fs::create_dir_all(&app_data);
    let state = Arc::new(AppState::new(app_data));
    let channels: ChannelMap = Arc::new(Mutex::new(HashMap::new()));

    // 3. Bind TCP Listener
    let listener = match TcpListener::bind("127.0.0.1:54321").await {
        Ok(l) => l,
        Err(_) => TcpListener::bind("127.0.0.1:0").await?,
    };
    let addr = listener.local_addr()?;
    let url = format!("http://127.0.0.1:{}", addr.port());

    // Record URL in lock file
    let _ = lock_file.set_len(0);
    let _ = lock_file.seek(SeekFrom::Start(0));
    let _ = lock_file.write_all(url.as_bytes());
    let _ = lock_file.flush();

    println!("NOESIS v0.1.2 (Linux x86_64) running at {}", url);

    // 4. Heartbeat Lifecycle (G1)
    LAST_HEARTBEAT.store(chrono::Utc::now().timestamp(), Ordering::SeqCst);
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
            let last = LAST_HEARTBEAT.load(Ordering::SeqCst);
            let now = chrono::Utc::now().timestamp();
            if now - last > 30 {
                let active = ACTIVE_TASKS.load(Ordering::SeqCst);
                let indexing = IS_INDEXING.load(Ordering::SeqCst);
                if active == 0 && !indexing {
                    println!("Noesis 30秒无心跳且无后台活动任务，安全退出进程。");
                    std::process::exit(0);
                }
            }
        }
    });

    // 5. Open browser if not headless
    if std::env::var("NOESIS_HEADLESS").is_err() {
        let u = url.clone();
        tokio::spawn(async move {
            tokio::time::sleep(tokio::time::Duration::from_millis(150)).await;
            let _ = Command::new("xdg-open").arg(&u).spawn();
        });
    }

    // Keep lock_file alive for the lifetime of the process
    let _lock_file_keeper = lock_file;

    // 6. Request loop
    loop {
        if let Ok((stream, _)) = listener.accept().await {
            let st = Arc::clone(&state);
            let ch = Arc::clone(&channels);
            tokio::spawn(async move {
                handle_http_connection(stream, st, ch).await;
            });
        }
    }
}

async fn handle_http_connection(mut stream: TcpStream, state: Arc<AppState>, channels: ChannelMap) {
    let mut buffer = vec![0u8; 8192];
    let bytes_read = match stream.read(&mut buffer).await {
        Ok(n) if n > 0 => n,
        _ => return,
    };

    let req_str = String::from_utf8_lossy(&buffer[..bytes_read]);
    let mut lines = req_str.lines();
    let request_line = match lines.next() {
        Some(l) => l,
        None => return,
    };

    let parts: Vec<&str> = request_line.split_whitespace().collect();
    if parts.len() < 2 {
        return;
    }
    let method = parts[0];
    let path_with_query = parts[1];

    let mut path_parts = path_with_query.splitn(2, '?');
    let path = path_parts.next().unwrap_or("/");
    let query = path_parts.next().unwrap_or("");

    // 1. Heartbeat
    if path == "/__noesis_heartbeat__" {
        LAST_HEARTBEAT.store(chrono::Utc::now().timestamp(), Ordering::SeqCst);
        let resp = "HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\nAccess-Control-Allow-Origin: *\r\nContent-Length: 2\r\n\r\nok";
        let _ = stream.write_all(resp.as_bytes()).await;
        return;
    }

    // 2. SSE Event Stream
    if path == "/__noesis_events__" {
        LAST_HEARTBEAT.store(chrono::Utc::now().timestamp(), Ordering::SeqCst);
        let mut channel_id = "default".to_string();
        for param in query.split('&') {
            let mut pair = param.splitn(2, '=');
            if pair.next() == Some("channel") {
                if let Some(val) = pair.next() {
                    channel_id = val.to_string();
                }
            }
        }

        let tx = get_or_create_channel(&channels, &channel_id);
        let mut rx = tx.subscribe();

        let sse_headers = "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nCache-Control: no-cache\r\nConnection: keep-alive\r\nAccess-Control-Allow-Origin: *\r\n\r\n";
        if stream.write_all(sse_headers.as_bytes()).await.is_err() {
            return;
        }

        while let Ok(msg) = rx.recv().await {
            let sse_msg = format!("data: {}\n\n", msg);
            if stream.write_all(sse_msg.as_bytes()).await.is_err() {
                break;
            }
        }
        return;
    }

    // 3. IPC Command Dispatch
    if path == "/__noesis_ipc__" {
        LAST_HEARTBEAT.store(chrono::Utc::now().timestamp(), Ordering::SeqCst);
        let mut body = String::new();
        if let Some(pos) = req_str.find("\r\n\r\n") {
            body.push_str(&req_str[pos + 4..]);
        }

        // Read more if Content-Length exceeds what was read
        let mut content_length: usize = 0;
        for line in req_str.lines() {
            if line.to_lowercase().starts_with("content-length:") {
                if let Some(len_str) = line.split(':').nth(1) {
                    content_length = len_str.trim().parse().unwrap_or(0);
                }
            }
        }
        if body.len() < content_length {
            let mut remaining = vec![0u8; content_length - body.len()];
            if stream.read_exact(&mut remaining).await.is_ok() {
                body.push_str(&String::from_utf8_lossy(&remaining));
            }
        }

        let parsed: Result<Value, _> = serde_json::from_str(&body);
        let (cmd, args) = match parsed {
            Ok(val) => (
                val["cmd"].as_str().unwrap_or("").to_string(),
                val["args"].clone(),
            ),
            Err(e) => {
                let err_resp = json!({ "error": format!("Invalid JSON: {}", e) }).to_string();
                send_http_response(&mut stream, 400, "application/json", err_resp.as_bytes()).await;
                return;
            }
        };

        // Determine channel
        let mut channel_id = None;
        for param in query.split('&') {
            let mut pair = param.splitn(2, '=');
            if pair.next() == Some("channel") {
                if let Some(val) = pair.next() {
                    if !val.is_empty() {
                        channel_id = Some(val.to_string());
                    }
                }
            }
        }
        if channel_id.is_none() {
            if let Some(id) = args["onEvent"]["__tauri_channel_id__"].as_str() {
                channel_id = Some(id.to_string());
            } else if let Some(id) = args["on_event"]["__tauri_channel_id__"].as_str() {
                channel_id = Some(id.to_string());
            }
        }

        let sink: Option<Box<dyn EventSink>> = channel_id.map(|cid| {
            let tx = get_or_create_channel(&channels, &cid);
            Box::new(SseEventSink { tx }) as Box<dyn EventSink>
        });

        match dispatch(&cmd, &state, args, sink.as_deref()).await {
            Ok(result) => {
                let resp_body = json!({ "result": result }).to_string();
                send_http_response(&mut stream, 200, "application/json", resp_body.as_bytes()).await;
            }
            Err(err) => {
                let resp_body = json!({ "error": err }).to_string();
                send_http_response(&mut stream, 200, "application/json", resp_body.as_bytes()).await;
            }
        }
        return;
    }

    // 4. Static Asset Serving & Dev URL Proxy (G2)
    if let Ok(dev_url) = std::env::var("NOESIS_DEV_URL") {
        let trimmed_dev = dev_url.trim_end_matches('/');
        let target = format!("{}{}", trimmed_dev, path_with_query);
        if let Ok(resp) = reqwest_proxy(&target, method).await {
            let _ = stream.write_all(&resp).await;
            return;
        }
    }

    serve_static_asset(&mut stream, path).await;
}

async fn reqwest_proxy(target_url: &str, method: &str) -> Result<Vec<u8>, String> {
    let client = reqwest::Client::new();
    let resp = match method {
        "GET" => client.get(target_url).send().await.map_err(|e| e.to_string())?,
        "HEAD" => client.head(target_url).send().await.map_err(|e| e.to_string())?,
        _ => client.get(target_url).send().await.map_err(|e| e.to_string())?,
    };

    let status = resp.status();
    let headers = resp.headers().clone();
    let body = resp.bytes().await.map_err(|e| e.to_string())?;

    let mut raw = format!("HTTP/1.1 {} {}\r\n", status.as_u16(), status.canonical_reason().unwrap_or("OK"));
    for (k, v) in headers.iter() {
        raw.push_str(&format!("{}: {}\r\n", k.as_str(), v.to_str().unwrap_or("")));
    }
    raw.push_str("\r\n");

    let mut out = raw.into_bytes();
    out.extend_from_slice(&body);
    Ok(out)
}

async fn serve_static_asset(stream: &mut TcpStream, req_path: &str) {
    let rel_path = req_path.trim_start_matches('/');
    let target_rel = if rel_path.is_empty() { "index.html" } else { rel_path };

    // Search order:
    // 1. $EXE_DIR/../share/noesis/dist
    // 2. /usr/share/noesis/dist
    // 3. ./dist
    // 4. Embedded DIST_DIR

    let mut asset_bytes: Option<Vec<u8>> = None;
    let mut mime_type = get_mime_type(target_rel);

    let candidates = [
        std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().and_then(|p| p.parent()).map(|p| p.join("share/noesis/dist"))),
        Some(PathBuf::from("/usr/share/noesis/dist")),
        Some(PathBuf::from("./dist")),
    ];

    for cand_opt in candidates.iter() {
        if let Some(ref cand) = cand_opt {
            let file_path = cand.join(target_rel);
            if file_path.is_file() {
                if let Ok(bytes) = std::fs::read(&file_path) {
                    asset_bytes = Some(bytes);
                    break;
                }
            }
        }
    }

    if asset_bytes.is_none() {
        if let Some(file) = DIST_DIR.get_file(target_rel) {
            asset_bytes = Some(file.contents().to_vec());
        }
    }

    // SPA fallback: if not found, serve index.html
    if asset_bytes.is_none() {
        for cand_opt in candidates.iter() {
            if let Some(ref cand) = cand_opt {
                let file_path = cand.join("index.html");
                if file_path.is_file() {
                    if let Ok(bytes) = std::fs::read(&file_path) {
                        asset_bytes = Some(bytes);
                        mime_type = "text/html; charset=utf-8";
                        break;
                    }
                }
            }
        }
        if asset_bytes.is_none() {
            if let Some(file) = DIST_DIR.get_file("index.html") {
                asset_bytes = Some(file.contents().to_vec());
                mime_type = "text/html; charset=utf-8";
            }
        }
    }

    let bytes = match asset_bytes {
        Some(b) => b,
        None => {
            let not_found = b"404 Not Found";
            send_http_response(stream, 404, "text/plain", not_found).await;
            return;
        }
    };

    if mime_type.starts_with("text/html") {
        let html_str = String::from_utf8_lossy(&bytes);
        let injected = if html_str.contains("</head>") {
            html_str.replace("</head>", &format!("{}\n</head>", IPC_SHIM_SCRIPT))
        } else {
            format!("{}\n{}", IPC_SHIM_SCRIPT, html_str)
        };
        send_http_response(stream, 200, mime_type, injected.as_bytes()).await;
    } else {
        send_http_response(stream, 200, mime_type, &bytes).await;
    }
}

fn get_mime_type(path: &str) -> &'static str {
    if path.ends_with(".html") {
        "text/html; charset=utf-8"
    } else if path.ends_with(".js") || path.ends_with(".mjs") {
        "application/javascript; charset=utf-8"
    } else if path.ends_with(".css") {
        "text/css; charset=utf-8"
    } else if path.ends_with(".wasm") {
        "application/wasm"
    } else if path.ends_with(".svg") {
        "image/svg+xml"
    } else if path.ends_with(".png") {
        "image/png"
    } else if path.ends_with(".jpg") || path.ends_with(".jpeg") {
        "image/jpeg"
    } else if path.ends_with(".json") {
        "application/json"
    } else {
        "application/octet-stream"
    }
}

async fn send_http_response(stream: &mut TcpStream, status: u16, content_type: &str, body: &[u8]) {
    let status_line = match status {
        200 => "HTTP/1.1 200 OK",
        400 => "HTTP/1.1 400 Bad Request",
        404 => "HTTP/1.1 404 Not Found",
        _ => "HTTP/1.1 500 Internal Server Error",
    };
    let header = format!(
        "{}\r\nContent-Type: {}\r\nContent-Length: {}\r\nAccess-Control-Allow-Origin: *\r\nConnection: close\r\n\r\n",
        status_line,
        content_type,
        body.len()
    );
    let _ = stream.write_all(header.as_bytes()).await;
    let _ = stream.write_all(body).await;
}
