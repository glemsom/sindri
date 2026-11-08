//! How a request's bytes travel, and the fake a test substitutes: the real
//! transport adds one run's headers to every request and hands it to the client
//! that really sends, while the replay hands back bytes a fixture recorded and
//! keeps whatever it was asked, so a test exercises the seam with no network.
//!
//! A transport dresses a request; it does not decide whether to send it again.
//! §4's retry reads a verdict that only the fold above can classify, so a
//! resend never starts here (ADR-0015). What is decided here is what rides
//! along: the run's extra request headers, `{session}` expanded to the session
//! id as the request is dressed.
//!
//! The replay is not a test-only fixture behind a feature: it is small, it sends
//! nothing, and it is how a front end, `doctor` and any later conformance check
//! can prove what the seam does without a key.

// rig's `http::` error is 128 bytes and is what the transport trait returns, so
// every fallible helper carries it: `#[allow]` here beats wrapping the trait,
// which would be a second seam in front of the first.
#![allow(clippy::result_large_err)]

use std::collections::VecDeque;
use std::fmt;
use std::sync::{Arc, Mutex};

use rig_core::http_client::{
    BoxedStream, Bytes, Error, HeaderValue, HttpClientExt, LazyBody, MultipartForm, Request,
    Response, StreamingResponse,
};

/// How many bytes a replayed reply hands out at a time, so the framing a live
/// event stream exercises is the framing a fixture exercises.
const CHUNK: usize = 64;

/// What an extra header's value writes where the session id goes, so a gateway
/// that caches per session follows `/new`.
const SESSION: &str = "{session}";

/// A transport that adds `headers` to every request and gives it to the client
/// underneath to send, expanding `{session}` to the session id it was given.
#[derive(Clone)]
pub struct Http<C> {
    /// The client that really sends.
    inner: C,
    /// The run's extra request headers, as name and value.
    headers: Vec<(String, String)>,
    /// The session those headers name, when one is known.
    session: Option<String>,
}

impl Http<()> {
    /// A header set, waiting for the client it dresses.
    pub fn new(headers: Vec<(String, String)>) -> Http<()> {
        Http {
            inner: (),
            headers,
            session: None,
        }
    }

    /// This header set with `{session}` standing for `session`, expanded as the
    /// request is dressed. Without it the placeholder is left as it was
    /// written, which is visible rather than silently empty.
    pub fn session(self, session: impl Into<String>) -> Http<()> {
        Http {
            inner: self.inner,
            headers: self.headers,
            session: Some(session.into()),
        }
    }
}

impl<C> Http<C> {
    /// This header set over `inner`, the transport that sends.
    pub fn over<D>(self, inner: D) -> Http<D> {
        Http {
            inner,
            headers: self.headers,
            session: self.session,
        }
    }

    /// `request` with every header added, or the failure of a header the HTTP
    /// framing cannot carry.
    fn dress<T>(&self, mut request: Request<T>) -> Result<Request<T>, Error> {
        for (name, value) in &self.headers {
            let name = http::HeaderName::try_from(name.as_str()).map_err(Error::instance)?;
            let value = HeaderValue::from_str(&self.expanded(value))?;
            request.headers_mut().append(name, value);
        }
        Ok(request)
    }

    /// `value` with every `{session}` replaced by the session id, which is the
    /// only place a session is known when a request goes out.
    fn expanded(&self, value: &str) -> String {
        match &self.session {
            Some(session) => value.replace(SESSION, session),
            None => value.to_owned(),
        }
    }
}

impl<C> fmt::Debug for Http<C> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Http")
            .field("headers", &self.headers.len())
            .field("session", &self.session.is_some())
            .finish_non_exhaustive()
    }
}

impl<C: HttpClientExt + Clone + 'static> HttpClientExt for Http<C> {
    fn send<T, U>(
        &self,
        request: Request<T>,
    ) -> impl Future<Output = Result<Response<LazyBody<U>>, Error>> + Send + 'static
    where
        T: Into<Bytes> + Send,
        U: From<Bytes> + Send + 'static,
    {
        let dressed = self.dress(request).map(|request| request.map(Into::into));
        let inner = self.inner.clone();
        async move { inner.send::<Bytes, U>(dressed?).await }
    }

    fn send_multipart<U>(
        &self,
        request: Request<MultipartForm>,
    ) -> impl Future<Output = Result<Response<LazyBody<U>>, Error>> + Send + 'static
    where
        U: From<Bytes> + Send + 'static,
    {
        let dressed = self.dress(request);
        let inner = self.inner.clone();
        async move { inner.send_multipart::<U>(dressed?).await }
    }

    fn send_streaming<T>(
        &self,
        request: Request<T>,
    ) -> impl Future<Output = Result<StreamingResponse, Error>> + Send
    where
        T: Into<Bytes> + Send,
    {
        let dressed = self.dress(request).map(|request| request.map(Into::into));
        let inner = self.inner.clone();
        async move { inner.send_streaming::<Bytes>(dressed?).await }
    }
}

/// One recorded reply: the status, the content type and the body a fixture
/// captured from a wire.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Canned {
    /// The HTTP status the wire answered with.
    status: u16,
    /// The content type the wire answered with.
    content_type: &'static str,
    /// The body, byte for byte.
    body: &'static str,
}

impl Canned {
    /// A `200` event stream, as a reply was recorded mid-stream.
    pub fn stream(body: &'static str) -> Self {
        Self {
            status: 200,
            content_type: "text/event-stream",
            body,
        }
    }

    /// A reply with `status` and `body`, as a refusal was recorded.
    pub fn reply(status: u16, body: &'static str) -> Self {
        Self {
            status,
            content_type: "application/json",
            body,
        }
    }
}

/// One request a [`Replay`] was given, as it was given: enough to prove which
/// endpoint was addressed, what was sent, and what rode along.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Seen {
    /// The method, as the wire wrote it.
    pub method: String,
    /// The whole URL, the query included.
    pub url: String,
    /// Every header, lower-cased names, in the order they were added.
    pub headers: Vec<(String, String)>,
    /// The body, lossily decoded: a JSON request is readable here.
    pub body: String,
}

/// A transport that sends nothing: it hands back recorded replies in the order
/// they were canned, and keeps every request it was given.
#[derive(Clone, Default)]
pub struct Replay {
    /// The replies still to hand out.
    replies: Arc<Mutex<VecDeque<Canned>>>,
    /// Every request, in the order it arrived.
    seen: Arc<Mutex<Vec<Seen>>>,
}

impl Replay {
    /// A transport replaying `replies` in order.
    pub fn new(replies: impl IntoIterator<Item = Canned>) -> Self {
        Self {
            replies: Arc::new(Mutex::new(replies.into_iter().collect())),
            seen: Arc::new(Mutex::new(Vec::new())),
        }
    }

    /// Every request this transport was asked to send, in order.
    pub fn requests(&self) -> Vec<Seen> {
        self.lock(&self.seen).clone()
    }

    /// The next canned reply, or the honest failure of a record that ran out.
    fn next_reply(&self) -> Result<Canned, Error> {
        self.lock(&self.replies).pop_front().ok_or_else(|| {
            Error::instance(std::io::Error::other(
                "the replay holds no reply for this request",
            ))
        })
    }

    /// What `request` was, kept for a later assertion.
    fn keep(&self, request: &Request<Bytes>) {
        let seen = Seen {
            method: request.method().to_string(),
            url: request.uri().to_string(),
            headers: request
                .headers()
                .iter()
                .map(|(name, value)| {
                    (
                        name.as_str().to_owned(),
                        String::from_utf8_lossy(value.as_bytes()).into_owned(),
                    )
                })
                .collect(),
            body: String::from_utf8_lossy(request.body()).into_owned(),
        };
        self.lock(&self.seen).push(seen);
    }

    fn lock<'held, T>(&self, mutex: &'held Mutex<T>) -> std::sync::MutexGuard<'held, T> {
        mutex
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// The reply `canned` is, as a whole body the client reads at once.
    fn whole<U: From<Bytes>>(canned: &Canned) -> Result<Response<LazyBody<U>>, Error> {
        let bytes = Bytes::from_static(canned.body.as_bytes());
        let body = Box::pin(async move { Ok(U::from(bytes)) }) as LazyBody<U>;
        Response::builder()
            .status(canned.status)
            .header("content-type", canned.content_type)
            .body(body)
            .map_err(Error::instance)
    }

    /// The reply `canned` is, as the transport hands out its body in chunks.
    fn streamed(canned: &Canned) -> Result<StreamingResponse, Error> {
        let chunks: Vec<Result<Bytes, Error>> = canned
            .body
            .as_bytes()
            .chunks(CHUNK)
            .map(|chunk| Ok(Bytes::copy_from_slice(chunk)))
            .collect();
        let body: BoxedStream = Box::pin(futures::stream::iter(chunks));
        Response::builder()
            .status(canned.status)
            .header("content-type", canned.content_type)
            .body(body)
            .map_err(Error::instance)
    }
}

impl fmt::Debug for Replay {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Replay")
            .field("replies", &self.lock(&self.replies).len())
            .field("seen", &self.lock(&self.seen).len())
            .finish()
    }
}

impl HttpClientExt for Replay {
    fn send<T, U>(
        &self,
        request: Request<T>,
    ) -> impl Future<Output = Result<Response<LazyBody<U>>, Error>> + Send + 'static
    where
        T: Into<Bytes> + Send,
        U: From<Bytes> + Send + 'static,
    {
        self.keep(&request.map(Into::into));
        let reply = self
            .next_reply()
            .and_then(|canned| Replay::whole::<U>(&canned));
        async move { reply }
    }

    // Not an `async fn`: the trait demands a `'static` future, and an `async fn`
    // borrowing `self` cannot give one.
    #[allow(clippy::manual_async_fn)]
    fn send_multipart<U>(
        &self,
        _request: Request<MultipartForm>,
    ) -> impl Future<Output = Result<Response<LazyBody<U>>, Error>> + Send + 'static
    where
        U: From<Bytes> + Send + 'static,
    {
        async move {
            Err(Error::instance(std::io::Error::other(
                "the replay sends no multipart request",
            )))
        }
    }

    fn send_streaming<T>(
        &self,
        request: Request<T>,
    ) -> impl Future<Output = Result<StreamingResponse, Error>> + Send
    where
        T: Into<Bytes> + Send,
    {
        self.keep(&request.map(Into::into));
        let reply = self
            .next_reply()
            .and_then(|canned| Replay::streamed(&canned));
        async move { reply }
    }
}
