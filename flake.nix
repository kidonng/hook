{
  description = "";

  inputs = {
    nixpkgs.url = "https://flakehub.com/f/DeterminateSystems/nixpkgs-weekly/0.1";
  };

  outputs = { nixpkgs, ... }:
    let
      systems = [
        "x86_64-linux"
        "aarch64-linux"
        "aarch64-darwin"
      ];
      forAllSystems = f:
        nixpkgs.lib.genAttrs systems
          (system: f nixpkgs.legacyPackages.${system});

      devTools = pkgs: [
        pkgs.bash
        pkgs.cargo
        pkgs.fish
        pkgs.rustc
      ];

      mkTask = pkgs: name: text:
        pkgs.writeShellApplication {
          inherit name text;
          runtimeInputs = devTools pkgs;
        };

      testRunner = pkgs:
        mkTask pkgs "hook-test" ''
        '';

      e2eRunner = pkgs:
        mkTask pkgs "hook-e2e" ''
        '';
      fmtRunner = pkgs:
        mkTask pkgs "hook-fmt" ''
        '';
    in
    {
      packages = forAllSystems (pkgs: rec {
        test = testRunner pkgs;
        e2e = e2eRunner pkgs;
        fmt = fmtRunner pkgs;
      });

      formatter = forAllSystems (pkgs: fmtRunner pkgs);

      devShells = forAllSystems (pkgs: {
        default = pkgs.mkShell {
          packages = devTools pkgs;
        };
      });
    };
}
