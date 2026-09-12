FROM grafana/grafana:13.1.4-slim

ARG IMAGE_SOURCE
ARG IMAGE_REVISION

COPY datasources/ /etc/grafana/provisioning/datasources/
COPY dashboards/ /etc/grafana/provisioning/dashboards/
COPY alerting/ /etc/grafana/provisioning/alerting/

LABEL org.opencontainers.image.source="${IMAGE_SOURCE}" \
      org.opencontainers.image.revision="${IMAGE_REVISION}"
