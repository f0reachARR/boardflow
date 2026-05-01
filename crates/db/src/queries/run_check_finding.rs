use boardflow_domain::models::run_check::RunCheckFinding;
use uuid::Uuid;

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
