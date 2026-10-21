use futures_util::{StreamExt, stream};

use super::decode;
use crate::{BridgeError, DecodeTarget, ResponseLimitExceeded};

fn frame(payload: &[u8]) -> Vec<u8> {
    let mut frame = u32::try_from(payload.len()).unwrap().to_be_bytes().to_vec();
    frame.extend_from_slice(payload);
    frame
}

async fn collect(chunks: Vec<Vec<u8>>, limit: usize) -> Vec<Result<Vec<u8>, BridgeError>> {
    decode(Box::pin(stream::iter(chunks.into_iter().map(Ok))), limit)
        .collect()
        .await
}

#[tokio::test]
async fn every_split_and_coalesced_frames_preserve_payloads() {
    let first = [0x81, 0xA1, b'a', 1];
    let second = [0x81, 0xA1, b'b', 2];
    let mut wire = frame(&first);
    wire.extend(frame(&second));
    for split in 0..=wire.len() {
        let output = collect(vec![wire[..split].to_vec(), wire[split..].to_vec()], 4).await;
        assert_eq!(output.len(), 2, "split {split}");
        assert_eq!(output[0].as_ref().unwrap(), &first);
        assert_eq!(output[1].as_ref().unwrap(), &second);
    }
    let output = collect(wire.into_iter().map(|byte| vec![byte]).collect(), 4).await;
    assert_eq!(output.len(), 2);
    assert_eq!(output[1].as_ref().unwrap(), &second);
}

#[tokio::test]
async fn prefix_limit_is_checked_before_reading_any_payload() {
    for advertised in [5_u32, u32::MAX] {
        let mut input = decode(
            Box::pin(
                stream::iter([Ok(advertised.to_be_bytes().to_vec())]).chain(stream::pending()),
            ),
            4,
        );
        let error = input.next().await.unwrap().unwrap_err();
        assert!(matches!(error, BridgeError::Decode(error)
            if error.target == DecodeTarget::StreamChunk
                && error.source.downcast_ref::<ResponseLimitExceeded>().is_some_and(|limit| limit.limit == 4)));
        assert!(input.next().await.is_none());
    }
}

#[tokio::test]
async fn empty_frames_and_all_truncated_boundaries_fail_once() {
    let valid = frame(&[0x81, 0xA1, b'a', 1]);
    for bytes in (1..valid.len())
        .map(|length| valid[..length].to_vec())
        .chain([vec![0, 0, 0, 0]])
    {
        let output = collect(vec![bytes], 4).await;
        assert_eq!(output.len(), 1);
        assert!(matches!(output[0], Err(BridgeError::Decode(_))));
    }
    assert!(collect(Vec::new(), 4).await.is_empty());
}

#[tokio::test]
async fn valid_frames_do_not_hide_a_truncated_tail_or_transport_failure() {
    let valid = frame(&[0x80]);
    let mut tail = valid.clone();
    tail.push(0);
    let output = collect(vec![tail], 4).await;
    assert_eq!(output.len(), 2);
    assert_eq!(output[0].as_ref().unwrap(), &[0x80]);
    assert!(matches!(output[1], Err(BridgeError::Decode(_))));

    let source = crate::TransportError::new(
        crate::TransportOperation::ReadResponseBytes,
        None,
        Box::new(std::io::Error::other("network failure")),
    );
    let output = decode(Box::pin(stream::iter([Ok(valid), Err(source)])), 4)
        .collect::<Vec<_>>()
        .await;
    assert_eq!(output.len(), 2);
    assert!(matches!(output[1], Err(BridgeError::Transport(_))));
}
