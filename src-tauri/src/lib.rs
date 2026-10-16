use std::process::{Command, Child};
use std::sync::{Mutex, Arc};
use std::sync::atomic::{AtomicU64, Ordering};
use std::fs;
use std::path::Path;
use std::collections::{HashMap, HashSet};
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use tauri::{State, AppHandle, Manager, WebviewUrl, WebviewWindowBuilder, TitleBarStyle, WindowEvent};
use std::time::{SystemTime, UNIX_EPOCH};

type DouyinItem = (String, String, String, String);

struct AppState {
    ffmpeg_process: Arc<Mutex<Option<(u64, Child)>>>,
    ffmpeg_op_lock: Arc<Mutex<()>>,
    ffmpeg_generation: Arc<AtomicU64>,
    douyin_feed: Arc<Mutex<Vec<DouyinItem>>>,
    douyin_probing: Arc<Mutex<HashSet<String>>>,
    douyin_category: Arc<Mutex<String>>,
    thumb_lock: Arc<Mutex<()>>,
    minutecast_400_until: Arc<AtomicU64>,
    was_precipitating: Arc<Mutex<bool>>,
}

fn get_ffmpeg_bin() -> &'static str {
    if Path::new("/opt/homebrew/bin/ffmpeg").exists() {
        "/opt/homebrew/bin/ffmpeg"
    } else if Path::new("/usr/local/bin/ffmpeg").exists() {
        "/usr/local/bin/ffmpeg"
    } else {
        "ffmpeg"
    }
}

fn get_ffprobe_bin() -> &'static str {
    if Path::new("/opt/homebrew/bin/ffprobe").exists() {
        "/opt/homebrew/bin/ffprobe"
    } else if Path::new("/usr/local/bin/ffprobe").exists() {
        "/usr/local/bin/ffprobe"
    } else {
        "ffprobe"
    }
}

fn cleanup_orphan_hls_ffmpeg() {
    let _ = Command::new("/usr/bin/pkill")
        .args(&["-9", "-f", "/tmp/cooltv_hls/seg_"])
        .status();
}

#[tauri::command]
async fn start_drag(app: AppHandle) -> Result<(), String> {
    if let Some(win) = app.get_webview_window("main") {
        let _ = win.start_dragging();
    }
    Ok(())
}

fn build_ffmpeg_args(
    actual_url: &str,
    is_local_file: bool,
    is_douyin: bool,
    can_stream_copy_h264: bool,
    use_sw_fallback: bool,
    seek_sec: Option<f64>,
    ts_pattern: &str,
    m3u8_path: &str,
) -> Vec<String> {
    let mut args = vec!["-v".to_string(), "error".to_string()];
    let v_codec = if use_sw_fallback { "libx264" } else { "h264_videotoolbox" };
    // 任务 1 (Round 5)：全链路音量均衡与峰值硬限幅滤镜，严控所有视频（尤其高响度抖音视频）不超过上限
    let audio_norm_std = "acompressor=threshold=-18dB:ratio=3.5:attack=5:release=85:makeup=1,alimiter=limit=0.75:attack=5:release=50";
    let audio_norm_douyin = "acompressor=threshold=-20dB:ratio=5:attack=4:release=75:makeup=1,alimiter=limit=0.55:attack=4:release=45";
    let ss_val = match seek_sec {
        Some(s) if s >= 0.0 => format!("{:.2}", s),
        _ => "2".to_string(),
    };

    if is_local_file {
        args.extend(vec![
            "-re".to_string(),
            "-fflags".to_string(), "+genpts+igndts+discardcorrupt".to_string(),
            "-analyzeduration".to_string(), "10M".to_string(),
            "-probesize".to_string(), "10M".to_string(),
            "-ss".to_string(), ss_val,
            "-i".to_string(), actual_url.to_string(),
            "-map".to_string(), "0:v:0".to_string(),
            "-map".to_string(), "0:a:0?".to_string(),
            "-vf".to_string(), "scale=trunc(iw/2)*2:trunc(ih/2)*2,format=yuv420p".to_string(),
            "-c:v".to_string(), v_codec.to_string(),
        ]);
        if use_sw_fallback {
            args.extend(vec!["-preset".to_string(), "ultrafast".to_string()]);
        } else {
            args.extend(vec![
                "-realtime".to_string(), "1".to_string(),
                "-allow_sw".to_string(), "1".to_string(),
            ]);
        }
        args.extend(vec![
            "-b:v".to_string(), "8M".to_string(),
            "-af".to_string(), audio_norm_std.to_string(),
            "-c:a".to_string(), "aac".to_string(),
            "-b:a".to_string(), "192k".to_string(),
            "-ar".to_string(), "44100".to_string(),
            "-ac".to_string(), "2".to_string(),
            "-f".to_string(), "hls".to_string(),
            "-hls_time".to_string(), "2".to_string(),
            "-hls_list_size".to_string(), "0".to_string(),
            "-hls_segment_filename".to_string(), ts_pattern.to_string(),
            m3u8_path.to_string()
        ]);
    } else if is_douyin {
        let ua = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/17.0 Safari/605.1.15";
        let header_lines = "Referer: https://www.douyin.com/\r\nOrigin: https://www.douyin.com\r\n";
        args.extend(vec![
            "-user_agent".to_string(), ua.to_string(),
            "-headers".to_string(), header_lines.to_string(),
            "-reconnect".to_string(), "1".to_string(),
            "-reconnect_streamed".to_string(), "1".to_string(),
            "-reconnect_delay_max".to_string(), "2".to_string(),
            "-rw_timeout".to_string(), "8000000".to_string(),
            "-fflags".to_string(), "+genpts+igndts+discardcorrupt".to_string(),
            "-i".to_string(), actual_url.to_string(),
            "-map".to_string(), "0:v:0".to_string(),
            "-map".to_string(), "0:a:0?".to_string(),
        ]);
        if can_stream_copy_h264 {
            args.extend(vec![
                "-c:v".to_string(), "copy".to_string(),
                "-af".to_string(), audio_norm_douyin.to_string(),
                "-c:a".to_string(), "aac".to_string(),
                "-b:a".to_string(), "160k".to_string(),
                "-ar".to_string(), "44100".to_string(),
                "-ac".to_string(), "2".to_string(),
                "-f".to_string(), "hls".to_string(),
                "-hls_time".to_string(), "2".to_string(),
                "-hls_list_size".to_string(), "0".to_string(),
                "-hls_segment_filename".to_string(), ts_pattern.to_string(),
                m3u8_path.to_string()
            ]);
        } else {
            // 任务 10 (Round 5)：大屏连接下抖音高帧率流畅度优化（保持原始高帧率 passthrough + 10M 高码率硬件加速）
            args.extend(vec![
                "-fps_mode".to_string(), "passthrough".to_string(),
                "-vf".to_string(), "scale=trunc(iw/2)*2:trunc(ih/2)*2,format=yuv420p".to_string(),
                "-c:v".to_string(), v_codec.to_string(),
            ]);
            if use_sw_fallback {
                args.extend(vec!["-preset".to_string(), "ultrafast".to_string()]);
            } else {
                args.extend(vec![
                    "-realtime".to_string(), "1".to_string(),
                    "-allow_sw".to_string(), "1".to_string(),
                ]);
            }
            args.extend(vec![
                "-b:v".to_string(), "10M".to_string(),
                "-maxrate".to_string(), "14M".to_string(),
                "-bufsize".to_string(), "20M".to_string(),
                "-af".to_string(), audio_norm_douyin.to_string(),
                "-c:a".to_string(), "aac".to_string(),
                "-b:a".to_string(), "160k".to_string(),
                "-ar".to_string(), "44100".to_string(),
                "-ac".to_string(), "2".to_string(),
                "-f".to_string(), "hls".to_string(),
                "-hls_time".to_string(), "2".to_string(),
                "-hls_list_size".to_string(), "0".to_string(),
                "-hls_segment_filename".to_string(), ts_pattern.to_string(),
                m3u8_path.to_string()
            ]);
        }
    } else {
        args.extend(vec![
            "-reconnect".to_string(), "1".to_string(),
            "-reconnect_streamed".to_string(), "1".to_string(),
            "-reconnect_delay_max".to_string(), "2".to_string(),
            "-rw_timeout".to_string(), "8000000".to_string(),
            "-fflags".to_string(), "+genpts+discardcorrupt".to_string(),
            "-i".to_string(), actual_url.to_string(),
            "-vf".to_string(), "scale=trunc(iw/2)*2:trunc(ih/2)*2,format=yuv420p".to_string(),
            "-c:v".to_string(), v_codec.to_string(),
        ]);
        if use_sw_fallback {
            args.extend(vec!["-preset".to_string(), "ultrafast".to_string()]);
        } else {
            args.extend(vec![
                "-realtime".to_string(), "1".to_string(),
                "-allow_sw".to_string(), "1".to_string(),
            ]);
        }
        args.extend(vec![
            "-b:v".to_string(), "6M".to_string(),
            "-g".to_string(), "50".to_string(),
            "-af".to_string(), audio_norm_std.to_string(),
            "-c:a".to_string(), "aac".to_string(),
            "-b:a".to_string(), "128k".to_string(),
            "-ar".to_string(), "44100".to_string(),
            "-ac".to_string(), "2".to_string(),
            "-f".to_string(), "hls".to_string(),
            "-hls_time".to_string(), "2".to_string(),
            "-hls_list_size".to_string(), "8".to_string(),
            "-hls_flags".to_string(), "delete_segments".to_string(),
            "-hls_segment_filename".to_string(), ts_pattern.to_string(),
            m3u8_path.to_string()
        ]);
    }

    args
}

fn probe_douyin_candidates(raw_candidates: &str) -> Option<(String, String)> {
    let ffprobe_bin = get_ffprobe_bin();
    let ua = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/17.0 Safari/605.1.15";
    let headers = "Referer: https://www.douyin.com/\r\nOrigin: https://www.douyin.com\r\n";

    for cand in raw_candidates.split("@@") {
        let u = cand.trim();
        if !u.starts_with("https://") {
            continue;
        }
        if let Ok(out) = Command::new(ffprobe_bin)
            .args(&[
                "-v", "error",
                "-user_agent", ua,
                "-headers", headers,
                "-rw_timeout", "4500000",
                "-analyzeduration", "1500000",
                "-probesize", "1500000",
                "-select_streams", "v:0",
                "-show_entries", "stream=codec_name",
                "-of", "default=noprint_wrappers=1:nokey=1",
                u,
            ])
            .output()
        {
            if out.status.success() {
                let codec = String::from_utf8_lossy(&out.stdout).trim().to_lowercase();
                if !codec.is_empty() && codec != "none" && !codec.contains("bvc2") && !codec.contains("bytevc2") {
                    return Some((u.to_string(), codec));
                }
            }
        }
    }
    None
}

#[tauri::command]
async fn get_media_duration(url: String) -> Result<f64, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let raw_url = if let Some(s) = url.strip_prefix("douyin:h264:") {
            s.split("@@").next().unwrap_or(s).to_string()
        } else if let Some(s) = url.strip_prefix("douyin:") {
            s.split("@@").next().unwrap_or(s).to_string()
        } else {
            url
        };
        let ffprobe_bin = get_ffprobe_bin();
        let mut cmd = Command::new(ffprobe_bin);
        cmd.args(&["-v", "error"]);
        if raw_url.starts_with("http") {
            let ua = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/17.0 Safari/605.1.15";
            let headers = "Referer: https://www.douyin.com/\r\nOrigin: https://www.douyin.com\r\n";
            cmd.args(&["-user_agent", ua, "-headers", headers, "-rw_timeout", "5000000"]);
        }
        cmd.args(&[
            "-show_entries", "format=duration",
            "-of", "default=noprint_wrappers=1:nokey=1",
            &raw_url,
        ]);
        if let Ok(out) = cmd.output() {
            if out.status.success() {
                let txt = String::from_utf8_lossy(&out.stdout).trim().to_string();
                if let Ok(dur) = txt.parse::<f64>() {
                    if dur.is_finite() && dur > 0.0 {
                        return Ok(dur);
                    }
                }
            }
        }
        Err("无法获取视频时长".into())
    })
    .await
    .map_err(|e| format!("查询时长异常: {}", e))?
}

#[tauri::command]
async fn start_ffmpeg(url: String, seek_sec: Option<f64>, state: State<'_, AppState>) -> Result<String, String> {
    let my_gen = state.ffmpeg_generation.fetch_add(1, Ordering::SeqCst) + 1;
    let proc_arc = state.ffmpeg_process.clone();
    let op_lock_arc = state.ffmpeg_op_lock.clone();
    let gen_arc = state.ffmpeg_generation.clone();

    tauri::async_runtime::spawn_blocking(move || {
        let hls_dir = "/tmp/cooltv_hls";
        let _ = fs::create_dir_all(hls_dir);

        {
            let _op_guard = op_lock_arc.lock().unwrap();
            if gen_arc.load(Ordering::SeqCst) != my_gen {
                return Err("已切换新视频".into());
            }
            {
                let mut process_guard = proc_arc.lock().unwrap();
                if let Some((_, mut child)) = process_guard.take() {
                    let _ = child.kill();
                    let _ = child.wait();
                }
            }
            if let Ok(entries) = fs::read_dir(hls_dir) {
                for entry in entries.flatten() {
                    let p = entry.path();
                    if p.is_file() {
                        let _ = fs::remove_file(p);
                    }
                }
            }
        }

        let (raw_url, is_douyin, is_verified_h264) = if let Some(stripped) = url.strip_prefix("douyin:h264:") {
            (stripped.to_string(), true, true)
        } else if let Some(stripped) = url.strip_prefix("douyin:") {
            (stripped.to_string(), true, false)
        } else {
            let dy = url.contains("douyin") || url.contains("byte") || url.contains("amemv") || url.contains("zjcdn") || url.contains("snssdk");
            (url, dy, false)
        };

        let candidates: Vec<String> = if is_douyin {
            raw_url.split("@@").filter(|s| !s.trim().is_empty()).map(|s| s.trim().to_string()).collect()
        } else {
            vec![raw_url]
        };

        if candidates.is_empty() {
            return Err("无效的视频地址".into());
        }

        let ffmpeg_bin = get_ffmpeg_bin();

        let mut attempts: Vec<(String, bool, bool)> = Vec::new();
        for cand in &candidates {
            if is_douyin && is_verified_h264 {
                attempts.push((cand.clone(), true, false));
            }
            attempts.push((cand.clone(), false, false));
            if is_douyin {
                attempts.push((cand.clone(), false, true));
            }
        }

        for (attempt_idx, (cand_url, copy_h264, use_sw)) in attempts.iter().enumerate() {
            if gen_arc.load(Ordering::SeqCst) != my_gen {
                return Err("已切换新视频".into());
            }

            let session_id = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_millis();
            let m3u8_filename = format!("stream_{}_{}_{}.m3u8", session_id, my_gen, attempt_idx);
            let m3u8_path = format!("{}/{}", hls_dir, m3u8_filename);
            let ts_pattern = format!("{}/seg_{}_{}_{}_%04d.ts", hls_dir, session_id, my_gen, attempt_idx);
            let is_local_file = cand_url.starts_with('/');

            let args = build_ffmpeg_args(
                cand_url,
                is_local_file,
                is_douyin,
                *copy_h264,
                *use_sw,
                seek_sec,
                &ts_pattern,
                &m3u8_path,
            );

            let spawn_res = {
                let _op_guard = op_lock_arc.lock().unwrap();
                if gen_arc.load(Ordering::SeqCst) != my_gen {
                    return Err("已切换新视频".into());
                }
                let mut process_guard = proc_arc.lock().unwrap();
                if let Some((_, mut old_c)) = process_guard.take() {
                    let _ = old_c.kill();
                    let _ = old_c.wait();
                }
                match Command::new(ffmpeg_bin).args(&args).spawn() {
                    Ok(c) => {
                        *process_guard = Some((my_gen, c));
                        Ok(())
                    }
                    Err(e) => Err(e),
                }
            };

            if let Err(e) = spawn_res {
                if attempt_idx + 1 == attempts.len() {
                    return Err(format!("FFmpeg 启动失败: {}", e));
                }
                continue;
            }

            let max_ticks = if is_douyin && attempt_idx + 1 < attempts.len() { 38 } else { 78 };
            let mut exited_early = false;

            for _ in 0..max_ticks {
                if gen_arc.load(Ordering::SeqCst) != my_gen {
                    return Err("已切换新视频".into());
                }
                if let Ok(content) = fs::read_to_string(&m3u8_path) {
                    if content.contains(".ts") {
                        return Ok(format!("http://127.0.0.1:8765/{}", m3u8_filename));
                    }
                }
                {
                    let mut process_guard = proc_arc.lock().unwrap();
                    if let Some((g, c)) = process_guard.as_mut() {
                        if *g == my_gen {
                            if let Ok(Some(_status)) = c.try_wait() {
                                if let Ok(content) = fs::read_to_string(&m3u8_path) {
                                    if content.contains(".ts") {
                                        return Ok(format!("http://127.0.0.1:8765/{}", m3u8_filename));
                                    }
                                }
                                exited_early = true;
                            }
                        } else {
                            return Err("已切换新视频".into());
                        }
                    }
                }
                if exited_early {
                    break;
                }
                std::thread::sleep(std::time::Duration::from_millis(85));
            }

            {
                let mut process_guard = proc_arc.lock().unwrap();
                if let Some((g, _)) = process_guard.as_ref() {
                    if *g == my_gen {
                        if let Some((_, mut c)) = process_guard.take() {
                            let _ = c.kill();
                            let _ = c.wait();
                        }
                    }
                }
            }
        }

        Err("转码等待超时".into())
    })
    .await
    .map_err(|e| format!("任务执行异常: {}", e))?
}

#[tauri::command]
async fn scan_local_folder(path: String) -> Result<Vec<(String, String)>, String> {
    tauri::async_runtime::spawn_blocking(move || {
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
    })
    .await
    .map_err(|e| format!("扫描异常: {}", e))?
}

#[tauri::command]
async fn extract_video_thumb(path: String, state: State<'_, AppState>) -> Result<String, String> {
    let thumb_dir = "/tmp/cooltv_hls/thumbs";
    let _ = fs::create_dir_all(thumb_dir);

    let mut hasher = DefaultHasher::new();
    path.hash(&mut hasher);
    let hash_id = hasher.finish();
    let filename = format!("thumb_{}.jpg", hash_id);
    let full_out = format!("{}/{}", thumb_dir, filename);

    let is_valid_jpg = |p: &str| -> bool {
        fs::metadata(p).map(|m| m.len() > 256).unwrap_or(false)
    };

    if is_valid_jpg(&full_out) {
        return Ok(format!("http://127.0.0.1:8765/thumbs/{}", filename));
    }

    let lock_arc = state.thumb_lock.clone();
    let full_out_clone = full_out.clone();

    tauri::async_runtime::spawn_blocking(move || {
        let _guard = lock_arc.lock().unwrap();
        if !is_valid_jpg(&full_out_clone) {
            let _ = fs::remove_file(&full_out_clone);
            let ffmpeg_bin = get_ffmpeg_bin();
            for seek_sec in &["5", "1", "0"] {
                let _ = Command::new(ffmpeg_bin)
                    .args(&[
                        "-y",
                        "-v", "error",
                        "-fflags", "+genpts+igndts+discardcorrupt",
                        "-threads", "2",
                        "-ss", seek_sec,
                        "-i", &path,
                        "-frames:v", "1",
                        "-vf", "scale=480:-2",
                        "-q:v", "4",
                        &full_out_clone,
                    ])
                    .status();
                if is_valid_jpg(&full_out_clone) {
                    break;
                }
                let _ = fs::remove_file(&full_out_clone);
            }
        }
        if is_valid_jpg(&full_out_clone) {
            Ok(format!("http://127.0.0.1:8765/thumbs/{}", filename))
        } else {
            Err("截帧失败".into())
        }
    })
    .await
    .map_err(|e| format!("截帧异常: {}", e))?
}

fn extract_first_rating(text: &str) -> Option<String> {
    let marker = "rating_nums\">";
    if let Some(pos) = text.find(marker) {
        let rest = &text[pos + marker.len()..];
        if let Some(end) = rest.find('<') {
            let score = rest[..end].trim();
            if let Ok(val) = score.parse::<f32>() {
                if val > 0.0 && val <= 10.0 {
                    return Some(format!("{:.1}", val));
                }
            }
        }
    }
    let mut search_slice = text;
    while let Some(pos) = search_slice.find("\"value\":") {
        let after = &search_slice[pos + 8..];
        let num_str: String = after
            .chars()
            .skip_while(|c| c.is_whitespace())
            .take_while(|c| c.is_ascii_digit() || *c == '.')
            .collect();
        if let Ok(val) = num_str.parse::<f32>() {
            if val > 0.0 && val <= 10.0 {
                return Some(format!("{:.1}", val));
            }
        }
        search_slice = after;
    }
    None
}

fn url_encode_simple(s: &str) -> String {
    let mut out = String::new();
    for b in s.as_bytes() {
        match *b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => out.push(*b as char),
            _ => out.push_str(&format!("%{:02X}", b)),
        }
    }
    out
}

#[tauri::command]
async fn fetch_douban_rating(title: String) -> Result<String, String> {
    let clean = title.trim().to_string();
    if clean.is_empty() {
        return Err("标题为空".into());
    }
    tauri::async_runtime::spawn_blocking(move || {
        let encoded = url_encode_simple(&clean);
        let ua = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/17.0 Safari/605.1.15";

        let urls = [
            (
                format!("https://www.douban.com/j/search?q={}&cat=1002", encoded),
                "https://www.douban.com/search",
            ),
            (
                format!("https://m.douban.com/rexxar/api/v2/search?q={}&type=movie", encoded),
                "https://m.douban.com/movie/",
            ),
            (
                format!("https://www.douban.com/search?cat=1002&q={}", encoded),
                "https://www.douban.com/",
            ),
        ];

        for (u, referer) in &urls {
            if let Ok(output) = Command::new("/usr/bin/curl")
                .args(&[
                    "-sL",
                    "--noproxy",
                    "*",
                    "--compressed",
                    "--max-time",
                    "6",
                    "-H",
                    &format!("User-Agent: {}", ua),
                    "-H",
                    &format!("Referer: {}", referer),
                    u,
                ])
                .output()
            {
                if output.status.success() {
                    let body = String::from_utf8_lossy(&output.stdout);
                    if let Some(score) = extract_first_rating(&body) {
                        return Ok(score);
                    }
                }
            }
        }
        Err("未找到豆瓣评分".into())
    })
    .await
    .map_err(|e| format!("查询异常: {}", e))?
}

fn parse_xmltv_timestamp(s: &str) -> Option<i64> {
    let trimmed = s.trim();
    if trimmed.len() < 14 {
        return None;
    }
    let digits = &trimmed[0..14];
    let year: i64 = digits[0..4].parse().ok()?;
    let month: i64 = digits[4..6].parse().ok()?;
    let day: i64 = digits[6..8].parse().ok()?;
    let hour: i64 = digits[8..10].parse().ok()?;
    let min: i64 = digits[10..12].parse().ok()?;
    let sec: i64 = digits[12..14].parse().ok()?;

    fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
        let y = if m <= 2 { y - 1 } else { y };
        let era = if y >= 0 { y } else { y - 399 } / 400;
        let yoe = y - era * 400;
        let m_adj = if m > 2 { m - 3 } else { m + 9 };
        let doy = (153 * m_adj + 2) / 5 + d - 1;
        let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
        era * 146097 + doe - 719468
    }

    let days = days_from_civil(year, month, day);
    let mut epoch = days * 86400 + hour * 3600 + min * 60 + sec;

    let rest = trimmed[14..].trim();
    if rest.len() >= 5 && (rest.starts_with('+') || rest.starts_with('-')) {
        let sign: i64 = if rest.starts_with('+') { 1 } else { -1 };
        let tz_h: i64 = rest[1..3].parse().unwrap_or(8);
        let tz_m: i64 = rest[3..5].parse().unwrap_or(0);
        epoch -= sign * (tz_h * 3600 + tz_m * 60);
    } else {
        epoch -= 8 * 3600;
    }

    Some(epoch)
}

fn parse_xmltv_sec_of_day(s: &str) -> Option<i64> {
    let trimmed = s.trim();
    if trimmed.len() < 14 {
        return None;
    }
    let digits = &trimmed[0..14];
    let hour: i64 = digits[8..10].parse().ok()?;
    let min: i64 = digits[10..12].parse().ok()?;
    let sec: i64 = digits[12..14].parse().ok()?;
    Some(hour * 3600 + min * 60 + sec)
}

fn normalize_channel_key(name: &str) -> String {
    name.trim()
        .to_uppercase()
        .replace('-', "")
        .replace('_', "")
        .replace(' ', "")
        .replace("综合", "")
        .replace("财经", "")
        .replace("纪录", "")
        .replace("新闻", "")
        .replace("频道", "")
        .replace("高清", "")
        .replace("FHD", "")
        .replace("HD", "")
        .replace("4K", "")
}

fn unescape_xml(s: &str) -> String {
    s.replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
        .replace("<![CDATA[", "")
        .replace("]]>", "")
}

fn extract_attr<'a>(tag: &'a str, attr_name: &str) -> Option<&'a str> {
    let pattern = format!("{}=\"", attr_name);
    let start = tag.find(&pattern)? + pattern.len();
    let rest = &tag[start..];
    let end = rest.find('"')?;
    Some(&rest[..end])
}

fn parse_epg_body_into_map(body: &str, result_map: &mut HashMap<String, String>) {
    let trimmed = body.trim_start();
    if trimmed.starts_with('{') {
        if let Ok(json_map) = serde_json::from_str::<HashMap<String, serde_json::Value>>(trimmed) {
            for (k, v) in json_map {
                if let Some(s) = v.as_str() {
                    if !s.trim().is_empty() {
                        result_map.insert(k.clone(), s.to_string());
                        result_map.insert(normalize_channel_key(&k), s.to_string());
                    }
                }
            }
        }
        return;
    }

    let now_epoch = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs() as i64;
    let now_bj_sec_of_day = (now_epoch + 8 * 3600).rem_euclid(86400);

    let mut chan_id_to_names: HashMap<String, Vec<String>> = HashMap::new();
    let mut tod_fallback_map: HashMap<String, String> = HashMap::new();

    let mut search_pos = 0;
    while let Some(c_pos) = body[search_pos..].find("<channel") {
        let abs_start = search_pos + c_pos;
        if let Some(c_end) = body[abs_start..].find("</channel>") {
            let block = &body[abs_start..abs_start + c_end];
            if let Some(tag_end) = block.find('>') {
                let open_tag = &block[..tag_end];
                if let Some(cid) = extract_attr(open_tag, "id") {
                    let mut names = vec![cid.to_string()];
                    let mut dn_pos = tag_end;
                    while let Some(d_start) = block[dn_pos..].find("<display-name") {
                        let d_abs = dn_pos + d_start;
                        if let Some(gt) = block[d_abs..].find('>') {
                            let content_start = d_abs + gt + 1;
                            if let Some(lt) = block[content_start..].find("</display-name>") {
                                let dn = unescape_xml(block[content_start..content_start + lt].trim());
                                if !dn.is_empty() && !names.contains(&dn) {
                                    names.push(dn);
                                }
                                dn_pos = content_start + lt + 15;
                                continue;
                            }
                        }
                        break;
                    }
                    chan_id_to_names.insert(cid.to_string(), names);
                }
            }
            search_pos = abs_start + c_end + 10;
        } else {
            break;
        }
    }

    let mut prog_pos = 0;
    while let Some(p_pos) = body[prog_pos..].find("<programme") {
        let abs_start = prog_pos + p_pos;
        if let Some(p_end) = body[abs_start..].find("</programme>") {
            let block = &body[abs_start..abs_start + p_end];
            if let Some(tag_end) = block.find('>') {
                let open_tag = &block[..tag_end];
                if let (Some(start_str), Some(stop_str), Some(cid)) = (
                    extract_attr(open_tag, "start"),
                    extract_attr(open_tag, "stop"),
                    extract_attr(open_tag, "channel"),
                ) {
                    let mut matched_exact = false;
                    let mut matched_tod = false;

                    if let (Some(st), Some(et)) = (parse_xmltv_timestamp(start_str), parse_xmltv_timestamp(stop_str)) {
                        if st <= now_epoch && now_epoch < et {
                            matched_exact = true;
                        }
                    }
                    if !matched_exact {
                        if let (Some(s_tod), Some(e_tod)) = (parse_xmltv_sec_of_day(start_str), parse_xmltv_sec_of_day(stop_str)) {
                            if s_tod <= e_tod {
                                if s_tod <= now_bj_sec_of_day && now_bj_sec_of_day < e_tod {
                                    matched_tod = true;
                                }
                            } else if now_bj_sec_of_day >= s_tod || now_bj_sec_of_day < e_tod {
                                matched_tod = true;
                            }
                        }
                    }

                    if matched_exact || matched_tod {
                        if let Some(t_start) = block[tag_end..].find("<title") {
                            let t_abs = tag_end + t_start;
                            if let Some(gt) = block[t_abs..].find('>') {
                                let c_start = t_abs + gt + 1;
                                if let Some(lt) = block[c_start..].find("</title>") {
                                    let prog_title = unescape_xml(block[c_start..c_start + lt].trim());
                                    if !prog_title.is_empty() {
                                        let target_ref = if matched_exact { &mut *result_map } else { &mut tod_fallback_map };
                                        target_ref.insert(cid.to_string(), prog_title.clone());
                                        target_ref.insert(normalize_channel_key(cid), prog_title.clone());
                                        if let Some(names) = chan_id_to_names.get(cid) {
                                            for n in names {
                                                target_ref.insert(n.clone(), prog_title.clone());
                                                target_ref.insert(normalize_channel_key(n), prog_title.clone());
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
            prog_pos = abs_start + p_end + 12;
        } else {
            break;
        }
    }

    for (k, v) in tod_fallback_map {
        result_map.entry(k).or_insert(v);
    }
}

#[tauri::command]
async fn fetch_epg_data(epg_url: String) -> Result<HashMap<String, String>, String> {
    let primary_url = if epg_url.trim().is_empty() {
        "https://epg.51zmt.top:8000/e.xml".to_string()
    } else {
        epg_url.trim().to_string()
    };

    tauri::async_runtime::spawn_blocking(move || {
        let mut result_map: HashMap<String, String> = HashMap::new();

        let mut urls_to_try = vec![primary_url.clone()];
        if primary_url.starts_with("https://") && primary_url.contains(":8000") {
            urls_to_try.push(primary_url.replacen("https://", "http://", 1));
        }
        urls_to_try.push("https://live.fanmingming.com/e.xml".to_string());
        urls_to_try.push("https://raw.githubusercontent.com/cohirogaobo/feelcool-tv-config/refs/heads/main/epg_lite.json".to_string());

        for u in &urls_to_try {
            let mut curl_cmd = Command::new("/usr/bin/curl");
            curl_cmd.args(&[
                "-sL",
                "--compressed",
                "-k",
                "--max-time",
                "16",
                "-H",
                "User-Agent: Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15",
            ]);
            if u.contains("51zmt") || u.contains("fanmingming") {
                curl_cmd.args(&["--noproxy", "*"]);
            }
            curl_cmd.arg(u);

            if let Ok(output) = curl_cmd.output() {
                if output.status.success() && output.stdout.len() > 20 {
                    let body = String::from_utf8_lossy(&output.stdout);
                    parse_epg_body_into_map(&body, &mut result_map);
                    if result_map.len() >= 15 {
                        break;
                    }
                }
            }
        }

        if result_map.is_empty() {
            Err("未能从 EPG 源解析到有效节目单".into())
        } else {
            Ok(result_map)
        }
    })
    .await
    .map_err(|e| format!("EPG 解析异常: {}", e))?
}

#[derive(serde::Serialize)]
struct WeatherBroadcastPayload {
    full_line: String,
    routine_headline: String,
    warning_headline: Option<String>,
    warning_key: String,
    areas: String,
    agency: String,
}

fn curl_get_json_direct(url: &str) -> (u16, Option<serde_json::Value>) {
    let out = Command::new("/usr/bin/curl")
        .args(&[
            "-sL",
            "--noproxy",
            "*",
            "--compressed",
            "--max-time",
            "6",
            "-H",
            "Accept-Encoding: gzip,deflate",
            "-H",
            "User-Agent: Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15",
            "-w",
            "\n__HTTP_CODE__:%{http_code}",
            url,
        ])
        .output();

    if let Ok(o) = out {
        let text = String::from_utf8_lossy(&o.stdout);
        if let Some(idx) = text.rfind("\n__HTTP_CODE__:") {
            let body = &text[..idx];
            let code_str = &text[idx + 15..].trim();
            let code = code_str.parse::<u16>().unwrap_or(0);
            let parsed = serde_json::from_str::<serde_json::Value>(body).ok();
            return (code, parsed);
        }
    }
    (0, None)
}

/// 去除区划名称末尾的“区”、“县”、“市”，如“玄武区”->“玄武”，“雨花台区”->“雨花台”
fn strip_district_suffix(name: &str) -> String {
    let t = name.trim();
    if t.chars().count() >= 2 {
        for suffix in &["新区", "区", "县", "市"] {
            if let Some(s) = t.strip_suffix(suffix) {
                if !s.is_empty() {
                    return s.to_string();
                }
            }
        }
    }
    t.to_string()
}

/// 根据当前城市与当前区，匹配当前区 + 临近三个区（共 4 个区域名称，不带“区”字）
fn resolve_four_nearby_districts(city: &str, current_dist: &str) -> Vec<String> {
    let cur = strip_district_suffix(current_dist);
    let clean_city = strip_district_suffix(city);

    // 针对南京各主城区的精准邻接排序（如玄武 -> 玄武 栖霞 雨花台 秦淮）
    if clean_city == "南京" {
        let nanjing_map: &[(&str, &[&str])] = &[
            ("玄武", &["玄武", "栖霞", "雨花台", "秦淮"]),
            ("秦淮", &["秦淮", "玄武", "雨花台", "建邺"]),
            ("建邺", &["建邺", "秦淮", "鼓楼", "雨花台"]),
            ("鼓楼", &["鼓楼", "玄武", "建邺", "栖霞"]),
            ("栖霞", &["栖霞", "玄武", "鼓楼", "江宁"]),
            ("雨花台", &["雨花台", "秦淮", "建邺", "江宁"]),
            ("江宁", &["江宁", "雨花台", "秦淮", "栖霞"]),
            ("浦口", &["浦口", "鼓楼", "建邺", "六合"]),
            ("六合", &["六合", "浦口", "栖霞", "玄武"]),
            ("溧水", &["溧水", "江宁", "高淳", "雨花台"]),
            ("高淳", &["高淳", "溧水", "江宁", "雨花台"]),
        ];
        for (k, list) in nanjing_map {
            if *k == cur {
                return list.iter().map(|s| s.to_string()).collect();
            }
        }
        return vec!["玄武".into(), "栖霞".into(), "雨花台".into(), "秦淮".into()];
    }

    let city_pools: &[(&str, &[&str])] = &[
        ("北京", &["朝阳", "海淀", "东城", "西城", "丰台", "石景山", "通州", "昌平", "大兴"]),
        ("上海", &["浦东", "黄浦", "徐汇", "静安", "长宁", "普陀", "虹口", "杨浦", "闵行"]),
        ("广州", &["天河", "越秀", "海珠", "荔湾", "白云", "黄埔", "番禺", "花都"]),
        ("深圳", &["福田", "南山", "罗湖", "宝安", "龙华", "龙岗", "盐田"]),
        ("杭州", &["西湖", "上城", "拱墅", "滨江", "萧山", "余杭", "钱塘"]),
        ("苏州", &["姑苏", "虎丘", "吴中", "相城", "吴江", "昆山"]),
        ("无锡", &["梁溪", "滨湖", "新吴", "锡山", "惠山"]),
        ("常州", &["天宁", "钟楼", "新北", "武进", "金坛"]),
        ("成都", &["锦江", "青羊", "武侯", "成华", "金牛", "双流", "温江"]),
        ("重庆", &["渝中", "江北", "南岸", "九龙坡", "沙坪坝", "渝北"]),
        ("武汉", &["武昌", "江汉", "江岸", "洪山", "汉阳", "硚口"]),
        ("西安", &["雁塔", "碑林", "新城", "莲湖", "未央", "长安"]),
        ("天津", &["和平", "河西", "南开", "河东", "河北", "红桥"]),
        ("合肥", &["蜀山", "庐阳", "包河", "瑶海"]),
        ("长沙", &["岳麓", "芙蓉", "天心", "开福", "雨花"]),
        ("郑州", &["金水", "中原", "二七", "管城", "惠济"]),
        ("青岛", &["市南", "市北", "崂山", "李沧", "黄岛"]),
        ("济南", &["历下", "市中", "槐荫", "天桥", "历城"]),
        ("宁波", &["海曙", "鄞州", "江北", "镇海", "北仑"]),
        ("厦门", &["思明", "湖里", "集美", "海沧"]),
        ("福州", &["鼓楼", "台江", "仓山", "晋安", "马尾"]),
        ("沈阳", &["和平", "沈河", "皇姑", "铁西", "浑南"]),
        ("大连", &["中山", "西岗", "沙河口", "甘井子"]),
    ];

    let mut out = Vec::new();
    if !cur.is_empty() {
        out.push(cur.clone());
    }
    for (c_name, pool) in city_pools {
        if *c_name == clean_city {
            for d in *pool {
                if out.len() >= 4 {
                    break;
                }
                let ds = d.to_string();
                if !out.contains(&ds) {
                    out.push(ds);
                }
            }
            break;
        }
    }
    if out.is_empty() && !clean_city.is_empty() {
        out.push(clean_city);
    }
    out
}

#[tauri::command]
async fn fetch_weather_broadcast(
    client_lat: Option<f64>,
    client_lon: Option<f64>,
    state: State<'_, AppState>,
) -> Result<WeatherBroadcastPayload, String> {
    let mc_400_arc = state.minutecast_400_until.clone();
    let was_precip_arc = state.was_precipitating.clone();

    tauri::async_runtime::spawn_blocking(move || {
        let api_key = "oLpVBp2pSM2J0ofeGZDdzfMPYePEbRZK";

        let mut lat: Option<f64> = None;
        let mut lon: Option<f64> = None;
        let mut domestic_city = String::new();
        let mut domestic_district = String::new();
        let mut domestic_province = String::new();

        if let (Some(clat), Some(clon)) = (client_lat, client_lon) {
            if clat > 3.0 && clat < 55.0 && clon > 70.0 && clon < 145.0 {
                lat = Some(clat);
                lon = Some(clon);
            }
        }

        // 强制直连调用美团高精度位置接口
        let (_, mt_json) = curl_get_json_direct("https://apimobile.meituan.com/locate/v2/ip/loc?rgeo=true");
        if let Some(mj) = mt_json {
            if let Some(d) = mj.get("data") {
                if lat.is_none() {
                    if let (Some(la), Some(lo)) = (d.get("lat").and_then(|v| v.as_f64()), d.get("lng").and_then(|v| v.as_f64())) {
                        if la.abs() > 1.0 && lo.abs() > 1.0 {
                            lat = Some(la);
                            lon = Some(lo);
                        }
                    }
                }
                if let Some(rg) = d.get("rgeo") {
                    if let Some(c) = rg.get("city").and_then(|v| v.as_str()) {
                        domestic_city = c.to_string();
                    }
                    if let Some(dist) = rg.get("district").and_then(|v| v.as_str()) {
                        domestic_district = dist.to_string();
                    }
                    if let Some(prov) = rg.get("province").and_then(|v| v.as_str()) {
                        domestic_province = prov.to_string();
                    }
                }
            }
        }

        // 备选直连：哔哩哔哩国内 IP 定位接口
        if lat.is_none() || domestic_city.is_empty() {
            let (_, bili_json) = curl_get_json_direct("https://api.live.bilibili.com/client/v1/Ip/getInfoNew");
            if let Some(bj) = bili_json {
                if let Some(d) = bj.get("data") {
                    if lat.is_none() {
                        let la_opt = d.get("latitude").and_then(|v| v.as_f64().or_else(|| v.as_str().and_then(|s| s.parse::<f64>().ok())));
                        let lo_opt = d.get("longitude").and_then(|v| v.as_f64().or_else(|| v.as_str().and_then(|s| s.parse::<f64>().ok())));
                        if let (Some(la), Some(lo)) = (la_opt, lo_opt) {
                            if la.abs() > 1.0 && lo.abs() > 1.0 {
                                lat = Some(la);
                                lon = Some(lo);
                            }
                        }
                    }
                    if domestic_city.is_empty() {
                        if let Some(c) = d.get("city").and_then(|v| v.as_str()) {
                            domestic_city = c.to_string();
                        }
                    }
                    if domestic_province.is_empty() {
                        if let Some(p) = d.get("province").and_then(|v| v.as_str()) {
                            domestic_province = p.to_string();
                        }
                    }
                }
            }
        }

        // 备选直连：腾讯位置接口
        if domestic_city.is_empty() {
            let (_, qq_json) = curl_get_json_direct("https://r.inews.qq.com/api/ip2city");
            if let Some(qj) = qq_json {
                if let Some(c) = qj.get("city").and_then(|v| v.as_str()) {
                    domestic_city = c.to_string();
                }
                if let Some(dist) = qj.get("district").and_then(|v| v.as_str()) {
                    if domestic_district.is_empty() {
                        domestic_district = dist.to_string();
                    }
                }
                if let Some(p) = qj.get("province").and_then(|v| v.as_str()) {
                    if domestic_province.is_empty() {
                        domestic_province = p.to_string();
                    }
                }
            }
        }

        let mut loc_key = String::new();
        let mut admin_name = domestic_province.clone();

        if let (Some(la), Some(lo)) = (lat, lon) {
            let lat_round = (la * 1000.0).round() / 1000.0;
            let lon_round = (lo * 1000.0).round() / 1000.0;
            let q_coord = format!("{:.3},{:.3}", lat_round, lon_round);
            let geo_url = format!(
                "https://openapi.weathercn.com/locations/v1/cities/geoposition/search.json?q={}&apikey={}&language=zh-cn&details=true",
                q_coord, api_key
            );
            let (_, geo_json) = curl_get_json_direct(&geo_url);
            if let Some(gj) = geo_json {
                if let Some(k) = gj.get("Key").and_then(|v| v.as_str()) {
                    loc_key = k.to_string();
                }
                if domestic_district.is_empty() {
                    if let Some(local_name) = gj.get("LocalizedName").and_then(|v| v.as_str()) {
                        domestic_district = local_name.to_string();
                    }
                }
                if domestic_city.is_empty() {
                    if let Some(parent_name) = gj.get("ParentCity").and_then(|p| p.get("LocalizedName")).and_then(|v| v.as_str()) {
                        domestic_city = parent_name.to_string();
                    }
                }
                if let Some(adm) = gj.get("AdministrativeArea").and_then(|a| a.get("LocalizedName")).and_then(|v| v.as_str()) {
                    if !adm.is_empty() {
                        admin_name = adm.to_string();
                    }
                }
            }
        }

        if loc_key.is_empty() && (!domestic_district.is_empty() || !domestic_city.is_empty()) {
            let query_place = if !domestic_district.is_empty() { &domestic_district } else { &domestic_city };
            let trans_url = format!(
                "https://openapi.weathercn.com/locations/v1/cities/translate?q={}&apikey={}&language=zh-cn&details=true",
                url_encode_simple(query_place),
                api_key
            );
            let (_, trans_json) = curl_get_json_direct(&trans_url);
            if let Some(arr) = trans_json.as_ref().and_then(|v| v.as_array()) {
                if let Some(first_hit) = arr.first() {
                    if let Some(k) = first_hit.get("Key").and_then(|v| v.as_str()) {
                        loc_key = k.to_string();
                    }
                    if let (Some(la), Some(lo)) = (
                        first_hit.get("GeoPosition").and_then(|g| g.get("Latitude")).and_then(|v| v.as_f64()),
                        first_hit.get("GeoPosition").and_then(|g| g.get("Longitude")).and_then(|v| v.as_f64()),
                    ) {
                        lat = Some(la);
                        lon = Some(lo);
                    }
                    if let Some(adm) = first_hit.get("AdministrativeArea").and_then(|a| a.get("LocalizedName")).and_then(|v| v.as_str()) {
                        if !adm.is_empty() {
                            admin_name = adm.to_string();
                        }
                    }
                }
            }
        }

        // 构造当前区 + 临近三个区（不显示“区”字），若城市下有补充区划则优先补足 4 个
        let mut four_districts = resolve_four_nearby_districts(&domestic_city, &domestic_district);
        if four_districts.len() < 4 {
            if let (Some(la), Some(lo)) = (lat, lon) {
                let offsets = [(0.055, 0.045), (-0.055, -0.045), (0.045, -0.055)];
                for (dlat, dlon) in &offsets {
                    if four_districts.len() >= 4 {
                        break;
                    }
                    let q_near = format!("{:.3},{:.3}", la + dlat, lo + dlon);
                    let near_url = format!(
                        "https://openapi.weathercn.com/locations/v1/cities/geoposition/search.json?q={}&apikey={}&language=zh-cn",
                        q_near, api_key
                    );
                    let (_, near_json) = curl_get_json_direct(&near_url);
                    if let Some(nj) = near_json {
                        if let Some(ln) = nj.get("LocalizedName").and_then(|v| v.as_str()) {
                            let stripped = strip_district_suffix(ln);
                            if !stripped.is_empty() && !four_districts.contains(&stripped) {
                                four_districts.push(stripped);
                            }
                        }
                    }
                }
            }
        }
        let areas_display = four_districts.into_iter().take(4).collect::<Vec<_>>().join(" ");

        // 第二行居右机构名称：如“江苏气象台”
        let clean_admin = admin_name
            .trim_end_matches("省")
            .trim_end_matches("市")
            .trim_end_matches("壮族自治区")
            .trim_end_matches("回族自治区")
            .trim_end_matches("维吾尔自治区")
            .trim_end_matches("自治区")
            .trim_end_matches("特别行政区")
            .to_string();
        let agency_name = if !clean_admin.is_empty() {
            format!("{}气象台", clean_admin)
        } else {
            "中央气象台".to_string()
        };

        let mut warning_headline: Option<String> = None;
        let mut warning_key = String::new();

        // 1. 查询灾害预警 API (/alerts/v1)
        if !loc_key.is_empty() {
            let alert_url = format!(
                "https://openapi.weathercn.com/alerts/v1/{}.json?apikey={}&language=zh-cn&details=true",
                loc_key, api_key
            );
            let (_, alert_json) = curl_get_json_direct(&alert_url);
            if let Some(arr) = alert_json.as_ref().and_then(|v| v.as_array()) {
                if let Some(first_alert) = arr.first() {
                    let desc = first_alert
                        .get("Description")
                        .and_then(|d| d.get("Localized"))
                        .and_then(|v| v.as_str())
                        .or_else(|| first_alert.get("Type").and_then(|v| v.as_str()))
                        .unwrap_or("气象灾害预警");
                    let alert_id = first_alert.get("AlertID").and_then(|v| v.as_i64()).unwrap_or(1);
                    warning_headline = Some(format!("气象预警 {}", desc));
                    warning_key = format!("alert_{}", alert_id);
                }
            }
        }

        // 2. 查询分钟级降水 API (/nowcast_cn/v3/basic)，检测：降水开始、即将停止、降水结束
        let mut mc_if_pre = false;
        let mut mc_checked = false;
        let now_sec = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs();
        if let (Some(la), Some(lo)) = (lat, lon) {
            if now_sec >= mc_400_arc.load(Ordering::SeqCst) {
                let lat_round = (la * 1000.0).round() / 1000.0;
                let lon_round = (lo * 1000.0).round() / 1000.0;
                let q_coord = format!("{:.3},{:.3}", lat_round, lon_round);
                let mc_url = format!(
                    "https://openapi.weathercn.com/nowcast_cn/v3/basic?q={}&apikey={}&language=zh-cn",
                    q_coord, api_key
                );
                let (mc_code, mc_json) = curl_get_json_direct(&mc_url);
                if mc_code == 400 {
                    mc_400_arc.store(now_sec + 3600, Ordering::SeqCst);
                } else if let Some(mj) = mc_json {
                    if let Some(data_obj) = mj.get("Data") {
                        mc_checked = true;
                        mc_if_pre = data_obj.get("IfPre").and_then(|v| v.as_bool()).unwrap_or(false);
                        let short_phrase = data_obj
                            .get("ShortPhrase")
                            .and_then(|v| v.as_str())
                            .or_else(|| data_obj.get("Description").and_then(|v| v.as_str()))
                            .unwrap_or("");

                        let mut is_stopping_soon = short_phrase.contains("停") || short_phrase.contains("止");
                        if !is_stopping_soon {
                            if let Some(intens) = data_obj.get("Intensity").and_then(|v| v.as_array()) {
                                if intens.len() >= 6 {
                                    let first_val = intens[0].as_f64().unwrap_or(0.0);
                                    let tail_max = intens.iter().skip(intens.len() / 2).filter_map(|x| x.as_f64()).fold(0.0, f64::max);
                                    if first_val > 0.02 && tail_max == 0.0 {
                                        is_stopping_soon = true;
                                    }
                                }
                            }
                        }

                        if warning_headline.is_none() && mc_if_pre {
                            if is_stopping_soon {
                                let msg = if short_phrase.is_empty() { "降雨即将停止" } else { short_phrase };
                                warning_headline = Some(format!("降水预报 {}", msg));
                                warning_key = format!("mc_stopping_{}", msg);
                            } else {
                                let msg = if short_phrase.is_empty() { "即将开始降水" } else { short_phrase };
                                warning_headline = Some(format!("降水预报 {}", msg));
                                warning_key = format!("mc_starting_{}", msg);
                            }
                        }
                    }
                }
            }
        }

        // 3. 查询空气质量 API (/airquality/v1/global/observations)
        let mut aqi_val: i64 = 0;
        let mut pm25_val: f64 = 0.0;
        if !loc_key.is_empty() {
            let aq_url = format!(
                "https://openapi.weathercn.com/airquality/v1/global/observations/{}.json?apikey={}&language=zh-cn",
                loc_key, api_key
            );
            let (_, aq_json) = curl_get_json_direct(&aq_url);
            if let Some(aj) = aq_json {
                aqi_val = aj.get("Index").and_then(|v| v.as_i64()).unwrap_or(0);
                pm25_val = aj.get("ParticulateMatter2_5").and_then(|v| v.as_f64()).unwrap_or(0.0);
            }
        }

        // 4. 查询当前实况天气 API (/currentconditions/v1)
        let mut weather_text = String::from("多云");
        let mut temp_c: f64 = 20.0;
        let mut humidity: i64 = 55;
        let mut has_precip = false;

        if !loc_key.is_empty() {
            let cc_url = format!(
                "https://openapi.weathercn.com/currentconditions/v1/{}.json?apikey={}&language=zh-cn&details=true",
                loc_key, api_key
            );
            let (_, cc_json) = curl_get_json_direct(&cc_url);
            if let Some(arr) = cc_json.as_ref().and_then(|v| v.as_array()) {
                if let Some(cc) = arr.first() {
                    if let Some(wt) = cc.get("WeatherText").and_then(|v| v.as_str()) {
                        if !wt.is_empty() {
                            weather_text = wt.to_string();
                        }
                    }
                    if let Some(tc) = cc.get("Temperature").and_then(|t| t.get("Metric")).and_then(|m| m.get("Value")).and_then(|v| v.as_f64()) {
                        temp_c = tc;
                    }
                    if let Some(rh) = cc.get("RelativeHumidity").and_then(|v| v.as_i64()) {
                        humidity = rh;
                    }
                    has_precip = cc.get("HasPrecipitation").and_then(|v| v.as_bool()).unwrap_or(false);
                }
            }
        }

        // 检测分钟级降水结束（此前处于降水状态，当前 MinuteCast 与实况均无降水）
        let now_precip = mc_if_pre || has_precip;
        {
            let mut prev_p = was_precip_arc.lock().unwrap();
            if *prev_p && !now_precip && mc_checked && warning_headline.is_none() {
                warning_headline = Some("降水预报 降水已结束".to_string());
                warning_key = format!("mc_ended_{}", now_sec / 600);
            }
            *prev_p = now_precip;
        }

        // 检测 AQI 超标与湿度超标
        if warning_headline.is_none() && (aqi_val >= 101 || pm25_val >= 75.0) {
            warning_headline = Some(format!("空气质量提醒 AQI {}", aqi_val));
            warning_key = format!("aqi_over_{}", aqi_val / 20);
        }
        if warning_headline.is_none() && humidity >= 85 {
            warning_headline = Some(format!("高湿提醒 湿度{}%", humidity));
            warning_key = format!("humidity_over_{}", humidity / 5);
        }

        // 第一行居左常规格式：“气象速报 阴 22°C”
        let routine_headline = format!("气象速报 {} {:.0}°C", weather_text, temp_c);

        // 第三轮单行完整格式：“气象速报 阴 22°C 湿度69% AQI45   玄武区 南京 江苏省       江苏气象台 中国气象局”
        let aqi_part = if aqi_val > 0 { format!(" AQI{}", aqi_val) } else { String::new() };
        let base_status = if let Some(ref w) = warning_headline {
            w.clone()
        } else {
            format!("气象速报 {} {:.0}°C 湿度{}%{}", weather_text, temp_c, humidity, aqi_part)
        };
        let mut loc_tokens = Vec::new();
        if !domestic_district.is_empty() {
            loc_tokens.push(domestic_district.clone());
        }
        if !domestic_city.is_empty() && domestic_city != domestic_district {
            loc_tokens.push(domestic_city.clone());
        }
        if !admin_name.is_empty() && admin_name != domestic_city {
            loc_tokens.push(admin_name.clone());
        }
        let loc_str = if loc_tokens.is_empty() { areas_display.clone() } else { loc_tokens.join(" ") };
        let full_line = format!("{}   {}       {} 中国气象局", base_status, loc_str, agency_name);

        Ok(WeatherBroadcastPayload {
            full_line,
            routine_headline,
            warning_headline,
            warning_key,
            areas: areas_display,
            agency: agency_name,
        })
    })
    .await
    .map_err(|e| format!("天气获取异常: {}", e))?
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

fn category_to_douyin_url(cat: &str) -> &'static str {
    match cat {
        "discover" => "https://www.douyin.com/discover",
        "recommend" => "https://www.douyin.com/?recommend=1",
        "knowledge" => "https://www.douyin.com/channel/300203",
        "acg" => "https://www.douyin.com/channel/300206",
        "game" => "https://www.douyin.com/channel/300205",
        "food" => "https://www.douyin.com/channel/300204",
        "sports" => "https://www.douyin.com/channel/300207",
        "fashion" => "https://www.douyin.com/channel/300208",
        "music" => "https://www.douyin.com/channel/300209",
        _ => "https://www.douyin.com/discover",
    }
}

fn ensure_douyin_scraper(
    app: &AppHandle,
    feed_arc: Arc<Mutex<Vec<DouyinItem>>>,
    probing_arc: Arc<Mutex<HashSet<String>>>,
) {
    if app.get_webview_window("douyin_scraper").is_some() {
        return;
    }

    let hook_script = r#"
        (function() {
            try {
                Object.defineProperty(document, 'visibilityState', { get: () => 'visible', configurable: true });
                Object.defineProperty(document, 'hidden', { get: () => false, configurable: true });
            } catch(e) {}

            try {
                if (window.MediaSource && MediaSource.isTypeSupported) {
                    const origIsType = MediaSource.isTypeSupported.bind(MediaSource);
                    MediaSource.isTypeSupported = function(mime) {
                        if (typeof mime === 'string' && /hev1|hvc1|bytevc|bvc2|av01|vp9/i.test(mime)) return false;
                        return origIsType(mime);
                    };
                }
                if (window.HTMLVideoElement && HTMLVideoElement.prototype && HTMLVideoElement.prototype.canPlayType) {
                    const origCanPlay = HTMLVideoElement.prototype.canPlayType;
                    HTMLVideoElement.prototype.canPlayType = function(mime) {
                        if (typeof mime === 'string' && /hev1|hvc1|bytevc|bvc2|av01|vp9/i.test(mime)) return '';
                        return origCanPlay.apply(this, arguments);
                    };
                }
            } catch(e) {}

            window.__FC_QUEUE__ = [];
            window.__FC_SEEN__ = new Set();
            window.__FC_FETCH_QUEUE__ = [];
            window.__FC_FETCHED_IDS__ = new Set();

            setInterval(() => {
                if (window.__FC_QUEUE__.length > 0) {
                    const item = window.__FC_QUEUE__.shift();
                    const payload = item.map(s => encodeURIComponent(s)).join('|||');
                    try {
                        document.title = '__FC_ITEM__:' + payload + '|||' + Date.now();
                    } catch(e) {}
                }
            }, 90);

            function normalizeHttps(u) {
                if (!u || typeof u !== 'string') return '';
                u = u.trim();
                if (u.startsWith('//')) return 'https:' + u;
                if (u.startsWith('http://')) return 'https://' + u.substring(7);
                return u;
            }

            function isInvalidVideoUrl(u) {
                if (!u) return true;
                const lower = u.toLowerCase();
                if (lower.includes('mime_type=audio') || lower.includes('.mp3') || lower.includes('.m4a') || lower.includes('ies-music') || lower.includes('.mpd')) {
                    return true;
                }
                return false;
            }

            function isByteVC2Entry(br) {
                if (!br) return false;
                if (br.is_bytevc1 === 2 || br.is_h265 === 2) return true;
                const g = String(br.gear_name || '') + ' ' + String(br.codec_type || '') + ' ' + String(br.VideoExtra || '');
                if (/bytevc2|bvc2/i.test(g)) return true;
                if (br.play_addr && String(br.play_addr.url_key || '').toLowerCase().includes('bytevc2')) return true;
                return false;
            }

            function appendUrlsFromAddr(addr, outList) {
                if (!addr || !Array.isArray(addr.url_list)) return;
                const sorted = addr.url_list.slice().sort((a, b) => {
                    const score = x => (x.includes('douyinvod.com') ? 3 : (x.includes('zjcdn.com') ? 2 : 0));
                    return score(b) - score(a);
                });
                for (const raw of sorted) {
                    const u = normalizeHttps(raw);
                    if (u && !isInvalidVideoUrl(u) && !u.includes('/aweme/v1/play') && !outList.includes(u)) {
                        outList.push(u);
                    }
                }
            }

            function pickBestPlayUrl(v) {
                const candidates = [];
                const hasBitRateList = Array.isArray(v.bit_rate) && v.bit_rate.length > 0;
                let allBitRatesAreByteVC2 = hasBitRateList;

                // 任务 10 (Round 5)：按帧率 (FPS, 优先 60fps) 与码率 (bit_rate) 降序排列，确保大屏下优先拉取最高帧率与最高画质源
                const sortedBitRates = hasBitRateList ? v.bit_rate.slice().sort((a, b) => {
                    const fpsA = Number((a && (a.FPS || a.fps)) || 30);
                    const fpsB = Number((b && (b.FPS || b.fps)) || 30);
                    if (fpsB !== fpsA) return fpsB - fpsA;
                    const brA = Number((a && a.bit_rate) || 0);
                    const brB = Number((b && b.bit_rate) || 0);
                    return brB - brA;
                }) : [];

                if (hasBitRateList) {
                    for (const br of sortedBitRates) {
                        if (!br) continue;
                        if (!isByteVC2Entry(br)) allBitRatesAreByteVC2 = false;
                        if (isByteVC2Entry(br) || br.format === 'dash') continue;
                        if (!br.is_h265 && !br.is_bytevc1 && br.play_addr) {
                            appendUrlsFromAddr(br.play_addr, candidates);
                        }
                    }
                }

                if (v.play_addr_264) appendUrlsFromAddr(v.play_addr_264, candidates);
                if (v.play_addr_h264) appendUrlsFromAddr(v.play_addr_h264, candidates);
                if (v.play_addr_lowbr && !isByteVC2Entry(v.play_addr_lowbr)) appendUrlsFromAddr(v.play_addr_lowbr, candidates);

                if (hasBitRateList) {
                    for (const br of sortedBitRates) {
                        if (!br || isByteVC2Entry(br) || br.format === 'dash') continue;
                        if ((br.is_bytevc1 === 1 || br.is_h265 === 1) && br.play_addr) {
                            appendUrlsFromAddr(br.play_addr, candidates);
                        }
                    }
                }

                if (!allBitRatesAreByteVC2) {
                    if (v.play_addr && !String(v.play_addr.url_key || '').toLowerCase().includes('bytevc2')) {
                        appendUrlsFromAddr(v.play_addr, candidates);
                    }
                    if (v.download_addr) {
                        appendUrlsFromAddr(v.download_addr, candidates);
                    }
                }

                return candidates.slice(0, 2).join('@@');
            }

            function recordAweme(aweme) {
                if (!aweme || !aweme.video) return;
                if (aweme.aweme_type === 68 || aweme.aweme_type === 101 || aweme.aweme_type === 2) return;
                if (aweme.images && Array.isArray(aweme.images) && aweme.images.length > 0) return;
                if (aweme.cell_room || aweme.live_reason || aweme.is_ads || aweme.raw_ad_data) return;

                const v = aweme.video;
                const playUrl = pickBestPlayUrl(v);
                if (!playUrl) return;

                const idKey = String(aweme.aweme_id || (v.play_addr && v.play_addr.uri) || playUrl.split('@@')[0] || '').trim();
                if (!idKey || window.__FC_SEEN__.has(idKey)) return;

                const rawDesc = String(aweme.desc || '抖音推荐视频').trim();
                const desc = rawDesc.length > 70 ? rawDesc.substring(0, 70) : rawDesc;
                const author = (aweme.author && aweme.author.nickname) ? ('@' + aweme.author.nickname) : '@抖音创作者';
                let cover = '';
                if (v.cover && Array.isArray(v.cover.url_list) && v.cover.url_list[0]) cover = v.cover.url_list[0];
                else if (v.origin_cover && Array.isArray(v.origin_cover.url_list) && v.origin_cover.url_list[0]) cover = v.origin_cover.url_list[0];
                else if (v.dynamic_cover && Array.isArray(v.dynamic_cover.url_list) && v.dynamic_cover.url_list[0]) cover = v.dynamic_cover.url_list[0];

                cover = normalizeHttps(cover);
                if (playUrl && cover) {
                    window.__FC_SEEN__.add(idKey);
                    window.__FC_QUEUE__.push([desc, author, cover, playUrl]);
                }
            }

            function deepScan(obj, depth) {
                if (!obj || typeof obj !== 'object' || depth > 9) return;
                if (obj.video && (obj.video.play_addr || obj.video.bit_rate)) {
                    recordAweme(obj);
                    return;
                }
                if (Array.isArray(obj)) {
                    for (let i = 0; i < obj.length; i++) deepScan(obj[i], depth + 1);
                } else {
                    for (const k in obj) {
                        if (Object.prototype.hasOwnProperty.call(obj, k)) {
                            deepScan(obj[k], depth + 1);
                        }
                    }
                }
            }

            const origJsonParse = JSON.parse;
            JSON.parse = function(text, reviver) {
                const res = origJsonParse.call(this, text, reviver);
                try {
                    if (typeof text === 'string' && (text.includes('play_addr') || text.includes('aweme_list') || text.includes('bit_rate') || text.includes('aweme_info'))) {
                        deepScan(res, 0);
                    }
                } catch(e) {}
                return res;
            };

            const origResJson = Response.prototype.json;
            Response.prototype.json = async function() {
                const data = await origResJson.apply(this, arguments);
                try { deepScan(data, 0); } catch(e) {}
                return data;
            };

            window.__FC_EMPTY_LOAD_COUNT__ = 0;
            // 任务 13 & 14 (Round 5)：全容器智能无限滚动触发 + 枯竭自动续流，确保焦点靠近末尾与全屏连播时始终能预加载新视频流
            window.__FC_LOAD_MORE__ = function() {
                const beforeSeen = window.__FC_SEEN__.size;
                for (let step = 0; step < 4; step++) {
                    setTimeout(() => {
                        try {
                            const nextBtn = document.querySelector('[data-e2e="video-switch-next-arrow"], .xgplayer-playswitch-next, [class*="playswitch-next"]');
                            if (nextBtn) nextBtn.click();
                            document.dispatchEvent(new KeyboardEvent('keydown', { key: 'ArrowDown', code: 'ArrowDown', keyCode: 40, which: 40, bubbles: true }));
                            window.dispatchEvent(new WheelEvent('wheel', { deltaY: 1200, bubbles: true }));
                            window.scrollBy({ top: 1400, behavior: 'smooth' });
                            const allDivs = document.querySelectorAll('div, main, section, ul');
                            for (let i = 0; i < allDivs.length; i++) {
                                const el = allDivs[i];
                                if (el.scrollHeight > el.clientHeight + 40) {
                                    el.scrollTop = el.scrollTop + 1400;
                                    el.dispatchEvent(new Event('scroll', { bubbles: true }));
                                }
                            }
                        } catch(e) {}
                    }, step * 200);
                }
                setTimeout(() => {
                    if (window.__FC_SEEN__.size === beforeSeen && window.__FC_QUEUE__.length === 0 && !document.getElementById('fc-login-bar')) {
                        window.__FC_EMPTY_LOAD_COUNT__ = (window.__FC_EMPTY_LOAD_COUNT__ || 0) + 1;
                        if (window.__FC_EMPTY_LOAD_COUNT__ >= 2) {
                            window.__FC_EMPTY_LOAD_COUNT__ = 0;
                            window.location.reload();
                        }
                    } else {
                        window.__FC_EMPTY_LOAD_COUNT__ = 0;
                    }
                }, 1400);
            };

            window.__FC_SHOW_LOGIN_BAR__ = function() {
                if (document.getElementById('fc-login-bar')) return;
                const bar = document.createElement('div');
                bar.id = 'fc-login-bar';
                bar.style.cssText = 'position:fixed;top:16px;left:50%;transform:translateX(-50%);z-index:2147483647;background:#0A84FF;color:#fff;padding:10px 28px;border-radius:999px;font-family:-apple-system,sans-serif;font-size:15px;font-weight:600;cursor:pointer;box-shadow:0 10px 30px rgba(0,0,0,0.6);border:1.5px solid rgba(255,255,255,0.4);';
                bar.innerText = '✅ 完成登录并返回 Feel Cool TV (或按 Esc / 关闭窗口)';
                bar.onclick = function() {
                    bar.remove();
                    document.title = '__FC_LOGIN_DONE__';
                };
                if (document.body) document.body.appendChild(bar);
            };

            window.addEventListener('keydown', function(e) {
                if (e.key === 'Escape' && document.getElementById('fc-login-bar')) {
                    document.getElementById('fc-login-bar').remove();
                    document.title = '__FC_LOGIN_DONE__';
                }
            }, true);

            setInterval(() => {
                if (!document.getElementById('fc-login-bar')) {
                    document.querySelectorAll('video, audio').forEach(el => { el.muted = true; el.volume = 0; });
                }
                const cards = document.querySelectorAll('[data-aweme-id]');
                cards.forEach(card => {
                    const id = card.getAttribute('data-aweme-id');
                    if (id && !window.__FC_FETCHED_IDS__.has(id)) {
                        window.__FC_FETCHED_IDS__.add(id);
                        window.__FC_FETCH_QUEUE__.push(id);
                    }
                });

                if (window.__FC_FETCH_QUEUE__.length > 0 && window.jingxuan_pre_build_util && window.jingxuan_pre_build_util.fetchAwemeInfo) {
                    const nextId = window.__FC_FETCH_QUEUE__.shift();
                    window.jingxuan_pre_build_util.fetchAwemeInfo({ awemeId: nextId, originType: 'quick_player' })
                        .then(res => { if (res && res.detail) recordAweme(res.detail); })
                        .catch(() => {});
                }
            }, 350);
        })();
    "#;

    let parsed_url: tauri::Url = "https://www.douyin.com/discover".parse().unwrap();
    let _ = WebviewWindowBuilder::new(app, "douyin_scraper", WebviewUrl::External(parsed_url))
        .title("抖音账号登录")
        .inner_size(1180.0, 760.0)
        .visible(false)
        .focused(false)
        .initialization_script(hook_script)
        .on_document_title_changed(move |win, title| {
            if title == "__FC_LOGIN_DONE__" {
                let _ = win.set_always_on_top(false);
                let _ = win.hide();
                feed_arc.lock().unwrap().clear();
                probing_arc.lock().unwrap().clear();
                let _ = win.eval("window.__FC_QUEUE__ = []; window.__FC_SEEN__.clear(); window.__FC_FETCHED_IDS__.clear(); window.location.reload();");
                if let Some(main_win) = win.app_handle().get_webview_window("main") {
                    let _ = main_win.set_focus();
                }
            } else if let Some(payload) = title.strip_prefix("__FC_ITEM__:") {
                let parts: Vec<&str> = payload.split("|||").collect();
                if parts.len() >= 4 {
                    let desc = url_decode(parts[0]);
                    let author = url_decode(parts[1]);
                    let cover = url_decode(parts[2]);
                    let play_url = url_decode(parts[3]);
                    let primary_url = play_url.split("@@").next().unwrap_or(&play_url).to_string();
                    if !primary_url.starts_with("https://") {
                        return;
                    }
                    {
                        let mut p_set = probing_arc.lock().unwrap();
                        if p_set.contains(&primary_url) {
                            return;
                        }
                        p_set.insert(primary_url.clone());
                    }

                    let feed_clone = feed_arc.clone();
                    tauri::async_runtime::spawn_blocking(move || {
                        if let Some((verified_url, codec)) = probe_douyin_candidates(&play_url) {
                            let tagged_url = if codec == "h264" {
                                format!("h264:{}", verified_url)
                            } else {
                                verified_url.clone()
                            };
                            let mut feed = feed_clone.lock().unwrap();
                            if !feed.iter().any(|x| x.3.ends_with(&verified_url)) {
                                feed.push((desc, author, cover, tagged_url));
                            }
                        }
                    });
                }
            }
        })
        .build();
}

#[tauri::command]
async fn get_douyin_feed(
    app: AppHandle,
    refresh: bool,
    load_more: Option<bool>,
    category: Option<String>,
    state: State<'_, AppState>,
) -> Result<Vec<DouyinItem>, String> {
    ensure_douyin_scraper(&app, state.douyin_feed.clone(), state.douyin_probing.clone());

    let req_cat = category.unwrap_or_else(|| "discover".to_string());
    let mut cat_changed = false;
    {
        let mut cur_cat = state.douyin_category.lock().unwrap();
        if *cur_cat != req_cat {
            *cur_cat = req_cat.clone();
            cat_changed = true;
        }
    }

    if refresh || cat_changed {
        state.douyin_feed.lock().unwrap().clear();
        state.douyin_probing.lock().unwrap().clear();
        let target_url = category_to_douyin_url(&req_cat);
        if let Some(win) = app.get_webview_window("douyin_scraper") {
            let js = format!(
                "window.__FC_QUEUE__ = []; window.__FC_SEEN__.clear(); window.__FC_FETCHED_IDS__.clear(); if (window.location.href !== '{}') {{ window.location.href = '{}'; }} else {{ window.location.reload(); }}",
                target_url, target_url
            );
            let _ = win.eval(&js);
        }
        return Ok(Vec::new());
    }

    if load_more.unwrap_or(false) {
        if let Some(win) = app.get_webview_window("douyin_scraper") {
            let _ = win.eval("if(window.__FC_LOAD_MORE__) window.__FC_LOAD_MORE__();");
        }
    }

    let feed = state.douyin_feed.lock().unwrap().clone();
    Ok(feed)
}

#[tauri::command]
async fn open_douyin_login(app: AppHandle, state: State<'_, AppState>) -> Result<(), String> {
    ensure_douyin_scraper(&app, state.douyin_feed.clone(), state.douyin_probing.clone());
    if let Some(win) = app.get_webview_window("douyin_scraper") {
        let _ = win.show();
        let _ = win.center();
        let _ = win.set_always_on_top(true);
        let _ = win.set_focus();
        let _ = win.eval("if(window.__FC_SHOW_LOGIN_BAR__) window.__FC_SHOW_LOGIN_BAR__();");
    }
    Ok(())
}

#[tauri::command]
async fn hide_douyin_login(app: AppHandle, state: State<'_, AppState>) -> Result<(), String> {
    if let Some(win) = app.get_webview_window("douyin_scraper") {
        let _ = win.set_always_on_top(false);
        let _ = win.hide();
        state.douyin_feed.lock().unwrap().clear();
        state.douyin_probing.lock().unwrap().clear();
        let _ = win.eval("const b = document.getElementById('fc-login-bar'); if(b) b.remove(); window.__FC_QUEUE__ = []; window.__FC_SEEN__.clear(); window.__FC_FETCHED_IDS__.clear(); window.location.reload();");
    }
    if let Some(main_win) = app.get_webview_window("main") {
        let _ = main_win.set_focus();
    }
    Ok(())
}

// 任务 12 (Round 5)：电源管理——彻底清理子进程并安全关闭退出或重启应用
#[tauri::command]
async fn quit_app(app: AppHandle, state: State<'_, AppState>) -> Result<(), String> {
    if let Ok(mut guard) = state.ffmpeg_process.lock() {
        if let Some((_, mut child)) = guard.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
    cleanup_orphan_hls_ffmpeg();
    app.exit(0);
    Ok(())
}

#[tauri::command]
async fn restart_app(app: AppHandle, state: State<'_, AppState>) -> Result<(), String> {
    if let Ok(mut guard) = state.ffmpeg_process.lock() {
        if let Some((_, mut child)) = guard.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
    cleanup_orphan_hls_ffmpeg();
    app.restart();
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    cleanup_orphan_hls_ffmpeg();

    let shared_douyin_feed: Arc<Mutex<Vec<DouyinItem>>> = Arc::new(Mutex::new(Vec::new()));
    let shared_douyin_probing: Arc<Mutex<HashSet<String>>> = Arc::new(Mutex::new(HashSet::new()));
    let setup_feed = shared_douyin_feed.clone();
    let setup_probing = shared_douyin_probing.clone();

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
            ffmpeg_process: Arc::new(Mutex::new(None)),
            ffmpeg_op_lock: Arc::new(Mutex::new(())),
            ffmpeg_generation: Arc::new(AtomicU64::new(0)),
            douyin_feed: shared_douyin_feed,
            douyin_probing: shared_douyin_probing,
            douyin_category: Arc::new(Mutex::new("discover".to_string())),
            thumb_lock: Arc::new(Mutex::new(())),
            minutecast_400_until: Arc::new(AtomicU64::new(0)),
            was_precipitating: Arc::new(Mutex::new(false)),
        })
        .setup(move |app| {
            if let Some(main_win) = app.get_webview_window("main") {
                let _ = main_win.set_title("");
                let _ = main_win.set_title_bar_style(TitleBarStyle::Overlay);
            }
            ensure_douyin_scraper(app.handle(), setup_feed.clone(), setup_probing.clone());
            Ok(())
        })
        .on_window_event(|window, event| {
            if window.label() == "main" {
                if let WindowEvent::CloseRequested { .. } | WindowEvent::Destroyed = event {
                    if let Some(state) = window.app_handle().try_state::<AppState>() {
                        if let Ok(mut guard) = state.ffmpeg_process.lock() {
                            if let Some((_, mut child)) = guard.take() {
                                let _ = child.kill();
                                let _ = child.wait();
                            }
                        }
                    }
                    cleanup_orphan_hls_ffmpeg();
                    window.app_handle().exit(0);
                }
            } else if window.label() == "douyin_scraper" {
                if let WindowEvent::CloseRequested { api, .. } = event {
                    api.prevent_close();
                    let _ = window.set_always_on_top(false);
                    let _ = window.hide();
                    if let Some(wv) = window.app_handle().get_webview_window("douyin_scraper") {
                        let _ = wv.eval("const b = document.getElementById('fc-login-bar'); if(b) b.remove(); window.__FC_QUEUE__ = []; window.__FC_SEEN__.clear(); window.__FC_FETCHED_IDS__.clear(); window.location.reload();");
                    }
                    if let Some(main_win) = window.app_handle().get_webview_window("main") {
                        let _ = main_win.set_focus();
                    }
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            start_drag,
            start_ffmpeg,
            get_media_duration,
            scan_local_folder,
            extract_video_thumb,
            fetch_douban_rating,
            fetch_epg_data,
            fetch_weather_broadcast,
            get_douyin_feed,
            open_douyin_login,
            hide_douyin_login,
            quit_app,
            restart_app
        ])
        .run(tauri::generate_context!())
        .expect("运行 Tauri 应用失败");
}
