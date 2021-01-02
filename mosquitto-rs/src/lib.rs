//! This crate implements an async MQTT client using libmosquitto.
//!
//! ```no_run
//! use mosquitto_rs::*;
//!
//! fn main() -> Result<(), Error> {
//!     smol::block_on(async {
//!         let mut client = Client::with_auto_id()?;
//!         let rc = client.connect(
//!                        "localhost", 1883,
//!                        std::time::Duration::from_secs(5), None).await?;
//!         println!("connect: {}", rc);
//!
//!         let subscriptions = client.subscriber().unwrap();
//!
//!         client.subscribe("test", QoS::AtMostOnce).await?;
//!         println!("subscribed");
//!
//!         client.publish("test", b"woot", QoS::AtMostOnce, false)
//!             .await?;
//!         println!("published");
//!
//!         if let Ok(msg) = subscriptions.recv().await {
//!             println!("msg: {:?}", msg);
//!         }
//!
//!         Ok(())
//!     })
//! }
//! ```
mod client;
mod error;
mod lowlevel;

pub use client::*;
pub use error::*;
pub use lowlevel::*;
