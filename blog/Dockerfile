# Build stage
FROM python:3.12-alpine AS builder
WORKDIR /site
COPY requirements.txt ./
RUN pip install --no-cache-dir -r requirements.txt
COPY mkdocs.yml ./
COPY docs/ ./docs/
COPY overrides/ ./overrides/
RUN mkdocs build --strict

# Production stage
FROM nginx:1.28-alpine

# The nginx user (UID 101) needs write access to its cache/log dirs and to the
# pid path (see `pid /tmp/nginx.pid;` in nginx.conf).
RUN chown -R nginx:nginx /usr/share/nginx/html /var/cache/nginx /var/log/nginx /tmp

COPY --chown=nginx:nginx --from=builder /site/site /usr/share/nginx/html
COPY --chown=nginx:nginx nginx.conf /etc/nginx/nginx.conf

EXPOSE 8088
USER nginx
CMD ["nginx", "-g", "daemon off;"]
