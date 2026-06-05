use std::{
    future::Future,
    pin::Pin,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    task::{Context as TaskContext, Poll},
    time::Duration,
};

use futures::{Stream, StreamExt};
use tokio::sync::watch;

use crate::{
    AssistantMessage, Context, DoneReason, Error, ErrorReason, Event, Model, Options, Result,
    StopReason,
};

#[derive(Clone, Debug)]
pub struct AbortHandle {
    aborted: Arc<AtomicBool>,
    tx: Arc<watch::Sender<bool>>,
}

impl AbortHandle {
    pub fn new() -> Self {
        let (tx, _rx) = watch::channel(false);
        Self {
            aborted: Arc::new(AtomicBool::new(false)),
            tx: Arc::new(tx),
        }
    }

    pub fn abort(&self) {
        self.aborted.store(true, Ordering::SeqCst);
        let _ = self.tx.send(true);
    }

    pub fn is_aborted(&self) -> bool {
        self.aborted.load(Ordering::SeqCst)
    }

    pub(crate) fn subscribe(&self) -> watch::Receiver<bool> {
        self.tx.subscribe()
    }
}

impl Default for AbortHandle {
    fn default() -> Self {
        Self::new()
    }
}

pub struct EventStream {
    inner: Pin<Box<dyn Stream<Item = Event> + Send>>,
    final_message: Arc<Mutex<Option<Result<AssistantMessage>>>>,
}

impl EventStream {
    pub fn new<S>(stream: S) -> Self
    where
        S: Stream<Item = Event> + Send + 'static,
    {
        Self {
            inner: Box::pin(stream),
            final_message: Arc::new(Mutex::new(None)),
        }
    }

    pub fn drive<S>(model: Model, source: S, abort: AbortHandle) -> Self
    where
        S: Stream<Item = Event> + Send + 'static,
    {
        let mut source = Box::pin(source);
        let mut abort_rx = abort.subscribe();
        let abort_keepalive = abort.clone();
        Self::new(async_stream::stream! {
            let _abort_keepalive = abort_keepalive;
            let mut saw_terminal = false;
            let mut last_partial = AssistantMessage::empty(&model);

            loop {
                tokio::select! {
                    _ = abort_rx.changed() => {
                        if *abort_rx.borrow() && !saw_terminal {
                            let mut partial = last_partial.clone();
                            partial.error_message = Some("aborted".to_string());
                            yield Event::Error {
                                reason: ErrorReason::Aborted,
                                detail: "aborted".to_string(),
                                partial,
                            };
                        }
                        if *abort_rx.borrow() {
                            break;
                        }
                    }
                    next = source.next() => {
                        match next {
                            Some(event) => {
                                last_partial = event.partial().clone();
                                if saw_terminal {
                                    continue;
                                }
                                saw_terminal = event.terminal();
                                yield event;
                                if saw_terminal {
                                    break;
                                }
                            }
                            None => {
                                if !saw_terminal {
                                    last_partial.stop_reason = Some(StopReason::Stop);
                                    yield Event::Done {
                                        reason: DoneReason::Stop,
                                        partial: last_partial,
                                    };
                                }
                                break;
                            }
                        }
                    }
                }
            }
        })
    }

    pub async fn result(&self) -> Result<AssistantMessage> {
        let mut guard = self
            .final_message
            .lock()
            .expect("event stream mutex poisoned");
        guard.take().unwrap_or_else(|| {
            Err(Error::Provider(
                "stream ended without terminal event".to_string(),
            ))
        })
    }

    fn remember_terminal(&self, event: &Event) {
        if !event.terminal() {
            return;
        }

        let result = match event {
            Event::Done { partial, .. } => Ok(partial.clone()),
            Event::Error { reason, detail, .. } => match reason {
                ErrorReason::Aborted => Err(Error::Aborted),
                ErrorReason::Error => Err(Error::Provider(detail.clone())),
            },
            _ => unreachable!(),
        };

        let mut guard = self
            .final_message
            .lock()
            .expect("event stream mutex poisoned");
        if guard.is_none() {
            *guard = Some(result);
        }
    }
}

impl Stream for EventStream {
    type Item = Event;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut TaskContext<'_>) -> Poll<Option<Self::Item>> {
        let next = self.inner.as_mut().poll_next(cx);
        if let Poll::Ready(Some(event)) = &next {
            self.remember_terminal(event);
        }
        next
    }
}

pub fn stream(model: &Model, ctx: &Context, opts: &Options) -> EventStream {
    crate::register_default_adapters();
    match crate::get_api_provider(model.api) {
        Some(adapter) => match adapter.stream(model, ctx, opts) {
            Ok(es) => es,
            Err(err) => {
                let model = model.clone();
                EventStream::new(async_stream::stream! {
                    let mut partial = AssistantMessage::empty(&model);
                    partial.error_message = Some(err.to_string());
                    yield Event::Error {
                        reason: ErrorReason::Error,
                        detail: err.to_string(),
                        partial,
                    };
                })
            }
        },
        None => {
            let model = model.clone();
            let detail = format!("no adapter registered for api {:?}", model.api);
            EventStream::new(async_stream::stream! {
                let mut partial = AssistantMessage::empty(&model);
                partial.error_message = Some(detail.clone());
                yield Event::Error {
                    reason: ErrorReason::Error,
                    detail,
                    partial,
                };
            })
        }
    }
}

pub async fn complete(model: &Model, ctx: &Context, opts: &Options) -> Result<AssistantMessage> {
    let mut stream = stream(model, ctx, opts);
    while stream.next().await.is_some() {}
    stream.result().await
}

pub fn is_context_overflow(err: &Error) -> bool {
    match err {
        Error::ContextOverflow(_) => true,
        Error::Provider(message) | Error::Cli(message) => {
            let message = message.to_lowercase();
            message.contains("context")
                && (message.contains("length")
                    || message.contains("window")
                    || message.contains("too long"))
        }
        _ => false,
    }
}

pub fn enforce_retry_delay(requested: std::time::Duration, cap: std::time::Duration) -> Result<()> {
    if requested > cap {
        return Err(Error::RetryDelayExceeded { requested, cap });
    }
    Ok(())
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub struct RetryPolicy {
    pub max_retries: u32,
    pub max_retry_delay: Duration,
    pub base_delay: Duration,
}

impl RetryPolicy {
    pub fn from_options(opts: &Options) -> Self {
        Self {
            max_retries: opts.max_retries,
            max_retry_delay: opts.max_retry_delay,
            base_delay: Duration::from_millis(100),
        }
    }
}

pub async fn retry_transient<T, Fut, Op, IsTransient, RequestedDelay>(
    policy: RetryPolicy,
    mut op: Op,
    is_transient: IsTransient,
    requested_delay: RequestedDelay,
) -> Result<T>
where
    Op: FnMut(u32) -> Fut,
    Fut: Future<Output = Result<T>>,
    IsTransient: Fn(&Error) -> bool,
    RequestedDelay: Fn(&Error) -> Option<Duration>,
{
    let mut attempt = 0;
    loop {
        match op(attempt).await {
            Ok(value) => return Ok(value),
            Err(err) if attempt < policy.max_retries && is_transient(&err) => {
                if let Some(delay) = requested_delay(&err) {
                    enforce_retry_delay(delay, policy.max_retry_delay)?;
                }

                let backoff = policy
                    .base_delay
                    .saturating_mul(2_u32.saturating_pow(attempt));
                enforce_retry_delay(backoff, policy.max_retry_delay)?;
                tokio::time::sleep(backoff).await;
                attempt += 1;
            }
            Err(err) => return Err(err),
        }
    }
}
