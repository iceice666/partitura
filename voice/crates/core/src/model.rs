/// Model seam: the `ModelStream` port over the echo model call.
///
/// Voice uses echo's real types directly; this trait only abstracts the *call* so that
/// the agent loop can be unit-tested with a `ScriptedStream` without hitting a live provider.
use echo::{Context, Event, Model, Options};
use futures::Stream;
use std::collections::VecDeque;
use std::pin::Pin;
use std::sync::Mutex;

/// Seam over the model call only — not over echo's types.
pub trait ModelStream: Send + Sync {
    fn stream(
        &self,
        model: &Model,
        ctx: &Context,
        opts: &Options,
    ) -> Pin<Box<dyn Stream<Item = Event> + Send>>;
}

/// Production implementation: calls echo's real `stream` function.
///
/// NOTE: echo's `stream()` is currently a stub (yields Start then Done{Stop} immediately,
/// ignoring the context). This will work correctly once echo's provider adapters ship.
pub struct EchoStream;

impl ModelStream for EchoStream {
    fn stream(
        &self,
        model: &Model,
        ctx: &Context,
        opts: &Options,
    ) -> Pin<Box<dyn Stream<Item = Event> + Send>> {
        let stream = echo::stream(model, ctx, opts);
        Box::pin(stream)
    }
}

/// Test implementation: yields a scripted sequence of echo events.
pub struct ScriptedStream {
    pub events: Vec<Event>,
    pub calls: Mutex<VecDeque<Vec<Event>>>,
}

impl ScriptedStream {
    pub fn new(events: Vec<Event>) -> Self {
        Self {
            events,
            calls: Mutex::new(VecDeque::new()),
        }
    }

    pub fn with_calls(calls: Vec<Vec<Event>>) -> Self {
        Self {
            events: vec![],
            calls: Mutex::new(VecDeque::from(calls)),
        }
    }
}

impl ModelStream for ScriptedStream {
    fn stream(
        &self,
        _model: &Model,
        _ctx: &Context,
        _opts: &Options,
    ) -> Pin<Box<dyn Stream<Item = Event> + Send>> {
        let events = self
            .calls
            .lock()
            .ok()
            .and_then(|mut calls| calls.pop_front())
            .unwrap_or_else(|| self.events.clone());
        Box::pin(async_stream::stream! {
            for event in events {
                yield event;
            }
        })
    }
}
