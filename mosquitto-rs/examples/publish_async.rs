//! This example shows how to make a client, subscribe to a wildcard topic (`test/#`)
//! and publish a message to a topic.
//! It then waits to receive a message from the subscription (which will likely
//! be the message it just sent) and then terminates.
use mosquitto_rs::*;

fn main() -> Result<(), Error> {
    smol::block_on(async {
        let client = Client::with_auto_id()?;
        let rc = client
            .connect("localhost", 1883, std::time::Duration::from_secs(5), None)
            .await?;
        println!("connect: {rc}");

        let subscriptions = client.subscriber().unwrap();

        client.subscribe("test/#", QoS::AtMostOnce).await?;
        println!("subscribed");

        client
            .publish("test/this", "woot", QoS::AtMostOnce, false)
            .await?;
        println!("published");

        if let Ok(msg) = subscriptions.recv().await {
            println!("msg: {msg:?}");
        }

        Ok(())
    })
}
