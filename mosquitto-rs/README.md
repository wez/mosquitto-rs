# An async MQTT client

This crate implements an async MQTT client using libmosquitto.

```rust
use mosquitto_rs::*;

fn main() -> Result<(), Error> {
    smol::block_on(async {
        let mut mosq = Client::with_auto_id()?;
        let rc = mosq.connect("localhost", 1883, std::time::Duration::from_secs(5), None).await?;
        println!("connect: {}", rc);

        let subscriptions = mosq.subscriber().unwrap();

        mosq.subscribe("test", QoS::AtMostOnce).await?;
        println!("subscribed");

        mosq.publish("test", b"woot", QoS::AtMostOnce, false)
            .await?;
        println!("published");

        if let Ok(msg) = subscriptions.recv().await {
            println!("msg: {:?}", msg);
        }

        Ok(())
    })
}
```
