FROM nginx:1.28-alpine

ARG IMAGE_SOURCE
ARG IMAGE_REVISION

RUN apk add --no-cache apache2-utils
RUN chown -R nginx:nginx /etc/nginx /var/cache/nginx /var/log/nginx /tmp
COPY --chown=nginx:nginx nginx.conf /etc/nginx/nginx.conf
COPY --chown=nginx:nginx entrypoint.sh /entrypoint.sh
RUN chmod +x /entrypoint.sh

LABEL org.opencontainers.image.source="${IMAGE_SOURCE}" \
      org.opencontainers.image.revision="${IMAGE_REVISION}"

USER nginx
EXPOSE 8888
ENTRYPOINT ["/entrypoint.sh"]
CMD ["nginx", "-g", "daemon off;"]
