use std::process::{Command, Child};
use std::sync::{Mutex, Arc};
use std::sync::atomic::{AtomicU64, Ordering};
use std::fs;
use std::path::Path;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use tauri::{State, AppHandle, Manager, WebviewUrl, WebviewWindowBuilder, PhysicalPosition};
use std::time::{SystemTime, UNIX_EPOCH};

type DouyinItem = (String, String, String, String);

struct AppState {
    ffmpeg_process: Mutex<Option<Child>>,
    ffmpeg_generation: AtomicU64,
    douyin_feed: Arc<Mutex<Vec<DouyinItem>>>,
}

#[tauri::command]
async fn start_ffmpeg(url: String, state: State<'_, AppState>) -> Result<String, String> {
    let my_gen = state.ffmpeg_generation.fetch_add(1, Ordering::SeqCst) + 1;

    {
        let mut process_guard = state.ffmpeg_process.lock().unwrap();
        if let Some(mut child) = process_guard.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }

    let hls_dir = "/tmp/cooltv_hls";
    let _ = fs::create_dir_all(hls_dir);

    if let Ok(entries) = fs::read_dir(hls_dir) {
        for entry in entries.flatten() {
            let p = entry.path();
            if p.is_file() {
                let _ = fs::remove_file(p);
            }
        }
    }

    let session_id = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_millis();
    let m3u8_filename = format!("stream_{}.m3u8", session_id);
    let m3u8_path = format!("{}/{}", hls_dir, m3u8_filename);
    let ts_pattern = format!("{}/seg_{}_%04d.ts", hls_dir, session_id);

    let is_local_file = url.starts_with('/');
    let is_douyin = url.contains("douyin") || url.contains("byte") || url.contains("amemv");
    let mut args = vec!["-v".to_string(), "error".to_string()];

    if is_local_file {
        args.extend(vec![
            "-re".to_string(),
            "-fflags".to_string(), "+genpts+igndts+discardcorrupt".to_string(),
            "-analyzeduration".to_string(), "10M".to_string(),
            "-probesize".to_string(), "10M".to_string(),
            "-ss".to_string(), "2".to_string(),
            "-i".to_string(), url,
            "-vf".to_string(), "scale=trunc(iw/2)*2:trunc(ih/2)*2,format=yuv420p".to_string(),
            "-c:v".to_string(), "h264_videotoolbox".to_string(),
            "-b:v".to_string(), "8M".to_string(),
            "-c:a".to_string(), "aac".to_string(),
            "-b:a".to_string(), "192k".to_string(),
            "-ar".to_string(), "44100".to_string(),
            "-ac".to_string(), "2".to_string(),
            "-f".to_string(), "hls".to_string(),
            "-hls_time".to_string(), "2".to_string(),
            "-hls_list_size".to_string(), "0".to_string(),
            "-hls_segment_filename".to_string(), ts_pattern,
            m3u8_path.clone()
        ]);
    } else {
        if is_douyin {
            args.extend(vec![
                "-headers".to_string(),
                "Referer: https://www.douyin.com/\r\nUser-Agent: Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15\r\n".to_string(),
            ]);
        }
        args.extend(vec![
            "-reconnect".to_string(), "1".to_string(),
            "-reconnect_streamed".to_string(), "1".to_string(),
            "-reconnect_delay_max".to_string(), "2".to_string(),
            "-fflags".to_string(), "+genpts+discardcorrupt".to_string(),
            "-i".to_string(), url,
            "-vf".to_string(), "scale=trunc(iw/2)*2:trunc(ih/2)*2,format=yuv420p".to_string(),
            "-c:v".to_string(), "h264_videotoolbox".to_string(),
            "-realtime".to_string(), "1".to_string(),
            "-b:v".to_string(), "6M".to_string(),
            "-g".to_string(), "50".to_string(),
            "-c:a".to_string(), "aac".to_string(),
            "-b:a".to_string(), "128k".to_string(),
            "-ar".to_string(), "44100".to_string(),
            "-ac".to_string(), "2".to_string(),
            "-f".to_string(), "hls".to_string(),
            "-hls_time".to_string(), "2".to_string(),
            "-hls_list_size".to_string(), "8".to_string(),
            "-hls_flags".to_string(), "delete_segments".to_string(),
            "-hls_segment_filename".to_string(), ts_pattern,
            m3u8_path.clone()
        ]);
    }

    let child = Command::new("ffmpeg")
        .args(&args)
        .spawn()
        .map_err(|e| format!("FFmpeg 启动失败: {}", e))?;

    {
        let mut process_guard = state.ffmpeg_process.lock().unwrap();
        *process_guard = Some(child);
    }

    for _ in 0..80 {
        if state.ffmpeg_generation.load(Ordering::SeqCst) != my_gen {
            return Err("已切换新视频".into());
        }
        if let Ok(content) = fs::read_to_string(&m3u8_path) {
            if content.contains(".ts") {
                return Ok(format!("http://127.0.0.1:8765/{}", m3u8_filename));
            }
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }

    Err("转码等待超时".into())
}

#[tauri::command]
async fn scan_local_folder(path: String) -> Result<Vec<(String, String)>, String> {
    let root = Path::new(&path);
    if !root.exists() {
        return Err(format!("路径不存在: {}", path));
    }

    let valid_exts = [
        "mp4", "mkv", "mov", "avi", "ts", "m4v", "webm", "flv", "wmv",
        "rmvb", "rm", "mpg", "mpeg", "vob", "3gp", "asf", "divx", "f4v", "m2ts", "mts", "ogv", "dat"
    ];
    let mut results = Vec::new();

    fn visit_dirs(dir: &Path, exts: &[&str], out: &mut Vec<(String, String)>, depth: usize) {
        if depth > 4 { return; }
        if let Ok(entries) = fs::read_dir(dir) {
            for entry in entries.flatten() {
                let p = entry.path();
                if p.is_dir() {
                    visit_dirs(&p, exts, out, depth + 1);
                } else if let Some(ext) = p.extension().and_then(|e| e.to_str()) {
                    if exts.contains(&ext.to_lowercase().as_str()) {
                        if let Some(stem) = p.file_stem().and_then(|s| s.to_str()) {
                            if !stem.starts_with('.') {
                                out.push((stem.to_string(), p.to_string_lossy().to_string()));
                            }
                        }
                    }
                }
            }
        }
    }

    visit_dirs(root, &valid_exts, &mut results, 0);
    results.sort_by(|a, b| a.0.cmp(&b.0));
    Ok(results)
}

#[tauri::command]
async fn extract_video_thumb(path: String) -> Result<String, String> {
    let thumb_dir = "/tmp/cooltv_hls/thumbs";
    let _ = fs::create_dir_all(thumb_dir);

    let mut hasher = DefaultHasher::new();
    path.hash(&mut hasher);
    let hash_id = hasher.finish();
    let filename = format!("thumb_{}.jpg", hash_id);
    let full_out = format!("{}/{}", thumb_dir, filename);

    if !Path::new(&full_out).exists() {
        let status = Command::new("ffmpeg")
            .args(&["-y", "-v", "error", "-ss", "5", "-i", &path, "-frames:v", "1", "-vf", "scale=480:-1", "-q:v", "4", &full_out])
            .status();

        if status.is_err() || !Path::new(&full_out).exists() {
            let _ = Command::new("ffmpeg")
                .args(&["-y", "-v", "error", "-ss", "0", "-i", &path, "-frames:v", "1", "-vf", "scale=480:-1", "-q:v", "4", &full_out])
                .status();
        }
    }

    if Path::new(&full_out).exists() {
        Ok(format!("http://127.0.0.1:8765/thumbs/{}", filename))
    } else {
        Err("截帧失败".into())
    }
}

fn parse_and_store_payload(payload: &str, feed_arc: &Arc<Mutex<Vec<DouyinItem>>>) {
    let decoded_payload = url_decode(payload);
    let mut feed = feed_arc.lock().unwrap();
    for row_str in decoded_payload.split("@@@") {
        let parts: Vec<&str> = row_str.split("|||").collect();
        if parts.len() == 4 {
            let title = url_decode(parts[0]);
            let author = url_decode(parts[1]);
            let cover = url_decode(parts[2]);
            let play_url = url_decode(parts[3]);
            if !play_url.is_empty() && !feed.iter().any(|x| x.0 == title) {
                feed.push((title, author, cover, play_url));
            }
        }
    }
}

#[tauri::command]
async fn get_douyin_feed(app: AppHandle, refresh: bool, state: State<'_, AppState>) -> Result<Vec<DouyinItem>, String> {
    if app.get_webview_window("douyin_scraper").is_none() {
        let hook_script = r#"
            (function() {
                // 1. 突破抖音源码中的 window.innerWidth >= 1536 限制与后台休眠检测
                try {
                    Object.defineProperty(window, 'innerWidth', { get: () => 1920, configurable: true });
                    Object.defineProperty(window, 'outerWidth', { get: () => 1920, configurable: true });
                    Object.defineProperty(document, 'visibilityState', { get: () => 'visible', configurable: true });
                    Object.defineProperty(document, 'hidden', { get: () => false, configurable: true });
                } catch(e) {}

                const collected = [];
                const seenTitles = new Set();
                let lastSentCount = 0;

                function flushToRust() {
                    if (collected.length === 0 || collected.length === lastSentCount) return;
                    lastSentCount = collected.length;
                    const batch = collected.slice(0, 25).map(item =>
                        item.map(s => encodeURIComponent(s)).join('|||')
                    ).join('@@@');
                    try {
                        // 通道 A：触发 Tauri 底层 on_navigation 拦截器（会被 Rust 捕获并取消跳转，不刷新页面）
                        window.location.href = 'https://fc-bridge.local/push?d=' + encodeURIComponent(batch);
                    } catch(e) {}
                }

                function recordAweme(aweme) {
                    if (!aweme || typeof aweme !== 'object' || !aweme.video) return;
                    const v = aweme.video;
                    const desc = String(aweme.desc || aweme.title || '抖音热门视频').trim();
                    if (!desc || seenTitles.has(desc)) return;

                    const author = (aweme.author && aweme.author.nickname) ? ('@' + aweme.author.nickname) : '@抖音创作者';
                    let cover = '';
                    if (v.cover && Array.isArray(v.cover.url_list) && v.cover.url_list[0]) cover = v.cover.url_list[0];
                    else if (v.origin_cover && Array.isArray(v.origin_cover.url_list) && v.origin_cover.url_list[0]) cover = v.origin_cover.url_list[0];
                    else if (v.dynamic_cover && Array.isArray(v.dynamic_cover.url_list) && v.dynamic_cover.url_list[0]) cover = v.dynamic_cover.url_list[0];

                    let playUrl = '';
                    if (v.play_addr && Array.isArray(v.play_addr.url_list) && v.play_addr.url_list.length > 0) {
                        playUrl = v.play_addr.url_list.find(u => u.includes('douyinvod.com')) || v.play_addr.url_list[0];
                    } else if (Array.isArray(v.bit_rate) && v.bit_rate[0] && v.bit_rate[0].play_addr && Array.isArray(v.bit_rate[0].play_addr.url_list)) {
                        const list = v.bit_rate[0].play_addr.url_list;
                        playUrl = list.find(u => u.includes('douyinvod.com')) || list[0] || '';
                    }

                    if (playUrl && cover) {
                        if (playUrl.startsWith('http://')) playUrl = 'https://' + playUrl.substring(7);
                        if (cover.startsWith('http://')) cover = 'https://' + cover.substring(7);
                        seenTitles.add(desc);
                        collected.push([desc, author, cover, playUrl]);
                        flushToRust();
                    }
                }

                // 2. 带 WeakSet 防死循环的深度扫描器（支持扫描深达 20 层的 JSON 与 React Fiber 树）
                function deepScan(rootObj) {
                    if (!rootObj || typeof rootObj !== 'object') return;
                    const visited = new WeakSet();
                    function walk(obj, depth) {
                        if (!obj || typeof obj !== 'object' || depth > 20 || visited.has(obj)) return;
                        visited.add(obj);
                        if (obj.video && (obj.video.play_addr || obj.video.bit_rate)) {
                            recordAweme(obj);
                            return;
                        }
                        if (Array.isArray(obj)) {
                            for (let i = 0; i < obj.length; i++) walk(obj[i], depth + 1);
                        } else {
                            for (const k in obj) {
                                if (k !== 'window' && k !== 'document' && k !== 'stateNode' && Object.prototype.hasOwnProperty.call(obj, k)) {
                                    try { walk(obj[k], depth + 1); } catch(e) {}
                                }
                            }
                        }
                    }
                    walk(rootObj, 0);
                }

                // 3. 无痕劫持 JSON.parse（伪装 [native code]，彻底绕过 webmssdk / bdms.js 反爬校验）
                const origParse = JSON.parse;
                const hookedParse = function(text, reviver) {
                    const res = origParse.call(this, text, reviver);
                    if (res && typeof res === 'object') {
                        setTimeout(() => deepScan(res), 0);
                    }
                    return res;
                };
                hookedParse.toString = () => 'function parse() { [native code] }';
                Object.defineProperty(JSON, 'parse', { value: hookedParse, configurable: true, writable: true });

                // 4. 无痕劫持 Response.prototype.json
                const origJson = Response.prototype.json;
                const hookedJson = function() {
                    return origJson.apply(this, arguments).then(data => {
                        setTimeout(() => deepScan(data), 0);
                        return data;
                    });
                };
                hookedJson.toString = () => 'function json() { [native code] }';
                Object.defineProperty(Response.prototype, 'json', { value: hookedJson, configurable: true, writable: true });

                // 5. 定时唤醒延迟脚本、扫描 React Fiber 节点与调用抖音源码内置的 jingxuan_pre_build_util
                const fetchedIds = new Set();
                setInterval(() => {
                    document.querySelectorAll('video, audio').forEach(el => { el.muted = true; el.volume = 0; });

                    if (window.__pc_pace_load_deferred_scripts) {
                        try { window.__pc_pace_load_deferred_scripts(); } catch(e) {}
                    } else if (window.__pace_load_deferred_scripts) {
                        try { window.__pace_load_deferred_scripts(); } catch(e) {}
                    }

                    // 扫描 DOM 上的 React Fiber 属性树
                    const domNodes = document.querySelectorAll('[data-aweme-id], [data-e2e="feed-video"], .discover-video-card-item');
                    domNodes.forEach(node => {
                        for (const k in node) {
                            if (k.startsWith('__reactFiber$') || k.startsWith('__reactProps$')) {
                                deepScan(node[k]);
                            }
                        }
                        const id = node.getAttribute('data-aweme-id');
                        if (id && !fetchedIds.has(id) && window.jingxuan_pre_build_util && window.jingxuan_pre_build_util.fetchAwemeInfo) {
                            fetchedIds.add(id);
                            window.jingxuan_pre_build_util.fetchAwemeInfo({ awemeId: id, originType: 'quick_player' })
                                .then(res => { if (res && res.detail) recordAweme(res.detail); })
                                .catch(() => {});
                        }
                    });
                }, 1000);
            })();
        "#;

        let feed_for_nav = state.douyin_feed.clone();
        let parsed_url: tauri::Url = "https://www.douyin.com/jingxuan".parse().unwrap();

        // 窗口放在屏幕内主窗口后方，避免 macOS 因屏幕外坐标冻结 WebKit 渲染
        if let Ok(win) = WebviewWindowBuilder::new(&app, "douyin_scraper", WebviewUrl::External(parsed_url))
            .title("Douyin Scraper")
            .inner_size(480.0, 320.0)
            .position(80.0, 80.0)
            .decorations(false)
            .focused(false)
            .visible(true)
            .initialization_script(hook_script)
            .on_navigation(move |url| {
                if url.host_str() == Some("fc-bridge.local") {
                    if let Some(query) = url.query() {
                        if let Some(payload) = query.strip_prefix("d=") {
                            parse_and_store_payload(payload, &feed_for_nav);
                        }
                    }
                    // 返回 false 取消网页跳转，让抖音页面继续留在原地工作
                    return false;
                }
                true
            })
            .build()
        {
            if let Some(main_win) = app.get_webview_window("main") {
                if let Ok(pos) = main_win.inner_position() {
                    let _ = win.set_position(PhysicalPosition::new(pos.x + 120, pos.y + 120));
                }
                let _ = main_win.set_focus();
            }
        }
    } else if refresh {
        state.douyin_feed.lock().unwrap().clear();
        if let Some(win) = app.get_webview_window("douyin_scraper") {
            let _ = win.eval("window.location.reload();");
        }
        if let Some(main_win) = app.get_webview_window("main") {
            let _ = main_win.set_focus();
        }
    }

    let feed = state.douyin_feed.lock().unwrap().clone();
    Ok(feed)
}

fn url_decode(s: &str) -> String {
    let mut bytes = Vec::new();
    let mut chars = s.as_bytes().iter().copied();
    while let Some(b) = chars.next() {
        if b == b'%' {
            let h1 = chars.next().unwrap_or(b'0');
            let h2 = chars.next().unwrap_or(b'0');
            if let Ok(val) = u8::from_str_radix(&String::from_utf8_lossy(&[h1, h2]), 16) {
                bytes.push(val);
            }
        } else if b == b'+' {
            bytes.push(b' ');
        } else {
            bytes.push(b);
        }
    }
    String::from_utf8_lossy(&bytes).to_string()
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let shared_douyin_feed: Arc<Mutex<Vec<DouyinItem>>> = Arc::new(Mutex::new(Vec::new()));

    std::thread::spawn(move || {
        let _ = fs::create_dir_all("/tmp/cooltv_hls/thumbs");
        rouille::start_server("127.0.0.1:8765", move |request| {
            let mut response = rouille::match_assets(&request, "/tmp/cooltv_hls");
            if response.is_success() {
                response.headers.retain(|(k, _)| k.as_ref().to_lowercase() != "cache-control");
                response.headers.push(("Cache-Control".into(), "no-cache, no-store, must-revalidate".into()));
                response.headers.push(("Pragma".into(), "no-cache".into()));
                response.headers.push(("Expires".into(), "0".into()));
                response.headers.push(("Access-Control-Allow-Origin".into(), "*".into()));
                return response;
            }
            rouille::Response::text("Not Found")
                .with_status_code(404)
                .with_additional_header("Access-Control-Allow-Origin", "*")
        });
    });

    tauri::Builder::default()
        .manage(AppState {
            ffmpeg_process: Mutex::new(None),
            ffmpeg_generation: AtomicU64::new(0),
            douyin_feed: shared_douyin_feed,
        })
        .invoke_handler(tauri::generate_handler![
            start_ffmpeg,
            scan_local_folder,
            extract_video_thumb,
            get_douyin_feed
        ])
        .run(tauri::generate_context!())
        .expect("运行 Tauri 应用失败");
}