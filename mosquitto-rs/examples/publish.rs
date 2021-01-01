use mosquitto_rs::*;

fn main() -> Result<(), Error> {
    let mut mosq = mosquitto_rs::lowlevel::Mosq::with_id("woot", false)?;
    mosq.connect("localhost", 1883, 5, None)?;
    mosq.publish("test/topic", b"hello!", QoS::AtMostOnce, false)?;

    Ok(())
}
