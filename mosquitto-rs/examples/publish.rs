use mosquitto_rs::*;
use std::cell::RefCell;

fn main() -> Result<(), Error> {
    #[derive(Debug)]
    struct Handlers {
        data: RefCell<i32>,
    }

    impl Handlers {
        fn bump_and_print(&self) {
            let mut data = self.data.borrow_mut();
            *data += 1;
            println!("data is now {}", *data);
        }
    }

    impl Callbacks for Handlers {
        fn on_connect(&self, mosq: &mut Mosq, status: ConnectionStatus) {
            println!("Connected: status={}", status);
            if !status.is_successful() {
                let _ = mosq.disconnect();
            } else {
                let sub_mid = mosq.subscribe("test/topic", QoS::AtMostOnce);
                println!("Queued subscribe mid {:?}", sub_mid);
                self.bump_and_print();
            }
        }

        fn on_publish(&self, _: &mut Mosq, mid: MessageId) {
            println!("published: mid={}", mid);
            self.bump_and_print();
        }

        fn on_disconnect(&self, _: &mut Mosq, reason: i32) {
            println!("disconnected: reason={}", reason);
            self.bump_and_print();
        }

        fn on_subscribe(&self, mosq: &mut Mosq, mid: MessageId, granted_qos: &[QoS]) {
            println!("on_subscribe: mid={} {:?}", mid, granted_qos);
            let mid = mosq
                .publish("test/topic", b"hello!", QoS::AtMostOnce, false)
                .ok();
            println!("Queued publish with mid = {:?}", mid);
        }

        fn on_message(
            &self,
            mosq: &mut Mosq,
            mid: MessageId,
            topic: String,
            payload: &[u8],
            qos: QoS,
            retain: bool,
        ) {
            println!(
                "Got message {} on topic {}, payload: {:?}, qos:{:?}, retain:{}",
                mid, topic, payload, qos, retain
            );
            mosq.disconnect().ok();
        }
    }

    let mosq = Mosq::with_id(
        Handlers {
            data: RefCell::new(0),
        },
        "woot",
        false,
    )?;
    mosq.start_loop_thread()?;

    mosq.connect_non_blocking("localhost", 1883, std::time::Duration::from_secs(5), None)?;
    mosq.loop_until_explicitly_disconnected(std::time::Duration::from_secs(10))?;

    println!("handler data is: {:?}", *mosq.get_callbacks());

    Ok(())
}
