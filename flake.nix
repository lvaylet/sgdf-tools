{
  description = "Suite d'outils de gestion pour le groupe scout local SGDF";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs = { self, nixpkgs, flake-utils }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        pkgs = import nixpkgs {
          inherit system;
        };

        workspacePackage = pkgs.rustPlatform.buildRustPackage {
          pname = "sgdf-tools";
          version = "0.1.0";
          src = ./.;

          cargoLock = {
            lockFile = ./Cargo.lock;
          };

          nativeBuildInputs = with pkgs; [
            pkg-config
          ];

          buildInputs = with pkgs; [
            openssl
            sqlite
          ];

          doCheck = false;
        };

        sgdf-api = pkgs.runCommand "sgdf-api-0.1.0" { } ''
          mkdir -p $out/bin
          cp ${workspacePackage}/bin/sgdf-api $out/bin/
        '';

        sgdf-cli = pkgs.runCommand "sgdf-cli-0.1.0" { } ''
          mkdir -p $out/bin
          cp ${workspacePackage}/bin/sgdf-cli $out/bin/
        '';
      in
      {
        packages = {
          default = workspacePackage;
          sgdf-api = sgdf-api;
          sgdf-cli = sgdf-cli;
        };

        devShells.default = pkgs.mkShell {
          buildInputs = with pkgs; [
            rustc
            cargo
            clippy
            rustfmt
            rust-analyzer
            pkg-config
            openssl
            sqlite
            sqlx-cli
            just
          ];

          RUST_SRC_PATH = "${pkgs.rustPlatform.rustLibSrc}";
        };
      }
    ) // {
      nixosModules.default = { config, lib, pkgs, ... }:
        with lib;
        let
          cfg = config.services.sgdf-api;
        in
        {
          options.services.sgdf-api = {
            enable = mkEnableOption "Service API SGDF";

            package = mkOption {
              type = types.package;
              default = self.packages.${pkgs.stdenv.hostPlatform.system}.sgdf-api;
              description = "Paquet contenant le binaire sgdf-api";
            };

            port = mkOption {
              type = types.port;
              default = 3000;
              description = "Port d'écoute du serveur HTTP sgdf-api";
            };

            host = mkOption {
              type = types.str;
              default = "127.0.0.1";
              description = "Adresse IP d'écoute de l'API";
            };

            dataDir = mkOption {
              type = types.str;
              default = "/var/lib/sgdf-tools";
              description = "Répertoire de stockage des données SQLite";
            };

            environmentFile = mkOption {
              type = types.nullOr types.path;
              default = null;
              description = "Fichier d'environnement contenant les secrets (notamment SGDF_AUTH_TOKEN)";
            };
          };

          config = mkIf cfg.enable {
            systemd.services.sgdf-api = {
              description = "Service API SGDF Tools";
              wantedBy = [ "multi-user.target" ];
              after = [ "network.target" ];

              serviceConfig = {
                ExecStart = "${cfg.package}/bin/sgdf-api";
                StateDirectory = "sgdf-tools";
                DynamicUser = true;
                Environment = [
                  "SGDF_PORT=${toString cfg.port}"
                  "SGDF_HOST=${cfg.host}"
                  "SGDF_DATABASE_URL=sqlite://${cfg.dataDir}/sgdf.db?mode=rwc"
                ];
                EnvironmentFile = mkIf (cfg.environmentFile != null) cfg.environmentFile;
                Restart = "on-failure";
              };
            };
          };
        };
    };
}
