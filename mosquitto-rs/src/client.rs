use crate::lowlevel::sys::mosq_err_t;
use crate::lowlevel::{Callbacks, MessageId, Mosq, QoS};
use crate::Error;
use async_channel::{bounded, unbounded, Receiver, Sender};
use std::collections::HashMap;
use std::os::raw::c_int;
use std::sync::Mutex;

struct Handler {
    connect: Mutex<Option<Sender<c_int>>>,
    mids: Mutex<HashMap<MessageId, Sender<MessageId>>>,
    subscriber_tx: Mutex<Sender<Message>>,
    subscriber_rx: Mutex<Option<Receiver<Message>>>,
}

impl Handler {
    fn new() -> Self {
        let (tx, rx) = unbounded();
        Self {
            connect: Mutex::new(None),
            mids: Mutex::new(HashMap::new()),
            subscriber_tx: Mutex::new(tx),
            subscriber_rx: Mutex::new(Some(rx)),
        }
    }
}

/// Represents a received message that matches one or
/// more of the subscription topic patterns on a client.
#[derive(Debug, Clone, Eq, PartialEq, Default)]
pub struct Message {
    /// The destination topic
    pub topic: String,
    /// The data payload bytes
    pub payload: Vec<u8>,
    /// The qos level at which the message was sent
    pub qos: QoS,
    /// Whether the message is a retained message.
    /// The broker will preserve the last retained
    /// message and send it to a subscriber at subscribe
    /// time.
    pub retain: bool,
    /// The message id
    pub mid: MessageId,
}

impl Callbacks for Handler {
    fn on_connect(&self, client: &mut Mosq, reason: c_int) {
        let mut connect = self.connect.lock().unwrap();
        if let Some(connect) = connect.take() {
            if connect.try_send(reason).is_err() {
                let _ = client.disconnect();
            }
        }
    }

    fn on_publish(&self, client: &mut Mosq, mid: MessageId) {
        let mut mids = self.mids.lock().unwrap();
        if let Some(tx) = mids.remove(&mid) {
            if tx.try_send(mid).is_err() {
                let _ = client.disconnect();
            }
        } else {
            let _ = client.disconnect();
        }
    }

    fn on_subscribe(&self, client: &mut Mosq, mid: MessageId, _granted_qos: &[QoS]) {
        let mut mids = self.mids.lock().unwrap();
        if let Some(tx) = mids.remove(&mid) {
            if tx.try_send(mid).is_err() {
                let _ = client.disconnect();
            }
        } else {
            let _ = client.disconnect();
        }
    }

    fn on_message(
        &self,
        client: &mut Mosq,
        mid: MessageId,
        topic: String,
        payload: &[u8],
        qos: QoS,
        retain: bool,
    ) {
        let m = Message {
            mid,
            topic,
            payload: payload.to_vec(),
            qos,
            retain,
        };
        if self.subscriber_tx.lock().unwrap().try_send(m).is_err() {
            let _ = client.disconnect();
        }
    }
}

/// A high-level, asynchronous mosquitto MQTT client
pub struct Client {
    mosq: Mosq,
}

impl Client {
    /// Create a new client instance with the specified id.
    /// If clean_session is true, instructs the broker to clean all messages
    /// and subscriptions on disconnect.  Otherwise it will preserve them.
    pub fn with_id(id: &str, clean_session: bool) -> Result<Self, Error> {
        let mosq = Mosq::with_id(id, clean_session)?;
        mosq.set_callbacks(Handler::new());
        mosq.start_loop_thread()?;
        Ok(Self { mosq })
    }

    /// Create a new client instance with a random client id
    pub fn with_auto_id() -> Result<Self, Error> {
        let mosq = Mosq::with_auto_id()?;
        mosq.set_callbacks(Handler::new());
        mosq.start_loop_thread()?;
        Ok(Self { mosq })
    }

    /// Configure the client with an optional username and password.
    /// The default is `None` for both.
    /// Whether you need to configure these credentials depends on the
    /// broker configuration.
    pub fn set_username_and_password(
        &mut self,
        username: Option<&str>,
        password: Option<&str>,
    ) -> Result<(), Error> {
        self.mosq.set_username_and_password(username, password)
    }

    /// Connect to the broker on the specified host and port.
    /// port is typically 1883 for mqtt, but it may be different
    /// in your environment.
    ///
    /// `keep_alive_seconds` specifies the interval at which
    /// keepalive requests are sent.  mosquitto has a minimum value
    /// of 5 for this and will generate an error if you use a smaller
    /// value.
    ///
    /// `bind_address` can be used to specify the outgoing interface
    /// for the connection.
    ///
    /// connect completes when the broker acknowledges the CONNECT
    /// command.
    ///
    /// Yields the connection return code; the value depends on the
    /// version of the MQTT protocol in use:
    /// For MQTT v5.0, look at section 3.2.2.2 Connect Reason code: <https://docs.oasis-open.org/mqtt/mqtt/v5.0/os/mqtt-v5.0-os.html>
    /// For MQTT v3.1.1, look at section 3.2.2.3 Connect Return code: <http://docs.oasis-open.org/mqtt/mqtt/v3.1.1/mqtt-v3.1.1.html>
    pub async fn connect(
        &mut self,
        host: &str,
        port: c_int,
        keep_alive_interval: std::time::Duration,
        bind_address: Option<&str>,
    ) -> Result<c_int, Error> {
        let handlers = self
            .mosq
            .get_callbacks::<Handler>()
            .expect("assigned during ctor");
        let (tx, rx) = bounded(1);
        handlers.connect.lock().unwrap().replace(tx);
        self.mosq
            .connect(host, port, keep_alive_interval, bind_address)?;
        let rc = rx
            .recv()
            .await
            .map_err(|_| Error::Mosq(mosq_err_t::MOSQ_ERR_INVAL))?;
        Ok(rc)
    }

    /// Publish a message to the specified topic.
    ///
    /// The payload size can be 0-283, 435 or 455 bytes; other values
    /// will generate an error result.
    ///
    /// `retain` will set the message to be retained by the broker,
    /// and delivered to new subscribers.
    ///
    /// Returns the assigned MessageId value for the publish.
    /// The publish may not complete immediately.
    /// You can use [set_callbacks](#method.set_callbacks) to register
    /// an `on_publish` event to determine when it completes.
    pub async fn publish(
        &mut self,
        topic: &str,
        payload: &[u8],
        qos: QoS,
        retain: bool,
    ) -> Result<MessageId, Error> {
        let (tx, rx) = bounded(1);

        {
            let handlers = self
                .mosq
                .get_callbacks::<Handler>()
                .expect("assigned during ctor");
            // Lock the map before we send, so that we can guarantee to
            // win the race with populating the map vs. signalling completion
            let mut mids = handlers.mids.lock().unwrap();
            let mid = self.mosq.publish(topic, payload, qos, retain)?;
            mids.insert(mid, tx);
        }

        let mid = rx
            .recv()
            .await
            .map_err(|_| Error::Mosq(mosq_err_t::MOSQ_ERR_INVAL))?;

        Ok(mid)
    }

    /// Returns a channel that yields messages from topics that this
    /// client has subscribed to.
    /// This method can be called only once; the first time it returns
    /// the channel and subsequently it no longer has the channel
    /// receiver to retur, so will yield None.
    pub fn subscriber(&mut self) -> Option<Receiver<Message>> {
        let handlers = self
            .mosq
            .get_callbacks::<Handler>()
            .expect("assigned during ctor");
        handlers.subscriber_rx.lock().unwrap().take()
    }

    /// Establish a subscription to topics matching pattern.
    /// The messages will be delivered via the channel returned
    /// via the [subscriber](#method.subscriber) method.
    pub async fn subscribe(&self, pattern: &str, qos: QoS) -> Result<(), Error> {
        let (tx, rx) = bounded(1);

        {
            let handlers = self
                .mosq
                .get_callbacks::<Handler>()
                .expect("assigned during ctor");
            // Lock the map before we send, so that we can guarantee to
            // win the race with populating the map vs. signalling completion
            let mut mids = handlers.mids.lock().unwrap();
            let mid = self.mosq.subscribe(pattern, qos)?;
            mids.insert(mid, tx);
        }

        let _ = rx
            .recv()
            .await
            .map_err(|_| Error::Mosq(mosq_err_t::MOSQ_ERR_INVAL))?;

        Ok(())
    }
}
