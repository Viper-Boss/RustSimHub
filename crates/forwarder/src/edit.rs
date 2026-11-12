// SPDX-License-Identifier: AGPL-3.0-only
//! Owner editing input. Retained credentials are resolved only inside Core.
use crate::*;
use serde::Deserialize;
use std::collections::HashMap;

#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum DestinationEdit {
    Keep {},
    Webhook {
        url: Secret,
        bearer: Option<Secret>,
    },
    Bark {
        server: String,
        key: Secret,
    },
    Telegram {
        bot_token: Secret,
        chat_id: Secret,
    },
    Dingtalk {
        access_token: Secret,
        secret: Option<Secret>,
    },
    Wecom {
        key: Secret,
    },
    Feishu {
        hook: Secret,
        secret: Option<Secret>,
    },
    Serverchan {
        send_key: Secret,
    },
    Pushplus {
        token: Secret,
    },
    Gotify {
        server: String,
        token: Secret,
    },
    #[cfg(feature = "smtp-production-candidate")]
    Email {
        smtp_host: Secret,
        port: u16,
        tls: crate::smtp_candidate::EmailTls,
        username: Secret,
        password: Secret,
        mail_from: Secret,
        rcpt_to: Secret,
    },
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TargetEdit {
    pub id: String,
    pub destination: DestinationEdit,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PolicyEdit {
    pub rules_revision: Option<u32>,
    pub enabled: bool,
    pub targets: Vec<TargetEdit>,
    pub rules: Vec<Rule>,
}
impl PolicyEdit {
    /// Existing destinations are moved, never returned to the browser. The
    /// subsequent configure transaction still enforces generation/request ID.
    pub fn resolve(self, previous: Policy) -> Result<Policy> {
        previous.validate()?;
        if self.rules_revision.is_some_and(|v| v != RULES_REVISION)
            || ((previous.rules.iter().any(Rule::extended)
                || self.rules.iter().any(Rule::extended))
                && self.rules_revision != Some(RULES_REVISION))
        {
            return Err(Error::Invalid);
        }
        if self.targets.len() > MAX_TARGETS || self.rules.len() > MAX_RULES {
            return Err(Error::Invalid);
        }
        let mut old: HashMap<_, _> = previous
            .targets
            .into_iter()
            .map(|t| (t.id, t.destination))
            .collect();
        let mut targets = Vec::new();
        for target in self.targets {
            let destination = match target.destination {
                DestinationEdit::Keep {} => old.remove(&target.id).ok_or(Error::Invalid)?,
                DestinationEdit::Webhook { url, bearer } => Destination::Webhook { url, bearer },
                DestinationEdit::Bark { server, key } => Destination::Bark { server, key },
                DestinationEdit::Telegram { bot_token, chat_id } => {
                    Destination::Telegram { bot_token, chat_id }
                }
                DestinationEdit::Dingtalk {
                    access_token,
                    secret,
                } => Destination::Dingtalk {
                    access_token,
                    secret,
                },
                DestinationEdit::Wecom { key } => Destination::Wecom { key },
                DestinationEdit::Feishu { hook, secret } => Destination::Feishu { hook, secret },
                DestinationEdit::Serverchan { send_key } => Destination::Serverchan { send_key },
                DestinationEdit::Pushplus { token } => Destination::Pushplus { token },
                DestinationEdit::Gotify { server, token } => Destination::Gotify { server, token },
                #[cfg(feature = "smtp-production-candidate")]
                DestinationEdit::Email {
                    smtp_host,
                    port,
                    tls,
                    username,
                    password,
                    mail_from,
                    rcpt_to,
                } => Destination::Email {
                    smtp_host,
                    port,
                    tls,
                    username,
                    password,
                    mail_from,
                    rcpt_to,
                },
            };
            targets.push(Target {
                id: target.id,
                destination,
            });
        }
        let policy = Policy {
            enabled: self.enabled,
            targets,
            rules: self.rules,
        };
        policy.validate()?;
        Ok(policy)
    }
}
