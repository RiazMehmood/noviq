//! Feature 009 — template persistence (additive tables from migration 021).
//!
//! Lifecycle (ADR / research R4): create-on-confirm, list, get, overwrite-on-
//! reconfirm (no version history in v1), delete. Gold-example CONTENT never
//! enters the retrieval corpus (SC-007) — see [`super::gold`].

use super::slot::{Slot, SlotKind};
use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use crate::db::Db;

/// A reusable template: header + ordered slots.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TemplateSchema {
    pub id: Option<i64>,
    pub name: String,
    /// "summary" | "note".
    pub instrument_type: String,
    pub source_format_ref: Option<String>,
    pub slots: Vec<Slot>,
}

impl TemplateSchema {
    pub fn validate(&self) -> Result<()> {
        if self.name.trim().is_empty() {
            return Err(anyhow!("template name must not be empty"));
        }
        if self.instrument_type != "summary" && self.instrument_type != "note" {
            return Err(anyhow!(
                "instrument_type must be 'summary' or 'note', got '{}'",
                self.instrument_type
            ));
        }
        if self.slots.is_empty() {
            return Err(anyhow!("template must have at least one slot"));
        }
        for s in &self.slots {
            s.validate().map_err(|e| anyhow!(e))?;
        }
        Ok(())
    }
}

/// Lightweight header for list views.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TemplateHeader {
    pub id: i64,
    pub name: String,
    pub instrument_type: String,
    pub updated_at: String,
}

/// Persist a template (create or overwrite). Returns the template id.
pub async fn save_template(pool: &Db, tpl: &TemplateSchema) -> Result<i64> {
    tpl.validate()?;
    let now = chrono::Utc::now().to_rfc3339();
    let mut txn = pool.begin().await?;

    let template_id = match tpl.id {
        Some(id) => {
            sqlx::query(
                "UPDATE template_schema
                 SET name = $1, instrument_type = $2, source_format_ref = $3, updated_at = $4
                 WHERE id = $5",
            )
            .bind(&tpl.name)
            .bind(&tpl.instrument_type)
            .bind(&tpl.source_format_ref)
            .bind(&now)
            .bind(id)
            .execute(&mut *txn)
            .await?;
            // Overwrite-on-reconfirm: replace all slots.
            sqlx::query("DELETE FROM template_slot WHERE template_id = $1")
                .bind(id)
                .execute(&mut *txn)
                .await?;
            id
        }
        None => {
            let id: i64 = sqlx::query_scalar(
                "INSERT INTO template_schema (name, instrument_type, source_format_ref, created_at, updated_at)
                 VALUES ($1, $2, $3, $4, $4) RETURNING id",
            )
            .bind(&tpl.name)
            .bind(&tpl.instrument_type)
            .bind(&tpl.source_format_ref)
            .bind(&now)
            .fetch_one(&mut *txn)
            .await?;
            id
        }
    };

    for s in &tpl.slots {
        sqlx::query(
            "INSERT INTO template_slot
               (template_id, ordering, kind, label, boilerplate_text, required_source_category, prose_guidance, provision_ref)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8)",
        )
        .bind(template_id)
        .bind(s.ordering)
        .bind(s.kind.as_str())
        .bind(&s.label)
        .bind(&s.boilerplate_text)
        .bind(&s.required_source_category)
        .bind(&s.prose_guidance)
        .bind(&s.provision_ref)
        .execute(&mut *txn)
        .await?;
    }

    txn.commit().await?;
    Ok(template_id)
}

/// List template headers (most recently updated first).
pub async fn list_templates(pool: &Db) -> Result<Vec<TemplateHeader>> {
    let rows: Vec<(i64, String, String, String)> = sqlx::query_as(
        "SELECT id, name, instrument_type, updated_at
         FROM template_schema ORDER BY updated_at DESC",
    )
    .fetch_all(pool)
    .await?;
    Ok(rows
        .into_iter()
        .map(|(id, name, instrument_type, updated_at)| TemplateHeader {
            id,
            name,
            instrument_type,
            updated_at,
        })
        .collect())
}

/// Load a full template (header + ordered slots).
pub async fn get_template(pool: &Db, id: i64) -> Result<TemplateSchema> {
    let header: Option<(i64, String, String, Option<String>)> = sqlx::query_as(
        "SELECT id, name, instrument_type, source_format_ref FROM template_schema WHERE id = $1",
    )
    .bind(id)
    .fetch_optional(pool)
    .await?;
    let (id, name, instrument_type, source_format_ref) =
        header.ok_or_else(|| anyhow!("template {id} not found"))?;

    let slot_rows: Vec<(
        i64,
        String,
        Option<String>,
        Option<String>,
        Option<String>,
        Option<String>,
        Option<String>,
    )> = sqlx::query_as(
        "SELECT ordering, kind, label, boilerplate_text, required_source_category, prose_guidance, provision_ref
         FROM template_slot WHERE template_id = $1 ORDER BY ordering ASC",
    )
    .bind(id)
    .fetch_all(pool)
    .await?;

    let mut slots = Vec::with_capacity(slot_rows.len());
    for (
        ordering,
        kind,
        label,
        boilerplate_text,
        required_source_category,
        prose_guidance,
        provision_ref,
    ) in slot_rows
    {
        let kind = SlotKind::from_str(&kind)
            .ok_or_else(|| anyhow!("unknown slot kind '{kind}' for template {id}"))?;
        slots.push(Slot {
            ordering,
            kind,
            label,
            boilerplate_text,
            required_source_category,
            prose_guidance,
            provision_ref,
        });
    }

    Ok(TemplateSchema {
        id: Some(id),
        name,
        instrument_type,
        source_format_ref,
        slots,
    })
}

/// Delete a template (cascades slots; nulls gold_example association).
pub async fn delete_template(pool: &Db, id: i64) -> Result<()> {
    sqlx::query("DELETE FROM template_schema WHERE id = $1")
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::test_pool;

    fn sample() -> TemplateSchema {
        TemplateSchema {
            id: None,
            name: "Summary to Chief Secretary".into(),
            instrument_type: "summary".into(),
            source_format_ref: None,
            slots: vec![
                Slot {
                    ordering: 0,
                    kind: SlotKind::Boilerplate,
                    label: Some("HEADER".into()),
                    boilerplate_text: Some("SUMMARY FOR THE CHIEF SECRETARY".into()),
                    required_source_category: None,
                    prose_guidance: None,
                    provision_ref: None,
                },
                Slot {
                    ordering: 1,
                    kind: SlotKind::Cited,
                    label: Some("POLICY / RULES".into()),
                    boilerplate_text: None,
                    required_source_category: Some("financial-powers rules".into()),
                    prose_guidance: None,
                    provision_ref: None,
                },
            ],
        }
    }

    #[tokio::test]
    async fn save_list_get_delete_roundtrip() {
        let pool = test_pool().await;
        let id = save_template(&pool, &sample()).await.unwrap();

        let headers = list_templates(&pool).await.unwrap();
        assert_eq!(headers.len(), 1);
        assert_eq!(headers[0].name, "Summary to Chief Secretary");

        let loaded = get_template(&pool, id).await.unwrap();
        assert_eq!(loaded.slots.len(), 2);
        assert_eq!(loaded.slots[1].kind, SlotKind::Cited);
        assert_eq!(
            loaded.slots[1].required_source_category.as_deref(),
            Some("financial-powers rules")
        );

        delete_template(&pool, id).await.unwrap();
        assert!(list_templates(&pool).await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn overwrite_on_reconfirm_replaces_slots() {
        let pool = test_pool().await;
        let id = save_template(&pool, &sample()).await.unwrap();
        let mut updated = get_template(&pool, id).await.unwrap();
        updated.slots.truncate(1); // drop the cited slot
        save_template(&pool, &updated).await.unwrap();
        let reloaded = get_template(&pool, id).await.unwrap();
        assert_eq!(reloaded.slots.len(), 1);
    }
}
