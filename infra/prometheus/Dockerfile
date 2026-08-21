FROM prom/prometheus:v3.14.0

ARG IMAGE_SOURCE
ARG IMAGE_REVISION

COPY prometheus.yml /etc/prometheus/prometheus.yml

LABEL org.opencontainers.image.source="${IMAGE_SOURCE}" \
      org.opencontainers.image.revision="${IMAGE_REVISION}"
