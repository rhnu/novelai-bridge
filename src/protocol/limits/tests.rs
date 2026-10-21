use super::*;

#[test]
fn success_counts_actual_chunks_and_rejects_before_buffer_growth() {
    let mut body = BodyBuffer::new(4, false);
    assert!(body.append(b"abc").unwrap());
    let error = body.append(b"de").unwrap_err();
    assert_eq!(error.limit, 4);
    assert_eq!(body.into_bytes(), b"abc");
}

#[test]
fn error_body_keeps_a_prefix_and_only_marks_actual_overflow() {
    let mut body = BodyBuffer::new(4, true);
    assert!(body.append(b"abcd").unwrap());
    assert!(!body.append(b"e").unwrap());
    assert_eq!(body.into_bytes(), b"abcd");
    let mut empty = BodyBuffer::new(0, true);
    assert!(!empty.append(b"x").unwrap());
    assert_eq!(empty.into_bytes(), b"");
}

#[test]
fn sse_budget_counts_split_lines_before_framing() {
    let mut budget = SseFrameBudget::new(12);
    budget.check(b"data: ").unwrap();
    assert!(budget.check(b"abcdefg").is_err());
}

#[test]
fn sse_budget_resets_on_blank_lines_across_all_delimiters() {
    for event in [b"data:x\n\n".as_slice(), b"data:x\r\r", b"data:x\r\n\r\n"] {
        let mut budget = SseFrameBudget::new(event.len());
        for _ in 0..3 {
            for byte in event {
                budget.check(&[*byte]).unwrap();
            }
        }
    }
}

#[test]
fn a_single_network_chunk_can_contain_many_bounded_events() {
    let mut budget = SseFrameBudget::new(8);
    budget.check(b"data:x\n\ndata:y\n\ndata:z\n\n").unwrap();
}
