FROM edoburu/pgbouncer:latest

ARG IMAGE_SOURCE
ARG IMAGE_REVISION

COPY --chown=postgres:postgres pgbouncer.ini /etc/pgbouncer/pgbouncer.ini
COPY --chown=postgres:postgres entrypoint.sh /entrypoint.sh
RUN chmod +x /entrypoint.sh

LABEL org.opencontainers.image.source="${IMAGE_SOURCE}" \
      org.opencontainers.image.revision="${IMAGE_REVISION}"

EXPOSE 6432
ENTRYPOINT ["/entrypoint.sh"]
CMD ["/usr/bin/pgbouncer", "/etc/pgbouncer/pgbouncer.ini"]
