# 🪝 Hook

> [!WARNING]
> Work in progress. Most fish built-ins left as is.

Hook is a [fish](https://fishshell.com/) parser and transpiler.

Hook can translate fish scripts to clean, idiomatic [Bash](https://www.gnu.org/software/bash/) scripts, allowing them to run (almost) anywhere.

> [!NOTE]
> Want the reverse (Bash → fish)? Checkout [Bait](https://github.com/kidonng/bait)!

## Install

- [Nix](https://nixos.org/)

    ```sh
    nix profile install github:kidonng/hook
    ```

## Usage

> [!WARNING]
> Hook translation may produce unexpected result. Check output before serious usage.

```sh
# Translate from stdin
echo 'set hello world; and echo $hello' | hook

# Translate to stdout
hook script.fish > script.sh
```
