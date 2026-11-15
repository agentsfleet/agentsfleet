//! Failed observations retain uncertainty instead of reporting a known zero.

use super::*;

fn known(input: u64, output: u64, cost: &str) -> ModelUsage {
    ModelUsage::Known {
        input_tokens: input,
        output_tokens: output,
        provider_cost_nanos: cost.to_owned(),
    }
}

fn refused(result: Result<()>, expected: &str) -> Result<()> {
    let Err(failure) = result else {
        return Err(error::config("invalid observation was accepted"));
    };
    assert!(matches!(failure.kind(), error::ErrorKind::Config { detail } if *detail == expected));
    Ok(())
}

#[test]
fn malformed_cost_is_rejected_without_changing_the_known_observation() -> Result<()> {
    let observed = Observed::new();
    let first = known(3, 4, "5");
    garde::Validate::validate(&first)?;
    observed.accept(&first)?;
    let before = observed.snapshot()?;
    // Negative leaf check: the native client already rejects this malformed cost.
    refused(
        observed.accept(&known(1, 1, "not-money")),
        "the daemon returned an invalid provider cost",
    )?;
    let after = observed.snapshot()?;
    assert_eq!(after.knowledge, Knowledge::Known);
    assert_eq!(after.calls, before.calls);
    assert_eq!(after.input_tokens, before.input_tokens);
    assert_eq!(after.output_tokens, before.output_tokens);
    assert_eq!(after.provider_cost_nanos, before.provider_cost_nanos);
    Ok(())
}

#[test]
fn a_later_known_call_cannot_erase_an_earlier_uncertain_dispatch() -> Result<()> {
    let observed = Observed::new();
    observed.accept(&known(3, 4, "5"))?;
    observed.uncertain()?;
    refused(
        observed.accept(&known(1, 1, "1")),
        "known provider usage followed an uncertain dispatch",
    )?;
    let snapshot = observed.snapshot()?;
    assert_eq!(snapshot.knowledge, Knowledge::Unknown);
    assert_eq!(snapshot.calls, 1);
    assert!(snapshot.input_tokens.is_none());
    assert!(snapshot.output_tokens.is_none());
    assert!(snapshot.provider_cost_nanos.is_none());
    Ok(())
}

#[test]
fn an_interrupted_update_makes_every_observation_refuse_the_poisoned_state() -> Result<()> {
    let observed = Observed::new();
    let interrupted = std::panic::catch_unwind(|| {
        let Ok(_held) = observed.meter.lock() else {
            return;
        };
        std::panic::resume_unwind(Box::new("fixture observation interruption"));
    });
    let Err(_) = interrupted else {
        return Err(error::config("fixture update did not unwind"));
    };
    for result in [
        observed.snapshot().map(drop),
        observed.uncertain(),
        observed.accept(&known(1, 1, "1")),
    ] {
        refused(result, METER_INTERRUPTED)?;
    }
    Ok(())
}
