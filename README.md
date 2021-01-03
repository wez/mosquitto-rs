# Mosquitto MQTT client in Rust

![build](https://github.com/wez/mosquitto-rs/workflows/Rust/badge.svg)
[![Crates.io](https://img.shields.io/crates/v/mosquitto-rs)](https://docs.rs/mosquitto-rs)

This crate implements an async MQTT client using libmosquitto.

```rust
//! This example shows how to make a client, subscribe to a wildcard topic (`test/#`)
//! and publish a message to a topic.
//! It then waits to receive a message from the subscription (which will likely
//! be the message it just sent) and then terminates.
use mosquitto_rs::*;

fn main() -> Result<(), Error> {
    smol::block_on(async {
        let mut client = Client::with_auto_id()?;
        let rc = client
            .connect("localhost", 1883, std::time::Duration::from_secs(5), None)
            .await?;
        println!("connect: {}", rc);

        let subscriptions = client.subscriber().unwrap();

        client.subscribe("test/#", QoS::AtMostOnce).await?;
        println!("subscribed");

        client
            .publish("test/this", b"woot", QoS::AtMostOnce, false)
            .await?;
        println!("published");

        if let Ok(msg) = subscriptions.recv().await {
            println!("msg: {:?}", msg);
        }

        Ok(())
    })
}
```

## Why?

There are already a couple of other mosquitto-based Rust crates, so why add
another one?  The others are various combinations of unmaintained, outdated,
have code paths that can lead to panics, or that have some slightly sketchy
unsafe code.

In addition, none of them offered an `async` interface.

Why mosquitto-based rather than a native Rust client?  There are certainly a
large number of native Rust clients on crates.io, but I was a bit disappointed
because they all used different versions of `tokio`, none of them current, and
the one I liked the look of the most didn't compile.  To make matters more
frustrating, rustls is widely used for these clients, along with `webpki`, but
that doesn't mesh well with self-signed certificates that are prevalent among
home automation environments where I'm interested in using MQTT.

So, when I realized that I'd need to spend a few hours on this, I opted for
something that I could easily manage and keep ticking over without wrestling
with TLS and tokio versions.

## Features

The following feature flags are available:

* `vendored-mosquitto` - use bundled libmosquitto 2.4 library. This is on by default.
* `vendored-openssl` - build openssl from source, rather than using the system library. Recommended for macOS and Windows users to enable this.

## Windows

On Windows, you'll need to build with `--feature vendored-openssl`.  Currently,
due to <https://github.com/alexcrichton/openssl-src-rs/issues/82>, you'll need
to deploy the dlls found in a randomized directory such as
`target\release\build\openssl-sys-HASH\out\openssl-build\install\bin` alongside
your application for it to start up correctly.

