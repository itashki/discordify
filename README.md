# Discordify

Prepare images and videos for Discord directly from Dolphin. One context menu
entry compresses files to **10 MB**, keeping originals untouched. Everything runs
locally; no files are uploaded.

Discordify is a Rust binary with no third-party Rust dependencies. It uses
FFmpeg/ffprobe for media processing and libnotify's `notify-send` for desktop
notifications. No terminal window or dialogs are opened by the Dolphin action.

## Nix

Build or run directly from GitHub, without manually cloning the repository:

```sh
nix build github:itashki/discordify
nix run github:itashki/discordify -- -- /path/to/video.mkv
```

The package wraps the executable with FFmpeg and libnotify on PATH, installs the
Dolphin service menu into `share/kio/servicemenus`, and sets its executable path
to the installed binary in the Nix store. The standalone `package.nix` is also
available through `pkgs.callPackage ./package.nix {}`.

### NixOS with flakes

Add the input and module to your system's `flake.nix`. Keep your own hostname,
architecture, and other modules:

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

Apply from your configuration directory:

```sh
sudo nixos-rebuild switch --flake .#my-host
```

Your system's `flake.lock` pins the Discordify revision. To update:

```sh
nix flake update discordify
sudo nixos-rebuild switch --flake .#my-host
```

### NixOS without flakes

Add to `configuration.nix`:

```nix
{ ... }:
{
  imports = [
    "${builtins.fetchTarball "https://github.com/itashki/discordify/archive/refs/heads/master.tar.gz"}/nix/module.nix"
  ];
  programs.discordify.enable = true;
}
```

Apply with `sudo nixos-rebuild switch`. For a reproducible installation, replace
`refs/heads/master` in the URL with a specific commit SHA.

### Home Manager

With flakes, pass the `discordify` input to your Home Manager module using
`extraSpecialArgs = { inherit discordify; };`, then add:

```nix
{ pkgs, discordify, ... }:
{
  home.packages = [ discordify.packages.${pkgs.stdenv.hostPlatform.system}.default ];
}
```

### Dolphin integration

Restart Dolphin after applying your configuration. Right-click an image or video
and choose **Prepare for Discord**. There is a single action with a 10 MB limit.
If necessary, enable it in Dolphin Settings → Context Menu.

For Home Manager installations, `~/.nix-profile/share` must be included in your
KDE session's `XDG_DATA_DIRS`. If Dolphin cannot find the menu, build the package
with `nix build github:itashki/discordify` and install a local copy:

```sh
mkdir -p ~/.local/share/kio/servicemenus
cp result/share/kio/servicemenus/discordify.desktop ~/.local/share/kio/servicemenus/
chmod +x ~/.local/share/kio/servicemenus/discordify.desktop
```

This copy contains a Nix store path, so copy it again after package updates.
Prefer NixOS or Home Manager for a permanent installation.

## Usage

```sh
discordify --notify -- video.mkv image.png
```

The default limit is 10 decimal MB (10,000,000 bytes). Discord upload limits can
vary by subscription and server; see the
[Discord attachment FAQ](https://support.discord.com/hc/en-us/articles/25444343291031-File-Attachments-FAQ).
The optional CLI flag `--limit-mb` also accepts fractional values.

Files already within the limit are left untouched. Larger files produce
`name.discord.mp4` or `name.discord.webp` alongside the original. Existing files
are never overwritten; numbered suffixes are used when necessary.

- Videos use two-pass H.264/AAC encoding with bitrate and resolution adjustment.
- Still images use WebP with progressively reduced quality and resolution.
- GIFs and other FFmpeg-readable animations with a known duration become MP4.
  MP4 animation output has no transparency. Animated WebP/APNG support depends
  on FFmpeg. Unreadable animations or unknown durations produce an error.

Output size is checked before saving. Very long videos or very small custom
limits may prevent compression from succeeding. Files are processed sequentially.

With `--notify`, desktop notifications show progress for the current pass about
every two seconds and the output path on completion. A notification service must
be running in your desktop session. Messages are also written to stderr.
Press Ctrl+C to interrupt a terminal run; forced termination may leave a
`.discordify-*` temporary directory beside the input file.

### Language

The Dolphin menu supports English and Russian. Application messages, errors,
help, and notifications use Russian for a Russian locale and English otherwise.
Locale selection follows the first nonempty value of `LC_ALL`, `LC_MESSAGES`,
and `LANG`, in that order. To force English for a terminal run:

```sh
LC_ALL=C discordify --notify -- video.mkv
```

## Development and checks

```sh
nix develop path:.
cargo build --offline
cargo test --offline
```

The development shell includes `rust-analyzer` and sets `RUST_SRC_PATH` to the
Rust standard library sources. Start your editor from `nix develop path:.` so
its language server inherits this environment. After changing the flake,
re-enter the shell and restart your editor or language server.

Integration tests use real FFmpeg to check image/video size limits, the default
10 MB limit, audio and duration preservation, originals and existing outputs,
filenames with special characters, small files, invalid media, and locale selection.
