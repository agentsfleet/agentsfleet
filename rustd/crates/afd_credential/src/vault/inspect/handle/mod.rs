//! Readiness inspects public binding metadata and wipes decoded refresh tokens.

use crate::{
    credential::{github, oauth},
    secrets::{Exchange, Supply},
};
use serde::Deserialize;
use serde_json::value::RawValue;
use zeroize::Zeroizing;

#[derive(Deserialize)]
pub(super) struct Handle<'a> {
    #[serde(borrow)]
    integration: Option<&'a RawValue>,
    #[serde(borrow)]
    installation_id: Option<&'a RawValue>,
    #[serde(borrow)]
    accounts_base: Option<&'a RawValue>,
    #[serde(borrow)]
    refresh_token: Option<&'a RawValue>,
    #[serde(borrow)]
    team_id: Option<&'a RawValue>,
    #[serde(borrow)]
    bot_token: Option<&'a RawValue>,
    #[serde(borrow)]
    webhook_secret: Option<&'a RawValue>,
}

impl Handle<'_> {
    pub(super) fn integration(&self) -> Option<String> {
        self.integration
            .and_then(|raw| serde_json::from_str(raw.get()).ok())
    }

    pub(super) fn connection_id(&self) -> Option<String> {
        if self.integration().as_deref() == Some(crate::secrets::connector::SLACK_INTEGRATION) {
            return self
                .team_id
                .and_then(|raw| serde_json::from_str::<String>(raw.get()).ok())
                .filter(|id| !id.is_empty());
        }
        github::installation(&self.metadata()).map(|id| id.to_string())
    }

    pub(super) fn usable(&self, name: &str, supply: Supply, exchange: Option<Exchange>) -> bool {
        if self.integration().as_deref() == Some(crate::secrets::connector::SLACK_INTEGRATION) {
            return name == crate::secrets::connector::SLACK_INTEGRATION
                && self.connection_id().is_some()
                && nonempty_secret(self.bot_token);
        }
        if supply != Supply::Inline && self.integration().as_deref() != Some(name) {
            return false;
        }
        match exchange {
            Some(Exchange::GithubApp) => self.connection_id().is_some(),
            Some(Exchange::OAuthRefresh { .. }) => {
                self.has_refresh_token()
                    && match oauth::steer(&self.metadata()) {
                        oauth::Steer::Declared => true,
                        oauth::Steer::To(base) => {
                            self.integration().as_deref() == Some("zoho")
                                && oauth::is_zoho_accounts_server(base)
                        }
                    }
            }
            _ => true,
        }
    }

    pub(super) fn webhook_usable(&self) -> bool {
        nonempty_secret(self.webhook_secret)
    }

    pub(super) fn svix_usable(&self) -> bool {
        self.webhook_secret
            .and_then(|raw| serde_json::from_str::<String>(raw.get()).ok())
            .map(Zeroizing::new)
            .is_some_and(|secret| afd_webhook::vendor::svix::SvixSecret::parse(&secret).is_some())
    }

    fn has_refresh_token(&self) -> bool {
        nonempty_secret(self.refresh_token)
    }

    fn metadata(&self) -> serde_json::Value {
        let mut fields = serde_json::Map::new();
        for (name, value) in [
            ("installation_id", self.installation_id),
            (
                afd_core::credential::FIELD_ACCOUNTS_BASE,
                self.accounts_base,
            ),
        ] {
            if let Some(value) = value.and_then(|raw| serde_json::from_str(raw.get()).ok()) {
                fields.insert(name.to_owned(), value);
            }
        }
        serde_json::Value::Object(fields)
    }
}

fn nonempty_secret(value: Option<&RawValue>) -> bool {
    value
        .and_then(|raw| serde_json::from_str::<String>(raw.get()).ok())
        .map(Zeroizing::new)
        .is_some_and(|token| !token.is_empty())
}

#[cfg(test)]
mod tests;
