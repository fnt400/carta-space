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

            buildInputs = with pkgs; [
              wayland
            ];

            cargoBuildFlags = [
              "--workspace"
              "--bins"
            ];

            cargoTestFlags = [
              "--workspace"
            ];

            installPhase = ''
              runHook preInstall

              install -Dm755 \
                target/${stdenv.hostPlatform.rust.cargoShortTarget}/release/carta \
                "$out/bin/carta"
              install -Dm755 \
                target/${stdenv.hostPlatform.rust.cargoShortTarget}/release/carta-cli \
                "$out/bin/carta-cli"

              wrapProgram "$out/bin/carta" \
                --prefix PATH : ${lib.makeBinPath [ pkgs.git pkgs.typst ]}
              wrapProgram "$out/bin/carta-cli" \
                --prefix PATH : ${lib.makeBinPath [ pkgs.git ]}

              runHook postInstall
            '';

            meta = {
              description = "Writing-first document environment";
              homepage = "https://github.com/fnt400/carta-space";
              license = lib.licenses.gpl3Plus;
              mainProgram = "carta";
              platforms = lib.platforms.linux;
            };
          };
        in
        {
          default = cartaSpace;
          carta-space = cartaSpace;
        });

      apps = forAllSystems (system: {
        default = {
          type = "app";
          program = "${self.packages.${system}.default}/bin/carta";
        };
      });
    };
}
