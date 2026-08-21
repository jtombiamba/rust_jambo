FROM alpine:3.21 AS builder
RUN apk add --no-cache gettext

FROM prom/alertmanager:v0.27.0

ARG IMAGE_SOURCE
ARG IMAGE_REVISION

COPY --from=builder /usr/bin/envsubst /usr/bin/envsubst
COPY --from=builder /usr/lib/libintl.so.8 /usr/lib/libintl.so.8
COPY --from=builder /lib/ld-musl-x86_64.so.1 /lib/ld-musl-x86_64.so.1

COPY alertmanager.yml.tpl /etc/alertmanager/alertmanager.yml.tpl
COPY entrypoint.sh /entrypoint.sh

LABEL org.opencontainers.image.source="${IMAGE_SOURCE}" \
      org.opencontainers.image.revision="${IMAGE_REVISION}"

EXPOSE 9093

ENTRYPOINT ["/entrypoint.sh"]
