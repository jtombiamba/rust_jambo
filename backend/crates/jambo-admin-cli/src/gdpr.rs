use jambo_db::repositories::UserRepository;

use super::refund::resolve_user;

/// List users who requested deletion and are awaiting the DPO soft-delete.
pub(crate) async fn deletion_list(
    db: &sea_orm::DatabaseConnection,
    format: &str,
) -> Result<(), String> {
    let repo = UserRepository::new(db.clone(), 0);
    let requests = repo
        .list_deletion_requests()
        .await
        .map_err(|e| format!("db: {e}"))?;

    if format == "json" {
        let out: Vec<serde_json::Value> = requests
            .iter()
            .map(|u| {
                serde_json::json!({
                    "id": u.id.to_string(),
                    "pseudo": u.pseudo,
                    "email": u.email,
                    "deletion_requested_at": u.deletion_requested_at.map(|t| t.to_rfc3339()),
                })
            })
            .collect();
        println!(
            "{}",
            serde_json::to_string_pretty(&out).map_err(|e| e.to_string())?
        );
    } else {
        for u in &requests {
            println!(
                "{}\t{}\t{}\t{}",
                u.id,
                u.pseudo,
                u.email,
                u.deletion_requested_at
                    .map(|t| t.to_rfc3339())
                    .unwrap_or_default()
            );
        }
    }
    Ok(())
}

/// Soft-delete a user and relabel their player rows. Accepts a UUID or pseudo.
pub(crate) async fn deletion_execute(
    db: &sea_orm::DatabaseConnection,
    format: &str,
    user: &str,
) -> Result<(), String> {
    let user_repo = UserRepository::new(db.clone(), 0);
    let user_id = resolve_user(&user_repo, user).await?;
    let outcome = user_repo
        .soft_delete(user_id)
        .await
        .map_err(|e| format!("db: {e}"))?;

    if format == "json" {
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "id": outcome.user.id.to_string(),
                "pseudo": outcome.user.pseudo,
                "deleted_at": outcome.user.deleted_at.map(|t| t.to_rfc3339()),
                "players_renamed": outcome.players_renamed,
            }))
            .map_err(|e| e.to_string())?
        );
    } else {
        println!("soft-deleted user {}", outcome.user.id);
        println!("  pseudo:          {}", outcome.user.pseudo);
        println!(
            "  deleted_at:      {}",
            outcome
                .user
                .deleted_at
                .map(|t| t.to_rfc3339())
                .unwrap_or_default()
        );
        println!("  players renamed: {}", outcome.players_renamed);
    }
    Ok(())
}
