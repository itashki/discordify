# Discordify

Локальная подготовка изображений и видео к отправке в Discord. Один бинарник
на Rust без сторонних Rust-библиотек; во время работы нужны FFmpeg/ffprobe,
а для уведомлений — `notify-send` из libnotify. Python, Konsole и диалоговые окна
не используются. Файлы никуда не загружаются.

## Nix

```sh
nix build github:itashki/discordify
nix run github:itashki/discordify -- --limit-mb 20 -- /path/to/video.mkv
```

Nix скачивает исходники автоматически; клонировать репозиторий вручную не нужно.

Пакет включает FFmpeg и libnotify в PATH обёртки, устанавливает меню в
`share/kio/servicemenus` и прописывает абсолютный путь к бинарнику из Nix store.
Отдельный `package.nix` можно подключить через `pkgs.callPackage ./package.nix {}`.

### NixOS с flakes

Добавьте input и модуль в системный `flake.nix` (сохраните свои hostname,
архитектуру и остальные модули):

```nix
{
  inputs.nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
  inputs.discordify.url = "github:itashki/discordify";
  inputs.discordify.inputs.nixpkgs.follows = "nixpkgs";

  outputs = { nixpkgs, discordify, ... }: {
    nixosConfigurations.my-host = nixpkgs.lib.nixosSystem {
      system = "x86_64-linux";
      modules = [
        ./configuration.nix
        discordify.nixosModules.default
        { programs.discordify.enable = true; }
      ];
    };
  };
}
```

Примените конфигурацию из её каталога:

```sh
sudo nixos-rebuild switch --flake .#my-host
```

Версия Discordify закрепляется в системном `flake.lock`. Для обновления:

```sh
nix flake update discordify
sudo nixos-rebuild switch --flake .#my-host
```

### NixOS без flakes

Добавьте в `configuration.nix`:

```nix
{ ... }:
{
  imports = [
    "${builtins.fetchTarball "https://github.com/itashki/discordify/archive/refs/heads/master.tar.gz"}/nix/module.nix"
  ];
  programs.discordify.enable = true;
}
```

Примените через `sudo nixos-rebuild switch`. Для воспроизводимой установки
замените `refs/heads/master` в URL на SHA выбранного коммита.

### Home Manager

При использовании flakes передайте input `discordify` в модуль Home Manager
через `extraSpecialArgs = { inherit discordify; };`, затем:

```nix
{ pkgs, discordify, ... }:
{
  home.packages = [ discordify.packages.${pkgs.stdenv.hostPlatform.system}.default ];
}
```

После применения конфигурации перезапустите Dolphin. В контекстном меню
изображения/видео появится «Подготовить для Discord» с лимитами 10, 20, 50,
500 МБ. При необходимости включите меню в настройках Dolphin → Контекстное меню.
Для Home Manager каталог `~/.nix-profile/share` должен входить в `XDG_DATA_DIRS`
сеанса KDE. Если меню не обнаруживается, можно установить локальную копию:

```sh
mkdir -p ~/.local/share/kio/servicemenus
cp result/share/kio/servicemenus/discordify.desktop ~/.local/share/kio/servicemenus/
chmod +x ~/.local/share/kio/servicemenus/discordify.desktop
```

Локальная копия содержит путь в store: после обновления пакета скопируйте её снова.
Для постоянной установки предпочтительнее NixOS или Home Manager.

## Использование

```sh
discordify --notify --limit-mb 20 -- video.mkv image.png
```

По умолчанию лимит 20 десятичных МБ (с небольшим запасом относительно 20 MiB).
Лимиты Discord могут меняться и зависеть от подписки/сервера:
[справка Discord](https://support.discord.com/hc/en-us/articles/25444343291031-File-Attachments-FAQ).
Параметр `--limit-mb` принимает также дробные значения.

Файлы в пределах лимита остаются без изменений. Для больших файлов рядом
создаются `имя.discord.mp4` или `имя.discord.webp`; существующие файлы и оригиналы
не перезаписываются. Видео: H.264/AAC, два прохода, подбор битрейта и разрешения.
Статичные изображения: WebP с постепенным снижением качества/разрешения.
GIF и другие читаемые FFmpeg анимации с известной длительностью превращаются в MP4.
У анимаций MP4 нет прозрачности; поддержка анимированного WebP/APNG зависит от FFmpeg.
Если FFmpeg не может прочитать анимацию или длительность, приложение сообщает ошибку.
При очень маленьком лимите/длинном видео приложение может отказаться от сжатия.

Итоговый размер проверяется перед сохранением. Обработка идёт последовательно.
В уведомлениях KDE обновляется прогресс текущего прохода примерно раз в две секунды;
после завершения показывается путь к результату. Требуется работающий сервис
уведомлений в пользовательском сеансе. Без `--notify` сообщения идут в stderr.
Прерывание в терминале: Ctrl+C (при принудительном завершении временный каталог
`.discordify-*` может остаться рядом с исходным файлом).

## Разработка и проверка

```sh
nix develop path:.
cargo build --offline
cargo test --offline
```

Dev shell также включает `rust-analyzer` и задаёт `RUST_SRC_PATH` для исходников
стандартной библиотеки. Запускайте редактор из `nix develop path:.`, чтобы его
Rust language server получил это окружение. После изменения flake выйдите из
старого dev shell, войдите снова и перезапустите редактор или language server.

Интеграционные тесты используют настоящий FFmpeg: проверяют размер изображения
и видео, сохранение звука/длительности, оригиналов и уже существующих результатов,
имена со спецсимволами, маленькие и повреждённые файлы.
