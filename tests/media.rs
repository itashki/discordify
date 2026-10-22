use std::{fs, path::PathBuf, process::Command};

struct Fixture(PathBuf);
impl Fixture {
    fn new(name: &str) -> Self {
        let path = std::env::temp_dir().join(format!("discordify-test-{}-{name}", std::process::id()));
        fs::create_dir_all(&path).unwrap();
        Self(path)
    }
    fn generate(&self, name: &str, arguments: &[&str]) -> PathBuf {
        let path = self.0.join(name);
        let result = Command::new("ffmpeg").args(["-v", "error", "-y"])
            .args(arguments).arg(&path).output().expect("Tests require ffmpeg");
        assert!(result.status.success(), "{}", String::from_utf8_lossy(&result.stderr));
        path
    }
    fn run(&self, path: &std::path::Path, limit: &str) -> std::process::Output {
        Command::new(env!("CARGO_BIN_EXE_discordify")).args(["--limit-mb", limit, "--"])
            .arg(path).output().unwrap()
    }
}
impl Drop for Fixture { fn drop(&mut self) { let _ = fs::remove_dir_all(&self.0); } }

#[test]
fn messages_follow_locale_precedence() {
    for (all, messages, lang, russian) in [
        ("C", "ru_RU.UTF-8", "ru_RU.UTF-8", false),
        ("ru_RU.UTF-8", "en_US.UTF-8", "en_US.UTF-8", true),
        ("", "ru_RU.UTF-8", "en_US.UTF-8", true),
        ("", "", "ru_RU.UTF-8", true),
        ("", "", "en_US.UTF-8", false),
        ("", "", "de_DE.UTF-8", false),
        ("", "", "", false),
    ] {
        for (argument, expected, success) in [
            ("--help", if russian { "По умолчанию лимит 10" } else { "Default limit: 10" }, true),
            ("--unknown", if russian { "Неизвестный параметр" } else { "Unknown option" }, false),
            ("--limit-mb", if russian { "Некорректный лимит" } else { "Invalid size limit" }, false),
        ] {
            let output = Command::new(env!("CARGO_BIN_EXE_discordify"))
                .env("LC_ALL", all).env("LC_MESSAGES", messages).env("LANG", lang)
                .arg(argument).output().unwrap();
            assert_eq!(output.status.success(), success);
            let text = if success { &output.stdout } else { &output.stderr };
            assert!(String::from_utf8_lossy(text).contains(expected), "locale: {all}/{messages}/{lang}");
        }
    }
}

#[test]
fn default_limit_compresses_files_over_ten_mb() {
    let f = Fixture::new("default-limit");
    let input = f.generate("large.bmp", &["-f", "lavfi", "-i", "testsrc=size=2048x2048", "-frames:v", "1"]);
    assert!(fs::metadata(&input).unwrap().len() > 10_000_000);
    let result = Command::new(env!("CARGO_BIN_EXE_discordify"))
        .arg("--").arg(&input).output().unwrap();
    assert!(result.status.success(), "{}", String::from_utf8_lossy(&result.stderr));
    assert!(fs::metadata(f.0.join("large.discord.webp")).unwrap().len() <= 10_000_000);
}

#[test]
fn image_compression_preserves_original_and_existing_output() {
    let f = Fixture::new("image");
    let input = f.generate("picture ; $ special.bmp", &["-f", "lavfi", "-i", "testsrc=size=512x512", "-frames:v", "1"]);
    let original = fs::read(&input).unwrap();
    let existing = f.0.join("picture ; $ special.discord.webp");
    fs::write(&existing, "keep me").unwrap();
    let result = f.run(&input, "0.01");
    assert!(result.status.success(), "{}", String::from_utf8_lossy(&result.stderr));
    let compressed = f.0.join("picture ; $ special.discord-1.webp");
    assert!(fs::metadata(compressed).unwrap().len() <= 10000);
    assert_eq!(fs::read(&input).unwrap(), original);
    assert_eq!(fs::read_to_string(existing).unwrap(), "keep me");
}

#[test]
fn video_compression_preserves_duration_and_audio() {
    let f = Fixture::new("video");
    let input = f.generate("clip.mkv", &["-f", "lavfi", "-i", "testsrc2=size=320x240:rate=24", "-f", "lavfi", "-i", "sine=frequency=440", "-t", "3", "-c:v", "ffv1", "-c:a", "pcm_s16le"]);
    let result = f.run(&input, "0.06");
    assert!(result.status.success(), "{}", String::from_utf8_lossy(&result.stderr));
    let compressed = f.0.join("clip.discord.mp4");
    assert!(fs::metadata(&compressed).unwrap().len() <= 60000);
    let probe = Command::new("ffprobe").args(["-v", "error", "-show_entries", "stream=codec_name:format=duration", "-of", "default=noprint_wrappers=1"]).arg(compressed).output().unwrap();
    let probe = String::from_utf8_lossy(&probe.stdout);
    assert!(probe.contains("codec_name=h264"));
    assert!(probe.contains("codec_name=aac"));
    let duration: f64 = probe.lines().find_map(|l| l.strip_prefix("duration=")).unwrap().parse().unwrap();
    assert!((duration - 3.).abs() < 0.15);
}

#[test]
fn small_file_is_unchanged_and_invalid_file_fails() {
    let f = Fixture::new("small");
    let input = f.generate("small.png", &["-f", "lavfi", "-i", "color=size=32x32", "-frames:v", "1"]);
    assert!(f.run(&input, "20").status.success());
    assert_eq!(fs::read_dir(&f.0).unwrap().count(), 1);
    let invalid = f.0.join("invalid.png");
    fs::write(&invalid, "not an image").unwrap();
    assert!(!f.run(&invalid, "20").status.success());
    assert!(!f.run(&input, "NaN").status.success());
}

#[test]
fn animated_gif_is_preserved_as_video_without_audio() {
    let f = Fixture::new("gif");
    let input = f.generate("animation.gif", &["-f", "lavfi", "-i", "testsrc=size=160x120:rate=10", "-t", "2"]);
    let result = f.run(&input, "0.02");
    assert!(result.status.success(), "{}", String::from_utf8_lossy(&result.stderr));
    let compressed = f.0.join("animation.discord.mp4");
    assert!(fs::metadata(&compressed).unwrap().len() <= 20000);
    let output = Command::new("ffprobe").args(["-v", "error", "-show_entries", "format=duration", "-of", "csv=p=0"]).arg(compressed).output().unwrap();
    let duration: f64 = String::from_utf8_lossy(&output.stdout).trim().parse().unwrap();
    assert!((duration - 2.).abs() < 0.15);
}
