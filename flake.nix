{
  description = "Partitura — Aria · Harmony · Voice · Echo dev environments";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs = { self, nixpkgs, flake-utils }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        pkgs = nixpkgs.legacyPackages.${system};

        commonTools = with pkgs; [
          git
        ];

      in {
        devShells = {

          # Default shell — shown by `direnv allow` / `nix develop`
          default = pkgs.mkShell {
            packages = commonTools;
            shellHook = ''
              echo "Partitura — four-package dev environment"
              echo ""
              echo "  nix develop .#harmony   Elixir/OTP state manager"
              echo "  nix develop .#voice     Rust agent harness"
              echo "  nix develop .#echo      OCaml REPL companion"
              echo "  nix develop .#aria      Desktop UI (SwiftUI/GTK4)"
            '';
          };

          # ── harmony ──────────────────────────────────────────────────────────
          # Elixir/OTP state manager daemon.
          harmony = pkgs.mkShell {
            packages = commonTools ++ (with pkgs; [
              erlang
              elixir
              elixir-ls          # LSP for editors
              rebar3             # compiles Erlang deps pulled in by mix
              openssl.dev        # needed by Erlang crypto / Phoenix TLS
            ]);
            # Hex + Rebar are fetched by mix; point them at a writable cache.
            MIX_HOME = "${builtins.getEnv "HOME"}/.mix";
            shellHook = ''
              echo "harmony — $(elixir --version | head -1)"
            '';
          };

          # ── voice ─────────────────────────────────────────────────────────────
          # Per-ticket agent harness — Rust workspace (edition 2024, resolver 3).
          voice = pkgs.mkShell {
            packages = commonTools ++ (with pkgs; [
              cargo
              rustc
              rustfmt
              clippy
              rust-analyzer
              pkg-config
              openssl.dev
            ]);
            RUST_BACKTRACE = "1";
            shellHook = ''
              echo "voice — $(rustc --version)"
            '';
          };

          # ── echo ──────────────────────────────────────────────────────────────
          # Conversational AI REPL — OCaml ≥ 5.1, dune ≥ 3.16.
          echo = pkgs.mkShell {
            packages = commonTools ++ (with pkgs; [
              # Compiler + build system
              ocaml          # tracks latest stable; nixos-unstable ships ≥ 5.1
              dune_3

              # Editor tooling
              ocamlPackages.ocaml-lsp-server
              ocamlPackages.ocamlformat
              ocamlPackages.merlin
              ocamlPackages.utop

              # Runtime deps (backend HTTP clients, JSON, async, terminal)
              ocamlPackages.cohttp-lwt-unix
              ocamlPackages.lwt
              ocamlPackages.yojson
              ocamlPackages.ppx_deriving
              ocamlPackages.lambda-term   # readline-style terminal interaction
              ocamlPackages.re            # regex
            ]);
            shellHook = ''
              echo "echo — OCaml $(ocaml --version)"
            '';
          };

          # ── aria ──────────────────────────────────────────────────────────────
          # Desktop UI — SwiftUI on macOS, GTK 4 on Linux.
          aria =
            if pkgs.stdenv.isDarwin
            then pkgs.mkShell {
              # Swift itself is provided by Xcode (must be installed separately).
              # Nix supplies code-quality tooling only.
              packages = commonTools ++ (with pkgs; [
                swiftlint
                swiftformat
              ]);
              shellHook = ''
                echo "aria — macOS/SwiftUI  (Xcode required; not managed by Nix)"
              '';
            }
            else pkgs.mkShell {
              packages = commonTools ++ (with pkgs; [
                gtk4
                gobject-introspection
                pkg-config
                vala
                meson
                ninja
              ]);
              shellHook = ''
                echo "aria — Linux/GTK4"
              '';
            };

        };
      }
    );
}
