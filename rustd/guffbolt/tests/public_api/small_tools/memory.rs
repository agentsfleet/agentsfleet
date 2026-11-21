//! The memory tools over a store, and a store that refuses.

use super::*;

#[tokio::test]
async fn memory_lists_recalls_and_forgets_newest_first() {
    let memory = LocalMemory::seeded(vec![
        Memory {
            key: "deploy".to_owned(),
            content: "Fridays".to_owned(),
            category: "core".to_owned(),
            reach: Reach::Private,
        },
        Memory {
            key: "oncall".to_owned(),
            content: "deploy pager".to_owned(),
            category: "daily".to_owned(),
            reach: Reach::Shared,
        },
    ]);
    let tools = built(Tools::builder().with_memory(&memory), &["memory_*"]);

    let listed = run(&tools, "memory_list", json!({}), None).await;
    let daily = run(&tools, "memory_list", json!({"category": "daily"}), None).await;
    let none = run(&tools, "memory_list", json!({"category": "weekly"}), None).await;
    let recalled = run(
        &tools,
        "memory_recall",
        json!({"query": "deploy", "limit": 1}),
        None,
    )
    .await;
    let both = run(&tools, "memory_recall", json!({"query": "DEPLOY"}), None).await;
    let nothing = run(&tools, "memory_recall", json!({"query": "\u{1F680}"}), None).await;
    let forgot = run(&tools, "memory_forget", json!({"key": "oncall"}), None).await;
    let unknown = run(&tools, "memory_forget", json!({"key": "oncall"}), None).await;
    let stored = run(
        &tools,
        "memory_store",
        json!({"key": "k", "content": "c", "category": "daily", "visibility": "shared"}),
        None,
    )
    .await;

    assert_eq!(listed.text, "oncall (daily)\ndeploy (core)");
    assert_eq!(daily.text, "oncall (daily)");
    assert_eq!(none.text, "nothing remembered");
    assert_eq!(
        recalled.text, "deploy (core): Fridays",
        "key matches first, at most the limit"
    );
    assert_eq!(
        both.text,
        "deploy (core): Fridays\noncall (daily): deploy pager"
    );
    assert_eq!(nothing.text, "nothing remembered matches \u{1F680}");
    assert!(
        forgot
            .text
            .starts_with("oncall is forgotten for the rest of this run"),
        "{}",
        forgot.text
    );
    assert_eq!(unknown.text, "nothing remembered under oncall");
    assert_eq!(stored.text, "stored k");
    assert_eq!(
        memory
            .entries()
            .iter()
            .map(|entry| (entry.key.as_str(), entry.reach))
            .collect::<Vec<_>>(),
        [("deploy", Reach::Private), ("k", Reach::Shared)]
    );
}

/// A store that refuses every call, and names a writer on what it recalls.
#[derive(Debug)]
struct Full;

fn full() -> MemoryRefused {
    MemoryRefused {
        code: code::MEMORY_FULL,
        detail: "the store is full".to_owned(),
    }
}

impl MemoryStore for Full {
    fn store(&self, _memory: Memory) -> MemoryFuture<'_, ()> {
        Box::pin(std::future::ready(Err(full())))
    }

    fn recall<'a>(&'a self, query: &'a str, _limit: usize) -> MemoryFuture<'a, Vec<Remembered>> {
        let shared = Remembered {
            key: "team".to_owned(),
            content: query.to_owned(),
            category: "core".to_owned(),
            writer: Some("Support triage".to_owned()),
        };
        Box::pin(std::future::ready(if query.is_empty() {
            Err(full())
        } else {
            Ok(vec![shared])
        }))
    }

    fn list<'a>(&'a self, _category: Option<&'a str>) -> MemoryFuture<'a, Vec<Remembered>> {
        Box::pin(std::future::ready(Err(full())))
    }

    fn forget<'a>(&'a self, _key: &'a str) -> MemoryFuture<'a, Forgot> {
        Box::pin(std::future::ready(Err(full())))
    }
}

#[tokio::test]
async fn a_store_that_refuses_reaches_the_model_as_its_code_and_a_writer_is_named() {
    let tools = built(Tools::builder().with_memory(&Full), &["memory_*"]);

    let refused = [
        run(
            &tools,
            "memory_store",
            json!({"key": "k", "content": "c"}),
            None,
        )
        .await,
        run(&tools, "memory_recall", json!({"query": ""}), None).await,
        run(&tools, "memory_list", json!({}), None).await,
        run(&tools, "memory_forget", json!({"key": "k"}), None).await,
    ];
    let shared = run(&tools, "memory_recall", json!({"query": "escalate"}), None).await;

    for output in &refused {
        assert_eq!(output.text, "[memory_full] the store is full");
        assert_eq!(output.error_code, Some(code::MEMORY_FULL));
    }
    assert_eq!(
        shared.text,
        "team (core, shared by Support triage): escalate"
    );
}
