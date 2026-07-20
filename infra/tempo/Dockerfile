# Stage 1: Create directory structure with correct ownership
FROM busybox:1.36 AS dirbuilder
RUN mkdir -p /var/tempo/generator && \
    chown -R 10001:10001 /var/tempo/generator && \
    chmod 0700 /var/tempo/generator

# Stage 2: Final image
FROM grafana/tempo:main-ea8fddf-1905-1
COPY --from=dirbuilder --chown=10001:10001 /var/tempo/generator /var/tempo/generator
COPY tempo-config.yml /etc/tempo/tempo-config.yml
EXPOSE 4318
