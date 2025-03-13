# Basalt Server

Basalt server is the server backend for the Basalt programming
competition.  For most use cases, you don't want to run this program
directly and instead use our docker container.  You can find more
information in the [docs](https://basalt.rs/cli).

> [!IMPORTANT]
>
> The Basalt server can only be run on Linux as we use Linux-specific
> features for sandboxing.
>
> If you are (unfortunately) on Windows or MacOS, please use our docker
> image through our [CLI](https://github.com/basalt-rs/basalt-cli).

## Layout

This repo is broken into two crates: `basalt-server-lib` and
`basalt-server`.

The `basalt-server-lib` crate is the core of the basalt-server,
containing most of the logic and routes.  The library is going to be the
main place of development for most changes.

The `basalt-server` crate contains the handling of the CLI and the
logging.

We have these split into two crates so that we can build the
documentation automatically at compile time through a build script which
needs to depend on the library.

## Development

Running the server is no different from any other rust project.

From the root of the repo:

```sh
cargo run
```
