//! `message`, `propose` and a typed handler.

use super::*;

#[tokio::test]
async fn message_hands_the_line_to_the_channel() {
    let heard = Arc::new(Mutex::new(Vec::new()));
    let kept = Arc::clone(&heard);
    let channel = LocalChannel::new(move |line| lock(&kept).push(line.to_owned()));
    let tools = built(Tools::builder().with_channel(&channel), &["message"]);

    let posted = run(
        &tools,
        "message",
        json!({"text": "halfway \u{2014} 3 of 6"}),
        None,
    )
    .await;
    let refused = run(&tools, "message", json!({"line": "x"}), None).await;

    assert_eq!(posted.text, "posted to the conversation");
    assert_eq!(*lock(&heard), ["halfway \u{2014} 3 of 6"]);
    assert_eq!(refused.error_code, Some(code::INVALID_ARGUMENTS));
    assert_eq!(format!("{channel:?}"), "LocalChannel { .. }");
}

/// A document whose proposals must each raise its number above the last.
#[derive(Debug)]
struct Counter;

impl Document for Counter {
    type Accepted = u64;
    type Revision = &'static str;

    fn schema(&self) -> ToolSpec<'_> {
        ToolSpec {
            name: "propose_count",
            description: "Propose a higher count.",
            parameters: &Value::Null,
        }
    }

    fn base(&self) -> &'static str {
        "rev-1"
    }

    fn apply(&self, head: Option<&u64>, proposal: &Value) -> Result<u64, Refusal> {
        let count = proposal["count"]
            .as_u64()
            .ok_or_else(|| Refusal::new(ErrorCode::new("count_missing"), "name a count"))?;
        if head.is_some_and(|head| count <= *head) {
            return Err(Refusal::new(
                ErrorCode::new("count_not_higher"),
                "the count must rise",
            ));
        }
        Ok(count)
    }
}

#[tokio::test]
async fn propose_keeps_the_last_accepted_proposal_and_refuses_the_rest() {
    let propose = Propose::new(Counter);
    let tools = built(
        Tools::builder().with_tool(propose.clone()),
        &["propose_count"],
    );
    let before = propose.kept();

    let first = run(&tools, "propose_count", json!({"count": 2}), None).await;
    let lower = run(&tools, "propose_count", json!({"count": 1}), None).await;
    let missing = run(&tools, "propose_count", json!({}), None).await;
    let not_object = run(&tools, "propose_count", json!([3]), None).await;
    let refusal = Refusal::new(ErrorCode::new("count_missing"), "name a count");

    assert_eq!(before, None);
    assert!(first.text.starts_with("Accepted."), "{}", first.text);
    assert_eq!(lower.text, "[count_not_higher] the count must rise");
    assert_eq!(missing.text, "[count_missing] name a count");
    assert_eq!(not_object.error_code, Some(code::INVALID_ARGUMENTS));
    assert_eq!(
        propose.kept(),
        Some(Kept {
            base: "rev-1",
            document: 2
        })
    );
    assert_eq!(
        (refusal.code(), refusal.reason()),
        (ErrorCode::new("count_missing"), "name a count")
    );
    assert_eq!(
        format!("{propose:?}"),
        "Propose { document: Counter, kept: true }"
    );
}

/// A handler of the embedder's own: its arguments' type is the schema the
/// model is told, and a call that breaks it never reaches the handler.
#[derive(Debug)]
struct Greet;

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
struct Name {
    /// Who to greet.
    name: String,
}

impl Handler for Greet {
    const NAME: &'static str = "greet";
    const DESCRIPTION: &'static str = "Greets someone.";
    type Arguments = Name;

    fn run(
        &self,
        arguments: Name,
        context: Context<'_>,
    ) -> impl Future<Output = ToolOutput> + Send {
        let greeting = format!("hello {} from {}", arguments.name, context.run_id);
        std::future::ready(ToolOutput::succeeded(greeting))
    }
}

#[tokio::test]
async fn a_typed_handler_is_told_its_arguments_schema_and_refuses_what_breaks_it() {
    let greet = Typed::new(Greet);
    let spec_name = greet.spec().name.to_owned();
    let schema = Schema::of::<Name>(Greet::DESCRIPTION);
    let tools = built(Tools::builder().with_tool(Typed::new(Greet)), &["greet"]);

    let greeted = run(&tools, "greet", json!({"name": "Ana\u{00EF}s"}), None).await;
    let refused = run(&tools, "greet", json!({"name": 7}), None).await;

    assert_eq!(spec_name, "greet");
    assert_eq!(format!("{:?}", greet.handler()), "Greet");
    assert_eq!(schema.description(), "Greets someone.");
    assert_eq!(at(schema.parameters(), "/properties/name/type"), "string");
    assert_eq!(at(schema.parameters(), "/required"), &json!(["name"]));
    assert_eq!(greeted.text, "hello Ana\u{00EF}s from run-1");
    assert!(
        refused
            .text
            .starts_with("[invalid_arguments] invalid type: integer `7`"),
        "{}",
        refused.text
    );
}
