use sea_orm::{
    ColumnTrait, DatabaseConnection, DbErr, EntityTrait, JoinType, QueryFilter, QueryOrder,
    QuerySelect, RelationTrait,
};
use uuid::Uuid;

use crate::database::models::{game, game_run, game_run_player, player, GameMode, GameStatus};

/// Game classification used by the refund report.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
pub enum ReportGameType {
    Solo,
    Multiplayer,
    Run,
}

#[allow(dead_code)]
impl ReportGameType {
    pub fn as_str(self) -> &'static str {
        match self {
            ReportGameType::Solo => "solo",
            ReportGameType::Multiplayer => "multiplayer",
            ReportGameType::Run => "run",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "solo" => Some(ReportGameType::Solo),
            "multiplayer" => Some(ReportGameType::Multiplayer),
            "run" => Some(ReportGameType::Run),
            _ => None,
        }
    }
}

/// Classify a settled game into a report type.
#[allow(dead_code)]
pub fn classify_game_type(game_run_id: Option<Uuid>, game_mode: GameMode) -> ReportGameType {
    if game_run_id.is_some() {
        ReportGameType::Run
    } else if game_mode == GameMode::Multiplayer {
        ReportGameType::Multiplayer
    } else {
        ReportGameType::Solo
    }
}

const SETTLED_STATUSES: [GameStatus; 3] = [
    GameStatus::Finished,
    GameStatus::Kora,
    GameStatus::DoubleKora,
];

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct RefundLedgerRow {
    pub game_id: Uuid,
    pub game_type: ReportGameType,
    pub status: GameStatus,
    pub bet: i32,
    pub won: bool,
    pub kicked: bool,
    pub finished_at: Option<chrono::DateTime<chrono::Utc>>,
    pub credits_at_end: i32,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
#[allow(dead_code)]
pub struct RefundSummary {
    pub games_played: i64,
    pub wins: i64,
    pub losses: i64,
    pub credits_won: i64,
    pub credits_lost: i64,
    pub net_credits: i64,
}

impl RefundSummary {
    fn add(&mut self, row: &RefundLedgerRow) {
        self.games_played += 1;
        if row.won {
            self.wins += 1;
            self.credits_won += row.bet as i64;
        } else {
            self.losses += 1;
            self.credits_lost += row.bet as i64;
        }
        self.net_credits = self.credits_won - self.credits_lost;
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
#[allow(dead_code)]
pub struct RefundReport {
    pub solo: RefundSummary,
    pub multiplayer: RefundSummary,
    pub run: RefundSummary,
    pub total: RefundSummary,
    pub unplayed_run_credits: i64,
}

#[allow(dead_code)]
pub struct AdminReportRepository {
    connection: DatabaseConnection,
}

#[allow(dead_code)]
impl AdminReportRepository {
    pub fn new(connection: DatabaseConnection) -> Self {
        Self { connection }
    }

    #[tracing::instrument(skip(self), fields(db.statement, db.rows_affected))]
    pub async fn player_refund_ledger(
        &self,
        user_id: Uuid,
        types: Option<&[ReportGameType]>,
        from: Option<chrono::DateTime<chrono::Utc>>,
        to: Option<chrono::DateTime<chrono::Utc>>,
        include_kicked: bool,
    ) -> Result<Vec<RefundLedgerRow>, DbErr> {
        let mut query = player::Entity::find()
            .filter(player::Column::UserId.eq(user_id))
            .filter(player::Column::PlayerType.eq(crate::database::models::PlayerType::Human))
            .join(JoinType::InnerJoin, player::Relation::Game.def())
            .filter(game::Column::Status.is_in(SETTLED_STATUSES));

        if !include_kicked {
            query = query.filter(player::Column::Kicked.eq(false));
        }

        let players = query
            .order_by_desc(player::Column::CreatedAt)
            .all(&self.connection)
            .await?;

        let game_ids: Vec<Uuid> = players.iter().map(|p| p.game_id).collect();
        let games_map: std::collections::HashMap<Uuid, crate::database::models::Game> =
            if game_ids.is_empty() {
                std::collections::HashMap::new()
            } else {
                game::Entity::find()
                    .filter(game::Column::Id.is_in(game_ids))
                    .all(&self.connection)
                    .await?
                    .into_iter()
                    .map(|g| (g.id, g))
                    .collect()
            };

        let mut rows = Vec::new();
        for p in players {
            let Some(g) = games_map.get(&p.game_id) else {
                continue;
            };
            let game_type = classify_game_type(g.game_run_id, g.game_mode);
            if let Some(types) = types {
                if !types.contains(&game_type) {
                    continue;
                }
            }
            if let Some(from) = from {
                if g.finished_at.is_none_or(|t| t < from) {
                    continue;
                }
            }
            if let Some(to) = to {
                if g.finished_at.is_none_or(|t| t > to) {
                    continue;
                }
            }
            rows.push(RefundLedgerRow {
                game_id: g.id,
                game_type,
                status: g.status,
                bet: g.bet,
                won: g.winner_id == Some(p.id),
                kicked: p.kicked,
                finished_at: g.finished_at,
                credits_at_end: p.credits,
            });
        }

        Ok(rows)
    }

    #[tracing::instrument(skip(self), fields(db.statement, db.rows_affected))]
    pub async fn player_refund_report(
        &self,
        user_id: Uuid,
        types: Option<&[ReportGameType]>,
        from: Option<chrono::DateTime<chrono::Utc>>,
        to: Option<chrono::DateTime<chrono::Utc>>,
        include_kicked: bool,
    ) -> Result<RefundReport, DbErr> {
        let rows = self
            .player_refund_ledger(user_id, types, from, to, include_kicked)
            .await?;
        let unplayed_run_credits = self.unplayed_run_credits(user_id).await?;

        let mut report = RefundReport {
            unplayed_run_credits,
            ..Default::default()
        };
        for row in &rows {
            report.total.add(row);
            match row.game_type {
                ReportGameType::Solo => report.solo.add(row),
                ReportGameType::Multiplayer => report.multiplayer.add(row),
                ReportGameType::Run => report.run.add(row),
            }
        }
        Ok(report)
    }

    /// Escrowed credits still provisioned for the user's active runs.
    #[tracing::instrument(skip(self), fields(db.statement, db.rows_affected))]
    pub async fn unplayed_run_credits(&self, user_id: Uuid) -> Result<i64, DbErr> {
        let rows = game_run_player::Entity::find()
            .filter(game_run_player::Column::UserId.eq(user_id))
            .join(
                JoinType::InnerJoin,
                game_run_player::Relation::GameRun.def(),
            )
            .filter(game_run::Column::Status.eq(crate::database::models::RunStatus::Active))
            .all(&self.connection)
            .await?;

        Ok(rows
            .iter()
            .map(|run_player| run_player.provisioned_credits as i64)
            .sum())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::database::models::GameMode;

    #[test]
    fn classify_run_by_game_run_id() {
        assert_eq!(
            classify_game_type(Some(Uuid::now_v7()), GameMode::Multiplayer),
            ReportGameType::Run
        );
        assert_eq!(
            classify_game_type(Some(Uuid::now_v7()), GameMode::Solo),
            ReportGameType::Run
        );
    }

    #[test]
    fn classify_multiplayer_and_solo() {
        assert_eq!(
            classify_game_type(None, GameMode::Multiplayer),
            ReportGameType::Multiplayer
        );
        assert_eq!(
            classify_game_type(None, GameMode::Solo),
            ReportGameType::Solo
        );
    }

    #[test]
    fn parse_report_type() {
        assert_eq!(ReportGameType::parse("solo"), Some(ReportGameType::Solo));
        assert_eq!(
            ReportGameType::parse("multiplayer"),
            Some(ReportGameType::Multiplayer)
        );
        assert_eq!(ReportGameType::parse("run"), Some(ReportGameType::Run));
        assert_eq!(ReportGameType::parse("nope"), None);
    }

    #[test]
    fn summary_net_credits_tracks_wins_and_losses() {
        let now = chrono::Utc::now();
        let mut summary = RefundSummary::default();
        summary.add(&RefundLedgerRow {
            game_id: Uuid::now_v7(),
            game_type: ReportGameType::Solo,
            status: GameStatus::Finished,
            bet: 10,
            won: true,
            kicked: false,
            finished_at: Some(now),
            credits_at_end: 100,
        });
        summary.add(&RefundLedgerRow {
            game_id: Uuid::now_v7(),
            game_type: ReportGameType::Solo,
            status: GameStatus::Finished,
            bet: 10,
            won: false,
            kicked: false,
            finished_at: Some(now),
            credits_at_end: 90,
        });
        assert_eq!(summary.games_played, 2);
        assert_eq!(summary.wins, 1);
        assert_eq!(summary.losses, 1);
        assert_eq!(summary.credits_won, 10);
        assert_eq!(summary.credits_lost, 10);
        assert_eq!(summary.net_credits, 0);
    }
}
