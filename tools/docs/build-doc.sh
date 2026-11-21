#!/usr/bin/env bash
# Builds a branded Magpie Gateway document (PDF and editable Word .docx) from numbered Markdown + metadata.yaml.
# Usage: tools/docs/build-doc.sh <source-folder> <output-name>
set -euo pipefail
repo="$(cd "$(dirname "$0")/../.." && pwd)"
src="$(cd "$1" && pwd)"
name="$2"
brand="$repo/docs/brand"
out_dir="$repo/docs/published"
mkdir -p "$out_dir"
pdf="$out_dir/$name.pdf"
docx="$out_dir/$name.docx"
meta="$src/metadata.yaml"

command -v pandoc >/dev/null || { echo "pandoc is required." >&2; exit 1; }
command -v weasyprint >/dev/null || { echo "weasyprint is required." >&2; exit 1; }
sources=("$src"/[0-9][0-9]-*.md)
value() { sed -n "s/^$1: *\"\{0,1\}\([^\"]*\)\"\{0,1\} *$/\1/p" "$meta" | head -1; }

html="$brand/.build-$name.html"
pandoc "${sources[@]}" \
  --from markdown+pipe_tables+raw_html+fenced_divs+bracketed_spans \
  --to html5 --standalone \
  --template "$brand/template.html" \
  --metadata-file "$meta" \
  --number-sections --columns=10000 --toc --toc-depth 2 --section-divs=false --syntax-highlighting=none \
  --resource-path "$src:$brand:$repo/docs" \
  --output "$html"
weasyprint --base-url "$brand/" "$html" "$pdf"
rm -f "$html"
echo "Wrote $pdf"

reference="$(mktemp -d)/reference.docx"
MAGPIE_REFERENCE_OUT="$reference" \
MAGPIE_DOC_TITLE="$(value running-title)" \
MAGPIE_DOC_VERSION="$(value version)" \
MAGPIE_DOC_DATE="$(value date)" \
  python3 "$brand/make_reference_docx.py" >/dev/null
(cd "$brand" && pandoc "${sources[@]}" \
  --from markdown+pipe_tables+fenced_divs+bracketed_spans \
  --to docx \
  --reference-doc "$reference" \
  --metadata-file "$meta" \
  --lua-filter "$brand/docx.lua" \
  --number-sections \
  --resource-path "$brand:$src:$repo/docs" \
  --output "$docx")
rm -rf "$(dirname "$reference")"
echo "Wrote $docx"
