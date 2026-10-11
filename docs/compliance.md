# Compliance evidence

The monthly report is produced by `magpie::evidence`. Every row names a
source and says whether the link is direct or interpretive. A gateway
does not, on its own, satisfy an ISM control or an Essential Eight
strategy.

## Sources, checked 9 October 2026

- ISM September 2026, including ISM-2159 for logging agentic tool
  invocations, external requests and outputs:
  https://www.cyber.gov.au/ism/2026-09
- ISM-1988, ISM-1985 (access) and ISM-1815 (modification/deletion), searchable retention and protecting logs:
  https://www.cyber.gov.au/ism/2026-09/guidelines-for-security-assurance
- Essential Eight maturity model FAQ, which still points at the
  November 2023 model:
  https://www.cyber.gov.au/business-government/asds-cyber-security-frameworks/essential-eight/essential-eight-maturity-model-faq
- DTA Policy for the responsible use of AI in government 2.0, effective
  15 December 2025:
  https://www.digital.gov.au/ai/ai-in-government-policy
- Standard for accountability 2.0, page last updated 2 December 2025:
  https://www.digital.gov.au/ai/ai-in-government-policy/accountability
- Agentic AI addendum to the AI technical standard, best-practice
  guidance, page last updated 4 June 2026:
  https://www.digital.gov.au/policy/ai/agentic-ai-addendum
  Statement pages checked the same day: AGT.3 Design
  (https://www.digital.gov.au/policy/ai/agentic-ai-addendum-statements-design),
  AGT.4 Data
  (https://www.digital.gov.au/policy/ai/agentic-ai-addendum-statements-data),
  AGT.7 Integrate
  (https://www.digital.gov.au/policy/ai/agentic-ai-addendum-statements-integrate)
  and AGT.8 Monitor
  (https://www.digital.gov.au/policy/ai/agentic-ai-addendum-statements-monitor).
  Those identifiers are the DTA's. Mapping a Magpie control onto a
  statement is interpretive. The addendum does not name this product.

## How to produce a file

From the repository:

```bash
cargo run --manifest-path magpie/Cargo.toml --bin magpie-evidence -- /tmp/magpie-evidence.pdf > /tmp/magpie-evidence.csv
```

The binary always uses zero counts. The path argument is only where to
write the PDF. The PDF is labelled as an example operational report.
Replace the counts with a query over `audit_events` before you give
the file to anyone.
The CSV columns are framework, reference, source, magpieControl,
observation and relationship.

## What "direct" means here

The use-case export uses the accountability standard's field list. That
row is marked direct for the field list only. The agency still owns the
register, the accountable official and the impact assessment. Every
other row in the report is interpretive.
