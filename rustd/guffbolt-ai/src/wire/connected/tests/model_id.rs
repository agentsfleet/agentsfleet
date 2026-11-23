//! A connected model is counted, measured and sent as the id it was
//! connected to, whatever id a request names.

use super::*;

// Value: a loop over any `Model` knows no model id and sends none; the
// connected model counts, measures and sends as the id it was connected to.
#[tokio::test]
async fn a_request_naming_no_model_is_counted_and_sent_as_the_connected_one() {
    let (turns, physical, _recorder) = turns(200);
    let messages = [Message::User("private source ".repeat(NEAR_CAPACITY))];
    let unnamed = Request {
        model: "",
        ..request(&messages)
    };

    let measured = turns.measure(unnamed).unwrap();
    let chunks: Vec<_> = turns.stream(unnamed).collect().await;

    assert_eq!(measured, turns.measure(request(&messages)).unwrap());
    assert!(chunks.iter().all(Result::is_ok), "{chunks:?}");
    assert_eq!(
        *physical.paths.lock().unwrap(),
        ["/v1/responses/input_tokens", "/v1/responses"]
    );
    assert_eq!(*physical.models.lock().unwrap(), [MODEL, MODEL]);
}
