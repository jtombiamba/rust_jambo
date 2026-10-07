use std::sync::Arc;

use tokio::sync::mpsc;

use crate::i18n::Lang;
use crate::mailer::Mailer;
use crate::observability::metrics::EMAIL_SEND_ERRORS_TOTAL;

const EMAIL_QUEUE_CAPACITY: usize = 256;

/// A single outgoing email, fully resolved and ready to be dispatched by the
/// background consumer. Keeping the payload resolved (no DB lookups inside the
/// queue) means enqueueing never blocks on I/O beyond the channel send itself.
#[derive(Debug, Clone)]
pub enum EmailJob {
    Invitation {
        to_email: String,
        inviter_name: String,
        game_id: String,
        lang: Lang,
    },
    ContactForm {
        name: String,
        email: String,
        subject: String,
        message: String,
        lang: Lang,
    },
    PasswordReset {
        to_email: String,
        reset_link: String,
        lang: Lang,
    },
    FreezeExpired {
        to_email: String,
        credit: i32,
        lang: Lang,
    },
    CashoutRequested {
        to_email: String,
        credits: i32,
        amount_eur_cents: i32,
        paypal_email: String,
        lang: Lang,
    },
    CashoutAdminAlert {
        to_email: String,
        pseudo: String,
        email: String,
        credits: i32,
        amount_eur_cents: i32,
        paypal_email: String,
    },
    StallKicked {
        to_email: String,
        game_id: String,
        bet: i32,
        lang: Lang,
    },
    StallWarning {
        to_email: String,
        game_id: String,
        inactive_minutes: i64,
        remaining_minutes: i64,
        lang: Lang,
    },
    RoomInvitation {
        to_email: String,
        inviter_name: String,
        room_name: String,
        invitation_code: String,
        lang: Lang,
    },
}

impl EmailJob {
    /// Metric label identifying the email type for `EMAIL_SEND_ERRORS_TOTAL`.
    pub fn label(&self) -> &'static str {
        match self {
            EmailJob::Invitation { .. } => "invitation",
            EmailJob::ContactForm { .. } => "contact_form",
            EmailJob::PasswordReset { .. } => "password_reset",
            EmailJob::FreezeExpired { .. } => "unfreeze",
            EmailJob::CashoutRequested { .. } => "cashout_requested",
            EmailJob::CashoutAdminAlert { .. } => "cashout_admin_alert",
            EmailJob::StallKicked { .. } => "stall_kicked",
            EmailJob::StallWarning { .. } => "stall_warning",
            EmailJob::RoomInvitation { .. } => "room_invitation",
        }
    }
}

/// A bounded, non-blocking outbound email queue backed by a single background
/// consumer. Request handlers enqueue and return immediately; the consumer is
/// the only place that awaits the actual SMTP send.
#[derive(Clone)]
pub struct EmailQueue {
    tx: mpsc::Sender<EmailJob>,
}

impl EmailQueue {
    /// Create a queue without a consumer, returning the receiver so callers
    /// (tests) can inspect what was enqueued.
    pub fn channel() -> (Self, mpsc::Receiver<EmailJob>) {
        let (tx, rx) = mpsc::channel(EMAIL_QUEUE_CAPACITY);
        (Self { tx }, rx)
    }

    /// Create a queue and spawn the background consumer that dispatches jobs to
    /// `mailer`.
    pub fn start(mailer: Arc<dyn Mailer>) -> Self {
        let (queue, mut rx) = Self::channel();
        tokio::spawn(async move {
            while let Some(job) = rx.recv().await {
                if let Err(e) = dispatch(&*mailer, &job).await {
                    tracing::error!(email_type = job.label(), error = %e, "failed to send email");
                    EMAIL_SEND_ERRORS_TOTAL
                        .with_label_values(&[job.label()])
                        .inc();
                }
            }
            tracing::info!("Email queue consumer shutting down");
        });
        queue
    }

    /// Enqueue a job without blocking. When the channel is full or the consumer
    /// has been dropped, the email is discarded and logged.
    pub fn enqueue(&self, job: EmailJob) {
        if let Err(e) = self.tx.try_send(job) {
            let label = match &e {
                mpsc::error::TrySendError::Full(job) => job.label(),
                mpsc::error::TrySendError::Closed(job) => job.label(),
            };
            tracing::warn!(
                email_type = label,
                "email queue full or closed; dropping email"
            );
            EMAIL_SEND_ERRORS_TOTAL.with_label_values(&[label]).inc();
        }
    }
}

async fn dispatch(mailer: &dyn Mailer, job: &EmailJob) -> Result<(), String> {
    match job {
        EmailJob::Invitation {
            to_email,
            inviter_name,
            game_id,
            lang,
        } => {
            mailer
                .send_invitation(to_email, inviter_name, game_id, *lang)
                .await
        }
        EmailJob::ContactForm {
            name,
            email,
            subject,
            message,
            lang,
        } => {
            mailer
                .send_contact_form(name, email, subject, message, *lang)
                .await
        }
        EmailJob::PasswordReset {
            to_email,
            reset_link,
            lang,
        } => {
            mailer
                .send_password_reset(to_email, reset_link, *lang)
                .await
        }
        EmailJob::FreezeExpired {
            to_email,
            credit,
            lang,
        } => mailer.send_freeze_expired(to_email, *credit, *lang).await,
        EmailJob::CashoutRequested {
            to_email,
            credits,
            amount_eur_cents,
            paypal_email,
            lang,
        } => {
            mailer
                .send_cashout_requested(to_email, *credits, *amount_eur_cents, paypal_email, *lang)
                .await
        }
        EmailJob::CashoutAdminAlert {
            to_email,
            pseudo,
            email,
            credits,
            amount_eur_cents,
            paypal_email,
        } => {
            mailer
                .send_cashout_admin_alert(
                    to_email,
                    pseudo,
                    email,
                    *credits,
                    *amount_eur_cents,
                    paypal_email,
                )
                .await
        }
        EmailJob::StallKicked {
            to_email,
            game_id,
            bet,
            lang,
        } => {
            mailer
                .send_stall_kicked(to_email, game_id, *bet, *lang)
                .await
        }
        EmailJob::StallWarning {
            to_email,
            game_id,
            inactive_minutes,
            remaining_minutes,
            lang,
        } => {
            mailer
                .send_stall_warning(
                    to_email,
                    game_id,
                    *inactive_minutes,
                    *remaining_minutes,
                    *lang,
                )
                .await
        }
        EmailJob::RoomInvitation {
            to_email,
            inviter_name,
            room_name,
            invitation_code,
            lang,
        } => {
            mailer
                .send_room_invitation(to_email, inviter_name, room_name, invitation_code, *lang)
                .await
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_helpers::MockMailer;

    #[test]
    fn email_job_labels_are_stable() {
        assert_eq!(
            EmailJob::Invitation {
                to_email: "a@b.co".into(),
                inviter_name: "p".into(),
                game_id: "g".into(),
                lang: Lang::En,
            }
            .label(),
            "invitation"
        );
        assert_eq!(
            EmailJob::PasswordReset {
                to_email: "a@b.co".into(),
                reset_link: "r".into(),
                lang: Lang::En,
            }
            .label(),
            "password_reset"
        );
        assert_eq!(
            EmailJob::StallWarning {
                to_email: "a@b.co".into(),
                game_id: "g".into(),
                inactive_minutes: 10,
                remaining_minutes: 5,
                lang: Lang::En,
            }
            .label(),
            "stall_warning"
        );
    }

    #[tokio::test]
    async fn enqueue_delivers_job_to_receiver() {
        let (queue, mut rx) = EmailQueue::channel();
        queue.enqueue(EmailJob::PasswordReset {
            to_email: "a@b.co".into(),
            reset_link: "r".into(),
            lang: Lang::En,
        });
        let job = rx.try_recv().expect("job should be enqueued");
        assert!(matches!(job, EmailJob::PasswordReset { .. }));
    }

    #[tokio::test]
    async fn consumer_dispatches_job_to_mailer() {
        let mailer: Arc<dyn Mailer> = Arc::new(MockMailer::ok());
        let queue = EmailQueue::start(mailer);
        queue.enqueue(EmailJob::Invitation {
            to_email: "a@b.co".into(),
            inviter_name: "p".into(),
            game_id: "g".into(),
            lang: Lang::En,
        });
        // The consumer runs asynchronously; give it a tick to process the job.
        tokio::task::yield_now().await;
    }
}
