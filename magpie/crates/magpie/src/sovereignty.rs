//! Destination region tags and sovereign mode.
//!
//! Australian region names below are the ones operators asked for, plus the
//! current Azure Australia region names. An empty or unknown region is not
//! treated as Australian.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Residency {
	Australia,
	Other,
	Unknown,
}

const AUSTRALIAN_REGIONS: &[&str] = &[
	"ap-southeast-2",
	"ap-southeast-4",
	"australiaeast",
	"australiasoutheast",
	"australiacentral",
	"australiacentral2",
	"on-prem-au",
	"au",
];

pub fn residency(region: &str) -> Residency {
	let normalised = region.trim().to_ascii_lowercase();
	if normalised.is_empty() {
		return Residency::Unknown;
	}
	if AUSTRALIAN_REGIONS.contains(&normalised.as_str()) {
		Residency::Australia
	} else {
		Residency::Other
	}
}

pub fn residency_label(region: &str) -> &'static str {
	match residency(region) {
		Residency::Australia => "Australia",
		Residency::Other => "Outside Australia",
		Residency::Unknown => "Unknown",
	}
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RouteCheck {
	pub allowed: bool,
	pub reason: String,
}

/// `OFFICIAL: Sensitive` and above stay on Australian regions.
/// `PROTECTED` also needs a destination on the approved list.
/// SECRET/TOP SECRET are refused: a PROTECTED approval is not authority to
/// process higher classifications in this deployment.
/// Sovereign mode denies every non-Australian destination.
pub fn check_route(
	marking: Option<crate::pspf::Marking>,
	region: &str,
	destination: &str,
	approved_protected: &[String],
	sovereign_mode: bool,
) -> RouteCheck {
	if marking.is_some_and(|m| m.at_least(crate::pspf::Marking::Secret)) {
		return RouteCheck { allowed: false, reason: "SECRET and TOP SECRET are not authorised for this gateway. A PROTECTED destination approval does not authorise higher classifications.".into() };
	}
	let where_hosted = residency(region);
	if sovereign_mode && where_hosted != Residency::Australia {
		return RouteCheck {
			allowed: false,
			reason: format!(
				"Sovereign mode blocks {destination} because its region '{region}' is not in Australia."
			),
		};
	}
	if let Some(marking) = marking {
		if marking.at_least(crate::pspf::Marking::OfficialSensitive)
			&& where_hosted != Residency::Australia
		{
			return RouteCheck {
				allowed: false,
				reason: format!(
					"{} and above may only go to a model hosted in Australia. {destination} is tagged '{region}'.",
					crate::pspf::Marking::OfficialSensitive.as_str()
				),
			};
		}
		if marking.at_least(crate::pspf::Marking::Protected)
			&& !approved_protected.iter().any(|name| name == destination)
		{
			return RouteCheck {
				allowed: false,
				reason: format!(
					"{} and above is blocked unless {destination} is on the approved list.",
					crate::pspf::Marking::Protected.as_str()
				),
			};
		}
	}
	RouteCheck {
		allowed: true,
		reason: "Destination is allowed for this marking and residency setting.".to_string(),
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::pspf::Marking;

	#[test]
	fn known_australian_regions() {
		for region in [
			"ap-southeast-2",
			"ap-southeast-4",
			"australiaeast",
			"australiasoutheast",
			"on-prem-au",
		] {
			assert_eq!(residency(region), Residency::Australia, "{region}");
		}
		assert_eq!(residency("us-east-1"), Residency::Other);
		assert_eq!(residency("  "), Residency::Unknown);
	}

	#[test]
	fn sensitive_traffic_stays_in_australia() {
		let blocked = check_route(
			Some(Marking::OfficialSensitive),
			"us-east-1",
			"example-us",
			&[],
			false,
		);
		assert!(!blocked.allowed);
		let allowed = check_route(
			Some(Marking::OfficialSensitive),
			"ap-southeast-2",
			"example-au",
			&[],
			false,
		);
		assert!(allowed.allowed);
	}

	#[test]
	fn protected_needs_an_approved_destination_even_in_australia() {
		let blocked = check_route(
			Some(Marking::Protected),
			"ap-southeast-2",
			"example-au",
			&["other".to_string()],
			false,
		);
		assert!(!blocked.allowed);
		let allowed = check_route(
			Some(Marking::Protected),
			"ap-southeast-2",
			"example-au",
			&["example-au".to_string()],
			false,
		);
		assert!(allowed.allowed);
	}

	#[test]
	fn sovereign_mode_blocks_unknown_regions() {
		let blocked = check_route(None, "", "mystery", &[], true);
		assert!(!blocked.allowed);
	}
}

#[cfg(test)]
mod higher_classification_tests {
	use super::*;
	#[test]
	fn protected_approval_never_authorises_secret_or_top_secret() {
		for marking in [
			crate::pspf::Marking::Secret,
			crate::pspf::Marking::TopSecret,
		] {
			let result = check_route(
				Some(marking),
				"ap-southeast-2",
				"approved-au-model",
				&["approved-au-model".into()],
				false,
			);
			assert!(!result.allowed);
			assert!(result.reason.contains("not authorised"));
		}
	}
}
