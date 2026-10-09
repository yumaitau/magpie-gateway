//! Visual policy to CEL, plus a check against a sample request.
//!
//! Tool allow lists use `mcp.tool.name`, which agentgateway already evaluates.
//! Marking and region clauses are for the Magpie decision and for the portal
//! tester. The proxy does not have those attributes unless a guardrail puts
//! them somewhere CEL can see, so the generated snippet says which clauses
//! the proxy can enforce on its own.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{Error, Result};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Effect {
	Allow,
	Deny,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Clause {
	pub field: String,
	pub op: String,
	pub value: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct VisualPolicy {
	pub name: String,
	pub effect: Effect,
	pub all: Vec<Clause>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct GeneratedCel {
	pub expression: String,
	pub proxy_ready: bool,
	pub note: String,
}

const PROXY_FIELDS: &[&str] = &["mcp.tool.name", "jwt.sub", "jwt.email", "jwt.groups"];

pub fn generate(policy: &VisualPolicy) -> Result<GeneratedCel> {
	if policy.all.is_empty() {
		return Err(Error::Msg(
			"A visual policy needs at least one clause.".into(),
		));
	}
	let mut parts = Vec::new();
	let mut proxy_ready = true;
	for clause in &policy.all {
		if !PROXY_FIELDS.contains(&clause.field.as_str()) {
			proxy_ready = false;
		}
		parts.push(clause_to_cel(clause)?);
	}
	let body = parts.join(" && ");
	let expression = match policy.effect {
		Effect::Allow => body,
		Effect::Deny => format!("!({body})"),
	};
	let note = if proxy_ready {
		"Every clause uses an attribute agentgateway already exposes.".to_string()
	} else {
		"Marking and region clauses are enforced by the Magpie guardrail. The proxy snippet only covers tool and JWT clauses.".to_string()
	};
	Ok(GeneratedCel {
		expression,
		proxy_ready,
		note,
	})
}

fn clause_to_cel(clause: &Clause) -> Result<String> {
	let field = match clause.field.as_str() {
		"mcp.tool.name" | "jwt.sub" | "jwt.email" | "marking" | "destination.region" => {
			clause.field.as_str()
		},
		"jwt.groups" => "jwt.groups",
		other => return Err(Error::Msg(format!("Unknown policy field '{other}'."))),
	};
	match clause.op.as_str() {
		"eq" => Ok(format!("{field} == {}", literal(&clause.value)?)),
		"in" => Ok(format!("{field} in {}", list(&clause.value)?)),
		"not_in" => Ok(format!("!({field} in {})", list(&clause.value)?)),
		"contains" => Ok(format!("{field}.contains({})", literal(&clause.value)?)),
		other => Err(Error::Msg(format!("Unknown operator '{other}'."))),
	}
}

fn literal(value: &Value) -> Result<String> {
	match value {
		Value::String(text) => Ok(format!("'{}'", text.replace('\'', "\\'"))),
		Value::Bool(flag) => Ok(flag.to_string()),
		Value::Number(number) => Ok(number.to_string()),
		_ => Err(Error::Msg(
			"Clause values must be strings, numbers or booleans.".into(),
		)),
	}
}

fn list(value: &Value) -> Result<String> {
	let items = value
		.as_array()
		.ok_or_else(|| Error::Msg("This operator expects a list.".into()))?;
	let mut rendered = Vec::new();
	for item in items {
		rendered.push(literal(item)?);
	}
	Ok(format!("[{}]", rendered.join(", ")))
}

pub fn tool_allow_list(tools: &[String]) -> String {
	if tools.is_empty() {
		return "false".to_string();
	}
	let items: Vec<String> = tools
		.iter()
		.map(|tool| format!("'{}'", tool.replace('\'', "\\'")))
		.collect();
	format!("mcp.tool.name in [{}]", items.join(", "))
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CelTestResult {
	pub compiled: bool,
	pub value: Option<Value>,
	pub error: Option<String>,
}

pub fn test_expression(source: &str, sample: &Value) -> CelTestResult {
	let program = match cel::Program::compile(source) {
		Ok(program) => program,
		Err(error) => {
			return CelTestResult {
				compiled: false,
				value: None,
				error: Some(error.to_string()),
			};
		},
	};
	let Some(object) = sample.as_object() else {
		return CelTestResult {
			compiled: true,
			value: None,
			error: Some("The sample request must be a JSON object of variables.".into()),
		};
	};
	let mut resolver = cel::context::MapResolver::new();
	for (key, value) in object {
		match cel::to_value(value.clone()) {
			Ok(converted) => resolver.add_variable_from_value(key.as_str(), converted),
			Err(error) => {
				return CelTestResult {
					compiled: true,
					value: None,
					error: Some(error.to_string()),
				};
			},
		}
	}
	let context = cel::Context::default();
	match program.execute_with(&context, &resolver) {
		Ok(value) => match serde_json::to_value(&value) {
			Ok(json) => CelTestResult {
				compiled: true,
				value: Some(json),
				error: None,
			},
			Err(error) => CelTestResult {
				compiled: true,
				value: None,
				error: Some(error.to_string()),
			},
		},
		Err(error) => CelTestResult {
			compiled: true,
			value: None,
			error: Some(error.to_string()),
		},
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use serde_json::json;

	#[test]
	fn visual_tool_rule_compiles_and_matches_a_sample() {
		let policy = VisualPolicy {
			name: "example tools".into(),
			effect: Effect::Allow,
			all: vec![Clause {
				field: "mcp.tool.name".into(),
				op: "in".into(),
				value: json!(["example_search"]),
			}],
		};
		let generated = generate(&policy).unwrap();
		assert!(generated.proxy_ready);
		assert_eq!(generated.expression, "mcp.tool.name in ['example_search']");
		let result = test_expression(
			&generated.expression,
			&json!({"mcp": {"tool": {"name": "example_search"}}}),
		);
		assert!(result.compiled, "{result:?}");
		assert_eq!(result.value, Some(json!(true)));
		assert!(result.error.is_none(), "{result:?}");
	}

	#[test]
	fn invalid_cel_is_reported() {
		let result = test_expression("mcp.tool.name ==", &json!({}));
		assert!(!result.compiled);
		assert!(result.error.is_some());
	}

	#[test]
	fn empty_allow_list_denies() {
		assert_eq!(tool_allow_list(&[]), "false");
	}
}
