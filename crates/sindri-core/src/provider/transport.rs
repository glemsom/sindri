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

/// How many bytes a replayed reply hands out at a time, so the framing a live event stream
/// exercises is the framing a fixture exercises.
const CHUNK: usize = 64;

const SESSION: &str = "{session}";

#[derive(Clone)]
pub struct Http<C> {
    inner: C,
    headers: Vec<(String, String)>,
    session: Option<String>,
}

impl Http<()> {
    pub fn new(headers: Vec<(String, String)>) -> Http<()> {
        Http {
            inner: (),
            headers,
            session: None,
        }
    }

    /// The run's headers with `{session}` standing for `session`, expanded as the request is
    /// dressed. Without it the placeholder is left as written, visible rather than silently
    /// empty.
    pub fn session(self, session: impl Into<String>) -> Http<()> {
        Http {
            inner: self.inner,
            headers: self.headers,
            session: Some(session.into()),
        }
    }
}

impl<C> Http<C> {
    pub fn over<D>(self, inner: D) -> Http<D> {
        Http {
            inner,
            headers: self.headers,
            session: self.session,
        }
    }

    fn dress<T>(&self, mut request: Request<T>) -> Result<Request<T>, Error> {
        for (name, value) in &self.headers {
            let name = http::HeaderName::try_from(name.as_str()).map_err(Error::instance)?;
            let value = HeaderValue::from_str(&self.expanded(value))?;
            request.headers_mut().append(name, value);
        }
        Ok(request)
    }

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

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Canned {
    status: u16,
    content_type: &'static str,
    body: &'static str,
}

impl Canned {
    pub fn stream(body: &'static str) -> Self {
        Self {
            status: 200,
            content_type: "text/event-stream",
            body,
        }
    }

    pub fn reply(status: u16, body: &'static str) -> Self {
        Self {
            status,
            content_type: "application/json",
            body,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Seen {
    pub method: String,
    pub url: String,
    pub headers: Vec<(String, String)>,
    pub body: String,
}

#[derive(Clone, Default)]
pub struct Replay {
    replies: Arc<Mutex<VecDeque<Canned>>>,
    seen: Arc<Mutex<Vec<Seen>>>,
}

impl Replay {
    pub fn new(replies: impl IntoIterator<Item = Canned>) -> Self {
        Self {
            replies: Arc::new(Mutex::new(replies.into_iter().collect())),
            seen: Arc::new(Mutex::new(Vec::new())),
        }
    }

    pub fn requests(&self) -> Vec<Seen> {
        self.lock(&self.seen).clone()
    }

    fn next_reply(&self) -> Result<Canned, Error> {
        self.lock(&self.replies).pop_front().ok_or_else(|| {
            Error::instance(std::io::Error::other(
                "the replay holds no reply for this request",
            ))
        })
    }

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

    fn whole<U: From<Bytes>>(canned: &Canned) -> Result<Response<LazyBody<U>>, Error> {
        let bytes = Bytes::from_static(canned.body.as_bytes());
        let body = Box::pin(async move { Ok(U::from(bytes)) }) as LazyBody<U>;
        Response::builder()
            .status(canned.status)
            .header("content-type", canned.content_type)
            .body(body)
            .map_err(Error::instance)
    }

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
