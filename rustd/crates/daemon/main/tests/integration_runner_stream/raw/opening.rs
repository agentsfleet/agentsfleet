//! Opening frames share the same real transport and bounded codec as established requests.

use super::*;

pub(super) fn response_reader(
    response: Response<af_stream::Incoming>,
    budget: &Budget,
    kind: StreamKind,
    stop: &CancellationToken,
) -> Reader<IncomingReader> {
    assert!(response.status().is_success());
    assert_eq!(
        response.headers()[protocol::VERSION_HEADER],
        protocol::VERSION
    );
    Reader::new(
        response.into_body().reader(),
        budget.clone(),
        kind,
        stop.clone(),
        STREAM_ANSWER_TIMEOUT,
    )
}

pub(super) async fn read(reader: &mut Reader<IncomingReader>) -> af_stream::Result<Option<Value>> {
    reader.next().await?.map(|frame| frame.decode()).transpose()
}

pub(super) async fn control_hello(
    connection: &mut Connection,
    endpoint: &Endpoint,
    token: &str,
    budget: &Budget,
    stop: &CancellationToken,
    deadline: Instant,
    running: &[af_core::id::Uuid7],
) -> Pipe {
    let (mut writer, body) = Outgoing::pipe();
    let opening = Request::post(endpoint.uri().clone())
        .header(protocol::VERSION_HEADER, protocol::VERSION)
        .header(protocol::STREAM_KIND_HEADER, StreamKind::Control.as_str())
        .header("authorization", format!("Bearer {token}"))
        .body(body)
        .expect("opening metadata");
    let hello = json!({
        "type":"hello", "request_id":1,
        "accepted_kinds":af_wire::lease::AcceptedKinds::FLEET,
        "running":running, "credits":0
    });
    let frame = af_stream::encode(&hello, budget, StreamKind::Control, stop, deadline)
        .await
        .expect("hello encodes");
    let (response, written) = tokio::join!(
        connection.open(opening, deadline),
        af_stream::write(&mut writer, &frame, stop, deadline)
    );
    written.expect("hello reaches the daemon");
    let reader = response_reader(
        response.expect("opening response"),
        budget,
        StreamKind::Control,
        stop,
    );
    Pipe { writer, reader }
}

pub(super) async fn confirm(
    pipe: &mut Pipe,
    budget: &Budget,
    stop: &CancellationToken,
    deadline: Instant,
) -> String {
    let prepared = tokio::time::timeout_at(deadline, read(&mut pipe.reader))
        .await
        .expect("prepared deadline")
        .expect("prepared decodes")
        .expect("prepared frame");
    assert_eq!((*crate::wire::pointer(&prepared, "/type")), "prepared");
    let owner = (*crate::wire::pointer(&prepared, "/connection_id"))
        .as_str()
        .expect("proposed owner")
        .to_owned();
    let confirm = json!({"type":"confirm", "request_id":1, "connection_id":owner});
    let frame = af_stream::encode(&confirm, budget, StreamKind::Control, stop, deadline)
        .await
        .expect("confirmation encodes");
    af_stream::write(&mut pipe.writer, &frame, stop, deadline)
        .await
        .expect("confirm owner");
    let welcome = tokio::time::timeout_at(deadline, read(&mut pipe.reader))
        .await
        .expect("welcome deadline")
        .expect("welcome decodes")
        .expect("welcome frame");
    assert_eq!((*crate::wire::pointer(&welcome, "/type")), "welcome");
    assert_eq!((*crate::wire::pointer(&welcome, "/connection_id")), owner);
    owner
}

pub(super) async fn secondary(
    connection: &mut Connection,
    endpoint: &Endpoint,
    owner: &str,
    budget: &Budget,
    stop: &CancellationToken,
    deadline: Instant,
) -> BTreeMap<StreamKind, Pipe> {
    let mut pipes = BTreeMap::new();
    for kind in [StreamKind::Work, StreamKind::Reports, StreamKind::Activity] {
        let (writer, body) = Outgoing::pipe();
        let request = Request::post(endpoint.uri().clone())
            .header(protocol::VERSION_HEADER, protocol::VERSION)
            .header(protocol::STREAM_KIND_HEADER, kind.as_str())
            .header(protocol::SESSION_HEADER, owner)
            .body(body)
            .expect("secondary metadata");
        let response = connection
            .open(request, deadline)
            .await
            .expect("secondary response");
        let reader = response_reader(response, budget, kind, stop);
        pipes.insert(kind, Pipe { writer, reader });
    }
    pipes
}
