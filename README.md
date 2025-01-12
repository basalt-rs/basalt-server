# Basalt Server

Basalt Server is essential for hosting competitions with Basalt.
Built for lightning speed and reliability using the Rust programming
language.

**DISCLAIMER**: This application can only be run on Linux since
our test runner, [leucite](https://github.com/basalt-rs/leucite),
requires this. If you're stuck on Windows (my condolences), then
your best bet is Docker.

## Getting Started

### Bare Metal

Releases are coming soon, but you'll need to build the server from
source for the time being.

```bash
git clone https://github.com/basalt-rs/basalt-server
```

```bash
cargo build --release
```

Your compiled binary can be found in `target/release/` called `basalt-server`.

You can move it to your path like so:

```bash
sudo mv target/release/basalt-server /bin/
```

Now verify your installation.

```bash
basalt-server --help
```

You should see a help message!

### Docker

*coming soon*
