{
  description = "cmux - terminal multiplexer for AI coding agents (Linux/VTE)";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
  };

  outputs = { self, nixpkgs }:
    let
      system = "x86_64-linux";
      pkgs = import nixpkgs { inherit system; };

      pname = "cmux";
      version = "0.1.0";

      cmux = pkgs.rustPlatform.buildRustPackage {
        inherit pname version;
        src = ./.;

        cargoHash = "sha256-X9jIXBx6HlpUyoh/wPxW5bj7G9EkmNzyHu3jikpw4/U=";

        nativeBuildInputs = with pkgs; [
          pkg-config
          wrapGAppsHook4
        ];

        buildInputs = with pkgs; [
          gtk4
          libadwaita
          glib
          gdk-pixbuf
          cairo
          pango
          graphene
          openssl
          vte-gtk4
        ];

        meta = with pkgs.lib; {
          description = "Terminal multiplexer for AI coding agents (Linux/VTE)";
          homepage = "https://github.com/LucasPC-hub/cmux-linux";
          license = licenses.agpl3Only;
          platforms = [ "x86_64-linux" ];
        };
      };
    in
    {
      packages.${system}.default = cmux;
    };
}
