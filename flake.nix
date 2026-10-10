{
  description = "Carta Space — writing-first document environment";

  inputs.nixpkgs.url = "github:NixOS/nixpkgs/nixos-26.05";

  outputs = { self, nixpkgs }:
    let
      systems = [ "x86_64-linux" "aarch64-linux" ];
      forAllSystems = nixpkgs.lib.genAttrs systems;
    in
    {
      packages = forAllSystems (system:
        let
          pkgs = import nixpkgs { inherit system; };
          inherit (pkgs) lib stdenv;

          # Runtime tools are private to the Carta editor wrappers. The
          # dictionary-enabled Hunspell wrapper sets DICPATH itself, so the
          # selected languages work without global host packages or symlinks.
          cartaHunspell = pkgs.hunspellWithDicts [
            pkgs.hunspellDicts.it_IT
            pkgs.hunspellDicts."fr-moderne"
            pkgs.hunspellDicts."en_GB-ise"
            pkgs.hunspellDicts.en_US
            pkgs.hunspellDicts.de_DE
            pkgs.hunspellDicts.es_ES
          ];
          cartaEditorPath = lib.makeBinPath [
            pkgs.git
            pkgs.curl
            pkgs.typst
            cartaHunspell
          ];

          # Preserve the frozen terminal frontend and CLI as a separate package.
          cartaSpace = pkgs.rustPlatform.buildRustPackage {
            pname = "carta-space";
            version = "0.2.0";

            src = self;
            cargoLock.lockFile = ./Cargo.lock;

            nativeBuildInputs = with pkgs; [
              git
              makeWrapper
              pkg-config
            ];
            buildInputs = with pkgs; [ wayland ];

            cargoBuildFlags = [ "--workspace" "--bins" ];
            cargoTestFlags = [ "--workspace" ];

            installPhase = ''
              runHook preInstall

              install -Dm755 \
                target/${stdenv.hostPlatform.rust.cargoShortTarget}/release/carta \
                "$out/bin/carta"
              install -Dm755 \
                target/${stdenv.hostPlatform.rust.cargoShortTarget}/release/carta-cli \
                "$out/bin/carta-cli"

              wrapProgram "$out/bin/carta" \
                --prefix PATH : ${cartaEditorPath}
              wrapProgram "$out/bin/carta-cli" \
                --prefix PATH : ${lib.makeBinPath [ pkgs.git ]}

              runHook postInstall
            '';

            meta = {
              description = "Carta Space terminal editor and administrative CLI";
              homepage = "https://github.com/fnt400/carta-space";
              license = lib.licenses.gpl3Plus;
              mainProgram = "carta";
              platforms = lib.platforms.linux;
            };
          };

          # The Iced GUI is intentionally outside the root Cargo workspace
          # and therefore needs its own tracked Cargo.lock.
          guiLibraries = with pkgs; [
            wayland
            libxkbcommon
            libX11
            libXcursor
            libXi
            libXrandr
            libxcb
          ];

          cartaGui = pkgs.rustPlatform.buildRustPackage {
            pname = "carta-gui";
            version = "0.2.0";

            src = self;
            cargoRoot = "crates/carta-gui";
            buildAndTestSubdir = "crates/carta-gui";
            cargoLock.lockFile = ./crates/carta-gui/Cargo.lock;

            nativeBuildInputs = with pkgs; [
              git
              copyDesktopItems
              makeWrapper
              pkg-config
            ];
            buildInputs = guiLibraries;

            desktopItems = [
              (pkgs.makeDesktopItem {
                name = "carta-space";
                desktopName = "Carta Space";
                genericName = "Text Editor";
                comment = "Distraction-free writing inspired by the Canon Cat";
                icon = "accessories-text-editor";
                exec = "carta-gui";
                categories = [ "Office" "TextEditor" ];
                terminal = false;
                startupNotify = true;
              })
            ];

            postInstall = ''
              # The desktop entry launches with no arguments. In that case
              # open the same XDG default Archive as the TUI; keep explicitly
              # provided Archive paths working from the command line.
              wrapProgram "$out/bin/carta-gui" \
                --prefix PATH : ${cartaEditorPath} \
                --prefix LD_LIBRARY_PATH : ${lib.makeLibraryPath guiLibraries} \
                --run 'if [ "$#" -eq 0 ]; then set -- "''${XDG_DATA_HOME:-$HOME/.local/share}/carta/archive"; fi'
            '';

            meta = {
              description = "Carta Space graphical writing environment";
              homepage = "https://github.com/fnt400/carta-space";
              license = lib.licenses.gpl3Plus;
              mainProgram = "carta-gui";
              platforms = lib.platforms.linux;
            };
          };
        in
        {
          # The GUI is the default v0.2 Nix build.
          default = cartaGui;
          carta-gui = cartaGui;
          # Existing package name remains available for the terminal version.
          carta-space = cartaSpace;
          carta-tui = cartaSpace;
          carta-cli = cartaSpace;
        });

      apps = forAllSystems (system:
        let
          packages = self.packages.${system};
        in
        {
          default = {
            type = "app";
            program = "${packages.carta-gui}/bin/carta-gui";
          };
          carta-gui = {
            type = "app";
            program = "${packages.carta-gui}/bin/carta-gui";
          };
          carta = {
            type = "app";
            program = "${packages.carta-space}/bin/carta";
          };
          carta-cli = {
            type = "app";
            program = "${packages.carta-space}/bin/carta-cli";
          };
        });
    };
}
