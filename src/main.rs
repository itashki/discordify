use std::{env, fs, io::{BufRead, BufReader, Read}, path::{Path, PathBuf}, process::{Command, Stdio}, time::Instant};

type Result<T> = std::result::Result<T, String>;

#[derive(Default)]
struct Notifications { enabled: bool, id: Option<String> }
impl Notifications {
    fn show(&mut self, message: &str, progress: Option<u32>, error: bool) {
        eprintln!("{message}");
        if !self.enabled { return; }
        let mut cmd = Command::new("notify-send");
        cmd.args(["--app-name=Discordify", "--icon=discord", "--print-id"]);
        if let Some(id) = &self.id { cmd.args(["--replace-id", id]); }
        if let Some(value) = progress {
            cmd.args(["--hint", &format!("int:value:{value}"), "--expire-time=0"]);
        } else { cmd.args(["--hint", "int:value:100", "--expire-time=10000"]); }
        if error { cmd.arg("--urgency=critical"); }
        if let Ok(output) = cmd.arg("Discordify").arg(message).output() {
            let id = String::from_utf8_lossy(&output.stdout).trim().to_owned();
            if output.status.success() && id.parse::<u32>().is_ok() { self.id = Some(id); }
        }
    }
}

fn probe(path: &Path) -> Result<(u32, u32, f64, bool, bool)> {
    let output = Command::new("ffprobe").args(["-v", "error", "-show_entries", "stream=codec_type,codec_name,width,height,nb_frames:format=duration,format_name", "-of", "default=noprint_wrappers=1"])
        .arg(path).output().map_err(|e| format!("Не удалось запустить ffprobe: {e}"))?;
    if !output.status.success() { return Err("Не удалось прочитать медиафайл".into()); }
    let mut width = 0; let mut height = 0; let mut duration = 0.; let mut audio = false; let mut animated = false;
    for line in String::from_utf8_lossy(&output.stdout).lines() {
        if let Some((key, value)) = line.split_once('=') {
            match key {
                "width" if width == 0 => width = value.parse().unwrap_or(0),
                "height" if height == 0 => height = value.parse().unwrap_or(0),
                "duration" => duration = value.parse().unwrap_or(0.),
                "codec_type" if value == "audio" => audio = true,
                "codec_name" | "format_name" if value == "apng" || value == "gif" => animated = true,
                "nb_frames" if value.parse::<u64>().unwrap_or(0) > 1 => animated = true,
                _ => {}
            }
        }
    }
    if width == 0 || height == 0 { return Err("В файле нет изображения или видео".into()); }
    Ok((width, height, duration, audio, animated))
}

fn encode(args: &[String], duration: f64, notice: &mut Notifications, label: &str) -> Result<()> {
    let mut child = Command::new("ffmpeg").args(["-hide_banner", "-loglevel", "error", "-nostdin", "-y", "-progress", "pipe:1"])
        .args(args).stdout(Stdio::piped()).stderr(Stdio::piped()).spawn().map_err(|e| format!("Не удалось запустить ffmpeg: {e}"))?;
    let stderr = child.stderr.take().unwrap();
    let errors = std::thread::spawn(move || {
        let mut tail = Vec::new();
        for byte in BufReader::new(stderr).bytes().flatten() {
            tail.push(byte);
            if tail.len() > 8192 { tail.drain(..4096); }
        }
        String::from_utf8_lossy(&tail).into_owned()
    });
    let mut last = Instant::now();
    for line in BufReader::new(child.stdout.take().unwrap()).lines().map_while(std::result::Result::ok) {
        if let Some(value) = line.strip_prefix("out_time_us=") {
            if duration > 0. && last.elapsed().as_secs() >= 2 {
                let time = value.parse::<f64>().unwrap_or(0.) / 1_000_000.;
                let percent = (time / duration * 100.).clamp(0., 99.) as u32;
                notice.show(&format!("{label}: {percent}%"), Some(percent), false);
                last = Instant::now();
            }
        }
    }
    let status = child.wait().map_err(|e| e.to_string())?;
    let error = errors.join().unwrap_or_default();
    if status.success() { Ok(()) } else { Err(format!("FFmpeg: {error}")) }
}

struct Temp(PathBuf);
impl Drop for Temp { fn drop(&mut self) { let _ = fs::remove_dir_all(&self.0); } }

fn process(path: &Path, limit: u64, notice: &mut Notifications) -> Result<PathBuf> {
    let path = fs::canonicalize(path).map_err(|e| e.to_string())?;
    if !path.is_file() { return Err("Нужен обычный файл".into()); }
    let (width, height, duration, audio, animated) = probe(&path)?;
    if fs::metadata(&path).map_err(|e| e.to_string())?.len() <= limit { return Ok(path); }
    let name = path.file_name().unwrap().to_string_lossy();
    let label = format!("Сжатие {name}");
    notice.show(&label, Some(0), false);
    let ext = path.extension().unwrap_or_default().to_string_lossy().to_lowercase();
    // Animated formats are encoded as video to retain their animation.
    let image = matches!(ext.as_str(), "jpg" | "jpeg" | "png" | "webp" | "bmp" | "tif" | "tiff" | "avif" | "heic") && duration <= 0. && !animated;
    if !image && (!duration.is_finite() || duration <= 0.) {
        return Err("Не удалось определить длительность видео (в том числе анимации)".into());
    }
    let parent = path.parent().unwrap();
    let temp = Temp(parent.join(format!(".discordify-{}-{}", std::process::id(), std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos())));
    fs::create_dir(&temp.0).map_err(|e| e.to_string())?;
    let suffix = if image { "webp" } else { "mp4" };
    let output = temp.0.join(format!("output.{suffix}"));
    let input = path.to_string_lossy().into_owned();
    let target = output.to_string_lossy().into_owned();
    let mut budget = (limit as f64 * 0.94) as u64;
    for attempt in 0..8 {
        let mut args = vec!["-i".into(), input.clone(), "-map".into(), "0:v:0".into(), "-map_metadata".into(), "-1".into()];
        if image {
            let scale = 0.75_f64.powi(attempt);
            let w = (width as f64 * scale).round().max(1.) as u32;
            let h = (height as f64 * scale).round().max(1.) as u32;
            args.extend(["-frames:v".into(), "1".into(), "-vf".into(), format!("scale={w}:{h}"), "-c:v".into(), "libwebp".into(), "-quality".into(), (90 - attempt * 8).max(30).to_string()]);
        } else {
            let total_rate = (budget as f64 * 8. / duration) as u64;
            let audio_rate = if audio { (total_rate / 5).clamp(16000, 128000) } else { 0 };
            if total_rate < audio_rate + 12000 { return Err("Слишком длинное видео для выбранного лимита. Увеличьте лимит или обрежьте видео".into()); }
            let video_rate = total_rate - audio_rate;
            let max_height = if video_rate < 150000 { 240 } else if video_rate < 400000 { 360 } else if video_rate < 1000000 { 480 } else if video_rate < 2500000 { 720 } else { 1080 };
            let scale = (max_height as f64 / height as f64).min(1.);
            let w = ((width as f64 * scale) as u32 / 2 * 2).max(2);
            let h = ((height as f64 * scale) as u32 / 2 * 2).max(2);
            args.extend(["-vf".into(), format!("scale={w}:{h},setsar=1,fps=30"), "-c:v".into(), "libx264".into(), "-preset".into(), "medium".into(), "-pix_fmt".into(), "yuv420p".into(), "-b:v".into(), video_rate.to_string()]);
            let passlog = temp.0.join("pass").to_string_lossy().into_owned();
            let mut first = args.clone();
            first.extend(["-an".into(), "-pass".into(), "1".into(), "-passlogfile".into(), passlog.clone(), "-f".into(), "null".into(), "/dev/null".into()]);
            encode(&first, duration, notice, &format!("{label}, анализ"))?;
            args.extend(["-pass".into(), "2".into(), "-passlogfile".into(), passlog]);
            if audio { args.extend(["-map".into(), "0:a:0".into(), "-c:a".into(), "aac".into(), "-b:a".into(), audio_rate.to_string()]); }
            args.extend(["-movflags".into(), "+faststart".into()]);
        }
        args.push(target.clone());
        encode(&args, duration, notice, &label)?;
        let size = fs::metadata(&output).map_err(|e| e.to_string())?.len();
        if size > 0 && size <= limit {
            let stem = path.file_stem().unwrap().to_string_lossy();
            for index in 0..10000 {
                let extra = if index == 0 { String::new() } else { format!("-{index}") };
                let destination = parent.join(format!("{stem}.discord{extra}.{suffix}"));
                match fs::hard_link(&output, &destination) {
                    Ok(()) => return Ok(destination),
                    Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
                    Err(e) => return Err(format!("Не удалось сохранить результат: {e}")),
                }
            }
            return Err("Слишком много файлов с таким именем".into());
        }
        budget = (budget as f64 * limit as f64 / size.max(1) as f64 * 0.9) as u64;
    }
    Err("Не удалось уложиться в лимит после 8 попыток".into())
}

fn main() {
    let mut limit = 20.;
    let mut files = Vec::new();
    let mut notice = Notifications::default();
    let mut args = env::args_os().skip(1);
    let mut positional = false;
    while let Some(arg) = args.next() {
        if !positional && arg == "--" { positional = true; }
        else if !positional && arg == "--notify" { notice.enabled = true; }
        else if !positional && arg == "--limit-mb" {
            limit = args.next().and_then(|v| v.to_str().and_then(|s| s.parse::<f64>().ok())).filter(|n| n.is_finite() && *n >= 0.001 && *n <= 100000.).unwrap_or_else(|| { eprintln!("Некорректный лимит МБ"); std::process::exit(2) });
        } else if !positional && (arg == "--help" || arg == "-h") {
            println!("discordify [--notify] [--limit-mb 20] -- FILE...\nЛимит в десятичных МБ. Оригиналы сохраняются; результат рядом с исходным файлом."); return;
        } else if !positional && arg.to_string_lossy().starts_with('-') {
            eprintln!("Неизвестный параметр: {}", arg.to_string_lossy()); std::process::exit(2);
        } else { files.push(PathBuf::from(arg)); }
    }
    if files.is_empty() { eprintln!("Укажите файлы. Справка: discordify --help"); std::process::exit(2); }
    let mut failed = false;
    for file in files {
        notice.id = None;
        match process(&file, (limit * 1_000_000.) as u64, &mut notice) {
            Ok(result) => notice.show(&format!("Готово: {}", result.display()), None, false),
            Err(error) => { notice.show(&format!("{}: {error}", file.display()), None, true); failed = true; }
        }
    }
    if failed { std::process::exit(1); }
}
