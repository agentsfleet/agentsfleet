//! Separate replica hubs deliver a cancellation only to its addressed runner.
#![cfg(feature = "test-util")]

use std::time::Duration;

use af_core::{
    env::{EnvSource as _, ProcessEnv},
    id::Uuid7,
};
use afd_bus::{Command, Received, Subject, dragonfly, test_util::physical_channel};
use afd_dragonfly::{Dragonfly, DragonflyConfig, config::DragonflyRole};

type TestResult<T = ()> = afd_bus::Result<T, Box<dyn std::error::Error + Send + Sync>>;
const BOUND: Duration = Duration::from_secs(5);

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live Dragonfly: make test-integration-rustd"]
async fn a_runner_command_crosses_replicas() -> TestResult {
    let url = ProcessEnv
        .get("TEST_DRAGONFLY_URL")
        .ok_or("TEST_DRAGONFLY_URL is unset; use make test-integration-rustd")?;
    let config = DragonflyConfig::from_url(DragonflyRole::Default, url)
        .with_ca_cert_file(ProcessEnv.get("TEST_DRAGONFLY_CA_CERT").map(Into::into))
        .with_request_timeout(BOUND);
    let client = afd_dragonfly::test_util::connect_live(&config).await?;
    let publishing = dragonfly::connect(client.clone(), config.clone()).await?;
    let receiving = dragonfly::connect(client.clone(), config).await?;
    let runner = Uuid7::parse("0195b4ba-8d3a-7000-8abc-00000000c001")?;
    let addressed = Subject::RunnerCommand(runner.clone());
    let unrelated = Subject::RunnerCommand(Uuid7::parse("0195b4ba-8d3a-7000-8abc-00000000c002")?);
    let subjects = [addressed.clone(), unrelated.clone()];
    let mut recipient = receiving.subscribe(&addressed).await?;
    let mut other = receiving.subscribe(&unrelated).await?;
    subscribers(&client, &subjects, 1).await?;
    let command = Command::Cancel {
        lease_id: Uuid7::parse("0195b4ba-8d3a-7000-8abc-00000000c003")?,
    };
    command.publish(&publishing, &runner).await?;
    let Received::Message(message) = tokio::time::timeout(BOUND, recipient.recv()).await?? else {
        return Err("the addressed runner did not receive its cancellation".into());
    };
    assert_eq!(message.subject, addressed);
    assert_eq!(serde_json::from_str::<Command>(&message.payload)?, command);
    assert!(
        tokio::time::timeout(Duration::from_secs(1), other.recv())
            .await
            .is_err(),
        "a different runner must remain silent for one second"
    );
    drop(recipient);
    drop(other);
    publishing.shutdown().await?;
    receiving.shutdown().await?;
    subscribers(&client, &subjects, 0).await?;
    Ok(())
}

async fn subscribers(client: &Dragonfly, subjects: &[Subject], wanted: i64) -> TestResult {
    tokio::time::timeout(BOUND, async {
        loop {
            let mut ready = true;
            for subject in subjects {
                let channel = physical_channel(subject);
                let mut command = redis::cmd("PUBSUB");
                command.arg("NUMSUB").arg(&channel);
                let reply: Vec<redis::Value> = client.command("PUBSUB", &channel, &command).await?;
                let [redis::Value::Map(entries)] = reply.as_slice() else {
                    return Err("PUBSUB NUMSUB did not return a RESP3 map".into());
                };
                let [(name, redis::Value::Int(count))] = entries.as_slice() else {
                    return Err("PUBSUB NUMSUB did not name exactly one channel".into());
                };
                assert_eq!(name, &redis::Value::BulkString(channel.into_bytes()));
                ready &= *count == wanted;
            }
            if ready {
                return TestResult::Ok(());
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await??;
    Ok(())
}
