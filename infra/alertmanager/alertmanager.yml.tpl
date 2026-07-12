# Jambo Game — Alertmanager Configuration
#
# Routes alerts from Prometheus to notification channels.
# Three severity tiers map to different notification urgency.
#

global:
  resolve_timeout: ${RESOLVE_TIMEOUT}
  smtp_smarthost: '${SMTP_SMARTHOST}'
  smtp_from: '${SMTP_FROM}'
  smtp_require_tls: ${SMTP_REQUIRE_TLS}

route:
  group_by: ['alertname', 'severity', 'team']
  group_wait: 30s
  group_interval: 2m
  repeat_interval: 4h
  receiver: 'default'

  routes:
    - match:
        severity: critical
      receiver: 'critical'
      repeat_interval: 15m
      continue: true

    - match:
        severity: warning
      receiver: 'warning'
      repeat_interval: 1h

    - match:
        severity: info
      receiver: 'info'
      repeat_interval: 24h

receivers:
  - name: 'default'
    webhook_configs:
      - url: ${ALERTMANAGER_URL}/alert-status
        send_resolved: true

  - name: 'critical'
    webhook_configs:
      - url: 'http://alertmanager-webhook:8080/alert'
        send_resolved: true
    email_configs:
      - to: '${EMAIL_ONCALL}'
        send_resolved: true
        headers:
          subject: '[CRITICAL] {{ .GroupLabels.alertname }} — Jambo Game'
    slack_configs:
      - api_url: '${SLACK_CRITICAL_WEBHOOK_URL}'
        send_resolved: true
        channel: '${SLACK_CRITICAL_CHANNEL}'
        title: '[CRITICAL] {{ .GroupLabels.alertname }}'
        text: >-
          {{ range .Alerts }}
            *Alert:* {{ .Annotations.summary }}
            *Description:* {{ .Annotations.description }}
            *Severity:* {{ .Labels.severity }}
            *Instance:* {{ .Labels.instance }}
            *Time:* {{ .StartsAt.Format "2006-01-02 15:04:05 UTC" }}
          {{ end }}

  - name: 'warning'
    webhook_configs:
      - url: 'http://alertmanager-webhook:8080/alert'
        send_resolved: true
    email_configs:
      - to: '${EMAIL_TEAM}'
        send_resolved: true
        headers:
          subject: '[WARNING] {{ .GroupLabels.alertname }} — Jambo Game'
    slack_configs:
      - api_url: '${SLACK_WARNING_WEBHOOK_URL}'
        send_resolved: true
        channel: '${SLACK_WARNING_CHANNEL}'
        title: '[WARNING] {{ .GroupLabels.alertname }}'
        text: >-
          {{ range .Alerts }}
            *Alert:* {{ .Annotations.summary }}
            *Description:* {{ .Annotations.description }}
            *Severity:* {{ .Labels.severity }}
            *Instance:* {{ .Labels.instance }}
          {{ end }}

  - name: 'info'
    webhook_configs:
      - url: ${ALERTMANAGER_URL}/alert-status
        send_resolved: true

inhibit_rules:
  - source_match:
      severity: critical
    target_match:
      severity: warning
    equal: ['alertname', 'instance']

  - source_match:
      severity: critical
    target_match:
      severity: info
    equal: ['alertname', 'instance']
