use crate::{Client, Message, QoS};
use matchit::Router;
use serde::de::DeserializeOwned;
use serde::Deserialize;
use serde_json::Value as JsonValue;
use std::future::Future;
use std::pin::Pin;
use std::str::FromStr;
use std::sync::Arc;
use thiserror::Error;

/// An error returned from the Router and related types
#[derive(Error, Debug)]
pub enum RouterError {
    #[error("payload is not utf8, cannot parse into string")]
    PayloadIsNotUtf8,
    #[error("failed to parse payload {text}: {error}")]
    PayloadParseFailed { text: String, error: String },
    #[error(transparent)]
    MqttError(#[from] crate::Error),
    #[error(transparent)]
    InsertError(#[from] matchit::InsertError),
    #[error(transparent)]
    MatchError(#[from] matchit::MatchError),
    #[error(transparent)]
    JsonError(#[from] serde_json::Error),
    #[error(transparent)]
    Any(#[from] anyhow::Error),
}

pub type RouterResult<T> = Result<T, RouterError>;
pub type MqttHandlerResult = anyhow::Result<()>;

/// Represents the context for handling a "request", an incoming
/// MQTT Message payload.
pub struct Request<S> {
    params: JsonValue,
    message: Message,
    state: S,
}

/// FromRequest allows you to parse and extract information
/// from a Request
pub trait FromRequest<S>: Sized {
    fn from_request(request: &Request<S>) -> RouterResult<Self>;
}

/// An extractor for the topic portion of a Message
pub struct Topic(pub String);

/// Extracts the Message::topic from a Request and wraps it in a Topic.
impl<S> FromRequest<S> for Topic {
    fn from_request(request: &Request<S>) -> RouterResult<Self> {
        Ok(Self(request.message.topic.clone()))
    }
}

/// An extractor for the payload portion of a Message.
/// Rather than simply copying the bytes, Payload will attempt to
/// parse the bytes with the help of the `FromStr` trait, allowing
/// you to declare a handler function like this, that will parse
/// the payload as a `u8` numeric value:
///
/// ```rust
/// use mosquitto_rs::router::Payload;
/// async fn my_handler(Payload(number): Payload<u8>) -> anyhow::Result<()> {
///   println!("The number is {number}");
///   Ok(())
/// }
/// ```
pub struct Payload<T>(pub T);

/// Extracts the payload portion of a message and parses it via `FromStr`
/// into type `T`.
impl<S, T> FromRequest<S> for Payload<T>
where
    T: FromStr,
    <T as FromStr>::Err: std::fmt::Debug,
{
    fn from_request(request: &Request<S>) -> RouterResult<Payload<T>> {
        let s = std::str::from_utf8(&request.message.payload)
            .map_err(|_| RouterError::PayloadIsNotUtf8)?;
        let result: T = s.parse().map_err(|err| RouterError::PayloadParseFailed {
            text: s.to_string(),
            error: format!("{err:#?}"),
        })?;
        Ok(Self(result))
    }
}

/// An extractor for the the topic portion of a Message.
/// Any parameters defined by the Route are populated into a map
/// and that map is deserialized into your type `T`.
///
/// ```rust
/// use mosquitto_rs::Client;
/// use mosquitto_rs::router::{MqttRouter, Params};
/// use serde::Deserialize;
///
/// #[derive(Deserialize)]
/// struct MyParams {
///    user: String,
/// }
///
/// async fn my_handler(Params(params): Params<MyParams>) -> anyhow::Result<()> {
///   println!("the user from the topic is {}", params.user);
///   Ok(())
/// }
///
/// async fn setup_router() -> anyhow::Result<()> {
///   let mut router = <MqttRouter>::new(Client::with_auto_id()?);
///   router.route("something/:user", my_handler).await?;
///   Ok(())
/// }
/// ```
pub struct Params<T>(pub T);
impl<S, T> FromRequest<S> for Params<T>
where
    T: DeserializeOwned,
{
    fn from_request(request: &Request<S>) -> RouterResult<Params<T>> {
        let parsed: T = serde_json::from_value(request.params.clone())?;
        Ok(Self(parsed))
    }
}

/// An extractor that allows access to the State data associated with
/// the router. The state value is passed down through `MqttRouter::dispatch`
/// and will be cloned and passed to your handler.
///
/// ```rust
/// use mosquitto_rs::router::State;
/// use std::sync::Arc;
///
/// struct MyState {}
///
/// async fn my_handler(State(state): State<Arc<MyState>>) -> anyhow::Result<()> {
///   Ok(())
/// }
/// ```
pub struct State<S>(pub S);
impl<S> FromRequest<S> for State<S>
where
    S: Clone + Send + Sync,
{
    fn from_request(request: &Request<S>) -> RouterResult<State<S>> {
        Ok(Self(request.state.clone()))
    }
}

/// A helper struct to type-erase handler functions for the router.
/// You do not normally need to consider the Dispatcher type directly,
/// as it is an implementation detail managed via the `MakeDispatcher` trait.
pub struct Dispatcher<S = ()>
where
    S: Clone + Send + Sync,
{
    func: Box<
        dyn Fn(Request<S>) -> Pin<Box<dyn Future<Output = MqttHandlerResult> + Send>> + Send + Sync,
    >,
}

impl<S: Clone + Send + Sync + 'static> Dispatcher<S> {
    pub async fn call(&self, params: JsonValue, message: Message, state: S) -> MqttHandlerResult {
        (self.func)(Request {
            params,
            message,
            state,
        })
        .await
    }

    pub fn new(
        func: Box<
            dyn Fn(Request<S>) -> Pin<Box<dyn Future<Output = MqttHandlerResult> + Send>>
                + Send
                + Sync,
        >,
    ) -> Self {
        Self { func }
    }
}

/// A helper trait to adapt generic handler functions into `Dispatcher` instances
/// that can be stored into a router.
/// You do not normally need to consider the `MakeDispatcher` trait directly,
/// as it is pre-registered for the compatible combinations of arguments.
pub trait MakeDispatcher<T, S: Clone + Send + Sync> {
    fn make_dispatcher(func: Self) -> Dispatcher<S>;
}

macro_rules! impl_make_dispatcher {
    (
        [$($ty:ident),*], $last:ident
    ) => {

impl<F, S, Fut, $($ty,)* $last> MakeDispatcher<($($ty,)* $last,), S> for F
where
    F: (Fn($($ty,)* $last) -> Fut) + Send + Sync + 'static,
    Fut: Future<Output = MqttHandlerResult> + Send ,
    S: Clone + Send + Sync + 'static,
    $( $ty: FromRequest<S>, )*
    $last: FromRequest<S>
{
    #[allow(non_snake_case)]
    fn make_dispatcher(func: F) -> Dispatcher<S> {
        let func = Arc::new(func);
        let wrap: Box<dyn Fn(Request<S>) -> Pin<Box<dyn Future<Output = MqttHandlerResult> + Send>> + Send + Sync> =
            Box::new(move |request: Request<S>| {
                let func = func.clone();
                Box::pin(async move {
                    $(
                    let $ty = $ty::from_request(&request)?;
                    )*

                    let $last = $last::from_request(&request)?;

                    func($($ty,)* $last).await
                })
            });

        Dispatcher::new(wrap)
    }
}

    }
}

#[rustfmt::skip]
macro_rules! all_the_tuples {
    ($name:ident) => {
        $name!([], T1);
        $name!([T1], T2);
        $name!([T1, T2], T3);
        $name!([T1, T2, T3], T4);
        $name!([T1, T2, T3, T4], T5);
        $name!([T1, T2, T3, T4, T5], T6);
        $name!([T1, T2, T3, T4, T5, T6], T7);
        $name!([T1, T2, T3, T4, T5, T6, T7], T8);
        $name!([T1, T2, T3, T4, T5, T6, T7, T8], T9);
        $name!([T1, T2, T3, T4, T5, T6, T7, T8, T9], T10);
        $name!([T1, T2, T3, T4, T5, T6, T7, T8, T9, T10], T11);
        $name!([T1, T2, T3, T4, T5, T6, T7, T8, T9, T10, T11], T12);
        $name!([T1, T2, T3, T4, T5, T6, T7, T8, T9, T10, T11, T12], T13);
        $name!([T1, T2, T3, T4, T5, T6, T7, T8, T9, T10, T11, T12, T13], T14);
        $name!([T1, T2, T3, T4, T5, T6, T7, T8, T9, T10, T11, T12, T13, T14], T15);
        $name!([T1, T2, T3, T4, T5, T6, T7, T8, T9, T10, T11, T12, T13, T14, T15], T16);
    };
}

all_the_tuples!(impl_make_dispatcher);

/// The `MqttRouter` type helps to manage topic subscriptions and dispatching
/// of matching messages to appropriate handler functions.
///
/// The generic `S` parameter represents some application-specific state
/// that is shared between various handlers. The `S` type needs to be
/// `Clone`. If you are using state, it is recommended that you use `Arc<S>`
/// or otherwise internally use something like `Arc` where the clone operation
/// is relatively cheap.
pub struct MqttRouter<S = ()>
where
    S: Clone + Send + Sync,
{
    router: Router<Dispatcher<S>>,
    client: Client,
}

impl<S: Clone + Send + Sync + 'static> MqttRouter<S> {
    /// Create a new router.
    ///
    /// If you don't want to specify the state type, construct it using
    /// this syntax, where the type name is enclosed in `<>`. That will
    /// allow the compiler to use the default state type of `()` without
    /// forcing you to write it out yourself.
    ///
    /// ```rust
    /// use mosquitto_rs::router::MqttRouter;
    /// use mosquitto_rs::Client;
    ///
    /// fn setup() -> anyhow::Result<()> {
    ///   let router = <MqttRouter>::new(Client::with_auto_id()?);
    ///   Ok(())
    /// }
    /// ```
    ///
    /// <https://www.reddit.com/r/rust/comments/ek6w5g/comment/fd91a0u/>
    pub fn new(client: Client) -> Self {
        Self {
            router: Router::new(),
            client,
        }
    }

    /// Register a route from a path like `foo/:bar` to a handler function.
    /// The corresponding mqtt topic pattern (`foo/+` in this case) will be subscribed to.
    /// When a message is received with that topic (say `foo/hello`) it will generate
    /// a Request with an associated parameter map like `{"bar": "hello"}`.
    /// Any extractors that you may have declared for your handler function parameters
    /// will be applied to the request to parse out the needed information.
    pub async fn route<'a, P, T, F>(&mut self, path: P, handler: F) -> RouterResult<()>
    where
        P: Into<String>,
        F: MakeDispatcher<T, S>,
    {
        let path = path.into();
        self.client
            .subscribe(&route_to_topic(&path), QoS::AtMostOnce)
            .await?;
        let dispatcher = F::make_dispatcher(handler);
        self.router.insert(path, dispatcher)?;
        Ok(())
    }

    /// Dispatch an mqtt message to a registered handler.
    pub async fn dispatch(&self, message: Message, state: S) -> RouterResult<()> {
        let topic = message.topic.to_string();
        let matched = self.router.at(&topic)?;

        let params = {
            let mut value_map = serde_json::Map::new();

            for (k, v) in matched.params.iter() {
                value_map.insert(k.into(), v.into());
            }

            if value_map.is_empty() {
                serde_json::Value::Null
            } else {
                serde_json::Value::Object(value_map)
            }
        };

        Ok(matched.value.call(params, message, state).await?)
    }

    pub fn client(&self) -> &Client {
        &self.client
    }
}

/// A helper to deserialize from a string into any type that
/// implements FromStr
pub fn parse_deser<'de, D, T: FromStr>(d: D) -> Result<T, D::Error>
where
    D: serde::Deserializer<'de>,
    <T as FromStr>::Err: std::fmt::Display,
{
    use serde::de::Error;
    let s = String::deserialize(d)?;
    s.parse::<T>()
        .map_err(|err| D::Error::custom(format!("parsing {s}: {err:#}")))
}

/// Convert a Router route into the corresponding mqtt topic.
/// `:foo` is replaced by `+`.
fn route_to_topic(route: &str) -> String {
    let mut result = String::new();
    let mut in_param = false;
    for c in route.chars() {
        if c == ':' {
            in_param = true;
            result.push('+');
            continue;
        }
        if c == '/' {
            in_param = false;
        }
        if in_param {
            continue;
        }
        result.push(c)
    }
    result
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_route_to_topic() {
        for (route, expected_topic) in [
            ("hello/:there", "hello/+"),
            ("a/:b/foo", "a/+/foo"),
            ("hello", "hello"),
            ("who:", "who+"),
        ] {
            let topic = route_to_topic(route);
            assert_eq!(
                topic, expected_topic,
                "route={route}, expected={expected_topic} actual={topic}"
            );
        }
    }

    #[test]
    fn routing() -> RouterResult<()> {
        let mut router = Router::new();
        router.insert("pv2mqtt/home", "Welcome!")?;
        router.insert("pv2mqtt/users/:name/:id", "A User")?;

        let matched = router.at("pv2mqtt/users/foo/978")?;
        assert_eq!(matched.params.get("id"), Some("978"));
        assert_eq!(*matched.value, "A User");

        Ok(())
    }
}
