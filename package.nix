{ lib, rustPlatform, makeWrapper, ffmpeg, libnotify }:
rustPlatform.buildRustPackage {
  pname = "discordify";
  version = "0.1.0";
  src = lib.cleanSourceWith {
    src = ./.;
    filter = path: type:
      lib.cleanSourceFilter path type
      && !(builtins.elem (baseNameOf path) [ "target" "result" ]);
  };
  cargoLock.lockFile = ./Cargo.lock;
  nativeBuildInputs = [ makeWrapper ];
  nativeCheckInputs = [ ffmpeg ];

  postInstall = ''
    wrapProgram $out/bin/discordify \
      --prefix PATH : ${lib.makeBinPath [ ffmpeg libnotify ]}
    install -Dm644 data/discordify.desktop $out/share/kio/servicemenus/discordify.desktop
    substituteInPlace $out/share/kio/servicemenus/discordify.desktop \
      --replace-fail '@EXEC@' "$out/bin/discordify"
  '';

  meta = {
    description = "Compress images and videos to a Discord attachment size limit";
    license = lib.licenses.mit;
    platforms = lib.platforms.linux;
    mainProgram = "discordify";
  };
}
