use boardflow_domain::models::run_check::{RunCheckFinding, RunCheckFindingListRow};
use uuid::Uuid;

/// List findings for a run_check with cursor pagination and optional severity filter.
/// Cursor: (sort_index, id). Order: sort_index ASC, id ASC.
/// bbox_json and raw_payload_json are excluded from results.
pub async fn list_by_run_check_id(
    executor: impl sqlx::Executor<'_, Database = sqlx::Postgres>,
    run_check_id: Uuid,
    limit: i64,
    cursor: Option<(i32, Uuid)>,
    severity_filter: Option<&str>,
) -> Result<Vec<RunCheckFindingListRow>, sqlx::Error> {
    let mut query = String::from(
        "SELECT id, run_check_id, severity, rule_code, title, message, subject_kind, subject_ref, \
         sheet_path, pcb_layer, x_um, y_um, sort_index, created_at \
         FROM run_check_findings WHERE run_check_id = $1",
    );
    let mut param_idx = 2u32;

    // Build dynamic WHERE clauses
    let cursor_clause = if cursor.is_some() {
        let clause = format!(" AND (sort_index, id) > (${}, ${})", param_idx, param_idx + 1);
        param_idx += 2;
        Some(clause)
    } else {
        None
    };

    let severity_clause = if severity_filter.is_some() {
        let clause = format!(" AND severity = ${}", param_idx);
        // param_idx += 1; // not needed after this
        Some(clause)
    } else {
        None
    };

    if let Some(ref c) = cursor_clause {
        query.push_str(c);
    }
    if let Some(ref s) = severity_clause {
        query.push_str(s);
    }

    query.push_str(" ORDER BY sort_index ASC, id ASC LIMIT $");
    // Compute final limit param index
    let limit_idx = 2
        + if cursor.is_some() { 2 } else { 0 }
        + if severity_filter.is_some() { 1 } else { 0 };
    query.push_str(&limit_idx.to_string());

    // Build and execute query with dynamic bindings
    let mut q = sqlx::query_as::<_, RunCheckFindingListRow>(&query).bind(run_check_id);

    if let Some((si, cid)) = cursor {
        q = q.bind(si).bind(cid);
    }
    if let Some(sev) = severity_filter {
        q = q.bind(sev);
    }
    q = q.bind(limit);

    q.fetch_all(executor).await
}

pub async fn insert(
    executor: impl sqlx::Executor<'_, Database = sqlx::Postgres>,
    id: Uuid,
    run_check_id: Uuid,
    severity: &str,
    rule_code: Option<&str>,
    title: Option<&str>,
    message: Option<&str>,
    subject_kind: Option<&str>,
    subject_ref: Option<&str>,
    sheet_path: Option<&str>,
    pcb_layer: Option<&str>,
    x_um: Option<i32>,
    y_um: Option<i32>,
    bbox_json: Option<&serde_json::Value>,
    raw_payload_json: Option<&serde_json::Value>,
    sort_index: i32,
) -> Result<RunCheckFinding, sqlx::Error> {
    sqlx::query_as::<_, RunCheckFinding>(
        r#"INSERT INTO run_check_findings (id, run_check_id, severity, rule_code, title, message, subject_kind, subject_ref, sheet_path, pcb_layer, x_um, y_um, bbox_json, raw_payload_json, sort_index, created_at)
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, NOW())
        RETURNING *"#,
    )
    .bind(id)
    .bind(run_check_id)
    .bind(severity)
    .bind(rule_code)
    .bind(title)
    .bind(message)
    .bind(subject_kind)
    .bind(subject_ref)
    .bind(sheet_path)
    .bind(pcb_layer)
    .bind(x_um)
    .bind(y_um)
    .bind(bbox_json)
    .bind(raw_payload_json)
    .bind(sort_index)
    .fetch_one(executor)
    .await
}
