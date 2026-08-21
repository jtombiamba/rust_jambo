FROM grafana/loki:3.7.6

ARG IMAGE_SOURCE
ARG IMAGE_REVISION

COPY loki-config.yml /etc/loki/loki-config.yml

LABEL org.opencontainers.image.source="${IMAGE_SOURCE}" \
      org.opencontainers.image.revision="${IMAGE_REVISION}"
