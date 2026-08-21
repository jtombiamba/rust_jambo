# Build stage
FROM node:22-alpine AS builder
WORKDIR /app
COPY package.json package-lock.json ./
RUN npm ci
COPY . .

RUN npm run build

# Production stage
FROM nginx:1.28-alpine

ARG IMAGE_SOURCE
ARG IMAGE_REVISION

# The nginx user (UID 101) needs write access to its cache/log dirs and to the
# pid path (see `pid /tmp/nginx.pid;` in nginx.conf).
RUN chown -R nginx:nginx /usr/share/nginx/html /var/cache/nginx /var/log/nginx /tmp

COPY --chown=nginx:nginx --from=builder /app/dist /usr/share/nginx/html
COPY --chown=nginx:nginx nginx.conf /etc/nginx/nginx.conf
# Copy runtime env config entrypoint script
COPY --chown=nginx:nginx env-config.sh /docker-entrypoint.d/40-env-config.sh
RUN chmod +x /docker-entrypoint.d/40-env-config.sh

LABEL org.opencontainers.image.source="${IMAGE_SOURCE}" \
      org.opencontainers.image.revision="${IMAGE_REVISION}"

EXPOSE 80
USER nginx
CMD ["nginx", "-g", "daemon off;"]
