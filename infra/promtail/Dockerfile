FROM grafana/promtail:3.6.11

ARG IMAGE_SOURCE
ARG IMAGE_REVISION

COPY promtail-config.yml /etc/promtail/promtail-config.yml

LABEL org.opencontainers.image.source="${IMAGE_SOURCE}" \
      org.opencontainers.image.revision="${IMAGE_REVISION}"
