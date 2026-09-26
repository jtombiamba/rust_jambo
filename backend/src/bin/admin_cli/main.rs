mod cashout;
mod keys;
mod refund;

use std::process::ExitCode;

use clap::{Parser, Subcommand};
use dialoguer::Input;
use uuid::Uuid;

use jambo_backend::config::Config;
use jambo_backend::database::repositories::{AdminKeyRepository, CashoutRepository};

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
        Command::KeysAdd { label } => keys::keys_add(&admin_repo, &label).await,
        Command::KeysList => keys::keys_list(&admin_repo).await,
        Command::KeysRevoke { label } => keys::keys_revoke(&admin_repo, &label).await,
        Command::PlayerRefund {
            user,
            types,
            from,
            to,
            include_kicked,
            unplayed_runs,
        } => {
            let _admin = authenticate(&admin_repo).await?;
            refund::player_refund(
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
            cashout::cashout_list(&db, &cli.format, status, user).await
        }
        Command::CashoutShow { id } => {
            let _admin = authenticate(&admin_repo).await?;
            cashout::cashout_show(&db, &cli.format, id).await
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
            cashout::cashout_pay(&config, &db, &admin, id).await
        }
    }
}

pub(crate) fn get_keypass() -> Result<String, String> {
    if let Ok(env_key) = std::env::var("ADMIN_CLI_KEYPASS") {
        if !env_key.is_empty() {
            return Ok(env_key);
        }
    }
    // rpassword::prompt_password("Admin keypass: ").map_err(|e| e.to_string())
    let keypass = Input::new()
        .with_prompt("Admin keypass")
        .interact_text()
        .map_err(|e| e.to_string())?;
    Ok(keypass)
}

/// Returns the matched admin label.
pub(crate) async fn authenticate(repo: &AdminKeyRepository) -> Result<String, String> {
    let keypass = get_keypass()?;
    let label = repo
        .verify(&keypass)
        .await
        .map_err(|e| format!("db: {e}"))?
        .ok_or_else(|| "invalid admin keypass".to_string())?;
    println!("authenticated as {label}");
    Ok(label)
}
