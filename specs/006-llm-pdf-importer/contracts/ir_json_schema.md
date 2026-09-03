# Contract: `DocIr` JSON Schema

Used by both llguidance (constrained sampling) and the schema-pass validator. Single source of truth.

**File**: `src-tauri/src/services/doc_ir/schema.json` (loaded at build time via `include_str!`).

```jsonc
{
  "$schema": "https://json-schema.org/draft/2020-12/schema",
  "type": "object",
  "required": ["doc_id", "doc_type", "title", "source_pdf", "extracted_at", "llm_model", "schema_version", "nodes"],
  "properties": {
    "doc_id":        { "type": "string", "pattern": "^[a-f0-9]{64}$" },
    "doc_type":      { "type": "string", "enum": ["constitution","act","rules","sro","contract","generic"] },
    "title":         { "type": "string", "minLength": 1 },
    "source_pdf":    { "type": "string" },
    "extracted_at":  { "type": "string", "format": "date-time" },
    "llm_model":     { "type": "string" },
    "schema_version":{ "type": "integer", "const": 1 },
    "doc_metadata":  { "type": "object" },
    "nodes": {
      "type": "array",
      "minItems": 1,
      "items": { "$ref": "#/$defs/StructureNode" }
    }
  },
  "$defs": {
    "StructureNode": {
      "type": "object",
      "required": ["id", "kind", "text", "page", "quality_flag"],
      "properties": {
        "id":        { "type": "string", "pattern": "^n[0-9]{4,}$" },
        "parent_id": { "type": ["string","null"], "pattern": "^n[0-9]{4,}$" },
        "kind":      { "type": "string", "enum": [
          "part","chapter","section","subsection","clause","sub_clause",
          "proviso","explanation","schedule","preamble","paragraph","heading","footnote"
        ]},
        "number":    { "type": ["string","null"] },
        "title":     { "type": ["string","null"] },
        "text":      { "type": "string" },
        "page":      { "type": "integer", "minimum": 1 },
        "annotations": {
          "type": "array",
          "items": { "$ref": "#/$defs/Annotation" }
        },
        "footnote_refs": {
          "type": "array",
          "items": { "type": "string", "pattern": "^n[0-9]{4,}$" }
        },
        "metadata":      { "type": "object" },
        "quality_flag":  { "type": "string", "enum": ["ok","low_confidence","user_repaired"] },
        "quality_error": { "type": ["string","null"] }
      }
    },
    "Annotation": {
      "type": "object",
      "required": ["type", "by"],
      "properties": {
        "type":        { "type": "string", "enum": ["insertion","substitution","omission","renumbering","proviso_added"] },
        "by":          { "type": "string", "minLength": 1 },
        "by_date":     { "type": ["string","null"], "format": "date" },
        "footnote_id": { "type": ["string","null"], "pattern": "^n[0-9]{4,}$" },
        "original":    { "type": ["string","null"] },
        "note":        { "type": ["string","null"] }
      }
    }
  }
}
```

Constraints **not** expressed in the schema (handled by the semantic validator):

- Parent-id integrity (no orphan, no cycle)
- Id uniqueness
- Monotonic-ish numbering within a parent
- ≥90% text coverage
- Footnote ref resolves to a `kind: "footnote"` node
- Omission marker rule (empty leaf text requires `annotations[].type = "omission"`)
- Page index ≤ source PDF page count
