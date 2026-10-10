//! One decision for an LLM message or an MCP tool call.
//!
//! The portal speaks agentgateway's LLM webhook JSON. MCP uses the same
//! decision over a small JSON check. A native gRPC ExtMcp server is not in
//! this cut. See docs/plan.md.

use std::collections::BTreeMap;

use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::approvals::{self, Approval, ItemKind};
use crate::audit::AuditEvent;
use crate::dlp::{self, Action, DlpRule, PublicHit};
use crate::presidio::PresidioEntity;
use crate::safety::{self, ArgumentRule, InjectionHit, KillSwitch, RateLimiter};
use crate::sovereignty;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Phase {
	Request,
	Response,
}

#[derive(Debug, Clone)]
pub struct Inspect<'a> {
	pub text: &'a str,
	pub metadata: &'a BTreeMap<String, String>,
	pub agent_id: &'a str,
	pub tool: Option<&'a str>,
	pub model: Option<&'a str>,
	pub destination_region: &'a str,
	pub destination_name: &'a str,
	pub arguments: Option<&'a Value>,
	pub phase: Phase,
	pub now: DateTime<Utc>,
	pub actor: &'a str,
}

#[derive(Debug, Clone)]
pub struct PolicySet<'a> {
	pub rules: &'a [DlpRule],
	pub approvals: &'a [Approval],
	pub approved_protected: &'a [String],
	pub sovereign_mode: bool,
	pub kill: &'a KillSwitch,
	pub limiter: Option<&'a RateLimiter>,
	pub argument_rules: &'a [ArgumentRule],
	pub presidio: &'a [PresidioEntity],
	pub token_key: &'a [u8],
	pub scan_injection: bool,
	pub extra_injection: &'a [InjectionHit],
}

#[derive(Debug, Clone)]
pub struct Outcome {
	pub action: Action,
	pub output: String,
	pub reason: String,
	pub hits: Vec<PublicHit>,
	pub audit: AuditEvent,
}

pub fn decide(input: &Inspect<'_>, policy: &PolicySet<'_>) -> Outcome {
	let today = input.now.date_naive();
	if let Some(reason) = policy.kill.tripped(input.agent_id) {
		return finish(input, Action::Block, String::new(), reason, vec![]);
	}
	if let Some(tool) = input.tool {
		if !approvals::is_approved(policy.approvals, ItemKind::Tool, tool, today) {
			return finish(
				input,
				Action::Block,
				String::new(),
				format!("Tool '{tool}' is not approved, or the approval has expired."),
				vec![],
			);
		}
		if let Some(limiter) = policy.limiter
			&& let Err(error) = limiter.check(input.agent_id, tool)
		{
			return finish(
				input,
				Action::Block,
				String::new(),
				error.to_string(),
				vec![],
			);
		}
		if let Some(arguments) = input.arguments
			&& let Some(rule) = policy.argument_rules.iter().find(|rule| rule.tool == tool)
			&& let Err(error) = safety::check_arguments(rule, arguments)
		{
			return finish(
				input,
				Action::Block,
				String::new(),
				error.to_string(),
				vec![],
			);
		}
	}
	if let Some(model) = input.model
		&& !approvals::is_approved(policy.approvals, ItemKind::Model, model, today)
	{
		return finish(
			input,
			Action::Block,
			String::new(),
			format!("Model '{model}' is not approved, or the approval has expired."),
			vec![],
		);
	}
	if policy.scan_injection {
		let mut hits = safety::scan_baseline(input.text);
		hits.extend(policy.extra_injection.iter().cloned());
		if let Some(hit) = hits.first() {
			return finish(
				input,
				Action::Block,
				String::new(),
				hit.detail.clone(),
				vec![],
			);
		}
	}
	let dlp = dlp::inspect(
		input.text,
		input.metadata,
		policy.rules,
		policy.presidio,
		policy.token_key,
	);
	let route = sovereignty::check_route(
		dlp.highest_marking,
		input.destination_region,
		input.destination_name,
		policy.approved_protected,
		policy.sovereign_mode,
	);
	if !route.allowed {
		return finish(input, Action::Block, String::new(), route.reason, dlp.hits);
	}
	let reason = if dlp.hits.is_empty() {
		route.reason
	} else {
		format!(
			"{} DLP hit(s), action {}.",
			dlp.hits.len(),
			dlp.action.as_str()
		)
	};
	finish(input, dlp.action, dlp.output, reason, dlp.hits)
}

fn finish(
	input: &Inspect<'_>,
	action: Action,
	output: String,
	reason: String,
	hits: Vec<PublicHit>,
) -> Outcome {
	let decision = match action {
		Action::Block | Action::RequireApproval => "block",
		Action::Allow => "allow",
		other => other.as_str(),
	};
	let audit = AuditEvent::new(
		"decision",
		input.actor,
		input.agent_id,
		input.tool.unwrap_or(""),
		input.model.unwrap_or(""),
		decision,
		&reason,
		json!({
			"phase": format!("{:?}", input.phase),
			"hits": hits,
			"destination": input.destination_name,
			"region": input.destination_region
		}),
	);
	Outcome {
		action,
		output,
		reason,
		hits,
		audit,
	}
}

#[derive(Debug, Deserialize)]
pub struct WebhookMessage {
	pub role: String,
	pub content: String,
}

#[derive(Debug, Deserialize)]
pub struct WebhookMessages {
	pub messages: Vec<WebhookMessage>,
}

#[derive(Debug, Deserialize)]
pub struct PromptWebhook {
	pub body: WebhookMessages,
}

#[derive(Debug, Deserialize)]
pub struct ChoiceWebhook {
	pub message: WebhookMessage,
}

#[derive(Debug, Deserialize)]
pub struct ResponseWebhookBody {
	pub choices: Vec<ChoiceWebhook>,
}

#[derive(Debug, Deserialize)]
pub struct ResponseWebhook {
	pub body: ResponseWebhookBody,
}

/// Reply to the proxy's LLM guardrail webhook.
///
/// On the request side a redact, mask or tokenise decision is returned as a
/// block. The proxy's mask action rebuilds the request from plain text
/// messages, which drops tool calls, tool results and cache markers, and
/// coding agents such as Claude Code and Codex stop working. Sending the
/// unredacted request is not an option either.
pub fn webhook_response(
	outcome: &Outcome,
	_messages: &[WebhookMessage],
	response_side: bool,
) -> Value {
	let block = |message: String| {
		json!({
			"action": {
				"body": message,
				"status_code": 403,
				"reason": outcome.reason
			}
		})
	};
	match outcome.action {
		Action::Block | Action::RequireApproval => block(format!(
			"Magpie Gateway blocked this request. {}",
			outcome.reason
		)),
		Action::Redact | Action::Mask | Action::Tokenise if !response_side => block(format!(
			"Magpie Gateway blocked this request because it contains information that policy says must not leave the organisation. {} Remove it and start a new conversation, because earlier messages are sent again with each request.",
			outcome.reason
		)),
		Action::Redact | Action::Mask | Action::Tokenise => json!({
			"action": {
				"body": { "choices": [{ "message": { "role": "assistant", "content": outcome.output } }] },
				"reason": outcome.reason
			}
		}),
		Action::Allow | Action::Log => json!({
			"action": { "reason": outcome.reason }
		}),
	}
}

pub fn join_messages(messages: &[WebhookMessage]) -> String {
	messages
		.iter()
		.map(|message| format!("{}: {}", message.role, message.content))
		.collect::<Vec<_>>()
		.join("\n")
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpCheckRequest {
	pub method: String,
	pub tool: Option<String>,
	pub arguments: Option<Value>,
	pub result_text: Option<String>,
	pub agent_id: String,
	pub model: Option<String>,
	pub destination_region: String,
	pub destination_name: String,
	pub phase: String,
	pub actor: String,
}

pub fn mcp_response(outcome: &Outcome) -> Value {
	let decision = match outcome.action {
		Action::Block | Action::RequireApproval => "reject",
		Action::Redact | Action::Mask | Action::Tokenise => "mutated",
		Action::Allow | Action::Log => "pass",
	};
	json!({
		"decision": decision,
		"reason": outcome.reason,
		"text": outcome.output,
		"hits": outcome.hits
	})
}

pub fn today_utc(now: DateTime<Utc>) -> NaiveDate {
	now.date_naive()
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::approvals::ApprovalStatus;
	use crate::dlp::default_rules;
	use crate::register::UseCase;
	use crate::safety::KillSwitch;

	fn approval(kind: ItemKind, name: &str) -> Approval {
		Approval {
			id: name.into(),
			kind,
			name: name.into(),
			business_owner: "Alex Nguyen".into(),
			purpose: "Example only.".into(),
			classification: "OFFICIAL".into(),
			expires_on: "2027-01-01".into(),
			status: ApprovalStatus::Approved,
			use_case: UseCase::example(),
		}
	}

	fn inspect<'a>(
		text: &'a str,
		tool: Option<&'a str>,
		metadata: &'a BTreeMap<String, String>,
	) -> Inspect<'a> {
		Inspect {
			text,
			metadata,
			agent_id: "example-agent",
			tool,
			model: Some("example-au"),
			destination_region: "ap-southeast-2",
			destination_name: "example-au",
			arguments: None,
			phase: Phase::Request,
			now: DateTime::parse_from_rfc3339("2026-10-09T01:00:00Z")
				.unwrap()
				.with_timezone(&Utc),
			actor: "alex@example.gov.au",
		}
	}

	#[test]
	fn unapproved_tool_is_denied_and_an_approved_one_is_redacted() {
		let metadata = BTreeMap::new();
		let approvals = vec![
			approval(ItemKind::Model, "example-au"),
			approval(ItemKind::Tool, "example_search"),
		];
		let kill = KillSwitch::default();
		let rules = default_rules();
		let policy = PolicySet {
			rules: &rules,
			approvals: &approvals,
			approved_protected: &[],
			sovereign_mode: false,
			kill: &kill,
			limiter: None,
			argument_rules: &[],
			presidio: &[],
			token_key: b"k",
			scan_injection: false,
			extra_injection: &[],
		};
		let denied = decide(&inspect("hello", Some("other_tool"), &metadata), &policy);
		assert_eq!(denied.action, Action::Block);
		let text = "ABN 12 000 000 041";
		let allowed = decide(&inspect(text, Some("example_search"), &metadata), &policy);
		assert_eq!(allowed.action, Action::Redact);
		assert!(!allowed.output.contains("12 000"));
		assert!(!allowed.audit.detail.to_string().contains("12 000 000 041"));
	}

	#[test]
	fn protected_outside_the_approved_list_is_blocked() {
		let mut metadata = BTreeMap::new();
		metadata.insert("classification".into(), "PROTECTED".into());
		let approvals = vec![approval(ItemKind::Model, "example-au")];
		let kill = KillSwitch::default();
		let rules = default_rules();
		let policy = PolicySet {
			rules: &rules,
			approvals: &approvals,
			approved_protected: &[],
			sovereign_mode: true,
			kill: &kill,
			limiter: None,
			argument_rules: &[],
			presidio: &[],
			token_key: b"k",
			scan_injection: true,
			extra_injection: &[],
		};
		let input = inspect("ordinary sentence", None, &metadata);
		let outcome = decide(&input, &policy);
		assert_eq!(outcome.action, Action::Block);
		assert!(outcome.reason.contains("PROTECTED") || outcome.reason.contains("approved"));
	}

	#[test]
	fn webhook_blocks_a_prompt_that_needs_redaction() {
		let metadata = BTreeMap::new();
		let approvals = vec![approval(ItemKind::Model, "example-au")];
		let kill = KillSwitch::default();
		let rules = default_rules();
		let policy = PolicySet {
			rules: &rules,
			approvals: &approvals,
			approved_protected: &["example-au".into()],
			sovereign_mode: false,
			kill: &kill,
			limiter: None,
			argument_rules: &[],
			presidio: &[],
			token_key: b"k",
			scan_injection: false,
			extra_injection: &[],
		};
		let text = "user: ABN 12 000 000 041";
		let messages = vec![WebhookMessage {
			role: "user".into(),
			content: "ABN 12 000 000 041".into(),
		}];
		let mut input = inspect(text, None, &metadata);
		input.text = "ABN 12 000 000 041";
		let outcome = decide(&input, &policy);
		assert_eq!(outcome.action, Action::Redact);
		let body = webhook_response(&outcome, &messages, false);
		assert_eq!(body["action"]["status_code"], 403);
		let text = body["action"]["body"].as_str().unwrap();
		assert!(text.contains("new conversation"), "{text}");
		assert!(!text.contains("000 000 041"), "{text}");
		let reply = webhook_response(&outcome, &messages, true);
		assert!(
			reply["action"]["body"]["choices"][0]["message"]["content"]
				.as_str()
				.unwrap()
				.contains("[redacted]")
		);
	}
}
