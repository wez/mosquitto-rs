# Mosquitto MQTT client in Rust

This crate implements an async MQTT client using libmosquitto.

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
