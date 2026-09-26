use uuid::Uuid;

use jambo_backend::config::Config;
use jambo_backend::database::models::CashoutStatus;
use jambo_backend::database::repositories::{
    AdminReportRepository, CashoutRepository, UserRepository,
};

use super::refund::{print_report_table, resolve_user};

pub(crate) async fn cashout_list(
    db: &sea_orm::DatabaseConnection,
    format: &str,
    status: Option<String>,
    user: Option<String>,
) -> Result<(), String> {
    println!("listing cashout requests");
    let repo = CashoutRepository::new(db.clone());
    let user_id = match user {
        Some(u) => {
            let user_repo = UserRepository::new(db.clone(), 0);
            Some(resolve_user(&user_repo, &u).await?)
        }
        None => None,
    };
    let status = match status {
        Some(s) => Some(parse_status(&s)?),
        None => None,
    };
    let (requests, _total) = repo
        .list_paginated(status, user_id, 1, 1000)
        .await
        .map_err(|e| format!("db: {e}"))?;

    let out: Vec<serde_json::Value> = requests
        .iter()
        .map(|r| {
            serde_json::json!({
                "id": r.id.to_string(),
                "user_id": r.user_id.to_string(),
                "credits": r.credits,
                "amount_eur_cents": r.amount_eur_cents,
                "status": r.status.to_string(),
                "paypal_email": r.paypal_email,
                "created_at": r.created_at.to_rfc3339(),
                "processed_at": r.processed_at.map(|t| t.to_rfc3339()),
            })
        })
        .collect();

    if format == "json" {
        println!(
            "{}",
            serde_json::to_string_pretty(&out).map_err(|e| e.to_string())?
        );
    } else {
        for r in &requests {
            println!(
                "{}\t{}\t{}\t{} credits\t{}\t{}",
                r.id, r.user_id, r.status, r.credits, r.paypal_email, r.created_at
            );
        }
    }
    Ok(())
}

pub(crate) async fn cashout_show(
    db: &sea_orm::DatabaseConnection,
    format: &str,
    id: Uuid,
) -> Result<(), String> {
    let repo = CashoutRepository::new(db.clone());
    let request = repo
        .find_by_id(id)
        .await
        .map_err(|e| format!("db: {e}"))?
        .ok_or_else(|| "cashout request not found".to_string())?;

    if format == "json" {
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "id": request.id.to_string(),
                "user_id": request.user_id.to_string(),
                "credits": request.credits,
                "amount_eur_cents": request.amount_eur_cents,
                "status": request.status.to_string(),
                "paypal_email": request.paypal_email,
                "created_at": request.created_at.to_rfc3339(),
            }))
            .map_err(|e| e.to_string())?
        );
    } else {
        println!("Cashout request {}", request.id);
        println!("  user:      {}", request.user_id);
        println!("  credits:   {}", request.credits);
        println!(
            "  amount:    {:.2} EUR",
            request.amount_eur_cents as f64 / 100.0
        );
        println!("  status:    {}", request.status);
        println!("  paypal:    {}", request.paypal_email);
        println!("  created:   {}", request.created_at);
    }

    let report_repo = AdminReportRepository::new(db.clone());
    let report = report_repo
        .player_refund_report(request.user_id, None, None, None, false)
        .await
        .map_err(|e| format!("db: {e}"))?;
    let topup_history = report_repo
        .player_topup_report(request.user_id, None, None)
        .await
        .map_err(|e| format!("db: {e}"))?;
    println!();
    println!("Fraud cross-check (settled games, bots excluded):");
    print_report_table(&report, &[], &topup_history);
    Ok(())
}

pub(crate) async fn cashout_pay(
    config: &Config,
    db: &sea_orm::DatabaseConnection,
    admin: &str,
    id: Uuid,
) -> Result<(), String> {
    let repo = CashoutRepository::new(db.clone());
    let request = repo
        .find_by_id(id)
        .await
        .map_err(|e| format!("db: {e}"))?
        .ok_or_else(|| "cashout request not found".to_string())?;
    if request.status != CashoutStatus::Approved {
        return Err("request must be approved before it can be paid".to_string());
    }

    let payment = jambo_backend::payment::PaymentService::new(
        config.paypal_client_id.clone(),
        config.paypal_client_secret.clone(),
        config.paypal_mode.clone(),
        config.paypal_unfreeze_amount_eur.clone(),
        config.paypal_topup_amount_eur.clone(),
        config.paypal_sandbox_url.clone(),
        config.paypal_live_url.clone(),
    );
    let batch_id = payment
        .create_payout(
            &request.paypal_email,
            request.amount_eur_cents,
            &request.id.to_string(),
        )
        .await
        .map_err(|e| format!("paypal payout failed: {e}"))?;

    repo.mark_paid(id, &batch_id, admin)
        .await
        .map_err(|e| format!("mark paid failed: {e}"))?;
    println!("paid {id} (payout batch {batch_id})");
    Ok(())
}

fn parse_status(s: &str) -> Result<CashoutStatus, String> {
    match s {
        "requested" => Ok(CashoutStatus::Requested),
        "approved" => Ok(CashoutStatus::Approved),
        "rejected" => Ok(CashoutStatus::Rejected),
        "paid" => Ok(CashoutStatus::Paid),
        _ => Err(format!("invalid status '{s}'")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_status_known() {
        assert_eq!(parse_status("requested").unwrap(), CashoutStatus::Requested);
        assert_eq!(parse_status("approved").unwrap(), CashoutStatus::Approved);
        assert_eq!(parse_status("rejected").unwrap(), CashoutStatus::Rejected);
        assert_eq!(parse_status("paid").unwrap(), CashoutStatus::Paid);
        assert!(parse_status("nope").is_err());
    }
}
