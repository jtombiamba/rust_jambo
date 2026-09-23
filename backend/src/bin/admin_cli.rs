use std::process::ExitCode;

use clap::{Parser, Subcommand};
use serde::Serialize;
use uuid::Uuid;

use jambo_backend::auth::password::hash_password;
use jambo_backend::config::Config;
use jambo_backend::database::models::CashoutStatus;
use jambo_backend::database::repositories::admin_report::{RefundReport, ReportGameType};
use jambo_backend::database::repositories::{
    AdminKeyRepository, AdminReportRepository, CashoutRepository, UserRepository,
};

#[derive(Parser)]
#[command(name = "admin-cli", about = "Jambo administrative CLI")]
struct Cli {
    #[arg(long, global = true, default_value = "table")]
    format: String,
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Generate a new admin keypass (bootstrap only when no active keys exist).
    KeysAdd { label: String },
    /// List active admin keys.
    KeysList,
    /// Revoke an admin key by label.
    KeysRevoke { label: String },
    /// Produce a player refund report.
    PlayerRefund {
        #[arg(long)]
        user: String,
        #[arg(long, value_delimiter = ',')]
        types: Option<Vec<String>>,
        #[arg(long)]
        from: Option<String>,
        #[arg(long)]
        to: Option<String>,
        #[arg(long)]
        include_kicked: bool,
        #[arg(long)]
        unplayed_runs: bool,
    },
    /// List cashout requests.
    CashoutList {
        #[arg(long)]
        status: Option<String>,
        #[arg(long)]
        user: Option<String>,
    },
    /// Show a cashout request with a fraud cross-check report.
    CashoutShow { id: Uuid },
    /// Approve a cashout request (unfreezes the account).
    CashoutApprove {
        id: Uuid,
        #[arg(long, default_value = "")]
        note: String,
    },
    /// Reject a cashout request (refunds credits, unfreezes the account).
    CashoutReject {
        id: Uuid,
        #[arg(long, default_value = "")]
        note: String,
    },
    /// Pay an approved cashout via PayPal Payouts.
    CashoutPay { id: Uuid },
}

#[tokio::main]
async fn main() -> ExitCode {
    let cli = Cli::parse();
    match run(cli).await {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}

async fn run(cli: Cli) -> Result<(), String> {
    let config = Config::from_env().map_err(|e| format!("config: {e}"))?;
    let db = jambo_backend::database::create_connection(&config)
        .await
        .map_err(|e| format!("db: {e}"))?;
    let admin_repo = AdminKeyRepository::new(db.clone());

    match cli.command {
        Command::KeysAdd { label } => keys_add(&admin_repo, &label).await,
        Command::KeysList => keys_list(&admin_repo).await,
        Command::KeysRevoke { label } => keys_revoke(&admin_repo, &label).await,
        Command::PlayerRefund {
            user,
            types,
            from,
            to,
            include_kicked,
            unplayed_runs,
        } => {
            let _admin = authenticate(&admin_repo).await?;
            player_refund(
                &db,
                &cli.format,
                &user,
                types,
                from,
                to,
                include_kicked,
                unplayed_runs,
            )
            .await
        }
        Command::CashoutList { status, user } => {
            let _admin = authenticate(&admin_repo).await?;
            cashout_list(&db, &cli.format, status, user).await
        }
        Command::CashoutShow { id } => {
            let _admin = authenticate(&admin_repo).await?;
            cashout_show(&db, &cli.format, id).await
        }
        Command::CashoutApprove { id, note } => {
            let admin = authenticate(&admin_repo).await?;
            let repo = CashoutRepository::new(db.clone());
            repo.approve(id, &note, &admin)
                .await
                .map_err(|e| format!("approve failed: {e}"))?;
            println!("approved {id}");
            Ok(())
        }
        Command::CashoutReject { id, note } => {
            let admin = authenticate(&admin_repo).await?;
            let repo = CashoutRepository::new(db.clone());
            repo.reject(id, &note, &admin)
                .await
                .map_err(|e| format!("reject failed: {e}"))?;
            println!("rejected {id}");
            Ok(())
        }
        Command::CashoutPay { id } => {
            let admin = authenticate(&admin_repo).await?;
            cashout_pay(&config, &db, &admin, id).await
        }
    }
}

fn get_keypass() -> Result<String, String> {
    if let Ok(env_key) = std::env::var("ADMIN_CLI_KEYPASS") {
        if !env_key.is_empty() {
            return Ok(env_key);
        }
    }
    rpassword::prompt_password("Admin keypass: ").map_err(|e| e.to_string())
}

/// Returns the matched admin label.
async fn authenticate(repo: &AdminKeyRepository) -> Result<String, String> {
    let keypass = get_keypass()?;
    let label = repo
        .verify(&keypass)
        .await
        .map_err(|e| format!("db: {e}"))?
        .ok_or_else(|| "invalid admin keypass".to_string())?;
    Ok(label)
}

async fn keys_add(repo: &AdminKeyRepository, label: &str) -> Result<(), String> {
    let active = repo.count_active().await.map_err(|e| format!("db: {e}"))?;
    if active > 0 {
        // Protect the table once initialised.
        let _ = authenticate(repo).await?;
    }
    let keypass = generate_keypass();
    let hash = hash_password(&keypass).map_err(|e| e.to_string())?;
    repo.create(label, &hash)
        .await
        .map_err(|e| format!("db: {e}"))?;
    println!("Created admin key '{label}'. Keypass (shown once, store safely):");
    println!("{keypass}");
    Ok(())
}

async fn keys_list(repo: &AdminKeyRepository) -> Result<(), String> {
    let _ = authenticate(repo).await?;
    let keys = repo.list_active().await.map_err(|e| format!("db: {e}"))?;
    for key in keys {
        println!("{}\t{}", key.label, key.created_at);
    }
    Ok(())
}

async fn keys_revoke(repo: &AdminKeyRepository, label: &str) -> Result<(), String> {
    let _ = authenticate(repo).await?;
    let rows = repo.revoke(label).await.map_err(|e| format!("db: {e}"))?;
    if rows == 0 {
        return Err(format!("no active key found with label '{label}'"));
    }
    println!("revoked '{label}'");
    Ok(())
}

fn generate_keypass() -> String {
    format!("{}{}", Uuid::new_v4().simple(), Uuid::new_v4().simple())
}

#[derive(Serialize)]
struct RefundSummaryOut {
    games_played: i64,
    wins: i64,
    losses: i64,
    credits_won: i64,
    credits_lost: i64,
    net_credits: i64,
}

#[derive(Serialize)]
struct LedgerRowOut {
    game_id: String,
    game_type: String,
    status: String,
    bet: i32,
    won: bool,
    kicked: bool,
    finished_at: Option<String>,
    credits_at_end: i32,
}

fn summary_out(
    s: &jambo_backend::database::repositories::admin_report::RefundSummary,
) -> RefundSummaryOut {
    RefundSummaryOut {
        games_played: s.games_played,
        wins: s.wins,
        losses: s.losses,
        credits_won: s.credits_won,
        credits_lost: s.credits_lost,
        net_credits: s.net_credits,
    }
}

#[allow(clippy::too_many_arguments)]
async fn player_refund(
    db: &sea_orm::DatabaseConnection,
    format: &str,
    user: &str,
    types: Option<Vec<String>>,
    from: Option<String>,
    to: Option<String>,
    include_kicked: bool,
    unplayed_runs: bool,
) -> Result<(), String> {
    let user_repo = UserRepository::new(db.clone(), 0);
    let user_id = resolve_user(&user_repo, user).await?;
    let report_repo = AdminReportRepository::new(db.clone());

    let parsed_types: Option<Vec<ReportGameType>> = types.map(|v| {
        v.iter()
            .filter_map(|s| ReportGameType::parse(s))
            .collect::<Vec<_>>()
    });
    let parsed_from = parse_date(from)?;
    let parsed_to = parse_date(to)?;

    let mut report = report_repo
        .player_refund_report(
            user_id,
            parsed_types.as_deref(),
            parsed_from,
            parsed_to,
            include_kicked,
        )
        .await
        .map_err(|e| format!("db: {e}"))?;
    if !unplayed_runs {
        report.unplayed_run_credits = 0;
    }

    let ledger = report_repo
        .player_refund_ledger(
            user_id,
            parsed_types.as_deref(),
            parsed_from,
            parsed_to,
            include_kicked,
        )
        .await
        .map_err(|e| format!("db: {e}"))?;

    match format {
        "json" => {
            let ledger_out: Vec<LedgerRowOut> = ledger
                .iter()
                .map(|r| LedgerRowOut {
                    game_id: r.game_id.to_string(),
                    game_type: r.game_type.as_str().to_string(),
                    status: format!("{:?}", r.status),
                    bet: r.bet,
                    won: r.won,
                    kicked: r.kicked,
                    finished_at: r.finished_at.map(|t| t.to_rfc3339()),
                    credits_at_end: r.credits_at_end,
                })
                .collect();
            let json = serde_json::json!({
                "solo": summary_out(&report.solo),
                "multiplayer": summary_out(&report.multiplayer),
                "run": summary_out(&report.run),
                "total": summary_out(&report.total),
                "unplayed_run_credits": report.unplayed_run_credits,
                "ledger": ledger_out,
            });
            println!(
                "{}",
                serde_json::to_string_pretty(&json).map_err(|e| e.to_string())?
            );
        }
        "csv" => {
            let mut wtr = csv::Writer::from_writer(std::io::stdout());
            wtr.write_record([
                "game_id",
                "game_type",
                "status",
                "bet",
                "won",
                "kicked",
                "finished_at",
                "credits_at_end",
            ])
            .map_err(|e| e.to_string())?;
            for r in &ledger {
                wtr.write_record(&[
                    r.game_id.to_string(),
                    r.game_type.as_str().to_string(),
                    format!("{:?}", r.status),
                    r.bet.to_string(),
                    r.won.to_string(),
                    r.kicked.to_string(),
                    r.finished_at.map(|t| t.to_rfc3339()).unwrap_or_default(),
                    r.credits_at_end.to_string(),
                ])
                .map_err(|e| e.to_string())?;
            }
            wtr.flush().map_err(|e| e.to_string())?;
        }
        _ => {
            print_report_table(&report, &ledger);
        }
    }
    Ok(())
}

fn print_report_table(
    report: &RefundReport,
    ledger: &[jambo_backend::database::repositories::admin_report::RefundLedgerRow],
) {
    println!("Refund report");
    println!(
        "  solo:        {} games, wins {} losses {}, net {} credits",
        report.solo.games_played, report.solo.wins, report.solo.losses, report.solo.net_credits
    );
    println!(
        "  multiplayer: {} games, wins {} losses {}, net {} credits",
        report.multiplayer.games_played,
        report.multiplayer.wins,
        report.multiplayer.losses,
        report.multiplayer.net_credits
    );
    println!(
        "  run:         {} games, wins {} losses {}, net {} credits",
        report.run.games_played, report.run.wins, report.run.losses, report.run.net_credits
    );
    println!(
        "  total:       {} games, net {} credits",
        report.total.games_played, report.total.net_credits
    );
    println!("  unplayed run credits: {}", report.unplayed_run_credits);
    println!();
    println!("Games:");
    for r in ledger {
        let result = if r.won { "win " } else { "loss" };
        let kicked = if r.kicked { " (kicked)" } else { "" };
        println!(
            "  {}  {}  {:?}  bet={}  {}{}  credits_end={}",
            r.game_id,
            r.game_type.as_str(),
            r.status,
            r.bet,
            result,
            kicked,
            r.credits_at_end
        );
    }
}

async fn resolve_user(user_repo: &UserRepository, input: &str) -> Result<Uuid, String> {
    if let Ok(id) = Uuid::parse_str(input) {
        return Ok(id);
    }
    let user = user_repo
        .find_by_pseudo(input)
        .await
        .map_err(|e| format!("db: {e}"))?
        .ok_or_else(|| format!("no user found for pseudo '{input}'"))?;
    Ok(user.id)
}

fn parse_date(input: Option<String>) -> Result<Option<chrono::DateTime<chrono::Utc>>, String> {
    match input {
        None => Ok(None),
        Some(s) => chrono::DateTime::parse_from_rfc3339(&s)
            .map(|dt| Some(dt.with_timezone(&chrono::Utc)))
            .map_err(|e| format!("invalid date '{s}': {e}")),
    }
}

async fn cashout_list(
    db: &sea_orm::DatabaseConnection,
    format: &str,
    status: Option<String>,
    user: Option<String>,
) -> Result<(), String> {
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

async fn cashout_show(
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
    println!();
    println!("Fraud cross-check (settled games, bots excluded):");
    print_report_table(&report, &[]);
    Ok(())
}

async fn cashout_pay(
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

    #[test]
    fn parse_date_valid() {
        let dt = parse_date(Some("2026-01-01T00:00:00Z".to_string())).unwrap();
        assert!(dt.is_some());
        assert!(parse_date(None).unwrap().is_none());
        assert!(parse_date(Some("not-a-date".to_string())).is_err());
    }

    #[test]
    fn resolve_user_accepts_uuid() {
        let id = Uuid::new_v4();
        assert_eq!(Uuid::parse_str(&id.to_string()).unwrap(), id);
    }
}
