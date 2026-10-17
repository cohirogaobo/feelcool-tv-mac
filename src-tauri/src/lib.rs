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
    douyin_feed: Arc<Mutex<HashMap<String, Vec<DouyinItem>>>>,
    douyin_probing: Arc<Mutex<HashSet<String>>>,
    douyin_category: Arc<Mutex<String>>,
    bili_cookie: Arc<Mutex<String>>,
    bili_page_map: Arc<Mutex<HashMap<String, u32>>>,
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

fn apply_no_proxy_env(cmd: &mut Command) {
    cmd.env("no_proxy", "*")
        .env("NO_PROXY", "*")
        .env_remove("http_proxy")
        .env_remove("https_proxy")
        .env_remove("all_proxy")
        .env_remove("HTTP_PROXY")
        .env_remove("HTTPS_PROXY")
        .env_remove("ALL_PROXY");
}

fn strip_stream_meta_suffix(raw: &str) -> (&str, Option<f64>, Option<&str>) {
    let mut base = raw.trim();
    let mut dur_opt: Option<f64> = None;
    let mut title_opt: Option<&str> = None;
    if let Some(idx) = base.find("@@title=") {
        let t = &base[idx + 8..];
        title_opt = Some(t.split("@@").next().unwrap_or(t));
        base = &base[..idx];
    }
    if let Some(idx) = base.find("@@dur=") {
        let d_str = &base[idx + 6..];
        let d_clean = d_str.split("@@").next().unwrap_or(d_str);
        if let Ok(d) = d_clean.parse::<f64>() {
            if d.is_finite() && d > 0.0 {
                dur_opt = Some(d);
            }
        }
        base = &base[..idx];
    }
    (base.trim(), dur_opt, title_opt)
}

fn build_ffmpeg_args(
    actual_url: &str,
    is_local_file: bool,
    is_douyin: bool,
    is_bilibili: bool,
    use_sw_fallback: bool,
    skip_audio_filter: bool,
    seek_sec: Option<f64>,
    ts_pattern: &str,
    m3u8_path: &str,
) -> Vec<String> {
    let mut args = vec!["-v".to_string(), "error".to_string()];
    let v_codec = if use_sw_fallback { "libx264" } else { "h264_videotoolbox" };
    // 任务 1 (Round 5)：全链路音量均衡与峰值硬限幅滤镜，严控所有视频（尤其高响度抖音/流媒体视频）不超过上限
    let audio_norm_std = "acompressor=threshold=-18dB:ratio=3.5:attack=5:release=85:makeup=1,alimiter=limit=0.75:attack=5:release=50";
    let audio_norm_douyin = "acompressor=threshold=-20dB:ratio=5:attack=4:release=75:makeup=1,alimiter=limit=0.55:attack=4:release=45";
    let ss_val = match seek_sec {
        Some(s) if s >= 0.0 => format!("{:.2}", s),
        _ => "2".to_string(),
    };

    if is_local_file {
        args.extend(vec![
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
            "-g".to_string(), "30".to_string(),
            "-b:v".to_string(), "8M".to_string(),
        ]);
        if !skip_audio_filter {
            args.extend(vec!["-af".to_string(), audio_norm_std.to_string()]);
        }
        args.extend(vec![
            "-c:a".to_string(), "aac".to_string(),
            "-b:a".to_string(), "192k".to_string(),
            "-ar".to_string(), "44100".to_string(),
            "-ac".to_string(), "2".to_string(),
            "-f".to_string(), "hls".to_string(),
            "-hls_time".to_string(), "1".to_string(),
            "-hls_list_size".to_string(), "0".to_string(),
            "-hls_playlist_type".to_string(), "event".to_string(),
            "-hls_flags".to_string(), "independent_segments".to_string(),
            "-hls_segment_filename".to_string(), ts_pattern.to_string(),
            m3u8_path.to_string()
        ]);
    } else if is_douyin || is_bilibili {
        let ua = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/17.0 Safari/605.1.15";
        let header_lines = if is_bilibili {
            "Referer: https://www.bilibili.com/\r\nOrigin: https://www.bilibili.com\r\n"
        } else {
            "Referer: https://www.douyin.com/\r\nOrigin: https://www.douyin.com\r\n"
        };
        args.extend(vec![
            "-user_agent".to_string(), ua.to_string(),
            "-headers".to_string(), header_lines.to_string(),
            "-reconnect".to_string(), "1".to_string(),
            "-reconnect_streamed".to_string(), "1".to_string(),
            "-reconnect_delay_max".to_string(), "2".to_string(),
            "-rw_timeout".to_string(), "8000000".to_string(),
            "-seekable".to_string(), "1".to_string(),
            "-multiple_requests".to_string(), "1".to_string(),
            "-analyzeduration".to_string(), "1200000".to_string(),
            "-probesize".to_string(), "1500000".to_string(),
            "-fflags".to_string(), "+genpts+igndts+discardcorrupt".to_string(),
        ]);
        if let Some(s) = seek_sec {
            if s > 0.05 {
                args.extend(vec!["-ss".to_string(), format!("{:.2}", s)]);
            }
        }
        // 任务 2 (Round 17)：流媒体统一使用 VideoToolbox / libx264 重整 GOP（-g 30 -hls_time 1 -hls_flags independent_segments），彻底消除 -c:v copy 因源视频 7s 长 GOP / MP4 EditList 偏移导致的开头数秒黑屏假分片
        args.extend(vec![
            "-i".to_string(), actual_url.to_string(),
            "-map".to_string(), "0:v:0".to_string(),
            "-map".to_string(), "0:a:0?".to_string(),
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
            "-g".to_string(), "30".to_string(),
            "-b:v".to_string(), "10M".to_string(),
            "-maxrate".to_string(), "14M".to_string(),
            "-bufsize".to_string(), "20M".to_string(),
        ]);
        if !skip_audio_filter {
            args.extend(vec!["-af".to_string(), audio_norm_douyin.to_string()]);
        }
        args.extend(vec![
            "-c:a".to_string(), "aac".to_string(),
            "-b:a".to_string(), "160k".to_string(),
            "-ar".to_string(), "44100".to_string(),
            "-ac".to_string(), "2".to_string(),
            "-f".to_string(), "hls".to_string(),
            "-hls_time".to_string(), "1".to_string(),
            "-hls_list_size".to_string(), "0".to_string(),
            "-hls_playlist_type".to_string(), "event".to_string(),
            "-hls_flags".to_string(), "independent_segments".to_string(),
            "-hls_segment_filename".to_string(), ts_pattern.to_string(),
            m3u8_path.to_string()
        ]);
    } else {
        // 任务 3 (Round 17)：修复国内 IPTV 直播源黑屏（注入标准浏览器 User-Agent、移除导致 HLS 302/切片刷挂起的 -reconnect_streamed、支持 1080i 隔行扫描自动反交错 yadif 与可选音轨映射）
        let ua = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/17.0 Safari/605.1.15";
        args.extend(vec![
            "-user_agent".to_string(), ua.to_string(),
            "-reconnect".to_string(), "1".to_string(),
            "-reconnect_delay_max".to_string(), "2".to_string(),
            "-rw_timeout".to_string(), "8000000".to_string(),
            "-analyzeduration".to_string(), "2200000".to_string(),
            "-probesize".to_string(), "2200000".to_string(),
            "-fflags".to_string(), "+genpts+igndts+discardcorrupt".to_string(),
            "-i".to_string(), actual_url.to_string(),
            "-map".to_string(), "0:v:0".to_string(),
            "-map".to_string(), "0:a:0?".to_string(),
            "-vf".to_string(), "yadif=0:-1:1,scale=trunc(iw/2)*2:trunc(ih/2)*2,format=yuv420p".to_string(),
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
            "-g".to_string(), "30".to_string(),
        ]);
        if !skip_audio_filter {
            args.extend(vec!["-af".to_string(), audio_norm_std.to_string()]);
        }
        args.extend(vec![
            "-c:a".to_string(), "aac".to_string(),
            "-b:a".to_string(), "128k".to_string(),
            "-ar".to_string(), "44100".to_string(),
            "-ac".to_string(), "2".to_string(),
            "-f".to_string(), "hls".to_string(),
            "-hls_time".to_string(), "1".to_string(),
            "-hls_list_size".to_string(), "10".to_string(),
            "-hls_flags".to_string(), "delete_segments+independent_segments".to_string(),
            "-hls_segment_filename".to_string(), ts_pattern.to_string(),
            m3u8_path.to_string()
        ]);
    }

    args
}

fn probe_douyin_candidates(raw_candidates: &str) -> Option<(String, String, Option<f64>)> {
    let (clean_candidates, hint_dur, _) = strip_stream_meta_suffix(raw_candidates);
    let ffprobe_bin = get_ffprobe_bin();
    let ua = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/17.0 Safari/605.1.15";
    let headers = "Referer: https://www.douyin.com/\r\nOrigin: https://www.douyin.com\r\n";

    for cand in clean_candidates.split("@@") {
        let u = cand.trim();
        if !u.starts_with("https://") {
            continue;
        }
        let mut cmd = Command::new(ffprobe_bin);
        apply_no_proxy_env(&mut cmd);
        if let Ok(out) = cmd
            .args(&[
                "-v", "error",
                "-user_agent", ua,
                "-headers", headers,
                "-rw_timeout", "3200000",
                "-analyzeduration", "500000",
                "-probesize", "500000",
                "-select_streams", "v:0",
                "-show_entries", "stream=codec_name:format=duration",
                "-of", "default=noprint_wrappers=1",
                u,
            ])
            .output()
        {
            if out.status.success() {
                let text = String::from_utf8_lossy(&out.stdout);
                let mut codec = String::new();
                let mut probed_dur: Option<f64> = hint_dur;
                for line in text.lines() {
                    let l = line.trim();
                    if let Some(c) = l.strip_prefix("codec_name=") {
                        codec = c.trim().to_lowercase();
                    } else if let Some(d) = l.strip_prefix("duration=") {
                        if probed_dur.is_none() {
                            if let Ok(val) = d.trim().parse::<f64>() {
                                if val.is_finite() && val > 0.0 {
                                    probed_dur = Some(val);
                                }
                            }
                        }
                    }
                }
                if !codec.is_empty()
                    && codec != "none"
                    && !codec.contains("bvc2")
                    && !codec.contains("bytevc2")
                    && !codec.contains("av01")
                {
                    return Some((u.to_string(), codec, probed_dur));
                }
            }
        }
    }
    None
}

fn resolve_iptv_stream_candidates(raw_input: &str) -> Vec<(String, bool)> {
    let (primary_url, _, title_opt) = strip_stream_meta_suffix(raw_input);
    let title = title_opt.unwrap_or("").trim();
    let ua = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/17.0 Safari/605.1.15";

    let is_overseas_channel = title.contains("CNA")
        || title.contains("亚洲新闻")
        || title.contains("NTV")
        || primary_url.contains("mediacorp")
        || primary_url.contains("cloudfront.net")
        || primary_url.contains("akamaized.net")
        || primary_url.contains("ntv.co.jp");

    let mut out: Vec<(String, bool)> = Vec::new();
    let mut push_unique = |u: String, direct_no_proxy: bool| {
        let s = u.trim().to_string();
        if !s.is_empty() && !out.iter().any(|(x, np)| x == &s && *np == direct_no_proxy) {
            out.push((s, direct_no_proxy));
        }
    };

    // 针对中国国内 IPTV 源：先用 curl --noproxy "*" 解析 HTTP 302 跳转后的真实地址（解决中移动 PLTV / 广电 302 相对路径丢失问题）
    if !is_overseas_channel && primary_url.starts_with("http") && !primary_url.contains("://[") {
        let mut cmd = Command::new("/usr/bin/curl");
        apply_no_proxy_env(&mut cmd);
        cmd.args(&[
            "-sL",
            "--noproxy",
            "*",
            "-k",
            "--connect-timeout",
            "2",
            "--max-time",
            "3",
            "-A",
            ua,
            "-r",
            "0-1024",
            "-o",
            "/dev/null",
            "-w",
            "%{http_code}|||%{url_effective}",
            primary_url,
        ]);
        if let Ok(res) = cmd.output() {
            let txt = String::from_utf8_lossy(&res.stdout);
            let parts: Vec<&str> = txt.trim().split("|||").collect();
            let code = parts.first().and_then(|c| c.parse::<u16>().ok()).unwrap_or(0);
            let eff = parts.get(1).copied().unwrap_or("").trim();
            if (200..400).contains(&code) && eff.starts_with("http") {
                push_unique(eff.to_string(), true);
            }
        }
    }

    if !is_overseas_channel {
        push_unique(primary_url.to_string(), true);
        push_unique(primary_url.to_string(), false);
    } else {
        push_unique(primary_url.to_string(), false);
        push_unique(primary_url.to_string(), true);
    }

    out
}

fn resolve_bilibili_stream_candidates(raw_spec: &str, cookie: &str) -> (Vec<String>, Option<f64>) {
    let (clean_spec, suffix_dur, _) = strip_stream_meta_suffix(raw_spec);
    let clean = clean_spec
        .trim()
        .trim_start_matches("bilibili:")
        .trim_start_matches("douyin:")
        .trim_start_matches("bili:");
    let parts: Vec<&str> = clean.split("@@").collect();
    let bvid = parts.first().copied().unwrap_or("").trim();
    if bvid.is_empty() {
        return (Vec::new(), suffix_dur);
    }
    let mut cid: u64 = parts
        .get(1)
        .and_then(|s| s.trim().parse::<u64>().ok())
        .unwrap_or(0);
    let mut duration_opt: Option<f64> = parts
        .get(2)
        .and_then(|s| s.trim().parse::<f64>().ok())
        .filter(|d| *d > 0.0)
        .or(suffix_dur);

    let ua = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/17.0 Safari/605.1.15";
    let curl_bili_json = |api_url: &str| -> Option<serde_json::Value> {
        let mut cmd = Command::new("/usr/bin/curl");
        apply_no_proxy_env(&mut cmd);
        cmd.args(&[
            "-sL",
            "--noproxy",
            "*",
            "--compressed",
            "--connect-timeout",
            "4",
            "--max-time",
            "6",
            "-H",
            &format!("User-Agent: {}", ua),
            "-H",
            "Referer: https://www.bilibili.com/",
        ]);
        if !cookie.trim().is_empty() {
            cmd.args(&["-H", &format!("Cookie: {}", cookie.trim())]);
        }
        cmd.arg(api_url);
        let out = cmd.output().ok()?;
        if !out.status.success() {
            return None;
        }
        serde_json::from_slice::<serde_json::Value>(&out.stdout).ok()
    };

    if cid == 0 || duration_opt.is_none() {
        let view_url = format!("https://api.bilibili.com/x/web-interface/view?bvid={}", bvid);
        if let Some(vj) = curl_bili_json(&view_url) {
            if let Some(d) = vj.get("data") {
                if cid == 0 {
                    cid = d.get("cid").and_then(|v| v.as_u64()).unwrap_or(0);
                }
                if duration_opt.is_none() {
                    duration_opt = d.get("duration").and_then(|v| v.as_f64()).filter(|x| *x > 0.0);
                }
            }
        }
    }

    if cid == 0 {
        return (Vec::new(), duration_opt);
    }

    // 任务 6 (Round 16)：针对 bilibili 长视频（> 300s），优先请求分段式流式接口（fnval=1&platform=pc），避免单文件超大 moov 索引导致首包阻塞；同时保留 html5 高清源作为双保险
    let is_long_video = duration_opt.unwrap_or(0.0) > 300.0;
    let ep_html5 = format!(
        "https://api.bilibili.com/x/player/playurl?bvid={}&cid={}&qn=64&type=&otype=json&platform=html5&high_quality=1",
        bvid, cid
    );
    let ep_pc_stream = format!(
        "https://api.bilibili.com/x/player/playurl?bvid={}&cid={}&qn=64&fnval=1&platform=pc&otype=json",
        bvid, cid
    );
    let api_endpoints = if is_long_video {
        [ep_pc_stream, ep_html5]
    } else {
        [ep_html5, ep_pc_stream]
    };

    let mut urls: Vec<String> = Vec::new();
    for ep in &api_endpoints {
        if let Some(pj) = curl_bili_json(ep) {
            if let Some(d) = pj.get("data") {
                if duration_opt.is_none() {
                    if let Some(tl) = d.get("timelength").and_then(|v| v.as_f64()) {
                        if tl > 0.0 {
                            duration_opt = Some(tl / 1000.0);
                        }
                    }
                }
                if let Some(durl_arr) = d.get("durl").and_then(|v| v.as_array()) {
                    for item in durl_arr {
                        if let Some(u) = item.get("url").and_then(|v| v.as_str()) {
                            let s = u.trim().to_string();
                            if s.starts_with("http") && !urls.contains(&s) {
                                urls.push(s);
                            }
                        }
                        if let Some(b_arr) = item.get("backup_url").and_then(|v| v.as_array()) {
                            for b in b_arr {
                                if let Some(bu) = b.as_str() {
                                    let s = bu.trim().to_string();
                                    if s.starts_with("http") && !urls.contains(&s) {
                                        urls.push(s);
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        if !urls.is_empty() {
            break;
        }
    }

    (urls, duration_opt)
}

#[tauri::command]
async fn get_media_duration(url: String, state: State<'_, AppState>) -> Result<f64, String> {
    if let (_, Some(d), _) = strip_stream_meta_suffix(&url) {
        if d > 0.0 {
            return Ok(d);
        }
    }
    let bili_cookie = state.bili_cookie.lock().unwrap().clone();
    tauri::async_runtime::spawn_blocking(move || {
        if url.starts_with("bilibili:") || url.starts_with("douyin:bili:") || url.starts_with("bili:") {
            let (clean_spec, _, _) = strip_stream_meta_suffix(&url);
            let clean = clean_spec
                .trim()
                .trim_start_matches("bilibili:")
                .trim_start_matches("douyin:")
                .trim_start_matches("bili:");
            let parts: Vec<&str> = clean.split("@@").collect();
            if let Some(inline_dur) = parts.get(2).and_then(|s| s.trim().parse::<f64>().ok()) {
                if inline_dur.is_finite() && inline_dur > 0.0 {
                    return Ok(inline_dur);
                }
            }
            let (cands, api_dur) = resolve_bilibili_stream_candidates(&url, &bili_cookie);
            if let Some(d) = api_dur {
                if d.is_finite() && d > 0.0 {
                    return Ok(d);
                }
            }
            if let Some(first_u) = cands.first() {
                let ffprobe_bin = get_ffprobe_bin();
                let ua = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/17.0 Safari/605.1.15";
                let headers = "Referer: https://www.bilibili.com/\r\nOrigin: https://www.bilibili.com\r\n";
                let mut cmd = Command::new(ffprobe_bin);
                apply_no_proxy_env(&mut cmd);
                if let Ok(out) = cmd
                    .args(&[
                        "-v", "error",
                        "-user_agent", ua,
                        "-headers", headers,
                        "-rw_timeout", "5000000",
                        "-show_entries", "format=duration",
                        "-of", "default=noprint_wrappers=1:nokey=1",
                        first_u,
                    ])
                    .output()
                {
                    if out.status.success() {
                        if let Ok(dur) = String::from_utf8_lossy(&out.stdout).trim().parse::<f64>() {
                            if dur.is_finite() && dur > 0.0 {
                                return Ok(dur);
                            }
                        }
                    }
                }
            }
            return Err("无法获取 bilibili 视频时长".into());
        }

        let (clean_url, _, _) = strip_stream_meta_suffix(&url);
        let raw_url = if let Some(s) = clean_url.strip_prefix("douyin:h264:") {
            s.split("@@").next().unwrap_or(s).to_string()
        } else if let Some(s) = clean_url.strip_prefix("douyin:") {
            s.split("@@").next().unwrap_or(s).to_string()
        } else {
            clean_url.to_string()
        };
        let ffprobe_bin = get_ffprobe_bin();
        let mut cmd = Command::new(ffprobe_bin);
        apply_no_proxy_env(&mut cmd);
        cmd.args(&["-v", "error"]);
        if raw_url.starts_with("http") {
            let ua = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/17.0 Safari/605.1.15";
            let is_bili = raw_url.contains("bilivideo.com") || raw_url.contains("bilibili.com") || raw_url.contains("hdslb.com");
            let headers = if is_bili {
                "Referer: https://www.bilibili.com/\r\nOrigin: https://www.bilibili.com\r\n"
            } else {
                "Referer: https://www.douyin.com/\r\nOrigin: https://www.douyin.com\r\n"
            };
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
    let bili_cookie = state.bili_cookie.lock().unwrap().clone();

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

        let (clean_url, _, _) = strip_stream_meta_suffix(&url);
        let is_bili_input = clean_url.starts_with("bilibili:")
            || clean_url.starts_with("douyin:bili:")
            || clean_url.starts_with("bili:")
            || clean_url.contains("bilivideo.com");

        // (cand_url, use_sw_fallback, skip_audio_filter, force_no_proxy)
        let mut attempts: Vec<(String, bool, bool, bool)> = Vec::new();
        let mut is_douyin = false;
        let mut is_bilibili = false;

        if is_bili_input {
            is_bilibili = true;
            let cands = if clean_url.starts_with("http://") || clean_url.starts_with("https://") {
                vec![clean_url.to_string()]
            } else {
                let (c, _) = resolve_bilibili_stream_candidates(clean_url, &bili_cookie);
                c
            };
            for c in cands {
                attempts.push((c.clone(), false, false, true));
                attempts.push((c.clone(), true, false, true));
            }
        } else if let Some(stripped) = clean_url.strip_prefix("douyin:h264:").or_else(|| clean_url.strip_prefix("douyin:")) {
            is_douyin = true;
            let list: Vec<String> = stripped
                .split("@@")
                .filter(|s| !s.trim().is_empty() && !s.starts_with("dur=") && !s.starts_with("title="))
                .map(|s| s.trim().to_string())
                .collect();
            for c in list {
                attempts.push((c.clone(), false, false, true));
                attempts.push((c.clone(), true, false, true));
            }
        } else if clean_url.starts_with('/') {
            attempts.push((clean_url.to_string(), false, false, true));
            attempts.push((clean_url.to_string(), true, false, true));
        } else {
            let dy = clean_url.contains("douyin")
                || clean_url.contains("byte")
                || clean_url.contains("amemv")
                || clean_url.contains("zjcdn")
                || clean_url.contains("snssdk");
            if dy {
                is_douyin = true;
                for s in clean_url.split("@@").filter(|s| !s.trim().is_empty()) {
                    let c = s.trim().to_string();
                    attempts.push((c.clone(), false, false, true));
                    attempts.push((c.clone(), true, false, true));
                }
            } else {
                let iptv_cands = resolve_iptv_stream_candidates(&url);
                for (c, no_proxy) in iptv_cands {
                    attempts.push((c.clone(), false, false, no_proxy));
                    attempts.push((c.clone(), true, true, no_proxy));
                }
            }
        }

        if attempts.is_empty() {
            return Err("无效的视频地址".into());
        }

        let ffmpeg_bin = get_ffmpeg_bin();

        for (attempt_idx, (cand_url, use_sw, skip_af, force_no_proxy)) in attempts.iter().enumerate() {
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
                is_bilibili,
                *use_sw,
                *skip_af,
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
                let mut cmd = Command::new(ffmpeg_bin);
                if *force_no_proxy {
                    apply_no_proxy_env(&mut cmd);
                }
                match cmd.args(&args).spawn() {
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

            let max_ticks = if attempt_idx + 1 < attempts.len() { 48 } else { 88 };
            let mut exited_early = false;

            for _ in 0..max_ticks {
                if gen_arc.load(Ordering::SeqCst) != my_gen {
                    return Err("已切换新视频".into());
                }
                {
                    let mut process_guard = proc_arc.lock().unwrap();
                    if let Some((g, c)) = process_guard.as_mut() {
                        if *g == my_gen {
                            if let Ok(Some(status)) = c.try_wait() {
                                if status.success() {
                                    if let Ok(content) = fs::read_to_string(&m3u8_path) {
                                        if content.contains(".ts") {
                                            return Ok(format!("http://127.0.0.1:8765/{}", m3u8_filename));
                                        }
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
                if let Ok(content) = fs::read_to_string(&m3u8_path) {
                    if content.contains(".ts") {
                        return Ok(format!("http://127.0.0.1:8765/{}", m3u8_filename));
                    }
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
            let _ = fs::remove_file(&m3u8_path);
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

/// 规范化街道名称，确保带有“街道”或“镇”后缀（如“梅山” -> “梅山街道”，“梅山街道” -> “梅山街道”）
fn normalize_street_name(name: &str) -> String {
    let t = name.trim();
    if t.is_empty() {
        return String::new();
    }
    if t.ends_with("街道") || t.ends_with("镇") || t.ends_with("乡") {
        return t.to_string();
    }
    for suffix in &["办事处", "片区", "商圈", "地区"] {
        if let Some(s) = t.strip_suffix(suffix) {
            if !s.is_empty() {
                if s.ends_with("街道") || s.ends_with("镇") {
                    return s.to_string();
                }
                return format!("{}街道", s);
            }
        }
    }
    format!("{}街道", t)
}

/// 任务 6 (Round 10)：根据当前城市、区县、街道或经纬度，匹配当前位置所在街道与邻近的三个街道（共 4 个街道名称）
/// 例如南京市雨花台区梅山街道 -> “梅山街道 赛虹桥街道 铁心桥街道 西善桥街道”
fn resolve_four_nearby_streets(
    city: &str,
    current_dist: &str,
    current_street: &str,
    lat: Option<f64>,
    lon: Option<f64>,
) -> Vec<String> {
    let clean_city = strip_district_suffix(city);
    let clean_dist = strip_district_suffix(current_dist);
    let mut norm_street = normalize_street_name(current_street);

    // 南京各街道经纬度中心点（当定位接口仅返回坐标或区名时，可根据坐标精准匹配所在街道）
    let nanjing_street_coords: &[(&str, &str, f64, f64)] = &[
        ("雨花台", "梅山街道", 31.946, 118.672),
        ("雨花台", "西善桥街道", 31.966, 118.705),
        ("雨花台", "铁心桥街道", 31.972, 118.756),
        ("雨花台", "赛虹桥街道", 32.002, 118.758),
        ("雨花台", "雨花街道", 31.996, 118.784),
        ("雨花台", "板桥街道", 31.918, 118.662),
        ("雨花台", "古雄街道", 31.902, 118.636),
        ("玄武", "新街口街道", 32.048, 118.788),
        ("玄武", "玄武门街道", 32.071, 118.785),
        ("玄武", "梅园新村街道", 32.044, 118.805),
        ("玄武", "锁金村街道", 32.078, 118.812),
        ("玄武", "孝陵卫街道", 32.036, 118.858),
        ("玄武", "红山街道", 32.092, 118.806),
        ("秦淮", "夫子庙街道", 32.022, 118.788),
        ("秦淮", "五老村街道", 32.038, 118.795),
        ("秦淮", "洪武路街道", 32.031, 118.786),
        ("秦淮", "朝天宫街道", 32.035, 118.774),
        ("秦淮", "中华门街道", 32.009, 118.779),
        ("建邺", "兴隆街道", 32.018, 118.736),
        ("建邺", "莫愁湖街道", 32.034, 118.756),
        ("建邺", "南苑街道", 32.016, 118.752),
        ("建邺", "沙洲街道", 31.992, 118.725),
        ("鼓楼", "湖南路街道", 32.068, 118.776),
        ("鼓楼", "华侨路街道", 32.052, 118.774),
        ("鼓楼", "宁海路街道", 32.062, 118.765),
        ("鼓楼", "江东街道", 32.048, 118.738),
        ("栖霞", "仙林街道", 32.105, 118.926),
        ("栖霞", "马群街道", 32.054, 118.902),
        ("栖霞", "迈皋桥街道", 32.104, 118.816),
        ("栖霞", "尧化街道", 32.122, 118.868),
        ("江宁", "东山街道", 31.954, 118.842),
        ("江宁", "秣陵街道", 31.922, 118.818),
        ("江宁", "淳化街道", 31.942, 118.928),
        ("江宁", "麒麟街道", 32.024, 118.924),
        ("浦口", "江浦街道", 32.058, 118.628),
        ("浦口", "顶山街道", 32.105, 118.675),
        ("浦口", "泰山街道", 32.142, 118.712),
    ];

    if norm_street.is_empty() && clean_city == "南京" {
        if let (Some(la), Some(lo)) = (lat, lon) {
            let mut best_st = "";
            let mut best_dist_sq = f64::MAX;
            for (d_name, st_name, sla, slo) in nanjing_street_coords {
                if !clean_dist.is_empty() && *d_name != clean_dist {
                    continue;
                }
                let dsq = (la - sla).powi(2) + (lo - slo).powi(2);
                if dsq < best_dist_sq {
                    best_dist_sq = dsq;
                    best_st = st_name;
                }
            }
            if !best_st.is_empty() {
                norm_street = best_st.to_string();
            }
        }
    }

    // 任务 1 (Round 15)：原气象速报共显示四个街道，现在起改为只显示三个街道
    let yuhuatai_map: &[(&str, &[&str])] = &[
        ("梅山街道", &["梅山街道", "赛虹桥街道", "铁心桥街道"]),
        ("赛虹桥街道", &["赛虹桥街道", "雨花街道", "铁心桥街道"]),
        ("铁心桥街道", &["铁心桥街道", "赛虹桥街道", "雨花街道"]),
        ("西善桥街道", &["西善桥街道", "梅山街道", "赛虹桥街道"]),
        ("雨花街道", &["雨花街道", "赛虹桥街道", "铁心桥街道"]),
        ("板桥街道", &["板桥街道", "古雄街道", "梅山街道"]),
        ("古雄街道", &["古雄街道", "板桥街道", "梅山街道"]),
    ];

    if clean_dist == "雨花台" || norm_street == "梅山街道" || norm_street == "赛虹桥街道" || norm_street == "铁心桥街道" || norm_street == "西善桥街道" {
        for (k, list) in yuhuatai_map {
            if *k == norm_street {
                return list.iter().take(3).map(|s| s.to_string()).collect();
            }
        }
        return vec![
            "梅山街道".into(),
            "赛虹桥街道".into(),
            "铁心桥街道".into(),
        ];
    }

    // 各区县街道池：当前街道置顶，自动补齐同区邻近两个街道（共显示三个街道）
    let district_street_pools: &[(&str, &str, &[&str])] = &[
        ("南京", "玄武", &["新街口街道", "玄武门街道", "梅园新村街道", "锁金村街道", "孝陵卫街道", "红山街道", "玄武湖街道"]),
        ("南京", "秦淮", &["夫子庙街道", "五老村街道", "洪武路街道", "朝天宫街道", "中华门街道", "双塘街道", "月牙湖街道", "光华路街道"]),
        ("南京", "建邺", &["兴隆街道", "莫愁湖街道", "南苑街道", "沙洲街道", "双闸街道", "江心洲街道"]),
        ("南京", "鼓楼", &["湖南路街道", "华侨路街道", "宁海路街道", "中央门街道", "挹江门街道", "江东街道", "凤凰街道", "热河南路街道"]),
        ("南京", "栖霞", &["仙林街道", "马群街道", "迈皋桥街道", "尧化街道", "燕子矶街道", "栖霞街道", "龙潭街道", "西岗街道"]),
        ("南京", "雨花台", &["梅山街道", "赛虹桥街道", "铁心桥街道", "西善桥街道", "雨花街道", "板桥街道", "古雄街道"]),
        ("南京", "江宁", &["东山街道", "秣陵街道", "百家湖街道", "淳化街道", "麒麟街道", "江宁街道", "汤山街道", "禄口街道"]),
        ("南京", "浦口", &["江浦街道", "顶山街道", "泰山街道", "沿江街道", "桥林街道", "汤泉街道"]),
        ("南京", "六合", &["雄州街道", "龙池街道", "横梁街道", "金牛湖街道"]),
        ("南京", "溧水", &["永阳街道", "柘塘街道", "东屏街道", "石湫街道"]),
        ("南京", "高淳", &["淳溪街道", "古柏街道", "漆桥街道", "固城街道"]),
        ("北京", "朝阳", &["建外街道", "朝外街道", "呼家楼街道", "三里屯街道", "望京街道", "奥园街道"]),
        ("北京", "海淀", &["中关村街道", "海淀街道", "学院路街道", "万柳街道", "清河街道", "羊坊店街道"]),
        ("北京", "东城", &["东华门街道", "景山街道", "交道口街道", "安定门街道", "北新桥街道"]),
        ("北京", "西城", &["西长安街街道", "新街口街道", "月坛街道", "金融街街道", "什刹海街道"]),
        ("上海", "浦东", &["陆家嘴街道", "惟明街道", "塘桥街道", "洋泾街道", "花木街道", "张江镇"]),
        ("上海", "黄浦", &["外滩街道", "南京东路街道", "淮海中路街道", "瑞金二路街道", "豫园街道"]),
        ("上海", "徐汇", &["徐家汇街道", "天平路街道", "湖南路街道", "枫林路街道", "漕河泾街道"]),
        ("上海", "静安", &["静安寺街道", "南京西路街道", "江宁路街道", "曹家渡街道", "天目西路街道"]),
        ("广州", "天河", &["天河南街道", "林和街道", "猎德街道", "冼村街道", "石牌街道", "五山街道"]),
        ("广州", "越秀", &["北京街道", "光塔街道", "人民街道", "东山街道", "农林街道"]),
        ("深圳", "南山", &["粤海街道", "南头街道", "南山街道", "沙河街道", "蛇口街道", "西丽街道"]),
        ("深圳", "福田", &["福田街道", "华强北街道", "园岭街道", "南园街道", "沙头街道", "莲花街道"]),
        ("杭州", "西湖", &["北山街道", "西溪街道", "灵隐街道", "翠苑街道", "文新街道", "古荡街道"]),
        ("杭州", "上城", &["湖滨街道", "清波街道", "小营街道", "望江街道", "四季青街道"]),
        ("苏州", "姑苏", &["平江街道", "金阊街道", "沧浪街道", "双塔街道", "虎丘街道"]),
        ("无锡", "梁溪", &["崇安寺街道", "通江街道", "广瑞路街道", "迎龙桥街道", "南禅寺街道"]),
        ("常州", "天宁", &["雕庄街道", "青龙街道", "茶山街道", "红梅街道", "天宁街道"]),
        ("成都", "锦江", &["春熙路街道", "书院街街道", "合江亭街道", "牛市口街道", "沙河街道"]),
        ("武汉", "武昌", &["黄鹤楼街道", "粮道街街道", "中华路街道", "水果湖街道", "中南路街道"]),
        ("西安", "雁塔", &["小寨路街道", "大雁塔街道", "长延堡街道", "电子城街道", "曲江街道"]),
    ];

    let mut out = Vec::new();
    if !norm_street.is_empty() {
        out.push(norm_street.clone());
    }

    for (c_name, d_name, pool) in district_street_pools {
        if (*c_name == clean_city && *d_name == clean_dist)
            || (!norm_street.is_empty() && pool.iter().any(|s| *s == norm_street))
        {
            for st in *pool {
                if out.len() >= 3 {
                    break;
                }
                let st_str = st.to_string();
                if !out.contains(&st_str) {
                    out.push(st_str);
                }
            }
            break;
        }
    }

    if out.len() < 3 && clean_city == "南京" {
        for fallback_st in &["梅山街道", "赛虹桥街道", "铁心桥街道"] {
            if out.len() >= 3 {
                break;
            }
            let s = fallback_st.to_string();
            if !out.contains(&s) {
                out.push(s);
            }
        }
    }

    if out.len() < 3 {
        let base = if !clean_dist.is_empty() { &clean_dist } else if !clean_city.is_empty() { &clean_city } else { "中心" };
        let generic_fallbacks = [
            format!("{}街道", base),
            "新华街道".to_string(),
            "朝阳街道".to_string(),
        ];
        for g in &generic_fallbacks {
            if out.len() >= 3 {
                break;
            }
            if !out.contains(g) {
                out.push(g.clone());
            }
        }
    }

    out.into_iter().take(3).collect()
}

#[tauri::command]
async fn fetch_weather_broadcast(
    client_lat: Option<f64>,
    client_lon: Option<f64>,
    location_mode: Option<String>,
    manual_city: Option<String>,
    manual_district: Option<String>,
    manual_street: Option<String>,
    state: State<'_, AppState>,
) -> Result<WeatherBroadcastPayload, String> {
    let mc_400_arc = state.minutecast_400_until.clone();
    let was_precip_arc = state.was_precipitating.clone();

    tauri::async_runtime::spawn_blocking(move || {
        let api_key = "oLpVBp2pSM2J0ofeGZDdzfMPYePEbRZK";

        let is_china_coord = |la: f64, lo: f64| -> bool {
            la >= 16.0 && la <= 54.0 && lo >= 73.0 && lo <= 136.0
        };
        let is_china_country = |c: &str| -> bool {
            let t = c.trim();
            t.is_empty() || t == "中国" || t.eq_ignore_ascii_case("cn") || t.eq_ignore_ascii_case("china")
        };
        let is_foreign_place_name = |name: &str| -> bool {
            let t = name.trim();
            if t.is_empty() {
                return false;
            }
            let foreign_keywords = [
                "阿姆斯特丹", "荷兰", "美国", "弗吉尼亚", "阿什本", "洛杉矶", "圣何塞",
                "新加坡", "东京", "日本", "首尔", "韩国", "伦敦", "英国", "法兰克福", "德国",
            ];
            foreign_keywords.iter().any(|kw| t.contains(kw))
        };

        let m_city = manual_city.unwrap_or_default().trim().to_string();
        let m_dist = manual_district.unwrap_or_default().trim().to_string();
        let m_street = manual_street.unwrap_or_default().trim().to_string();
        let is_manual = location_mode.as_deref() == Some("manual")
            && (!m_city.is_empty() || !m_dist.is_empty() || !m_street.is_empty());

        let mut lat: Option<f64> = None;
        let mut lon: Option<f64> = None;
        let mut domestic_city = String::new();
        let mut domestic_district = String::new();
        let mut domestic_province = String::new();
        let mut domestic_street = String::new();

        if is_manual {
            domestic_city = m_city.clone();
            domestic_district = m_dist.clone();
            domestic_street = m_street.clone();
        } else {
            if let (Some(clat), Some(clon)) = (client_lat, client_lon) {
                if is_china_coord(clat, clon) {
                    lat = Some(clat);
                    lon = Some(clon);
                }
            }

            // 强制直连调用美团高精度位置接口（严格校验中国境内坐标与国家，拦截 VPN 海外出口如阿姆斯特丹）
            let (_, mt_json) = curl_get_json_direct("https://apimobile.meituan.com/locate/v2/ip/loc?rgeo=true");
            if let Some(mj) = mt_json {
                if let Some(d) = mj.get("data") {
                    let rg_opt = d.get("rgeo");
                    let country_str = rg_opt
                        .and_then(|rg| rg.get("country"))
                        .and_then(|v| v.as_str())
                        .unwrap_or("");
                    let city_str = rg_opt
                        .and_then(|rg| rg.get("city"))
                        .and_then(|v| v.as_str())
                        .unwrap_or("");
                    let coord_ok = match (d.get("lat").and_then(|v| v.as_f64()), d.get("lng").and_then(|v| v.as_f64())) {
                        (Some(la), Some(lo)) => is_china_coord(la, lo),
                        _ => false,
                    };
                    if coord_ok && is_china_country(country_str) && !is_foreign_place_name(city_str) {
                        if lat.is_none() {
                            if let (Some(la), Some(lo)) = (d.get("lat").and_then(|v| v.as_f64()), d.get("lng").and_then(|v| v.as_f64())) {
                                lat = Some(la);
                                lon = Some(lo);
                            }
                        }
                        if let Some(rg) = rg_opt {
                            if !city_str.is_empty() {
                                domestic_city = city_str.to_string();
                            }
                            if let Some(dist) = rg.get("district").and_then(|v| v.as_str()) {
                                domestic_district = dist.to_string();
                            }
                            if let Some(prov) = rg.get("province").and_then(|v| v.as_str()) {
                                domestic_province = prov.to_string();
                            }
                            for st_key in &["street", "township", "town", "areaName"] {
                                if let Some(st) = rg.get(*st_key).and_then(|v| v.as_str()) {
                                    if !st.trim().is_empty() {
                                        domestic_street = st.trim().to_string();
                                        break;
                                    }
                                }
                            }
                        }
                    }
                }
            }

            // 若有中国境内经纬度坐标，调用美团经纬度逆地理接口获取街道级定位信息
            if let (Some(la), Some(lo)) = (lat, lon) {
                if is_china_coord(la, lo) {
                    let mt_rev_url = format!(
                        "https://apimobile.meituan.com/group/v1/city/latlng/{:.5},{:.5}?tag=0",
                        la, lo
                    );
                    let (_, mt_rev_json) = curl_get_json_direct(&mt_rev_url);
                    if let Some(mrj) = mt_rev_json {
                        if let Some(d) = mrj.get("data") {
                            let c_name = d.get("city").and_then(|v| v.as_str()).unwrap_or("");
                            if !is_foreign_place_name(c_name) {
                                if domestic_city.is_empty() && !c_name.is_empty() {
                                    domestic_city = c_name.to_string();
                                }
                                if domestic_district.is_empty() {
                                    if let Some(dist) = d.get("district").and_then(|v| v.as_str()) {
                                        domestic_district = dist.to_string();
                                    }
                                }
                                if domestic_province.is_empty() {
                                    if let Some(prov) = d.get("province").and_then(|v| v.as_str()) {
                                        domestic_province = prov.to_string();
                                    }
                                }
                                if domestic_street.is_empty() {
                                    for st_key in &["areaName", "street", "township", "town"] {
                                        if let Some(st) = d.get(*st_key).and_then(|v| v.as_str()) {
                                            let st_trim = st.trim();
                                            if !st_trim.is_empty() && st_trim != domestic_district && st_trim != domestic_city {
                                                domestic_street = st_trim.to_string();
                                                break;
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }

            // 备选直连：哔哩哔哩国内 IP 定位接口（严格校验中国境内）
            if lat.is_none() || domestic_city.is_empty() {
                let (_, bili_json) = curl_get_json_direct("https://api.live.bilibili.com/client/v1/Ip/getInfoNew");
                if let Some(bj) = bili_json {
                    if let Some(d) = bj.get("data") {
                        let country_str = d.get("country").and_then(|v| v.as_str()).unwrap_or("");
                        let city_str = d.get("city").and_then(|v| v.as_str()).unwrap_or("");
                        if is_china_country(country_str) && !is_foreign_place_name(city_str) {
                            if lat.is_none() {
                                let la_opt = d.get("latitude").and_then(|v| v.as_f64().or_else(|| v.as_str().and_then(|s| s.parse::<f64>().ok())));
                                let lo_opt = d.get("longitude").and_then(|v| v.as_f64().or_else(|| v.as_str().and_then(|s| s.parse::<f64>().ok())));
                                if let (Some(la), Some(lo)) = (la_opt, lo_opt) {
                                    if is_china_coord(la, lo) {
                                        lat = Some(la);
                                        lon = Some(lo);
                                    }
                                }
                            }
                            if domestic_city.is_empty() && !city_str.is_empty() {
                                domestic_city = city_str.to_string();
                            }
                            if domestic_province.is_empty() {
                                if let Some(p) = d.get("province").and_then(|v| v.as_str()) {
                                    if !is_foreign_place_name(p) {
                                        domestic_province = p.to_string();
                                    }
                                }
                            }
                        }
                    }
                }
            }

            // 备选直连：腾讯位置接口（严格校验 country == "中国"）
            if domestic_city.is_empty() {
                let (_, qq_json) = curl_get_json_direct("https://r.inews.qq.com/api/ip2city");
                if let Some(qj) = qq_json {
                    let country_str = qj.get("country").and_then(|v| v.as_str()).unwrap_or("");
                    let city_str = qj.get("city").and_then(|v| v.as_str()).unwrap_or("");
                    if is_china_country(country_str) && !is_foreign_place_name(city_str) && !city_str.is_empty() {
                        domestic_city = city_str.to_string();
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
            }

            // 若因全局 VPN/TUN 代理导致所有 IP 定位均返回海外节点，优先回退用户保存的手动位置，否则回退默认国内城市（南京）
            if domestic_city.is_empty() && domestic_district.is_empty() && lat.is_none() {
                if !m_city.is_empty() || !m_dist.is_empty() || !m_street.is_empty() {
                    domestic_city = m_city;
                    domestic_district = m_dist;
                    domestic_street = m_street;
                } else {
                    domestic_city = "南京".to_string();
                    domestic_district = "雨花台区".to_string();
                    domestic_street = "梅山街道".to_string();
                }
            }
        }

        let mut loc_key = String::new();
        let mut admin_name = domestic_province.clone();

        if let (Some(la), Some(lo)) = (lat, lon) {
            if is_china_coord(la, lo) {
                let lat_round = (la * 1000.0).round() / 1000.0;
                let lon_round = (lo * 1000.0).round() / 1000.0;
                let q_coord = format!("{:.3},{:.3}", lat_round, lon_round);
                let geo_url = format!(
                    "https://openapi.weathercn.com/locations/v1/cities/geoposition/search.json?q={}&apikey={}&language=zh-cn&details=true",
                    q_coord, api_key
                );
                let (_, geo_json) = curl_get_json_direct(&geo_url);
                if let Some(gj) = geo_json {
                    let local_name = gj.get("LocalizedName").and_then(|v| v.as_str()).unwrap_or("");
                    if !is_foreign_place_name(local_name) {
                        if let Some(k) = gj.get("Key").and_then(|v| v.as_str()) {
                            loc_key = k.to_string();
                        }
                        if domestic_district.is_empty() && !local_name.is_empty() {
                            domestic_district = local_name.to_string();
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
                        if is_china_coord(la, lo) {
                            lat = Some(la);
                            lon = Some(lo);
                        }
                    }
                    if domestic_city.is_empty() {
                        if let Some(parent_name) = first_hit.get("ParentCity").and_then(|p| p.get("LocalizedName")).and_then(|v| v.as_str()) {
                            domestic_city = parent_name.to_string();
                        } else if let Some(ln) = first_hit.get("LocalizedName").and_then(|v| v.as_str()) {
                            domestic_city = ln.to_string();
                        }
                    }
                    if let Some(adm) = first_hit.get("AdministrativeArea").and_then(|a| a.get("LocalizedName")).and_then(|v| v.as_str()) {
                        if !adm.is_empty() {
                            admin_name = adm.to_string();
                        }
                    }
                }
            }
        }

        // 任务 1 (Round 15)：原气象速报共显示四个街道，现在起改为只显示三个街道
        let three_streets = resolve_four_nearby_streets(
            &domestic_city,
            &domestic_district,
            &domestic_street,
            lat,
            lon,
        );
        let areas_display = three_streets.into_iter().take(3).collect::<Vec<_>>().join(" ");

        // 任务 6 (Round 10)：第二行居右气象台署名由省级改为市级（如“南京市气象台”）
        let clean_city_name = domestic_city
            .trim()
            .trim_end_matches("市")
            .trim_end_matches("地区")
            .trim_end_matches("自治州")
            .to_string();
        let agency_name = if !clean_city_name.is_empty() {
            format!("{}市气象台", clean_city_name)
        } else {
            let clean_admin = admin_name
                .trim_end_matches("省")
                .trim_end_matches("市")
                .trim_end_matches("壮族自治区")
                .trim_end_matches("回族自治区")
                .trim_end_matches("维吾尔自治区")
                .trim_end_matches("自治区")
                .trim_end_matches("特别行政区")
                .to_string();
            if !clean_admin.is_empty() {
                format!("{}市气象台", clean_admin)
            } else {
                "南京市气象台".to_string()
            }
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
    feed_arc: Arc<Mutex<HashMap<String, Vec<DouyinItem>>>>,
    probing_arc: Arc<Mutex<HashSet<String>>>,
    cat_arc: Arc<Mutex<String>>,
) {
    if app.get_webview_window("douyin_scraper").is_some() {
        return;
    }

    let hook_script = r#"
        (function() {
            // 任务 1 (Round 17)：突破 macOS 隐藏 WKWebView (visible: false) 后台休眠节流与尺寸检测
            // 在隐藏窗口中原生 requestAnimationFrame 会被 WebKit 挂起导致抖音 React/Pace 首屏长久卡死，改用 16ms 定时器驱动
            try {
                Object.defineProperty(window, 'innerWidth', { get: () => 1920, configurable: true });
                Object.defineProperty(window, 'innerHeight', { get: () => 1080, configurable: true });
                Object.defineProperty(window, 'outerWidth', { get: () => 1920, configurable: true });
                Object.defineProperty(window, 'outerHeight', { get: () => 1080, configurable: true });
                Object.defineProperty(document, 'visibilityState', { get: () => 'visible', configurable: true });
                Object.defineProperty(document, 'hidden', { get: () => false, configurable: true });
                const origRaf = window.requestAnimationFrame;
                window.requestAnimationFrame = function(cb) {
                    if (document.getElementById('fc-login-bar') && origRaf) {
                        return origRaf.call(window, cb);
                    }
                    return setTimeout(() => {
                        try { cb(performance.now()); } catch (_) {}
                    }, 16);
                };
                window.cancelAnimationFrame = function(id) {
                    clearTimeout(id);
                };
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

            window.__FC_PAGE_START_TS__ = Date.now();
            window.__FC_LAST_LOAD_MORE_TS__ = 0;
            window.__FC_QUEUE__ = [];
            window.__FC_SEEN__ = new Set();

            function purgeDouyinFeedStorage() {
                try {
                    localStorage.removeItem('__fc_boot_seen_v1__');
                    try { sessionStorage.clear(); } catch (_) {}
                    for (let ki = localStorage.length - 1; ki >= 0; ki--) {
                        const k = localStorage.key(ki) || '';
                        if (/jingxuan|discover_feed|feed_cache|aweme_list|prefetch|recommend_cache|preload|water_fall/i.test(k)) {
                            localStorage.removeItem(k);
                        }
                    }
                } catch (_) {}
            }
            purgeDouyinFeedStorage();
            window.__FC_PURGE_STORAGE__ = purgeDouyinFeedStorage;

            setInterval(() => {
                if (window.__FC_QUEUE__.length > 0) {
                    const item = window.__FC_QUEUE__.shift();
                    const payload = item.map(s => encodeURIComponent(s)).join('|||');
                    try {
                        document.title = '__FC_ITEM__:' + payload + '|||' + Date.now();
                    } catch(e) {}
                }
            }, 35);

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
                if (/bytevc2|bvc2|av01/i.test(g)) return true;
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

                return candidates.slice(0, 3).join('@@');
            }

            function isExcludedDouyinUrl(u) {
                if (!u) return false;
                return /favorite|digg|like|collection|collect|history|visited|familiar|following|user\/post|aweme\/post|follow\/feed/i.test(String(u));
            }

            function recordAweme(aweme) {
                if (!aweme || !aweme.video) return;
                if (aweme.aweme_type === 68 || aweme.aweme_type === 101 || aweme.aweme_type === 2) return;
                if (aweme.images && Array.isArray(aweme.images) && aweme.images.length > 0) return;
                if (aweme.cell_room || aweme.live_reason || aweme.is_ads || aweme.raw_ad_data) return;
                // 任务 3 (Round 18)：排除用户曾经点过赞 / 收藏过的视频项，防止抖音网页端个人点赞区或收藏区视频混入推荐流造成重复内容
                if (
                    aweme.user_digged === 1 ||
                    aweme.user_digged === true ||
                    aweme.is_digged === 1 ||
                    aweme.is_digged === true ||
                    aweme.collect_stat === 1 ||
                    aweme.user_favorited === 1 ||
                    (aweme.status && (aweme.status.is_digged === 1 || aweme.status.is_digged === true))
                ) return;

                const v = aweme.video;
                let playUrl = pickBestPlayUrl(v);
                if (!playUrl) return;

                const rawDurMs = Number(v.duration || aweme.duration || 0);
                if (rawDurMs > 500) {
                    const durSec = (rawDurMs > 1000 ? (rawDurMs / 1000.0) : rawDurMs).toFixed(2);
                    playUrl = playUrl + '@@dur=' + durSec;
                }

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

            // 任务 10 (Round 17) & 任务 3 (Round 18)：仅扫描实时网络推荐流返回的数据结构，严禁扫描 DOM 静态预构建卡片、SSR 静态快照以及个人点赞/收藏/历史列表
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
                        if (
                            k === '_SSR_DATA' ||
                            k === 'renderData' ||
                            k === 'initialState' ||
                            k === 'favorite_list' ||
                            k === 'digg_list' ||
                            k === 'like_list' ||
                            k === 'collect_list' ||
                            k === 'history_list'
                        ) continue;
                        if (Object.prototype.hasOwnProperty.call(obj, k)) {
                            deepScan(obj[k], depth + 1);
                        }
                    }
                }
            }

            const origJsonParse = JSON.parse;
            const hookedJsonParse = function(text, reviver) {
                const res = origJsonParse.call(this, text, reviver);
                try {
                    if (typeof text === 'string' && text.length < 900000 && (text.includes('"aweme_list"') || text.includes('"cards"') || text.includes('"aweme_info"'))) {
                        const renderEl = document.getElementById('RENDER_DATA');
                        if (!renderEl || renderEl.textContent !== text) {
                            setTimeout(() => deepScan(res, 0), 0);
                        }
                    }
                } catch(e) {}
                return res;
            };
            hookedJsonParse.toString = () => 'function parse() { [native code] }';
            Object.defineProperty(JSON, 'parse', { value: hookedJsonParse, configurable: true, writable: true });

            const origResJson = Response.prototype.json;
            const hookedResJson = function() {
                const respUrl = (this && this.url) ? String(this.url) : '';
                return origResJson.apply(this, arguments).then(data => {
                    try {
                        if (!isExcludedDouyinUrl(respUrl)) {
                            setTimeout(() => deepScan(data, 0), 0);
                        }
                    } catch(e) {}
                    return data;
                });
            };
            hookedResJson.toString = () => 'function json() { [native code] }';
            Object.defineProperty(Response.prototype, 'json', { value: hookedResJson, configurable: true, writable: true });

            try {
                const origXhrOpen = XMLHttpRequest.prototype.open;
                const origXhrSend = XMLHttpRequest.prototype.send;
                XMLHttpRequest.prototype.open = function(method, url) {
                    this.__fc_url = String(url || '');
                    return origXhrOpen.apply(this, arguments);
                };
                XMLHttpRequest.prototype.send = function() {
                    this.addEventListener('load', function() {
                        try {
                            const u = this.__fc_url || '';
                            if (!isExcludedDouyinUrl(u) && /aweme|feed|tab|channel|discover|recommend/i.test(u)) {
                                if (this.responseType === '' || this.responseType === 'text') {
                                    const txt = this.responseText;
                                    if (txt && (txt.includes('play_addr') || txt.includes('aweme_list') || txt.includes('bit_rate') || txt.includes('cards'))) {
                                        deepScan(origJsonParse(txt), 0);
                                    }
                                } else if (this.responseType === 'json' && this.response) {
                                    deepScan(this.response, 0);
                                }
                            }
                        } catch (_) {}
                    });
                    return origXhrSend.apply(this, arguments);
                };
            } catch (_) {}

            try {
                if (window.fetch) {
                    const origFetch = window.fetch;
                    const hookedFetch = async function(input, init) {
                        const resp = await origFetch.apply(this, arguments);
                        try {
                            const reqUrl = (typeof input === 'string') ? input : (input && input.url ? input.url : '');
                            if (!isExcludedDouyinUrl(reqUrl) && (!reqUrl || /aweme|feed|tab|channel|discover|recommend/i.test(reqUrl))) {
                                const cloned = resp.clone();
                                cloned.text().then(txt => {
                                    if (txt && (txt.includes('play_addr') || txt.includes('aweme_list') || txt.includes('bit_rate') || txt.includes('cards'))) {
                                        try { deepScan(origJsonParse(txt), 0); } catch (_) {}
                                    }
                                }).catch(() => {});
                            }
                        } catch (_) {}
                        return resp;
                    };
                    hookedFetch.toString = () => 'function fetch() { [native code] }';
                    window.fetch = hookedFetch;
                }
            } catch (_) {}

            // 任务 1 (Round 17)：移除首屏加载期间 1.6s 误触发 window.location.reload() 的自杀式循环，仅在页面初始化完成后节流触发下拉续流
            window.__FC_LOAD_MORE__ = function(forceScroll) {
                const now = Date.now();
                if (!forceScroll && (now - window.__FC_PAGE_START_TS__ < 2600)) {
                    return;
                }
                if (!forceScroll && (now - (window.__FC_LAST_LOAD_MORE_TS__ || 0) < 1600)) {
                    return;
                }
                window.__FC_LAST_LOAD_MORE_TS__ = now;

                for (let step = 0; step < 3; step++) {
                    setTimeout(() => {
                        try {
                            if (window.__pc_pace_load_deferred_scripts) {
                                try { window.__pc_pace_load_deferred_scripts(); } catch(_) {}
                            } else if (window.__pace_load_deferred_scripts) {
                                try { window.__pace_load_deferred_scripts(); } catch(_) {}
                            }
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
                    }, step * 220);
                }
            };

            setTimeout(() => { if (window.__FC_SEEN__.size < 6 && window.__FC_LOAD_MORE__) window.__FC_LOAD_MORE__(true); }, 2800);

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

            window.addEventListener('contextmenu', function(e) {
                e.preventDefault();
                e.stopPropagation();
                return false;
            }, true);

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
                if (window.__pc_pace_load_deferred_scripts) {
                    try { window.__pc_pace_load_deferred_scripts(); } catch(_) {}
                } else if (window.__pace_load_deferred_scripts) {
                    try { window.__pace_load_deferred_scripts(); } catch(_) {}
                }
            }, 450);
        })();
    "#;

    let boot_ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(1);
    let init_url_str = format!("https://www.douyin.com/discover?_fc_boot={}", boot_ts);
    let parsed_url: tauri::Url = init_url_str.parse().unwrap();
    let modern_ua = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/128.0.0.0 Safari/537.36";
    let _ = WebviewWindowBuilder::new(app, "douyin_scraper", WebviewUrl::External(parsed_url))
        .title("抖音账号登录")
        .user_agent(modern_ua)
        .inner_size(1180.0, 760.0)
        .visible(false)
        .focused(false)
        .initialization_script(hook_script)
        .on_document_title_changed(move |win, title| {
            if title == "__FC_LOGIN_DONE__" {
                let _ = win.set_always_on_top(false);
                let _ = win.hide();
                feed_arc.lock().unwrap().retain(|k, _| k.starts_with("bili_"));
                probing_arc.lock().unwrap().clear();
                let _ = win.eval("if(window.__FC_PURGE_STORAGE__) window.__FC_PURGE_STORAGE__(); window.__FC_PAGE_START_TS__ = Date.now(); window.__FC_QUEUE__ = []; window.__FC_SEEN__.clear(); window.location.reload();");
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
                    let (clean_candidates, _, _) = strip_stream_meta_suffix(&play_url);
                    let primary_url = clean_candidates.split("@@").next().unwrap_or(clean_candidates).to_string();
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

                    // 任务 9 (Round 17)：恢复严格的 ffprobe 编码预检门禁，仅当确认候选地址可解码且非 ByteVC2 (bvc2) 时才推入推荐流，彻底杜绝不可播放卡片混入列表
                    let active_cat = cat_arc.lock().unwrap().clone();
                    let feed_clone = feed_arc.clone();
                    tauri::async_runtime::spawn_blocking(move || {
                        if let Some((verified_url, codec, dur_opt)) = probe_douyin_candidates(&play_url) {
                            let mut tagged_url = if codec == "h264" {
                                format!("h264:{}", verified_url)
                            } else {
                                verified_url.clone()
                            };
                            if let Some(d) = dur_opt {
                                if d > 0.5 {
                                    tagged_url = format!("{}@@dur={:.2}", tagged_url, d);
                                }
                            }
                            let mut feed_map = feed_clone.lock().unwrap();
                            let cat_list = feed_map.entry(active_cat).or_insert_with(Vec::new);
                            if !cat_list.iter().any(|x| x.3.contains(&verified_url)) {
                                cat_list.push((desc, author, cover, tagged_url));
                            }
                        }
                    });
                }
            }
        })
        .build();
}

fn normalize_bili_cover(raw: &str) -> String {
    let trimmed = raw.trim();
    if trimmed.starts_with("//") {
        format!("https:{}", trimmed)
    } else if trimmed.starts_with("http://") {
        format!("https://{}", &trimmed[7..])
    } else {
        trimmed.to_string()
    }
}

fn parse_bili_item_node(item: &serde_json::Value) -> Option<DouyinItem> {
    let bvid = item
        .get("bvid")
        .and_then(|v| v.as_str())
        .or_else(|| item.get("share_copy").and_then(|v| v.as_str()).filter(|s| s.starts_with("BV")))
        .unwrap_or("")
        .trim();
    if bvid.is_empty() {
        return None;
    }
    let cid = item.get("cid").and_then(|v| v.as_u64()).unwrap_or(0);
    let duration = item.get("duration").and_then(|v| v.as_u64()).unwrap_or(0);
    let raw_title = item
        .get("title")
        .and_then(|v| v.as_str())
        .unwrap_or("bilibili 视频")
        .replace("<em>", "")
        .replace("</em>", "")
        .trim()
        .to_string();
    let title = if raw_title.chars().count() > 70 {
        raw_title.chars().take(70).collect()
    } else {
        raw_title
    };
    let owner_name = item
        .get("owner")
        .and_then(|o| o.get("name"))
        .and_then(|v| v.as_str())
        .or_else(|| item.get("author").and_then(|v| v.as_str()))
        .unwrap_or("bilibili UP主")
        .trim();
    let author = if owner_name.starts_with('@') {
        owner_name.to_string()
    } else {
        format!("@{}", owner_name)
    };
    let raw_pic = item
        .get("pic")
        .and_then(|v| v.as_str())
        .or_else(|| item.get("cover").and_then(|v| v.as_str()))
        .unwrap_or("");
    let cover = normalize_bili_cover(raw_pic);
    if cover.is_empty() {
        return None;
    }
    let play_spec = format!("bili:{}@@{}@@{}", bvid, cid, duration);
    Some((title, author, cover, play_spec))
}

fn fetch_bilibili_category_items(cat: &str, cookie: &str, page: u32) -> Vec<DouyinItem> {
    let ua = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/17.0 Safari/605.1.15";
    let curl_json = |api_url: &str| -> Option<serde_json::Value> {
        let mut cmd = Command::new("/usr/bin/curl");
        cmd.args(&[
            "-sL",
            "--noproxy",
            "*",
            "--compressed",
            "--connect-timeout",
            "4",
            "--max-time",
            "6",
            "-H",
            &format!("User-Agent: {}", ua),
            "-H",
            "Referer: https://www.bilibili.com/",
        ]);
        if !cookie.trim().is_empty() {
            cmd.args(&["-H", &format!("Cookie: {}", cookie.trim())]);
        }
        cmd.arg(api_url);
        let out = cmd.output().ok()?;
        if !out.status.success() {
            return None;
        }
        serde_json::from_slice::<serde_json::Value>(&out.stdout).ok()
    };

    let mut urls: Vec<String> = Vec::new();
    match cat {
        "bili_rcmd" => {
            urls.push(format!(
                "https://api.bilibili.com/x/web-interface/index/top/rcmd?fresh_type=3&version=1&ps=14&fresh_idx={}&fresh_idx_1h={}",
                page, page
            ));
            urls.push(format!(
                "https://api.bilibili.com/x/web-interface/wbi/index/top/feed/rcmd?ps=16&fresh_idx={}",
                page
            ));
            urls.push(format!(
                "https://api.bilibili.com/x/web-interface/popular?ps=20&pn={}",
                page
            ));
        }
        "bili_popular" => {
            urls.push(format!(
                "https://api.bilibili.com/x/web-interface/popular?ps=20&pn={}",
                page
            ));
            urls.push("https://api.bilibili.com/x/web-interface/ranking/v2?rid=0&type=all".to_string());
        }
        _ => {
            let rid = match cat {
                "bili_anime" => 1,
                "bili_bangumi" => 33,
                "bili_cinephile" => 181,
                "bili_game" => 4,
                "bili_knowledge" => 36,
                "bili_tech" => 188,
                "bili_music" => 3,
                "bili_food" => 211,
                "bili_dance" => 129,
                "bili_sports" => 234,
                _ => 1,
            };
            urls.push(format!(
                "https://api.bilibili.com/x/web-interface/dynamic/region?ps=20&pn={}&rid={}",
                page, rid
            ));
            urls.push(format!(
                "https://api.bilibili.com/x/web-interface/ranking/v2?rid={}&type=all",
                rid
            ));
        }
    }

    let mut items: Vec<DouyinItem> = Vec::new();
    for u in &urls {
        if let Some(j) = curl_json(u) {
            if let Some(d) = j.get("data") {
                if let Some(arr) = d
                    .get("item")
                    .and_then(|v| v.as_array())
                    .or_else(|| d.get("archives").and_then(|v| v.as_array()))
                    .or_else(|| d.get("list").and_then(|v| v.as_array()))
                {
                    for node in arr {
                        if let Some(parsed) = parse_bili_item_node(node) {
                            if !items.iter().any(|x| x.3 == parsed.3) {
                                items.push(parsed);
                            }
                        }
                    }
                }
            }
        }
        if items.len() >= 12 {
            break;
        }
    }
    items
}

fn ensure_bilibili_scraper(
    app: &AppHandle,
    cookie_arc: Arc<Mutex<String>>,
    feed_arc: Arc<Mutex<HashMap<String, Vec<DouyinItem>>>>,
) {
    if app.get_webview_window("bilibili_scraper").is_some() {
        return;
    }

    // 任务 2 (Round 18)：注入桌面版现代 Chrome 128 User-Agent 与版本特征伪装，拦截 fe-activity-low-version 低版本提示页并自动引导至标准登录页
    let bili_hook_script = r#"
        (function() {
            try {
                const modernUa = 'Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/128.0.0.0 Safari/537.36';
                Object.defineProperty(navigator, 'userAgent', { get: () => modernUa, configurable: true });
                Object.defineProperty(navigator, 'appVersion', { get: () => '5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/128.0.0.0 Safari/537.36', configurable: true });
                Object.defineProperty(navigator, 'vendor', { get: () => 'Google Inc.', configurable: true });
                if (!window.chrome) {
                    window.chrome = { runtime: {} };
                }
            } catch (e) {}

            function checkLowVersionRedirect() {
                try {
                    const href = String(window.location.href || '');
                    const bodyTxt = (document.body && document.body.innerText) ? document.body.innerText : '';
                    if (href.includes('low-version') || href.includes('unsupport') || bodyTxt.includes('浏览器版本过低')) {
                        window.location.replace('https://passport.bilibili.com/login');
                        return true;
                    }
                } catch (e) {}
                return false;
            }

            checkLowVersionRedirect();

            window.__FC_SYNC_BILI_COOKIE__ = function() {
                try {
                    const ck = document.cookie || '';
                    if (ck && (ck.includes('SESSDATA') || ck.includes('bili_jct') || ck.includes('DedeUserID') || ck.includes('buvid3'))) {
                        document.title = '__FC_BILI_COOKIE__:' + encodeURIComponent(ck) + '|||' + Date.now();
                    }
                } catch(e) {}
            };

            setInterval(() => {
                if (checkLowVersionRedirect()) return;
                if (!document.getElementById('fc-login-bar')) {
                    document.querySelectorAll('video, audio').forEach(el => { el.muted = true; el.volume = 0; });
                }
                if (window.__FC_SYNC_BILI_COOKIE__) window.__FC_SYNC_BILI_COOKIE__();
            }, 1200);

            window.__FC_SHOW_LOGIN_BAR__ = function() {
                if (checkLowVersionRedirect()) return;
                if (document.getElementById('fc-login-bar')) return;
                const bar = document.createElement('div');
                bar.id = 'fc-login-bar';
                bar.style.cssText = 'position:fixed;top:16px;left:50%;transform:translateX(-50%);z-index:2147483647;background:#0A84FF;color:#fff;padding:10px 28px;border-radius:999px;font-family:-apple-system,sans-serif;font-size:15px;font-weight:600;cursor:pointer;box-shadow:0 10px 30px rgba(0,0,0,0.6);border:1.5px solid rgba(255,255,255,0.4);';
                bar.innerText = '✅ 完成登录并返回 Feel Cool TV (或按 Esc / 关闭窗口)';
                bar.onclick = function() {
                    bar.remove();
                    try {
                        document.title = '__FC_BILI_LOGIN_DONE__:' + encodeURIComponent(document.cookie || '');
                    } catch(e) {
                        document.title = '__FC_BILI_LOGIN_DONE__:';
                    }
                };
                if (document.body) document.body.appendChild(bar);
            };

            window.addEventListener('contextmenu', function(e) {
                e.preventDefault();
                e.stopPropagation();
                return false;
            }, true);

            window.addEventListener('keydown', function(e) {
                if (e.key === 'Escape' && document.getElementById('fc-login-bar')) {
                    document.getElementById('fc-login-bar').remove();
                    try {
                        document.title = '__FC_BILI_LOGIN_DONE__:' + encodeURIComponent(document.cookie || '');
                    } catch(err) {
                        document.title = '__FC_BILI_LOGIN_DONE__:';
                    }
                }
            }, true);
        })();
    "#;

    let parsed_url: tauri::Url = "https://passport.bilibili.com/login".parse().unwrap();
    let modern_ua = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/128.0.0.0 Safari/537.36";
    let _ = WebviewWindowBuilder::new(app, "bilibili_scraper", WebviewUrl::External(parsed_url))
        .title("bilibili 账号登录")
        .user_agent(modern_ua)
        .inner_size(1180.0, 760.0)
        .visible(false)
        .focused(false)
        .initialization_script(bili_hook_script)
        .on_document_title_changed(move |win, title| {
            if let Some(ck_part) = title.strip_prefix("__FC_BILI_LOGIN_DONE__:") {
                let raw_ck = url_decode(ck_part.split("|||").next().unwrap_or(ck_part));
                if !raw_ck.trim().is_empty() {
                    *cookie_arc.lock().unwrap() = raw_ck;
                }
                let _ = win.set_always_on_top(false);
                let _ = win.hide();
                {
                    let mut fmap = feed_arc.lock().unwrap();
                    fmap.retain(|k, _| !k.starts_with("bili_"));
                }
                if let Some(main_win) = win.app_handle().get_webview_window("main") {
                    let _ = main_win.set_focus();
                }
            } else if let Some(ck_part) = title.strip_prefix("__FC_BILI_COOKIE__:") {
                let raw_ck = url_decode(ck_part.split("|||").next().unwrap_or(ck_part));
                if !raw_ck.trim().is_empty() {
                    *cookie_arc.lock().unwrap() = raw_ck;
                }
            }
        })
        .build();
}

// 任务 7 (Round 11) & 任务 3 (Round 15)：按分类独立缓存抖音与 bilibili 推荐流，切换分类时绝不清空其他分类已加载的推荐流
#[tauri::command]
async fn get_douyin_feed(
    app: AppHandle,
    refresh: bool,
    load_more: Option<bool>,
    category: Option<String>,
    state: State<'_, AppState>,
) -> Result<Vec<DouyinItem>, String> {
    let req_cat = category.unwrap_or_else(|| "discover".to_string());

    if req_cat.starts_with("bili_") {
        ensure_bilibili_scraper(
            &app,
            state.bili_cookie.clone(),
            state.douyin_feed.clone(),
        );
        let want_more = load_more.unwrap_or(false);
        let feed_arc = state.douyin_feed.clone();
        let page_arc = state.bili_page_map.clone();
        let cookie_str = state.bili_cookie.lock().unwrap().clone();
        let cat_key = req_cat.clone();

        if refresh {
            {
                let mut feed_map = feed_arc.lock().unwrap();
                feed_map.remove(&cat_key);
            }
            let next_page = {
                let mut pmap = page_arc.lock().unwrap();
                let p = pmap.entry(cat_key.clone()).or_insert(0);
                *p = (*p % 12) + 1;
                *p
            };
            let fetched = tauri::async_runtime::spawn_blocking(move || {
                fetch_bilibili_category_items(&cat_key, &cookie_str, next_page)
            })
            .await
            .unwrap_or_default();

            if !fetched.is_empty() {
                let mut feed_map = state.douyin_feed.lock().unwrap();
                feed_map.insert(req_cat.clone(), fetched.clone());
            }
            return Ok(fetched);
        }

        let existing = {
            let feed_map = state.douyin_feed.lock().unwrap();
            feed_map.get(&req_cat).cloned().unwrap_or_default()
        };

        if existing.is_empty() || want_more {
            let next_page = {
                let mut pmap = page_arc.lock().unwrap();
                let p = pmap.entry(cat_key.clone()).or_insert(0);
                *p = (*p % 15) + 1;
                *p
            };
            let fetched = tauri::async_runtime::spawn_blocking(move || {
                fetch_bilibili_category_items(&cat_key, &cookie_str, next_page)
            })
            .await
            .unwrap_or_default();

            if !fetched.is_empty() {
                let mut feed_map = state.douyin_feed.lock().unwrap();
                let list = feed_map.entry(req_cat.clone()).or_insert_with(Vec::new);
                for item in fetched {
                    if !list.iter().any(|x| x.3 == item.3) {
                        list.push(item);
                    }
                }
                return Ok(list.clone());
            }
        }

        return Ok(existing);
    }

    ensure_douyin_scraper(
        &app,
        state.douyin_feed.clone(),
        state.douyin_probing.clone(),
        state.douyin_category.clone(),
    );

    if refresh {
        {
            let mut cur_cat = state.douyin_category.lock().unwrap();
            *cur_cat = req_cat.clone();
        }
        {
            let mut feed_map = state.douyin_feed.lock().unwrap();
            feed_map.remove(&req_cat);
        }
        {
            let mut p_set = state.douyin_probing.lock().unwrap();
            p_set.clear();
        }
        let base_url = category_to_douyin_url(&req_cat);
        let ts = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis())
            .unwrap_or(1);
        let sep = if base_url.contains('?') { "&" } else { "?" };
        let target_url = format!("{}{}_fc_t={}", base_url, sep, ts);
        if let Some(win) = app.get_webview_window("douyin_scraper") {
            let js = format!(
                "if(window.__FC_PURGE_STORAGE__) window.__FC_PURGE_STORAGE__(); window.__FC_PAGE_START_TS__ = Date.now(); window.__FC_QUEUE__ = []; window.__FC_SEEN__.clear(); window.location.href = '{}';",
                target_url
            );
            let _ = win.eval(&js);
        }
        return Ok(Vec::new());
    }

    let existing = {
        let feed_map = state.douyin_feed.lock().unwrap();
        feed_map.get(&req_cat).cloned().unwrap_or_default()
    };

    let want_more = load_more.unwrap_or(false);
    if existing.is_empty() || want_more {
        let mut cat_switched = false;
        {
            let mut cur_cat = state.douyin_category.lock().unwrap();
            if *cur_cat != req_cat {
                *cur_cat = req_cat.clone();
                cat_switched = true;
            }
        }
        if cat_switched {
            let base_url = category_to_douyin_url(&req_cat);
            let ts = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_millis())
                .unwrap_or(1);
            let sep = if base_url.contains('?') { "&" } else { "?" };
            let target_url = format!("{}{}_fc_t={}", base_url, sep, ts);
            if let Some(win) = app.get_webview_window("douyin_scraper") {
                let js = format!(
                    "if(window.__FC_PURGE_STORAGE__) window.__FC_PURGE_STORAGE__(); window.__FC_PAGE_START_TS__ = Date.now(); window.__FC_QUEUE__ = []; window.__FC_SEEN__.clear(); window.location.href = '{}';",
                    target_url
                );
                let _ = win.eval(&js);
            }
        } else if let Some(win) = app.get_webview_window("douyin_scraper") {
            let force_arg = if want_more && !existing.is_empty() { "true" } else { "false" };
            let _ = win.eval(&format!("if(window.__FC_LOAD_MORE__) window.__FC_LOAD_MORE__({});", force_arg));
        }
    }

    Ok(existing)
}

#[tauri::command]
async fn open_douyin_login(app: AppHandle, state: State<'_, AppState>) -> Result<(), String> {
    ensure_douyin_scraper(
        &app,
        state.douyin_feed.clone(),
        state.douyin_probing.clone(),
        state.douyin_category.clone(),
    );
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
        state.douyin_feed.lock().unwrap().retain(|k, _| k.starts_with("bili_"));
        state.douyin_probing.lock().unwrap().clear();
        let _ = win.eval("const b = document.getElementById('fc-login-bar'); if(b) b.remove(); if(window.__FC_PURGE_STORAGE__) window.__FC_PURGE_STORAGE__(); window.__FC_PAGE_START_TS__ = Date.now(); window.__FC_QUEUE__ = []; window.__FC_SEEN__.clear(); window.location.reload();");
    }
    if let Some(main_win) = app.get_webview_window("main") {
        let _ = main_win.set_focus();
    }
    Ok(())
}

#[tauri::command]
async fn open_bilibili_login(app: AppHandle, state: State<'_, AppState>) -> Result<(), String> {
    ensure_bilibili_scraper(
        &app,
        state.bili_cookie.clone(),
        state.douyin_feed.clone(),
    );
    if let Some(win) = app.get_webview_window("bilibili_scraper") {
        let _ = win.show();
        let _ = win.center();
        let _ = win.set_always_on_top(true);
        let _ = win.set_focus();
        let _ = win.eval("if (String(window.location.href || '').includes('low-version') || (document.body && document.body.innerText && document.body.innerText.includes('浏览器版本过低'))) { window.location.replace('https://passport.bilibili.com/login'); } setTimeout(() => { if(window.__FC_SHOW_LOGIN_BAR__) window.__FC_SHOW_LOGIN_BAR__(); }, 200);");
    }
    Ok(())
}

#[tauri::command]
async fn hide_bilibili_login(app: AppHandle, state: State<'_, AppState>) -> Result<(), String> {
    if let Some(win) = app.get_webview_window("bilibili_scraper") {
        let _ = win.eval("if(window.__FC_SYNC_BILI_COOKIE__) window.__FC_SYNC_BILI_COOKIE__(); const b = document.getElementById('fc-login-bar'); if(b) b.remove();");
        let _ = win.set_always_on_top(false);
        let _ = win.hide();
        state.douyin_feed.lock().unwrap().retain(|k, _| !k.starts_with("bili_"));
    }
    if let Some(main_win) = app.get_webview_window("main") {
        let _ = main_win.set_focus();
    }
    Ok(())
}

// 任务 12 (Round 5) & 任务 4 (Round 16)：电源管理——彻底清理子进程并安全关闭退出或无白屏热重启应用
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
    state.ffmpeg_generation.fetch_add(1, Ordering::SeqCst);
    if let Ok(mut guard) = state.ffmpeg_process.lock() {
        if let Some((_, mut child)) = guard.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
    cleanup_orphan_hls_ffmpeg();
    if let Ok(entries) = fs::read_dir("/tmp/cooltv_hls") {
        for entry in entries.flatten() {
            let p = entry.path();
            if p.is_file() {
                let _ = fs::remove_file(p);
            }
        }
    }
    if let Ok(mut fmap) = state.douyin_feed.lock() {
        fmap.clear();
    }
    if let Ok(mut pset) = state.douyin_probing.lock() {
        pset.clear();
    }
    if let Ok(mut cat) = state.douyin_category.lock() {
        *cat = "discover".to_string();
    }
    if let Some(dy_win) = app.get_webview_window("douyin_scraper") {
        let _ = dy_win.set_always_on_top(false);
        let _ = dy_win.hide();
        let ts = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis()).unwrap_or(1);
        let _ = dy_win.eval(&format!(
            "if(window.__FC_PURGE_STORAGE__) window.__FC_PURGE_STORAGE__(); window.__FC_PAGE_START_TS__ = Date.now(); window.__FC_QUEUE__ = []; window.__FC_SEEN__.clear(); window.location.href = 'https://www.douyin.com/discover?_fc_boot={}';",
            ts
        ));
    }
    if let Some(bili_win) = app.get_webview_window("bilibili_scraper") {
        let _ = bili_win.set_always_on_top(false);
        let _ = bili_win.hide();
    }
    if let Some(main_win) = app.get_webview_window("main") {
        let _ = main_win.eval("window.location.reload();");
        let _ = main_win.set_focus();
    }
    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    cleanup_orphan_hls_ffmpeg();

    let shared_douyin_feed: Arc<Mutex<HashMap<String, Vec<DouyinItem>>>> = Arc::new(Mutex::new(HashMap::new()));
    let shared_douyin_probing: Arc<Mutex<HashSet<String>>> = Arc::new(Mutex::new(HashSet::new()));
    let shared_douyin_cat: Arc<Mutex<String>> = Arc::new(Mutex::new("discover".to_string()));
    let shared_bili_cookie: Arc<Mutex<String>> = Arc::new(Mutex::new(String::new()));
    let setup_feed = shared_douyin_feed.clone();
    let setup_probing = shared_douyin_probing.clone();
    let setup_cat = shared_douyin_cat.clone();

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
            douyin_category: shared_douyin_cat,
            bili_cookie: shared_bili_cookie,
            bili_page_map: Arc::new(Mutex::new(HashMap::new())),
            thumb_lock: Arc::new(Mutex::new(())),
            minutecast_400_until: Arc::new(AtomicU64::new(0)),
            was_precipitating: Arc::new(Mutex::new(false)),
        })
        .setup(move |app| {
            if let Some(main_win) = app.get_webview_window("main") {
                let _ = main_win.set_title("");
                let _ = main_win.set_title_bar_style(TitleBarStyle::Overlay);
            }
            ensure_douyin_scraper(app.handle(), setup_feed.clone(), setup_probing.clone(), setup_cat.clone());
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
            } else if window.label() == "bilibili_scraper" {
                if let WindowEvent::CloseRequested { api, .. } = event {
                    api.prevent_close();
                    let _ = window.set_always_on_top(false);
                    let _ = window.hide();
                    if let Some(wv) = window.app_handle().get_webview_window("bilibili_scraper") {
                        let _ = wv.eval("if(window.__FC_SYNC_BILI_COOKIE__) window.__FC_SYNC_BILI_COOKIE__(); const b = document.getElementById('fc-login-bar'); if(b) b.remove();");
                    }
                    if let Some(state) = window.app_handle().try_state::<AppState>() {
                        if let Ok(mut fmap) = state.douyin_feed.lock() {
                            fmap.retain(|k, _| !k.starts_with("bili_"));
                        }
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
            open_bilibili_login,
            hide_bilibili_login,
            quit_app,
            restart_app
        ])
        .run(tauri::generate_context!())
        .expect("运行 Tauri 应用失败");
}
