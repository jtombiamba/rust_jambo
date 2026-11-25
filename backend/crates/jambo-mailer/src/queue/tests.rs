use super::*;

use std::sync::atomic::{AtomicUsize, Ordering};

struct MockMailer;

#[async_trait::async_trait]
impl Mailer for MockMailer {
    async fn send_password_reset(
        &self,
        _to_email: &str,
        _reset_link: &str,
        _lang: Lang,
    ) -> Result<(), String> {
        Ok(())
    }
    async fn send_invitation(
        &self,
        _to_email: &str,
        _inviter_name: &str,
        _game_id: &str,
        _lang: Lang,
    ) -> Result<(), String> {
        Ok(())
    }
    async fn send_freeze_expired(
        &self,
        _to_email: &str,
        _credit: i32,
        _lang: Lang,
    ) -> Result<(), String> {
        Ok(())
    }
    async fn send_contact_form(
        &self,
        _name: &str,
        _email: &str,
        _subject: &str,
        _message: &str,
        _lang: Lang,
    ) -> Result<(), String> {
        Ok(())
    }
    async fn send_stall_warning(
        &self,
        _to_email: &str,
        _game_id: &str,
        _inactive_minutes: i64,
        _remaining_minutes: i64,
        _lang: Lang,
    ) -> Result<(), String> {
        Ok(())
    }
    async fn send_stall_kicked(
        &self,
        _to_email: &str,
        _game_id: &str,
        _bet: i32,
        _lang: Lang,
    ) -> Result<(), String> {
        Ok(())
    }
    async fn send_room_invitation(
        &self,
        _to_email: &str,
        _inviter_name: &str,
        _room_name: &str,
        _invitation_code: &str,
        _lang: Lang,
    ) -> Result<(), String> {
        Ok(())
    }
    async fn send_cashout_requested(
        &self,
        _to_email: &str,
        _credits: i32,
        _amount_eur_cents: i32,
        _payout_destination: &str,
        _lang: Lang,
    ) -> Result<(), String> {
        Ok(())
    }
    async fn send_cashout_rejected(
        &self,
        _to_email: &str,
        _credits: i32,
        _amount_eur_cents: i32,
        _lang: Lang,
    ) -> Result<(), String> {
        Ok(())
    }
    async fn send_cashout_admin_alert(
        &self,
        _to_email: &str,
        _pseudo: &str,
        _email: &str,
        _credits: i32,
        _amount_eur_cents: i32,
        _payout_destination: &str,
    ) -> Result<(), String> {
        Ok(())
    }

    async fn send_cashout_paid(
        &self,
        _to_email: &str,
        _amount_eur_cents: i32,
        _lang: Lang,
    ) -> Result<(), String> {
        Ok(())
    }

    async fn send_cashout_failed(
        &self,
        _to_email: &str,
        _amount_eur_cents: i32,
        _lang: Lang,
    ) -> Result<(), String> {
        Ok(())
    }

    async fn send_connect_account_verified(
        &self,
        _to_email: &str,
        _lang: Lang,
    ) -> Result<(), String> {
        Ok(())
    }

    async fn send_connect_account_verification_failed(
        &self,
        _to_email: &str,
        _lang: Lang,
    ) -> Result<(), String> {
        Ok(())
    }

    async fn send_connect_account_onboarding_interrupted(
        &self,
        _to_email: &str,
        _lang: Lang,
    ) -> Result<(), String> {
        Ok(())
    }

    async fn send_deletion_confirmation(
        &self,
        _to_email: &str,
        _confirm_link: &str,
        _lang: Lang,
    ) -> Result<(), String> {
        Ok(())
    }
}

struct FailNMailer {
    inner: MockMailer,
    failures_remaining: AtomicUsize,
}

impl FailNMailer {
    fn new(failures: usize) -> Self {
        Self {
            inner: MockMailer,
            failures_remaining: AtomicUsize::new(failures),
        }
    }
}

#[async_trait::async_trait]
impl Mailer for FailNMailer {
    async fn send_password_reset(
        &self,
        to_email: &str,
        reset_link: &str,
        lang: Lang,
    ) -> Result<(), String> {
        if self.failures_remaining.load(Ordering::SeqCst) > 0 {
            self.failures_remaining.fetch_sub(1, Ordering::SeqCst);
            return Err("smtp temporarily unavailable".to_string());
        }
        self.inner
            .send_password_reset(to_email, reset_link, lang)
            .await
    }
    async fn send_invitation(
        &self,
        to_email: &str,
        inviter_name: &str,
        game_id: &str,
        lang: Lang,
    ) -> Result<(), String> {
        self.inner
            .send_invitation(to_email, inviter_name, game_id, lang)
            .await
    }
    async fn send_freeze_expired(
        &self,
        to_email: &str,
        credit: i32,
        lang: Lang,
    ) -> Result<(), String> {
        self.inner.send_freeze_expired(to_email, credit, lang).await
    }
    async fn send_contact_form(
        &self,
        name: &str,
        email: &str,
        subject: &str,
        message: &str,
        lang: Lang,
    ) -> Result<(), String> {
        self.inner
            .send_contact_form(name, email, subject, message, lang)
            .await
    }
    async fn send_stall_warning(
        &self,
        to_email: &str,
        game_id: &str,
        inactive_minutes: i64,
        remaining_minutes: i64,
        lang: Lang,
    ) -> Result<(), String> {
        self.inner
            .send_stall_warning(to_email, game_id, inactive_minutes, remaining_minutes, lang)
            .await
    }
    async fn send_stall_kicked(
        &self,
        to_email: &str,
        game_id: &str,
        bet: i32,
        lang: Lang,
    ) -> Result<(), String> {
        self.inner
            .send_stall_kicked(to_email, game_id, bet, lang)
            .await
    }
    async fn send_room_invitation(
        &self,
        to_email: &str,
        inviter_name: &str,
        room_name: &str,
        invitation_code: &str,
        lang: Lang,
    ) -> Result<(), String> {
        self.inner
            .send_room_invitation(to_email, inviter_name, room_name, invitation_code, lang)
            .await
    }
    async fn send_cashout_requested(
        &self,
        to_email: &str,
        credits: i32,
        amount_eur_cents: i32,
        payout_destination: &str,
        lang: Lang,
    ) -> Result<(), String> {
        self.inner
            .send_cashout_requested(
                to_email,
                credits,
                amount_eur_cents,
                payout_destination,
                lang,
            )
            .await
    }
    async fn send_cashout_rejected(
        &self,
        to_email: &str,
        credits: i32,
        amount_eur_cents: i32,
        lang: Lang,
    ) -> Result<(), String> {
        self.inner
            .send_cashout_rejected(to_email, credits, amount_eur_cents, lang)
            .await
    }
    async fn send_cashout_admin_alert(
        &self,
        to_email: &str,
        pseudo: &str,
        email: &str,
        credits: i32,
        amount_eur_cents: i32,
        payout_destination: &str,
    ) -> Result<(), String> {
        self.inner
            .send_cashout_admin_alert(
                to_email,
                pseudo,
                email,
                credits,
                amount_eur_cents,
                payout_destination,
            )
            .await
    }
    async fn send_cashout_paid(
        &self,
        to_email: &str,
        amount_eur_cents: i32,
        lang: Lang,
    ) -> Result<(), String> {
        self.inner
            .send_cashout_paid(to_email, amount_eur_cents, lang)
            .await
    }
    async fn send_cashout_failed(
        &self,
        to_email: &str,
        amount_eur_cents: i32,
        lang: Lang,
    ) -> Result<(), String> {
        self.inner
            .send_cashout_failed(to_email, amount_eur_cents, lang)
            .await
    }
    async fn send_connect_account_verified(
        &self,
        to_email: &str,
        lang: Lang,
    ) -> Result<(), String> {
        self.inner
            .send_connect_account_verified(to_email, lang)
            .await
    }
    async fn send_connect_account_verification_failed(
        &self,
        to_email: &str,
        lang: Lang,
    ) -> Result<(), String> {
        self.inner
            .send_connect_account_verification_failed(to_email, lang)
            .await
    }
    async fn send_connect_account_onboarding_interrupted(
        &self,
        to_email: &str,
        lang: Lang,
    ) -> Result<(), String> {
        self.inner
            .send_connect_account_onboarding_interrupted(to_email, lang)
            .await
    }
    async fn send_deletion_confirmation(
        &self,
        to_email: &str,
        confirm_link: &str,
        lang: Lang,
    ) -> Result<(), String> {
        self.inner
            .send_deletion_confirmation(to_email, confirm_link, lang)
            .await
    }
}

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
    let mailer: Arc<dyn Mailer> = Arc::new(MockMailer);
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

#[tokio::test]
async fn dispatch_with_retry_succeeds_on_first_attempt() {
    let mailer = FailNMailer::new(0);
    let job = EmailJob::PasswordReset {
        to_email: "a@b.co".into(),
        reset_link: "r".into(),
        lang: Lang::En,
    };
    let retry_delays = &[Duration::ZERO, Duration::ZERO];
    assert!(dispatch_with_retry(&mailer, &job, retry_delays)
        .await
        .is_ok());
}

#[tokio::test]
async fn dispatch_with_retry_succeeds_after_retries() {
    let mailer = FailNMailer::new(2);
    let job = EmailJob::PasswordReset {
        to_email: "a@b.co".into(),
        reset_link: "r".into(),
        lang: Lang::En,
    };
    let retry_delays = &[Duration::ZERO, Duration::ZERO];
    assert!(dispatch_with_retry(&mailer, &job, retry_delays)
        .await
        .is_ok());
}

#[tokio::test]
async fn dispatch_with_retry_returns_err_after_exhausting_retries() {
    let mailer = FailNMailer::new(10);
    let job = EmailJob::PasswordReset {
        to_email: "a@b.co".into(),
        reset_link: "r".into(),
        lang: Lang::En,
    };
    let retry_delays = &[Duration::ZERO, Duration::ZERO];
    assert!(dispatch_with_retry(&mailer, &job, retry_delays)
        .await
        .is_err());
}
