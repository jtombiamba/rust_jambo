use chrono::SecondsFormat;
use serde::Serialize;
use uuid::Uuid;

use jambo_backend::database::repositories::admin_report::{
    RefundLedgerRow, RefundReport, RefundSummary, ReportGameType, TopupReportRow,
};
use jambo_backend::database::repositories::{AdminReportRepository, UserRepository};

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

fn summary_out(s: &RefundSummary) -> RefundSummaryOut {
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
pub(crate) async fn player_refund(
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

    let topup_history = report_repo
        .player_topup_report(user_id, None, None)
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
            print_report_table(&report, &ledger, &topup_history);
        }
    }
    Ok(())
}

pub(crate) fn print_report_table(
    report: &RefundReport,
    ledger: &[RefundLedgerRow],
    topup_history: &[TopupReportRow],
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
    println!("Topup history:");
    for t in topup_history {
        println!(
            "  {}  {}  amount={}  credits={}  order_id={:?}",
            t.created_at.to_rfc3339_opts(SecondsFormat::Secs, true),
            t.kind,
            t.total_amount,
            t.credits,
            t.order_id.as_ref().unwrap_or(&"N/A".to_string())
        );
    }
}

pub(crate) async fn resolve_user(user_repo: &UserRepository, input: &str) -> Result<Uuid, String> {
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

#[cfg(test)]
mod tests {
    use super::*;

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
