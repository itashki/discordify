{ config, lib, pkgs, ... }:
{
  options.programs.discordify.enable = lib.mkEnableOption "Discordify and its Dolphin service menu";

  config = lib.mkIf config.programs.discordify.enable {
    environment.systemPackages = [ (pkgs.callPackage ../package.nix { }) ];
  };
}
