use std::time::Duration;

use echo::{
    AssistantMessage, DoneReason, Error, ErrorReason, Event, EventStream, Options, Provider,
    RetryPolicy, get_model,
};
use futures::StreamExt;

#[tokio::test]
async fn shared_driver_enforces_single_terminal_event() {
    let model = get_model(Provider::Anthropic, "claude-opus-4-8").unwrap();
    let partial = AssistantMessage::empty(&model);
    let source = async_stream::stream! {
        yield Event::Done {
            reason: DoneReason::Stop,
            partial: partial.clone(),
        };
        yield Event::Done {
            reason: DoneReason::Length,
            partial,
        };
    };
    let mut stream = EventStream::drive(model, source, Options::default().abort);
    let mut events = Vec::new();
    while let Some(event) = stream.next().await {
        events.push(event);
    }

    assert_eq!(events.len(), 1);
    assert!(matches!(
        events[0],
        Event::Done {
            reason: DoneReason::Stop,
            ..
        }
    ));
    assert!(stream.result().await.is_ok());
}

#[tokio::test]
async fn abort_ends_stream_with_aborted_terminal_error() {
    let model = get_model(Provider::Openai, "gpt-5").unwrap();
    let source_model = model.clone();
    let abort = echo::AbortHandle::new();
    let source = async_stream::stream! {
        let partial = AssistantMessage::empty(&source_model);
        yield Event::Start { partial };
        tokio::time::sleep(Duration::from_secs(60)).await;
    };
    let mut stream = EventStream::drive(model, source, abort.clone());

    let first = stream.next().await.unwrap();
    assert!(matches!(first, Event::Start { .. }));
    abort.abort();
    let terminal = stream.next().await.unwrap();
    assert!(matches!(
        terminal,
        Event::Error {
            reason: ErrorReason::Aborted,
            ..
        }
    ));
    assert!(matches!(stream.result().await, Err(Error::Aborted)));
}

#[test]
fn retry_delay_over_cap_fails_fast() {
    let err =
        echo::enforce_retry_delay(Duration::from_secs(10), Duration::from_secs(5)).unwrap_err();
    assert!(matches!(err, Error::RetryDelayExceeded { .. }));
    assert!(echo::enforce_retry_delay(Duration::from_secs(5), Duration::from_secs(5)).is_ok());
}

#[tokio::test]
async fn transient_errors_retry_until_success() {
    let policy = RetryPolicy {
        max_retries: 2,
        max_retry_delay: Duration::from_millis(10),
        base_delay: Duration::from_millis(1),
    };
    let value = echo::retry_transient(
        policy,
        |attempt| async move {
            if attempt < 2 {
                Err(Error::Provider("500 transient".to_string()))
            } else {
                Ok(attempt)
            }
        },
        |err| matches!(err, Error::Provider(message) if message.contains("500")),
        |_| None,
    )
    .await
    .unwrap();

    assert_eq!(value, 2);
}

#[tokio::test]
async fn provider_requested_retry_delay_over_cap_fails_without_sleeping() {
    let policy = RetryPolicy {
        max_retries: 1,
        max_retry_delay: Duration::from_millis(5),
        base_delay: Duration::from_millis(1),
    };
    let err = echo::retry_transient(
        policy,
        |_| async { Err::<(), _>(Error::Provider("429 retry later".to_string())) },
        |_| true,
        |_| Some(Duration::from_millis(10)),
    )
    .await
    .unwrap_err();

    assert!(matches!(err, Error::RetryDelayExceeded { .. }));
}
