use std::sync::Mutex;

use async_trait::async_trait;
use novelai_bridge::{
    CreditEvent, CreditObserver, ObservedOperation, RequestCostEstimate, RequestOutcome,
};

use super::*;

#[derive(Default)]
struct RecordingObserver {
    events: Mutex<Vec<CreditEvent>>,
}

#[async_trait]
impl CreditObserver for RecordingObserver {
    async fn observe(&self, event: CreditEvent) {
        self.events
            .lock()
            .expect("test event mutex is not poisoned")
            .push(event);
    }
}

#[tokio::test]
async fn credit_observer_receives_paired_events_without_prompt_text() {
    let transport = MockTransport::with_bytes_response(create_zip_with_single_png());
    let observer = Arc::new(RecordingObserver::default());
    let client = Client::new(Arc::new(transport), ClientConfig::default())
        .with_credit_observer(Arc::<RecordingObserver>::clone(&observer));

    let mut params = request_for("a very specific secret prompt", Model::NaiDiffusion45Full);
    params.core.seed = 5;
    let _ = client.generate(params).await.unwrap();

    let events = observer.events.lock().unwrap().clone();
    assert_eq!(events.len(), 2);

    let CreditEvent::RequestPlanned {
        fingerprint: planned,
        estimate,
    } = &events[0]
    else {
        panic!("expected a planned event, got {:?}", events[0]);
    };
    let ObservedOperation::GenerateImage(summary) = &planned.operation else {
        panic!("expected generation summary");
    };
    assert_eq!(summary.pricing.model, Model::NaiDiffusion45Full);
    assert!(summary.prompt_len_chars > 0);
    assert!(matches!(estimate, RequestCostEstimate::Generation(value) if value.total > 0));
    assert!(!format!("{planned:?}").contains("secret prompt"));

    let CreditEvent::RequestSettled {
        fingerprint: settled,
        outcome,
    } = &events[1]
    else {
        panic!("expected a settled event, got {:?}", events[1]);
    };
    assert_eq!(settled.request_id, planned.request_id);
    assert_eq!(*outcome, RequestOutcome::Succeeded { image_count: 1 });
}

#[tokio::test]
async fn credit_observer_reports_local_validation_failures() {
    let transport = MockTransport::default();
    let observer = Arc::new(RecordingObserver::default());
    let client = Client::new(Arc::new(transport), ClientConfig::default())
        .with_credit_observer(Arc::<RecordingObserver>::clone(&observer));

    // Local validation runs before observation, so a rejected request emits nothing at all.
    let _ = client.generate(request("  ")).await.unwrap_err();
    assert!(observer.events.lock().unwrap().is_empty());
}

#[tokio::test]
async fn credit_observer_reports_transport_failures() {
    let transport = MockTransport::default();
    transport.set_bytes_error(novelai_bridge::TransportError::new(
        novelai_bridge::TransportOperation::SendRequest,
        None,
        "simulated".into(),
    ));
    let observer = Arc::new(RecordingObserver::default());
    let client = Client::new(Arc::new(transport), ClientConfig::default())
        .with_credit_observer(Arc::<RecordingObserver>::clone(&observer));

    let _ = client.generate(request("1girl")).await.unwrap_err();

    let events = observer.events.lock().unwrap().clone();
    assert_eq!(events.len(), 2);
    assert!(matches!(
        events[1],
        CreditEvent::RequestSettled {
            outcome: RequestOutcome::Failed { .. },
            ..
        }
    ));
}
